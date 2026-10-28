"""Speech-gated, bounded turn analysis; no models, provider or native input."""
import sys
from pathlib import Path
import tempfile
import unittest
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from scripts.capture_turn import Activity, TurnEndUnavailable


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

    def test_wrong_model_is_rejected_before_optional_imports(self):
        with tempfile.TemporaryDirectory() as root:
            path=Path(root)/'wrong.onnx';path.write_bytes(b'wrong')
            with self.assertRaises(ValueError):Activity(path,path)

if __name__ == '__main__':unittest.main()
