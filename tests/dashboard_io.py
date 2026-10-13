"""HTTP, SQLite, signature and real Unix-control journeys for the private dashboard."""
import base64
from contextlib import closing
import json
import http.server
import subprocess
import os
from pathlib import Path
import socket
import sqlite3
import sys
import tempfile
import threading
import time
import unittest
from unittest.mock import patch
from urllib.parse import urlencode

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
from cryptography.hazmat.primitives import serialization
from fastapi.testclient import TestClient
from dashboard.app import create_app, validate_launch
from dashboard.provider import Provider, control


class Receiver:
    def __init__(self):
        self.human = 100
        self.allowed = True
        self.calls = []
        self.failed = False
    def authorized(self, human): return self.allowed and human == self.human
    def snapshot(self): return {'schema': 'test', 'sessions': [], 'stop': {'state':'no_operation'}}
    def stop(self, nonce):
        self.calls.append(nonce)
        if self.failed: raise TimeoutError()
        return {'state':'stopping', 'operation':'native-operation'}
    def close(self): pass


class DashboardHTTP(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.config = {'state_dir':self.tmp.name, 'origin':'https://example.test',
                       'prefix':'/sessions', 'bot_id':123}
        os.chmod(self.tmp.name, 0o700)
        self.provider = Receiver()
        self.app = create_app(self.config,self.provider,lambda raw,bot:int(raw))
        self.client = TestClient(self.app,base_url='https://example.test')
        self.headers = {'origin':'https://example.test'}
    def tearDown(self): self.client.close(); self.tmp.cleanup()
    def login(self):
        response=self.client.post('/api/session',json={'init_data':'100'},headers=self.headers)
        self.assertEqual(response.status_code,200)
        cookie=response.headers['set-cookie']
        self.assertIn('HttpOnly',cookie);self.assertIn('Secure',cookie);self.assertIn('Path=/sessions',cookie)
        # Real reverse proxy strips the prefix upstream while browser cookie scope retains it.
        self.client.cookies.set('telecodex_dashboard',self.client.cookies.get('telecodex_dashboard'),path='/')
        return self.client.get('/api/state').json()['csrf']
    def stop(self,nonce,csrf):
        return self.client.post('/api/stop',json={'nonce':nonce},headers={**self.headers,'x-dashboard-csrf':csrf})
    def test_private_data_and_actions_require_signed_allowed_owner(self):
        self.assertEqual(self.client.get('/api/state').status_code,401)
        self.assertEqual(self.client.post('/api/session',json={'init_data':'101'},headers=self.headers).status_code,403)
        csrf=self.login()
        self.assertEqual(self.client.get('/api/state').status_code,200)
        self.assertEqual(self.stop('action-123456789012','wrong').status_code,403)
        self.provider.allowed=False
        self.assertEqual(self.client.get('/api/state').status_code,401)
        self.assertEqual(self.stop('action-123456789012',csrf).status_code,401)
        self.assertEqual(self.provider.calls,[])
    def test_origin_body_bounds_no_path_override_and_public_shell(self):
        self.assertEqual(self.client.get('/').status_code,200)
        self.assertEqual(self.client.get('/assets/app.js').status_code,200)
        self.assertEqual(self.client.get('/assets/space-grotesk-latin.woff2').status_code,200)
        self.assertIn('SIL',self.client.get('/assets/OFL.txt').text)
        self.assertEqual(self.client.get('/assets/app.py').status_code,404)
        self.assertEqual(self.client.post('/api/session',json={'init_data':'100'}).status_code,403)
        self.assertEqual(self.client.post('/api/session',content=b'x'*16385,headers=self.headers).status_code,413)
        csrf=self.login()
        self.assertEqual(self.client.post('/api/stop',json={'nonce':'action-123456789012','path':'/other'},headers={**self.headers,'x-dashboard-csrf':csrf}).status_code,400)
        self.assertEqual(self.provider.calls,[])
    def test_stop_persists_one_intent_across_http_loss_and_service_restart(self):
        csrf=self.login();nonce='action-123456789012'
        self.assertEqual(self.stop(nonce,csrf).json()['state'],'stopping')
        self.assertEqual(self.stop(nonce,csrf).json()['operation'],'native-operation')
        self.client.close()
        self.client=TestClient(create_app(self.config,self.provider,lambda raw,bot:int(raw)),base_url='https://example.test')
        csrf=self.login()
        self.assertEqual(self.stop(nonce,csrf).json()['operation'],'native-operation')
        self.assertEqual(self.provider.calls,[nonce])
    def test_ambiguous_acceptance_and_crash_intent_are_not_replayed(self):
        csrf=self.login();self.provider.failed=True;nonce='ambiguous-123456789012'
        self.assertEqual(self.stop(nonce,csrf).json()['state'],'unconfirmed')
        self.assertEqual(self.stop(nonce,csrf).json()['state'],'unconfirmed')
        with closing(sqlite3.connect(Path(self.tmp.name)/'dashboard.sqlite3')) as db:
            db.execute('INSERT INTO actions VALUES(?,?,?,NULL)',(100,'crashed-123456789012','requested'));db.commit()
        self.assertEqual(self.stop('crashed-123456789012',csrf).json()['state'],'unconfirmed')
        self.assertEqual(self.provider.calls,[nonce])
    def test_revocation_during_snapshot_denies_response(self):
        self.login()
        def revoke(): self.provider.allowed=False;return {'private':'hidden'}
        self.provider.snapshot=revoke
        response=self.client.get('/api/state')
        self.assertEqual(response.status_code,401);self.assertNotIn('hidden',response.text)


class MenuOperator(unittest.TestCase):
    def test_actual_http_menu_receipt_rollback_scope_and_unsafe_state(self):
        with tempfile.TemporaryDirectory() as temporary:
            root=Path(temporary);root.chmod(0o700)
            calls=[];menu={'type':'commands'}
            class Handler(http.server.BaseHTTPRequestHandler):
                def do_POST(self):
                    nonlocal menu
                    method=self.path.rsplit('/',1)[-1]
                    data=json.loads(self.rfile.read(int(self.headers['content-length'])))
                    calls.append((method,data))
                    if method=='getMe':result={'id':123,'username':'TestBot'}
                    elif method=='getChatMenuButton':result=menu
                    elif method=='setChatMenuButton':menu=data['menu_button'];result=True
                    else:raise ValueError(method)
                    raw=json.dumps({'ok':True,'result':result}).encode()
                    self.send_response(200);self.end_headers();self.wfile.write(raw)
                def log_message(self,*args):pass
            server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Handler)
            threading.Thread(target=server.serve_forever,daemon=True).start()
            try:
                token=root/'token';token.write_text('synthetic-token');token.chmod(0o600)
                receiver=root/'receiver.toml'
                receiver.write_text(f'[telegram]\nbot_token_file="{token}"\napi_base="http://127.0.0.1:{server.server_port}"\n[session_control]\nhuman_id=100\n')
                config=root/'dashboard.toml'
                config.write_text(f'receiver_config="{receiver}"\nstate_dir="{root}"\norigin="https://example.test"\nprefix="/sessions"\nbot_id=123\nbot_username="TestBot"\n')
                def run(operation):
                    return subprocess.run([sys.executable,'-B','-m','dashboard.operator','--config',str(config),operation],capture_output=True,text=True,cwd=Path(__file__).resolve().parents[1])
                self.assertEqual(run('publish-menu').returncode,0)
                self.assertEqual(menu['text'],'Sessions')
                receipt=json.loads((root/'menu-before.json').read_text())
                self.assertEqual(receipt['binding']['human'],100)
                self.assertEqual(receipt['menu'],{'type':'commands'})
                self.assertEqual(run('restore-menu').returncode,0)
                self.assertEqual(menu,{'type':'commands'})
                plain_config = config.read_text()
                config.write_text(plain_config + 'miniapp_short_name="sessions"\n')
                self.assertEqual(run('publish-menu').returncode,0)
                self.assertEqual(menu['web_app']['url'],'https://t.me/TestBot/sessions?mode=compact')
                self.assertEqual(run('restore-menu').returncode,0)
                self.assertEqual(menu,{'type':'commands'})
                before=sum(method=='setChatMenuButton' for method,_ in calls)
                for invalid in ['', '../another', 'sessions?startapp=other', 'a'*65]:
                    config.write_text(plain_config + f'miniapp_short_name={json.dumps(invalid)}\n')
                    self.assertNotEqual(run('publish-menu').returncode,0)
                self.assertEqual(sum(method=='setChatMenuButton' for method,_ in calls),before)
                # A bad current selection must not obstruct a scoped rollback.
                self.assertEqual(run('restore-menu').returncode,0)
                config.write_text(plain_config)
                receiver.write_text(receiver.read_text().replace('human_id=100','human_id=101'))
                before=sum(method=='setChatMenuButton' for method,_ in calls)
                self.assertNotEqual(run('restore-menu').returncode,0)
                self.assertEqual(sum(method=='setChatMenuButton' for method,_ in calls),before)
                receiver.write_text(receiver.read_text().replace('human_id=101','human_id=100'))
                (root/'menu-before.json').unlink();(root/'menu-before.json').symlink_to(token)
                self.assertNotEqual(run('publish-menu').returncode,0)
                self.assertEqual(token.read_text(),'synthetic-token')
            finally:server.shutdown();server.server_close()


