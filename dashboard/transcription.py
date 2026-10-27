"""Bounded, transcription-only audio preparation; never conversation admission.

Qwen and Gemini use manual audio activities to emit input transcripts only.
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

    def __init__(self, endpoint, credential_file, connect=None, provider='qwen'):
        url = urlsplit(endpoint)
        if provider not in ('qwen', 'gemini'):
            raise ValueError('Select a supported transcription provider')
        model = 'gemini-3.5-transcribe-live' if provider == 'gemini' else dict(parse_qsl(url.query, strict_parsing=True)).get('model')
        if provider == 'gemini':
            valid = (endpoint == 'wss://generativelanguage.googleapis.com/ws/google.ai.generativelanguage.v1beta.GenerativeService.BidiGenerateContent')
        else:
            valid = not (url.scheme != 'wss' or not url.hostname or url.username or url.password
                or url.fragment or url.port not in (None, 443)
                or url.path != '/api-ws/v1/realtime'
                or list(parse_qsl(url.query)) != [('model', 'qwen-audio-3.0-realtime-plus')])
        if not valid:
            raise ValueError('Select an exact supported transcription endpoint')
        token = private_bytes(credential_file, 16384).decode().strip()
        if not token or any(c.isspace() for c in token):
            raise ValueError('Transcription credential unavailable')
        self._timeout_errors = (TimeoutError,)
        if connect is None:
            import websocket
            connect = websocket.create_connection
            self._timeout_errors += (websocket.WebSocketTimeoutException,)
        self.model = model
        self.provider = provider
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
            credential = 'x-goog-api-key: ' if provider == 'gemini' else 'Authorization: Bearer '
            self._socket = connect(endpoint, header=[credential + token],
                                   timeout=10, redirect_limit=0, enable_multithread=True)
            if hasattr(self._socket,'settimeout'):
                self._socket.settimeout(1)
            if provider == 'gemini':
                self._send({'setup': {'model': 'models/' + model,
                    'generationConfig': {'responseModalities': ['TEXT']},
                    'inputAudioTranscription': {'mode': 'VERBATIM', 'languageCodes': []},
                    'realtimeInputConfig': {'automaticActivityDetection': {'disabled': True}}}})
            else:
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
            encoded = base64.b64encode(pcm).decode('ascii')
            self._send({'realtimeInput': {'audio': {'data': encoded, 'mimeType': 'audio/pcm;rate=16000'}}}
                       if self.provider == 'gemini' else {'type': 'input_audio_buffer.append', 'audio': encoded})
            self.audio_sha256.update(pcm)
            self.audio_bytes += len(pcm)

    def finish(self):
        """Finalize recognition only. This does not admit a human instruction."""
        with self._lock:
            if self.phase != 'ready' or not self.audio_bytes:
                raise ValueError('Finish one nonempty ready stream')
            self.phase = 'finalizing'
            self._send({'realtimeInput': {'activityEnd': {}}} if self.provider == 'gemini'
                       else {'type': 'input_audio_buffer.commit'})

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
            if self.provider == 'gemini' and isinstance(raw, bytes):
                if len(raw) > self.max_event_bytes:
                    self._fail('invalid-transcription-event')
                raw = raw.decode('utf-8')
            if not isinstance(raw, str) or len(raw.encode()) > self.max_event_bytes:
                self._fail('invalid-transcription-event')
            event = json.loads(raw)
            if not isinstance(event, dict):
                self._fail('invalid-transcription-event')
        except self._timeout_errors:
            # Silence is not a disconnection or permission to open a new stream.
            # The owning capture's deadline/auth checks bound the whole session.
            return {'type':'status'}
        except TranscriptionUnavailable:
            raise
        except Exception:
            self._fail('transcription-receive-unconfirmed')
        with self._lock:
            if self.provider == 'gemini':
                return self._gemini_event(event)
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

    def _gemini_event(self, event):
        # This is the dedicated transcription model, never a live agent. Keep
        # one manual activity per capture; generated text/audio/tools are errors.
        if (not event or set(event) - {'setupComplete', 'serverContent', 'usageMetadata', 'voiceActivity'}
                or len(set(event) - {'usageMetadata', 'voiceActivity'}) > 1):
            self._fail('unexpected-transcription-provider-event')
        if 'setupComplete' in event:
            if self.phase != 'opening' or not isinstance(event['setupComplete'], dict):
                self._fail('transcription-manual-mode-unconfirmed')
            self._send({'realtimeInput': {'activityStart': {}}})
            self.phase = 'ready'
            return {'type': 'ready'}
        content = event.get('serverContent', {})
        if (not isinstance(content, dict) or any(k in content for k in ('modelTurn', 'outputTranscription'))
                or content.get('interrupted')):
            self._fail('unexpected-transcription-provider-event')
        final = 'inputTranscription' in content
        if not final and 'interimInputTranscription' not in content:
            return {'type': 'status'}
        if (self.phase not in ('ready', 'finalizing') or not self.audio_bytes
                or final and self.phase != 'finalizing'):
            self._fail('transcription-final-before-finish' if final else 'transcription-result-before-audio')
        value = content['inputTranscription' if final else 'interimInputTranscription']
        text = value.get('text') if isinstance(value, dict) else None
        if (not isinstance(text, str) or len(text.encode()) > self.max_text_bytes
                or final and not text.strip()):
            self._fail('invalid-transcription-text')
        self.text = text
        self.revision += 1
        now = time.monotonic()
        if self.first_partial_at is None and text:
            self.first_partial_at = now
        if final:
            self.phase = 'completed'
            self.finalized_at = now
        return {'type': 'final' if final else 'partial', 'text': text, 'revision': self.revision}

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
