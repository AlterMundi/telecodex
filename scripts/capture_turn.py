"""Finite PCM activity/turn-analysis worker; never transcribes or admits input.

Silero's documented ONNX state interface is adapted from the MIT-licensed
snakers4/silero-vad repository; see docs/silero-vad-LICENSE.txt.
"""
import argparse
import base64
import hashlib
import json
from pathlib import Path
import resource
import signal
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from scripts.turn_end import MODEL_SHA256, SmartTurn, TurnEndUnavailable

VAD_SHA256 = '7ed98ddbad84ccac4cd0aeb3099049280713df825c610a8ed34543318f1b2c49'
VAD_BYTES = 1289603
SCHEMA = 'telecodex.turn-activity/v1'


class Activity:
    """One capture's recurrent state; at most120s PCM and thirty turn scores."""
    def __init__(self, vad_path, turn_path):
        with Path(vad_path).open('rb') as source:
            model = source.read(VAD_BYTES + 1)
        if len(model) != VAD_BYTES or hashlib.sha256(model).hexdigest() != VAD_SHA256:
            raise ValueError('Expected the pinned Silero16k ONNX model')
        import numpy as np
        import onnxruntime as ort
        ort.disable_telemetry_events()
        options = ort.SessionOptions()
        options.inter_op_num_threads = options.intra_op_num_threads = 1
        options.execution_mode = ort.ExecutionMode.ORT_SEQUENTIAL
        self._session = ort.InferenceSession(model, sess_options=options, providers=['CPUExecutionProvider'])
        self._np = np
        self._state = np.zeros((2, 1, 128), dtype=np.float32)
        self._context = np.zeros((1, 64), dtype=np.float32)
        self._rate = np.array(16000, dtype=np.int64)
        self.turn = SmartTurn(turn_path)
        self.pending = bytearray()
        self.tail = bytearray()
        self.bytes = self.frames = self.generation = self.positive = self.quiet = self.analyses = 0
        self.speaking = self.speech_seen = False
        self.scored_generation = 0

    def _probability(self, frame):
        np = self._np
        audio = np.frombuffer(frame, dtype='<i2').astype(np.float32).reshape(1, 512) / 32768
        joined = np.concatenate((self._context, audio), axis=1)
        output, self._state = self._session.run(None, {'input': joined, 'state': self._state, 'sr': self._rate})
        self._context = joined[:, -64:]
        probability = float(output.reshape(-1)[0])
        if not 0 <= probability <= 1:
            raise ValueError('Invalid speech probability')
        return probability

    def feed(self, data, *, analyze=True):
        if type(analyze) is not bool:
            raise ValueError("Analysis selection must be boolean")
        if not isinstance(data, bytes) or not data or len(data) % 2 or len(data) > 32000 or self.bytes + len(data) > 120 * 32000:
            raise ValueError('PCM chunk outside capture bounds')
        self.bytes += len(data)
        self.pending.extend(data)
        self.tail.extend(data)
        del self.tail[:-256000]
        while len(self.pending) >= 1024:
            frame = bytes(self.pending[:1024]);del self.pending[:1024]
            self.frames += 1
            if self._probability(frame) >= .5:
                if not self.speaking:
                    self.generation += 1
                self.speaking = True
                self.positive += 1
                self.quiet = 0
                if self.positive >= 3:
                    self.speech_seen = True
            else:
                self.positive = 0
                self.quiet += 1
                if self.quiet >= 7:
                    self.speaking = False
        result = {'schema': SCHEMA, 'type': 'activity', 'generation': self.generation,
                  'speaking': self.speaking, 'speech_seen': self.speech_seen, 'audio_bytes': self.bytes}
        if analyze and self.speech_seen and not self.speaking and self.generation != self.scored_generation:
            if self.analyses >= 30:
                raise TurnEndUnavailable('Turn-analysis capture budget reached')
            self.scored_generation = self.generation
            self.analyses += 1
            score = self.turn.predict(bytes(self.tail), speech_present=True)
            if score['candidate_complete']:
                result['endpoint'] = {'generation': self.generation, 'audio_bytes': self.bytes,
                                      'probability': score['probability'], 'authority': 'analysis_only'}
        return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--vad-model', type=Path, required=True)
    parser.add_argument('--turn-model', type=Path, required=True)
    args = parser.parse_args()
    # This owns only the optional analysis child, never native conversations.
    resource.setrlimit(resource.RLIMIT_CPU, (60, 65))
    signal.alarm(145)
    try:
        analyzer = Activity(args.vad_model, args.turn_model)
        print(json.dumps({'schema': SCHEMA, 'type': 'ready', 'vad_sha256': VAD_SHA256, 'turn_sha256': MODEL_SHA256}), flush=True)
        while True:
            raw = sys.stdin.buffer.readline(45001)
            if not raw:
                break
            if len(raw) > 45000:
                raise ValueError('Analysis command exceeds bound')
            value = json.loads(raw)
            if (not isinstance(value, dict) or set(value) not in ({'pcm'}, {'pcm', 'defer'})
                    or not isinstance(value['pcm'], str) or type(value.get('defer', False)) is not bool):
                raise ValueError('Expected one PCM command')
            data = base64.b64decode(value['pcm'], validate=True)
            print(json.dumps(analyzer.feed(data, analyze=not value.get('defer', False))), flush=True)
    except (ValueError, OSError, TurnEndUnavailable, ImportError):
        print(json.dumps({'schema': SCHEMA, 'type': 'unavailable'}), flush=True)
        return 1
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
