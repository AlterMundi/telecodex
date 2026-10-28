"""Authenticated voice preparation over one bounded WebSocket; no input dispatch."""
import asyncio
from contextlib import suppress
import hashlib
import io
import json
import logging
from pathlib import Path
import re
import secrets
import time
import threading
import wave

from fastapi import HTTPException, WebSocket
from starlette.requests import Request
from starlette.websockets import WebSocketDisconnect

from .private_state import private_directory, read_json, write_json
from .transcription import TranscriptionStream, TranscriptionUnavailable
from scripts.speech_render import lock


def wav_bytes(pcm):
    output = io.BytesIO()
    with wave.open(output, 'wb') as audio:
        audio.setnchannels(1); audio.setsampwidth(2); audio.setframerate(16000)
        audio.writeframes(pcm)
    return output.getvalue()


def save_prepared(root, ident, binding, pcm, result):
    """Only completed server recognition can prepare an exact original WAV."""
    if (str(__import__('uuid').UUID(ident)) != ident
            or not re.fullmatch('[a-f0-9]{64}', binding)
            or not isinstance(pcm, bytes) or not pcm or len(pcm) % 2
            or len(pcm) > TranscriptionStream.max_audio_bytes
            or result.get('final') is not True
            or result.get('pcm_sha256') != hashlib.sha256(pcm).hexdigest()
            or result.get('audio_millis') != len(pcm)*1000//32000
            or not isinstance(result.get('text'), str) or not result['text'].strip()
            or len(result['text'].encode()) > 16000):
        raise ValueError('Completed recognition must match the captured audio')
    root = private_directory(root)
    value = {'schema':'telecodex.streaming-transcript/v1', 'capture_id':ident,
             'binding':binding, 'source_sha256':hashlib.sha256(wav_bytes(pcm)).hexdigest(),
             'created_at':time.time(), **result}
    with lock(root / '.lock'):
        target = root / (ident + '.json')
        if target.exists() or target.is_symlink():
            old = read_json(target)
            if any(old.get(k) != v for k,v in value.items() if k != 'created_at'):
                raise ValueError('Prepared recording identity cannot be replaced')
            return old
        # This cache retains text only. Never evict an uncertain/original input.
        files = list(root.iterdir())
        if len(files) > 32768 or sum(p.lstat().st_size for p in files) + 65536 > 64*1024*1024:
            raise ValueError('Prepared transcription storage is full')
        write_json(target, value)
    return value


