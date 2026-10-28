"""Speech-gated, bounded turn analysis; no models, provider or native input."""
import sys
import asyncio
from pathlib import Path
import tempfile
import unittest
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from scripts.capture_turn import Activity, TurnEndUnavailable
from dashboard.turn_detection import analyze_capture, DetectionUnavailable


class CaptureTurn(unittest.TestCase):
    def analyzer(self):
        value = object.__new__(Activity)
        value.pending = bytearray(); value.tail = bytearray()
        value.bytes = value.frames = value.generation = value.positive = value.quiet = value.analyses = 0
        value.speaking = value.speech_seen = False; value.scored_generation = 0
        self.probability = 0
        value._probability = lambda _: self.probability
        self.scored = []
        class Turn:
            def predict(_, pcm, *, speech_present):
                self.assertTrue(speech_present);self.scored.append(pcm)
                return {'candidate_complete': True, 'probability': .9}
        value.turn = Turn()
        return value

    def test_silence_never_scores_and_pending_frames_are_carried(self):
        value = self.analyzer()
        for _ in range(10): result = value.feed(b'\0'*3200)
        self.assertFalse(result['speech_seen']);self.assertNotIn('endpoint',result)
        self.assertEqual(value.frames,31);self.assertEqual(len(value.pending),256)
        self.assertEqual(self.scored,[])

    def test_speech_then_quiet_scores_once_per_generation(self):
        value = self.analyzer();self.probability=.8
        self.assertTrue(value.feed(b'\1\0'*1536)['speech_seen'])
        self.probability=0
        self.assertNotIn('endpoint', value.feed(b'\0'*6144))
        result=value.feed(b'\0'*1024)
        self.assertEqual(result['endpoint']['generation'],1)
        self.assertEqual(result['endpoint']['audio_bytes'],10240)
        self.assertEqual(result['endpoint']['authority'],'analysis_only')
        self.assertNotIn('endpoint',value.feed(b'\0'*8192));self.assertEqual(len(self.scored),1)
        self.probability=.8;value.feed(b'\0'*3072);self.probability=0
        self.assertEqual(value.feed(b'\0'*7168)['endpoint']['generation'],2)

    def test_short_noise_never_establishes_speech_and_bounds_are_enforced(self):
        value=self.analyzer();self.probability=.9;value.feed(b'\0'*2048)
        self.probability=0;self.assertNotIn('endpoint',value.feed(b'\0'*8192))
        self.assertEqual(self.scored,[])
        for data in [b'',b'\0',b'\0'*32002,'not PCM']:
            with self.assertRaises(ValueError):value.feed(data)
        value.bytes=120*32000
        with self.assertRaises(ValueError):value.feed(b'\0'*1024)
        value.bytes=0;value.speech_seen=True;value.generation=100;value.analyses=30
        with self.assertRaises(TurnEndUnavailable):value.feed(b'\0'*1024)

    def test_startup_backfill_defers_scoring_until_the_current_tail(self):
        value=self.analyzer();self.probability=.8;value.feed(b'\0'*3072,analyze=False)
        self.probability=0
        self.assertNotIn('endpoint',value.feed(b'\0'*8192,analyze=False))
        self.assertEqual(self.scored,[])
        result=value.feed(b'\0'*1024)
        self.assertEqual(result['endpoint']['audio_bytes'],12288)
        self.assertEqual(len(self.scored),1)
        with self.assertRaises(ValueError):value.feed(b'\0'*1024,analyze='yes')

    def test_wrong_model_is_rejected_before_optional_imports(self):
        with tempfile.TemporaryDirectory() as root:
            path=Path(root)/'wrong.onnx';path.write_bytes(b'wrong')
            with self.assertRaises(ValueError):Activity(path,path)

class CaptureAnalysisLifecycle(unittest.IsolatedAsyncioTestCase):
    def worker(self):
        class Worker:
            def __init__(self):self.feeds=[];self.closed=False;self.queue=asyncio.Queue()
            async def feed(self,pcm,*,defer=False):
                self.feeds.append((len(pcm),defer));await self.queue.put({'type':'activity','audio_bytes':sum(n for n,_ in self.feeds)})
            async def receive(self):return await self.queue.get()
            async def close(self):self.closed=True
        return Worker()
    async def test_late_open_backfills_once_without_obsolete_endpoint_scores(self):
        worker=self.worker();pcm=bytearray(b'\0'*64000);changed=asyncio.Event();observed=[];complete=asyncio.Event()
        async def open_worker():return worker
        async def observe(value):
            observed.append(value)
            if value['audio_bytes']==64000:complete.set()
        task=asyncio.create_task(analyze_capture(open_worker,pcm,changed,observe))
        await asyncio.wait_for(complete.wait(),1)
        self.assertEqual(worker.feeds,[(32000,True),(32000,False)])
        pcm.extend(b'\0'*1024);changed.set();await asyncio.sleep(.01)
        self.assertEqual(worker.feeds[-1],(1024,False))
        task.cancel()
        with self.assertRaises(asyncio.CancelledError):await task
        self.assertTrue(worker.closed)
    async def test_late_first_join_keeps_current_tail_and_absolute_endpoint_offsets(self):
        worker=self.worker();pcm=bytearray(b'\0'*320000);changed=asyncio.Event();observed=[];complete=asyncio.Event()
        async def open_worker():return worker
        async def observe(value):
            observed.append(value)
            if value['audio_bytes']==len(pcm):complete.set()
        task=asyncio.create_task(analyze_capture(open_worker,pcm,changed,observe))
        await asyncio.wait_for(complete.wait(),1)
        self.assertEqual(sum(n for n,_ in worker.feeds),256000)
        self.assertEqual(bytes(pcm),b'\0'*320000)
        await worker.queue.put({'type':'activity','audio_bytes':256000,
            'endpoint':{'audio_bytes':256000,'generation':1}})
        await asyncio.sleep(.01)
        self.assertEqual(observed[-1]['endpoint']['audio_bytes'],320000)
        pcm.extend(b'\0'*1024);changed.set();await asyncio.sleep(.01)
        self.assertEqual(observed[-1]['audio_bytes'],321024)
        task.cancel()
        with self.assertRaises(asyncio.CancelledError):await task
        self.assertTrue(worker.closed)
    async def test_steady_state_lag_failure_closes_only_the_analysis_worker(self):
        worker=self.worker()
        async def open_worker():return worker
        current=asyncio.Event()
        async def observe(_):current.set()
        pcm=bytearray(b'\0'*1024);changed=asyncio.Event()
        task=asyncio.create_task(analyze_capture(open_worker,pcm,changed,observe))
        await asyncio.wait_for(current.wait(),1)
        pcm.extend(b'\0'*256002);changed.set()
        with self.assertRaisesRegex(DetectionUnavailable,'fell behind'):
            await task
        self.assertTrue(worker.closed);self.assertEqual(worker.feeds,[(1024,False)])

if __name__ == '__main__':unittest.main()
