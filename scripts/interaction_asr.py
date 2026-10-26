#!/usr/bin/env python3
"""Disposable preparation for one finalized interaction clip, with durable text.

Reuse the selected foreground voice-note helper. This adapter owns only its
private preparation cache; it never deletes the caller's original recording.
"""
import argparse
import hashlib
import importlib.util
import json
import io
import math
import os
from pathlib import Path
import re
import stat
import subprocess
import sys
import wave
import uuid

from speech_render import (RenderError, atomic, canonical, directory, lock,
                           private_bytes, sync_directory)

MAX_SOURCE = 20 * 1024 * 1024
MAX_JOB = 32 * 1024 * 1024
DEFAULT_BUDGET = 128 * 1024 * 1024


def digest(data):
    return hashlib.sha256(data).hexdigest()


def probe_duration(source):
    # The reused helper prefers first-audio-stream duration, then the container.
    # Bound preparation before it can write PCM, not only after paid execution.
    output = subprocess.check_output(['ffprobe', '-v', 'error', '-select_streams', 'a:0',
        '-show_entries', 'stream=duration:format=duration', '-of', 'json', str(source)], timeout=15,
        stderr=subprocess.DEVNULL)
    if len(output) > 4096:
        raise RenderError('asr-duration-probe-bound')
    info = json.loads(output)
    stream = next(iter(info.get('streams', [])), {})
    durations = [float(value) for value in (stream.get('duration'),
                 info.get('format', {}).get('duration')) if value]
    if any(not math.isfinite(value) or not 0 < value <= 120 for value in durations):
        raise RenderError('interaction-asr-duration-bound')
    if durations:
        return max(durations), None
    # MediaRecorder emits streaming WebM without a declared duration. Decode at
    # most one second beyond our bound, then give the existing file helper a
    # finalized WAV. No provider is contacted and no disk payload is written yet.
    pcm = subprocess.check_output(['ffmpeg', '-v', 'error', '-nostdin', '-i', str(source),
        '-t', '121', '-vn', '-ac', '1', '-ar', '16000', '-f', 's16le', 'pipe:1'],
        timeout=15, stderr=subprocess.DEVNULL)
    duration = len(pcm) / 32000
    if len(pcm) % 2 or not 0 < duration <= 120:
        raise RenderError('interaction-asr-duration-bound')
    return duration, pcm


def config(binding):
    value = json.loads(private_bytes(binding, 16384))
    if (not isinstance(value, dict)
            or set(value) - {'schema', 'helper', 'route_binding', 'cache_root', 'max_cache_bytes', 'streaming_cache_root'}
            or value.get('schema') != 'telecodex.interaction-asr/v1'
            or any(not isinstance(value.get(k), str) or not Path(value[k]).is_absolute()
                   for k in ('helper', 'route_binding', 'cache_root'))
            or type(value.get('max_cache_bytes', DEFAULT_BUDGET)) is not int
            or not MAX_JOB <= value.get('max_cache_bytes', DEFAULT_BUDGET) <= 1024**3):
        raise RenderError('invalid-interaction-asr-binding')
    if 'streaming_cache_root' in value and (not isinstance(value['streaming_cache_root'],str) or not Path(value['streaming_cache_root']).is_absolute()):
        raise RenderError('invalid-streaming-cache-binding')
    return value


