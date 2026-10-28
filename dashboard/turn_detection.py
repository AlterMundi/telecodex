"""One optional, bounded analysis child for an already authorized voice capture."""
import asyncio
import base64
import json

from scripts.capture_turn import MODEL_SHA256, SCHEMA, VAD_SHA256


class DetectionUnavailable(Exception):
    pass


def command(config):
    selected = config.get('turn_detection', {})
    if selected.get('enabled') is not True:
        return None
    value = selected.get('command')
    if (set(selected) != {'enabled', 'command'} or not isinstance(value, list)
            or not 2 <= len(value) <= 16 or any(not isinstance(x, str) or not x or len(x) > 2048 for x in value)
            or not value[0].startswith('/') or config.get('mode', 'prepared') != 'prepared'):
        raise ValueError('Select one prepared-mode turn-analysis command')
    return value


class Detector:
    @classmethod
    async def open(cls, argv):
        value = cls()
        value.process = await asyncio.create_subprocess_exec(*argv, stdin=asyncio.subprocess.PIPE,
            stdout=asyncio.subprocess.PIPE, stderr=asyncio.subprocess.DEVNULL, limit=8192,
            start_new_session=True)
        try:
            ready = await asyncio.wait_for(value.receive(), 8)
            if ready != {'schema': SCHEMA, 'type': 'ready', 'vad_sha256': VAD_SHA256, 'turn_sha256': MODEL_SHA256}:
                raise DetectionUnavailable('Unexpected analysis worker')
            return value
        except BaseException:
            await value.close()
            raise

    async def feed(self, pcm):
        if not pcm or len(pcm) > 32000 or len(pcm) % 2:
            raise DetectionUnavailable('Invalid analysis PCM')
        try:
            self.process.stdin.write(json.dumps({'pcm': base64.b64encode(pcm).decode('ascii')}).encode() + b'\n')
            await asyncio.wait_for(self.process.stdin.drain(), 1)
        except (OSError, TimeoutError, RuntimeError) as error:
            raise DetectionUnavailable('Analysis input unavailable') from error

    async def receive(self):
        try:
            raw = await self.process.stdout.readline()
            if not raw or len(raw) > 8192:
                raise ValueError('Analysis output outside bounds')
            value = json.loads(raw)
            if not isinstance(value, dict) or value.get('schema') != SCHEMA or value.get('type') not in ('ready', 'activity'):
                raise ValueError('Analysis unavailable')
            return value
        except (OSError, ValueError, RuntimeError) as error:
            raise DetectionUnavailable('Analysis output unavailable') from error

    async def close(self):
        if self.process.stdin:
            self.process.stdin.close()
        try:
            await asyncio.wait_for(self.process.wait(), 2)
        except TimeoutError:
            if self.process.returncode is None:
                try: self.process.terminate()
                except ProcessLookupError: pass
            try:
                await asyncio.wait_for(self.process.wait(), 2)
            except TimeoutError:
                if self.process.returncode is None:
                    try: self.process.kill()
                    except ProcessLookupError: pass
                await self.process.wait()
