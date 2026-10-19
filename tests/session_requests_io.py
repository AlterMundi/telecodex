#!/usr/bin/env python3
"""Attributed local session requests across native/HTTP/SQLite boundaries.
All accounts, turns and model output are simulated; no paid model call.
"""
import json
import pathlib
import sys
from routing_io import Routing


class Sessions(Routing):
    def __init__(self,binary):
        self.outputs={};self.calls=0
        super().__init__(binary)

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
    j=Sessions(binary)
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
        j.answer(8,'Selected dependency completed.')
        j.wait(lambda:len(j.instructions(7))==2 and j.instructions(7)[1][1]=='accepted')
        returned=json.loads(j.instructions(7)[1][2]);assert returned['relation']['kind']=='context'
        assert returned['requests']==json.loads(j.instructions(7)[0][2])['requests']
        packet=j.packets['native-topic-7'][-1]['input'][0]['text']
        provenance=json.loads(next(l.removeprefix('Transport provenance: ') for l in packet.splitlines() if l.startswith('Transport provenance: ')))
        assert provenance['original_human_author'] is None and provenance['author']['kind']=='session_result'
        assert len(j.runs)==2
        ok,status=j.tool(7,'session_requests',{});assert ok and status['requests'][0]['phase']=='returned'
        ok,status=j.tool(7,'session_receive',{'request_id':admitted['request']['id']});assert ok and status['new_return_instruction'] is None
        j.answer(7,'Original task completed with the attributed dependency.')
        j.wait(lambda:j.row("SELECT count(*) FROM interaction_deliveries WHERE phase!='delivered'")==[(0,)] and j.row('SELECT count(*) FROM interaction_deliveries')[0][0]>=2,seconds=25)
        j.idle()
        assert len(j.instructions(8))==1 and len(j.instructions(7))==2
        print('PASS attributed local request, immutable retry, self/recursive refusal, live context return, original request correlation and committed human returns')
    finally:j.close()


def one_slot_deferred_return(binary):
    j=Sessions(binary)
    try:
        j.prepare(max_active_workers=1)
        source_starts=len([r for r in j.rpc if r.get('method')=='turn/start' and r['params']['threadId']=='native-topic-7'])
        j.admit('#Alpha Work and consult the selected existing Beta session.',topic=1)
        j.wait(lambda:j.instructions(7) and j.instructions(7)[0][1]=='accepted')
        _,admitted=j.request()
        assert j.instructions(8)[0][1]=='queued' and 'native-topic-8' not in j.runs
        j.defer(7)
        j.wait(lambda:j.instructions(8)[0][1]=='accepted')
        j.wait(lambda:j.row("SELECT t.status FROM turns t JOIN sessions s ON s.id=t.session_id WHERE s.thread_id=7 ORDER BY t.id DESC LIMIT 1")==[('completed',)])
        j.answer(8,'Dependency returned after the original turn ended.')
        j.wait(lambda:len(j.instructions(7))==2 and j.instructions(7)[1][1]=='accepted')
        assert len([r for r in j.rpc if r.get('method')=='turn/start' and r['params']['threadId']=='native-topic-7'])==source_starts+2
        j.answer(7,'Original logical task resumed using returned evidence.')
        j.wait(lambda:j.row("SELECT count(*) FROM interaction_deliveries WHERE phase!='delivered'")==[(0,)] and j.row('SELECT count(*) FROM interaction_deliveries')[0][0]>=3,seconds=25)
        j.idle()
        assert j.row('SELECT phase FROM interaction_session_requests WHERE id=?',(admitted['request']['id'],))==[('returned',)]
        print('PASS one worker slot: nonblocking dependency admission, deferred parent completion, helper execution, correlated automatic continuation without capacity deadlock')
    finally:j.close()


def cancelled_caller(binary):
    j=Sessions(binary)
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
        j.wait(lambda:j.row("SELECT count(*) FROM interaction_deliveries WHERE phase!='delivered'")==[(0,)] and j.row('SELECT count(*) FROM interaction_deliveries')[0][0]>=1,seconds=25)
        j.idle()
        assert len(j.instructions(7))==1
        assert len([r for r in j.rpc if r.get('method')=='turn/start' and r['params']['threadId']=='native-topic-7'])==source_starts+1
        print('PASS scoped caller interruption withdraws automatic return and causal grant; completed accepted helper work remains canonical without restarting caller')
    finally:j.close()


if __name__=='__main__':
    binary=str(pathlib.Path(sys.argv[1]).resolve())
    live_return(binary)
    one_slot_deferred_return(binary)
    cancelled_caller(binary)
