#!/usr/bin/env python3
"""Nexus ingress through real process pipes, HTTP, native WebSocket and SQLite.
Synthetic identities/operator/model only. Real Nexus API qualification is separate.
"""
import base64, hashlib, json, pathlib, sqlite3, struct, sys, threading, time
from goals_io import Goals

OPERATOR = '''import argparse,json,pathlib,sqlite3,sys
p=argparse.ArgumentParser();p.add_argument('--workspace');p.add_argument('--state');p.add_argument('op');a=p.parse_args();v=json.load(sys.stdin)
db=sqlite3.connect(pathlib.Path(a.state)/'operator.sqlite',timeout=5);db.row_factory=sqlite3.Row;db.execute('BEGIN IMMEDIATE')
root_binding=json.loads(db.execute("SELECT doc FROM binding").fetchone()[0])
selected=v.get('table_id')
if a.op in ('inspect','dispatch'):
 selected=json.loads(db.execute('SELECT doc FROM requests WHERE id=?',(v['request_id'],)).fetchone()[0])['request']['table_id']
b=root_binding.get('extra',{}).get(selected,root_binding)
def read(id):
 r=db.execute('SELECT doc,status,dispatch,native,reply FROM requests WHERE id=?',(id,)).fetchone();doc=json.loads(r['doc']);doc['request'].update(status=r['status'],native_turn_id=r['native'],reply=r['reply']);doc['dispatch_id']=r['dispatch'];return doc
if a.op=='binding':
 # Simulate the maintenance operator acquiring the hold during this async lookup.
 marker=pathlib.Path(a.state)/'hold-during-binding'
 if marker.exists():
  with sqlite3.connect(pathlib.Path(a.workspace)/'state.sqlite') as bridge:
   bridge.execute("INSERT OR REPLACE INTO bot_state(key,value) VALUES('native_writer_hold:100:7','fixture-maintenance')")
  marker.unlink()
 out={k:b[k] for k in ('binding_id','revision','active','label','responder_ref')} if b.get('revision') else None
elif a.op=='configure':
 assert v['expected_revision']==b['revision'] and v['table_id']=='table' and v['author_ref']=='human:test' and v['target']['human_user_id']==100 and v['target']['bot_id']==123
 b={**v,'binding_id':'binding','revision':b['revision']+1};db.execute('UPDATE binding SET doc=?',(json.dumps(b),));db.execute("UPDATE requests SET status='withdrawn' WHERE status='queued'");out={'binding_id':b['binding_id'],'revision':b['revision']}
elif a.op=='pending':out=[r[0] for r in db.execute("SELECT id FROM requests WHERE status='queued'")] if b['active'] else []
elif a.op in ('inspect','dispatch'):
 out=read(v['request_id']);assert out['target']['listener_id']==v['listener_id']
 if a.op=='dispatch':
  if out['request']['status']=='queued':
   assert b['active'] and b['revision']==out['request']['binding_revision']
   out['dispatch']=True;out['dispatch_id']='dispatch-'+v['request_id'];db.execute("UPDATE requests SET status='handed_off',dispatch=? WHERE id=?",(out['dispatch_id'],v['request_id']))
  else:out={'dispatch':False,'request_id':v['request_id'],'status':out['request']['status']}
 else:out['dispatch']=False
elif a.op=='receipt':
 out=read(v['request_id']);assert out['dispatch_id']==v['dispatch_id'] and out['target']['native_thread_id']==v['native_thread_id']
 old=out['request'];status=v['status'];native=v.get('native_turn_id');reply=v.get('reply')
 if status=='accepted':assert native and not reply
 elif status=='completed':assert native==old['native_turn_id'] and old['status'] in ('accepted','completed') and reply
 elif status in ('failed','interrupted'):assert native==old['native_turn_id'] and old['status'] in ('accepted',status)
 elif status=='rejected':assert old['native_turn_id'] is None and not native and v['reason']
 else:raise ValueError('bad status')
 if old['status'] not in ('completed','interrupted','failed','rejected'):db.execute('UPDATE requests SET status=?,native=?,reply=? WHERE id=?',(status,native,reply,v['request_id']))
 out=read(v['request_id'])['request']
else:raise ValueError('bad operation')
db.commit();print(json.dumps(out))
'''

