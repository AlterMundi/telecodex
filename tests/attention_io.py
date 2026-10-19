#!/usr/bin/env python3
"""Twenty retained synthetic contexts with two active native worker reservations.
Qualifies control/return behavior, not real-model throughput or monetary cost.
"""
import json
import html
import threading
import time
import pathlib
import re
import sys
import subprocess
import uuid
from routing_io import Routing


class Load(Routing):
    def __init__(self,binary):
        self.live=set();self.peak=0
        super().__init__(binary)
        self.socket=self.root/'control.sock'
        self.config.write_text(self.config.read_text().replace('[telegram]',
            f'[session_control]\nsocket_path="{self.socket}"\nhuman_id=100\n[telegram]'))

    def read(self,*arguments):
        run=subprocess.run([self.binary,'--control',str(self.config),*map(str,arguments)],
            capture_output=True,text=True,timeout=15)
        assert run.returncode==0,run.stderr
        return json.loads(run.stdout)['result']

    def prepare_load(self,stop=27):
        self.prepare()
        self.routed=False
        for topic in range(9,stop):
            self.admit('Seed existing native conversation',topic=topic)
            self.wait(lambda topic=topic:self.has('Initial goal turn delivered.',topic))
            self.idle()
        self.stop()
        with self.config.open('a') as f:
            for topic in range(9,stop):f.write(f'\n[[interaction.targets]]\nname="Work{topic}"\ntopic_id={topic}\n')
        self.messages.clear();self.routed=True
        self.start()

    def handle_extra_rpc(self,m,send):
        p=m.get('params',{})
        if self.allow_native_launch and m.get('method')=='turn/start' and p.get('clientUserMessageId','').startswith('telegram:'):
            return super().handle_extra_rpc(m,send)
        if self.routed and m.get('method')=='turn/start':
            thread=p['threadId']
            if thread not in {f'native-topic-{n}' for n in range(7,27)}:
                send({'id':m['id'],'result':{'turn':{'id':'front-turn'}}})
                send({'method':'turn/started','params':{'threadId':thread,'turn':{'id':'front-turn','status':'inProgress'}}})
                send({'method':'item/completed','params':{'threadId':thread,'turnId':'front-turn','item':{'id':'front','type':'agentMessage','phase':'final_answer','text':'Front remains responsive.'}}})
                send({'method':'turn/completed','params':{'threadId':thread,'turn':{'id':'front-turn','status':'completed'}}})
                return True
            with self.lock:
                self.live.add(thread);self.peak=max(self.peak,len(self.live))
                assert len(self.live)<=2, 'Worker capacity exceeded'
        return super().handle_extra_rpc(m,send)

    def answer(self,topic,text,**kwargs):
        with self.lock:self.live.remove(f'native-topic-{topic}')
        return super().answer(topic,text,**kwargs)



def attention_snapshot(j, label):
    before=len(j.messages)
    j.admit('/interaction attention',topic=1)
    j.wait(lambda:any('Interaction attention (current listener instance)' in m['text'] for m,_ in j.messages[before:]))
    message=next(m['text'] for m,_ in j.messages[before:] if 'Interaction attention (current listener instance)' in m['text'])
    value=json.loads(html.unescape(re.sub(r'<[^>]+>','',message.split('\n',1)[1])))
    # The human Telegram formatter strips underscore markup, as in the existing
    # waitingtopics assertion below. Normalize that presentation spelling only.
    value={k.replace('_',''):v for k,v in value.items()}
    return {'stage':label,**{k:value[k.replace('_','')] for k in ('limit','active','waiting','started','peak','total_wait_ms','max_wait_ms')},
            'queued_by_session':dict(j.row("SELECT s.thread_id,count(*) FROM interaction_instructions i JOIN sessions s ON s.id=i.session_id WHERE i.phase='queued' GROUP BY s.thread_id"))}


