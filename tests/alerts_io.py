"""Real Alerts HTTP/SQLite receiving journeys, with a deliberately mocked sender."""
from contextlib import closing
import hashlib
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import tempfile
import time
import unittest

ROOT=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(ROOT))
from dashboard.alerts import Receiver,AlertsUnavailable
from dashboard.provider import Provider as ActualProvider
from dashboard.app import create_app
from fastapi.testclient import TestClient

CORE=Path(os.environ.get('ALERTS_CORE_SOURCE',ROOT.parent/'daimon-alerts'))

class Sender:
    def __init__(self): self.calls=[]; self.reply={'ok':True,'result':{'chat':{'id':100},'message_id':17}}
    def verify(self): pass
    def send(self,projection):
        self.calls.append(('sendMessage',{'text':projection['processing_state'].upper()}))
        if self.reply is None: return {'outcome':'uncertain'}
        return {'outcome':'delivered','message_ref':'telegram-17','destination_ref':'telegram-100'}

@unittest.skipUnless((CORE/'target/debug/daimon-alerts').is_file(),'qualified core fixture binary required')
class Adapter(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory(); self.directory=Path(self.temp.name); os.chmod(self.directory,0o700)
        with socket.socket() as stream: stream.bind(('127.0.0.1',0)); port=stream.getsockname()[1]
        self.base=f'http://127.0.0.1:{port}'; token='fictional-receiving-token-0000000000000000000000000'
        self.token=self.directory/'token'; self.token.write_text(token); self.token.chmod(0o600)
        actor=dict(actor_id='receiving-human',token_sha256=hashlib.sha256(token.encode()).hexdigest(),producers=['fixture'],scopes=['human','other'],adapters=['telegram'],grants=['publish','read','status','acknowledge','deliver'])
        config=dict(database=str(self.directory/'core.sqlite'),listen=f'127.0.0.1:{port}',freshness_seconds=60,max_incidents=100,retention_seconds=600,actors=[actor],routes=[dict(receiving_scope='human',adapter_id='telegram')])
        path=self.directory/'core.json'; path.write_text(json.dumps(config)); path.chmod(0o600)
        self.process=subprocess.Popen([str(CORE/'target/debug/daimon-alerts'),'serve',str(path)],stderr=subprocess.PIPE)
        import sqlite3
        db=self.directory/'bot.sqlite'
        with closing(sqlite3.connect(db)) as con: con.execute('CREATE TABLE users(tg_user_id INTEGER,allowed INTEGER)');con.execute('INSERT INTO users VALUES(100,1)'); con.commit()
        self.bot={'db_path':str(db)}; self.sender=Sender()
        outer=self
        class Binding:
            human=100
            binding_bytes=b'fictional-receiving-authority'
            receiver={'db_path':str(db)}
            allowed=True
            def assert_binding(self): pass
            def authorized(self,human): return self.allowed and human==100
        self.provider=Binding()
        self.receiving_config=dict(state_dir=str(self.directory/'receiver'),bot_id=123,bot_username='FictionalBot',alerts=dict(base_url=self.base,token_file=str(self.token),receiving_scope='human',adapter_id='telegram',deliver=True,interval_seconds=15))
        self.receiver=Receiver(self.receiving_config,self.provider,sender=self.sender)
        self.provider.alerts_receiver=self.receiver
        for _ in range(100):
            try: self.receiver.client.call('GET','/v1/status','Status'); break
            except AlertsUnavailable: time.sleep(.02)
        self.value=dict(schema='daimon-alerts/v1',producer_id='fixture',incident_id='credit-1',operation_id='batch-1',revision=1,receiving_scope='human',phase='index',resource_ref='worker',severity='error',problem_code='payment_required',summary='Fictional batch pending: credit required.',action='Restore credit, then explicitly resume.',source_at=int(time.time()),updated_at=int(time.time()),processing_state='open',work_preserved=True)
        self.receiver.client.call('PUT','/v1/incidents/fixture/credit-1','Acceptance',self.value)
    def tearDown(self): self.process.terminate(); self.process.communicate(timeout=5); self.temp.cleanup()
    def test_send_receipt_resolution_and_scope(self):
        other=dict(self.value,incident_id='other',receiving_scope='other')
        self.receiver.client.call('PUT','/v1/incidents/fixture/other','Acceptance',other)
        self.receiver.tick(); self.assertEqual(len(self.receiver.snapshot()['incidents']),1)
        self.assertEqual(len(self.sender.calls),1)
        self.receiver.tick(); self.assertEqual(len(self.sender.calls),1)
        view=self.receiver.snapshot()['incidents'][0]; self.assertEqual(view['projection']['processing_state'],'open'); self.assertEqual(view['deliveries'][0]['state'],'delivered')
        self.receiver.acknowledge('fixture','credit-1','ack-1'); self.receiver.acknowledge('fixture','credit-1','ack-1')
        self.receiver.tick(); self.assertEqual(len(self.receiver.snapshot()['incidents'][0]['acknowledgments']),1)
        self.receiver.client.call('PUT','/v1/incidents/fixture/credit-1','Acceptance',dict(self.value,revision=2,processing_state='resolved'))
        self.receiver.tick(); self.assertEqual(len(self.sender.calls),2)
        self.assertIn('RESOLVED',self.sender.calls[-1][1]['text'])
        view=ActualProvider.alerts(self.provider)
        self.assertEqual(view['incidents'][0]['projection']['processing_state'],'resolved')
    def test_uncertain_and_receipt_loss_never_repeat(self):
        self.sender.reply=None
        self.receiver.tick(); self.receiver.tick()
        self.assertEqual(len(self.sender.calls),1)
        self.assertEqual(self.receiver.snapshot()['incidents'][0]['deliveries'][0]['state'],'uncertain')
    def test_lost_service_receipt_reconciles_without_send(self):
        original=self.receiver.client.call
        def fail(method,path,shape,value=None):
            if path.endswith('/receipt'): raise AlertsUnavailable('test_receipt_loss')
            return original(method,path,shape,value)
        self.receiver.client.call=fail
        self.receiver.tick(); self.assertEqual(self.receiver.snapshot()['state'],'unavailable')
        self.receiver=Receiver(self.receiving_config,self.provider,sender=self.sender)
        self.receiver.tick()
        self.assertEqual(len(self.sender.calls),1)
        current=self.receiver.client.call('GET','/v1/incidents/fixture/credit-1','Incident')
        self.assertEqual(current['deliveries'][0]['state'],'delivered')
    def test_http_auth_ack_and_grant_revocation(self):
        receiving=self.receiver
        class Provider:
            allowed=True
            def authorized(self,human): return self.allowed and human==100
            def snapshot(self): return {'sessions':[]}
            def alerts(self):
                self.alerts_receiver=receiving
                return ActualProvider.alerts(self)
            def acknowledge_alert(self,*args): return receiving.acknowledge(*args)
            def close(self): pass
        receiving.tick()
        provider=Provider()
        config=dict(state_dir=str(self.directory/'dashboard'),origin='https://example.test',prefix='/sessions',bot_id=123)
        client=TestClient(create_app(config,provider,lambda raw,bot:int(raw)),base_url='https://example.test')
        headers={'origin':'https://example.test'}
        payload={'producer_id':'fixture','incident_id':'credit-1','request_id':'ack-2'}
        self.assertEqual(client.post('/api/alerts/acknowledge',json=payload,headers=headers).status_code,401)
        session=client.post('/api/session',json={'init_data':'100'},headers=headers).json()
        headers['authorization']='Bearer '+session['session_token']
        state=client.get('/api/state',headers=headers).json()
        self.assertEqual(state['alerts']['incidents'][0]['projection']['problem_code'],'payment_required')
        self.assertEqual(client.post('/api/alerts/acknowledge',json=payload,headers=headers).status_code,403)
        headers['x-dashboard-csrf']=state['csrf']
        first=client.post('/api/alerts/acknowledge',json=payload,headers=headers)
        self.assertEqual(first.status_code,200); self.assertEqual(client.post('/api/alerts/acknowledge',json=payload,headers=headers).json(),first.json())
        provider.allowed=False
        self.assertEqual(client.post('/api/alerts/acknowledge',json=payload,headers=headers).status_code,401)
        client.close()

if __name__=='__main__': unittest.main(verbosity=2)
