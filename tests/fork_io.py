#!/usr/bin/env python3
"""Native fork/topic binding journey using synthetic HTTP, stdio RPC and SQLite."""
import collections
import http.server
import importlib.util
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


spec = importlib.util.spec_from_file_location('activity',pathlib.Path(__file__).resolve().parents[1]/'scripts/activity_indicator.py')
activity=importlib.util.module_from_spec(spec)
spec.loader.exec_module(activity)

def main(binary):
    with tempfile.TemporaryDirectory(prefix='telecodex-fork-') as directory:
        root = pathlib.Path(directory)
        updates, calls = collections.deque(), []
        topics = []
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
                    topics.append(data['name'])
                    result = dict(message_thread_id=7+len(topics), name=data['name'], icon_color=0)
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
import json,sys,pathlib,time
if sys.argv[1:]==['login','status']:
 print('Logged in using ChatGPT');sys.exit(0)
log=pathlib.Path(__file__).with_name('rpc.jsonl')
for line in sys.stdin:
 r=json.loads(line)
 with log.open('a') as f:f.write(json.dumps(r)+'\\n')
 if 'id' not in r:continue
 m=r['method']
 if m=='initialize':result={'userAgent':'fixture'}
 elif m=='thread/turns/list':
  if r['params'].get('itemsView')=='full':
   assert r['params']['threadId']=='parent-thread' and r['params']['sortDirection']=='desc'
   def turn(id,text,kind='userMessage'):
    item={'type':kind,'content':[{'type':'text','text':text}]} if kind=='userMessage' else {'type':kind,'text':text}
    return {'id':id,'status':'completed','items':[item]}
   if r['params'].get('cursor')=='older':
    result={'data':[turn('repeat-2','Repeated legacy'),turn('repeat-1','Repeated legacy'),turn('legacy','Legacy **resource decision**. Followup in same turn.','agentMessage'),turn('oldest','Original source intent')]}
   else:result={'data':[turn('newer','LATER NATIVE')],'nextCursor':'older'}
  else:
   assert r['params']=={'threadId':'parent-thread','limit':1,'sortDirection':'desc'}
   result={'data':[{'id':'parent-turn','status':'completed'}]}
 elif m=='thread/fork':
  assert r['params']=={'threadId':'parent-thread','excludeTurns':True,'lastTurnId':'parent-turn'}
  result={'thread':{'id':'child-thread'}}
 elif m=='thread/name/set':result={}
 elif m=='config/read':result={'config':{'mcp_servers':{'fixture':{'enabled':True}}}}
 elif m=='thread/start':
  if r['params'].get('ephemeral'):
   assert r['params']['config']['features.shell_tool'] is False
   assert r['params']['config']['mcp_servers.fixture.enabled'] is False
   assert r['params']['sandbox']=='read-only'
   result={'thread':{'id':'summary-thread'}}
  else:result={'thread':{'id':'handoff-thread'}}
 elif m=='thread/resume':result={'thread':{'id':r['params']['threadId']}}
 elif m=='turn/start':
  thread=r['params']['threadId'];answer='Child reply'
  if thread=='summary-thread':
   gate=log.with_name('hold-summary')
   deadline=time.monotonic()+15
   while gate.exists() and time.monotonic()<deadline:time.sleep(.02)
   context=json.loads(r['params']['input'][0]['text'])['source_entries']
   assert 'LATER SOURCE' not in str(context)
   answer=json.dumps({'summary':'Focused shared resources decision','recent_indices':[len(context)-1]})
  print(json.dumps({'id':r['id'],'result':{'turn':{'id':'child-turn'}}}),flush=True)
  print(json.dumps({'method':'item/completed','params':{'threadId':thread,'turnId':'child-turn','item':{'type':'agentMessage','id':'reply','phase':'final_answer','text':answer}}}),flush=True)
  print(json.dumps({'method':'turn/completed','params':{'threadId':thread,'turn':{'id':'child-turn','status':'completed'}}}),flush=True)
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
        def send(text, user=100, topic=7, reply=None, reply_text='Quoted text'):
            nonlocal seq
            seq+=1
            updates.append(dict(update_id=seq,message=dict(message_id=seq,message_thread_id=topic,chat=dict(id=100,type='private'),text=text,**{'from':dict(id=user,is_bot=False,first_name='Fixture')})))
            if reply is not None:updates[-1]['message']['reply_to_message']={'message_id':reply,'text':reply_text}
            return seq
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
            send('/fork Shared Resources', reply=77, reply_text=None)
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
            quote=send('SHARED RESOURCES source decision')
            wait(lambda:db.execute("SELECT COUNT(*) FROM turns WHERE prompt='SHARED RESOURCES source decision' AND status='completed'").fetchone()[0]==1)
            send('LATER SOURCE')
            wait(lambda:db.execute("SELECT COUNT(*) FROM turns WHERE prompt='LATER SOURCE' AND status='completed'").fetchone()[0]==1)
            source_before=db.execute('SELECT * FROM sessions WHERE thread_id=7').fetchone()
            (root/'hold-summary').touch()
            send('/fork Shared Resources Focused',reply=quote)
            wait(lambda:db.execute("SELECT value FROM bot_state WHERE key='command_activity:100:7'").fetchone() is not None and json.loads(db.execute("SELECT value FROM bot_state WHERE key='command_activity:100:7'").fetchone()[0])['phase']=='synthesizing_handoff')
            observer_db=activity.readonly(root/'state.sqlite')
            command=activity.command_activity(observer_db,100,7,time.time())
            assert command and 'Synthesizing focused handoff' in command['line']
            telegram=activity.Telegram({'bot_token_file':str(token),'api_base':f'http://127.0.0.1:{server.server_port}'})
            requests=activity.StatusRequests(observer_db,telegram,root/'status-requests.json')
            status_id=send('/status')
            wait(lambda:observer_db.execute('SELECT status FROM incoming_updates WHERE update_id=?',(status_id,)).fetchone() is not None)
            requests.respond({'100:7':command['line']},lambda:'weekly unavailable',{'100:7':'parent-thread'},
                             {'100:7':{'command':command['operation'],'active':True,'turn':command['operation']}})
            assert any(m=='sendMessage' and 'Synthesizing focused handoff' in d['text'] for m,d in calls)
            assert observer_db.execute('SELECT status FROM incoming_updates WHERE update_id=?',(status_id,)).fetchone()[0]=='received'
            observer_db.close()
            (root/'hold-summary').unlink()
            wait(lambda:any(m=='sendMessage' and 'Focused fork created:' in d['text'] for m,d in calls))
            wait(lambda:db.execute("SELECT value FROM bot_state WHERE key='command_activity:100:7'").fetchone() is None)
            assert db.execute('SELECT * FROM sessions WHERE thread_id=7').fetchone()==source_before
            assert db.execute('SELECT codex_thread_id,session_prompt FROM sessions WHERE thread_id=9').fetchone()==(None,'Keep this preference')
            assert db.execute("SELECT value FROM bot_state WHERE key='session_title_owner:100:9'").fetchone()[0]=='Shared Resources Focused'
            notice=db.execute("SELECT value FROM bot_state WHERE key='handoff_notice:100:9'").fetchone()[0]
            assert 'Focused shared resources decision' in notice and 'SHARED RESOURCES' in notice and 'LATER SOURCE' not in notice
            rpc=[json.loads(line) for line in (root/'rpc.jsonl').read_text().splitlines()]
            assert len([r for r in rpc if r.get('method')=='thread/fork'])==1
            send('Continue focused',topic=9)
            wait(lambda:db.execute("SELECT COUNT(*) FROM turns WHERE prompt='Continue focused' AND status='completed'").fetchone()[0]==1)
            rpc=[json.loads(line) for line in (root/'rpc.jsonl').read_text().splitlines()]
            inputs=[r['params']['input'][0]['text'] for r in rpc if r.get('method')=='turn/start' and r['params']['threadId']=='handoff-thread']
            assert len(inputs)==1 and 'Focused shared resources decision' in inputs[0]
            naming=[i for i,r in enumerate(rpc) if r.get('method')=='thread/name/set' and r['params']=={'threadId':'handoff-thread','name':'Shared Resources Focused'}]
            first_input=next(i for i,r in enumerate(rpc) if r.get('method')=='turn/start' and r['params']['threadId']=='handoff-thread')
            assert naming and naming[0]<first_input,'first native handoff input must use the chosen topic name'
            assert db.execute("SELECT value FROM bot_state WHERE key='handoff_notice:100:9'").fetchone() is None
            count=len(topics)
            send('/fork Unknown quote',reply=999999)
            wait(lambda:any(m=='sendMessage' and 'quoted text was not found' in d['text'] for m,d in calls))
            assert len(topics)==count
            send('/fork Legacy resources',reply=555555,reply_text='Legacy resource decision.')
            wait(lambda:len(topics)==count+1)
            wait(lambda:any(m=='sendMessage' and 'Focused fork created: Legacy resources' in d['text'] for m,d in calls))
            notice=db.execute("SELECT value FROM bot_state WHERE key='handoff_notice:100:10'").fetchone()[0]
            assert 'inclusive completed native turn, resolved by unique text' in notice
            assert 'LATER NATIVE' not in notice
            rpc=[json.loads(line) for line in (root/'rpc.jsonl').read_text().splitlines()]
            summaries=[r['params']['input'][0]['text'] for r in rpc if r.get('method')=='turn/start' and r['params']['threadId']=='summary-thread']
            assert 'Followup in same turn.' in summaries[-1] and 'LATER NATIVE' not in summaries[-1]
            count=len(topics);inferences=len(summaries)
            send('/fork Ambiguous legacy',reply=666666,reply_text='Repeated legacy')
            wait(lambda:any(m=='sendMessage' and 'occurs more than once' in d['text'] for m,d in calls))
            assert len(topics)==count
            wait(lambda:db.execute("SELECT value FROM bot_state WHERE key='command_activity:100:7'").fetchone() is None)
            rpc=[json.loads(line) for line in (root/'rpc.jsonl').read_text().splitlines()]
            assert len([r for r in rpc if r.get('method')=='turn/start' and r['params']['threadId']=='summary-thread'])==inferences
            print('fork HTTP/RPC/SQLite journey passed: binding, parent preservation, notice/restart, first-only native input, title, owner; fork creation starts no inference')
        finally:
            if process.poll() is None:process.send_signal(signal.SIGINT)
            _,err=process.communicate(timeout=10)
            assert b'synthetic-fork-token' not in err
            server.shutdown()

if __name__=='__main__':main(str(pathlib.Path(sys.argv[1]).resolve()))
