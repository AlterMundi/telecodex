"""Authenticated WebSocket preparation, exact audio receipt and no dispatch."""
import hashlib
import asyncio
import json
import os
from pathlib import Path
import queue
import sys
import tempfile
import threading
import socket
import time
import unittest
from unittest.mock import patch
import uuid
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
sys.path.insert(0,str(Path(__file__).resolve().parent))
sys.path.insert(0,str(Path(__file__).resolve().parents[1]/'scripts'))
from fastapi.testclient import TestClient
from starlette.websockets import WebSocketDisconnect
from dashboard.app import create_app
from dashboard.provider import Provider
from dashboard.companion import Grants
from scripts.interaction_asr import streaming_result
from dashboard.private_state import read_json
from dashboard.live_transcription import save_prepared, wav_bytes
from dashboard.transcription import TranscriptionUnavailable
from dashboard_io import Receiver


class Stream:
    opened = []
    def __init__(self,*args,**kwargs):
        self.selected_provider=kwargs.get('provider','qwen')
        self.events=queue.Queue();self.events.put({'type':'ready'})
        self.pcm=b'';self.finished=False;self.closed=False;self.closed_event=threading.Event()
        self.opened.append(self)
    def append(self,pcm):
        self.pcm+=pcm
        self.events.put({'type':'partial','text':'Draft words','revision':1})
    def finish(self):
        self.finished=True;self.events.put({'type':'final','text':'Final words','revision':2})
    def receive(self):
        value=self.events.get(timeout=2)
        if value is None:raise TranscriptionUnavailable('closed')
        return value
    def result(self):
        return {'text':'Final words','final':True,'model':'gemini-3.5-transcribe-live' if self.selected_provider=='gemini' else 'qwen-audio-3.0-realtime-plus',
                'pcm_sha256':hashlib.sha256(self.pcm).hexdigest(),
                'audio_millis':len(self.pcm)*1000//32000}
    def close(self):self.closed=True;self.events.put(None);self.closed_event.set()


class LiveTranscription(unittest.TestCase):
    def setUp(self):
        self.tmp=tempfile.TemporaryDirectory();self.root=Path(self.tmp.name);self.root.chmod(0o700)
        self.provider=Receiver();self.provider.receiver={'voice':{'enabled':True}}
        self.provider.input_id=Provider.input_id
        self.provider.binding_bytes=b'fixture-receiving-binding'
        self.provider.assert_binding=lambda:None
        self.provider.presentation_list=lambda *_: {'items':[]}
        self.binding='b'*64
        self.provider.presentation=lambda _: {'binding':self.binding}
        self.config={'state_dir':str(self.root/'state'),'origin':'https://example.test','bot_id':123,
                     'companion':{'enabled':True,'grants_dir':str(self.root/'grants')},
                     'streaming_transcription':{'enabled':True,'endpoint':'wss://provider.test/api-ws/v1/realtime?model=qwen-audio-3.0-realtime-plus',
                     'credential_file':str(self.root/'key'),'cache_root':str(self.root/'prepared'),
                     'turn_detection':{'enabled':True,'command':['/fixture/python','capture-turn']}}}
        self.patch=patch('dashboard.live_transcription.TranscriptionStream',Stream);self.patch.start()
        # The constant belongs to the adapter contract, including in synthetic peers.
        Stream.max_audio_bytes=120*32000;Stream.opened=[]
        self.client=TestClient(create_app(self.config,self.provider,lambda raw,bot:int(raw)),base_url='https://example.test')
        self.client.__enter__()
        response=self.client.post('/api/session',json={'init_data':'100'},headers={'origin':'https://example.test'})
        self.assertEqual(response.status_code,200)
        self.client.cookies.set('telecodex_dashboard',self.client.cookies.get('telecodex_dashboard'),path='/')
        self.csrf=self.client.get('/api/state').json()['csrf']
        self.ident=str(uuid.uuid4())
    def tearDown(self):self.client.__exit__(None,None,None);self.patch.stop();self.tmp.cleanup()
    def meta(self,**extra):return {'id':self.ident,'binding':self.binding,'csrf':self.csrf,**extra}
    def open(self,origin='https://example.test'):
        return self.client.websocket_connect('/api/interaction/voice/live',headers={'origin':origin})
    def test_streams_before_finish_retains_only_text_for_exact_wav(self):
        pcm=b'\0\0'*1600
        with self.open() as ws:
            ws.send_json(self.meta());self.assertEqual(ws.receive_json()['type'],'ready')
            ws.send_bytes(pcm);self.assertEqual(ws.receive_json()['type'],'partial')
            self.assertFalse(list((self.root/'prepared').glob('*.json')))
            ws.send_json({'type':'finish'})
            result=ws.receive_json();self.assertEqual(result['type'],'prepared')
            self.assertEqual(result['source_sha256'],hashlib.sha256(wav_bytes(pcm)).hexdigest())
        value=json.loads((self.root/'prepared'/f'{self.ident}.json').read_text())
        self.assertEqual(value['text'],'Final words');self.assertEqual(value['capture_id'],self.ident)
        self.assertEqual(value['binding'],self.binding);self.assertEqual(self.provider.calls,[])
        self.assertFalse(list((self.root/'prepared').glob('*.wav')))
        self.assertTrue(Stream.opened[0].finished);self.assertTrue(Stream.opened[0].closed_event.wait(2))
        original=self.root/self.ident;original.write_bytes(wav_bytes(pcm));original.chmod(0o600)
        trusted=streaming_result({'streaming_cache_root':str(self.root/'prepared')},original,
                                 hashlib.sha256(original.read_bytes()).hexdigest())
        self.assertEqual(trusted,{'text':'Final words','streaming_model':'qwen-audio-3.0-realtime-plus'})
    def test_selected_gemini_prepares_exact_original_with_no_dispatch(self):
        self.client.__exit__(None,None,None)
        self.config['streaming_transcription']['provider']='gemini'
        self.client=TestClient(create_app(self.config,self.provider,lambda raw,bot:int(raw)),base_url='https://example.test');self.client.__enter__()
        self.client.post('/api/session',json={'init_data':'100'},headers={'origin':'https://example.test'})
        self.client.cookies.set('telecodex_dashboard',self.client.cookies.get('telecodex_dashboard'),path='/')
        self.csrf=self.client.get('/api/state').json()['csrf']
        pcm=b'\0\0'*1600
        with self.open() as ws:
            ws.send_json(self.meta());self.assertEqual(ws.receive_json()['type'],'ready')
            ws.send_bytes(pcm);self.assertEqual(ws.receive_json()['type'],'partial')
            ws.send_json({'type':'finish'});self.assertEqual(ws.receive_json()['type'],'prepared')
        original=self.root/self.ident;original.write_bytes(wav_bytes(pcm));original.chmod(0o600)
        value=streaming_result({'streaming_cache_root':str(self.root/'prepared')},original,
                               hashlib.sha256(original.read_bytes()).hexdigest())
        self.assertEqual(value,{'text':'Final words','streaming_model':'gemini-3.5-transcribe-live'})
        self.assertEqual(Stream.opened[0].selected_provider,'gemini')
        self.assertEqual(self.provider.calls,[])

    def select_streaming_fallback(self):
        self.client.__exit__(None,None,None)
        selected=self.config['streaming_transcription'];selected['provider']='gemini'
        selected['fallback']={'provider':'qwen','endpoint':'wss://fallback.test/api-ws/v1/realtime?model=qwen-audio-3.0-realtime-plus','credential_file':str(self.root/'qwen-key')}
        self.client=TestClient(create_app(self.config,self.provider,lambda raw,bot:int(raw)),base_url='https://example.test');self.client.__enter__()
        self.client.post('/api/session',json={'init_data':'100'},headers={'origin':'https://example.test'})
        self.client.cookies.set('telecodex_dashboard',self.client.cookies.get('telecodex_dashboard'),path='/')
        self.csrf=self.client.get('/api/state').json()['csrf']

    def test_unavailable_gemini_open_selects_qwen_before_any_audio(self):
        self.select_streaming_fallback();attempts=[]
        class UnavailableOpen(Stream):
            def __init__(self,*args,**kwargs):
                attempts.append(kwargs['provider'])
                if kwargs['provider']=='gemini':raise TranscriptionUnavailable('Synthetic unavailable open')
                super().__init__(*args,**kwargs)
        with patch('dashboard.live_transcription.TranscriptionStream',UnavailableOpen),self.open() as ws:
            ws.send_json(self.meta());self.assertEqual(ws.receive_json()['type'],'ready')
            ws.send_bytes(b'\0\0'*1600);self.assertEqual(ws.receive_json()['type'],'partial')
            ws.send_json({'type':'finish'});self.assertEqual(ws.receive_json()['type'],'prepared')
        self.assertEqual(attempts,['gemini','qwen'])
        value=json.loads((self.root/'prepared'/f'{self.ident}.json').read_text())
        self.assertEqual(value['model'],'qwen-audio-3.0-realtime-plus')
        self.assertEqual(value['audio_millis'],100);self.assertEqual(self.provider.calls,[])

    def test_failed_gemini_setup_closes_it_before_selecting_qwen(self):
        self.select_streaming_fallback()
        class FailedSetup(Stream):
            def receive(self):
                if self.selected_provider=='gemini':raise TranscriptionUnavailable('Synthetic unavailable setup')
                return super().receive()
        with patch('dashboard.live_transcription.TranscriptionStream',FailedSetup),self.open() as ws:
            ws.send_json(self.meta());self.assertEqual(ws.receive_json()['type'],'ready')
            ws.send_bytes(b'\0\0');ws.receive_json();ws.send_json({'type':'finish'});self.assertEqual(ws.receive_json()['type'],'prepared')
        self.assertEqual([v.selected_provider for v in Stream.opened],['gemini','qwen'])
        self.assertTrue(Stream.opened[0].closed);self.assertEqual(Stream.opened[0].pcm,b'')

    def test_no_stream_provider_replay_after_readiness_or_audio(self):
        self.select_streaming_fallback()
        class FailedAudio(Stream):
            def append(self,pcm):
                super().append(pcm);raise TranscriptionUnavailable('Synthetic uncertain audio send')
        with patch('dashboard.live_transcription.TranscriptionStream',FailedAudio),self.open() as ws:
            ws.send_json(self.meta());self.assertEqual(ws.receive_json()['type'],'ready')
            ws.send_bytes(b'\0\0');self.assertEqual(ws.receive_json()['type'],'unavailable')
        self.assertEqual([v.selected_provider for v in Stream.opened],['gemini'])
        self.assertFalse(list((self.root/'prepared').glob('*.json')))
        self.assertEqual(self.provider.calls,[])

    def test_caption_mode_never_prepares_canonical_text(self):
        self.client.__exit__(None,None,None)
        self.config['streaming_transcription']['mode']='captions'
        self.config['streaming_transcription'].pop('turn_detection',None)
        self.client=TestClient(create_app(self.config,self.provider,lambda raw,bot:int(raw)),base_url='https://example.test');self.client.__enter__()
        response=self.client.post('/api/session',json={'init_data':'100'},headers={'origin':'https://example.test'})
        self.client.cookies.set('telecodex_dashboard',self.client.cookies.get('telecodex_dashboard'),path='/');self.csrf=self.client.get('/api/state').json()['csrf']
        with self.open() as ws:
            ws.send_json(self.meta(mode='captions'));self.assertEqual(ws.receive_json()['type'],'ready')
            ws.send_bytes(b'\0\0'*1600);self.assertEqual(ws.receive_json()['type'],'partial')
            ws.send_json({'type':'finish'});self.assertEqual(ws.receive_json()['type'],'unavailable')
        self.assertFalse(list((self.root/'prepared').glob('*.json')))
        self.assertFalse(Stream.opened[0].finished);self.assertTrue(Stream.opened[0].closed_event.wait(2))
        self.assertEqual(self.provider.calls,[])
        with self.open() as ws:
            ws.send_json(self.meta());self.assertEqual(ws.receive_json()['type'],'unavailable')
        self.assertEqual(len(Stream.opened),1,'A stale prepared-mode client must not select a different stream contract')

    def test_wrong_origin_csrf_or_binding_never_opens_provider(self):
        with self.assertRaises(WebSocketDisconnect):
            with self.open('https://wrong.test'):pass
        for meta in (self.meta(csrf='wrong'),self.meta(binding='c'*64)):
            with self.open() as ws:
                ws.send_json(meta);self.assertEqual(ws.receive_json()['type'],'unavailable')
        self.assertEqual(Stream.opened,[])
    def test_discard_and_disconnect_do_not_finish_or_save(self):
        for discard in (True,False):
            with self.open() as ws:
                ws.send_json(self.meta());self.assertEqual(ws.receive_json()['type'],'ready')
                ws.send_bytes(b'\0\0');ws.receive_json()
                if discard:ws.send_json({'type':'discard'})
        self.assertFalse(any(x.finished for x in Stream.opened))
        self.assertFalse(list((self.root/'prepared').glob('*.json')))
    def test_replaced_binding_blocks_final_preparation(self):
        with self.assertLogs('dashboard.live_transcription',level='WARNING') as logs:
            with self.open() as ws:
                ws.send_json(self.meta());ws.receive_json();ws.send_bytes(b'\0\0');ws.receive_json()
                self.binding='d'*64
                ws.send_json({'type':'finish'});self.assertEqual(ws.receive_json()['type'],'unavailable')
            self.assertTrue(Stream.opened[0].closed_event.wait(2))
        diagnostic=json.loads(logs.records[0].getMessage().split(' ',1)[1])
        self.assertEqual(diagnostic,{'capture_id':self.ident,'stage':'finish_authorization',
            'audio_bytes':2,'finish_received':True,'prepared_saved':False,'error_type':'ValueError'})
        self.assertFalse(Stream.opened[0].finished)
        self.assertFalse(list((self.root/'prepared').glob('*.json')))
    def test_revoked_human_cannot_finish_preparation(self):
        with self.open() as ws:
            ws.send_json(self.meta());ws.receive_json();ws.send_bytes(b'\0\0');ws.receive_json()
            self.provider.allowed=False
            ws.send_json({'type':'finish'});self.assertEqual(ws.receive_json()['type'],'unavailable')
        self.assertFalse(Stream.opened[0].finished)
        self.assertFalse(list((self.root/'prepared').glob('*.json')))
    def test_companion_scope_and_revocation_use_existing_grants(self):
        grants=Grants(self.config,self.provider)
        for scope in ('monitor_readonly','interaction_only'):
            output=self.root/(scope+'.json')
            enrolled=grants.enroll('Synthetic voice client',output,3600,scope=scope)
            credential=read_json(output)
            with self.open() as ws:
                ws.send_json(self.meta(authorization=credential['authorization'],csrf=credential['csrf']))
                if scope=='monitor_readonly':
                    self.assertEqual(ws.receive_json()['type'],'unavailable');self.assertEqual(Stream.opened,[])
                else:
                    self.assertEqual(ws.receive_json()['type'],'ready')
                    ws.send_bytes(b'\0\0');ws.receive_json();grants.revoke(enrolled['grant_id'])
                    ws.send_json({'type':'finish'});self.assertEqual(ws.receive_json()['type'],'unavailable')
        self.assertFalse(any(value.finished for value in Stream.opened))
    def test_completed_receipt_is_immutable_and_requires_matching_audio(self):
        pcm=b'\0\0'*1600
        result={'text':'Retained words','final':True,'model':'synthetic',
                'pcm_sha256':hashlib.sha256(pcm).hexdigest(),'audio_millis':100}
        root=self.root/'prepared'
        first=save_prepared(root,self.ident,self.binding,pcm,result)
        self.assertEqual(first,save_prepared(root,self.ident,self.binding,pcm,result))
        before=(root/(self.ident+'.json')).read_bytes()
        for changed in ({**result,'text':'replacement'},{**result,'final':False},{**result,'pcm_sha256':'0'*64}):
            with self.assertRaises(ValueError):save_prepared(root,self.ident,self.binding,pcm,changed)
            self.assertEqual((root/(self.ident+'.json')).read_bytes(),before)
        unsafe=str(uuid.uuid4());(root/(unsafe+'.json')).symlink_to(root/(self.ident+'.json'))
        with self.assertRaises(OSError):save_prepared(root,unsafe,self.binding,pcm,result)

    def test_revoked_actor_receives_no_delayed_caption(self):
        with self.open() as ws:
            ws.send_json(self.meta());self.assertEqual(ws.receive_json()['type'],'ready')
            self.provider.allowed=False
            Stream.opened[0].events.put({'type':'partial','text':'Must not leak','revision':1})
            self.assertEqual(ws.receive_json()['type'],'unavailable')
        self.assertFalse(Stream.opened[0].finished)
    def test_canceled_dial_closes_late_socket_and_releases_actor(self):
        entered,release,closed=threading.Event(),threading.Event(),threading.Event()
        class Delayed(Stream):
            count=0
            def __init__(self,*args,**kwargs):
                Delayed.count+=1;self.delayed=Delayed.count==1
                if self.delayed:
                    entered.set()
                    if not release.wait(3):raise TimeoutError('Synthetic dial gate')
                super().__init__(*args,**kwargs)
            def close(self):
                super().close()
                if self.delayed:closed.set()
        with patch('dashboard.live_transcription.TranscriptionStream',Delayed):
            try:
                with self.open() as ws:
                    ws.send_json(self.meta());self.assertTrue(entered.wait(2))
                # New actor admission must not wait for a canceled dial's socket.
                with self.open() as ws:
                    ws.send_json(self.meta());self.assertEqual(ws.receive_json()['type'],'ready')
                    ws.send_json({'type':'discard'})
                release.set();self.assertTrue(closed.wait(2))
            finally:release.set()
    def test_second_cancel_during_close_does_not_strand_actor_slot(self):
        entered,release=threading.Event(),threading.Event()
        class SlowClose(Stream):
            count=0
            def __init__(self,*args,**kwargs):
                SlowClose.count+=1;self.slow=SlowClose.count==1;super().__init__(*args,**kwargs)
            def close(self):
                if self.slow:
                    entered.set();release.wait(3)
                super().close()
        with patch('dashboard.live_transcription.TranscriptionStream',SlowClose):
            try:
                with self.open() as ws:
                    ws.send_json(self.meta());self.assertEqual(ws.receive_json()['type'],'ready')
                    ws.send_json({'type':'discard'});self.assertTrue(entered.wait(2))
                with self.open() as ws:
                    ws.send_json(self.meta());self.assertEqual(ws.receive_json()['type'],'ready')
                    ws.send_json({'type':'discard'})
            finally:release.set()
    def test_real_uvicorn_upgrade_and_ordered_capture(self):
        import uvicorn
        import websocket
        app=create_app(self.config,self.provider,lambda raw,bot:int(raw))
        server=uvicorn.Server(uvicorn.Config(app,log_level='error',lifespan='off'))
        listener=socket.socket();listener.bind(('127.0.0.1',0));listener.listen(16)
        thread=threading.Thread(target=server.run,kwargs={'sockets':[listener]},daemon=True);thread.start()
        deadline=time.monotonic()+3
        while not server.started and thread.is_alive() and time.monotonic()<deadline:time.sleep(.005)
        try:
            self.assertTrue(server.started,'Locked receiving runtime must support real WebSocket upgrade')
            ws=websocket.create_connection('ws://127.0.0.1:'+str(listener.getsockname()[1])+'/api/interaction/voice/live',
                                           origin='https://example.test',timeout=3)
            try:
                ws.send(json.dumps(self.meta(authorization='Bearer '+next(c.value for c in self.client.cookies.jar if c.name=='telecodex_dashboard'))))
                self.assertEqual(json.loads(ws.recv())['type'],'ready')
                ws.send_binary(b'\0\0'*1600);self.assertEqual(json.loads(ws.recv())['type'],'partial')
                ws.send(json.dumps({'type':'finish'}));self.assertEqual(json.loads(ws.recv())['type'],'prepared')
            finally:ws.close()
        finally:
            server.should_exit=True;thread.join(4);listener.close()
            self.assertFalse(thread.is_alive(),'Finite fixture server must stop after its socket closes')
        self.assertEqual(self.provider.calls,[])

    def test_optional_detection_shares_pcm_without_admitting_input(self):
        class Analysis:
            instance=None
            @classmethod
            async def open(cls,_):
                cls.instance=cls();cls.instance.queue=asyncio.Queue();cls.instance.pcm=b'';cls.instance.closed=False
                return cls.instance
            async def feed(self,data):
                self.pcm+=data
                await self.queue.put({'schema':'telecodex.turn-activity/v1','type':'activity','generation':1,'speaking':False,
                    'speech_seen':True,'audio_bytes':len(self.pcm),'endpoint':{'generation':1,'audio_bytes':len(self.pcm),
                    'probability':.9,'authority':'analysis_only'}})
            async def receive(self):return await self.queue.get()
            async def close(self):self.closed=True
        pcm=b'\0\0'*1600
        with patch('dashboard.live_transcription.Detector',Analysis):
            with self.open() as ws:
                ws.send_json(self.meta(turn_detection=True));self.assertEqual(ws.receive_json()['type'],'ready')
                ws.send_bytes(pcm);events=[ws.receive_json(),ws.receive_json()]
                self.assertEqual({v['type'] for v in events},{'partial','turn_activity'})
                self.assertEqual(next(v for v in events if v['type']=='turn_activity')['id'],self.ident)
                self.assertEqual(self.provider.calls,[]);self.assertEqual(Analysis.instance.pcm,pcm)
                ws.send_json({'type':'finish'});self.assertEqual(ws.receive_json()['type'],'prepared')
            self.assertTrue(Analysis.instance.closed)
        self.assertEqual(self.provider.calls,[])

    def test_detector_failure_keeps_existing_asr_and_original_boundary(self):
        from dashboard.turn_detection import DetectionUnavailable
        class Unavailable:
            @classmethod
            async def open(cls,_):raise DetectionUnavailable('Fixture analysis failure')
        with patch('dashboard.live_transcription.Detector',Unavailable):
            with self.open() as ws:
                ws.send_json(self.meta(turn_detection=True));self.assertEqual(ws.receive_json()['type'],'turn_detection_unavailable')
                self.assertEqual(ws.receive_json()['type'],'ready')
                ws.send_bytes(b'\0\0'*1600);self.assertEqual(ws.receive_json()['type'],'partial')
                ws.send_json({'type':'finish'});self.assertEqual(ws.receive_json()['type'],'prepared')
        self.assertEqual(self.provider.calls,[])

    def test_analysis_pipe_failure_does_not_close_recognition(self):
        from dashboard.turn_detection import DetectionUnavailable
        class BrokenPipe:
            instance=None
            @classmethod
            async def open(cls,_):cls.instance=cls();cls.instance.closed=False;return cls.instance
            async def receive(self):await asyncio.Future()
            async def feed(self,_):raise DetectionUnavailable('Fixture pipe failure')
            async def close(self):self.closed=True
        with patch('dashboard.live_transcription.Detector',BrokenPipe):
            with self.open() as ws:
                ws.send_json(self.meta(turn_detection=True));self.assertEqual(ws.receive_json()['type'],'ready')
                ws.send_bytes(b'\0\0'*1600)
                self.assertEqual({ws.receive_json()['type'],ws.receive_json()['type']},{'partial','turn_detection_unavailable'})
                ws.send_bytes(b'\0\0'*1600);self.assertEqual(ws.receive_json()['type'],'partial')
                ws.send_json({'type':'finish'});self.assertEqual(ws.receive_json()['type'],'prepared')
            self.assertTrue(BrokenPipe.instance.closed)
        self.assertEqual(self.provider.calls,[])

    def test_explicit_credential_does_not_downgrade_to_cookie(self):
        with self.open() as ws:
            ws.send_json(self.meta(authorization='Bearer invalid'));self.assertEqual(ws.receive_json()['type'],'unavailable')
        self.assertEqual(Stream.opened,[])


if __name__=='__main__':unittest.main()
