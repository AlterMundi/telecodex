#!/usr/bin/env python3
"""Native fork/topic binding journey using synthetic HTTP, stdio RPC and SQLite."""
import collections
import http.server
import html
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


def main(binary):
    with tempfile.TemporaryDirectory(prefix='telecodex-fork-') as directory:
        root = pathlib.Path(directory)
        updates, calls = collections.deque(), []
        class Telegram(http.server.BaseHTTPRequestHandler):
            def do_POST(self):
                data = json.loads(self.rfile.read(int(self.headers.get('Content-Length', '0'))) or b'{}')
                method = self.path.rsplit('/', 1)[-1]
                calls.append((method, data))
                if method == 'getMe':
                    result = dict(id=123, is_bot=True, first_name='Fixture', has_topics_enabled=True)
                elif method == 'getUpdates':
                    result = [updates.popleft()] if updates else []
                    if not result: time.sleep(.02)
                elif method == 'createForumTopic':
                    result = dict(message_thread_id=8, name=data['name'], icon_color=0)
                elif method == 'sendMessage':
                    result = dict(message_id=len(calls), chat=dict(id=data['chat_id'], type='private'), text=data['text'], message_thread_id=data.get('message_thread_id'))
                else: result = True
                raw = json.dumps(dict(ok=True, result=result)).encode()
                self.send_response(200); self.send_header('Content-Length',str(len(raw))); self.end_headers()
                try: self.wfile.write(raw)
                except (BrokenPipeError, ConnectionResetError): pass
            def log_message(self,*_): pass
        server = http.server.ThreadingHTTPServer(('127.0.0.1',0),Telegram)
        threading.Thread(target=server.serve_forever,daemon=True).start()
        native = root/'codex'
        native.write_text('''#!/usr/bin/python3
import json,sys,pathlib
if sys.argv[1:]==['login','status']:
 print('Logged in using ChatGPT');sys.exit(0)
log=pathlib.Path(__file__).with_name('rpc.jsonl')
for line in sys.stdin:
 r=json.loads(line)
 with log.open('a') as f:f.write(json.dumps(r)+'\\n')
 if 'id' not in r:continue
 m=r['method']
 if m=='initialize':result={'userAgent':'fixture'}
 elif m=='thread/turns/list':result={'data':[{'id':'parent-turn','status':'completed'}]}
 elif m=='thread/fork':
  assert r['params']=={'threadId':'parent-thread','excludeTurns':True,'lastTurnId':'parent-turn'}
  result={'thread':{'id':'child-thread'}}
 elif m=='thread/name/set':result={}
 elif m=='thread/resume':result={'thread':{'id':'child-thread'}}
 elif m=='turn/start':
  print(json.dumps({'id':r['id'],'result':{'turn':{'id':'child-turn'}}}),flush=True)
  print(json.dumps({'method':'item/completed','params':{'threadId':'child-thread','turnId':'child-turn','item':{'type':'agentMessage','id':'reply','phase':'final_answer','text':'Child reply'}}}),flush=True)
  print(json.dumps({'method':'turn/completed','params':{'threadId':'child-thread','turn':{'id':'child-turn','status':'completed'}}}),flush=True)
  continue
 elif m=='account/rateLimits/read':result={'rateLimits':{}}
 else:raise RuntimeError('unexpected native RPC '+m)
 print(json.dumps({'id':r['id'],'result':result}),flush=True)
''');native.chmod(0o700)
        token=root/'token';token.write_text('synthetic-fork-token');token.chmod(0o600)
        config=root/'config.toml';config.write_text(f'''db_path="{root}/state.sqlite"
startup_admin_ids=[100,101]
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
        def start():
            return subprocess.Popen([binary,str(config)],cwd=root,stdout=subprocess.PIPE,stderr=subprocess.PIPE,
                env=dict(os.environ,HOME=str(root),CODEX_HOME=str(root/'codex-home'),RUST_LOG='warn'))
        process=start()
        seq=0
        def send(text, user=100, topic=7):
            nonlocal seq
            seq+=1
            updates.append(dict(update_id=seq,message=dict(message_id=seq,message_thread_id=topic,chat=dict(id=100,type='private'),text=text,**{'from':dict(id=user,is_bot=False,first_name='Fixture')})))
        def wait(predicate):
            deadline=time.monotonic()+15
            while time.monotonic()<deadline:
                if predicate():return
                assert process.poll() is None, 'bridge exited'
                time.sleep(.03)
            raise AssertionError('journey timed out: '+repr([c for c in calls if c[0]!='getUpdates']))
        try:
            wait(lambda:any(m=='setMyCommands' for m,_ in calls))
            send('/pwd')
            wait(lambda:any(m=='sendMessage' and str(root) in html.unescape(d['text']) for m,d in calls))
            db=sqlite3.connect(root/'state.sqlite')
            db.execute("UPDATE sessions SET codex_thread_id='parent-thread',force_fresh_thread=0,session_prompt='Keep this preference' WHERE thread_id=7");db.commit()
            before=db.execute('SELECT * FROM sessions WHERE thread_id=7').fetchone()
            send('/fork Shared Resources')
            wait(lambda:any(m=='sendMessage' and 'Fork created:' in d['text'] for m,d in calls))
            child=db.execute('SELECT codex_thread_id,force_fresh_thread,session_prompt,busy FROM sessions WHERE thread_id=8').fetchone()
            assert child==('child-thread',0,'Keep this preference',0),child
            after=db.execute('SELECT * FROM sessions WHERE thread_id=7').fetchone()
            assert before==after,'parent changed'
            notice=db.execute("SELECT value FROM bot_state WHERE key='fork_notice:child-thread'").fetchone()[0]
            assert 'human just created' in notice and 'Shared Resources' in notice
            audit=db.execute("SELECT action FROM audit_log WHERE action LIKE 'fork_%' ORDER BY id").fetchall()
            assert [r[0] for r in audit]==['fork_requested','fork_native_created','fork_topic_created','fork_bound']
            rpc=[json.loads(x) for x in (root/'rpc.jsonl').read_text().splitlines()]
            assert not any(r.get('method')=='turn/start' for r in rpc),'fork started inference'
            assert any(r.get('method')=='thread/name/set' and r['params']['name']=='Shared Resources' for r in rpc)
            # Pending first-turn notice and binding survive a real process restart.
            process.send_signal(signal.SIGINT);process.communicate(timeout=10)
            process=start();time.sleep(.3)
            assert db.execute("SELECT value FROM bot_state WHERE key='fork_notice:child-thread'").fetchone()[0]==notice
            assert db.execute('SELECT codex_thread_id FROM sessions WHERE thread_id=8').fetchone()[0]=='child-thread'
            # Unauthorized user cannot clone another human's history.
            count=sum(m=='createForumTopic' for m,_ in calls)
            send('/fork Not Mine',101)
            wait(lambda:any(m=='sendMessage' and 'creator' in d['text'] for m,d in calls))
            assert sum(m=='createForumTopic' for m,_ in calls)==count
            send('Continue Shared Resources', topic=8)
            wait(lambda:db.execute("SELECT COUNT(*) FROM turns WHERE status='completed'").fetchone()[0]==1)
            assert db.execute("SELECT value FROM bot_state WHERE key='fork_notice:child-thread'").fetchone() is None
            send('Next child message', topic=8)
            wait(lambda:db.execute("SELECT COUNT(*) FROM turns WHERE status='completed'").fetchone()[0]==2)
            rpc=[json.loads(x) for x in (root/'rpc.jsonl').read_text().splitlines()]
            inputs=[r['params']['input'][0]['text'] for r in rpc if r.get('method')=='turn/start']
            assert len(inputs)==2 and '[Conversation origin]' in inputs[0] and 'Shared Resources' in inputs[0]
            assert '[Conversation origin]' not in inputs[1], inputs[1]
            print('fork HTTP/RPC/SQLite journey passed: binding, parent preservation, notice/restart, title, owner, no inference')
        finally:
            if process.poll() is None:process.send_signal(signal.SIGINT)
            _,err=process.communicate(timeout=10)
            assert b'synthetic-fork-token' not in err
            server.shutdown()

if __name__=='__main__':main(str(pathlib.Path(sys.argv[1]).resolve()))