def install(app, config, provider, authorize, origins):
    selected = config.get('streaming_transcription', {})
    if selected.get('enabled') is not True:
        return False
    mode=selected.get('mode','prepared')
    if (mode not in ('prepared','captions')
            or selected.get('provider','qwen') not in ('qwen','gemini')
            or set(selected)-{'enabled','endpoint','credential_file','cache_root','mode','provider','fallback'}
            or not {'enabled','endpoint','credential_file','cache_root'}<=set(selected)
            or any(not isinstance(selected[k], str) for k in ('endpoint','credential_file','cache_root'))
            or not all(Path(selected[k]).is_absolute() for k in ('credential_file','cache_root'))):
        raise ValueError('Explicit private streaming-transcription binding required')
    fallback=selected.get('fallback')
    if fallback is not None and (selected.get('provider')!='gemini'
            or not isinstance(fallback,dict) or set(fallback)!={'provider','endpoint','credential_file'}
            or fallback.get('provider')!='qwen'
            or not isinstance(fallback.get('endpoint'),str)
            or not isinstance(fallback.get('credential_file'),str)
            or not Path(fallback['credential_file']).is_absolute()):
        raise ValueError('Select one explicit Qwen fallback for Gemini streaming')
    candidates=[selected]+([fallback] if fallback is not None else [])
    root = private_directory(selected['cache_root'])
    active = set()

    @app.websocket('/api/interaction/voice/live')
    async def voice_live(socket: WebSocket):
        diagnostic = {'stage':'metadata', 'audio_bytes':0, 'finish_received':False, 'prepared_saved':False}
        stream = None
        reader = None
        owner = None
        incoming = None
        opening_lock, abandoned, opened = threading.Lock(), threading.Event(), []
        if socket.headers.get('origin') not in origins:
            await socket.close(code=1008); return
        await socket.accept()
        try:
            async with asyncio.timeout(140):
                raw = await asyncio.wait_for(socket.receive_text(), timeout=5)
                if len(raw.encode()) > 8192:
                    raise ValueError('Streaming capture metadata exceeds bound')
                meta = json.loads(raw)
                if (not isinstance(meta, dict) or set(meta)-{'id','binding','csrf','authorization','mode'}
                        or not all(isinstance(meta.get(k),str) for k in ('id','binding','csrf'))
                        or not re.fullmatch('[a-f0-9-]{36}',meta['id'])
                        or not re.fullmatch('[a-f0-9]{64}',meta['binding'])):
                    raise ValueError('One exact capture binding required')
                if meta.get('mode','prepared')!=mode:
                    raise ValueError('Capture transcription mode changed')
                provider.input_id(meta['id'])
                diagnostic['capture_id'] = meta['id']
                headers = list(socket.scope['headers'])
                if 'authorization' in meta:
                    if (not isinstance(meta['authorization'],str) or len(meta['authorization'])>128
                            or socket.headers.getlist('authorization')):
                        raise ValueError('One explicit session credential required')
                    headers.append((b'authorization',meta['authorization'].encode('ascii')))
                request = Request({**socket.scope, 'type':'http', 'method':'POST',
                    'scheme':'https' if socket.url.scheme=='wss' else 'http', 'headers':headers})
                session = await authorize(request)
                if not secrets.compare_digest(meta['csrf'],session['csrf']):
                    raise HTTPException(403,'Action token missing')
                if not getattr(provider,'receiver',{}).get('voice'):
                    raise ValueError('Voice input unavailable')
                async def recheck():
                    current = await authorize(request)
                    if current['human'] != session['human']:
                        raise HTTPException(403,'Conversation audience changed')
                    value = await asyncio.to_thread(provider.presentation, f'input list {2**63-1} after')
                    if value['binding'] != meta['binding']:
                        raise ValueError('Conversation binding changed')
                await recheck()
                owner = session['human']
                if owner in active or len(active) >= 2:
                    owner = None
                    raise ValueError('Voice preparation capacity occupied')
                active.add(owner)
                candidate_index=0
                def open_provider(start=0):
                    nonlocal candidate_index
                    for index in range(start,len(candidates)):
                        if abandoned.is_set():raise TranscriptionUnavailable('Capture closed during connection setup')
                        choice=candidates[index]
                        try:
                            value=TranscriptionStream(choice['endpoint'],Path(choice['credential_file']),
                                                      provider=choice.get('provider','qwen'))
                        except (OSError,ValueError,TranscriptionUnavailable):
                            if index+1==len(candidates):raise
                            continue
                        with opening_lock:
                            if not abandoned.is_set():
                                candidate_index=index;opened.append(value)
                                return value
                        value.close()
                        raise TranscriptionUnavailable('Capture closed during connection setup')
                    raise TranscriptionUnavailable('No configured streaming provider')
                diagnostic['stage'] = 'provider_open'
                stream = await asyncio.to_thread(open_provider)
                pcm = bytearray()
                ready, finished = asyncio.Event(), asyncio.Event()
                async def receive_provider():
                    nonlocal stream
                    opening_deadline=time.monotonic()+8
                    while True:
                        try:
                            if not ready.is_set() and time.monotonic()>=opening_deadline:
                                raise TranscriptionUnavailable('Streaming readiness unavailable')
                            event = await asyncio.to_thread(stream.receive)
                        except (OSError,ValueError,TranscriptionUnavailable):
                            # No PCM can be accepted before ready. Once advertised,
                            # never replay this capture into another paid stream.
                            if ready.is_set() or candidate_index+1>=len(candidates):raise
                            await recheck()
                            old=stream
                            await asyncio.to_thread(old.close)
                            with opening_lock:
                                if old in opened:opened.remove(old)
                            stream=await asyncio.to_thread(open_provider,candidate_index+1)
                            opening_deadline=time.monotonic()+8
                            continue
                        if event['type']=='ready': ready.set()
                        if event['type'] in ('ready','partial'):
                            await recheck()
                            await socket.send_json(event)
                        elif event['type']=='final':
                            if mode=='captions':
                                raise ValueError('Live captions are not a confirmed transcript')
                            diagnostic['stage'] = 'final_authorization'
                            await recheck()
                            diagnostic['stage'] = 'save_prepared'
                            value = await asyncio.to_thread(save_prepared,root,meta['id'],meta['binding'],bytes(pcm),stream.result())
                            diagnostic['prepared_saved'] = True
                            await socket.send_json({'type':'prepared','id':meta['id'],
                                'source_sha256':value['source_sha256'],'text':value['text']})
                            finished.set(); return
                reader = asyncio.create_task(receive_provider())
                checked = time.monotonic()
                while not finished.is_set():
                    incoming = asyncio.create_task(socket.receive())
                    done,_ = await asyncio.wait((incoming,reader), return_when=asyncio.FIRST_COMPLETED)
                    if reader in done:
                        incoming.cancel()
                        with suppress(asyncio.CancelledError): await incoming
                        await reader
                        break
                    message = await incoming
                    if message['type']=='websocket.disconnect':
                        break  # No finish or original admission on disconnect.
                    if time.monotonic()-checked >= 15:
                        await recheck();checked = time.monotonic()
                    if 'bytes' in message and message['bytes'] is not None:
                        if not ready.is_set():raise ValueError('Wait for recognition readiness')
                        data = message['bytes']
                        await asyncio.to_thread(stream.append,data)
                        diagnostic['audio_bytes'] += len(data)
                        diagnostic['stage'] = 'receiving_audio'
                        if mode=='prepared':pcm.extend(data)
                    else:
                        control = message.get('text','')
                        if len(control.encode())>256:raise ValueError('Control exceeds bound')
                        command = json.loads(control)
                        if command == {'type':'discard'}:
                            diagnostic['discarded'] = True
                            break
                        if command != {'type':'finish'}:raise ValueError('Unknown capture control')
                        diagnostic['finish_received'] = True
                        if mode=='captions':raise ValueError('Live captions cannot finalize the original')
                        diagnostic['stage'] = 'finish_authorization'
                        await recheck()
                        diagnostic['stage'] = 'provider_finalize'
                        await asyncio.to_thread(stream.finish)
                        await reader
                        break
        except (HTTPException, ValueError, OSError, UnicodeError, TranscriptionUnavailable,
                WebSocketDisconnect, TimeoutError) as error:
            diagnostic['error_type'] = type(error).__name__
            with suppress(Exception):
                await socket.send_json({'type':'unavailable','message':'Live transcription unavailable. Your recording has not been submitted.'})
        finally:
            # One metadata-only line under the existing bounded system journal.
            # Never include provider events, exception messages, speech or keys.
            if (mode == 'prepared' and diagnostic['audio_bytes']
                    and not diagnostic['prepared_saved'] and not diagnostic.get('discarded')):
                logging.getLogger(__name__).warning('voice_preparation_closed %s',
                    json.dumps(diagnostic, sort_keys=True))
            try:
                # Cancel before any cleanup await: WebView teardown can cancel this
                # handler again while close() unblocks the provider's reader.
                if reader is not None:
                    reader.cancel()
                    if reader.done():
                        with suppress(asyncio.CancelledError,Exception):reader.result()
                if incoming is not None:
                    incoming.cancel()
                # A canceled async wait cannot cancel the blocking dial. Its owning
                # thread closes a late socket instead of orphaning it after timeout.
                with opening_lock:
                    abandoned.set()
                    closing = opened.pop() if opened else None
                if closing is not None:
                    closing_task = asyncio.create_task(asyncio.to_thread(closing.close))
                    # Retrieve completion even if the WebView cancels our wait.
                    closing_task.add_done_callback(lambda task: None if task.cancelled() else task.exception())
                    await asyncio.shield(closing_task)
                if incoming is not None:
                    with suppress(asyncio.CancelledError,Exception):await incoming
                if reader is not None:
                    reader.cancel()
                    with suppress(asyncio.CancelledError,Exception): await reader
                with suppress(Exception):await socket.close()
            finally:
                if owner is not None:active.discard(owner)
    return True
