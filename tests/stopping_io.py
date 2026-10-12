#!/usr/bin/env python3
"""Scoped Stop through real Unix IPC/HTTP, native WebSocket and SQLite.
All conversations and model output are synthetic; no live model/bot calls.
"""
import copy,json,pathlib,subprocess,sys,time,sqlite3,socket
from goals_io import Goals

class Stopping(Goals):
    def __init__(self,binary):
        super().__init__(binary,'hold')
        self.socket=self.root/'control.sock'
        self.children={};self.bad_ancestry=False
        self.config.write_text(self.config.read_text().replace('[telegram]',f'[session_control]\nsocket_path="{self.socket}"\nhuman_id=100\n[telegram]'))
    def start(self):
        super().start()
        def ready():
            try:
                with socket.socket(socket.AF_UNIX,socket.SOCK_STREAM) as client:
                    client.connect(str(self.socket))
                    return not hasattr(socket,'SO_PEERCRED') or int.from_bytes(client.getsockopt(socket.SOL_SOCKET,socket.SO_PEERCRED,12)[:4],sys.byteorder)==self.process.pid
            except OSError:return False
        self.wait(ready)
    def handle_extra_rpc(self,request,send):
        p=request.get('params',{});method=request.get('method');ident=request.get('id')
        if method=='thread/list' and 'ancestorThreadId' in p:
            rows=[]
            if not p.get('archived'):
                for child,parent in self.children.items():
                    current=parent
                    while current in self.children:current=self.children[current]
                    if current==p['ancestorThreadId'] or self.bad_ancestry:
                        rows.append({'id':child,'parentThreadId':parent,'source':{'subAgent':{'thread_spawn':{'parent_thread_id':parent,'depth':1}}}})
            send({'id':ident,'result':{'data':rows,'nextCursor':None}});return True
        if method=='thread/read':
            send({'id':ident,'result':{'thread':{'id':p['threadId'],'status':{'type':'active' if self.holding.get(p['threadId']) else 'idle'}}}});return True
        if method=='turn/interrupt':
            thread=p['threadId'];current=self.holding.pop(thread,None)
            send({'id':ident,'result':{}})
            if current:self.broadcast(thread,'turn/completed',{'turn':{'id':current,'status':'interrupted'}})
            return True
        return False
    def control(self,args,ok=True):
        result=subprocess.run([self.binary,'--control',str(self.config),'stop',*args.split()],capture_output=True,text=True,timeout=35)
        assert (result.returncode==0)==ok,(result.returncode,result.stdout,result.stderr)
        return json.loads(result.stdout)
    def operation(self):
        return self.row('SELECT id,phase FROM stop_operations ORDER BY rowid DESC LIMIT 1')[0]
    def preload(self,thread,status):
        self.goals[thread]={'threadId':thread,'objective':'Original work '+thread,'status':status,'tokenBudget':200,'tokensUsed':17,'timeUsedSeconds':8,'createdAt':10,'updatedAt':10}
    def seed(self):
        self.start();self.admit('Original unfinished first request',topic=7)
        self.wait(lambda:self.holding.get('native-topic-7'))
        self.admit('Original unfinished second request',topic=8)
        self.wait(lambda:self.holding.get('native-topic-8'))
        self.preload('native-topic-7','active');self.preload('native-topic-8','paused')

def stop_restart_resume(binary):
    j=Stopping(binary)
    try:
        j.seed();first=j.control('all')['result'];ident=first['operation']
        j.wait(lambda:j.operation()[1]=='stopped');j.idle()
        assert j.goals['native-topic-7']['status']=='paused' and j.goals['native-topic-8']['status']=='paused'
        assert j.native_turns==2
        assert j.control('all')['result']['operation']==ident
        held=j.admit('New input during Stop',topic=8)
        j.wait(lambda:j.row('SELECT status FROM incoming_updates WHERE update_id=?',(held,))==[('stop_held',)])
        assert j.native_turns==2
        j.stop();j.start()
        assert j.operation()==(ident,'stopped')
        assert j.control('status '+ident)['result']['state']=='stopped'
        j.control('resume '+ident)
        j.wait(lambda:j.native_turns==4)
        j.wait(lambda:j.goals['native-topic-7']['status']=='active')
        assert j.goals['native-topic-8']['status']=='paused'
        j.wait(lambda:j.operation()[1]=='resumed')
        starts=[x for x in j.rpc if x.get('method')=='turn/start']
        assert 'explicitly resumed' in starts[-2]['params']['input'][0]['text'] or 'explicitly resumed' in starts[-1]['params']['input'][0]['text']
        assert not any('Original unfinished' in x['params']['input'][0]['text'] for x in starts[2:])
        j.control('resume '+ident,False);assert j.native_turns==4
        print('PASS native goal/ordinary Stop, existing paused goal, caller-independent coordinator, retained ingress, dedup, restart and explicit non-replaying Resume')
    finally:j.close()