class SignedLaunch(unittest.TestCase):
    def test_independently_signed_menu_payload_and_tampering_policy(self):
        key=Ed25519PrivateKey.generate()
        public=key.public_key().public_bytes(serialization.Encoding.Raw,serialization.PublicFormat.Raw)
        def raw(age=0):
            pairs={'auth_date':str(int(time.time())-age),'user':json.dumps({'id':100,'first_name':'Human'})}
            message='123:WebAppData\n'+'\n'.join(f'{k}={v}' for k,v in sorted(pairs.items()))
            pairs['signature']=base64.urlsafe_b64encode(key.sign(message.encode())).decode().rstrip('=')
            pairs['hash']='0'*64
            return urlencode(pairs)
        # Independent fixture key only, never selected by production requests/config.
        with patch('init_data_py.validation._resolve_public_key',return_value=public):
            valid=raw();self.assertEqual(validate_launch(valid,123),100)
            for invalid,bot in [(valid+'&auth_date=0',123),(raw(400),123),(raw(-60),123),
                                (valid.replace('Human','Other'),123),(valid,124)]:
                with self.assertRaises(ValueError):validate_launch(invalid,bot)


class NativeMetadata:
    def status(self, tid): return {'type':'active' if tid=='root' else 'idle'}
    def rpc(self,method,params): return {'goal': {'threadId':params['threadId'],'status':'paused','objective':'Retained objective'}}
    def close(self): pass


