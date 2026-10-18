#!/usr/bin/env python3
"""Bounded file synthesis with durable request identity; never a playback receipt.

Owner configuration selects the provider and private cache. One JSON object on
stdin supplies text and an opaque receiving scope. This utility establishes no
human, source, session or audience authority: its caller must enforce those.
The HTTP timeout bounds socket operations. The caller must also enforce an
overall process-group deadline and bounded stdout, including on cancellation.
"""
import argparse
from contextlib import contextmanager
import fcntl
import hashlib
import io
import json
import os
from pathlib import Path
import re
import stat
import sys
import tempfile
import time
from urllib.error import HTTPError
from urllib.parse import urlsplit
from urllib.request import Request
import uuid
import wave

SCHEMA = 'telecodex.speech-route/v1'
ENDPOINT = 'https://877104f7-e885-42b9-8de8-f6e4c6303969.invocation.api.nvcf.nvidia.com/v1/audio/synthesize'
MAX_INPUT = 32768
MAX_AUDIO = 8 * 1024 * 1024


class RenderError(Exception):
    """Controlled error codes only; provider payloads and URLs never escape."""


def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(',', ':'), ensure_ascii=False).encode()


def sha(data):
    return hashlib.sha256(data).hexdigest()


def private_bytes(path, limit):
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    with os.fdopen(fd, 'rb') as source:
        info = os.fstat(source.fileno())
        if (not stat.S_ISREG(info.st_mode) or info.st_uid != os.getuid()
                or info.st_mode & 0o077 or info.st_size > limit):
            raise RenderError('unsafe-private-file')
        data = source.read(limit + 1)
        if len(data) > limit:
            raise RenderError('private-file-bound')
        return data


def directory(path):
    path = Path(path)
    if not path.parent.exists():
        directory(path.parent)
    path.mkdir(mode=0o700, exist_ok=True)
    info = path.lstat()
    if (not stat.S_ISDIR(info.st_mode) or info.st_uid != os.getuid()
            or info.st_mode & 0o077):
        raise RenderError('unsafe-cache-directory')
    # Sync the name in its containing directory, not only later state contents.
    # Newly created ancestors are established in the same order before any POST.
    sync_directory(path.parent)
    return path


def sync_directory(path):
    fd = os.open(path, os.O_RDONLY | os.O_DIRECTORY)
    try:
        os.fsync(fd)
    finally:
        os.close(fd)


def atomic(path, data):
    with tempfile.NamedTemporaryFile(dir=path.parent, delete=False) as temporary:
        name = Path(temporary.name)
        try:
            temporary.write(data)
            temporary.flush()
            os.fsync(temporary.fileno())
            os.replace(name, path)
        finally:
            name.unlink(missing_ok=True)
    # The retained intent must survive before the external request begins.
    sync_directory(path.parent)


