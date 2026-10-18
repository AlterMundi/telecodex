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
from routing_io import Routing


class Load(Routing):
    def __init__(self,binary):
        self.live=set();self.peak=0
        super().__init__(binary)

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
        metrics={"schema":"telecodex.attention-qualification/v1","synthetic":True,"model_calls":0,"retained_contexts":20,"snapshots":[]}
        for topic in (7,8):j.admit('#'+('Alpha' if topic==7 else 'Beta')+' Begin the selected task.',topic=1)
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
                for n in range(1000):
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
        finally:
            stop.set();burst.join(timeout=3)
        assert not burst.is_alive() and not errors,errors
        metrics['progress_events']=len(progress);metrics['approval_still_pending']=True
        j.broadcast('native-topic-8','serverRequest/resolved',{'requestId':900})
        j.admit('#Work9 Correct the waiting task to 23.',topic=1)
        j.wait(lambda:len(j.instructions(9))==2)
        assert all(row[1]=='queued' for row in j.instructions(9))
        j.admit('#Work26 /interrupt',topic=1)
        j.wait(lambda:j.instructions(26)[0][1]=='retained_unsent')
        j.admit('Can we still discuss the work here?',topic=1)
        j.wait(lambda:j.has('Front remains responsive.',1))
        j.admit('/interaction attention',topic=1)
        j.wait(lambda:j.has('Interaction attention (current listener instance)',1))
        assert len(j.runs)==2 and j.peak==2
        # Beta completes before the corrected Alpha; FIFO capacity admits waiting
        # contexts while the front and the already running worker stay usable.
        j.answer(8,'Completed worker 8.')
        completed={8}
        while len(completed)<18:
            j.wait(lambda:any(int(t.rsplit('-',1)[1]) not in completed and t!='native-topic-7' for t in list(j.live)))
            topic=next(int(t.rsplit('-',1)[1]) for t in list(j.live) if int(t.rsplit('-',1)[1]) not in completed and t!='native-topic-7')
            j.wait(lambda:all(r[1]=='accepted' for r in j.instructions(topic)))
            if topic==9:
                assert any('Correct the waiting task to 23.' in part.get('text','') for packet in j.packets['native-topic-9'] for part in packet['input'])
            j.answer(topic,f'Completed worker {topic}.');completed.add(topic)
        # Eighteen other contexts (8..25), then the original corrected worker.
        assert completed==set(range(8,26))
        j.answer(7,'Completed worker 7 with correction 17.')
        # The shared Telegram publisher deliberately rate-limits this finite burst.
        # Wait for committed receipts; worker completion alone is not delivery.
        j.wait(lambda:j.row("SELECT count(*) FROM interaction_deliveries WHERE phase!='delivered'")==[(0,)] and j.row('SELECT count(*) FROM interaction_deliveries')[0][0]>=19,seconds=75)
        j.idle()
        assert j.peak==2 and 'native-topic-26' not in j.runs
        assert j.instructions(26)[0][1]=='retained_unsent'
        assert len(j.runs)==19
        assert all(j.has(f'Completed worker {n}',1) for n in range(7,26))
        metrics['snapshots'].append(attention_snapshot(j,'completed'))
        assert metrics['snapshots'][-1]['active']==metrics['snapshots'][-1]['waiting']==0
        assert all(v['peak']<=2 for v in metrics['snapshots'])
        print('METRICS '+json.dumps(metrics,sort_keys=True))
        print('PASS 2→5→20 retained contexts with two active native workers, active and waiting corrections, scoped waiting cancellation, independent front, reverse completion and acknowledged correlated returns; synthetic concurrency only')
    finally:j.close()


def withdrawn_goal(binary):
    j=Load(binary)
    try:
        j.prepare_load(10)
        for name in ('Alpha','Beta'):j.admit(f'#{name} Occupy a worker slot.',topic=1)
        j.wait(lambda:all(j.instructions(n) and j.instructions(n)[0][1]=='accepted' for n in (7,8)))
        assert 'native-topic-9' not in j.goals
        j.admit('/goal This queued objective must never activate.',topic=9)
        j.wait(lambda:j.has('Starting native goal work.',9))
        j.admit('/interaction attention',topic=1)
        j.wait(lambda:any('Interaction attention' in m['text'] and 9 in json.loads(re.sub(r'<[^>]+>', '', m['text'].split('\n',1)[1]))['waitingtopics'] for m,_ in j.messages))
        j.admit('#Work9 Pending human input behind the queued goal.',topic=1)
        j.wait(lambda:j.instructions(9) and j.instructions(9)[0][1]=='queued')
        j.admit('/goal clear',topic=9)
        j.wait(lambda:j.has('No goal to clear.',9))
        j.answer(8,'Release capacity after queued goal withdrawal.')
        j.wait(lambda:j.instructions(9)[0][1]=='retained_unsent')
        j.wait(lambda:j.row("SELECT value FROM bot_state WHERE key='goal_intent:100:9'")!=[])
        assert 'native-topic-9' not in j.goals and 'native-topic-9' not in j.runs
        assert not any(m.get('method')=='thread/goal/set' and m.get('params',{}).get('threadId')=='native-topic-9' for m in j.rpc)
        j.answer(7,'Finish remaining capacity holder.')
        j.wait(lambda:j.row("SELECT count(*) FROM interaction_deliveries WHERE phase!='delivered'")==[(0,)] and j.row('SELECT count(*) FROM interaction_deliveries')[0][0]>=2)
        j.idle()
        print('PASS queued goal revoked during capacity wait cannot activate; pending human input remains known unsent without replay')
    finally:j.close()


if __name__=='__main__':
    binary=str(pathlib.Path(sys.argv[1]).resolve())
    retained_load(binary)
    withdrawn_goal(binary)

