"""Authenticated HTTP to actual renderer/cache; synthetic source and speech peers."""
import concurrent.futures
import copy
import hashlib
import fcntl
import json
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import speech_render as fixtures
from fastapi.testclient import TestClient
from dashboard.app import create_app
from dashboard.provider import ProviderUnavailable


class Receiver:
    def __init__(self):
        self.allowed = True
        self.changed = False
        self.parts = 0
        self.source = {'schema':'telecodex.presentation-part/v1',
            'id':'delivery:11111111-1111-1111-1111-111111111111', 'revision':'a'*64,
            'kind':'worker_result', 'entry_binding':[100, {'chat_id':100,'thread_id':1},1,'entry-native'],
            'text':'Alfa sigue trabajando. Beta ya respondió.', 'offset':0, 'next_offset':None,
            'current':True, 'correction_pending':False, 'completed':True, 'playback':'not_observed'}
    def authorized(self, human): return human == 100 and self.allowed
    def close(self): pass
    def snapshot(self): return {'sessions':[]}
    def presentation_list(self, delivery, native):
        result=copy.deepcopy(self.source)
        result['preview']=result.pop('text')
        return {'schema':'telecodex.presentation-list/v1','items':[result]}
    def presentation_part(self, source, revision, offset):
        self.parts += 1
        if self.changed or source != self.source['id'] or revision != self.source['revision'] or offset != 0:
            raise ProviderUnavailable('Source changed')
        return copy.deepcopy(self.source)


class Presentation(unittest.TestCase):
    def setUp(self):
        self.fixture = fixtures.Render()
        self.fixture.setUp()
        self.root = self.fixture.root
        self.provider = Receiver()
        self.config = {'state_dir':str(self.root/'dashboard'), 'origin':'https://example.test',
            'prefix':'/sessions','bot_id':123,'speech':{'binding':str(self.fixture.binding),'timeout_seconds':5}}
        self.client = self.client_for()
        self.headers = {'origin':'https://example.test'}
        self.body = {'source':self.provider.source['id'],'revision':self.provider.source['revision'],'offset':0}

    def client_for(self):
        return TestClient(create_app(self.config,self.provider,lambda raw,bot:int(raw)),base_url='https://example.test')

    def login(self):
        reply = self.client.post('/api/session',json={'init_data':'100'},headers=self.headers)
        token = reply.json()['session_token']
        self.headers['authorization']='Bearer '+token
        csrf = self.client.get('/api/state',headers=self.headers).json()['csrf']
        self.headers['x-dashboard-csrf']=csrf

    def audio(self, body=None):
        return self.client.post('/api/interaction/audio',json=self.body if body is None else body,headers=self.headers)

    def tearDown(self):
        self.client.close()
        self.fixture.tearDown()

    def test_authentication_source_selection_and_reconnect_reuse_actual_audio(self):
        self.assertEqual(self.audio().status_code,401)
        self.login()
        missing={k:v for k,v in self.headers.items() if k!='x-dashboard-csrf'}
        self.assertEqual(self.client.post('/api/interaction/audio',json=self.body,headers=missing).status_code,403)
        self.assertEqual(self.audio({**self.body,'text':'browser-chosen words'}).status_code,409)
        self.assertEqual(len(self.fixture.calls),0)
        first=self.audio()
        self.assertEqual(first.status_code,200,first.text)
        self.assertEqual(first.content,self.fixture.data)
        self.assertEqual(first.headers['x-audio-sha256'],hashlib.sha256(first.content).hexdigest())
        self.assertEqual(first.headers['x-playback-observation'],'not_observed')
        self.assertNotIn(str(self.root),str(dict(first.headers)))
        self.assertIn(self.provider.source['text'].encode(),self.fixture.calls[0][2])
        self.client.close()
        self.client=self.client_for()
        self.login()
        resumed=self.audio()
        self.assertEqual(resumed.content,first.content)
        self.assertEqual(len(self.fixture.calls),1)
        self.assertGreaterEqual(self.provider.parts,4)

    def test_correction_during_synthesis_prevents_stale_audio_response(self):
        self.login()
        self.fixture.hold=True
        with concurrent.futures.ThreadPoolExecutor(max_workers=1) as pool:
            pending=pool.submit(self.audio)
            self.assertTrue(self.fixture.entered.wait(timeout=4))
            self.provider.changed=True
            self.fixture.release.set()
            response=pending.result(timeout=8)
        self.assertEqual(response.status_code,409)
        self.assertNotEqual(response.headers.get('content-type'),'audio/wav')
        # Completed synthesis remains retained; changing authority never repeats worker work.
        states=list((self.root/'cache').rglob('state.json'))
        self.assertEqual(json.loads(states[0].read_text())['phase'],'rendered')
        self.assertEqual(len(self.fixture.calls),1)

    def test_owner_revocation_during_synthesis_prevents_private_audio(self):
        self.login()
        self.fixture.hold=True
        with concurrent.futures.ThreadPoolExecutor(max_workers=1) as pool:
            pending=pool.submit(self.audio)
            self.assertTrue(self.fixture.entered.wait(timeout=4))
            self.provider.allowed=False
            self.fixture.release.set()
            response=pending.result(timeout=8)
        self.assertEqual(response.status_code,401)
        self.assertNotEqual(response.content,self.fixture.data)
        self.assertEqual(len(self.fixture.calls),1)

    def test_total_deadline_reaps_renderer_and_retains_unknown_request_without_retry(self):
        self.client.close()
        self.config['speech']['timeout_seconds']=1
        self.client=self.client_for()
        self.login()
        self.fixture.hold=True
        try:
            response=self.audio()
            self.assertEqual(response.status_code,409)
            self.assertTrue(self.fixture.entered.is_set())
            state=next((self.root/'cache').rglob('state.json'))
            self.assertEqual(json.loads(state.read_text())['phase'],'intent')
            with state.with_name('.lock').open('rb') as lock:
                fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
            self.assertEqual(self.audio().status_code,409)
            self.assertEqual(len(self.fixture.calls),1)
        finally:
            self.fixture.release.set()


if __name__=='__main__':
    unittest.main()
