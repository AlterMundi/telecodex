#!/usr/bin/env python3
"""Real private-file hashing, streaming HTTP and scoped grants; no native/models."""
from contextlib import contextmanager
from datetime import datetime, timedelta, timezone
import copy
import hashlib
import os
from pathlib import Path
import sys
import tempfile
import unittest
import uuid
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from fastapi.testclient import TestClient
from dashboard.app import create_app
from dashboard.companion import Grants, permitted
from dashboard.provider import Provider


class Files:
    human=100
    binding_bytes=b'synthetic-artifact-receiver'
    def __init__(self,root):
        self.allowed=True
        self.root=root
        self.data=b'Actual generated file bytes\x00\xff'
        self.sha=hashlib.sha256(self.data).hexdigest()
        self.path=root/'retained'/self.sha
        self.path.parent.mkdir(mode=0o700)
        self.path.write_bytes(self.data);self.path.chmod(0o400)
        self.receiver={'interaction':{'artifact_store':{'directory':str(self.path.parent)}}}
        self.ref={'id':str(uuid.uuid4()),'name':'generated.txt','sha256':self.sha,
            'bytes':len(self.data),'expires_at':(datetime.now(timezone.utc)+timedelta(hours=24)).isoformat(),
            'entry_binding':[100,{'chat_id':100,'thread_id':1},1,'old-native'],
            'native_thread':'original-worker','native_turn':'original-turn','native_item':'observed-item'}
        self.renew_during_open=False
        self.reads=0
    def assert_binding(self):pass
    def authorized(self,human):return self.allowed and human==100
    def close(self):pass
    def presentation_list(self,*args):return {'binding':'logical-entry','items':[]}
    def artifacts(self):return {'items':[self.artifact_detail(self.ref['id'])]}
    def artifact_detail(self,ident):
        if ident!=self.ref['id']:raise ValueError('Unknown retained artifact')
        self.reads+=1
        value=copy.deepcopy(self.ref)
        if self.renew_during_open and self.reads%2==0:value['entry_binding'][3]='renewed-native'
        return value
    artifact_open=Provider.artifact_open
    input_id=staticmethod(Provider.input_id)


class ArtifactAccessHTTP(unittest.TestCase):
    def setUp(self):
        self.temporary=tempfile.TemporaryDirectory()
        self.root=Path(self.temporary.name)
        self.provider=Files(self.root)
        self.config={'state_dir':str(self.root/'dashboard'),'origin':'https://example.test','bot_id':123,
            'companion':{'enabled':True,'grants_dir':str(self.root/'grants')}}
        self.client=TestClient(create_app(self.config,self.provider,lambda raw,bot:int(raw)),base_url=self.config['origin'])
        login=self.client.post('/api/session',json={'init_data':'100'},headers={'origin':self.config['origin']})
        self.owner={'authorization':'Bearer '+login.json()['session_token']}
        self.url='/api/interaction/artifacts/'+self.provider.ref['id']
    def tearDown(self):self.client.close();self.temporary.cleanup()
    def device(self,scope='interaction_only',access=False):
        target=self.root/(uuid.uuid4().hex+'.json')
        grants=Grants(self.config,self.provider)
        grants.enroll('Synthetic file client',target,60,scope=scope,artifact_access=access)
        import json
        credential=json.loads(target.read_text())
        return grants,credential,{'authorization':credential['authorization']}

    def test_actual_bytes_hash_headers_and_native_renewal(self):
        self.provider.renew_during_open=True
        response=self.client.get(self.url,headers=self.owner)
        self.assertEqual(response.status_code,200,response.text)
        self.assertEqual(response.content,self.provider.data)
        self.assertEqual(response.headers['x-artifact-sha256'],self.provider.sha)
        self.assertEqual(response.headers['cache-control'],'no-store')
        self.assertIn('attachment',response.headers['content-disposition'])
        self.assertEqual(response.headers['x-content-type-options'],'nosniff')

    def test_explicit_device_access_and_revocation_do_not_upgrade_other_scopes(self):
        grants,credential,device=self.device()
        self.assertEqual(self.client.get(self.url,headers=device).status_code,403)
        grants.artifacts(credential['grant_id'],True)
        self.assertEqual(self.client.get(self.url,headers=device).content,self.provider.data)
        grants.artifacts(credential['grant_id'],False)
        self.assertEqual(self.client.get(self.url,headers=device).status_code,403)
        _,_,monitor=self.device('monitor_readonly')
        self.assertEqual(self.client.get(self.url,headers=monitor).status_code,403)
        self.assertFalse(permitted('GET',self.url,'monitor_readonly',True))
        with self.assertRaises(ValueError):self.device('monitor_readonly',True)

    def test_expired_denied_and_tampered_files_never_return_bytes(self):
        self.provider.ref['expires_at']='2000-01-01T00:00:00+00:00'
        self.assertEqual(self.client.get(self.url,headers=self.owner).status_code,409)
        self.provider.ref['expires_at']=(datetime.now(timezone.utc)+timedelta(hours=24)).isoformat()
        self.provider.path.chmod(0o600);self.provider.path.write_bytes(b'x'*len(self.provider.data));self.provider.path.chmod(0o400)
        self.assertEqual(self.client.get(self.url,headers=self.owner).status_code,409)
        self.provider.allowed=False
        self.assertEqual(self.client.get(self.url,headers=self.owner).status_code,401)

    def test_no_arbitrary_path_or_symlink_download(self):
        outside=self.root/'private';outside.write_bytes(b'UNRELATED PRIVATE')
        self.provider.path.unlink();self.provider.path.symlink_to(outside)
        response=self.client.get(self.url,headers=self.owner)
        self.assertEqual(response.status_code,409)
        self.assertNotIn(b'UNRELATED PRIVATE',response.content)
        self.assertEqual(self.client.get('/api/interaction/artifacts/not-a-reference',headers=self.owner).status_code,409)
        self.assertEqual(self.client.get(self.url).status_code,401)


if __name__=='__main__':unittest.main()
