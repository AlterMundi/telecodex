"""Bounded, transcription-only audio preparation; never conversation admission.

Qwen's manual audio session emits input transcripts without response.create.
This adapter deliberately has no dialogue, tool, native-input or retry API.
Its partials remain revisable preparation until an owning ingress commits them.
"""
import base64
import hashlib
import json
import threading
import time
from urllib.parse import parse_qsl, urlsplit

from scripts.speech_render import private_bytes


class TranscriptionUnavailable(Exception):
    """A failed stream is not permission to replay its provider operation."""


class TranscriptionStream:
    sample_rate = 16000
    max_audio_bytes = 120 * sample_rate * 2
    max_event_bytes = 65536
    max_text_bytes = 16000

    def __init__(self, endpoint, credential_file, connect=None):
        url = urlsplit(endpoint)
        model = dict(parse_qsl(url.query, strict_parsing=True)).get('model')
        if (url.scheme != 'wss' or not url.hostname or url.username or url.password
                or url.fragment or url.port not in (None, 443)
                or url.path != '/api-ws/v1/realtime'
                or list(parse_qsl(url.query)) != [('model', 'qwen-audio-3.0-realtime-plus')]):
            raise ValueError('Select an exact supported transcription endpoint')
        token = private_bytes(credential_file, 16384).decode().strip()
        if not token or any(c.isspace() for c in token):
            raise ValueError('Transcription credential unavailable')
        if connect is None:
            import websocket
            connect = websocket.create_connection
        self.model = model
        self.phase = 'opening'
        self.audio_bytes = 0
        self.audio_sha256 = hashlib.sha256()
        self.started_at = time.monotonic()
        self.first_partial_at = None
        self.finalized_at = None
        self.text = ''
        self.item_id = None
        self.revision = 0
        self._lock = threading.RLock()
        self._reader = threading.Lock()
        self._socket = None
        try:
            # No redirects, auto-reconnect, provider trace or secret in arguments.
            self._socket = connect(endpoint, header=['Authorization: Bearer ' + token],
                                   timeout=10, redirect_limit=0, enable_multithread=True)
            self._send({'type': 'session.update', 'session': {
                'modalities': ['text'], 'input_audio_format': 'pcm',
                'turn_detection': None}})
        except Exception:
            self._fail('transcription-open-unconfirmed')

    def _send(self, value):
        try:
            self._socket.send(json.dumps(value))
        except Exception:
            self._fail('transcription-send-unconfirmed')

    def _fail(self, reason):
        self.phase = 'failed'
        if self._socket is not None:
            self._socket.close()
        raise TranscriptionUnavailable(reason) from None

    def append(self, pcm):
        """Append mono16kHz PCM16; input order belongs to one stream/socket."""
        with self._lock:
            if (self.phase != 'ready' or not isinstance(pcm, bytes) or not pcm
                    or len(pcm) % 2 or len(pcm) > 32000
                    or self.audio_bytes + len(pcm) > self.max_audio_bytes):
                raise ValueError('Provide bounded PCM only after stream readiness')
            self._send({'type': 'input_audio_buffer.append',
                        'audio': base64.b64encode(pcm).decode('ascii')})
            self.audio_sha256.update(pcm)
            self.audio_bytes += len(pcm)

    def finish(self):
        """Finalize recognition only. This does not admit a human instruction."""
        with self._lock:
            if self.phase != 'ready' or not self.audio_bytes:
                raise ValueError('Finish one nonempty ready stream')
            self.phase = 'finalizing'
            self._send({'type': 'input_audio_buffer.commit'})

    def receive(self):
        """Read one provider event; partial text replaces the previous revision."""
        # Serialize dequeue and application together, while audio writes retain
        # their separate lock and can continue during a blocking provider read.
        with self._reader:
            return self._receive()

    def _receive(self):
        if self.phase in ('closed', 'failed', 'completed'):
            raise ValueError('Transcription stream is terminal')
        try:
            raw = self._socket.recv()
            if not isinstance(raw, str) or len(raw.encode()) > self.max_event_bytes:
                self._fail('invalid-transcription-event')
            event = json.loads(raw)
            if not isinstance(event, dict):
                self._fail('invalid-transcription-event')
        except TranscriptionUnavailable:
            raise
        except Exception:
            self._fail('transcription-receive-unconfirmed')
        with self._lock:
            kind = event.get('type')
            if not isinstance(kind, str) or kind == 'error' or kind.startswith('response.'):
                # No unexpected generated answer is allowed onto the voice path.
                self._fail('unexpected-transcription-provider-event')
            if kind == 'session.updated':
                session = event.get('session', {})
                if (self.phase != 'opening' or not isinstance(session, dict)
                        or session.get('turn_detection') is not None
                        or session.get('model', self.model) != self.model
                        or session.get('modalities', ['text']) != ['text']):
                    self._fail('transcription-manual-mode-unconfirmed')
                # Qwen acknowledges the update but omits turn_detection from
                # its session echo. The explicit null request and updated ACK
                # establish manual mode; reject a contradictory echo/output.
                self.phase = 'ready'
                return {'type': 'ready'}
            prefix = 'conversation.item.input_audio_transcription.'
            if kind.startswith(prefix):
                if self.phase not in ('ready', 'finalizing') or not self.audio_bytes:
                    self._fail('transcription-result-before-audio')
                item = event.get('item_id')
                if not isinstance(item, str) or not item or len(item) > 256:
                    self._fail('transcription-item-unconfirmed')
                if self.item_id is not None and self.item_id != item:
                    self._fail('transcription-item-changed')
                self.item_id = item
                final = kind == prefix + 'completed'
                if final:
                    if self.phase != 'finalizing':
                        self._fail('transcription-final-before-finish')
                    text = event.get('transcript')
                elif kind in (prefix + 'delta', prefix + 'text'):
                    text, stash = event.get('text', ''), event.get('stash', '')
                    if not isinstance(text, str) or not isinstance(stash, str):
                        self._fail('invalid-transcription-text')
                    text += stash
                else:
                    self._fail('transcription-failed-or-unknown')
                if not isinstance(text, str) or len(text.encode()) > self.max_text_bytes:
                    self._fail('invalid-transcription-text')
                self.text = text
                self.revision += 1
                now = time.monotonic()
                if self.first_partial_at is None and text:
                    self.first_partial_at = now
                if final:
                    if not text.strip():
                        self._fail('empty-final-transcription')
                    self.phase = 'completed'
                    self.finalized_at = now
                return {'type': 'final' if final else 'partial', 'text': text,
                        'revision': self.revision}
            return {'type': 'status'}

    def result(self):
        if self.phase != 'completed':
            raise ValueError('No completed transcription')
        return {'text': self.text, 'final': True, 'model': self.model,
                'pcm_sha256': self.audio_sha256.hexdigest(),
                'audio_millis': self.audio_bytes * 1000 // (2 * self.sample_rate)}

    def close(self):
        with self._lock:
            if self._socket is not None:
                self._socket.close()
            if self.phase not in ('completed', 'failed'):
                self.phase = 'closed'
