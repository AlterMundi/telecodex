#!/usr/bin/env python3
"""Current-turn artifact contracts and receipts over native pipes, HTTP and SQLite."""
import collections
import http.server
import json
import os
import pathlib
import signal
import sqlite3
import subprocess
import sys
import tempfile
import threading
import time


def scenario(binary, mode):
    with tempfile.TemporaryDirectory(prefix='telecodex-artifacts-') as name:
        root = pathlib.Path(name)
        updates, calls, uploads = collections.deque(), [], []
        release_upload = threading.Event()
        class Telegram(http.server.BaseHTTPRequestHandler):
            def do_POST(self):
                raw = self.rfile.read(int(self.headers.get('Content-Length', '0')))
                method = self.path.rsplit('/', 1)[-1]
                data = {} if method == 'sendDocument' else json.loads(raw or b'{}')
                calls.append((method, data))
                if method == 'getMe': result = dict(id=123, is_bot=True, first_name='Fixture', has_topics_enabled=True)
                elif method == 'getUpdates':
                    result = [updates.popleft()] if updates else []
                    if not result: time.sleep(.02)
                elif method == 'sendDocument':
                    # Inspect actual multipart bytes, including topic and distinct generated contents.
                    assert b'name="chat_id"\r\n\r\n100' in raw and b'name="message_thread_id"\r\n\r\n7' in raw
                    assert b'PRIVATE-HISTORICAL' not in raw
                    uploads.append(raw)
                    if mode in ('rejected', 'uncertain', 'interrupted') and len(uploads) == 2:
                        if mode == 'interrupted':
                            release_upload.wait(timeout=20)
                            self.connection.shutdown(2); return
                        if mode == 'uncertain':
                            self.connection.shutdown(2); return
                        body=json.dumps(dict(ok=False,description='synthetic upload rejected')).encode()
                        self.send_response(400);self.send_header('Content-Length', str(len(body)));self.end_headers();self.wfile.write(body);return
                    result = dict(message_id=100+len(uploads),chat=dict(id=100,type='private'),message_thread_id=7)
                elif method in ('sendMessage', 'editMessageText'):
                    result=dict(message_id=len(calls),chat=dict(id=100,type='private'),message_thread_id=7)
                else: result=True
                body=json.dumps(dict(ok=True,result=result)).encode()
                self.send_response(200);self.send_header('Content-Length',str(len(body)));self.end_headers()
                try:self.wfile.write(body)
                except (BrokenPipeError,ConnectionResetError):pass
            def log_message(self,*_):pass
        server=http.server.ThreadingHTTPServer(('127.0.0.1',0),Telegram)
        threading.Thread(target=server.serve_forever,daemon=True).start()
        native=root/'codex'
        native.write_text('''#!/usr/bin/python3
import json,pathlib,re,sys
root=pathlib.Path(__file__).parent
if sys.argv[1:]==['login','status']:print('Logged in using ChatGPT');sys.exit(0)
for line in sys.stdin:
 r=json.loads(line)
 with (root/'rpc.jsonl').open('a') as f:f.write(json.dumps(r)+'\\n')
 if 'id' not in r:continue
 m=r['method'];p=r.get('params',{})
 if m=='initialize':result={}
 elif m=='account/rateLimits/read':result={}
 elif m in ('thread/start','thread/resume'):
  # This fixture deliberately ignores resume overrides for the loaded thread.
  result={'thread':{'id':'native-artifact-thread'}}
 elif m=='turn/start':
  contract=p['additionalContext']['telecodex.runtime']
  assert contract['kind']=='application'
  out=pathlib.Path(re.search(r'\\n([^\\n]+/out)\\n',contract['value']).group(1))
  count=len(list(root.glob('completed-*')))+1
  (root/('completed-'+str(count))).touch()
  mode=(root/'mode').read_text()
  answer='Generated requested deliverables.'
  if mode=='stale':answer=str(root/'.telecodex/turns/historical/out/private.txt')
  elif mode=='missing':answer=str(out/'missing.txt')
  elif mode=='symlink':(out/'link.txt').symlink_to(root/'private.txt')
  elif mode=='nested':
   (out/'nested').mkdir();(out/'nested/file.txt').write_text('nested output')
   answer=str(out/'nested/file.txt')
  else:
   for index in range(2):
    (out/(str(index)+'.txt')).write_text('TURN-'+str(count)+'-FILE-'+str(index))
  print(json.dumps({'id':r['id'],'result':{'turn':{'id':'native-turn-'+str(count)}}}),flush=True)
  print(json.dumps({'method':'item/completed','params':{'threadId':'native-artifact-thread','turnId':'native-turn-'+str(count),'item':{'type':'agentMessage','id':'answer','phase':'final_answer','text':answer}}}),flush=True)
  print(json.dumps({'method':'turn/completed','params':{'threadId':'native-artifact-thread','turn':{'id':'native-turn-'+str(count),'status':'completed'}}}),flush=True)
  continue
 else:raise RuntimeError('unexpected RPC '+m)
 print(json.dumps({'id':r['id'],'result':result}),flush=True)
''');native.chmod(0o700)
        (root/'mode').write_text(mode);(root/'private.txt').write_text('PRIVATE-HISTORICAL')
        historic=root/'.telecodex/turns/historical/out';historic.mkdir(parents=True)
        (historic/'private.txt').write_text('PRIVATE-HISTORICAL')
        token=root/'token';token.write_text('synthetic-token');token.chmod(0o600)
        config=root/'config.toml';config.write_text(f'''db_path="{root}/state.sqlite"
startup_admin_ids=[100]
poll_timeout_seconds=1
[telegram]
bot_token_file="{token}"
api_base="http://127.0.0.1:{server.server_port}"
lifecycle_notifications=false
[codex]
binary="{native}"
default_cwd="{root}"
auto_attach_latest_history=false
import_cli_history=false
import_desktop_history=false
''')
        def start():return subprocess.Popen([binary,str(config)],cwd=root,stdout=subprocess.PIPE,stderr=subprocess.PIPE,env=dict(os.environ,HOME=str(root),CODEX_HOME=str(root/'codex-home'),RUST_LOG='warn'))
        process=start();db=None
        def wait(predicate):
            deadline=time.monotonic()+20
            while time.monotonic()<deadline:
                if predicate():return
                if process.poll() is not None:
                    raise AssertionError('bridge exited: '+process.communicate(timeout=5)[1].decode())
                time.sleep(.03)
            raise AssertionError('artifact journey timed out: '+repr(calls[-10:]))
        def send(number):updates.append(dict(update_id=number,message=dict(message_id=number,message_thread_id=7,chat=dict(id=100,type='private'),text='Make turn files '+str(number),**{'from':dict(id=100,is_bot=False,first_name='Fixture')})))
        try:
            wait(lambda:any(m=='setMyCommands' for m,_ in calls));send(1)
            wait(lambda:(root/'state.sqlite').exists())
            db=sqlite3.connect(root/'state.sqlite')
            if mode == 'interrupted':
                wait(lambda:len(uploads) == 2)
                process.kill();process.communicate(timeout=10);release_upload.set()
                db.execute("UPDATE turns SET started_at=datetime('now','-1 hour') WHERE status='running'")
                db.execute("UPDATE sessions SET updated_at=datetime('now','-1 hour')")
                # Simulate elapsed crash-recovery lease; never bypass a real live owner.
                db.execute("UPDATE app_instance_lock SET heartbeat_at=datetime('now','-1 hour')")
                db.commit()
                process=start()
                wait(lambda:sum(m=='setMyCommands' for m,_ in calls)==2)
                wait(lambda:db.execute("SELECT status FROM turns").fetchone()[0]=='failed')
                time.sleep(.3)
                assert len(uploads)==2, 'interrupted upload was replayed'
                assert db.execute("SELECT COUNT(*) FROM audit_log WHERE action='artifact_delivery_attempted'").fetchone()[0]==2
                assert db.execute("SELECT COUNT(*) FROM audit_log WHERE action='artifact_delivery_accepted'").fetchone()[0]==1
                assert len(list((root/'.telecodex/turns').glob('1-*/out')))==1
                assert db.execute('SELECT codex_thread_id FROM sessions WHERE thread_id=7').fetchone()[0]=='native-artifact-thread'
                rpc=[json.loads(line) for line in (root/'rpc.jsonl').read_text().splitlines()]
                assert sum(r.get('method')=='turn/start' for r in rpc)==1
                print(json.dumps(dict(scenario='artifacts_interrupted',uploads=2,accepted=1,restart_replay=False)),flush=True)
                return
            expected='completed' if mode=='success' else 'failed'
            wait(lambda:db.execute('SELECT COUNT(*) FROM turns WHERE status=?',(expected,)).fetchone()[0]==1)
            if mode=='success':
                send(2);wait(lambda:db.execute("SELECT COUNT(*) FROM turns WHERE status='completed'").fetchone()[0]==2)
                assert len(uploads)==4
                for turn in (1,2):
                    for index in (0,1):assert sum(('TURN-'+str(turn)+'-FILE-'+str(index)).encode() in raw for raw in uploads)==1
                rpc=[json.loads(line) for line in (root/'rpc.jsonl').read_text().splitlines()]
                starts=[r for r in rpc if r.get('method')=='turn/start']
                assert len(starts)==2 and starts[0]['params']['threadId']==starts[1]['params']['threadId']
                assert any(r.get('method')=='thread/resume' for r in rpc)
                assert starts[0]['params']['additionalContext']!=starts[1]['params']['additionalContext']
                assert db.execute("SELECT COUNT(*) FROM audit_log WHERE action='artifact_delivery_accepted'").fetchone()[0]==4
            else:
                wait(lambda:any('Turn failed:' in d.get('text','') for m,d in calls if m in ('sendMessage','editMessageText')))
                assert len(uploads)==(2 if mode in ('rejected','uncertain') else 0)
                assert db.execute("SELECT COUNT(*) FROM audit_log WHERE action='artifact_delivery_accepted'").fetchone()[0]==(1 if uploads else 0)
                assert len(list((root/'.telecodex/turns').glob('1-*/out')))==(0 if mode in ('stale','missing') else 1),'failed output retention mismatch'
                process.send_signal(signal.SIGINT);process.communicate(timeout=10);process=start()
                wait(lambda:sum(m=='setMyCommands' for m,_ in calls)==2)
                time.sleep(.3)
                assert len(uploads)==(2 if mode in ('rejected','uncertain') else 0),'restart replayed delivery'
                assert db.execute('SELECT codex_thread_id FROM sessions WHERE thread_id=7').fetchone()[0]=='native-artifact-thread'
                assert db.execute('SELECT COUNT(*) FROM incoming_updates').fetchone()[0]==1
            assert (historic/'private.txt').read_text()=='PRIVATE-HISTORICAL'
            print(json.dumps(dict(scenario='artifacts_'+mode,uploads=len(uploads),native_binding_preserved=True,historical_files_untouched=True)),flush=True)
        finally:
            if db:db.close()
            if process.poll() is None:process.send_signal(signal.SIGINT);process.communicate(timeout=10)
            release_upload.set()
            server.shutdown();server.server_close()

if __name__=='__main__':
    binary=str(pathlib.Path(sys.argv[1]).resolve())
    for mode in sys.argv[2:] or ('success','stale','missing','nested','symlink','rejected','uncertain','interrupted'):scenario(binary,mode)
