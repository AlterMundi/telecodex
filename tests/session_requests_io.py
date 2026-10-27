#!/usr/bin/env python3
"""Attributed local session requests across native/HTTP/SQLite boundaries.
All accounts, turns and model output are simulated; no paid model call.
"""
import json
import pathlib
import subprocess
import sqlite3
import sys
from routing_io import Routing


class Sessions(Routing):
    def __init__(self,binary,*,control=False):
        self.outputs={};self.calls=0
        super().__init__(binary)
        self.audience_checks=control
        if not control:return
        self.socket=self.root/'control.sock'
        self.config.write_text(self.config.read_text().replace('[telegram]',f'[session_control]\nsocket_path="{self.socket}"\nhuman_id=100\n[telegram]'))

    def prepare(self,**kwargs):
        super().prepare(**kwargs)
        if not self.audience_checks:return
        self.wait_control_socket(self.socket)
        self.admit('/status',topic=1)
        self.wait(lambda:bool(self.row('SELECT id FROM sessions WHERE thread_id=1')))

    def read(self,*args,ok=True):
        p=subprocess.run([self.binary,'--control',str(self.config),*args],capture_output=True,text=True,timeout=15)
        assert (p.returncode==0)==ok,p.stderr
        return json.loads(p.stdout)

    def internal_delivery(self,text):
        rows=self.row('SELECT d.id,d.doc FROM interaction_deliveries d JOIN interaction_results r ON r.id=d.result_id WHERE r.body=?',(text,))
        assert rows and all(json.loads(doc)['internal'] for _,doc in rows)
        for ident,_ in rows:
            assert self.row('SELECT count(*) FROM interaction_public_deliveries WHERE id=?',(ident,))==[(0,)]
            assert self.row('SELECT count(*) FROM interaction_delivery_parts WHERE delivery_id=?',(ident,))==[(0,)]
            # Simulate a client that retained this source's valid public
            # revision before the scope correction. Only this synthetic fixture
            # mutates its own document; the runtime never exposes an override.
            with sqlite3.connect(self.root/'state.sqlite') as db:
                db.execute("UPDATE interaction_deliveries SET doc=json_set(doc,'$.internal',json('false')) WHERE id=?",(ident,))
            try:
                before=self.read('presentation','list','0','0')['result']
                cached=next(i for i in before['items'] if i['id']=='delivery:'+ident)
            finally:
                with sqlite3.connect(self.root/'state.sqlite') as db:
                    db.execute("UPDATE interaction_deliveries SET doc=json_set(doc,'$.internal',json('true')) WHERE id=?",(ident,))
            self.read('presentation','part','delivery:'+ident,cached['revision'],'0',ok=False)
        assert not self.has(text,1)
        page=self.read('presentation','list','0','0')['result']
        assert not any(text in i.get('preview','') for i in page['items'])
        return rows

    def handle_extra_rpc(self,m,send):
        if not m.get('method') and str(m.get('id','')).startswith('local-call-'):
            self.outputs[m['id']]=m['result'];return True
        return super().handle_extra_rpc(m,send)

    def tool(self,topic,operation,arguments):
        self.calls+=1;ident=f'local-call-{self.calls}';thread=f'native-topic-{topic}'
        turn=self.runs[thread]
        send=next(send for selected,send in list(self.clients.values()) if selected==thread)
        send({'id':ident,'method':'item/tool/call','params':{'threadId':thread,'turnId':turn,'callId':ident,'tool':'interaction_controller','namespace':None,'arguments':{'operation':operation,'arguments':arguments}}})
        self.wait(lambda:ident in self.outputs)
        result=self.outputs[ident]
        return result['success'],json.loads(result['contentItems'][0]['text']) if result['success'] else result['contentItems'][0]['text']

    def request(self):
        parent=self.instructions(7)[0][0]
        args={'parent_instruction':parent,'request_id':'selected-dependency','selector':'/self#Beta','text':'Session Alpha asks Beta for the selected dependency answer. This text is written by Alpha, not the human.'}
        success,result=self.tool(7,'session_request',args)
        assert success,result
        return args,result

    def defer(self,topic):
        thread=f'native-topic-{topic}';turn=self.runs[thread];rows=self.instructions(topic)
        requests=sorted({r for row in rows for r in json.loads(row[2])['requests']})
        claim={'requests':requests,'covered':[row[0] for row in rows],'completed':[]}
        self.broadcast(thread,'item/completed',{'turnId':turn,'item':{'id':'deferred','type':'agentMessage','phase':'final_answer','text':'Independent work finished; selected dependency deferred.\n<daimon-result>'+json.dumps(claim)+'</daimon-result>'}})
        self.broadcast(thread,'turn/completed',{'turn':{'id':turn,'status':'completed'}})


