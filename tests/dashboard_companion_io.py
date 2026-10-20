"""Explicit device grants through actual private files, HTTP and speech cache."""
import copy
import hashlib
import os
import tempfile
import uuid
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
from dashboard.companion import Grants, authority
from dashboard.provider import Provider, ProviderUnavailable
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

    def test_profile_is_receiver_selected_presentation_without_permission_expansion(self):
        response=self.client.get('/api/interaction?presentation_profile=explorer',headers={**self.headers,'x-presentation-profile':'explorer'})
        self.assertEqual(response.status_code,200)
        self.assertEqual(response.json()['presentation_profile'],'simple')
        selected=self.root/'explorer.json'
        self.grants.enroll('Explorer',selected,3600,profile='explorer')
        device=read_json(selected)
        headers={**self.headers,'authorization':device['authorization'],'x-dashboard-csrf':device['csrf']}
        self.assertEqual(self.client.get('/api/interaction',headers=headers).json()['presentation_profile'],'explorer')
        self.assertEqual(self.client.get('/api/state',headers=headers).status_code,403)
        self.assertEqual(self.client.post('/api/stop',json={},headers=headers).status_code,403)
        self.assertEqual(self.client.post('/api/interaction/requests/'+('a'*32),json={},headers=headers).status_code,403)
        before=read_json(self.record)
        before.pop('presentation_profile',None);write_json(self.record,before)
        self.assertEqual(self.client.get('/api/interaction',headers=self.headers).json()['presentation_profile'],'simple','old grants retain the configured simple default')
        with self.assertRaises(ValueError):self.grants.enroll('Invalid',self.root/'invalid-profile.json',3600,profile='owner')

    def test_standalone_shell_has_no_telegram_runtime_dependency(self):
        standalone=self.client.get('/?view=conversation&client=standalone')
        self.assertEqual(standalone.status_code,200)
        self.assertNotIn('https://telegram.org/js/telegram-web-app.js',standalone.text)
        self.assertIn('assets/app.js',standalone.text)
        self.assertIn('https://telegram.org/js/telegram-web-app.js',self.client.get('/').text)

    def test_conversation_grant_reads_only_its_existing_input_trace_routes(self):
        from dashboard.companion import permitted
        self.provider.conversation_input_trace=lambda ident:{'id':ident,'read_only':True}
        ident=str(uuid.uuid4())
        path='/api/interaction/inputs/'+ident+'/trace'
        self.assertEqual(self.client.get(path,headers=self.headers).json(),{'id':ident,'read_only':True})
        for route in (path,path+'/original',path+'/'+str(uuid.uuid4())):
            self.assertTrue(permitted('GET',route))
            self.assertFalse(permitted('POST',route))
        for route in ('/api/interaction/telegram-inputs','/api/interaction/inputs/telegram:123/trace','/api/stop','/api/state'):
            self.assertEqual(self.client.get(route,headers=self.headers).status_code,403)

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
        self.provider.conversation_inputs=lambda before,direction:{'items':[]}
        response=self.client.get('/api/interaction/inputs',headers=headers)
        self.assertEqual(response.status_code,200,response.text)
        self.assertEqual(response.json()['viewer'],{'label':'Selected monitor','expires':credential['expires']})
        self.assertEqual(self.client.get('/api/interaction/inputs',headers=self.headers).json()['viewer'],
                         {'label':'Selected fixture button','expires':self.device['expires']})
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

    def test_goal_review_is_readonly_for_device_and_monitor(self):
        ident='11111111-1111-4111-8111-111111111111'
        for path in ('/api/interaction/goals','/api/interaction/goals/'+ident):
            response=self.client.get(path,headers=self.headers)
            self.assertEqual(response.status_code,200,response.text)
            self.assertEqual(self.client.post(path,json={},headers=self.headers).status_code,403)
            for scope in ('interaction_only','monitor_readonly'):
                from dashboard.companion import permitted
                self.assertTrue(permitted('GET',path,scope))
                for method in ('POST','PUT','DELETE'):
                    self.assertFalse(permitted(method,path,scope))
        self.assertEqual(self.client.get('/api/interaction/goals/not-a-uuid',headers=self.headers).status_code,403)
        self.grants.revoke(self.device['grant_id'])
        self.assertEqual(self.client.get('/api/interaction/goals',headers=self.headers).status_code,401)
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

    def test_owner_renewal_preserves_button_identity_and_permissions(self):
        original=read_json(self.record)
        expired={**original,'expires':int(time.time())-1}
        write_json(self.record,expired)
        before=self.output.read_bytes()
        self.assertEqual(self.client.get('/api/interaction',headers=self.headers).status_code,401)
        receipt=self.grants.renew(self.receipt['grant_id'],7*86400)
        self.assertEqual(self.output.read_bytes(),before,'Saved capture identity must not change')
        self.assertEqual(receipt['previous_expires'],expired['expires'])
        self.assertEqual(read_json(self.record),{**expired,'expires':receipt['expires']})
        self.assertEqual(self.client.get('/api/interaction',headers=self.headers).status_code,200)
        self.assertEqual(self.client.get('/api/state',headers=self.headers).status_code,403)
        self.assertEqual(len(self.speech.calls),0)

    def test_renewal_refuses_changed_authority_scope_owner_and_revoked_grants(self):
        original=read_json(self.record)
        for change in ({'authority':'changed'},{'scope':'monitor_readonly'},
                       {'human':101},{'csrf':'invalid'},{'expires':False}):
            write_json(self.record,{**original,**change})
            with self.assertRaises(ValueError):self.grants.renew(self.receipt['grant_id'],7*86400)
            self.assertEqual(read_json(self.record),{**original,**change})
        write_json(self.record,original)
        for seconds in (True,59,30*86400+1,60):
            with self.assertRaises(ValueError):self.grants.renew(self.receipt['grant_id'],seconds)
        self.provider.allowed=False
        with self.assertRaises(ValueError):self.grants.renew(self.receipt['grant_id'],7*86400)
        self.provider.allowed=True
        self.grants.revoke(self.receipt['grant_id'])
        with self.assertRaises(FileNotFoundError):self.grants.renew(self.receipt['grant_id'],7*86400)
        self.assertFalse(self.record.exists())

    def test_concurrent_renewal_and_revocation_cannot_resurrect_access(self):
        import threading
        entered=threading.Event();release=threading.Event()
        original=self.provider.presentation_list
        def held(*args):
            entered.set()
            if not release.wait(timeout=4):raise TimeoutError('release renewal fixture')
            return original(*args)
        self.provider.presentation_list=held
        with concurrent.futures.ThreadPoolExecutor(max_workers=1) as pool:
            pending=pool.submit(self.grants.renew,self.receipt['grant_id'],7*86400)
            try:
                self.assertTrue(entered.wait(timeout=4))
                with self.assertRaises(renderer.RenderError):
                    Grants(self.config,self.provider).revoke(self.receipt['grant_id'])
            finally:release.set()
            self.assertTrue(pending.result(timeout=5)['renewed'])
        self.grants.revoke(self.receipt['grant_id'])
        with self.assertRaises(FileNotFoundError):self.grants.renew(self.receipt['grant_id'],7*86400)
        self.assertEqual(self.client.get('/api/interaction',headers=self.headers).status_code,401)

    def test_profile_adoption_is_explicit_preserves_device_and_exact_retry(self):
        previous=authority(self.config,self.provider)
        before=read_json(self.record);credential=self.output.read_bytes()
        self.provider.binding_bytes=b'owner-selected-updated-transcription-route'
        self.assertEqual(self.client.get('/api/interaction',headers=self.headers).status_code,401)
        with self.assertRaises(ValueError):self.grants.renew(self.receipt['grant_id'],7*86400)
        with self.assertRaises(ValueError):self.grants.reauthorize(self.receipt['grant_id'],'0'*64)
        result=self.grants.reauthorize(self.receipt['grant_id'],previous)
        self.assertTrue(result['reauthorized']);self.assertEqual(result['expires'],before['expires'])
        after=read_json(self.record)
        self.assertEqual({k:v for k,v in before.items() if k!='authority'},
            {k:v for k,v in after.items() if k!='authority'})
        self.assertEqual(self.output.read_bytes(),credential)
        self.assertEqual(self.client.get('/api/interaction',headers=self.headers).status_code,200)
        self.assertTrue(self.grants.reauthorize(self.receipt['grant_id'],previous)['already_current'])
        self.assertEqual(self.client.post('/api/state',headers=self.headers).status_code,403)

    def test_adoption_refuses_expired_foreign_revoked_and_concurrent_revoke(self):
        previous=authority(self.config,self.provider);original=read_json(self.record)
        self.provider.binding_bytes=b'owner-selected-updated-profile'
        for change in ({'expires':int(time.time())-1},{'human':101},{'scope':'owner'},{'csrf':'invalid'}):
            write_json(self.record,{**original,**change})
            with self.assertRaises(ValueError):self.grants.reauthorize(self.receipt['grant_id'],previous)
        write_json(self.record,original)
        self.provider.allowed=False
        with self.assertRaises(ValueError):self.grants.reauthorize(self.receipt['grant_id'],previous)
        self.provider.allowed=True
        with renderer.lock(self.grants.root/'owner-mutation.lock'):
            with self.assertRaises(renderer.RenderError):self.grants.reauthorize(self.receipt['grant_id'],previous)
        self.grants.revoke(self.receipt['grant_id'])
        with self.assertRaises(FileNotFoundError):self.grants.reauthorize(self.receipt['grant_id'],previous)
        self.assertFalse(self.record.exists())