class Nexus(Goals):
    def __init__(self,binary,mode='complete'):
        super().__init__(binary,mode)
        self.records={}
        self.state=self.root/'nexus';self.state.mkdir(mode=0o700)
        self.operator=self.root/'nexus_fixture.py';self.operator.write_text(OPERATOR)
        with sqlite3.connect(self.state/'operator.sqlite') as db:
            db.executescript('CREATE TABLE binding(doc TEXT);CREATE TABLE requests(id TEXT PRIMARY KEY,doc TEXT,status TEXT,dispatch TEXT,native TEXT,reply TEXT);')
            db.execute('INSERT INTO binding VALUES(?)',(json.dumps({'revision':0,'active':False}),))
    def wait(self,condition,seconds=15):
        try: return super().wait(condition,seconds)
        except Exception:
            if self.process:
                self.process.terminate();output,error=self.process.communicate(timeout=8);self.process=None;print(output[-12000:]+error,file=sys.stderr)
            raise
    def prepare(self):
        self.start();self.admit('Establish the existing native conversation');self.wait(lambda:self.has('Initial goal turn delivered.'));self.idle();self.stop()
        self.before=self.row('SELECT id,codex_thread_id,session_title,cwd FROM sessions WHERE thread_id=7')
        with self.config.open('a') as f:
            f.write(f'''\n[nexus]
python="{sys.executable}"
operator="{self.operator}"
workspace="{self.root}"
state="{self.state}"
listener_id="listener"
responder_ref="daimon:test"
launch_selector="fixture-launch"
poll_seconds=1
[[nexus.routes]]
table_id="table"
author_ref="human:test"
human_user_id=100
label="Nexus"
''')
        self.start();self.command('/nexus connect','Nexus: connected');self.mode='hold' if self.mode=='hold_after_connect' else self.mode
    def enqueue(self,id='request',table='table'):
        with sqlite3.connect(self.state/'operator.sqlite') as db:
            b=json.loads(db.execute('SELECT doc FROM binding').fetchone()[0]);b=b.get('extra',{}).get(table,b);assert b['active']
            req={'request_id':id,'table_id':table,'author_ref':'human:test','binding_id':b['binding_id'],'binding_revision':b['revision'],'responder_ref':b['responder_ref'],'contribution':{'contribution_id':id+'-contribution','text':'Selected human question '+id,'anchors':[{'source_id':'source','transcript_version':'v1','start_ms':1000,'end_ms':2000}]},'return_context':{'playback_ms':1000}}
            doc={'target':b['target'],'request':req};db.execute("INSERT INTO requests VALUES(?,?,'queued',NULL,NULL,NULL)",(id,json.dumps(doc)))
    def state_of(self,id='request'):
        with sqlite3.connect(self.state/'operator.sqlite') as db:return db.execute('SELECT status,native,reply FROM requests WHERE id=?',(id,)).fetchone()
    def finish(self,thread,turn,continuation=False):
        self.records[(thread,turn)]={'id':turn,'status':'completed','items':[{'type':'agentMessage','phase':'final_answer','text':'Initial goal turn delivered.'}]}
        super().finish(thread,turn,continuation)
    def native_connection(self,sock):
        stream=sock.makefile('rb');headers={}
        while True:
            line=stream.readline()
            if not line:return
            if line==b'\r\n':break
            if b':'in line:
                k,v=line.split(b':',1);headers[k.strip().lower()]=v.strip()
        accept=base64.b64encode(hashlib.sha1(headers[b'sec-websocket-key']+b'258EAFA5-E914-47DA-95CA-C5AB0DC85B11').digest());sock.sendall(b'HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: '+accept+b'\r\n\r\n')
        lock=threading.Lock();thread=None;turn=None
        def send(v):
            data=json.dumps(v).encode();size=len(data);header=bytes([129,size]) if size<126 else b'\x81\x7e'+struct.pack('!H',size)
            with lock:sock.sendall(header+data)
        try:
            while True:
                head=stream.read(2)
                if len(head)<2:return
                opcode=head[0]&15;size=head[1]&127
                if size==126:size=struct.unpack('!H',stream.read(2))[0]
                elif size==127:size=struct.unpack('!Q',stream.read(8))[0]
                mask=stream.read(4);raw=stream.read(size)
                if opcode==8:return
                if opcode!=1:continue
                m=json.loads(bytes(v^mask[i%4] for i,v in enumerate(raw)));method=m.get('method');ident=m.get('id');p=m.get('params',{})
                with self.lock:self.rpc.append(m)
                if method=='initialize':send({'id':ident,'result':{}})
                elif method in ('thread/start','thread/resume'):
                    if method=='thread/start':self.thread_sequence+=1;thread=f'native-topic-{self.thread_sequence+6}'
                    else:thread=p['threadId'];assert p['excludeTurns'] is True
                    self.clients[id(sock)]=(thread,send);send({'id':ident,'result':{'thread':{'id':thread,'turns':[]},'model':'fixture-model'}})
                elif method=='thread/goal/get':send({'id':ident,'result':{'goal':None}})
                elif method=='thread/turns/list':send({'id':ident,'result':{'data':[v for (t,_),v in self.records.items() if t==p['threadId']],'nextCursor':None}})
                elif method=='thread/name/set':send({'id':ident,'result':{}})
                elif method=='turn/start':
                    assert thread in ('native-topic-7','native-topic-8');self.native_turns+=1;turn=f'turn-{self.native_turns}'
                    if self.mode=='reject':send({'id':ident,'error':{'code':-32600,'message':'synthetic rejection'}});continue
                    if self.mode=='lost_ack':sock.close();return
                    send({'id':ident,'result':{'turn':{'id':turn}}});self.broadcast(thread,'turn/started',{'turn':{'id':turn,'status':'inProgress'}})
                    if self.mode in ('hold','reject_steer'):self.holding[thread]=turn
                    else:self.finish(thread,turn)
                elif method=='turn/steer':
                    if self.mode=='reject_steer':send({'id':ident,'error':{'code':-32600,'message':'synthetic definite steer rejection'}});continue
                    if self.mode=='lost_steer':sock.close();return
                    send({'id':ident,'result':{'turnId':turn}})
                elif method=='turn/interrupt':send({'id':ident,'result':{}});self.broadcast(thread,'turn/completed',{'turn':{'id':turn,'status':'interrupted'}})
                elif method and ident is not None:send({'id':ident,'result':{}})
        finally:self.clients.pop(id(sock),None)