def live_return(binary):
    j=Sessions(binary,control=True)
    try:
        j.prepare()
        j.admit('/self#Alpha Work on the selected task and consult Beta if useful.',topic=1)
        j.wait(lambda:j.instructions(7) and j.instructions(7)[0][1]=='accepted')
        ok,caps=j.tool(7,'capabilities',{});assert ok and 'session_request' in caps['operations']
        ok,inputs=j.tool(7,'inputs',{});assert ok and inputs['inputs'][0]['eligible_human_parent']
        args,admitted=j.request()
        j.wait(lambda:j.instructions(8) and j.instructions(8)[0][1]=='accepted')
        helper=j.instructions(8)[0][0]
        ok,repeated=j.tool(7,'session_request',args);assert ok and not repeated['new_admission']
        assert repeated['request']['id']==admitted['request']['id'] and len(j.instructions(8))==1
        ok,_=j.tool(7,'session_request',{**args,'text':'Changed bytes under the same id'});assert not ok
        ok,_=j.tool(7,'session_request',{**args,'request_id':'self','selector':'Alpha'});assert not ok
        ok,inputs=j.tool(8,'inputs',{});assert ok and not inputs['inputs'][0]['eligible_human_parent']
        ok,_=j.tool(8,'session_request',{**args,'parent_instruction':helper,'request_id':'recursive','selector':'Alpha'});assert not ok
        # Missing and incomplete declarations are retained but never public or
        # treated as a completed helper dependency.
        thread='native-topic-8';turn=j.runs[thread]
        j.broadcast(thread,'item/completed',{'turnId':turn,'item':{'id':'unreported','type':'agentMessage','phase':'final_answer','text':'Unreported helper evidence.'}})
        j.wait(lambda:j.row("SELECT count(*) FROM interaction_native_messages WHERE text='Unreported helper evidence.'")==[(1,)])
        j.answer(8,'Incomplete helper evidence.',covered=[],item='partial',finish=False)
        j.wait(lambda:j.row("SELECT count(*) FROM interaction_results WHERE body='Incomplete helper evidence.'")==[(1,)])
        assert j.row('SELECT phase FROM interaction_session_requests')==[('requested',)]
        j.internal_delivery('Incomplete helper evidence.')
        assert len(j.instructions(7))==1
        j.answer(8,'Selected dependency completed.')
        j.wait(lambda:len(j.instructions(7))==2 and j.instructions(7)[1][1]=='accepted')
        returned=json.loads(j.instructions(7)[1][2]);assert returned['relation']['kind']=='context'
        assert returned['requests']==json.loads(j.instructions(7)[0][2])['requests']
        packet=j.packets['native-topic-7'][-1]['input'][0]['text']
        provenance=json.loads(next(l.removeprefix('Transport provenance: ') for l in packet.splitlines() if l.startswith('Transport provenance: ')))
        assert provenance['original_human_author'] is None and provenance['author']['kind']=='session_result'
        j.internal_delivery('Selected dependency completed.')
        assert len(j.runs)==2
        ok,status=j.tool(7,'session_requests',{});assert ok and status['requests'][0]['phase']=='returned'
        ok,status=j.tool(7,'session_receive',{'request_id':admitted['request']['id']});assert ok and status['new_return_instruction'] is None
        j.answer(7,'Original task completed with the attributed dependency.')
        j.wait(lambda:j.row("SELECT count(*) FROM interaction_public_deliveries WHERE phase!='delivered'")==[(0,)] and j.row('SELECT count(*) FROM interaction_public_deliveries')[0][0]>=1,seconds=25)
        j.idle()
        assert len(j.instructions(8))==1 and len(j.instructions(7))==2
        assert sum('Original task completed with the attributed dependency.' in m['text'] for m,_ in j.messages)==1
        page=j.read('presentation','list','0','0')['result']
        public=[i for i in page['items'] if 'Original task completed with the attributed dependency.' in i.get('preview','')]
        assert len(public)==1 and public[0]['sequence']>0
        print('PASS attributed local request, immutable retry, self/recursive refusal, live context return, original request correlation and committed human returns')
    finally:j.close()