class ProviderIO(unittest.TestCase):
    def setUp(self):
        self.tmp=tempfile.TemporaryDirectory();self.root=Path(self.tmp.name);self.root.chmod(0o700)
        self.db=self.root/'bridge.sqlite'
        with closing(sqlite3.connect(self.db)) as db:
            db.executescript('''CREATE TABLE users(tg_user_id INTEGER,allowed INTEGER);
                INSERT INTO users VALUES(100,1),(101,1);
                CREATE TABLE sessions(id INTEGER,chat_id INTEGER,thread_id INTEGER,session_title TEXT,
                    codex_thread_id TEXT,busy INTEGER,updated_at TEXT,creator_user_id INTEGER);
                INSERT INTO sessions VALUES(1,100,7,'Owned','root',1,'now',100),(2,101,8,'Foreign','foreign',1,'now',101);
                CREATE TABLE stop_operations(id TEXT,human INTEGER,phase TEXT,document TEXT);''')
        native=self.root/'native';(native/'sessions').mkdir(parents=True)
        self.rollout=native/'sessions'/'root.jsonl'
        self.rollout.write_text(json.dumps({'type':'session_meta','payload':{'id':'root'}})+'\n')
        self.rollout.chmod(0o600)
        with closing(sqlite3.connect(native/'state_5.sqlite')) as db:
            db.execute('CREATE TABLE threads(id TEXT,rollout_path TEXT,archived INTEGER,source TEXT,agent_nickname TEXT)')
            db.execute('INSERT INTO threads VALUES(?,?,0,?,?)',('root',str(self.rollout),'"cli"','Parent'))
            for child,parent in [('child','root'),('grandchild','child'),('foreign-child','foreign')]:
                db.execute('INSERT INTO threads VALUES(?,?,0,?,?)',(child,'',json.dumps({'subagent':{'thread_spawn':{'parent_thread_id':parent}}}),child))
            db.commit()
        receiver=self.root/'receiver.toml';receiver.write_text(f'db_path="{self.db}"\n[codex]\nbinary="codex"\n[session_control]\nhuman_id=100\nsocket_path="{self.root}/control.sock"\n')
        self.provider=Provider({'receiver_config':str(receiver),'native_home':str(native),'bot_username':'TestBot','storage':{'Host':'/'}})
        self.provider.native=NativeMetadata()
    def tearDown(self):self.provider.close();self.tmp.cleanup()
    def test_native_binding_scope_nested_tree_goal_and_host_data(self):
        result=self.provider.snapshot()
        self.assertEqual([s['title'] for s in result['sessions']],['Owned'])
        node=result['sessions'][0]
        self.assertEqual(node['goal']['status'],'paused')
        self.assertEqual([c['title'] for c in node['children']],['child','grandchild'])
        self.assertNotIn('foreign',json.dumps(result))
        self.assertGreater(result['host']['memory_total'],0)
        self.assertNotIn('rollout_path',json.dumps(result))
        self.assertFalse(self.provider.authorized(101));self.assertFalse(self.provider.authorized(999))
    def test_scope_changes_invalidate_cached_results(self):
        self.provider.snapshot()
        with closing(sqlite3.connect(self.db)) as db:db.execute('UPDATE sessions SET creator_user_id=101 WHERE id=1');db.commit()
        self.assertEqual(self.provider.snapshot()['sessions'],[])
    def test_receiving_grant_change_rejects_old_cookie_and_scheduled_stop(self):
        self.assertTrue(self.provider.authorized(100))
        path=self.provider.receiver_path
        path.write_text(path.read_text().replace('human_id=100','human_id=101'))
        self.assertFalse(self.provider.authorized(100))
        with patch('dashboard.provider.control') as call:
            with self.assertRaises(Exception):self.provider.stop('delayed-123456789012')
            call.assert_not_called()
    def test_unix_provider_carries_the_exact_stable_nonce_once(self):
        path=self.root/'control.sock';server=socket.socket(socket.AF_UNIX);server.bind(str(path));os.chmod(path,0o600);server.listen()
        requests=[]
        def receive():
            with closing(server.accept()[0]) as stream:
                data=stream.recv(4096);requests.append(json.loads(data))
                stream.sendall(b'{"ok":true,"result":{"state":"stopping","operation":"retained"}}\n')
        worker=threading.Thread(target=receive);worker.start()
        result=control(path,'stable-nonce-12345678');worker.join(timeout=2);server.close()
        self.assertEqual(requests,[{'command':'all','nonce':'stable-nonce-12345678'}])
        self.assertEqual(result['operation'],'retained')


if __name__=='__main__':unittest.main()