def delivered(binary):
    j=Nexus(binary)
    try:
        j.prepare();starts=j.native_turns;j.enqueue();j.wait(lambda:j.state_of()[0]=='completed');j.idle()
        assert j.native_turns==starts+1 and j.state_of()[1]=='turn-2' and j.state_of()[2]=='Initial goal turn delivered.'
        assert j.row('SELECT id,codex_thread_id,session_title,cwd FROM sessions WHERE thread_id=7')==j.before
        submitted=[x for x in j.rpc if x.get('method')=='turn/start'][-1];text=submitted['params']['input'][0]['text'];assert 'Selected human question request' in text and '"transcript_version":"v1"' in text
        time.sleep(1.2);assert j.native_turns==starts+1
        j.stop();j.start();j.command('/nexus reconcile','Nexus: connected');assert j.native_turns==starts+1
        assert sum(x.get('method')=='thread/start' for x in j.rpc)==1
        print('PASS existing-thread delivery, actual RPC turn/reply, source anchors, dedup, restart receipt-only reconciliation')
    finally:j.close()


def table_launches(binary):
    j=Nexus(binary)
    try:
        j.prepare();starts=j.native_turns;j.stop()
        with j.config.open('a') as config:
            config.write('\n[[nexus.routes]]\ntable_id="research"\nauthor_ref="human:test"\nhuman_user_id=100\nlabel="Research"\nlaunch_selector="research-launch"\n')
        j.start()
        j.command('/nexus open research','startapp=research-launch')
        j.command('/nexus open table','startapp=fixture-launch')
        assert j.native_turns==starts
        assert j.row('SELECT id,codex_thread_id,session_title,cwd FROM sessions WHERE thread_id=7')==j.before
        print('PASS selected research launch and legacy recording launch retain their own destinations without native input or rebinding')
    finally:j.close()


