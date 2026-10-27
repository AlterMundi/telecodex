#!/usr/bin/env python3
"""Durable transcript/disposable preparation contracts; no provider or sensor."""
import importlib.util
import json
import io
import uuid
import wave
import hashlib
import os
import stat
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT/'scripts'))
spec=importlib.util.spec_from_file_location('interaction_asr_adapter',ROOT/'scripts/interaction_asr.py')
asr=importlib.util.module_from_spec(spec);spec.loader.exec_module(asr)

HELPER='''import hashlib,json
from pathlib import Path

def process(source,binding):
    route=json.loads(Path(binding).read_text())
    root=Path(route['cache_root']);job=root/'source-identity'/'route-identity'
    job.mkdir(mode=0o700,parents=True,exist_ok=True)
    calls=Path(__file__).with_suffix('.calls')
    calls.write_text((calls.read_text() if calls.exists() else '')+'call\\n')
    if source.read_bytes()==b'failed operation':raise RuntimeError('simulated uncertain provider')
    media=job/'source.media';media.write_bytes(source.read_bytes())
    chunks=job/'chunks';chunks.mkdir(mode=0o700,exist_ok=True)
    chunk=chunks/'00000.wav';chunk.write_bytes(b'prepared pcm')
    digest=lambda path:hashlib.sha256(path.read_bytes()).hexdigest()
    (job/'manifest.json').write_text(json.dumps({'sources':[{'path':str(media),'sha256':digest(media)}],
        'chunks':[{'file':'chunks/00000.wav','sha256':digest(chunk)}]}))
    output=job/'responses'/'request';output.mkdir(mode=0o700,parents=True,exist_ok=True)
    draft=output/'draft.json';draft.write_text(json.dumps({'processing_complete':True,'segments':[{'text':'Final words'}]}))
    return {'text':'Final words','segments':[{'text':'Final words'}],
        'received_duration_seconds':1,'review_status':'unreviewed','draft_path':str(draft)}
'''

