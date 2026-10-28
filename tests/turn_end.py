#!/usr/bin/env python3
"""Turn-score window, refusal and concurrency contracts; no weights or models."""
import importlib.util
import math
from pathlib import Path
import tempfile
import threading
import time
import unittest
import wave

spec=importlib.util.spec_from_file_location('turn_end',Path(__file__).resolve().parents[1]/'scripts/turn_end.py')
module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)

class TurnEnd(unittest.TestCase):
    def analyzer(self,score=lambda pcm,options:.8,timeout=.2):
        value=object.__new__(module.SmartTurn);value._lock=threading.Lock();value._score=score;value._options=lambda:type('Options',(),{'terminate':False})();value.timeout=timeout;return value

    def test_speech_is_required_and_score_is_only_analysis(self):
        calls=[];value=self.analyzer(lambda pcm,options:calls.append(pcm) or .9)
        self.assertFalse(value.predict(b'\0'*1600,speech_present=False)['candidate_complete']);self.assertFalse(calls)
        result=value.predict(b'\0'*1600,speech_present=True)
        self.assertEqual(result['authority'],'analysis_only');self.assertTrue(result['candidate_complete']);self.assertEqual(len(calls),1)
        for pcm in [b'\0'*1598,b'\0'*1601,b'\0'*(module.WINDOW_BYTES+2),'not bytes']:
            with self.assertRaises(ValueError):value.predict(pcm,speech_present=True)
        with self.assertRaises(ValueError):value.predict(b'\0'*1600,speech_present=1)

    def test_busy_does_not_queue_and_lock_is_released(self):
        entered=threading.Event();release=threading.Event();results=[]
        def score(pcm,options):entered.set();release.wait(.5);return .8
        value=self.analyzer(score,timeout=1)
        worker=threading.Thread(target=lambda:results.append(value.predict(b'\0'*1600,speech_present=True)));worker.start();self.assertTrue(entered.wait(.5))
        with self.assertRaises(module.TurnEndUnavailable):value.predict(b'\0'*1600,speech_present=True)
        release.set();worker.join(1);self.assertFalse(worker.is_alive());self.assertEqual(len(results),1)
        self.assertTrue(value.predict(b'\0'*1600,speech_present=True)['candidate_complete'])

    def test_late_and_invalid_scores_are_not_returned(self):
        terminated=[]
        def delayed(pcm,options):time.sleep(.03);terminated.append(options.terminate);return .99
        value=self.analyzer(delayed,timeout=.01)
        with self.assertRaises(module.TurnEndUnavailable):value.predict(b'\0'*1600,speech_present=True)
        self.assertEqual(terminated,[True]);self.assertFalse(value._lock.locked())
        for probability in [math.nan,math.inf,-.1,1.1]:
            with self.assertRaises(module.TurnEndUnavailable):self.analyzer(lambda pcm,options:probability).predict(b'\0'*1600,speech_present=True)

    def test_wrong_model_is_rejected_without_optional_dependencies(self):
        with tempfile.TemporaryDirectory() as root:
            path=Path(root)/'model.onnx';path.write_bytes(b'not the selected model')
            with self.assertRaises(ValueError):module.SmartTurn(path)
            with self.assertRaises(ValueError):module.SmartTurn(path,timeout=0)

    def test_wav_tail_and_malformed_audio(self):
        with tempfile.TemporaryDirectory() as root:
            path=Path(root)/'audio.wav';pcm=b'\x01\x00'*(2*16000)+b'\x02\x00'*(8*16000)
            with wave.open(str(path),'wb') as out:out.setparams((1,2,16000,0,'NONE','not compressed'));out.writeframes(pcm)
            self.assertEqual(module.read_pcm(path),pcm[-module.WINDOW_BYTES:])
            path.write_bytes(path.read_bytes()[:-2])
            with self.assertRaises(ValueError):module.read_pcm(path)
            with wave.open(str(path),'wb') as out:out.setparams((2,2,16000,0,'NONE','not compressed'));out.writeframes(b'\0'*3200)
            with self.assertRaises(ValueError):module.read_pcm(path)

if __name__=='__main__':unittest.main()