def retained_load(binary):
    j=Load(binary)
    try:
        j.prepare_load()
        j.release_question.set()  # This scenario does not hold the Telegram peer.
        metrics={"schema":"telecodex.attention-qualification/v1","synthetic":True,"model_calls":0,"retained_contexts":20,"snapshots":[]}
        j.admit('Seed the owned integrating entry.',topic=1)
        j.wait(lambda:j.has('Front remains responsive.',1))
        binding=j.read('presentation','list',0,0)['binding']
        j.messages.clear()
        # Canonical requests/results belong to canonical originals. Keep the
        # other eighteen contexts on Telegram to also exercise mixed ingress.
        for name in ('Alpha','Beta'):
            j.read('input','submit',json.dumps({'id':str(uuid.uuid4()),'binding':binding,
                'text':'#'+name+' Begin the selected task.'}))
        j.wait(lambda:all(j.instructions(n) and j.instructions(n)[0][1]=='accepted' for n in (7,8)))
        metrics['snapshots'].append(attention_snapshot(j,'two'))
        for topic in range(9,12):j.admit(f'#Work{topic} Begin the selected task.',topic=1)
        j.wait(lambda:all(j.instructions(n) for n in range(7,12)))
        assert len(j.runs)==2
        metrics['snapshots'].append(attention_snapshot(j,'five'))
        for topic in range(12,27):j.admit(f'#Work{topic} Begin the selected task.',topic=1)
        j.wait(lambda:all(j.instructions(n) for n in range(7,27)))
        assert len(j.runs)==2
        metrics['snapshots'].append(attention_snapshot(j,'twenty'))
        # Native approval remains unresolved while sustained progress, steering
        # and front dialogue compete. These peers perform no inference.
        worker=next(send for thread,send in list(j.clients.values()) if thread=='native-topic-8')
        worker({'id':900,'method':'item/commandExecution/requestApproval','params':{
            'threadId':'native-topic-8','turnId':j.runs['native-topic-8'],'itemId':'attention-approval',
            'command':'synthetic command only','reason':'Keep this request pending during load',
            'availableDecisions':['accept','decline']}})
        j.wait(lambda:j.row("SELECT count(*) FROM human_interactions WHERE state='pending' AND json_extract(payload_json,'$.source.kind')='approval'")==[(1,)])
        stop=threading.Event();progress=[];errors=[]
        def emit():
            try:
                for n in range(5000):
                    if stop.is_set():break
                    j.broadcast('native-topic-7','item/completed',{'turnId':j.runs['native-topic-7'],'item':{
                        'id':'attention-progress-'+str(n),'type':'agentMessage','phase':'commentary','text':'Work remains in progress '+str(n)}})
                    progress.append(n);time.sleep(.01)
            except BaseException as error:errors.append(error)
        burst=threading.Thread(target=emit,daemon=True);burst.start()
        try:
            j.wait(lambda:len(progress)>=5)
            clock=time.monotonic()
            j.admit('#Alpha Correct the active task to 17.',topic=1)
            j.wait(lambda:len(j.instructions(7))==2 and j.instructions(7)[1][1]=='accepted',seconds=3)
            metrics['active_steer_ms']=round((time.monotonic()-clock)*1000,3)
            clock=time.monotonic()
            j.admit('Can we still discuss the work here?',topic=1)
            j.wait(lambda:j.has('Front remains responsive.',1),seconds=3)
            metrics['front_dialogue_ms']=round((time.monotonic()-clock)*1000,3)
            assert burst.is_alive(),'Sustained progress ended before controls were observed'
            assert j.row("SELECT count(*) FROM human_interactions WHERE state='pending' AND json_extract(payload_json,'$.source.kind')='approval'")==[(1,)]
            assert not j.answers,'Load automatically answered an unselected approval'
            # Read the same owner-local sources used by the authenticated client
            # while progress is still arriving, not only after the burst ends.
            alpha=next(send for thread,send in list(j.clients.values()) if thread=='native-topic-7')
            clock=time.monotonic()
            alpha({'id':901,'method':'item/tool/requestUserInput','params':{
                'threadId':'native-topic-7','turnId':j.runs['native-topic-7'],'itemId':'attention-question',
                'isBlocking':False,'questions':[{'id':'scope','header':'Scope',
                    'question':'Choose the scope while other work progresses.','options':None}]}})
            pending=[]
            def question_visible():
                pending[:]=j.read('requests','list',0)['items']
                return any(item['kind']=='question' and item['state']=='pending' for item in pending)
            j.wait(question_visible,seconds=3)
            question=next(item for item in pending if item['kind']=='question')
            detail=j.read('requests','detail',question['id'],0)
            assert detail['actionable'] and 'Choose the scope while other work progresses.' in detail['body']
            assert any(item['kind']=='approval' and item['state']=='pending' for item in pending)
            metrics['pending_question_read_ms']=round((time.monotonic()-clock)*1000,3)
            assert not j.answers,'Reading a pending question answered a native request'
            j.broadcast('native-topic-8','serverRequest/resolved',{'requestId':900})
            clock=time.monotonic();before=len(progress)
            j.answer(8,'Completed worker 8.')
            sources=[]
            def result_visible():
                sources[:]=j.read('presentation','list',0,0)['items']
                return any(item['kind']=='worker_result' and item['preview']=='Completed worker 8.' for item in sources)
            j.wait(result_visible,seconds=3)
            selected=next(item for item in sources if item['kind']=='worker_result' and item['preview']=='Completed worker 8.')
            part=j.read('presentation','part',selected['id'],selected['revision'],0)
            assert part['text']=='Completed worker 8.' and part['current'] and part['completed']
            metrics['completed_result_read_ms']=round((time.monotonic()-clock)*1000,3)
            j.wait(lambda:len(progress)>before,seconds=1)
            assert burst.is_alive(),'Progress ended before the pending question and result were read'
            assert j.read('requests','detail',question['id'],0)['actionable']
            metrics['question_and_result_during_progress']=True
        finally:
            stop.set();burst.join(timeout=3)
        assert not burst.is_alive() and not errors,errors
        metrics['progress_events']=len(progress);metrics['approval_still_pending']=True
        j.broadcast('native-topic-7','serverRequest/resolved',{'requestId':901})
        j.admit('#Work10 Correct the waiting task to 23.',topic=1)
        j.wait(lambda:len(j.instructions(10))==2)
        assert all(row[1]=='queued' for row in j.instructions(10))
        j.admit('#Work26 /interrupt',topic=1)
        j.wait(lambda:j.instructions(26)[0][1]=='retained_unsent')
        j.admit('Can we still discuss the work here?',topic=1)
        j.wait(lambda:j.has('Front remains responsive.',1))
        j.admit('/interaction attention',topic=1)
        j.wait(lambda:j.has('Interaction attention (current listener instance)',1))
        # Releasing Beta may admit the next queued worker asynchronously.
        assert j.peak==2
        # Beta completes before the corrected Alpha; FIFO capacity admits waiting
        # contexts while the front and the already running worker stay usable.
        completed={8}
        while len(completed)<18:
            j.wait(lambda:any(int(t.rsplit('-',1)[1]) not in completed and t!='native-topic-7' for t in list(j.live)))
            topic=next(int(t.rsplit('-',1)[1]) for t in list(j.live) if int(t.rsplit('-',1)[1]) not in completed and t!='native-topic-7')
            j.wait(lambda:all(r[1]=='accepted' for r in j.instructions(topic)))
            if topic==10:
                assert any('Correct the waiting task to 23.' in part.get('text','') for packet in j.packets['native-topic-10'] for part in packet['input'])
            j.answer(topic,f'Completed worker {topic}.');completed.add(topic)
        # Eighteen other contexts (8..25), then the original corrected worker.
        assert completed==set(range(8,26))
        j.answer(7,'Completed worker 7 with correction 17.')
        # Wait for committed Telegram receipts. Canonical originals have no
        # Telegram publication/hearing receipt; inspect their retained sources.
        # Nineteen completed workers plus two separately retained follow-ups.
        j.wait(lambda:j.row("SELECT count(*) FROM interaction_deliveries WHERE phase!='delivered' AND json_extract(doc,'$.origin.message_id') IS NOT NULL")==[(0,)] and j.row('SELECT count(*) FROM interaction_deliveries')==[(21,)],seconds=75)
        j.idle()
        assert j.peak==2 and 'native-topic-26' not in j.runs
        assert j.instructions(26)[0][1]=='retained_unsent'
        assert len(j.runs)==19
        assert all(j.has(f'Completed worker {n}',1) for n in range(9,26))
        final_sources=[];cursor={'delivery':0,'native':0}
        while True:
            page=j.read('presentation','list',cursor['delivery'],cursor['native'])
            final_sources.extend(page['items'])
            if not page['more']:break
            assert page['cursor']!=cursor,'Canonical source cursor stopped advancing'
            cursor=page['cursor']
        assert {'Completed worker 8.','Completed worker 7 with correction 17.'} <= {
            item['preview'] for item in final_sources if item['kind']=='worker_result'}
        metrics['snapshots'].append(attention_snapshot(j,'completed'))
        assert metrics['snapshots'][-1]['active']==metrics['snapshots'][-1]['waiting']==0
        assert all(v['peak']<=2 for v in metrics['snapshots'])
        print('METRICS '+json.dumps(metrics,sort_keys=True))
        print('PASS 2→5→20 retained contexts with two active native workers, live canonical question/result access during progress, corrections, scoped waiting cancellation, independent front, reverse completion and acknowledged correlated returns; synthetic concurrency only')
    finally:j.close()