class Retention(unittest.TestCase):
    def setUp(self):
        self.previous_umask=os.umask(0o077)
        self.temp=tempfile.TemporaryDirectory();self.root=Path(self.temp.name)
        self.source=self.root/'original.wav';self.source.write_bytes(b'original private audio')
        self.helper=self.root/'voice_note.py';self.helper.write_text(HELPER)
        self.route=self.root/'route.json';self.route.write_text(json.dumps({'schema':'audio-transcription.route/v1','provider':'fictional'}))
        self.binding=self.root/'adapter.json'
        self.cfg={'schema':'telecodex.interaction-asr/v1','helper':str(self.helper),
            'route_binding':str(self.route),'cache_root':str(self.root/'cache')}
        self.write_config()
        self.probe=patch.object(asr,'probe_duration',return_value=(1,None));self.probe.start()
    def tearDown(self):self.probe.stop();self.temp.cleanup();os.umask(self.previous_umask)
    def write_config(self):self.binding.write_text(json.dumps(self.cfg))
    def calls(self):
        path=self.helper.with_suffix('.calls');return len(path.read_text().splitlines()) if path.exists() else 0
    def run_asr(self):return asr.process(self.source,self.binding)
    def streaming_fixture(self):
        self.source=self.root/str(uuid.uuid4())
        pcm=b'\0\0'*1600
        buffer=io.BytesIO()
        with wave.open(buffer,'wb') as audio:
            audio.setnchannels(1);audio.setsampwidth(2);audio.setframerate(16000);audio.writeframes(pcm)
        self.source.write_bytes(buffer.getvalue())
        root=self.root/'streamed';root.mkdir(mode=0o700)
        self.cfg['streaming_cache_root']=str(root);self.write_config()
        value={'schema':'telecodex.streaming-transcript/v1','capture_id':self.source.name,
            'binding':'b'*64,'source_sha256':hashlib.sha256(self.source.read_bytes()).hexdigest(),
            'pcm_sha256':hashlib.sha256(pcm).hexdigest(),'audio_millis':100,
            'model':'qwen-audio-3.0-realtime-plus','text':'Live final words','final':True}
        record=root/(self.source.name+'.json');record.write_text(json.dumps(value))
        return record,value
    def test_exact_completed_stream_skips_file_provider_and_retains_recovery(self):
        record,_=self.streaming_fixture()
        result=asr.process(self.source,self.binding,include_retention=True)
        self.assertEqual(result,{'text':'Live final words','final':True,'preparation_audio_deleted':True,'streaming_model':'qwen-audio-3.0-realtime-plus'})
        self.assertEqual(self.calls(),0)
        record.unlink()
        self.assertEqual(asr.process(self.source,self.binding,completed_only=True)['text'],'Live final words')
        self.assertEqual(self.calls(),0)
    def identity_job(self):
        identity={'schema':'telecodex.interaction-asr-result/v1',
            'source_sha256':asr.digest(self.source.read_bytes()),
            'route_sha256':asr.digest(self.route.read_bytes()),'helper_sha256':asr.digest(self.helper.read_bytes())}
        return self.root/'cache'/asr.digest(asr.canonical(identity))
    def full_prepared_cache(self):
        root=self.root/'streamed';root.mkdir(mode=0o700,exist_ok=True)
        with (root/'retained.json').open('wb') as stream:stream.truncate(asr.PREPARED_BUDGET)
        self.cfg['streaming_cache_root']=str(root);self.write_config();return root
    def test_prepared_commit_does_not_rescan_or_sync_file_asr_history(self):
        record,_=self.streaming_fixture();(self.root/'cache').mkdir(mode=0o700)
        directory=asr.directory
        def checked(path):
            self.assertFalse(Path(path).is_relative_to(self.root/'cache'),'Prepared text must not establish another file-ASR outcome')
            return directory(path)
        with patch.object(asr,'directory',side_effect=checked),patch.object(asr,'reserve',side_effect=AssertionError('Historical file-ASR scan')):
            self.assertEqual(self.run_asr()['text'],'Live final words')
        self.assertEqual(len(list((self.root/'streamed').glob('asr-*.json'))),1)
        self.assertFalse(list((self.root/'cache').glob('*/state.json')))
        record.unlink();self.assertEqual(asr.process(self.source,self.binding,completed_only=True)['text'],'Live final words')
        self.assertEqual(self.calls(),0)
    def test_full_prepared_cache_preserves_legacy_completion_and_uncertainty(self):
        self.run_asr();self.full_prepared_cache()
        self.assertEqual(asr.process(self.source,self.binding,completed_only=True)['text'],'Final words')
        self.assertEqual(self.calls(),1)
        state=self.identity_job()/'state.json';value=json.loads(state.read_text());value['phase']='unconfirmed';state.write_text(json.dumps(value))
        with self.assertRaisesRegex(asr.RenderError,'retained-asr-outcome-no-automatic-retry'):
            self.run_asr()
        self.assertEqual(self.calls(),1)
    def test_full_prepared_cache_does_not_disable_ordinary_file_fallback(self):
        root=self.full_prepared_cache();self.assertEqual(self.run_asr()['text'],'Final words')
        self.assertEqual(self.calls(),1);self.assertFalse(list(root.glob('asr-*.json')))
    def test_new_stream_completion_obeys_shared_prepared_quota(self):
        self.streaming_fixture();self.full_prepared_cache()
        with self.assertRaisesRegex(asr.RenderError,'prepared-transcription-storage-full'):
            self.run_asr()
        self.assertEqual(self.calls(),0);self.assertFalse(list((self.root/'streamed').glob('asr-*.json')))
    def test_late_old_helper_cannot_admit_file_asr_during_prepared_commit(self):
        self.streaming_fixture();original=asr.streaming_result;attempts=[]
        def late_writer(*args):
            job=self.identity_job()
            with self.assertRaisesRegex(asr.RenderError,'render-in-progress'):
                with asr.lock(job/'.lock'):attempts.append('old helper admitted')
            return original(*args)
        with patch.object(asr,'streaming_result',side_effect=late_writer):
            self.assertEqual(self.run_asr()['text'],'Live final words')
        self.assertEqual(attempts,[]);self.assertEqual(self.calls(),0)
        self.assertEqual(asr.process(self.source,self.binding,completed_only=True)['text'],'Live final words')
    def test_changed_flat_completion_is_refused_without_file_asr(self):
        self.streaming_fixture();self.run_asr()
        record=next((self.root/'streamed').glob('asr-*.json'));value=json.loads(record.read_text());value['result']['text']='Changed';record.write_text(json.dumps(value))
        with self.assertRaisesRegex(asr.RenderError,'retained-asr-outcome-no-automatic-retry'):
            self.run_asr()
        self.assertEqual(self.calls(),0)
    def test_stream_receipt_cannot_substitute_another_original(self):
        record,value=self.streaming_fixture();value['source_sha256']='a'*64;record.write_text(json.dumps(value))
        with self.assertRaisesRegex(asr.RenderError,'invalid-completed-streaming-transcript'):
            self.run_asr()
        self.assertEqual(self.calls(),0)
    def test_gemini_manual_final_reuses_completed_text_without_file_asr(self):
        record,value=self.streaming_fixture()
        value['model']='gemini-3.5-transcribe-live';record.write_text(json.dumps(value))
        self.assertEqual(self.run_asr(),{'text':'Live final words','final':True,'streaming_model':'gemini-3.5-transcribe-live'})
        record.unlink()
        self.assertEqual(asr.process(self.source,self.binding,completed_only=True)['streaming_model'],'gemini-3.5-transcribe-live')
        self.assertEqual(self.calls(),0)
    def test_unselected_stream_model_is_not_a_trusted_transcript(self):
        record,value=self.streaming_fixture()
        value['model']='gemini-conversational-agent';record.write_text(json.dumps(value))
        with self.assertRaisesRegex(asr.RenderError,'invalid-completed-streaming-transcript'):
            self.run_asr()
        self.assertEqual(self.calls(),0)
    def test_stream_receipt_pcm_integrity_and_identity(self):
        record,value=self.streaming_fixture();value['pcm_sha256']='a'*64;record.write_text(json.dumps(value))
        with self.assertRaisesRegex(asr.RenderError,'streaming-audio-original-changed'):
            self.run_asr()
        self.assertEqual(self.calls(),0)
    def test_uncertain_file_asr_is_not_overridden_by_later_live_text(self):
        record,value=self.streaming_fixture();record.unlink()
        self.helper.write_text(HELPER.replace("if source.read_bytes()==b'failed operation'",'if source.exists()'))
        with self.assertRaisesRegex(asr.RenderError,'asr-unconfirmed'):
            self.run_asr()
        record.write_text(json.dumps(value))
        with self.assertRaisesRegex(asr.RenderError,'retained-asr-outcome-no-automatic-retry'):
            self.run_asr()
        self.assertEqual(self.calls(),1)

    def test_completed_text_reused_after_media_disposal_without_paid_replay(self):
        first=self.run_asr();self.assertEqual(first,{'text':'Final words','final':True})
        self.assertEqual(self.calls(),1);self.assertTrue(self.source.exists())
        self.assertFalse(list((self.root/'cache').rglob('source.media')))
        self.assertFalse(list((self.root/'cache').rglob('*.wav')))
        self.assertTrue(list((self.root/'cache').rglob('draft.json')))
        self.assertEqual(self.run_asr(),first);self.assertEqual(self.calls(),1)
    def test_cleanup_interruption_recovers_from_completed_receipt(self):
        with patch.object(asr,'retire_preparation',side_effect=OSError('crash seam')):
            self.run_asr()
        self.assertTrue(list((self.root/'cache').rglob('source.media')))
        self.run_asr();self.assertEqual(self.calls(),1)
        self.assertFalse(list((self.root/'cache').rglob('source.media')))
    def test_optional_preparation_receipt_distinguishes_pending_cleanup_without_asr_replay(self):
        with patch.object(asr,'retire_preparation',side_effect=OSError('deferred')):
            first=asr.process(self.source,self.binding,include_retention=True)
        self.assertFalse(first['preparation_audio_deleted'])
        self.assertTrue(list((self.root/'cache').rglob('source.media')))
        recovered=asr.process(self.source,self.binding,include_retention=True)
        self.assertTrue(recovered['preparation_audio_deleted'])
        self.assertFalse(list((self.root/'cache').rglob('source.media')))
        self.assertEqual(self.calls(),1)

    def test_completed_only_reader_never_probes_or_calls_provider(self):
        with self.assertRaises(asr.RenderError):
            asr.process(self.source,self.binding,completed_only=True)
        self.assertEqual(self.calls(),0)
        self.run_asr()
        with patch.object(asr,'probe_duration',side_effect=AssertionError('Reader prepared audio')):
            self.assertEqual(asr.process(self.source,self.binding,completed_only=True)['text'],'Final words')
        self.assertEqual(self.calls(),1)

    def test_uncertain_helper_is_never_automatically_retried(self):
        self.source.write_bytes(b'failed operation')
        for _ in range(2):
            with self.assertRaises(asr.RenderError):self.run_asr()
        self.assertEqual(self.calls(),1)
        states=[json.loads(p.read_text()) for p in (self.root/'cache').glob('*/state.json')]
        self.assertEqual(states[0]['phase'],'unconfirmed')
    def test_protected_reservation_blocks_new_audio_before_helper(self):
        self.cfg['max_cache_bytes']=asr.MAX_JOB;self.write_config()
        self.source.write_bytes(b'failed operation')
        with self.assertRaises(asr.RenderError):self.run_asr()
        self.source.write_bytes(b'another original')
        with self.assertRaisesRegex(asr.RenderError,'capacity'):self.run_asr()
        self.assertEqual(self.calls(),1)
    def test_history_accounting_is_read_only_and_new_intent_remains_durable(self):
        root=self.root/'cache';root.mkdir(mode=0o700)
        for n in range(40):
            old=root/f'{n:064x}';old.mkdir(mode=0o700)
            (old/'state.json').write_text(json.dumps({'phase':'completed'}))
            (old/'draft.json').write_text('retained transcript')
        job=root/('f'*64);job.mkdir(mode=0o700)
        synced=[];fsync=os.fsync
        def record(fd):
            info=os.fstat(fd)
            if stat.S_ISDIR(info.st_mode):synced.append((info.st_dev,info.st_ino))
            return fsync(fd)
        before={p:p.read_bytes() for p in root.glob('*/draft.json')}
        with patch.object(os,'fsync',side_effect=record):
            asr.reserve(root,job,{'phase':'intent'},asr.DEFAULT_BUDGET)
        self.assertEqual(json.loads((job/'state.json').read_text()),{'phase':'intent'})
        info=job.stat()
        self.assertEqual(synced,[(info.st_dev,info.st_ino)],'Only the new intent directory needs a durability barrier')
        self.assertEqual({p:p.read_bytes() for p in before},before)

    def test_accounting_rejects_symlink_or_nonprivate_history_before_reserving(self):
        root=self.root/'cache';root.mkdir(mode=0o700)
        job=root/('f'*64);job.mkdir(mode=0o700)
        old=root/('a'*64);old.symlink_to(self.root,target_is_directory=True)
        with self.assertRaisesRegex(asr.RenderError,'unsafe-asr-cache-directory'):
            asr.reserve(root,job,{'phase':'intent'},asr.DEFAULT_BUDGET)
        self.assertFalse((job/'state.json').exists())
        old.unlink();old.mkdir(mode=0o755);old.chmod(0o755)
        with self.assertRaisesRegex(asr.RenderError,'unsafe-asr-cache-directory'):
            asr.reserve(root,job,{'phase':'intent'},asr.DEFAULT_BUDGET)
        self.assertFalse((job/'state.json').exists())
    def test_changed_derivative_is_protected_and_text_still_available(self):
        with patch.object(asr,'retire_preparation',side_effect=OSError('defer')):self.run_asr()
        media=next((self.root/'cache').rglob('source.media'));media.write_bytes(b'changed')
        self.assertEqual(self.run_asr()['text'],'Final words')
        self.assertTrue(media.exists());self.assertEqual(self.calls(),1)
    def test_corrupt_completed_result_cannot_become_replay(self):
        self.run_asr();state=next((self.root/'cache').glob('*/state.json'))
        value=json.loads(state.read_text());value['result']['text']='changed';state.write_text(json.dumps(value))
        with self.assertRaises(asr.RenderError):self.run_asr()
        self.assertEqual(self.calls(),1)
    def test_cli_completes_and_reopens_with_only_final_text_stdout(self):
        probe=self.root/'ffprobe';probe.write_text('#!'+sys.executable+'\nprint('+repr(json.dumps({'format':{'duration':'1.0'}}))+')\n');probe.chmod(0o700)
        environment={**os.environ,'PATH':str(self.root)+os.pathsep+os.environ['PATH']}
        command=[sys.executable,str(ROOT/'scripts/interaction_asr.py'),'--binding',str(self.binding),str(self.source)]
        for _ in range(2):
            outcome=subprocess.run(command,capture_output=True,text=True,check=True,env=environment)
            self.assertEqual(json.loads(outcome.stdout),{'text':'Final words','final':True})
        self.assertEqual(self.calls(),1)
    def test_real_long_compressed_source_refused_before_helper_preparation(self):
        self.probe.stop()
        self.source=self.root/'long.flac'
        subprocess.run(['ffmpeg','-v','error','-f','lavfi','-i','anullsrc=r=16000:cl=mono',
            '-t','121','-c:a','flac',str(self.source)],check=True)
        self.assertLess(self.source.stat().st_size,asr.MAX_SOURCE)
        with self.assertRaisesRegex(asr.RenderError,'duration'):self.run_asr()
        self.assertEqual(self.calls(),0)
        self.assertFalse(list((self.root/'cache').rglob('*.wav')))
        self.assertFalse(list((self.root/'cache').glob('*/state.json')))

    def test_stream_and_container_duration_disagreement_cannot_expand_preparation(self):
        self.probe.stop()
        for stream,container in ((121,1),(1,121),(float('nan'),1)):
            metadata={'streams':[{'duration':str(stream)}],'format':{'duration':str(container)}}
            with patch.object(asr.subprocess,'check_output',return_value=json.dumps(metadata).encode()):
                with self.assertRaisesRegex(asr.RenderError,'duration'):self.run_asr()
        self.assertEqual(self.calls(),0)
        self.assertFalse(list((self.root/'cache').glob('*/state.json')))

    def streaming_webm(self, seconds):
        self.source=self.root/'browser.webm'
        audio=subprocess.check_output(['ffmpeg','-v','error','-f','lavfi','-i',
            'sine=frequency=440:sample_rate=16000','-t',str(seconds),'-c:a','libopus',
            '-live','1','-f','webm','pipe:1'])
        self.source.write_bytes(audio)

    def test_browser_stream_without_duration_is_finalized_and_media_retired(self):
        self.probe.stop();self.streaming_webm(1)
        before=self.source.read_bytes()
        duration,pcm=asr.probe_duration(self.source)
        self.assertGreater(duration,.9);self.assertIsNotNone(pcm)
        self.assertEqual(self.run_asr(),{'text':'Final words','final':True})
        state=json.loads(next((self.root/'cache').glob('*/state.json')).read_text())
        self.assertIn('prepared_source_sha256',state)
        self.assertEqual(self.source.read_bytes(),before)
        self.assertFalse(list((self.root/'cache').rglob('*.wav')))
        self.assertFalse(list((self.root/'cache').rglob('source.media')))
        self.assertEqual(self.run_asr()['text'],'Final words');self.assertEqual(self.calls(),1)

    def test_overlong_stream_without_duration_refused_before_helper(self):
        self.probe.stop();self.streaming_webm(121)
        with self.assertRaisesRegex(asr.RenderError,'duration'):self.run_asr()
        self.assertEqual(self.calls(),0)
        self.assertFalse(list((self.root/'cache').glob('*/state.json')))

    def test_nonprivate_source_refused_before_helper(self):
        self.source.chmod(0o644)
        with self.assertRaises(asr.RenderError):self.run_asr()
        self.assertEqual(self.calls(),0)

if __name__=='__main__':unittest.main()
