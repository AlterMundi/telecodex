#!/usr/bin/env python3
"""Actual HTTP/filesystem/process contracts; all voices and credentials are fake."""
import concurrent.futures
import hashlib
import http.server
import importlib.util
import io
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import threading
import unittest
from unittest import mock
import wave

SCRIPT = Path(__file__).resolve().parents[1] / 'scripts/speech_render.py'
spec = importlib.util.spec_from_file_location('speech_render', SCRIPT)
speech = importlib.util.module_from_spec(spec)
spec.loader.exec_module(speech)


def wav():
    stream = io.BytesIO()
    with wave.open(stream, 'wb') as output:
        output.setparams((1, 2, 22050, 0, 'NONE', 'not compressed'))
        output.writeframes(b'\x01\x00' * 2205)
    return stream.getvalue()


class Render(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix='telecodex-speech-')
        self.root = Path(self.temporary.name)
        self.calls = []
        self.status = 200
        self.content_type = 'audio/wav'
        self.declared_size = None
        self.data = wav()
        self.hold = False
        self.synced = []
        self.synced_on_request = []
        self.entered, self.release = threading.Event(), threading.Event()
        owner = self

        class Handler(http.server.BaseHTTPRequestHandler):
            def do_POST(self):
                body = self.rfile.read(int(self.headers['Content-Length']))
                owner.calls.append((self.path, self.headers.get('Authorization'), body))
                owner.synced_on_request = list(owner.synced)
                owner.entered.set()
                if owner.hold:
                    owner.release.wait(timeout=5)
                self.send_response(owner.status, 'synthetic-secret')
                self.send_header('Content-Type', owner.content_type)
                self.send_header('Content-Length', str(owner.declared_size if owner.declared_size is not None else len(owner.data)))
                if owner.status == 302:
                    self.send_header('Location', '/must-not-follow')
                self.end_headers()
                try:
                    self.wfile.write(owner.data)
                except (ConnectionError, OSError):
                    pass

            def log_message(self, *_):
                pass

        self.server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Handler)
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()
        self.environment = self.root / 'credentials.env'
        self.environment.write_text('NVIDIA_API_KEY=synthetic-secret\n')
        self.environment.chmod(0o600)
        self.binding = self.root / 'route.json'
        self.config = {'schema': speech.SCHEMA, 'provider': 'nvidia-magpie',
                       'cache_root': str(self.root / 'cache'), 'env_file': str(self.environment),
                       'voice': 'Magpie-Multilingual.ES-US.Isabela', 'language': 'es-US',
                       'endpoint': f'http://127.0.0.1:{self.server.server_port}/synthesize',
                       'timeout_seconds': 2}
        self.binding.write_text(json.dumps(self.config))
        self.binding.chmod(0o600)
        self.request = {'scope': 'synthetic-human-100', 'text': 'Alfa sigue trabajando; Beta respondió.'}

    def tearDown(self):
        self.release.set()
        self.server.shutdown()
        self.server.server_close()
        self.thread.join(timeout=2)
        self.temporary.cleanup()

    def render(self, request=None):
        return speech.render(self.binding, self.request if request is None else request)

    def test_success_restart_cache_scope_and_no_playback_claim(self):
        first = self.render()
        self.assertFalse(first['cache_hit'])
        self.assertEqual(first['text_sha256'], hashlib.sha256(self.request['text'].encode()).hexdigest())
        self.assertEqual(first['audio_sha256'], hashlib.sha256(self.data).hexdigest())
        self.assertEqual(first['playback'], 'not_observed')
        self.assertEqual(first['format']['audio_millis'], 100)
        # A separate helper process recovers completed synthesis without another request.
        run = subprocess.run([sys.executable, str(SCRIPT), '--binding', str(self.binding)],
                             input=json.dumps(self.request), text=True, capture_output=True, timeout=5)
        self.assertEqual(run.returncode, 0, run.stdout)
        self.assertTrue(json.loads(run.stdout)['cache_hit'])
        self.assertEqual(len(self.calls), 1)
        self.assertIn(self.request['text'].encode(), self.calls[0][2])
        self.assertEqual(self.calls[0][1], 'Bearer synthetic-secret')
        other = self.render({**self.request, 'scope': 'another-authorized-receiving-scope'})
        self.assertNotEqual(other['render_id'], first['render_id'])
        self.assertEqual(len(self.calls), 2)
        for path in (self.root / 'cache').rglob('state.json'):
            self.assertNotIn('synthetic-secret', path.read_text())
            self.assertNotIn(self.request['text'], path.read_text())

    def test_duplicate_while_preparing_makes_one_provider_call(self):
        self.hold = True
        with concurrent.futures.ThreadPoolExecutor(max_workers=1) as pool:
            first = pool.submit(self.render)
            self.assertTrue(self.entered.wait(timeout=3))
            with self.assertRaisesRegex(speech.RenderError, 'render-in-progress'):
                self.render()
            self.release.set()
            first.result(timeout=5)
        self.assertEqual(len(self.calls), 1)

    def test_rejected_or_redirected_request_is_retained_without_secret_or_retry(self):
        for status in (503, 302):
            with self.subTest(status=status):
                self.status = status
                request = {**self.request, 'scope': str(status)}
                before = len(self.calls)
                run = subprocess.run([sys.executable, str(SCRIPT), '--binding', str(self.binding)],
                                     input=json.dumps(request), text=True, capture_output=True, timeout=5)
                self.assertEqual(run.returncode, 1)
                self.assertNotIn('synthetic-secret', run.stdout + run.stderr)
                self.assertEqual(json.loads(run.stdout)['automatic_retry'], False)
                with self.assertRaisesRegex(speech.RenderError, 'retained-render-outcome'):
                    self.render(request)
                self.assertEqual(len(self.calls), before + 1)
                self.assertEqual(self.calls[-1][0], '/synthesize')

    def test_incomplete_audio_is_uncertain_not_a_new_network_attempt(self):
        self.declared_size = len(self.data) + 20
        with self.assertRaisesRegex(speech.RenderError, 'unconfirmed'):
            self.render()
        with self.assertRaisesRegex(speech.RenderError, 'retained-render-outcome'):
            self.render()
        self.assertEqual(len(self.calls), 1)
        states = list((self.root / 'cache').rglob('state.json'))
        self.assertEqual(json.loads(states[0].read_text())['phase'], 'unconfirmed')
        self.assertFalse(list((self.root / 'cache').rglob('audio.wav')))

    def test_killed_request_retains_intent_without_reissuing(self):
        self.hold = True
        child = subprocess.Popen([sys.executable, str(SCRIPT), '--binding', str(self.binding)],
                                 stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                 text=True)
        try:
            child.stdin.write(json.dumps(self.request))
            child.stdin.close()
            self.assertTrue(self.entered.wait(timeout=3))
            child.kill()
            child.wait(timeout=3)
            state = next((self.root / 'cache').rglob('state.json'))
            self.assertEqual(json.loads(state.read_text())['phase'], 'intent')
            with self.assertRaisesRegex(speech.RenderError, 'retained-render-outcome'):
                self.render()
            self.assertEqual(len(self.calls), 1)
        finally:
            if child.poll() is None:
                child.kill()
                child.wait(timeout=3)
            child.stdout.close()
            child.stderr.close()
            self.release.set()

    def test_lost_state_does_not_replace_existing_audio(self):
        result = self.render()
        state = Path(result['audio_path']).with_name('state.json')
        state.unlink()
        with self.assertRaisesRegex(speech.RenderError, 'orphan-render'):
            self.render()
        self.assertEqual(len(self.calls), 1)

    def test_directory_names_and_intent_are_synced_before_provider_request(self):
        cache = self.root / 'new-parent' / 'new-cache'
        self.config['cache_root'] = str(cache)
        self.binding.write_text(json.dumps(self.config))
        actual_sync = speech.sync_directory

        def observed_sync(path):
            actual_sync(path)
            self.synced.append(Path(path))

        with mock.patch.object(speech, 'sync_directory', side_effect=observed_sync):
            result = self.render()
        job = Path(result['audio_path']).parent
        # These are completed real fsyncs observed before HTTP, not after response.
        self.assertTrue({self.root, cache.parent, cache, job}.issubset(self.synced_on_request))

    def test_cached_audio_tamper_or_symlink_never_regenerates(self):
        result = self.render()
        audio = Path(result['audio_path'])
        audio.write_bytes(b'corrupt')
        with self.assertRaises(speech.RenderError):
            self.render()
        audio.unlink()
        target = self.root / 'other.wav'
        target.write_bytes(self.data)
        target.chmod(0o600)
        audio.symlink_to(target)
        with self.assertRaises(OSError):
            self.render()
        self.assertEqual(len(self.calls), 1)

    def test_input_and_header_limits_precede_audio_publication(self):
        for request in ({'scope': 'x', 'text': 'x' * 1801},
                        {'scope': '', 'text': 'hello'},
                        {**self.request, 'endpoint': 'model-authored'}):
            with self.assertRaises(speech.RenderError):
                self.render(request)
        self.assertEqual(self.calls, [])
        self.declared_size = speech.MAX_AUDIO + 1
        with self.assertRaisesRegex(speech.RenderError, 'unconfirmed'):
            self.render()
        self.assertEqual(len(self.calls), 1)
        self.assertFalse(list((self.root / 'cache').rglob('audio.wav')))


if __name__ == '__main__':
    unittest.main()