def reviewed_iteration(binary):
    j=Nexus(binary)
    try:
        j.prepare();starts=j.native_turns;j.stop();j.enqueue('iteration-request')
        # Use the same ingress ledger with a reviewed work package, not an audio note.
        with sqlite3.connect(j.state/'operator.sqlite') as db:
            doc=json.loads(db.execute("SELECT doc FROM requests WHERE id='iteration-request'").fetchone()[0])
            request=doc['request'];request.pop('contribution');request.pop('return_context')
            request.update(request_kind='iteration',iteration_id='research',iteration_revision=1,iteration={
                'revision':1,'state':'active','activated_by':'human:test','tasks':[{'task_id':'map','revision':0,'state':'ready'}],
                'spec':{'iteration_id':'research','title':'Clarify evidence layers','scope':'Selected research only','tasks':[{
                    'task_id':'map','instruction':'Keep measured geometry separate from human plausibility',
                    'targets':[{'context_id':'research','version':'v1','kind':'relation','object_id':'evidence-link'}],
                    'acceptance':'Retain canonical source and published map observations','executor':'daimon'}]}})
            db.execute("UPDATE requests SET doc=? WHERE id='iteration-request'",(json.dumps(doc),))
        j.start();j.wait(lambda:j.state_of('iteration-request')[0]=='completed');j.idle()
        assert j.native_turns==starts+1
        submitted=[x for x in j.rpc if x.get('method')=='turn/start'][-1]['params']['input'][0]['text']
        assert 'requested Advance' in submitted and '"object_id":"evidence-link"' in submitted
        assert '"revision":1' in submitted and 'A native reply does not complete the iteration' in submitted
        assert 'Selected human question' not in submitted
        j.stop();j.start();j.command('/nexus reconcile','Nexus: connected')
        assert j.native_turns==starts+1
        print('PASS reviewed iteration uses existing native thread with frozen relation/version scope and no replay; fixture reply is not a work effect')
    finally:j.close()



def peer_request(binary):
    j=Nexus(binary)
    try:
        j.prepare();starts=j.native_turns;j.stop();j.enqueue('peer-request')
        with sqlite3.connect(j.state/'operator.sqlite') as db:
            doc=json.loads(db.execute("SELECT doc FROM requests WHERE id='peer-request'").fetchone()[0])
            req=doc['request'];req.pop('contribution');req.pop('return_context')
            req.update(request_kind='peer',peer_request_id='selected-peer-request',peer_request={
                'request_id':'selected-peer-request','recipient_label':'Fixture peer',
                'recipient_ref':'dm:being:v1:fixture_peer','disclosure':'whole-tribe',
                'target':{'context_id':'research','version':'v2','kind':'relation','object_id':'exact-link'},
                'shared_message':'Exactly the human-selected public-to-Tribe question.'})
            db.execute("UPDATE requests SET doc=? WHERE id='peer-request'",(json.dumps(doc),))
        j.start();j.wait(lambda:j.state_of('peer-request')[0]=='completed');j.idle()
        assert j.native_turns==starts+1
        text=[x for x in j.rpc if x.get('method')=='turn/start'][-1]['params']['input'][0]['text']
        assert '"object_id":"exact-link"' in text and '"version":"v2"' in text
        assert 'whole Tribe' in text and 'Your native reply is private' in text
        assert 'Exactly the human-selected public-to-Tribe question.' in text
        assert 'Selected human question' not in text
        j.stop();j.start();j.command('/nexus reconcile','Nexus: connected')
        assert j.native_turns==starts+1
        print('PASS explicit peer disclosure reaches existing native thread once; native fixture reply is not a signed peer response')
    finally:j.close()


