"""Source-linked synthesis; delivery to a client never claims playback."""
import asyncio
import hashlib
import json
import os
from pathlib import Path
import re
import signal
import stat
import sys

from scripts import speech_render


class SpeechUnavailable(Exception):
    pass


def speech_part(source):
    """Choose an exact canonical prefix; offsets remain Unicode character counts."""
    # Question labels and their synthetic prefixes keep the existing reader contract.
    if source.get('kind') == 'native_question':
        return source
    text = source['text']
    bound = 240 if source['offset'] == 0 else 640
    if len(text) <= bound:
        return source
    window = text[:bound]
    boundaries = [m.end() for m in re.finditer(r'[.!?](?:\s|$)|\n', window)
                  if m.end() >= 60]
    end = boundaries[-1] if boundaries else window.rfind(' ')
    if end < 60:
        end = bound
    # Keep ordinary links/code/emphasis together rather than feeding a TTS
    # segment half a label or URL. This is a conservative boundary preference;
    # Markdown interpretation itself belongs to the standard parser.
    spans = r'!?\[[^\]\n]*\]\([^\n]*?\)|(`+)[^`]*\1|\*\*[^*]*\*\*|__[^_]*__'
    for match in re.finditer(spans, text):
        if match.start() < end < match.end():
            end = match.start() if match.start() >= 60 else match.end()
            break
    return {**source, 'text':text[:end], 'next_offset':source['offset']+end}


def speech_request(source, language="en-US"):
    text = source['text']
    if source.get('kind') in ('worker_result','worker_progress','matrix_result') and source.get('offset') == 0:
        label = source.get('label')
        if not isinstance(label,str) or not label.strip():
            raise SpeechUnavailable('Selected work source has no audible label')
        label = label.strip().lstrip('#').strip()
        if not label:
            raise SpeechUnavailable('Selected work source has no audible label')
        introduction = 'Novedades desde' if language.split('-')[0] == 'es' else 'Update from'
        text = introduction + ' ' + label[:198] + '\n\n' + text
    scope = speech_render.sha(speech_render.canonical([source['entry_binding'],source['id'],source['offset']]))
    return {'scope':scope,'text':text}


def canonical_word_alignment(source, payload, language, alignment):
    """Only exact plain-text projection can certify canonical word positions.

    Markdown/URL/topic-label transformations retain approximate Follow. Do not
    guess a canonical position from provider ordinals or a text similarity match.
    """
    text = payload['text']
    spoken = speech_render.spoken_markdown(text, language)
    if (not isinstance(alignment, dict)
            or alignment.get('text_sha256') != hashlib.sha256(spoken.encode()).hexdigest()
            or not text.endswith(source['text']) or not spoken.endswith(source['text']) or not isinstance(alignment.get('words'), list)
            or not 1 <= len(alignment['words']) <= 512):
        return None
    prefix = len(spoken)-len(source['text']); result = []; previous = 0; last_time = 0
    for word in alignment['words']:
        if (not isinstance(word,list) or len(word)!=4 or any(type(n) is not int for n in word)):
            return None
        begin,end,start,stop = word
        if not previous <= begin < end <= len(spoken) or not last_time <= start < stop <= 3600000:
            return None
        previous=end;last_time=stop
        if begin >= prefix:
            result.append([source['offset']+begin-prefix,source['offset']+end-prefix,start,stop])
    return {'offset':source['offset'],'words':result} if result else None