def one_slot_deferred_return(binary):
    j=Sessions(binary,control=True)
    try:
        j.prepare(max_active_workers=1)
        source_starts=len([r for r in j.rpc if r.get('method')=='turn/start' and r['params']['threadId']=='native-topic-7'])
        original=j.admit('#Alpha Work and consult the selected existing Beta session.',topic=1)
        j.wait(lambda:j.instructions(7) and j.instructions(7)[0][1]=='accepted')
        _,admitted=j.request()
        assert j.instructions(8)[0][1]=='queued' and 'native-topic-8' not in j.runs
        j.defer(7)
        j.wait(lambda:j.instructions(8)[0][1]=='accepted')
        j.wait(lambda:j.row("SELECT t.status FROM turns t JOIN sessions s ON s.id=t.session_id WHERE s.thread_id=7 ORDER BY t.id DESC LIMIT 1")==[('completed',)])
        j.answer(8,'Dependency returned after the original turn ended.')
        j.wait(lambda:len(j.instructions(7))==2 and j.instructions(7)[1][1]=='accepted')
        assert len([r for r in j.rpc if r.get('method')=='turn/start' and r['params']['threadId']=='native-topic-7'])==source_starts+2
        rows=j.instructions(7);returned=rows[-1][0]
        claim={'requests':json.loads(rows[0][2])['requests'],'covered':[returned],'completed':[],'deferred':json.loads(rows[0][2])['requests']}
        j.broadcast('native-topic-7','item/completed',{'turnId':j.runs['native-topic-7'],'item':{'id':'internal-narration','type':'agentMessage','phase':'final_answer','text':'Original logical task resumed using returned evidence.\n<daimon-result>'+json.dumps(claim)+'</daimon-result>'}})
        j.wait(lambda:j.row("SELECT count(*) FROM interaction_results WHERE body='Original logical task resumed using returned evidence.'")==[(1,)])
        hidden=j.internal_delivery('Original logical task resumed using returned evidence.')
        # A genuine later human correction keeps its own public reply. Retained
        # internal evidence must stay hidden even after this input joins the turn.
        j.admit('#Alpha Correct the selected original requirement.',topic=1,reply=original)
        j.wait(lambda:len(j.instructions(7))==3 and j.instructions(7)[2][1]=='accepted')
        j.internal_delivery('Original logical task resumed using returned evidence.')
        j.answer(7,'Corrected original task completed using internal evidence.',item='corrected-final')
        j.wait(lambda:j.row("SELECT count(*) FROM interaction_public_deliveries WHERE phase!='delivered'")==[(0,)] and j.row('SELECT count(*) FROM interaction_public_deliveries')[0][0]>=2,seconds=25)
        j.idle()
        assert j.row('SELECT phase FROM interaction_session_requests WHERE id=?',(admitted['request']['id'],))==[('returned',)]
        assert sum('Corrected original task completed using internal evidence.' in m['text'] for m,_ in j.messages)==1
        j.stop();j.start();j.wait_control_socket(j.socket)
        j.internal_delivery('Original logical task resumed using returned evidence.')
        # Pre-upgrade records had no frozen audience. Retained authorship/coverage
        # filters those records without deleting delivery receipts or native text.
        with sqlite3.connect(j.root/'state.sqlite') as db:
            for ident,_ in hidden:
                db.execute("UPDATE interaction_deliveries SET doc=json_remove(doc,'$.internal') WHERE id=?",(ident,))
        assert all(j.row('SELECT count(*) FROM interaction_public_deliveries WHERE id=?',(ident,))==[(0,)] for ident,_ in hidden)
        page=j.read('presentation','list','0','0')['result']
        assert any('Corrected original task completed using internal evidence.' in i.get('preview','') for i in page['items'])
        print('PASS one worker slot: nonblocking dependency admission, deferred parent completion, helper execution, correlated automatic continuation without capacity deadlock')
    finally:j.close()


def deferred_logical_completion(binary):
    j=Sessions(binary,control=True)
    try:
        j.prepare(max_active_workers=1)
        j.admit('#Alpha Complete the original work after consulting Beta.',topic=1)
        j.wait(lambda:j.instructions(7) and j.instructions(7)[0][1]=='accepted')
        _,admitted=j.request();j.defer(7)
        j.wait(lambda:j.instructions(8) and j.instructions(8)[0][1]=='accepted')
        j.answer(8,'Internal completed dependency for unfinished human work.')
        j.wait(lambda:len(j.instructions(7))==2 and j.instructions(7)[1][1]=='accepted')
        j.answer(7,'The original human task is now complete with cumulative coverage.')
        j.wait(lambda:j.has('The original human task is now complete with cumulative coverage.',1));j.idle()
        assert sum('The original human task is now complete with cumulative coverage.' in m['text'] for m,_ in j.messages)==1
        j.internal_delivery('Internal completed dependency for unfinished human work.')
        print('PASS a genuinely unfinished human task can complete once after an internal return, covering original and returned instructions across physical executions')
    finally:j.close()


