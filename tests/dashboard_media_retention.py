"""Scoped original-audio producer: real bytes, cancellation, caps and cleanup."""
import asyncio
import hashlib
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import AsyncMock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from dashboard.media_retention import MediaRetention, RetentionUnavailable
from dashboard.provider import Provider
from dashboard.speech import Speech
from scripts import speech_render
import speech_render as fixture

NATIVE = '11111111-1111-4111-8111-111111111111'

class Originals(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.config = {'directory':str(self.root/'handoff'), 'human_id':100,
            'chat_id':100,'topic_id':1,'native_thread':NATIVE,'max_bytes':256*1024*1024,'since':'2026-01-01T00:00:00+00:00'}
        self.retention = MediaRetention(self.config)
        self.source = {'entry_binding':[100,{'chat_id':100,'thread_id':1},2,NATIVE], 'id':'original','offset':0,'revision':'a'*64,'kind':'integrator_final','text':'Synthetic original.'}
    def tearDown(self):self.temp.cleanup()
    def receipts(self):return [json.loads(p.read_bytes()) for p in self.retention.root.glob('*.json')]
    def test_scope_checks_each_actor_entry_and_native(self):
        for binding in ([101,self.source['entry_binding'][1],2,NATIVE],
                        [100,{'chat_id':100,'thread_id':9},2,NATIVE],
                        [100,self.source['entry_binding'][1],2,'other-native'],[],None):
            self.assertIsNone(self.retention.begin({**self.source,'entry_binding':binding},'human','audio/wav',10))
        self.assertEqual(list(self.retention.root.iterdir()),[])
    def test_content_address_receipt_and_restart_retry_at_cap(self):
        data=b'original audio';receipt=self.retention.retain(self.source,'human','audio/wav',data)
        self.assertEqual((self.retention.root/receipt['audio_sha256']).read_bytes(),data)
        self.assertFalse(receipt['reconstruction']);self.assertEqual(receipt['playback'],'not_observed')
        other=MediaRetention(self.config);other.config['max_bytes']=1
        self.assertEqual(other.retain(self.source,'human','audio/wav',data),receipt)
        with self.assertRaises(RetentionUnavailable):other.begin(self.source,'assistant','audio/mpeg',100)
        self.assertEqual((other.root/receipt['audio_sha256']).read_bytes(),data)
    def test_reservation_bounds_concurrent_and_uncertain_growth(self):
        self.retention.config['max_bytes']=65536
        one=self.retention.begin(self.source,'assistant','audio/mpeg',20000)
        one.append(b'actual emitted bytes')
        with self.assertRaises(RetentionUnavailable):self.retention.begin(self.source,'assistant','audio/mpeg',20000)
        receipt=one.finish(complete=False)
        self.assertFalse(receipt['complete']);self.assertEqual(len(self.receipts()),1)
        self.assertEqual((self.retention.root/receipt['audio_sha256']).read_bytes(),b'actual emitted bytes')
        with self.assertRaises(RetentionUnavailable):one.append(b'x')
    def test_human_cleanup_retains_original_before_unlink(self):
        p=object.__new__(Provider);p.receiver={'db_path':str(self.root/'db')};p.media_retention=self.retention
        directory=self.root/'db.voice';directory.mkdir(mode=0o700)
        ident='22222222-2222-4222-8222-222222222222';data=b'owned human clip'
        path=directory/ident;path.write_bytes(data);path.chmod(0o600);digest=hashlib.sha256(data).hexdigest()
        value={**self.source,'id':ident,'text':'Original transcript','voice':{'mime':'audio/wav','audio_sha256':digest,
            'transcription':{'audio_sha256':digest,'capture_id':ident,'committed_at':'2026-10-29T00:00:00+00:00'}}}
        self.assertTrue(p.retire_voice_capture(value));self.assertFalse(path.exists())
        self.assertEqual((self.retention.root/digest).read_bytes(),data)
        # At capacity a newly committed original remains in the source location.
        path.write_bytes(data+b'new');path.chmod(0o600);digest=hashlib.sha256(path.read_bytes()).hexdigest()
        value['voice']['audio_sha256']=value['voice']['transcription']['audio_sha256']=digest
        self.retention.config['max_bytes']=1
        self.assertFalse(p.retire_voice_capture(value));self.assertTrue(path.exists())
    def test_corrupt_or_symlink_destination_preserves_capture(self):
        data=b'original';receipt=self.retention.retain(self.source,'human','audio/wav',data)
        path=self.retention.root/receipt['audio_sha256'];path.unlink();path.symlink_to(self.root/'unrelated')
        with self.assertRaises((RetentionUnavailable,OSError,ValueError)):
            self.retention.retain(self.source,'human','audio/wav',data)
    def test_ordinary_speech_reserves_before_renderer_and_protects_original(self):
        f=fixture.Render();f.setUp()
        try:
            speech=Speech({'binding':str(f.binding)});speech.media_retention=self.retention
            data=asyncio.run(speech.render(self.source))[0]
            self.assertEqual(len(f.calls),1)
            self.assertEqual(self.receipts()[0]['speaker'],'assistant')
            self.assertEqual((self.retention.root/hashlib.sha256(data).hexdigest()).read_bytes(),data)
            self.retention.config['max_bytes']=1
            with patch.object(speech,'_render',new=AsyncMock()) as render:
                with self.assertRaises(RetentionUnavailable):asyncio.run(speech.render(self.source))
                self.assertEqual(render.await_count,0)
        finally:f.tearDown()
    def test_cancel_after_wav_generated_preserves_same_cached_original(self):
        f=fixture.Render();f.setUp()
        try:
            speech=Speech({'binding':str(f.binding)});speech.media_retention=self.retention
            from dashboard.speech import speech_request
            payload=speech_request(self.source,f.config['language'])
            speech_render.render(f.binding,payload)
            with patch.object(speech,'_render',new=AsyncMock(side_effect=asyncio.CancelledError)):
                with self.assertRaises(asyncio.CancelledError):asyncio.run(speech.render(self.source))
            receipt=self.receipts()[0]
            self.assertGreater(receipt['audio_bytes'],44);self.assertFalse(receipt['complete'])
            self.assertEqual(len(f.calls),1,'no regeneration on cancellation')
        finally:f.tearDown()
    def test_actual_stream_preserved_before_first_yield_and_on_disconnect(self):
        f=fixture.Render();f.setUp()
        try:
            f.qwen_route();f.content_type='text/event-stream';f.data=f.stream_data()
            speech=Speech({'binding':str(f.binding),'streaming_output':True});speech.media_retention=self.retention
            async def interrupted():
                stream=speech.stream(self.source)
                first=await anext(stream)
                import base64
                self.assertEqual(next(self.retention.root.glob('*.part')).read_bytes(),base64.b64decode(first['data']))
                await stream.aclose()
            asyncio.run(interrupted())
            receipt=self.receipts()[0];self.assertFalse(receipt['complete'])
            self.assertEqual((self.retention.root/receipt['audio_sha256']).read_bytes(),b'ID3synthetic encoded audio')
            self.assertEqual(len(f.calls),1)
        finally:f.tearDown()
    def test_actual_stream_terminal_and_prune_keep_handoff(self):
        f=fixture.Render();f.setUp()
        try:
            f.qwen_route();f.content_type='text/event-stream';f.data=f.stream_data()
            speech=Speech({'binding':str(f.binding),'streaming_output':True});speech.media_retention=self.retention
            async def completed():return [frame async for frame in speech.stream(self.source)]
            frames=asyncio.run(completed());receipt=self.receipts()[0]
            self.assertTrue(receipt['complete']);self.assertEqual(frames[-1]['audio_sha256'],receipt['audio_sha256'])
            import time
            speech_render.prune_audio(speech_render.route(f.binding),now=time.time()+4000)
            self.assertEqual((self.retention.root/receipt['audio_sha256']).read_bytes(),b'ID3synthetic encoded audio')
        finally:f.tearDown()
    def test_earlier_human_capture_is_outside_forward_only_scope(self):
        source={**self.source,'voice':{'transcription':{'committed_at':'2025-01-01T00:00:00Z'}}}
        self.assertIsNone(self.retention.begin(source,'human','audio/wav',10))
    def test_voice_http_capacity_is_visible_without_native_submission(self):
        from dashboard_companion_io import Companion
        from dashboard.media_retention import RetentionUnavailable
        c=Companion('runTest');c.setUp()
        try:
            def full(*args):raise RetentionUnavailable('Original audio storage full; originals preserved')
            c.provider.conversation_voice_submit=full
            headers={**c.headers,'content-type':'audio/wav','x-input-binding':'a'*64,
                     'x-input-id':'22222222-2222-4222-8222-222222222222','x-audio-sha256':hashlib.sha256(b'x').hexdigest()}
            response=c.client.post('/api/interaction/voice',content=b'x',headers=headers)
            self.assertEqual(response.status_code,507,response.text)
            self.assertIn('originals preserved',response.json()['detail'])
        finally:c.tearDown()

    def test_nonblocking_lock_failure_is_visible_storage_failure(self):
        with speech_render.lock(self.retention.root/'.admission.lock'):
            with self.assertRaises(RetentionUnavailable):self.retention.begin(self.source,'human','audio/wav',10)

if __name__=='__main__':unittest.main()
