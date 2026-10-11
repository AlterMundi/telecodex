#!/usr/bin/env python3
"""Native goals through the shared proxy, Telegram HTTP and durable SQLite.
All peers/accounts/model output are synthetic; no live model/Telegram call.
"""
import base64
import copy
import hashlib
import json
import pathlib
import struct
import sys
import threading
import time
from questions_io import Journey

class Goals(Journey):
    def __init__(self,binary,mode='complete'):
        self.goals={}; self.clients={}; self.native_turns=0; self.holding={}
        self.start_seen=threading.Event();self.release_start=threading.Event()
        super().__init__(binary,mode)
    def broadcast(self,thread,method,params):
        for _,(selected,send) in list(self.clients.items()):
            if selected==thread:
                try: send({'method':method,'params':{'threadId':thread,**params}})
                except OSError: pass
    def finish(self,thread,turn,continuation=False):
        self.broadcast(thread,'item/completed',{'turnId':turn,'item':{'id':turn+'-answer','type':'agentMessage','phase':'final_answer','text':('Continuation delivered.' if continuation else 'Initial goal turn delivered.')}})
        if continuation:
            self.goals[thread].update(status='complete',tokensUsed=81,timeUsedSeconds=7)
            self.broadcast(thread,'thread/goal/updated',{'goal':copy.deepcopy(self.goals[thread])})
        self.broadcast(thread,'turn/completed',{'turn':{'id':turn,'status':'completed'}})
        if self.goals.get(thread,{}).get('status')=='active' and not continuation:
            def follow():
                time.sleep(.6 if self.mode=='delayed_snapshot' else .1)
                if self.goals.get(thread,{}).get('status')!='active': return
                next_turn=turn+'-continued'; self.native_turns+=1
                self.broadcast(thread,'turn/started',{'turn':{'id':next_turn,'status':'inProgress'}})
                self.finish(thread,next_turn,True)
            threading.Thread(target=follow,daemon=True).start()
    def native_connection(self,sock):
        stream=sock.makefile('rb');headers={}
        while True:
            line=stream.readline()
            if not line:return
            if line==b'\r\n':break
            if b':'in line:
                k,v=line.split(b':',1);headers[k.strip().lower()]=v.strip()
        accept=base64.b64encode(hashlib.sha1(headers[b'sec-websocket-key']+b'258EAFA5-E914-47DA-95CA-C5AB0DC85B11').digest())
        sock.sendall(b'HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: '+accept+b'\r\n\r\n')
        write_lock=threading.Lock();thread=None;turn=None
        def send(v):
            data=json.dumps(v).encode();header=bytes([129,len(data)]) if len(data)<126 else b'\x81\x7e'+struct.pack('!H',len(data))
            with write_lock:sock.sendall(header+data)
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
                    self.clients[id(sock)]=(thread,send)
                    send({'id':ident,'result':{'thread':{'id':thread,'turns':[]},'model':'fixture-model'}})
                elif method=='thread/turns/list':
                    existing=self.holding.get(p['threadId'])
                    send({'id':ident,'result':{'data':[{'id':existing,'status':'inProgress'}] if existing else []}})
                elif method=='thread/goal/get':
                    if self.mode=='unsupported':send({'id':ident,'error':{'code':-32601,'message':'goals unavailable'}});continue
                    if self.mode=='delayed_snapshot':
                        snapshot=copy.deepcopy(self.goals.get(p['threadId']))
                        def late_snapshot(request=ident,goal=snapshot):
                            time.sleep(.3);send({'id':request,'result':{'goal':goal}})
                        threading.Thread(target=late_snapshot,daemon=True).start();continue
                    send({'id':ident,'result':{'goal':copy.deepcopy(self.goals.get(p['threadId']))}})
                elif method=='thread/goal/clear':
                    goal=self.goals.pop(p['threadId'],None);send({'id':ident,'result':{'cleared':goal is not None}})
                    self.broadcast(p['threadId'],'thread/goal/cleared',{})
                elif method=='thread/goal/set':
                    t=p['threadId'];g=self.goals.get(t)
                    if g is None:
                        if not p.get('objective'):send({'id':ident,'error':{'code':-32600,'message':'No goal currently set'}});continue
                        g={'threadId':t,'objective':p['objective'],'status':'active','tokenBudget':None,'tokensUsed':0,'timeUsedSeconds':0,'createdAt':int(time.time()),'updatedAt':int(time.time())};self.goals[t]=g
                    for k in ('objective','status','tokenBudget'):
                        if k in p:g[k]=p[k]
                    send({'id':ident,'result':{'goal':copy.deepcopy(g)}})
                    self.broadcast(t,'thread/goal/updated',{'goal':copy.deepcopy(g)})
                    if g['status']=='active' and t in self.holding and self.mode=='complete':
                        active=self.holding.pop(t)
                        threading.Thread(target=lambda:(time.sleep(.15),self.finish(t,active)),daemon=True).start()
                elif method=='turn/start':
                    assert p.get('additionalContext',{}).get('telecodex.runtime',{}).get('value'),p
                    self.native_turns+=1;turn=f'turn-{self.native_turns}'
                    if self.mode=='staging':
                        self.start_seen.set()
                        def late_start(selected=thread,selected_turn=turn,request=ident):
                            self.release_start.wait(timeout=10)
                            send({'id':request,'result':{'turn':{'id':selected_turn}}})
                            self.broadcast(selected,'turn/started',{'turn':{'id':selected_turn,'status':'inProgress'}})
                            time.sleep(.1);self.finish(selected,selected_turn)
                        threading.Thread(target=late_start,daemon=True).start();continue
                    send({'id':ident,'result':{'turn':{'id':turn}}});self.broadcast(thread,'turn/started',{'turn':{'id':turn,'status':'inProgress'}})
                    if self.mode=='complete' and self.goals.get(thread):self.holding[thread]=turn
                    elif self.mode=='hold':self.holding[thread]=turn
                    else:self.finish(thread,turn)
                elif method=='turn/steer':send({'id':ident,'result':{'turnId':turn}})
                elif method and ident is not None:send({'id':ident,'result':{}})
        finally:self.clients.pop(id(sock),None)
    def has(self,text,topic=7):
        return any(text in m['text'] and m.get('message_thread_id')==topic for m,_ in self.messages)
    def command(self,text,expected,**kwargs):
        previous=len(self.messages);self.admit(text,**kwargs)
        self.wait(lambda:any(expected in m['text'] for m,_ in self.messages[previous:]))
    def idle(self):
        self.wait(lambda:self.row('select count(*) from sessions where busy=1')==[(0,)])


