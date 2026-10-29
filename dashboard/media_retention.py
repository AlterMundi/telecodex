"""Opt-in producer handoff of scoped original audio, never an archive assembler.

Reserve before capture admission/synthesis; persist streamed bytes before emitting
frames. Completed and interrupted originals stay immutable until the consumer's
explicit retirement. No TTL, polling, provider call or automatic eviction.
"""
import hashlib
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import secrets
import stat
import uuid
from functools import wraps

from .private_state import private_directory, read_json
from scripts.speech_render import atomic, canonical, lock, private_bytes, sync_directory, RenderError

MAX_BYTES = 256 * 1024 * 1024
MAX_ENTRIES = 4096


class RetentionUnavailable(Exception):
    pass


def storage_error(method):
    @wraps(method)
    def wrapped(*args, **kwargs):
        try:
            return method(*args, **kwargs)
        except (RenderError, OSError, ValueError) as exc:
            raise RetentionUnavailable('Original audio protection unavailable; protected originals remain saved') from exc
    return wrapped


class MediaRetention:
    def __init__(self, config):
        if (set(config) != {'directory', 'human_id', 'chat_id', 'topic_id', 'native_thread', 'max_bytes', 'since'}
                or any(type(config[k]) is not int or config[k] <= 0 for k in ('human_id', 'chat_id', 'topic_id', 'max_bytes'))
                or config['max_bytes'] > MAX_BYTES
                or str(uuid.UUID(config['native_thread'])) != config['native_thread']
                or not Path(config['directory']).is_absolute()):
            raise ValueError('Explicit bounded original-audio scope required')
        self.since = datetime.fromisoformat(config['since'])
        if self.since.tzinfo is None:
            raise ValueError('Original-audio scope requires an explicit timezone')
        self.config = dict(config)
        self.root = private_directory(config['directory'])
        sync_directory(self.root.parent)

    def selected(self, source):
        binding = source.get('entry_binding')
        c = self.config
        committed = (source.get('voice') or {}).get('transcription', {}).get('committed_at')
        if committed:
            try:
                if datetime.fromisoformat(committed) < self.since:
                    return False
            except (ValueError, TypeError):
                return False
        elif datetime.now(timezone.utc) < self.since:
            return False
        return (isinstance(binding, list) and len(binding) == 4
                and binding[0] == c['human_id']
                and binding[1] == {'chat_id': c['chat_id'], 'thread_id': c['topic_id']}
                and binding[3] == c['native_thread'])

    @storage_error
    def begin(self, source, speaker, mime, bound):
        if not self.selected(source):
            return None
        if speaker not in ('human', 'assistant') or type(bound) is not int or not 0 < bound <= 20*1024*1024:
            raise ValueError('Invalid original-audio reservation')
        # Only server-certified provenance, never a caller-supplied filesystem path.
        provenance = {k: source[k] for k in ('entry_binding', 'id', 'revision', 'offset', 'next_offset', 'kind') if k in source}
        return Recording(self, provenance, speaker, mime, bound)

    @storage_error
    def retain(self, source, speaker, mime, data):
        if not self.selected(source):
            return None
        # A matching complete handoff needs no new capacity, even at its cap.
        provenance = {k: source[k] for k in ('entry_binding', 'id', 'revision', 'offset', 'next_offset', 'kind') if k in source}
        digest = hashlib.sha256(data).hexdigest()
        info = {'schema':'telecodex.original-audio/v1', 'speaker':speaker,
                'mime':mime, 'source':provenance, 'reconstruction':False,
                'playback':'not_observed', 'protection_since':self.config['since'], 'complete':True,
                'audio_bytes':len(data), 'audio_sha256':digest}
        record = hashlib.sha256(canonical(info)).hexdigest()
        with lock(self.root / '.admission.lock'):
            if (self.root / (record + '.json')).exists():
                if (read_json(self.root / (record + '.json')) != info
                        or private_bytes(self.root / digest, len(data)) != data):
                    raise RetentionUnavailable('Original audio handoff changed')
                return {**info, 'receipt':record}
        original = self.begin(source, speaker, mime, len(data))
        original.append(data)
        return original.finish(complete=True)

    def _usage(self):
        total = 0
        entries = list(self.root.iterdir())
        if len(entries) > MAX_ENTRIES:
            raise RetentionUnavailable('Original audio storage reached its entry limit; originals preserved')
        for p in entries:
            m = p.lstat()
            if not stat.S_ISREG(m.st_mode) or m.st_uid != os.getuid() or m.st_mode & 0o077:
                raise RetentionUnavailable('Original audio storage changed; originals preserved')
            # In-progress bytes already belong to their reserved budget.
            if p.name.endswith('.part'):
                if not p.with_suffix('.reservation').exists():
                    total += m.st_size
            elif p.name.endswith('.reservation'):
                total += read_json(p)['reserved_bytes'] + m.st_size
            else:
                total += m.st_size
        return total, len(entries)


class Recording:
    def __init__(self, retention, provenance, speaker, mime, bound):
        self.retention = retention
        self.bound = bound
        self.used = 0
        self.closed = False
        self.info = {'schema': 'telecodex.original-audio/v1', 'speaker': speaker,
                     'mime': mime, 'source': provenance, 'reconstruction': False,
                     'playback': 'not_observed', 'protection_since':retention.config['since']}
        root = retention.root
        self.name = 'capture-' + secrets.token_hex(16)
        self.reservation = root / (self.name + '.reservation')
        self.path = root / (self.name + '.part')
        with lock(root / '.admission.lock'):
            used, count = retention._usage()
            # Room for reservation, media and <=16KiB terminal provenance. Unknown
            # interrupted reservations keep their budget until explicit inspection.
            if count + 3 > MAX_ENTRIES or used + bound + 32768 > retention.config['max_bytes']:
                raise RetentionUnavailable('Original audio storage full; existing originals preserved. Disable this dialogue retention or transfer and explicitly retire its media before new audio.')
            atomic(self.reservation, canonical({**self.info, 'reserved_bytes': bound + 16384}))
            fd = os.open(self.path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
            self.stream = os.fdopen(fd, 'wb')
            sync_directory(root)

    @storage_error
    def append(self, data):
        if self.closed or not isinstance(data, bytes) or self.used + len(data) > self.bound:
            raise RetentionUnavailable('Original audio exceeds its reservation; protected bytes retained')
        self.stream.write(data)
        self.stream.flush()
        os.fsync(self.stream.fileno())
        self.used += len(data)

    @storage_error
    def finish(self, complete=False):
        if self.closed:
            return self.receipt
        self.stream.close()
        root = self.retention.root
        with lock(root / '.admission.lock'):
            data = private_bytes(self.path, self.bound)
            digest = hashlib.sha256(data).hexdigest()
            info = {**self.info, 'complete': complete, 'audio_bytes': len(data), 'audio_sha256': digest}
            record = hashlib.sha256(canonical(info)).hexdigest()
            destination = root / digest
            if destination.exists() or destination.is_symlink():
                if private_bytes(destination, self.bound) != data:
                    raise RetentionUnavailable('Original audio content address changed')
            else:
                os.link(self.path, destination, follow_symlinks=False)
                sync_directory(root)
            atomic(root / (record + '.json'), canonical(info))
            # Receipt and bytes are durable before removing pending protection.
            self.path.unlink()
            self.reservation.unlink()
            sync_directory(root)
            self.receipt = {**info, 'receipt': record}
            self.closed = True
            return self.receipt
