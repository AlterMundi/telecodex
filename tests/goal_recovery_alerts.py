#!/usr/bin/env python3
"""Actual loopback Alerts upserts/private-file handoff, no native/model work."""
import copy
import http.server
import json
from pathlib import Path
import subprocess
import sqlite3
import sys
import tempfile
import threading
import time
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from scripts.goal_recovery_alerts import publish
from scripts.activity_indicator import recovery_line, goal_recovery_state


class GoalAlerts(unittest.TestCase):
    def test_private_adapter_http_replay_resolution_binding_and_safe_failure(self):
        calls = []
        accepted = {}
        class Handler(http.server.BaseHTTPRequestHandler):
            def do_PUT(self):
                value = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
                calls.append(value)
                if self.headers.get('Authorization') != 'Bearer fictional-local-token':
                    self.send_error(401)
                    return
                key = (value['incident_id'], value['revision'])
                old = accepted.get(key)
                if old is not None and old != value:
                    self.send_error(409)
                    return
                accepted[key] = value
                result = {'schema':'daimon-alerts/v1','producer_id':value['producer_id'],
                          'incident_id':value['incident_id'],'revision':value['revision'],
                          'accepted_at':int(time.time()),'idempotent':old is not None}
                raw = json.dumps(result).encode()
                self.send_response(200 if old else 201)
                self.send_header('Content-Length',str(len(raw)))
                self.end_headers()
                self.wfile.write(raw)
            def log_message(self,*args): pass
        server = http.server.ThreadingHTTPServer(('127.0.0.1',0),Handler)
        thread = threading.Thread(target=server.serve_forever,daemon=True)
        thread.start()
        try:
            with tempfile.TemporaryDirectory() as directory:
                root=Path(directory)
                token=root/'token';token.write_text('fictional-local-token');token.chmod(0o600)
                config={'base_url':f'http://127.0.0.1:{server.server_port}','token_file':str(token),
                        'producer_id':'fictional-recovery','receiving_scope':'fictional-human','resource_ref':'fictional-native-goals'}
                projection={'schema':'daimon-alerts/v1','producer_id':config['producer_id'],'incident_id':'episode-1','operation_id':'episode-1',
                    'revision':1,'receiving_scope':config['receiving_scope'],'resource_ref':config['resource_ref'],'phase':'goal_recovery_pending',
                    'severity':'warning','problem_code':'provider_error','summary':'A capacity recovery remains pending.',
                    'action':'Inspect retained state; acknowledgment does not resume work.','source_at':int(time.time())-1000,
                    'updated_at':int(time.time()),'processing_state':'open','work_preserved':True}
                self.assertFalse(publish(config,projection)['idempotent'])
                self.assertTrue(publish(config,projection)['idempotent'])
                self.assertEqual(calls[0],calls[1],'Lost upsert reply must preserve exact content')
                foreign={**projection,'receiving_scope':'another-human'}
                with self.assertRaises(ValueError):publish(config,foreign)
                self.assertEqual(len(calls),2,'Binding rejection must happen before HTTP')
                private=root/'producer.json';private.write_text(json.dumps(config));private.chmod(0o600)
                resolved={**copy.deepcopy(projection),'revision':2,'processing_state':'resolved','phase':'goal_recovery_returned'}
                script=Path(__file__).resolve().parents[1]/'scripts/goal_recovery_alerts.py'
                result=subprocess.run([sys.executable,str(script),'--config',str(private)],input=json.dumps(resolved),capture_output=True,text=True,timeout=8)
                self.assertEqual(result.returncode,0,result.stderr)
                self.assertEqual(json.loads(result.stdout)['revision'],2)
                token.chmod(0o644)
                failed=subprocess.run([sys.executable,str(script),'--config',str(private)],input=json.dumps(resolved),capture_output=True,text=True,timeout=8)
                self.assertNotEqual(failed.returncode,0)
                self.assertNotIn('fictional-local-token',failed.stdout+failed.stderr)
                self.assertEqual(len(calls),3)
        finally:
            server.shutdown();server.server_close();thread.join()

    def test_status_projection_keeps_current_owner_and_native_binding(self):
        db=sqlite3.connect(':memory:')
        db.executescript("""CREATE TABLE users(tg_user_id INTEGER,allowed INTEGER);
          INSERT INTO users VALUES(100,1),(101,1);
          CREATE TABLE sessions(id INTEGER,creator_user_id INTEGER,chat_id INTEGER,thread_id INTEGER,codex_thread_id TEXT,force_fresh_thread INTEGER);
          INSERT INTO sessions VALUES(1,100,100,7,'native-a',0),(2,101,101,8,'native-b',0);
          CREATE TABLE goal_retries(human_id INTEGER,session_id INTEGER,native_thread TEXT,phase TEXT,next_at INTEGER,attempts INTEGER);
          INSERT INTO goal_retries VALUES(100,1,'native-a','waiting',220,3),(101,2,'native-b','uncertain',0,1);
        """)
        self.assertEqual(goal_recovery_state(db,100,100,7,'native-a'),{'enabled':None,'state':'waiting','next_attempt_at':None,'attempts':3})
        self.assertIsNone(goal_recovery_state(db,100,101,8,'native-b'))
        db.executescript("""CREATE TABLE bot_state(key TEXT,value TEXT);
          INSERT INTO bot_state VALUES('goal_recovery_backend','{"enabled":false}');
          CREATE TABLE goal_delegations(human_id INTEGER,doc TEXT);
          INSERT INTO goal_delegations VALUES(100,'{"recovery":true,"scope":"owned_sessions"}');
        """)
        disabled=goal_recovery_state(db,100,100,7,'native-a')
        self.assertFalse(disabled['enabled'])
        self.assertIsNone(disabled['next_attempt_at'])
        self.assertNotIn('next check',recovery_line(disabled))
        db.execute("UPDATE bot_state SET value=?",(json.dumps({'enabled':True}),))
        self.assertTrue(goal_recovery_state(db,100,100,7,'native-a')['enabled'])
        db.execute("INSERT INTO bot_state VALUES('goal_recovery_policy:100:100:7','{\"enabled\":false}')")
        self.assertFalse(goal_recovery_state(db,100,100,7,'native-a')['enabled'])
        db.execute("UPDATE sessions SET codex_thread_id='replacement' WHERE id=1")
        self.assertIsNone(goal_recovery_state(db,100,100,7,'replacement'))
        db.close()

    def test_recovery_status_uses_retained_labels_and_relative_time(self):
        self.assertIn('next check in 2m',recovery_line({'enabled':True,'state':'waiting','next_attempt_at':220},now=100))
        self.assertIn('automatic replay held',recovery_line({'state':'uncertain'}))
        self.assertIn('continuation accepted',recovery_line({'state':'acknowledged'}))
        self.assertNotIn('complete',recovery_line({'state':'acknowledged'}))
        self.assertIsNone(recovery_line({'state':'arbitrary provider text'}))


if __name__=='__main__':unittest.main()