def lifecycle(binary):
    j=Goals(binary)
    try:
        j.start();j.command('/goal','No goal currently set')
        assert not any(x.get('method')=='turn/start' for x in j.rpc)
        j.admit('/goal Complete two native steps')
        j.wait(lambda:j.has('Continuation delivered.'));j.idle()
        assert j.native_turns==2 and sum(x.get('method')=='turn/start' for x in j.rpc)==1
        g=j.goals['native-topic-7'];g.update(status='paused',tokenBudget=200,tokensUsed=81)
        j.command('/goal','Goal: paused');j.command('/goal edit','Edit native goal')
        editor=next(m for m,_ in reversed(j.messages) if m['text'].startswith('Edit native goal'))
        assert any('force_reply'in p.get('reply_markup',{}) for _,p in j.messages)
        j.command('New objective from wrong human','expired',reply=editor['message_id'],user=101)
        assert g['objective']=='Complete two native steps'
        j.command('Edited objective','Goal: paused',reply=editor['message_id'])
        assert g['objective']=='Edited objective' and g['tokenBudget']==200 and g['tokensUsed']==81
        starts=sum(x.get('method')=='turn/start' for x in j.rpc)
        j.command('/goal Replacement objective','Replace the current goal?')
        control=next(x for x in reversed(j.messages) if 'inline_keyboard'in x[1].get('reply_markup',{}))
        callback=j.button(control,':replace')
        j.command(None,'another user/topic',callback=callback,topic=8)
        assert g['objective']=='Edited objective'
        j.admit(callback=callback)
        j.wait(lambda:j.goals.get('native-topic-7',{}).get('objective')=='Replacement objective')
        j.wait(lambda:j.native_turns==4);j.idle()
        assert sum(x.get('method')=='turn/start' for x in j.rpc)==starts+1
        j.command('/goal clear','Goal cleared.');j.command('/goal clear','No goal to clear.')
        before=len(j.rpc);j.stop();j.start();j.command('/goal','No goal currently set')
        assert not any(x.get('method')=='turn/start' for x in j.rpc[before:])
        assert j.row("select count(*) from incoming_updates where status in ('received','processing','queued','started')")==[(0,)]
        print('PASS lifecycle, continuation, summary, ForceReply edit, replacement, isolation, clear, restart')
    finally:j.close()

def live_controls(binary):
    j=Goals(binary,'hold')
    try:
        j.start();j.admit('Hold ordinary native work');j.wait(lambda:bool(j.holding))
        j.command('/goal Set goal during work','Goal: active')
        assert sum(x.get('method')=='turn/start' for x in j.rpc)==1
        j.command('/goal pause','Goal: paused');j.command('/goal resume','Goal: active')
        j.command('/goal clear','Goal cleared.')
        assert not any(x.get('method')=='turn/interrupt' for x in j.rpc)
        thread,turn=next(iter(j.holding.items()));j.finish(thread,turn);j.idle()
        assert j.native_turns==1
        print('PASS live creation, pause/resume/clear, original stream preserved, no extra kickoff/interrupt')
    finally:j.close()