def changed_goal_and_other_hold(binary):
    j=Stopping(binary)
    try:
        j.seed();ident=j.control('all')['result']['operation'];j.wait(lambda:j.operation()[1]=='stopped');j.idle()
        j.goals['native-topic-7']['objective']='New human definition'
        j.control('resume '+ident,False);assert j.native_turns==2 and j.operation()[1]=='stopped'
        j.goals['native-topic-7']['objective']='Original work native-topic-7'
        with sqlite3.connect(j.root/'state.sqlite') as db:db.execute("INSERT INTO bot_state(key,value) VALUES('native_writer_hold:100:8','foreign-maintenance')")
        j.control('resume '+ident,False);assert j.native_turns==2
        assert j.row("SELECT value FROM bot_state WHERE key='native_writer_hold:100:8'")==[('foreign-maintenance',)]
        print('PASS changed goal and independent maintenance hold preserve fences and refuse Resume before any native effect')
    finally:j.close()

def native_children_and_ancestry(binary):
    j=Stopping(binary)
    try:
        j.seed();j.children={'child-one':'native-topic-7','grandchild':'child-one'}
        j.holding.update({'child-one':'child-turn','grandchild':'grandchild-turn'})
        j.preload('child-one','active')
        ident=j.control('all')['result']['operation'];j.wait(lambda:j.operation()[1]=='stopped');j.idle()
        assert not j.holding and j.goals['child-one']['status']=='paused'
        interrupted=[r['params']['threadId'] for r in j.rpc if r.get('method')=='turn/interrupt']
        assert 'child-one' in interrupted and 'grandchild' in interrupted
        j.control('resume '+ident);j.wait(lambda:j.native_turns==4)
        starts=[r['params']['input'][0]['text'] for r in j.rpc if r.get('method')=='turn/start'][2:]
        assert any('child-one' in text and 'native recovery' in text for text in starts)
        assert j.goals['child-one']['status']=='paused' # Recovery is delivered to parent, not falsely claimed.
        print('PASS verified descendant Stop including active child goal; attributed child recovery delivered to parent without claiming child resume')
    finally:j.close()
    j=Stopping(binary)
    try:
        j.seed();j.children={'foreign-child':'foreign-root'};j.holding['foreign-child']='foreign-turn';j.bad_ancestry=True
        ident=j.control('all')['result']['operation'];j.wait(lambda: any(t['phase']=='unconfirmed' for t in json.loads(j.row('SELECT document FROM stop_operations WHERE id=?',(ident,))[0][0])['targets']))
        assert j.holding['foreign-child']=='foreign-turn'
        assert not any(r.get('method')=='turn/interrupt' and r['params']['threadId']=='foreign-child' for r in j.rpc)
        j.control('release '+ident)
        assert j.operation()[1]=='released'
        print('PASS foreign ancestry rejected without foreign control; explicit release does not resume or replay work')
    finally:j.close()

def maintenance_restart(binary):
    j=Stopping(binary)
    try:
        j.seed();ident=j.control('all')['result']['operation']
        j.wait(lambda:j.operation()[1]=='stopped');j.idle()
        j.control('restart '+ident)
        before=[m for m in j.messages if 'Restarting after coordinated Stop' in m[1]['text']]
        assert sorted(m[1]['message_thread_id'] for m in before)==[7,8]
        j.control('restart '+ident)
        assert len([m for m in j.messages if 'Restarting after coordinated Stop' in m[1]['text']])==2
        assert j.native_turns==2 # Arming does not resume or restart the same boot.
        j.stop();j.start()
        j.wait(lambda:j.native_turns==4 and j.operation()[1]=='resumed')
        j.wait(lambda:j.goals['native-topic-7']['status']=='active')
        assert j.goals['native-topic-8']['status']=='paused'
        after=[m for m in j.messages if 'Service restarted after coordinated Stop' in m[1]['text']]
        assert sorted(m[1]['message_thread_id'] for m in after)==[7,8]
        j.stop();j.start();time.sleep(.4)
        assert j.native_turns==4
        print('PASS explicit one-reboot maintenance notices, active goal/turn continuation, preserved paused goal and no replay on a second reboot')
    finally:j.close()

if __name__=='__main__':
    binary=str(pathlib.Path(sys.argv[1]).resolve());stop_restart_resume(binary);changed_goal_and_other_hold(binary);native_children_and_ancestry(binary);maintenance_restart(binary)

