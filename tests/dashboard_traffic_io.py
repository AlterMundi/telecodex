"""Real traffic journal, Alerts HTTP and receiving-auth boundary. No Telegram sends."""
from contextlib import closing
import importlib.util
import json
from pathlib import Path
import sqlite3
import sys
import time
import unittest

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
spec = importlib.util.spec_from_file_location('alerts_fixture', ROOT/'tests/dashboard_alerts_io.py')
fixture = importlib.util.module_from_spec(spec);spec.loader.exec_module(fixture)
from dashboard.traffic import Traffic
from dashboard.alerts import AlertsUnavailable
from scripts.bot_traffic import Journal, private_token
from fastapi.testclient import TestClient
from dashboard.app import create_app


class TrafficHTTP(fixture.Receiving):
    # Reuse the real published core setup; do not inherit unrelated test cases.
    def setUp(self):
        super().setUp()
        self.bot_token = self.root/'bot-token'
        self.bot_token.write_text('123456:'+'SYNTHETIC_ONLY_'*3);self.bot_token.chmod(0o600)
        self.meter_config = self.root/'meter.toml'
        self.meter_config.write_text(f'port=8081\ntoken_file="{self.bot_token}"\nstate_dir="{self.root}/capture"\n')
        self.meter_config.chmod(0o600)
        self.journal = Journal(self.root/'capture')
        self.journal.bind_bot(private_token(self.bot_token).hex())
        self.provider.receiver = {'telegram':{'bot_token_file':str(self.bot_token),'api_base':'http://127.0.0.1:8081'}}
        self.provider.bindings = lambda: [{'chat_id':100,'thread_id':7,'session_title':'Owned topic'}]
        self.config['traffic'] = dict(enabled=True,meter_config=str(self.meter_config),alerts={
            'enabled':True,'base_url':self.base,'token_file':str(self.token),
            'producer_id':'fixture','observer_id':'fixture','receiving_scope':'human'})
        self.health = {'ok':True,'dropped_records':0}
        self.make_traffic()
    def make_traffic(self):
        self.traffic = Traffic(self.config,self.provider,health=lambda:self.health)
    def tearDown(self):
        self.journal.close();super().tearDown()
    def attempt(self, method='sendMessage', limited=False, topic=7):
        ident=self.journal.start('bridge',123,method)
        response={'ok':False,'error_code':429,'parameters':{'retry_after':120}} if limited else {'ok':True,'result':True}
        self.journal.finish(ident,{'chat_id':100,'message_thread_id':topic},json.dumps(response).encode(),
                            (10,20),429 if limited else 200,1,'response',True)
    def own_incidents(self):
        rows,_=self.client.collection('/v1/incidents','IncidentPage')
        return [r for r in rows if r['projection']['phase']=='bot-traffic']
    def test_traffic_cooldown_requires_same_method_recovery_and_recurrence(self):
        self.attempt(limited=True);self.attempt('sendChatAction');self.attempt('sendChatAction',topic=999)
        self.assertFalse(self.traffic.notification_ready())
        self.traffic.tick();view=self.traffic.snapshot()
        self.assertEqual(view['cooldown']['state'],'active');self.assertFalse(self.traffic.notification_ready())
        self.assertEqual(view['totals']['requests'],3);self.assertEqual(view['topics'],[{'name':'Owned topic','requests':2,'rate_limited':1}])
        self.assertNotIn('chat_id',json.dumps(view))
        incident=self.own_incidents()[0]['projection'];self.assertEqual(incident['processing_state'],'open')
        self.make_traffic();self.traffic.tick()
        self.assertEqual(len(self.own_incidents()),1)
        # Expiring a deadline is not successful sending.
        self.traffic.state['blocked']['sendMessage']['until']=time.time()-1
        self.traffic.tick();self.assertEqual(self.traffic.snapshot()['cooldown']['state'],'awaiting_recovery')
        self.assertTrue(self.traffic.notification_ready())
        self.attempt();self.traffic.tick()
        result=self.own_incidents()[0]['projection']
        self.assertEqual(result['incident_id'],incident['incident_id']);self.assertGreater(result['revision'],incident['revision'])
        self.assertEqual(result['processing_state'],'resolved')
        self.attempt(limited=True);self.traffic.tick()
        self.assertEqual(len(self.own_incidents()),2)
    def test_lost_acceptance_preserves_ordered_open_resolution_across_restart(self):
        real=self.traffic.client.call
        lost=[True]
        def call(method,path,shape,value=None):
            result=real(method,path,shape,value)
            if '/incidents/' in path and lost[0]:
                lost[0]=False;raise AlertsUnavailable('service_unavailable')
            return result
        self.traffic.client.call=call
        self.attempt(limited=True);self.traffic.tick()
        self.assertEqual(self.traffic.snapshot()['alerts_state'],'unavailable')
        original=self.traffic.state['outbox'][0].copy()
        self.attempt();self.make_traffic();self.traffic.tick()
        self.assertEqual(self.traffic.state['outbox'],[])
        result=self.own_incidents()[0]
        self.assertEqual(result['projection']['processing_state'],'resolved')
        self.assertEqual(result['projection']['incident_id'],original['incident_id'])
        self.assertEqual(len(result['deliveries']),2)
    def test_collector_outage_and_coverage_loss(self):
        self.health=None;self.traffic.tick();self.assertEqual(len(self.own_incidents()),0)
        self.traffic.tick();self.assertEqual(self.own_incidents()[0]['projection']['processing_state'],'open')
        self.health={'ok':True,'dropped_records':2};self.traffic.tick()
        self.assertEqual(self.traffic.snapshot()['dropped_records'],2)
        self.assertEqual({r['projection']['problem_code'] for r in self.own_incidents()},{'observation_unreadable'})
        self.traffic.state['loss_at']=time.time()-61;self.traffic.tick()
        self.assertTrue(all(r['projection']['processing_state']=='resolved' for r in self.own_incidents()))
        self.health=None;self.make_traffic();self.traffic.tick();self.traffic.tick()
        self.assertEqual(len(self.own_incidents()),3)
    def test_authenticated_traffic_survives_native_failure_and_revocation(self):
        self.attempt();self.traffic.tick()
        with TestClient(create_app(self.config,self.provider,lambda raw,bot:100,alerts=self.receiver,
                                   traffic=self.traffic),base_url='https://example.test') as client:
            self.assertEqual(client.get('/api/traffic').status_code,401)
            reply=client.post('/api/session',json={'init_data':'fiction'},headers={'origin':'https://example.test'})
            token=reply.json()['session_token'];headers={'Authorization':'Bearer '+token}
            self.assertEqual(client.get('/api/state',headers=headers).status_code,503)
            self.assertEqual(client.get('/api/traffic',headers=headers).json()['totals']['requests'],1)
            self.provider.allowed=False
            self.assertEqual(client.get('/api/traffic',headers=headers).status_code,401)
    def test_repeated_transport_failures_recover_only_after_same_method_success(self):
        for _ in range(3):
            ident=self.journal.start('bridge',123,'sendMessage')
            self.journal.finish(ident,{},b'',(0,0),None,1,'upstream_unknown',False)
        self.attempt('getUpdates');self.traffic.tick()
        self.assertEqual(self.traffic.snapshot()['failed_methods'],['sendMessage'])
        self.assertEqual(self.traffic.snapshot()['totals']['failures'],3)
        self.assertEqual(self.own_incidents()[0]['projection']['processing_state'],'open')
        self.attempt();self.traffic.tick()
        self.assertEqual(self.own_incidents()[0]['projection']['processing_state'],'resolved')

    def test_bot_binding_change_refuses_capture(self):
        with self.assertRaises(ValueError):self.journal.bind_bot('wrong-bot')
        self.journal.db.execute("UPDATE settings SET value='other' WHERE key='bot_fingerprint'");self.journal.db.commit()
        self.traffic.tick();self.assertEqual(self.traffic.snapshot()['state'],'unavailable')
    def test_prior_state_without_transport_map_reopens(self):
        from dashboard.private_state import write_json
        self.traffic.state.pop('transport_failed');write_json(self.traffic.state_path,self.traffic.state)
        self.make_traffic();self.traffic.tick()
        self.assertEqual(self.traffic.snapshot()['state'],'available')

    def test_delivery_defers_before_claim_and_resumes_once(self):
        self.receiver.dispatch_ready=lambda:False
        self.receiver.tick();self.assertEqual(self.sender.calls,[])
        self.assertEqual(self.incident()['deliveries'][0]['state'],'pending')
        self.receiver.dispatch_ready=lambda:True
        self.receiver.tick();self.receiver.tick()
        self.assertEqual(len(self.sender.calls),1)


# Inherited Alerts cases have their own suite; this suite runs only affected traffic cases.
for name in dir(fixture.Receiving):
    if name.startswith('test_') and name not in TrafficHTTP.__dict__:
        setattr(TrafficHTTP,name,None)
if __name__=='__main__':unittest.main()