def streaming_result(cfg, source, source_hash):
    root = cfg.get('streaming_cache_root')
    if root is None:
        return None
    try:
        ident = str(uuid.UUID(Path(source).name))
    except ValueError:
        return None  # Legacy source names retain the ordinary file-ASR path.
    if ident != Path(source).name:
        return None
    directory(root)
    try:
        record = json.loads(private_bytes(Path(root)/(ident+'.json'),65536))
    except FileNotFoundError:
        return None
    if (not isinstance(record,dict) or record.get('schema') != 'telecodex.streaming-transcript/v1'
            or record.get('capture_id') != ident or record.get('source_sha256') != source_hash
            or record.get('final') is not True
            or record.get('model') != 'qwen-audio-3.0-realtime-plus'
            or not re.fullmatch('[a-f0-9]{64}',str(record.get('binding','')))
            or not isinstance(record.get('text'),str) or not record['text'].strip()
            or len(record['text'].encode()) > 16000):
        raise RenderError('invalid-completed-streaming-transcript')
    try:
        with wave.open(io.BytesIO(private_bytes(source,MAX_SOURCE)),'rb') as audio:
            if (audio.getnchannels(),audio.getsampwidth(),audio.getframerate(),audio.getcomptype()) != (1,2,16000,'NONE'):
                raise ValueError('Exact captured PCM required')
            pcm=audio.readframes(120*16000+1)
    except (ValueError,wave.Error,EOFError):
        raise RenderError('invalid-streaming-audio-original') from None
    if (not pcm or len(pcm)>120*32000 or digest(pcm)!=record.get('pcm_sha256')
            or len(pcm)*1000//32000 != record.get('audio_millis')):
        raise RenderError('streaming-audio-original-changed')
    return {'text':record['text'],'streaming_model':record['model']}


def payload_bytes(root):
    total = 0
    for path in root.rglob('*'):
        info = path.lstat()
        if (info.st_uid != os.getuid() or info.st_mode & 0o077
                or not (stat.S_ISREG(info.st_mode) or stat.S_ISDIR(info.st_mode))):
            raise RenderError('unsafe-asr-cache-artifact')
        if stat.S_ISREG(info.st_mode):
            total += info.st_size
    return total


def reserve(root, job, state, limit):
    with lock(root / '.budget.lock'):
        used = 0
        for other in root.iterdir():
            if not re.fullmatch('[a-f0-9]{64}', other.name):
                continue
            # Accounting reads retained jobs. directory() establishes a durable
            # name and fsyncs its parent even when it already exists; repeating
            # that barrier for every past note delays the next provider call.
            info = other.lstat()
            if (not stat.S_ISDIR(info.st_mode) or info.st_uid != os.getuid()
                    or info.st_mode & 0o077):
                raise RenderError('unsafe-asr-cache-directory')
            actual = payload_bytes(other)
            try:
                previous = json.loads(private_bytes(other / 'state.json', 2*1024*1024))
            except FileNotFoundError:
                previous = {}
            # Intent and uncertain helper work occupy their maximum reservation.
            used += max(actual, MAX_JOB) if previous.get('phase') in ('intent', 'unconfirmed') else actual
        if used + MAX_JOB > limit:
            raise RenderError('asr-cache-capacity-text-remains-available')
        atomic(job / 'state.json', canonical(state))


def retire_preparation(job, state):
    """Only media listed by this completed helper's manifest is disposable."""
    if state.get('preparation_kind') == 'streaming':
        return  # This completed path retained text only, never raw preparation.
    draft = Path(state['result']['draft_path'])
    preparation = job / 'preparation'
    if draft != draft.resolve() or '..' in draft.parts:
        raise RenderError('asr-preparation-path-mismatch')
    draft.relative_to(preparation)
    helper_job = draft.parents[2]
    manifest = json.loads(private_bytes(helper_job / 'manifest.json', 2*1024*1024))
    sources = manifest.get('sources')
    if (not isinstance(sources, list) or len(sources) != 1
            or sources[0].get('path') != str(helper_job / 'source.media')
            or sources[0].get('sha256') != state.get('prepared_source_sha256', state['source_sha256'])):
        raise RenderError('asr-preparation-source-mismatch')
    media = [(helper_job / 'source.media', sources[0]['sha256'])]
    if 'prepared_source_sha256' in state:
        media.append((preparation / 'input.wav', state['prepared_source_sha256']))
    for chunk in manifest['chunks']:
        if not re.fullmatch(r'chunks/[0-9]{5}\.wav', chunk['file']):
            raise RenderError('asr-preparation-chunk-mismatch')
        media.append((helper_job / chunk['file'], chunk['sha256']))
    for path, expected in media:
        # Validate all ancestors inside this adapter's cache, never follow a link.
        parent = path.parent
        while parent != job:
            directory(parent)
            parent = parent.parent
        try:
            data = private_bytes(path, MAX_JOB)
        except FileNotFoundError:
            continue  # Complete an interrupted retirement without ASR replay.
        if digest(data) != expected:
            raise RenderError('asr-preparation-media-changed')
        path.unlink()
        sync_directory(path.parent)


def process(source, binding, include_retention=False, completed_only=False):
    cfg = config(binding)
    source_hash = digest(private_bytes(source, MAX_SOURCE))
    route_bytes = private_bytes(cfg['route_binding'], 16384)
    selected_route = json.loads(route_bytes)
    if selected_route.get('schema') != 'audio-transcription.route/v1':
        raise RenderError('invalid-selected-foreground-route')
    helper = Path(cfg['helper'])
    # Helper source is public code, while bindings and media remain private.
    helper_hash = digest(helper.read_bytes())
    identity = {'schema': 'telecodex.interaction-asr-result/v1',
                'source_sha256': source_hash, 'route_sha256': digest(route_bytes),
                'helper_sha256': helper_hash}
    root = directory(cfg['cache_root'])
    job = directory(root / digest(canonical(identity)))
    state_path = job / 'state.json'
    with lock(job / '.lock'):
        if state_path.exists() or state_path.is_symlink():
            state = json.loads(private_bytes(state_path, 2*1024*1024))
            if (any(state.get(k) != v for k, v in identity.items())
                    or state.get('phase') != 'completed'
                    or state.get('result_sha256') != digest(canonical(state.get('result')))):
                raise RenderError('retained-asr-outcome-no-automatic-retry')
        else:
            prepared = streaming_result(cfg,source,source_hash)
            if prepared is not None:
                state = {**identity, 'phase':'completed', 'preparation_kind':'streaming',
                         'result':prepared, 'result_sha256':digest(canonical(prepared))}
                reserve(root,job,state,cfg.get('max_cache_bytes',DEFAULT_BUDGET))
            else:
                if completed_only:
                    raise RenderError("no-completed-transcript-no-asr-attempted")
                _, pcm = probe_duration(source)
                state = {**identity, 'phase': 'intent'}
                reserve(root, job, state, cfg.get('max_cache_bytes', DEFAULT_BUDGET))
                preparation = directory(job / 'preparation')
                helper_source = Path(source)
                if pcm is not None:
                    helper_source = preparation / 'input.wav'
                    with wave.open(str(helper_source), 'wb') as output:
                        output.setnchannels(1)
                        output.setsampwidth(2)
                        output.setframerate(16000)
                        output.writeframes(pcm)
                    state['prepared_source_sha256'] = digest(private_bytes(helper_source, MAX_SOURCE))
                    atomic(state_path, canonical(state))
                local_route = {**selected_route, 'cache_root': str(preparation)}
                atomic(job / 'route.json', canonical(local_route))
                # Same helper/provider behavior, isolated disposable preparation root.
                sys.path.insert(0, str(helper.parent))
                try:
                    spec = importlib.util.spec_from_file_location('selected_voice_note', helper)
                    module = importlib.util.module_from_spec(spec)
                    spec.loader.exec_module(module)
                    result = module.process(helper_source, job / 'route.json')
                    if (not isinstance(result.get('text'), str) or not result['text'].strip()
                            or len(result['text'].encode()) > 16000
                            or digest(helper.read_bytes()) != helper_hash
                            or private_bytes(cfg['route_binding'], 16384) != route_bytes
                            or digest(private_bytes(source, MAX_SOURCE)) != source_hash):
                        raise RenderError('invalid-final-interaction-transcript')
                    # The generic helper declares completeness in its retained draft.
                    draft = Path(result['draft_path'])
                    draft.relative_to(preparation)
                    report = json.loads(private_bytes(draft, 2*1024*1024))
                    if report.get('processing_complete') is not True:
                        raise RenderError('incomplete-interaction-transcript')
                    state.update(phase='completed', result=result, result_sha256=digest(canonical(result)))
                    atomic(state_path, canonical(state))
                except Exception:
                    # Never turn an ambiguous provider operation into an automatic retry.
                    state['phase'] = 'unconfirmed'
                    atomic(state_path, canonical(state))
                    raise RenderError('asr-unconfirmed-no-automatic-retry') from None
        preparation_deleted = False
        try:
            retire_preparation(job, state)
            preparation_deleted = True
        except (OSError, ValueError, KeyError, TypeError, RenderError):
            # Durable text remains usable; a later exact read can finish cleanup.
            pass
        result = {'text': state['result']['text'], 'final': True}
        if include_retention:
            result['preparation_audio_deleted'] = preparation_deleted
        return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binding', type=Path, required=True)
    parser.add_argument('source', type=Path)
    parser.add_argument('--retention-receipt', action='store_true', help='Report verified adapter-owned preparation deletion separately from final text')
    parser.add_argument('--completed-only', action='store_true', help='Read verified completed text only; never prepare audio or call ASR')
    args = parser.parse_args()
    os.umask(0o077)
    try:
        print(json.dumps(process(args.source, args.binding, include_retention=args.retention_receipt, completed_only=args.completed_only), ensure_ascii=False))
        return 0
    except (OSError, ValueError, KeyError, TypeError, RenderError, subprocess.SubprocessError):
        print(json.dumps({'ok': False, 'error': 'interaction-asr-unavailable', 'automatic_retry': False}))
        return 1


if __name__ == '__main__':
    sys.exit(main())