def direct_goal_bypasses_routing_capacity(binary):
    j=Load(binary)
    try:
        j.prepare_load(10)
        j.allow_native_launch=True
        for name in ('Alpha','Beta'):j.admit(f'#{name} Occupy a worker slot.',topic=1)
        j.wait(lambda:all(j.instructions(n) and j.instructions(n)[0][1]=='accepted' for n in (7,8)))
        objective='Independent native work starts while router capacity is full.'
        j.admit('/goal '+objective,topic=9)
        j.wait(lambda:j.goals.get('native-topic-9',{}).get('status')=='active' and 'native-topic-9' in j.runs)
        assert not j.instructions(9), 'A direct goal became router input'
        quota=attention_snapshot(j,'direct-goal')
        assert quota['active']==2 and quota['waiting']==0, quota
        packet=j.packets['native-topic-9'][-1]
        assert j.goals['native-topic-9']['objective']==objective
        assert '[Interaction controller]' not in str(packet['input']), packet['input']
        j.admit('/goal clear',topic=9)
        j.wait(lambda:j.has('Goal cleared.',9))
        assert 'native-topic-9' not in j.goals
        native=j.runs['native-topic-9']
        j.broadcast('native-topic-9','item/completed',{'turnId':native,'item':{'id':'direct-goal-answer','type':'agentMessage','phase':'final_answer','text':'Independent native work answer.'}})
        j.broadcast('native-topic-9','turn/completed',{'turn':{'id':native,'status':'completed'}})
        j.wait(lambda:j.has('Independent native work answer.',9))
        j.answer(8,'Routed Beta continues independently.')
        j.answer(7,'Routed Alpha continues independently.')
        j.wait(lambda:j.row("SELECT count(*) FROM interaction_deliveries WHERE phase!='delivered'")==[(0,)] and j.row('SELECT count(*) FROM interaction_deliveries')[0][0]==2)
        j.idle()
        assert not any(m.get('method')=='turn/interrupt' for m in j.rpc)
        print('PASS ordinary native goal starts and clears outside a saturated router quota, retains its own answer, and leaves routed work intact')
    finally:j.close()


if __name__=='__main__':
    binary=str(pathlib.Path(sys.argv[1]).resolve())
    retained_load(binary)
    direct_goal_bypasses_routing_capacity(binary)

