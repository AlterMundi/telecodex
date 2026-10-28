"""Optional CPU turn-end scoring. Analysis never commits or routes an utterance.

Uses Pipecat Smart Turn v3.2's documented Whisper feature extraction. The model
is separately obtained and checksum-pinned; ordinary app imports do not load it.
"""
import argparse
import hashlib
import json
import math
from pathlib import Path
import threading
import time
import wave

MODEL_SHA256 = '2bb026316b14a660486a75b1733cd3fbab8c2fd0314dc9af7be49f8cca967e4f'
MODEL_BYTES = 8679182
WINDOW_BYTES = 8 * 16000 * 2


class TurnEndUnavailable(Exception):
    """Keep explicit Send when a current, bounded score is unavailable."""


def read_pcm(path):
    """Read only the last eight seconds of a bounded canonical PCM WAV."""
    path = Path(path)
    if path.stat().st_size > 20 * 1024 * 1024:
        raise ValueError('Audio exceeds the recording bound')
    with wave.open(str(path)) as source:
        if (source.getnchannels(), source.getsampwidth(), source.getframerate(), source.getcomptype()) != (1, 2, 16000, 'NONE'):
            raise ValueError('Expected mono PCM16 at 16000 Hz')
        if not 800 <= source.getnframes() <= 120 * 16000:
            raise ValueError('Expected 0.05 to 120 seconds of audio')
        source.setpos(max(0, source.getnframes() - 8 * 16000))
        pcm = source.readframes(8 * 16000)
        if len(pcm) != min(source.getnframes(), 8 * 16000) * 2:
            raise ValueError('Incomplete WAV audio')
        return pcm


class SmartTurn:
    def __init__(self, model_path, *, timeout=.75):
        if not math.isfinite(timeout) or not .01 <= timeout <= 2:
            raise ValueError('Turn analysis timeout must be between 0.01 and 2 seconds')
        model = Path(model_path)
        with model.open('rb') as source:
            model_bytes = source.read(MODEL_BYTES + 1)
        if len(model_bytes) != MODEL_BYTES or hashlib.sha256(model_bytes).hexdigest() != MODEL_SHA256:
            raise ValueError('Expected the pinned Smart Turn v3.2 CPU model')
        import numpy as np
        import onnxruntime as ort
        from transformers import WhisperFeatureExtractor
        ort.disable_telemetry_events()
        options = ort.SessionOptions()
        options.execution_mode = ort.ExecutionMode.ORT_SEQUENTIAL
        options.intra_op_num_threads = options.inter_op_num_threads = 1
        options.graph_optimization_level = ort.GraphOptimizationLevel.ORT_ENABLE_ALL
        session = ort.InferenceSession(model_bytes, sess_options=options, providers=['CPUExecutionProvider'])
        extractor = WhisperFeatureExtractor(chunk_length=8)

        def score(pcm, run_options):
            audio = np.frombuffer(pcm, dtype='<i2').astype(np.float32) / 32768
            audio = np.pad(audio, (128000 - len(audio), 0))
            features = extractor(audio, sampling_rate=16000, return_tensors='np',
                padding='max_length', max_length=128000, truncation=True,
                do_normalize=True).input_features.astype(np.float32)
            return float(np.asarray(session.run(None, {'input_features': features}, run_options)[0]).reshape(-1)[0])

        self._score = score
        self._options = ort.RunOptions
        self._lock = threading.Lock()
        self.timeout = timeout

    def predict(self, pcm, *, speech_present):
        if not isinstance(pcm, bytes) or not 1600 <= len(pcm) <= WINDOW_BYTES or len(pcm) % 2:
            raise ValueError('Expected 0.05 to 8 seconds of mono PCM16')
        if type(speech_present) is not bool:
            raise ValueError('An explicit speech-activity observation is required')
        base = {'schema': 'telecodex.turn-end-score/v1', 'authority': 'analysis_only',
                'model_sha256': MODEL_SHA256, 'audio_bytes': len(pcm)}
        if not speech_present:
            return {**base, 'probability': None, 'candidate_complete': False,
                    'reason': 'speech_not_established'}
        if not self._lock.acquire(blocking=False):
            raise TurnEndUnavailable('Turn analyzer busy; no analysis queued')
        started = time.monotonic()
        options = self._options()
        expired = threading.Event()

        def expire():
            expired.set()
            options.terminate = True

        timer = threading.Timer(self.timeout, expire)
        timer.daemon = True
        try:
            timer.start()
            probability = self._score(pcm, options)
            elapsed = time.monotonic() - started
            if expired.is_set() or elapsed > self.timeout:
                raise TurnEndUnavailable('Turn analysis exceeded its deadline')
            if not math.isfinite(probability) or not 0 <= probability <= 1:
                raise TurnEndUnavailable('Invalid turn probability')
            return {**base, 'probability': probability, 'candidate_complete': probability > .5,
                    'elapsed_millis': elapsed * 1000}
        except TurnEndUnavailable:
            raise
        except Exception as error:
            if expired.is_set():
                raise TurnEndUnavailable('Turn analysis exceeded its deadline') from error
            raise TurnEndUnavailable('Turn analysis unavailable; keep explicit Send') from error
        finally:
            timer.cancel()
            self._lock.release()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--model', type=Path, required=True)
    parser.add_argument('--audio', type=Path, required=True)
    parser.add_argument('--speech-present', action='store_true', help='Analyze an already established speech segment; not a VAD replacement')
    args = parser.parse_args()
    pcm = read_pcm(args.audio)
    analyzer = SmartTurn(args.model)
    print(json.dumps(analyzer.predict(pcm, speech_present=args.speech_present)))


if __name__ == '__main__':
    main()
