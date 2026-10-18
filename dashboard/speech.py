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


class Speech:
    def __init__(self, config):
        self.binding = Path(config['binding'])
        self.timeout = config.get('timeout_seconds', 60)
        if not self.binding.is_absolute() or type(self.timeout) is not int or not 1 <= self.timeout <= 120:
            raise ValueError('Absolute owner speech binding and bounded deadline required')
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
        text = source['text']
        if source.get('kind') in ('worker_result','matrix_result') and source.get('offset') == 0:
            label = source.get('label')
            if not isinstance(label, str) or not label.strip():
                raise SpeechUnavailable('Selected work source has no audible label')
            # Keep canonical text/offsets intact. The certified source label is
            # spoken once, within the renderer's existing 1,800-character bound.
            text = label[:198] + '. ' + text
        scope = speech_render.sha(speech_render.canonical([
            source['entry_binding'], source['id'], source['offset']]))
        request = json.dumps({'scope': scope, 'text': text}).encode()
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