def controls(binary):
    j=Nexus(binary,'hold_after_connect')
    try:
        j.prepare();j.enqueue('first');j.wait(lambda:j.state_of('first')[0]=='accepted');starts=j.native_turns
        j.enqueue('correction');j.wait(lambda:j.state_of('correction')[0]=='accepted');assert j.native_turns==starts
        assert any(x.get('method')=='turn/steer' and 'Selected human question correction' in x['params']['input'][0]['text'] for x in j.rpc)
        j.command('/stop','Stop signal sent.');j.wait(lambda:j.state_of('first')[0]=='interrupted' and j.state_of('correction')[0]=='interrupted');j.idle()
        assert j.state_of('first')[1]==j.state_of('correction')[1]
        print('PASS selected correction through existing steer, matching actual native turn, observed interruption')
    finally:j.close()


def refusals(binary):
    j=Nexus(binary)
    try:
        j.prepare();starts=j.native_turns
        with sqlite3.connect(j.root/'state.sqlite') as db:db.execute("UPDATE sessions SET codex_thread_id='other-thread' WHERE thread_id=7")
        j.enqueue();time.sleep(2.3);assert j.native_turns==starts and j.state_of()[0]=='queued'
        j.command('/nexus disconnect','Nexus: not connected');assert j.state_of()[0]=='withdrawn'
        assert j.row('SELECT codex_thread_id FROM sessions WHERE thread_id=7')==[('other-thread',)]
        print('PASS wrong-thread refusal and revision withdrawal, no auto-repair or native submission')
    finally:j.close()


def uncertainty(binary):
    j=Nexus(binary)
    try:
        j.prepare();j.mode='lost_ack';starts=j.native_turns;j.enqueue();j.wait(lambda:j.native_turns==starts+1);j.wait(lambda:j.row("SELECT phase FROM nexus_requests WHERE id='request'")==[('undetermined',)])
        assert j.state_of()[0]=='handed_off' and j.state_of()[1] is None
        j.stop();j.mode='complete';j.start();j.command('/nexus reconcile','Nexus: connected');time.sleep(1.2);assert j.native_turns==starts+1
        assert j.row('SELECT codex_thread_id FROM sessions WHERE thread_id=7')==[('native-topic-7',)]
        print('PASS lost native acknowledgement retained and never replayed or replaced after restart')
    finally:j.close()


def known_turn_recovery(binary):
    j=Nexus(binary,'hold_after_connect')
    try:
        j.prepare();j.enqueue();j.wait(lambda:j.state_of()[0]=='accepted');native=j.state_of()[1];starts=j.native_turns;j.stop()
        # Model finishes while the listener has no stream. Only explicit selected
        # turn metadata inspection can recover the literal reply on return.
        j.records[('native-topic-7',native)]={'id':native,'status':'completed','items':[{'type':'agentMessage','phase':'final_answer','text':'Recovered literal final.'}]}
        j.records[('native-topic-7','foreign-turn')]={'id':'foreign-turn','status':'completed','items':[{'type':'agentMessage','phase':'final_answer','text':'Unselected reply must not be stored.'}]}
        j.start();j.command('/nexus reconcile','Nexus: connected');j.wait(lambda:j.state_of()[0]=='completed')
        assert j.state_of()[1]==native and j.state_of()[2]=='Recovered literal final.' and j.native_turns==starts
        assert not j.row("SELECT reply FROM nexus_executions WHERE reply LIKE '%Unselected%'")
        print('PASS explicit accepted-turn reply recovery after restart, no native input and no other turn persistence')
    finally:j.close()


