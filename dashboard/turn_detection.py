"""One optional, bounded analysis child for an already authorized voice capture."""
import asyncio
import base64
from contextlib import suppress
import json

from scripts.capture_turn import MODEL_SHA256, SCHEMA, VAD_SHA256


class DetectionUnavailable(Exception):
    pass


def command(config):
    selected = config.get('turn_detection', {})
    if selected.get('enabled') is not True:
        return None
    value = selected.get('command')
    if (not {'enabled', 'command'} <= set(selected) or set(selected) - {'enabled', 'command', 'continuous'}
            or 'continuous' in selected and type(selected['continuous']) is not bool or not isinstance(value, list)
            or not 2 <= len(value) <= 16 or any(not isinstance(x, str) or not x or len(x) > 2048 for x in value)
            or not value[0].startswith('/') or config.get('mode', 'prepared') != 'prepared'):
        raise ValueError('Select one prepared-mode turn-analysis command')
    return value


class Detector:
    @classmethod
    async def open(cls, argv, *, continuous=False):
        value = cls()
        value.continuous = continuous
        value.process = await asyncio.create_subprocess_exec(*argv, *(['--continuous'] if continuous else []), stdin=asyncio.subprocess.PIPE,
            stdout=asyncio.subprocess.PIPE, stderr=asyncio.subprocess.DEVNULL, limit=8192,
            start_new_session=True)
        try:
            ready = await asyncio.wait_for(value.receive(), 30 if continuous else 15)
            if ready != {'schema': SCHEMA, 'type': 'ready', 'vad_sha256': VAD_SHA256, 'turn_sha256': MODEL_SHA256, **({'continuous': True} if continuous else {})}:
                raise DetectionUnavailable('Unexpected analysis worker')
            return value
        except BaseException:
            await value.close()
            raise

    async def feed(self, pcm, *, defer=False):
        if not pcm or len(pcm) > 32000 or len(pcm) % 2:
            raise DetectionUnavailable('Invalid analysis PCM')
        await self.write({'pcm': base64.b64encode(pcm).decode('ascii'), **({'defer': True} if defer else {})})

    async def boundary(self, value):
        if not self.continuous:
            raise DetectionUnavailable('Foreground analysis not selected')
        await self.write({'boundary': value})

    async def write(self, value):
        try:
            self.process.stdin.write(json.dumps(value).encode() + b'\n')
            await asyncio.wait_for(self.process.stdin.drain(), 1)
        except (OSError, TimeoutError, RuntimeError) as error:
            raise DetectionUnavailable('Analysis input unavailable') from error

    async def receive(self):
        try:
            raw = await self.process.stdout.readline()
            if not raw or len(raw) > 8192:
                raise ValueError('Analysis output outside bounds')
            value = json.loads(raw)
            if not isinstance(value, dict) or value.get('schema') != SCHEMA or value.get('type') not in (('ready', 'activity', 'boundary') if self.continuous else ('ready', 'activity')):
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


async def analyze_capture(open_worker, pcm, changed, on_activity):
    """Analyze this capture without blocking ASR; at most8s of analysis backlog.

    Model startup runs beside recognition. A late first join analyzes only the
    current eight-second tail, preserving absolute original sample offsets.
    Later backlog overflow still disables analysis. The original PCM remains
    untouched; no extra audio buffer or model cache is retained.
    """
    worker = reader = wake = None
    try:
        worker = await open_worker()
        origin = max(0, len(pcm) - 256000)
        origin += origin % 2  # Start on a complete PCM16 sample.
        async def receive():
            while True:
                value = await worker.receive()
                if value.get('type') != 'activity':
                    raise DetectionUnavailable('Unexpected analysis response')
                # The worker starts its bounded tail at zero. Client fences
                # always refer to the complete original, not that local tail.
                value = {**value, 'audio_bytes': value['audio_bytes'] + origin}
                if 'onset' in value:
                    value['onset'] = {**value['onset'], 'audio_bytes': value['onset']['audio_bytes'] + origin}
                if 'endpoint' in value:
                    value['endpoint'] = {**value['endpoint'],
                        'audio_bytes': value['endpoint']['audio_bytes'] + origin}
                await on_activity(value)
        reader = asyncio.create_task(receive())
        fed = origin
        initial = True
        while True:
            changed.clear()
            end = len(pcm)
            if end - fed > 256000:
                raise DetectionUnavailable('Analysis fell behind this capture')
            while fed < end:
                stop = min(fed + 32000, end)
                chunk = bytes(pcm[fed:stop])
                if initial and stop < end:
                    await worker.feed(chunk, defer=True)
                else:
                    await worker.feed(chunk)
                fed = stop
            if end:
                initial = False
            if reader.done():
                await reader
            wake = asyncio.create_task(changed.wait())
            done, _ = await asyncio.wait((wake, reader), return_when=asyncio.FIRST_COMPLETED)
            if reader in done:
                await reader
            wake.cancel()
            with suppress(asyncio.CancelledError):
                await wake
            wake = None
    finally:
        for task in (reader, wake):
            if task is not None:
                task.cancel()
        if worker is not None:
            closing = asyncio.create_task(worker.close())
            closing.add_done_callback(lambda task: None if task.cancelled() else task.exception())
            await asyncio.shield(closing)
        for task in (reader, wake):
            if task is not None:
                with suppress(asyncio.CancelledError, Exception):
                    await task