def completed_group_not_renarrated(binary,mixed=False):
    j=Sessions(binary,control=True)
    try:
        j.prepare()
        j.admit('#Alpha First independently requested task.',topic=1)
        j.wait(lambda:j.instructions(7) and j.instructions(7)[0][1]=='accepted')
        j.admit('#Alpha Second independently requested task.',topic=1)
        j.wait(lambda:len(j.instructions(7))==2 and j.instructions(7)[1][1]=='accepted')
        _,admitted=j.request()
        j.wait(lambda:j.instructions(8) and j.instructions(8)[0][1]=='accepted')
        rows=j.instructions(7);requests=[json.loads(row[2])['requests'][0] for row in rows]
        assert requests[0]!=requests[1]
        claim={'requests':requests,'covered':[r[0] for r in rows],'completed':requests[:1] if mixed else requests}
        j.broadcast('native-topic-7','item/completed',{'turnId':j.runs['native-topic-7'],'item':{'id':'grouped-human','type':'agentMessage','phase':'final_answer','text':'Original grouped human answer.\n<daimon-result>'+json.dumps(claim)+'</daimon-result>'}})
        j.broadcast('native-topic-7','turn/completed',{'turn':{'id':j.runs['native-topic-7'],'status':'completed'}})
        j.wait(lambda:j.has('Original grouped human answer.',1))
        prior=j.row("SELECT d.request_id,d.doc FROM interaction_public_deliveries d JOIN interaction_results r ON r.id=d.result_id WHERE r.body='Original grouped human answer.'")
        assert len(prior)==1 and prior[0][0]==requests[1],'Exercise the non-primary first request'
        assert json.loads(prior[0][1])['completed'] is (not mixed)
        j.answer(8,'Helper finished after original human work completed.')
        j.wait(lambda:len(j.instructions(7))==3 and j.instructions(7)[2][1]=='accepted')
        j.answer(7,'Unwanted repeated completion of the first original task.',item='unwanted-repeat')
        j.wait(lambda:j.row("SELECT count(*) FROM interaction_results WHERE body='Unwanted repeated completion of the first original task.'")==[(1,)])
        j.internal_delivery('Unwanted repeated completion of the first original task.')
        j.idle()
        assert sum('Original grouped human answer.' in m['text'] for m,_ in j.messages)==1
        print('PASS already completed secondary human request stays internal after helper return; mixed completion='+str(mixed))
    finally:j.close()


def cancelled_caller(binary):
    j=Sessions(binary,control=True)
    try:
        j.prepare()
        source_starts=len([r for r in j.rpc if r.get('method')=='turn/start' and r['params']['threadId']=='native-topic-7'])
        j.admit('#Alpha Selected original task.',topic=1)
        j.wait(lambda:j.instructions(7) and j.instructions(7)[0][1]=='accepted')
        _,admitted=j.request()
        j.wait(lambda:j.instructions(8) and j.instructions(8)[0][1]=='accepted')
        j.admit('#Alpha /interrupt',topic=1)
        j.wait(lambda:j.row('SELECT phase FROM interaction_session_requests WHERE id=?',(admitted['request']['id'],))==[('withdrawn',)])
        j.answer(8,'Completed already accepted helper work after caller interruption.')
        j.wait(lambda:j.row('SELECT count(*) FROM interaction_deliveries')[0][0]>=1,seconds=25)
        j.internal_delivery('Completed already accepted helper work after caller interruption.')
        j.idle()
        assert len(j.instructions(7))==1
        assert len([r for r in j.rpc if r.get('method')=='turn/start' and r['params']['threadId']=='native-topic-7'])==source_starts+1
        print('PASS scoped caller interruption withdraws automatic return and causal grant; completed accepted helper work remains canonical without restarting caller')
    finally:j.close()


if __name__=='__main__':
    binary=str(pathlib.Path(sys.argv[1]).resolve())
    live_return(binary)
    one_slot_deferred_return(binary)
    deferred_logical_completion(binary)
    completed_group_not_renarrated(binary)
    completed_group_not_renarrated(binary,mixed=True)
    cancelled_caller(binary)