def automatic_missing_terminal(binary):
    j=Nexus(binary,'hold_after_connect')
    try:
        j.prepare();j.enqueue();j.wait(lambda:j.state_of()[0]=='accepted')
        native=j.state_of()[1];starts=j.native_turns;j.stop()
        with sqlite3.connect(j.root/'state.sqlite') as db:
            local=db.execute("SELECT local_turn FROM nexus_requests WHERE id='request'").fetchone()[0]
            db.execute("UPDATE turns SET status='completed' WHERE id=?",(local,))
        # Only selected native metadata supplies the final and terminal evidence.
        j.records[('native-topic-7',native)]={'id':native,'status':'completed','items':[{'type':'agentMessage','phase':'final_answer','text':'Automatically recovered selected final.'}]}
        j.records[('native-topic-7','foreign-turn')]={'id':'foreign-turn','status':'completed','items':[{'type':'agentMessage','phase':'final_answer','text':'Do not select this other reply.'}]}
        j.start();j.wait(lambda:j.state_of()[0]=='completed')
        assert j.state_of()[1:]==(native,'Automatically recovered selected final.')
        assert j.native_turns==starts
        reads=[r for r in j.rpc if r.get('method')=='thread/turns/list']
        assert reads and all(r['params'].get('itemsView')=='summary' for r in reads)
        assert not j.row("SELECT reply FROM nexus_executions WHERE reply LIKE '%other reply%'")
        print('PASS automatic exact-turn terminal recovery without human reconcile or native replay')
    finally:j.close()


def connect_during_work(binary):
    j=Nexus(binary)
    try:
        j.prepare();j.command('/nexus disconnect','Nexus: not connected');j.mode='hold';j.admit('Ordinary work before new connection');j.wait(lambda:bool(j.holding));starts=j.native_turns
        j.command('/nexus connect','Nexus: connected');j.enqueue();j.wait(lambda:j.state_of()[0]=='accepted')
        native=j.state_of()[1];j.command('/nexus disconnect','Nexus: not connected');j.finish('native-topic-7',native);j.wait(lambda:j.state_of()[0]=='completed');j.idle()
        assert j.native_turns==starts and j.state_of()[2]=='Initial goal turn delivered.'
        print('PASS connect during existing work, steer into that physical turn, final retained after disconnect')
    finally:j.close()


def writer_hold(binary):
    j=Nexus(binary)
    try:
        j.prepare();starts=j.native_turns
        with sqlite3.connect(j.root/'state.sqlite') as db:
            db.execute("INSERT INTO bot_state(key,value) VALUES('native_writer_hold:100:7','fixture-maintenance')")
        j.enqueue();time.sleep(2.3)
        assert j.state_of()[0]=='queued' and j.native_turns==starts, 'held topic admitted Nexus turn'
        assert not j.row("SELECT id FROM nexus_requests WHERE id='request'")
        j.stop();j.start();time.sleep(1.2)
        assert j.state_of()[0]=='queued' and j.native_turns==starts, 'restart bypassed writer hold'
        # The same hold namespace must not turn into a global ingress suspension.
        with sqlite3.connect(j.root/'state.sqlite') as db:
            db.execute("DELETE FROM bot_state WHERE key='native_writer_hold:100:7'")
            db.execute("INSERT INTO bot_state(key,value) VALUES('native_writer_hold:100:8','unrelated-maintenance')")
        j.wait(lambda:j.state_of()[0]=='completed');j.idle()
        assert j.native_turns==starts+1
        print('PASS held Nexus remains queued across restart, unrelated hold does not block selected topic')
    finally:j.close()


