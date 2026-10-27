#!/usr/bin/env python3
"""Actual HTTP/filesystem/process contracts; all voices and credentials are fake."""
import concurrent.futures
import base64
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
import time
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
        self.stream_hold = False
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
                    if owner.stream_hold:
                        boundary=owner.data.index(b'\n\n')+2
                        self.wfile.write(owner.data[:boundary]);self.wfile.flush()
                        owner.release.wait(timeout=5)
                        self.wfile.write(owner.data[boundary:])
                    else:
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

    def qwen_route(self):
        self.environment.write_text('synthetic-qwen-key\n')
        self.config.update(provider='qwen-tts', model='qwen-audio-3.0-tts-plus',
                           voice='longanlingxin', credential_file=str(self.environment),
                           instruction='Warm and natural; preserve Spanish and English.')
        del self.config['env_file']
        self.binding.write_text(json.dumps(self.config))

    def test_markdown_projection_preserves_labels_code_and_canonical_digest(self):
        original = '**Real Time**: [Android](https://example.org/path). Use `git status`. 🎸'
        request = {**self.request, 'text': original}
        result = self.render(request)
        self.assertIn('Real Time: Android. Use git status. 🎸'.encode(), self.calls[0][2])
        self.assertNotIn(b'**Real Time**', self.calls[0][2])
        self.assertNotIn(b'https://example.org', self.calls[0][2])
        self.assertEqual(result['text_sha256'], hashlib.sha256(original.encode()).hexdigest())
        self.assertTrue(self.render(request)['cache_hit'])
        self.assertEqual(len(self.calls), 1)
        # Identical spoken text with different original markup is not a cache
        # collision against the source digest or an invented original.
        plain = {**request, 'text': 'Real Time: Android. Use git status. 🎸'}
        self.assertNotEqual(self.render(plain)['render_id'], result['render_id'])
        self.assertEqual(speech.spoken_markdown('<https://example.org/a>', 'es-US'), 'enlace')
        self.assertEqual(speech.spoken_markdown('<https://example.org/a>', 'en-US'), 'link')
        self.assertEqual(speech.spoken_markdown('![Diagram](https://example.org/image)', 'en-US'), 'Diagram')
        self.assertEqual(speech.spoken_markdown('```sh\ngit status --short\n```', 'en-US'), 'git status --short')
        self.assertEqual(speech.spoken_markdown('2 * 3 = 6', 'en-US'), '2 * 3 = 6')

    def test_spoken_topic_labels_preserve_code_and_issue_references(self):
        self.assertEqual(speech.spoken_markdown('Novedades de **#Realtime** y #H.B. Guitar.', 'es-US'),
                         'Novedades de Realtime y H.B. Guitar.')
        self.assertEqual(speech.spoken_markdown('Use C#; see #123 and `#Realtime`.', 'en-US'),
                         'Use C#; see #123 and #Realtime.')

    def test_streamed_markdown_uses_label_not_url_and_retains_source_hash(self):
        self.qwen_route();self.content_type='text/event-stream';self.data=self.stream_data();frames=[]
        original='Read **Real Time** and [the source](https://example.org/).'
        speech.stream_render(self.binding,{**self.request,'text':original},frames.append)
        self.assertEqual(json.loads(self.calls[0][2])['input']['text'],'Read Real Time and the source.')
        state=json.loads(next((self.root/'cache').glob('*/state.json')).read_text())
        self.assertEqual(state['text_sha256'],hashlib.sha256(original.encode()).hexdigest())

    def test_qwen_http_identity_download_and_cache(self):
        self.qwen_route()
        url = 'https://speech.oss-ap-southeast-1.aliyuncs.com/sample.wav?signature=fake'
        self.content_type = 'application/json'
        self.data = json.dumps({'output': {'audio': {'url': url.replace('https:', 'http:')}}}).encode()
        original = speech.fetch
        downloads = []
        audio = io.BytesIO()
        with wave.open(audio, 'wb') as w:
            w.setparams((1, 2, 24000, 0, 'NONE', 'not compressed'))
            w.writeframes(b'\x01\x00' * 2400)
        def fetch(request, timeout, **kw):
            if request.full_url == url:
                downloads.append(request)
                return audio.getvalue()
            return original(request, timeout, **kw)
        with mock.patch.object(speech, 'fetch', side_effect=fetch):
            first = self.render()
            self.assertTrue(self.render()['cache_hit'])
            self.assertEqual(first['format']['sample_rate_hz'], 24000)
            self.assertEqual(len(self.calls), 1)
            self.assertEqual(self.calls[0][1], 'Bearer synthetic-qwen-key')
            body = json.loads(self.calls[0][2])
            self.assertEqual(body['input']['text'], self.request['text'])
            self.assertEqual(body['model'], self.config['model'])
            self.assertIsNone(downloads[0].get_header('Authorization'))
            self.config['instruction'] = 'A different style'
            self.binding.write_text(json.dumps(self.config))
            self.assertNotEqual(self.render()['render_id'], first['render_id'])

    def test_qwen_rejects_untrusted_download_and_preserves_uncertainty(self):
        self.qwen_route()
        self.content_type = 'application/json'
        self.data = json.dumps({'output': {'audio': {'url': 'http://127.0.0.1/private'}}}).encode()
        with self.assertRaisesRegex(speech.RenderError, 'unconfirmed'):
            self.render()
        with self.assertRaisesRegex(speech.RenderError, 'retained-render-outcome'):
            self.render()
        self.assertEqual(len(self.calls), 1)

    def test_qwen_requires_explicit_endpoint_and_normalizes_only_known_stream_header(self):
        self.qwen_route()
        del self.config['endpoint']
        self.binding.write_text(json.dumps(self.config))
        with self.assertRaisesRegex(speech.RenderError, 'endpoint'):
            self.render()
        self.assertEqual(len(self.calls), 0)
        import struct
        data = (b'RIFF\xbf\xff\xff\x7fWAVEfmt \x10\x00\x00\x00'
                + struct.pack('<HHIIHH', 1, 1, 24000, 48000, 2, 16)
                + b'data\x9b\xff\xff\x7f' + b'\x00\x00' * 2400)
        self.assertEqual(speech.audio_info(speech.qwen_wav(data))['audio_millis'], 100)
        with self.assertRaises(speech.RenderError):
            speech.qwen_wav(data[:-1])
        corrupt = data[:40] + struct.pack('<I', 5000) + data[44:]
        with self.assertRaises(speech.RenderError):
            speech.qwen_wav(corrupt)

    def age_audio(self, result, seconds):
        path=Path(result['audio_path']).parent/'state.json'
        state=json.loads(path.read_text());state['last_used_at']=time.time()-seconds
        speech.atomic(path,speech.canonical(state))
        return path

    def test_expired_completed_audio_keeps_receipt_and_renders_only_on_request(self):
        first=self.render();path=self.age_audio(first,7200)
        removed=speech.prune_audio(self.config)
        self.assertEqual(removed['removed_audio_bytes'],len(self.data))
        self.assertFalse(Path(first['audio_path']).exists())
        state=json.loads(path.read_text())
        self.assertEqual(state['phase'],'audio_expired')
        self.assertEqual(state['text_sha256'],first['text_sha256'])
        self.assertEqual(len(self.calls),1,'Maintenance cannot call the provider')
        second=self.render()
        self.assertFalse(second['cache_hit'])
        self.assertEqual(json.loads(path.read_text())['generation'],2)
        self.assertTrue(self.render()['cache_hit'])
        self.assertEqual(len(self.calls),2,'Only the selected render request recreates audio')

    def test_audio_retention_never_deletes_busy_or_uncertain_work(self):
        first=self.render();path=self.age_audio(first,7200)
        with speech.lock(path.parent/'.lock'):
            self.assertEqual(speech.prune_audio(self.config)['removed_audio_bytes'],0)
        state=json.loads(path.read_text());state['phase']='unconfirmed'
        speech.atomic(path,speech.canonical(state))
        self.assertEqual(speech.prune_audio(self.config)['removed_audio_bytes'],0)
        self.assertTrue(Path(first['audio_path']).exists())
        with self.assertRaisesRegex(speech.RenderError,'retained-render-outcome'):
            self.render()
        self.assertEqual(len(self.calls),1)

    def test_audio_budget_expires_oldest_completed_payload_before_age_limit(self):
        stream=io.BytesIO()
        with wave.open(stream,'wb') as output:
            output.setparams((1,2,22050,0,'NONE','not compressed'))
            output.writeframes(b'\x01\x00'*(22050*100))
        self.data=stream.getvalue()
        first=self.render();self.age_audio(first,700)
        second=self.render({**self.request,'scope':'second-result'});self.age_audio(second,600)
        result=speech.prune_audio({**self.config,'audio_cache_max_bytes':speech.MAX_AUDIO})
        self.assertFalse(Path(first['audio_path']).exists())
        self.assertTrue(Path(second['audio_path']).exists())
        self.assertLessEqual(result['accounted_audio_bytes'],speech.MAX_AUDIO)
        self.assertEqual(len(self.calls),2)

    def test_audio_budget_refuses_new_synthesis_before_storage_can_grow(self):
        self.config['audio_cache_max_bytes']=speech.MAX_AUDIO
        self.binding.write_text(json.dumps(self.config))
        self.render()
        with self.assertRaisesRegex(speech.RenderError,'audio-cache-capacity'):
            self.render({**self.request,'scope':'new-part'})
        self.assertEqual(len(self.calls),1,'A full protected budget must not spend another provider call')

    def test_capacity_pruning_counts_protected_payloads_and_intent_reservations(self):
        stream=io.BytesIO()
        with wave.open(stream,'wb') as output:
            output.setparams((1,2,22050,0,'NONE','not compressed'))
            output.writeframes(b'\x01\x00'*(22050*100))
        self.data=stream.getvalue()
        protected=speech.directory(self.root/'cache'/('f'*64))
        for phase in ('unconfirmed','intent'):
            with self.subTest(phase=phase):
                first=self.render();self.age_audio(first,600)
                speech.atomic(protected/'state.json',speech.canonical({'phase':phase}))
                if phase=='unconfirmed':
                    speech.atomic(protected/'audio.wav',b'\x00'*speech.MAX_AUDIO)
                else:
                    (protected/'audio.wav').unlink()
                before=len(self.calls)
                # The next maximum-sized render needs8MiB of free capacity in
                # a16MiB budget; protected occupancy must trigger eligible GC.
                result=speech.prune_audio({**self.config,'audio_cache_max_bytes':speech.MAX_AUDIO})
                self.assertFalse(Path(first['audio_path']).exists())
                self.assertEqual(json.loads((protected/'state.json').read_text())['phase'],phase)
                self.assertEqual(result['accounted_audio_bytes'],speech.MAX_AUDIO)
                self.assertEqual(len(self.calls),before)

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
        # Budget accounting may reject the unsafe artifact before the cache's
        # O_NOFOLLOW read. Both paths must refuse it without regeneration.
        with self.assertRaises((OSError,speech.RenderError)):
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

    def stream_data(self, *, terminal=True):
        raw=b'ID3synthetic encoded audio'
        frames=[{'output':{'audio':{'data':base64.b64encode(raw).decode()}}}]
        if terminal:frames.append({'output':{'finish_reason':'stop'}})
        return b''.join(b'data: '+json.dumps(v).encode()+b'\n\n' for v in frames)

    def test_stream_intent_cache_integrity_and_retention(self):
        self.qwen_route();self.content_type='text/event-stream';self.data=self.stream_data();frames=[]
        speech.stream_render(self.binding,self.request,frames.append)
        self.assertEqual([v['type'] for v in frames],['audio','end'])
        self.assertEqual(frames[0]['sequence'],0)
        self.assertEqual(len(self.calls),1)
        audio=next((self.root/'cache').glob('*/audio.mp3'))
        self.assertEqual(speech.audio_occupancy(audio.parent.parent),audio.stat().st_size)
        again=[];speech.stream_render(self.binding,self.request,again.append)
        self.assertEqual(again,frames);self.assertEqual(len(self.calls),1)
        result=speech.prune_audio(speech.route(self.binding),now=time.time()+4000)
        self.assertGreater(result['removed_audio_bytes'],0);self.assertFalse(audio.exists())
        speech.stream_render(self.binding,self.request,lambda frame:None)
        self.assertEqual(len(self.calls),2,'Known expired media may be explicitly synthesized again')
        audio.write_bytes(b'changed')
        with self.assertRaisesRegex(speech.RenderError,'integrity'):
            speech.stream_render(self.binding,self.request,lambda frame:None)
        self.assertEqual(len(self.calls),2)

    def test_stream_uncertainty_and_disconnected_reader_never_retry(self):
        self.qwen_route();self.content_type='text/event-stream';self.data=self.stream_data(terminal=False)
        with self.assertRaisesRegex(speech.RenderError,'unconfirmed'):
            speech.stream_render(self.binding,self.request,lambda frame:None)
        with self.assertRaisesRegex(speech.RenderError,'no-automatic-retry'):
            speech.stream_render(self.binding,self.request,lambda frame:None)
        self.assertEqual(len(self.calls),1)
        self.data=self.stream_data()
        def disconnected(frame):raise BrokenPipeError()
        with self.assertRaisesRegex(speech.RenderError,'unconfirmed'):
            speech.stream_render(self.binding,{**self.request,'scope':'another-explicit-source'},disconnected)
        self.assertEqual(len(self.calls),2)

    def test_stream_parser_multiline_bounds_and_terminal(self):
        raw=self.stream_data().replace(b'data: {"output":',b'data: {\ndata: "output":')
        self.assertEqual(list(speech.sse_audio(io.BytesIO(raw))),[b'ID3synthetic encoded audio'])
        for data in (b'data: '+b'x'*(1024*1024+1),self.stream_data(terminal=False),b'data: {"output":{"audio":{"data":"!!"}}}\n\n'):
            with self.assertRaises((ValueError,speech.RenderError)):
                list(speech.sse_audio(io.BytesIO(data)))


if __name__ == '__main__':
    unittest.main()