class CaptureRetention(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.provider = object.__new__(Provider)
        self.provider.receiver = {'db_path':str(self.root/'state.sqlite')}
        self.directory = self.root/'state.sqlite.voice'
        self.directory.mkdir(mode=0o700)
        self.ident = str(uuid.uuid4())
        self.path = self.directory/self.ident
        self.path.write_bytes(b'original owned clip');self.path.chmod(0o600)
        digest = hashlib.sha256(self.path.read_bytes()).hexdigest()
        self.value = {'id':self.ident,'text':'Committed words','phase':'preparing',
            'voice':{'audio_sha256':digest,'transcription':{'audio_sha256':digest,
                'capture_id':self.ident,'committed_at':'2026-10-18T00:00:00Z'}}}
    def tearDown(self):self.temp.cleanup()
    def test_confirmed_clip_removed_but_text_and_identity_survive(self):
        original = copy.deepcopy(self.value)
        self.assertTrue(self.provider.retire_voice_capture(self.value))
        self.assertFalse(self.path.exists())
        self.assertEqual(self.value,original)
        self.assertFalse(self.provider.retire_voice_capture(self.value))
    def test_pending_mismatched_empty_and_changed_audio_are_retained(self):
        for change in ('pending','identity','hash','text','commit'):
            value = copy.deepcopy(self.value)
            if change == 'pending':value['voice'].pop('transcription')
            if change == 'identity':value['voice']['transcription']['capture_id']=str(uuid.uuid4())
            if change == 'hash':value['voice']['transcription']['audio_sha256']='0'*64
            if change == 'text':value['text']=''
            if change == 'commit':value['voice']['transcription'].pop('committed_at')
            self.assertFalse(self.provider.retire_voice_capture(value),change)
            self.assertTrue(self.path.exists())
        self.path.write_bytes(b'changed bytes')
        self.assertFalse(self.provider.retire_voice_capture(self.value))
    def test_links_and_nonprivate_files_are_not_removed(self):
        self.path.chmod(0o644)
        self.assertFalse(self.provider.retire_voice_capture(self.value))
        outside = self.root/'other-recording';self.path.rename(outside)
        self.path.symlink_to(outside)
        self.assertFalse(self.provider.retire_voice_capture(self.value))
        self.assertTrue(outside.exists());self.assertTrue(self.path.is_symlink())
    def test_cleanup_error_does_not_change_committed_input(self):
        original = copy.deepcopy(self.value)
        with patch('dashboard.provider.os.open',side_effect=OSError('simulated')):
            self.assertFalse(self.provider.retire_voice_capture(self.value))
        self.assertEqual(self.value,original);self.assertTrue(self.path.exists())


class CaptureCapacity(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.directory = self.root/'voice'
        self.directory.mkdir(mode=0o700)
    def tearDown(self):self.temp.cleanup()
    def retain(self, data=b'new recording', ident=None):
        ident = ident or str(uuid.uuid4())
        Provider.retain_voice_capture(self.directory,ident,data,hashlib.sha256(data).hexdigest())
        return self.directory/ident
    def protected(self, size):
        path=self.directory/str(uuid.uuid4())
        with path.open('wb') as stream:stream.truncate(size)
        path.chmod(0o600)
        return path
    def test_full_uncertain_storage_refuses_new_but_allows_exact_retry(self):
        data=b'exact retained original';path=self.retain(data)
        protected=self.protected(256*1024*1024-len(data))
        before={p.name:p.stat().st_size for p in self.directory.iterdir()}
        with self.assertRaises(ProviderUnavailable):self.retain()
        self.assertEqual(self.retain(data,path.name),path)
        self.assertEqual(path.read_bytes(),data)
        self.assertEqual({p.name:p.stat().st_size for p in self.directory.iterdir()},before)
        self.assertTrue(protected.exists())
    def test_parallel_admission_cannot_overrun_shared_budget(self):
        import concurrent.futures,threading
        self.protected(255*1024*1024)
        barrier=threading.Barrier(2)
        def submit():
            barrier.wait()
            try:self.retain(b'x'*(1024*1024));return True
            except ProviderUnavailable:return False
        with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:
            results=list(pool.map(lambda _:submit(),range(2)))
        self.assertEqual(results.count(True),1)
        self.assertLessEqual(sum(p.stat().st_size for p in self.directory.iterdir()),256*1024*1024)
    def test_exact_retry_completes_failed_publication_barrier(self):
        ident=str(uuid.uuid4());data=b'retained before failed sync'
        with patch('dashboard.provider.sync_directory',side_effect=OSError('injected')):
            with self.assertRaises(OSError):self.retain(data,ident)
        self.assertEqual((self.directory/ident).read_bytes(),data)
        with patch('dashboard.provider.sync_directory',wraps=renderer.sync_directory) as barrier:
            self.retain(data,ident)
            barrier.assert_called_once_with(self.directory)
    def test_many_small_uncertain_files_cannot_exhaust_metadata(self):
        for _ in range(128):self.protected(1)
        before={p.name for p in self.directory.iterdir()}
        with self.assertRaises(ProviderUnavailable):self.retain()
        self.assertEqual({p.name for p in self.directory.iterdir()}-{'admission.lock'},before)
    def test_unknown_unsafe_storage_and_changed_identity_are_preserved(self):
        data=b'first';path=self.retain(data)
        with self.assertRaises(ValueError):self.retain(b'changed',path.name)
        self.assertEqual(path.read_bytes(),data)
        external=self.root/'unrelated';external.write_bytes(b'preserve')
        link=self.directory/'unknown-link';link.symlink_to(external)
        with self.assertRaises(ProviderUnavailable):self.retain()
        self.assertTrue(link.is_symlink());self.assertEqual(external.read_bytes(),b'preserve')


if __name__=='__main__':
    unittest.main()
