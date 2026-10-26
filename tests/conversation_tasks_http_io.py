#!/usr/bin/env python3
"""Canonical durable tasks in the independent HTTP client; synthetic native peers."""
from pathlib import Path
import sys
import json
import sqlite3
import uuid
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from fastapi.testclient import TestClient
from dashboard.app import create_app
from dashboard.provider import Provider
from dashboard.companion import permitted
from live_tasks_io import NotifiedWarm


class TaskJourney(NotifiedWarm):
    def handle_extra_rpc(self,message,send):
        p=message.get('params',{});method=message.get('method')
        if self.routed and method in ('turn/start','turn/steer') and p.get('threadId')=='native-topic-9' and not p.get('clientUserMessageId','').startswith('telegram:'):
            thread=p['threadId'];self.packets.setdefault(thread,[]).append(p)
            if method=='turn/start':
                turn='task-root-'+str(len(self.packets[thread]));self.runs[thread]=turn
                send({'id':message['id'],'result':{'turn':{'id':turn}}})
                self.broadcast(thread,'turn/started',{'turn':{'id':turn,'status':'inProgress'}})
            else:send({'id':message['id'],'result':{'turnId':self.runs[thread]}})
            return True
        return super().handle_extra_rpc(message,send)


def qualify(binary):
    j=TaskJourney(str(Path(binary).resolve()));client=None
    try:
        j.prepare(seed_entry=True);j.allow_native_launch=True
        j.admit('Coordinate selected work.',topic=1)
        j.wait(lambda:'native-topic-9' in j.runs)
        access=j.access(1)
        original=j.cli(access,'inputs')['inputs'][-1]['input_ref']
        j.declare_task=True
        j.cli(access,'dispatch',{'input_ref':original,'destinations':[{'selector':'Alpha','relation':'new','requests':[]}]})
        j.wait(lambda:j.instructions(7) and j.instructions(7)[0][1]=='accepted')
        j.answer(7,'The selected work needs the retained human observation.')
        j.wait(lambda:j.row("SELECT count(*) FROM human_interactions WHERE kind='task' AND state='pending'")==[(1,)])
        task=j.cli(access,'human_tasks')['items'][0]
        config={'receiver_config':str(j.config),'state_dir':str(j.root/'dashboard'),'origin':'https://example.test','bot_id':123}
        provider=Provider(config)
        client=TestClient(create_app(config,provider,lambda raw,bot:int(raw)),base_url=config['origin'])
        assert permitted('GET','/api/interaction/tasks')
        assert permitted('GET','/api/interaction/tasks/'+task['id'])
        assert not permitted('GET','/api/interaction/tasks','monitor_readonly')
        assert permitted('POST','/api/interaction/tasks/'+task['id'])
        assert not permitted('POST','/api/interaction/tasks/'+task['id'],'monitor_readonly')
        assert client.get('/api/interaction/tasks').status_code==401
        login=client.post('/api/session',json={'init_data':'100'},headers={'origin':config['origin']})
        headers={'authorization':'Bearer '+login.json()['session_token']}
        response=client.get('/api/interaction/tasks',headers=headers)
        assert response.status_code==200,response.text
        page=response.json();assert [v['id'] for v in page['items']]==[task['id']]
        assert page['read_only'] and page['items'][0]['kind']=='task'
        detail=client.get('/api/interaction/tasks/'+task['id'],headers=headers)
        assert detail.status_code==200,detail.text
        view=detail.json();assert view['body']=='Synthetic required observation; no real human is contacted.'
        assert view['read_only'] and not view['can_withdraw'] and view['label']=='#Alpha'
        assert client.post('/api/interaction/tasks/'+task['id'],headers=headers,json={'state':'completed'}).status_code in (403,405)
        assert j.row('SELECT state FROM human_interactions WHERE id=?',(task['id'],))==[('pending',)]
        assert client.get('/api/interaction/tasks/'+task['id'],headers=headers,params={'offset':1,'revision':'0'*64}).status_code==409
        # The selected task remains metadata from capture through actual worker
        # delivery, without changing the human text or resolving the action.
        before=len(j.packets['native-topic-7']);original_id=str(uuid.uuid4())
        post_headers={**headers,'origin':config['origin'],'x-dashboard-csrf':page['csrf']}
        for invalid in ({'source':'task:'+task['id'],'revision':view['revision']},
                        {'source':'task:bad-id','revision':view['revision'],'context_only':True}):
            rejected=client.post('/api/interaction/inputs',headers=post_headers,json={'id':str(uuid.uuid4()),'binding':page['binding'],'text':'Invalid reference','reply':invalid})
            assert rejected.status_code==400,rejected.text
        posted=client.post('/api/interaction/inputs',headers=post_headers,json={'id':original_id,'binding':page['binding'],
            'text':'Here is what I observed.','reply':{'source':'task:'+task['id'],'revision':view['revision'],'context_only':True}})
        assert posted.status_code==200,posted.text
        j.wait(lambda:j.row('SELECT phase FROM conversation_inputs WHERE id=?',(original_id,))==[('accepted',)])
        access=j.access(1)
        j.cli(access,'dispatch',{'input_ref':original_id,'destinations':[{'selector':'Alpha','relation':'new','requests':[]}]})
        j.wait(lambda:len(j.packets['native-topic-7'])>before)
        packet=j.packets['native-topic-7'][-1]['input'][0]['text']
        provenance=json.loads(next(line.removeprefix('Transport provenance: ') for line in packet.splitlines() if line.startswith('Transport provenance: ')))
        assert provenance['task_context']['source']=='task:'+task['id']
        assert provenance['task_context']['revision']==view['revision']
        assert provenance['original_human_author']==100 and 'Here is what I observed.' in packet
        retained=json.loads(j.row('SELECT doc FROM conversation_inputs WHERE id=?',(original_id,))[0][0])
        assert retained['text']=='Here is what I observed.'
        assert j.row('SELECT state FROM human_interactions WHERE id=?',(task['id'],))==[('pending',)]
        # Add exact, synthetic ledger records to exercise scope and pagination;
        # the original routed declaration above used the real broker/controller.
        with sqlite3.connect(j.root/'state.sqlite') as db:
            db.row_factory=sqlite3.Row
            base=dict(db.execute('SELECT * FROM human_interactions WHERE id=?',(task['id'],)).fetchone())
            direct=dict(db.execute("SELECT t.id AS local_turn,s.id AS session_id,s.codex_thread_id AS native FROM turns t JOIN sessions s ON s.id=t.session_id WHERE s.thread_id=8 ORDER BY t.id DESC LIMIT 1").fetchone())
            def insert(ident,client,source):
                payload=json.loads(base['payload_json']);payload.update(id=ident,local_turn=source['local_turn'],native_thread=source['native'])
                db.execute("INSERT INTO human_interactions(id,session_id,requester,native_thread,local_turn,kind,state,instance_id,client_id,payload_json,created_at,updated_at) VALUES(?,?,?,?,?,'task','pending',?,?,?,?,?)",(ident,source['session_id'],100,source['native'],source['local_turn'],base['instance_id'],client,json.dumps(payload),base['created_at'],base['updated_at']))
            direct_id=uuid.uuid4().hex;insert(direct_id,'direct-only',direct)
            for n in range(12):insert(uuid.uuid4().hex,'page-'+str(n),dict(session_id=base['session_id'],native=base['native_thread'],local_turn=base['local_turn']))
            db.commit()
        first=client.get('/api/interaction/tasks',headers=headers).json()
        assert len(first['items'])==10 and first['more']
        second=client.get('/api/interaction/tasks',headers=headers,params={'after':first['cursor']}).json()
        assert len(second['items'])==3 and not second['more']
        assert direct_id not in {v['id'] for v in first['items']+second['items']}
        assert client.get('/api/interaction/tasks/'+direct_id,headers=headers).status_code==409
        # Human dismissal uses the same scoped ledger, not a native answer or turn.
        selected=second['items'][0]
        selected=client.get('/api/interaction/tasks/'+selected['id'],headers=headers).json()
        assert selected['can_dismiss'] and selected['state']=='pending'
        decision={'binding':page['binding'],'revision':selected['revision'],'action':'dismiss'}
        path='/api/interaction/tasks/'+selected['id']
        assert client.post(path,headers=headers,json=decision).status_code==403
        assert client.post(path,headers=post_headers,json={'state':'completed'}).status_code==400
        assert client.post(path,headers=post_headers,json={**decision,'revision':'0'*64}).status_code==409
        assert client.post(path,headers=post_headers,json={**decision,'binding':'0'*64}).status_code==409
        assert client.post('/api/interaction/tasks/'+direct_id,headers=post_headers,json=decision).status_code==409
        packets_before=sum(len(v) for v in j.packets.values())
        dismissed=client.post(path,headers=post_headers,json=decision)
        assert dismissed.status_code==200,dismissed.text
        receipt=dismissed.json();assert receipt['dismissed'] and receipt['new_dismissal'] and not receipt['human_completion'] and not receipt['work_cancelled']
        repeated=client.post(path,headers=post_headers,json=decision)
        assert repeated.status_code==200 and not repeated.json()['new_dismissal'],repeated.text
        current=client.get(path,headers=headers).json()
        assert current['state']=='cancelled' and current['dismissal']['revision']==selected['revision'] and not current['can_dismiss']
        assert sum(len(v) for v in j.packets.values())==packets_before,'Dismiss must not start/steer/cancel source work'
        assert j.row("SELECT count(*) FROM audit_log WHERE action='human_task_dismissed'")==[(1,)]
        assert j.row('SELECT state FROM human_interactions WHERE id=?',(direct_id,))==[('pending',)]
        retired=j.cli(access,'withdraw_task',{'id':task['id'],'revision':view['revision'],'reason':'Synthetic later work made this action obsolete; no human completion is asserted.','links':[]})
        assert retired['human_completion'] is False
        closed=client.get('/api/interaction/tasks/'+task['id'],headers=headers).json()
        assert closed['state']=='withdrawn' and closed['body']==view['body']
        with sqlite3.connect(j.root/'state.sqlite') as db:
            db.execute('UPDATE sessions SET force_fresh_thread=1 WHERE id=?',(base['session_id'],));db.commit()
        assert client.get('/api/interaction/tasks',headers=headers).json()['items']==[]
        assert client.get('/api/interaction/tasks/'+task['id'],headers=headers).status_code==409
        print('Durable task HTTP visibility and ownership: passed; zero models or physical devices.')
    finally:
        if client:client.close();provider.close()
        j.close()


if __name__=='__main__':qualify(sys.argv[1])