@contextmanager
def lock(path):
    fd = os.open(path, os.O_CREAT | os.O_RDWR | os.O_NOFOLLOW, 0o600)
    with os.fdopen(fd, 'rb') as handle:
        info = os.fstat(handle.fileno())
        if (not stat.S_ISREG(info.st_mode) or info.st_uid != os.getuid()
                or info.st_mode & 0o077):
            raise RenderError('unsafe-render-lock')
        try:
            fcntl.flock(handle, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            raise RenderError('render-in-progress') from None
        yield


def route(path):
    value = json.loads(private_bytes(path, 16384))
    allowed = {'schema', 'provider', 'cache_root', 'env_file', 'voice', 'language',
               'endpoint', 'timeout_seconds', 'audio_retention_seconds', 'audio_cache_max_bytes'}
    if (not isinstance(value, dict) or set(value) - allowed or value.get('schema') != SCHEMA
            or value.get('provider') != 'nvidia-magpie'
            or not isinstance(value.get('cache_root'), str)
            or not Path(value['cache_root']).is_absolute()
            or not isinstance(value.get('env_file'), str)
            or not Path(value['env_file']).is_absolute()
            or not isinstance(value.get('voice'), str)
            or not re.fullmatch(r'Magpie-Multilingual\.[A-Za-z0-9.-]{1,96}', value['voice'])
            or not isinstance(value.get('language'), str)
            or not re.fullmatch(r'[a-z]{2}-[A-Z]{2}', value['language'])):
        raise RenderError('invalid-speech-route')
    endpoint = value.get('endpoint', ENDPOINT)
    if not isinstance(endpoint, str):
        raise RenderError('invalid-synthesis-endpoint')
    parsed = urlsplit(endpoint)
    if (parsed.username or parsed.password or parsed.query or parsed.fragment
            or not parsed.hostname or parsed.scheme not in ('http', 'https')
            or (parsed.scheme == 'http' and parsed.hostname not in ('127.0.0.1', '::1', 'localhost'))):
        raise RenderError('invalid-synthesis-endpoint')
    timeout = value.get('timeout_seconds', 45)
    if type(timeout) is not int or not 1 <= timeout <= 120:
        raise RenderError('invalid-synthesis-deadline')
    if (type(value.get('audio_retention_seconds', 3600)) is not int
            or not 300 <= value.get('audio_retention_seconds', 3600) <= 7 * 86400
            or type(value.get('audio_cache_max_bytes', 256 * 1024 * 1024)) is not int
            or not MAX_AUDIO <= value.get('audio_cache_max_bytes', 256 * 1024 * 1024) <= 1024 * 1024 * 1024):
        raise RenderError('invalid-audio-retention')
    return {**value, 'endpoint': endpoint, 'timeout_seconds': timeout}


def prune_audio(config, *, exclude=None, now=None):
    """Expire only verified completed WAVs; preserve small outcome receipts."""
    try:
        return _prune_audio(config, exclude=exclude, now=now)
    except RenderError as error:
        if str(error) != 'render-in-progress':
            raise
        return {'removed_audio_bytes': 0, 'maintenance_busy': True, 'provider_calls': 0}


def _prune_audio(config, *, exclude=None, now=None):
    now = time.time() if now is None else now
    root = directory(config['cache_root'])
    age = config.get('audio_retention_seconds', 3600)
    limit = config.get('audio_cache_max_bytes', 256 * 1024 * 1024)
    removed = 0
    with lock(root / '.retention.lock'):
        candidates = []
        for job in root.iterdir():
            if not re.fullmatch(r'[a-f0-9]{64}', job.name) or job.name == exclude:
                continue
            try:
                info = job.lstat()
                if not stat.S_ISDIR(info.st_mode) or info.st_uid != os.getuid() or info.st_mode & 0o077:
                    continue
                state = json.loads(private_bytes(job / 'state.json', 32768))
                audio = (job / 'audio.wav').lstat()
                used = state.get('last_used_at', state.get('created_at'))
                if (state.get('schema') != 'telecodex.speech-render/v1'
                        or state.get('identity_sha256') != job.name
                        or state.get('phase') not in ('rendered', 'audio_expired')
                        or not isinstance(used, (int, float)) or isinstance(used, bool)
                        or not stat.S_ISREG(audio.st_mode) or audio.st_uid != os.getuid()
                        or audio.st_mode & 0o077 or audio.st_size != state.get('audio_bytes')):
                    continue
                candidates.append((used, job, audio.st_size))
            except (OSError, ValueError, KeyError, TypeError, RenderError):
                continue
        remaining = audio_occupancy(root)
        for used, job, size in sorted(candidates):
            if now - used < 300 or (now - used < age and remaining <= limit):
                continue  # A bounded grace protects a renderer's immediate reader.
            try:
                with lock(job / '.lock'):
                    state = json.loads(private_bytes(job / 'state.json', 32768))
                    latest = state.get('last_used_at', state.get('created_at'))
                    if latest != used or state.get('phase') not in ('rendered', 'audio_expired'):
                        continue
                    audio = job / 'audio.wav'
                    data = private_bytes(audio, MAX_AUDIO)
                    if sha(data) != state['audio_sha256']:
                        continue  # Changed/uncertain media is never disposable cache.
                    state.update(phase='audio_expired', audio_expired_at=now)
                    atomic(job / 'state.json', canonical(state))
                    audio.unlink()
                    sync_directory(job)
                    remaining -= size
                    removed += size
            except (OSError, ValueError, KeyError, TypeError, RenderError):
                continue
    return {'removed_audio_bytes': removed, 'accounted_audio_bytes': remaining,
            'receipts_preserved': True, 'provider_calls': 0}


def audio_occupancy(root):
    used = 0
    for job in root.iterdir():
        if not re.fullmatch(r'[a-f0-9]{64}', job.name) or job.is_symlink() or not job.is_dir():
            continue
        audio = job / 'audio.wav'
        if audio.exists() or audio.is_symlink():
            info = audio.lstat()
            if not stat.S_ISREG(info.st_mode) or info.st_uid != os.getuid() or info.st_mode & 0o077:
                raise RenderError('unsafe-audio-budget-artifact')
            used += info.st_size
        else:
            try:
                prior = json.loads(private_bytes(job / 'state.json', 32768))
                if prior.get('phase') == 'intent':
                    used += MAX_AUDIO
            except FileNotFoundError:
                pass
    return used


def reserve_audio(config, state_file, state):
    """Bound payloads plus in-flight maximum sizes before a provider request."""
    root = Path(config['cache_root'])
    with lock(root / '.audio-budget.lock'):
        used = audio_occupancy(root)
        if used + MAX_AUDIO > config.get('audio_cache_max_bytes', 256 * 1024 * 1024):
            raise RenderError('audio-cache-capacity-text-remains-available')
        atomic(state_file, canonical(state))


def credential(path):
    # Parse only the selected variable; dotenv is data, never shell code.
    for line in private_bytes(path, 1024 * 1024).decode().splitlines():
        match = re.fullmatch(r'\s*(?:export\s+)?NVIDIA_API_KEY\s*=\s*(.*?)\s*', line)
        if match:
            value = match[1]
            if value[:1] in ('"', "'") and value[-1:] == value[:1]:
                value = value[1:-1]
            else:
                value = value.split(' #', 1)[0].strip()
            if value and '\r' not in value and '\n' not in value:
                return value
    raise RenderError('synthesis-credential-unavailable')


def audio_info(data):
    if not 44 <= len(data) <= MAX_AUDIO:
        raise RenderError('invalid-synthesized-audio')
    try:
        with wave.open(io.BytesIO(data), 'rb') as source:
            frames = source.getnframes()
            if (source.getnchannels(), source.getsampwidth(), source.getframerate(), source.getcomptype()) != (1, 2, 22050, 'NONE'):
                raise RenderError('unexpected-synthesized-format')
            if not 0 < frames <= 120 * 22050 or len(source.readframes(frames + 1)) != frames * 2:
                raise RenderError('incomplete-synthesized-audio')
            return {'sample_rate_hz': 22050, 'channels': 1, 'audio_millis': frames * 1000 // 22050}
    except (wave.Error, EOFError):
        raise RenderError('invalid-synthesized-audio') from None


def multipart(fields):
    boundary = 'speech-' + uuid.uuid4().hex
    body = b''
    for name, text in fields.items():
        body += (f'--{boundary}\r\nContent-Disposition: form-data; name="{name}"\r\n\r\n'.encode()
                 + text.encode() + b'\r\n')
    return boundary, body + f'--{boundary}--\r\n'.encode()


def render(binding, request):
    if (not isinstance(request, dict) or set(request) != {'scope', 'text'}
            or not isinstance(request['scope'], str) or not 1 <= len(request['scope']) <= 128
            or not isinstance(request['text'], str) or not request['text'].strip()
            or len(request['text']) > 1800 or '\0' in request['text']):
        raise RenderError('invalid-speech-request')
    config = route(binding)
    fields = {'text': request['text'], 'language': config['language'], 'voice': config['voice'],
              'encoding': 'LINEAR_PCM', 'sample_rate_hz': '22050'}
    identity = {'schema': 'telecodex.speech-render/v1', 'scope': request['scope'],
                'provider': config['provider'], 'endpoint': config['endpoint'], 'fields': fields}
    ident = sha(canonical(identity))
    job = directory(directory(config['cache_root']) / ident)
    prune_audio(config, exclude=ident)
    state_file, audio_file = job / 'state.json', job / 'audio.wav'
    with lock(job / '.lock'):
        generation = 1
        if state_file.exists() or state_file.is_symlink():
            state = json.loads(private_bytes(state_file, 32768))
            if (not isinstance(state, dict) or state.get('schema') != 'telecodex.speech-render/v1'
                    or state.get('identity_sha256') != ident or state.get('phase') not in ('rendered', 'audio_expired')
                    or state.get('text_sha256') != sha(request['text'].encode())
                    or state.get('voice') != config['voice'] or state.get('language') != config['language']):
                raise RenderError('retained-render-outcome-no-automatic-retry')
            if state['phase'] == 'rendered':
                data = private_bytes(audio_file, MAX_AUDIO)
                info = audio_info(data)
                if sha(data) != state['audio_sha256'] or info != state['format']:
                    raise RenderError('retained-render-integrity-failed')
                state['last_used_at'] = time.time()
                atomic(state_file, canonical(state))
                return result(state, audio_file, True)
            # This is a known completed render whose disposable payload expired,
            # not an uncertain provider request. Only this new render call may
            # produce audio again; pruning itself never invokes a provider.
            if audio_file.exists():
                if sha(private_bytes(audio_file, MAX_AUDIO)) != state['audio_sha256']:
                    raise RenderError('expired-render-integrity-failed')
                audio_file.unlink()
                sync_directory(job)
            generation = state.get('generation', 1) + 1
        if audio_file.exists() or audio_file.is_symlink():
            raise RenderError('orphan-render-no-automatic-retry')
        prune_audio({**config, 'audio_cache_max_bytes':
                     config.get('audio_cache_max_bytes', 256 * 1024 * 1024) - MAX_AUDIO}, exclude=ident)
        token = credential(config['env_file'])
        state = {'schema': 'telecodex.speech-render/v1', 'identity_sha256': ident,
                 'phase': 'intent', 'provider': config['provider'], 'voice': config['voice'],
                 'language': config['language'], 'text_sha256': sha(request['text'].encode()),
                 'created_at': time.time(), 'last_used_at': time.time(), 'generation': generation}
        reserve_audio(config, state_file, state)
        boundary, body = multipart(fields)
        # No retry or redirect may resend a credential/request to another endpoint.
        http = Request(config['endpoint'], data=body, headers={
            'Authorization': 'Bearer ' + token, 'Content-Type': 'multipart/form-data; boundary=' + boundary})
        started = time.monotonic()
        try:
            # A redirect handler is disabled below; ordinary urlopen follows 307/308
            # differently across versions and must not define credential authority.
            data = fetch(http, config['timeout_seconds'])
            info = audio_info(data)
            atomic(audio_file, data)
            state.update(phase='rendered', audio_sha256=sha(data), audio_bytes=len(data),
                         format=info, request_millis=int((time.monotonic() - started) * 1000))
        except HTTPError as error:
            state.update(phase='rejected', http_status=error.code)
            error.close()
            atomic(state_file, canonical(state))
            raise RenderError('synthesis-rejected-no-automatic-retry') from None
        except Exception:
            state['phase'] = 'unconfirmed'
            atomic(state_file, canonical(state))
            raise RenderError('synthesis-unconfirmed-no-automatic-retry') from None
        atomic(state_file, canonical(state))
        return result(state, audio_file, False)


def fetch(request, timeout):
    from urllib.request import HTTPRedirectHandler, build_opener

    class NoRedirect(HTTPRedirectHandler):
        def redirect_request(self, req, fp, code, msg, headers, newurl):
            return None

    with build_opener(NoRedirect).open(request, timeout=timeout) as response:
        if response.headers.get_content_type() not in ('audio/wav', 'audio/x-wav', 'application/octet-stream'):
            raise RenderError('unexpected-synthesis-content-type')
        length = response.headers.get('Content-Length')
        if length is not None and (not length.isdecimal() or int(length) > MAX_AUDIO):
            raise RenderError('synthesized-audio-bound')
        data = response.read(MAX_AUDIO + 1)
        if len(data) > MAX_AUDIO or length is not None and len(data) != int(length):
            raise RenderError('incomplete-synthesis-transfer')
        return data


def result(state, audio_file, cached):
    return {'schema': 'telecodex.speech-audio/v1', 'render_id': state['identity_sha256'],
            'audio_path': str(audio_file), 'audio_sha256': state['audio_sha256'],
            'text_sha256': state['text_sha256'], 'voice': state['voice'],
            'language': state['language'], 'format': state['format'],
            'cache_hit': cached, 'playback': 'not_observed'}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binding', type=Path, required=True)
    parser.add_argument('--prune', action='store_true', help='Expire completed cached audio without provider calls')
    args = parser.parse_args()
    os.umask(0o077)
    try:
        if args.prune:
            print(json.dumps(prune_audio(route(args.binding))))
            return 0
        raw = sys.stdin.buffer.read(MAX_INPUT + 1)
        if len(raw) > MAX_INPUT:
            raise RenderError('speech-request-bound')
        value = render(args.binding, json.loads(raw))
        print(json.dumps(value))
        return 0
    except RenderError as error:
        print(json.dumps({'ok': False, 'error': str(error), 'automatic_retry': False}))
    except (OSError, ValueError, KeyError, TypeError):
        print(json.dumps({'ok': False, 'error': 'speech-render-unavailable', 'automatic_retry': False}))
    return 1


if __name__ == '__main__':
    sys.exit(main())