def unsupported(binary):
    j=Goals(binary,'unsupported')
    try:
        j.start();j.admit('Create saved binding');j.wait(lambda:j.has('Initial goal turn delivered.'));j.idle()
        n=sum(x.get('method')=='turn/start' for x in j.rpc)
        j.command('/goal','goals unavailable')
        assert sum(x.get('method')=='turn/start' for x in j.rpc)==n
        print('PASS unsupported API reported without forwarding as model input')
    finally:j.close()

def adopt_live_turn(binary):
    j=Goals(binary,'hold')
    try:
        j.start();j.admit('Seed saved native history');j.wait(lambda:bool(j.holding))
        thread,turn=next(iter(j.holding.items()));j.holding.clear();j.finish(thread,turn);j.idle()
        j.holding[thread]='external-native-turn';j.native_turns+=1
        j.admit('/goal Adopt native work')
        j.wait(lambda:j.goals.get(thread,{}).get('status')=='active')
        assert sum(x.get('method')=='turn/start' for x in j.rpc)==1
        j.finish(thread,'external-native-turn');j.wait(lambda:j.has('Continuation delivered.'));j.idle()
        assert sum(x.get('method')=='turn/start' for x in j.rpc)==1
        print('PASS existing native turn adopted and continuation delivered without duplicate kickoff')
    finally:j.close()

def staged_controls(binary,action):
    j=Goals(binary,'staging')
    try:
        j.start();j.admit('/goal Initial objective')
        j.wait(lambda:j.start_seen.is_set())
        j.command('/goal '+action,'Goal: paused' if action=='pause' else 'Goal cleared.')
        checkpoint=len(j.rpc);j.release_start.set();j.wait(lambda:j.has('Initial goal turn delivered.'));j.idle()
        assert not any(x.get('method')=='thread/goal/set' and x['params'].get('status')=='active' for x in j.rpc[checkpoint:])
        assert not any(x.get('method')=='turn/interrupt' for x in j.rpc)
        assert j.goals.get('native-topic-7',{}).get('status')==('paused' if action=='pause' else None)
        print('PASS '+action+' during staged activation is not overwritten; kickoff finishes without continuation')
    finally:j.release_start.set();j.close()

def delayed_snapshot(binary):
    j=Goals(binary,'delayed_snapshot')
    try:
        j.goals['native-topic-7']={'threadId':'native-topic-7','objective':'Existing active goal','status':'active','tokenBudget':None,'tokensUsed':0,'timeUsedSeconds':0,'createdAt':int(time.time()),'updatedAt':int(time.time())}
        j.start();j.admit('Continue ordinary work with an existing goal')
        j.wait(lambda:j.has('Initial goal turn delivered.'))
        j.wait(lambda:j.has('Continuation delivered.'));j.idle()
        assert j.native_turns==2 and sum(x.get('method')=='turn/start' for x in j.rpc)==1
        print('PASS delayed goal snapshot after first completion retains native continuation')
    finally:j.close()

def long_summary(binary):
    j=Goals(binary)
    try:
        j.start();j.admit('Seed saved native history')
        j.wait(lambda:j.has('Initial goal turn delivered.'));j.idle()
        prefix='Literal *objective*: '
        objective=prefix+('x'*(4000-len(prefix)))
        assert len(objective)==4000
        j.goals['native-topic-7']={'threadId':'native-topic-7','objective':objective,'status':'paused','tokenBudget':None,'tokensUsed':0,'timeUsedSeconds':0,'createdAt':int(time.time()),'updatedAt':int(time.time())}
        previous=len(j.messages);j.admit('/goal')
        j.wait(lambda:any('/goal edit' in m['text'] for m,_ in j.messages[previous:]))
        chunks=[m['text'] for m,_ in j.messages[previous:]]
        assert len(chunks)>=2 and all(len(c)<=4096 for c in chunks),chunks
        assert objective.replace(' ','') in ''.join(chunks).replace(' ','').replace('\n','')
        assert sum(x.get('method')=='turn/start' for x in j.rpc)==1
        print('PASS full-length literal goal summary is chunked without a model turn')
    finally:j.close()

if __name__=='__main__':
    binary=str(pathlib.Path(sys.argv[1]).resolve())
    lifecycle(binary);live_controls(binary);unsupported(binary);staged_controls(binary,'pause');staged_controls(binary,'clear');adopt_live_turn(binary);delayed_snapshot(binary);long_summary(binary)