class Speech:
    def __init__(self, config):
        self.binding = Path(config['binding'])
        self.timeout = config.get('timeout_seconds', 60)
        if not self.binding.is_absolute() or type(self.timeout) is not int or not 1 <= self.timeout <= 120:
            raise ValueError('Absolute owner speech binding and bounded deadline required')
        self.incremental = config.get('incremental_segments',False)
        if type(self.incremental) is not bool:
            raise ValueError('incremental_segments must be a boolean')
        self.streaming = config.get('streaming_output',False)
        if type(self.streaming) is not bool:
            raise ValueError('streaming_output must be a boolean')
        self.word_timestamps = config.get('word_timestamps',False)
        if type(self.word_timestamps) is not bool:
            raise ValueError('word_timestamps must be a boolean')
        self.live_response = config.get('live_response',False)
        if type(self.live_response) is not bool:
            raise ValueError('live_response must be a boolean')
        self.capacity = asyncio.Semaphore(2)

    async def render(self, source):
        if self.capacity.locked():
            raise SpeechUnavailable('Speech preparation capacity is occupied')
        async with self.capacity:
            return await self._render(source)

    async def _render(self, source):
        # The caller supplies a controller-certified part, never browser-authored text.
        config_bytes = speech_render.private_bytes(self.binding, 16384)
        route = speech_render.route(self.binding)
        payload = speech_request(source, route['language'])
        text = payload['text']
        request = json.dumps(payload).encode()
        command = [sys.executable, str(Path(speech_render.__file__).resolve()), '--binding', str(self.binding)]
        child = await asyncio.create_subprocess_exec(*command, stdin=asyncio.subprocess.PIPE,
            stdout=asyncio.subprocess.PIPE, stderr=asyncio.subprocess.DEVNULL, start_new_session=True)
        try:
            async with asyncio.timeout(self.timeout):
                child.stdin.write(request)
                await child.stdin.drain()
                child.stdin.close()
                output = bytearray()
                while chunk := await child.stdout.read(min(4096, 65537 - len(output))):
                    output.extend(chunk)
                    if len(output) > 65536:
                        raise SpeechUnavailable('Speech renderer output exceeds bound')
                code = await child.wait()
                if code != 0:
                    raise SpeechUnavailable('Speech render incomplete; no automatic provider retry')
        finally:
            # Only the renderer's owned process group, independent of all workers.
            try:
                os.killpg(child.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            await child.wait()
        if speech_render.private_bytes(self.binding, 16384) != config_bytes:
            raise SpeechUnavailable('Speech receiving binding changed')
        result = json.loads(output)
        if not isinstance(result, dict):
            raise SpeechUnavailable('Speech renderer response unavailable')
        render_id = result.get('render_id', '')
        if (result.get('schema') != 'telecodex.speech-audio/v1'
                or not isinstance(render_id, str) or not re.fullmatch(r'[a-f0-9]{64}', render_id)
                or result.get('text_sha256') != hashlib.sha256(text.encode()).hexdigest()
                or result.get('voice') != route['voice'] or result.get('language') != route['language']):
            raise SpeechUnavailable('Speech render source mismatch')
        audio = Path(route['cache_root']) / render_id / 'audio.wav'
        if result.get('audio_path') != str(audio):
            raise SpeechUnavailable('Speech renderer selected an unrelated artifact')
        for directory in (audio.parent.parent, audio.parent):
            info = directory.lstat()
            if not stat.S_ISDIR(info.st_mode) or info.st_uid != os.getuid() or info.st_mode & 0o077:
                raise SpeechUnavailable('Speech artifact directory unavailable')
        data = speech_render.private_bytes(audio, speech_render.MAX_AUDIO)
        if (hashlib.sha256(data).hexdigest() != result.get('audio_sha256')
                or speech_render.audio_info(data) != result.get('format')):
            raise SpeechUnavailable('Speech artifact integrity failed')
        return data, result

    async def stream(self, source):
        if not self.streaming or self.capacity.locked():
            raise SpeechUnavailable('Streaming speech unavailable or occupied')
        async with self.capacity:
            config_bytes = speech_render.private_bytes(self.binding,16384)
            command = [sys.executable,str(Path(speech_render.__file__).resolve()),'--binding',str(self.binding),'--stream']
            child = await asyncio.create_subprocess_exec(*command,stdin=asyncio.subprocess.PIPE,
                stdout=asyncio.subprocess.PIPE,stderr=asyncio.subprocess.DEVNULL,start_new_session=True,limit=65536)
            total = 0; ended = False
            try:
                async with asyncio.timeout(self.timeout):
                    language=speech_render.route(self.binding)['language']
                    payload=speech_request(source,language)
                    request={**payload, **({'timing':True} if self.word_timestamps else {})}
                    child.stdin.write(json.dumps(request).encode());await child.stdin.drain();child.stdin.close()
                    while raw := await child.stdout.readline():
                        total += len(raw)
                        if len(raw)>65536 or total>16*1024*1024 or ended:
                            raise SpeechUnavailable('Streaming speech output exceeds bound')
                        value=json.loads(raw)
                        if value.get('type') not in ('audio','end'):
                            raise SpeechUnavailable('Speech outcome retained; no automatic retry')
                        if speech_render.private_bytes(self.binding,16384)!=config_bytes:
                            raise SpeechUnavailable('Speech receiving binding changed')
                        ended=value['type']=='end'
                        if ended:
                            # Retain the terminal frame until the process confirms success.
                            terminal=value
                        else:
                            yield value
                    if await child.wait()!=0 or not ended:
                        raise SpeechUnavailable('Streaming speech incomplete')
                    alignment=canonical_word_alignment(source,payload,language,terminal.pop('alignment',None))
                    if alignment is not None:
                        terminal['alignment']=alignment
                    yield terminal
            finally:
                try:os.killpg(child.pid,signal.SIGKILL)
                except ProcessLookupError:pass
                await child.wait()