def writer_hold_steer(binary):
    j=Nexus(binary,'hold_after_connect')
    try:
        j.prepare();j.enqueue();j.wait(lambda:j.state_of()[0]=='accepted')
        starts=j.native_turns;steers=sum(x.get('method')=='turn/steer' for x in j.rpc)
        with sqlite3.connect(j.root/'state.sqlite') as db:
            db.execute("INSERT INTO bot_state(key,value) VALUES('native_writer_hold:100:7','fixture-maintenance')")
        j.enqueue('correction');time.sleep(2.3)
        assert j.state_of('correction')[0]=='queued' and j.native_turns==starts
        assert sum(x.get('method')=='turn/steer' for x in j.rpc)==steers, 'held topic admitted Nexus steer'
        assert j.state_of()[0]=='accepted', 'maintenance changed existing native work'
        print('PASS active held writer is not steered or interrupted by Nexus')
    finally:j.close()


def writer_hold_race(binary):
    j=Nexus(binary)
    try:
        j.prepare();starts=j.native_turns
        (j.state/'hold-during-binding').write_text('acquire during binding')
        j.enqueue();j.wait(lambda:not (j.state/'hold-during-binding').exists());time.sleep(1.2)
        assert j.state_of()[0]=='queued' and j.native_turns==starts, 'async binding lookup bypassed new hold'
        assert not j.row("SELECT id FROM nexus_requests WHERE id='request'")
        print('PASS hold acquired during async validation refuses admission before native input')
    finally:j.close()


def writer_hold_other_target(binary):
    j=Nexus(binary)
    try:
        j.prepare();j.stop();starts=j.native_turns
        with j.config.open('a') as f:
            f.write('\n[[nexus.routes]]\ntable_id="other"\nauthor_ref="human:test"\nhuman_user_id=100\nlabel="Other"\n')
        with sqlite3.connect(j.state/'operator.sqlite') as db:
            b=json.loads(db.execute('SELECT doc FROM binding').fetchone()[0])
            other=json.loads(json.dumps(b));other.update(table_id='other',binding_id='other-binding',label='Other')
            other['target'].update(topic_id=8,native_thread_id='native-topic-8')
            b['extra']={'other':other};db.execute('UPDATE binding SET doc=?',(json.dumps(b),))
        with sqlite3.connect(j.root/'state.sqlite') as db:
            db.row_factory=sqlite3.Row
            row=dict(db.execute('SELECT * FROM sessions WHERE thread_id=7').fetchone());row.pop('id')
            row.update(thread_id=8,codex_thread_id='native-topic-8',session_title='Other')
            db.execute('INSERT INTO sessions ('+','.join(row)+') VALUES ('+','.join('?' for _ in row)+')',list(row.values()))
            db.execute('INSERT INTO nexus_bindings VALUES(?,?,?)',('other','human:test',json.dumps(other)))
            db.execute("INSERT INTO bot_state(key,value) VALUES('native_writer_hold:100:7','fixture-maintenance')")
        j.enqueue('held');j.enqueue('free',table='other');j.start()
        j.wait(lambda:j.state_of('free')[0]=='completed');j.idle()
        assert j.state_of('held')[0]=='queued' and j.native_turns==starts+1
        assert not j.row("SELECT id FROM nexus_requests WHERE id='held'")
        assert [x['params']['threadId'] for x in j.rpc if x.get('method')=='turn/start'][-1]=='native-topic-8'
        print('PASS held request is skipped while another target delivers in the same poll')
    finally:j.close()

if __name__=='__main__':
    binary=str(pathlib.Path(sys.argv[1]).resolve())
    scenarios=(delivered,table_launches,reviewed_iteration,peer_request,controls,refusals,uncertainty,known_turn_recovery,automatic_missing_terminal,connect_during_work,writer_hold,writer_hold_steer,writer_hold_race,writer_hold_other_target)
    for run in scenarios:
        if len(sys.argv)==2 or run.__name__ in sys.argv[2:]:run(binary)
