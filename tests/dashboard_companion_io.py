"""Explicit device grants through actual private files, HTTP and speech cache."""
import concurrent.futures
import json
from pathlib import Path
import sys
import time
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from fastapi.testclient import TestClient
from dashboard.app import create_app
from dashboard.companion import Grants
from dashboard.private_state import read_json, write_json
from presentation_io import Receiver
import speech_render as speech_fixture
from scripts import speech_render as renderer


class Owner(Receiver):
    human = 100
    binding_bytes = b'fictional-native-owner-entry-and-target-profile'
    def assert_binding(self): pass


class Companion(unittest.TestCase):
    def setUp(self):
        self.speech = speech_fixture.Render(); self.speech.setUp()
        self.root = self.speech.root
        self.provider = Owner()
        self.config = {'state_dir':str(self.root/'dashboard'), 'origin':'https://example.test',
            'bot_id':123, 'prefix':'/sessions',
            'companion':{'enabled':True,'grants_dir':str(self.root/'grants')},
            'speech':{'binding':str(self.speech.binding),'timeout_seconds':5}}
        self.grants = Grants(self.config, self.provider)
        self.output = self.root/'device.json'
        self.receipt = self.grants.enroll('Selected fixture button', self.output, 3600)
        self.device = read_json(self.output)
        self.record = self.root/'grants'/(self.receipt['grant_id']+'.json')
        self.client = self.client_for()
        self.headers = {'origin':self.config['origin'], 'authorization':self.device['authorization'],
            'x-dashboard-csrf':self.device['csrf']}
        self.body = {'source':self.provider.source['id'],'revision':self.provider.source['revision'],'offset':0}

    def client_for(self):
        return TestClient(create_app(self.config,self.provider,lambda raw,bot:int(raw)),base_url=self.config['origin'])

    def tearDown(self):
        self.client.close(); self.speech.tearDown()

    def audio(self):
        return self.client.post('/api/interaction/audio',json=self.body,headers=self.headers)

    def test_private_enrollment_and_actual_selected_audio_reuse_after_restart(self):
        self.assertEqual(self.output.stat().st_mode & 0o777,0o600)
        self.assertNotIn(self.device['authorization'],json.dumps(self.receipt))
        self.assertNotIn(self.device['authorization'],self.record.read_text())
        response=self.client.get('/api/interaction',headers=self.headers)
        self.assertEqual(response.status_code,200,response.text)
        self.assertEqual(response.json()['csrf'],self.device['csrf'])
        first=self.audio();self.assertEqual(first.status_code,200,first.text)
        self.assertEqual(first.content,self.speech.data)
        self.client.close();self.client=self.client_for()
        self.assertEqual(self.audio().content,first.content)
        self.assertEqual(len(self.speech.calls),1,'A reconnect does not repeat synthesis or native work')

    def test_administrative_and_approval_routes_denied_even_with_full_session_cookie(self):
        login=self.client.post('/api/session',json={'init_data':'100'},headers={'origin':self.config['origin']})
        full=login.json()['session_token']
        headers={**self.headers,'cookie':'telecodex_dashboard='+full}
        for path in ('/api/state','/api/traffic','/api/alerts','/api/stop','/api/interaction/requests/'+'b'*32):
            method='post' if path.endswith('/stop') or '/requests/' in path else 'get'
            response=getattr(self.client,method)(path,headers=headers)
            self.assertEqual(response.status_code,403,(path,response.text))
        self.assertEqual(self.client.post('/api/session',json={'init_data':'100'},headers=headers).status_code,403)
        forged={**headers,'authorization':'Companion '+full}
        self.assertEqual(self.client.get('/api/interaction',headers=forged).status_code,401)
        malformed={**headers,'authorization':'Companion invalid'}
        self.assertEqual(self.client.get('/api/interaction',headers=malformed).status_code,401)
        duplicate=[*self.headers.items(),('authorization','Bearer '+full)]
        self.assertEqual(self.client.get('/api/interaction',headers=duplicate).status_code,401)
        self.assertEqual(len(self.speech.calls),0)

    def test_monitor_grant_is_read_only_and_never_inherits_ambient_owner(self):
        from dashboard.companion import permitted
        output=self.root/'monitor.json'
        receipt=self.grants.enroll('Selected monitor',output,3600,scope='monitor_readonly')
        credential=read_json(output)
        self.assertEqual(receipt['scope'],'monitor_readonly')
        self.assertEqual(credential['scope'],'monitor_readonly')
        login=self.client.post('/api/session',json={'init_data':'100'},headers={'origin':self.config['origin']})
        headers={'origin':self.config['origin'],'authorization':credential['authorization'],
                 'cookie':'telecodex_dashboard='+login.json()['session_token']}
        self.assertEqual(self.client.get('/api/interaction',headers=headers).status_code,200)
        self.provider.conversation_telegram_inputs=lambda before,direction:{'items':[],'read_only':True}
        response=self.client.get('/api/interaction/telegram-inputs',headers=headers)
        self.assertEqual(response.status_code,200,response.text)
        self.provider.conversation_input_trace=lambda ident:{'id':ident,'read_only':True}
        path='/api/interaction/inputs/telegram:123/trace'
        self.assertEqual(self.client.get(path,headers=headers).status_code,200)
        allowed=['/api/interaction','/api/interaction/inputs','/api/interaction/part',
                 '/api/interaction/telegram-inputs',path,path+'/original',
                 '/api/interaction/inputs/'+'a'*36+'/trace/'+'b'*36]
        for selected in allowed:
            self.assertTrue(permitted('GET',selected,'monitor_readonly'),selected)
            for method in ('POST','PUT','DELETE'):
                self.assertFalse(permitted(method,selected,'monitor_readonly'))
        for selected in ('/api/state','/api/stop','/api/resume','/api/traffic','/api/alerts',
                         '/api/interaction/requests','/api/session','/api/interaction/audio',
                         '/api/interaction/voice','/api/interaction/visual','/api/interaction/inputs',
                         '/api/interaction/inputs/'+'a'*36+'/route'):
            if selected!='/api/interaction/inputs':
                self.assertEqual(self.client.get(selected,headers=headers).status_code,403,selected)
            self.assertEqual(self.client.post(selected,json={},headers=headers).status_code,403,selected)
        # Existing interaction credentials do not silently gain monitor access.
        self.assertEqual(self.client.get(path,headers=self.headers).status_code,403)
        self.grants.revoke(receipt['grant_id'])
        self.assertEqual(self.client.get(path,headers=headers).status_code,401)
        self.assertEqual(len(self.speech.calls),0)

    def test_origin_csrf_and_browser_chosen_speech_rejected_before_provider(self):
        self.assertEqual(self.client.post('/api/interaction/audio',json=self.body,headers={**self.headers,'origin':'https://other.test'}).status_code,403)
        self.assertEqual(self.client.post('/api/interaction/audio',json=self.body,headers={**self.headers,'x-dashboard-csrf':'wrong'}).status_code,403)
        self.assertEqual(self.client.post('/api/interaction/audio',json={**self.body,'text':'unowned words'},headers=self.headers).status_code,409)
        self.assertEqual(len(self.speech.calls),0)

    def test_revocation_during_actual_synthesis_prevents_private_audio(self):
        self.speech.hold=True
        with concurrent.futures.ThreadPoolExecutor(max_workers=1) as pool:
            pending=pool.submit(self.audio)
            self.assertTrue(self.speech.entered.wait(timeout=4))
            self.assertTrue(self.grants.revoke(self.receipt['grant_id'])['revoked'])
            self.speech.release.set();result=pending.result(timeout=8)
        self.assertEqual(result.status_code,401)
        self.assertEqual(len(self.speech.calls),1)
        self.assertEqual(self.audio().status_code,401)
        self.assertTrue(self.grants.revoke(self.receipt['grant_id'])['already_absent'])

    def test_expiry_owner_and_receiving_profile_changes_fail_closed(self):
        original=read_json(self.record)
        write_json(self.record,{**original,'expires':int(time.time())-1})
        self.assertEqual(self.audio().status_code,401)
        write_json(self.record,original);self.provider.allowed=False
        self.assertEqual(self.audio().status_code,401)
        self.provider.allowed=True;self.provider.binding_bytes=b'changed owned entry or target profile'
        self.assertEqual(self.audio().status_code,401)
        self.assertEqual(len(self.speech.calls),0)

    def test_disabled_and_malformed_or_unsafe_grants_never_inherit_authority(self):
        self.client.close();self.config['companion']['enabled']=False;self.client=self.client_for()
        self.assertEqual(self.audio().status_code,401)
        self.client.close();self.config['companion']['enabled']=True;self.client=self.client_for()
        write_json(self.record,[])
        self.assertEqual(self.audio().status_code,401)
        self.record.chmod(0o644)
        self.assertEqual(self.audio().status_code,401)
        self.assertEqual(len(self.speech.calls),0)

    def test_enrollment_preserves_existing_device_credentials_and_requires_real_owner(self):
        before=self.output.read_bytes()
        with self.assertRaises(ValueError):self.grants.enroll('Another button',self.output,3600)
        self.assertEqual(self.output.read_bytes(),before)
        self.provider.allowed=False
        with self.assertRaises(ValueError):self.grants.enroll('Other',self.root/'other.json',3600)
        self.assertFalse((self.root/'other.json').exists())
        self.assertEqual(len(list((self.root/'grants').glob('*.json'))),1)

    def test_retry_after_interrupted_revocation_establishes_durable_absence(self):
        with patch('dashboard.companion.sync_directory',side_effect=OSError('synthetic barrier failure')):
            with self.assertRaises(OSError):self.grants.revoke(self.receipt['grant_id'])
        self.assertFalse(self.record.exists())
        with patch('dashboard.companion.sync_directory',wraps=renderer.sync_directory) as synced:
            self.assertTrue(self.grants.revoke(self.receipt['grant_id'])['already_absent'])
            synced.assert_called_once_with(self.root/'grants')

    def test_enrollment_establishes_new_directory_names_before_publication(self):
        config={**self.config,'companion':{'enabled':True,'grants_dir':str(self.root/'new-grants'/'nested')}}
        with patch('scripts.speech_render.sync_directory',wraps=renderer.sync_directory) as synced:
            grants=Grants(config,self.provider)
            output=self.root/'new-device'/'nested'/'device.json'
            receipt=grants.enroll('Durable fixture',output,3600)
        parents=[call.args[0] for call in synced.call_args_list]
        self.assertIn(self.root,parents)
        self.assertIn(self.root/'new-grants',parents)
        self.assertIn(self.root/'new-device',parents)
        self.assertEqual(read_json(output)['grant_id'],receipt['grant_id'])


if __name__=='__main__':
    unittest.main()
