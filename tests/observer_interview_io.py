#!/usr/bin/env python3
"""Selected findings become independent interviews; synthetic peers, zero models."""
import copy
import json
import sqlite3
from pathlib import Path
import sys
from observer_io import Observer
from routing_io import Routing


class Interview(Observer, Routing):
    def __init__(self,binary):
        self.entry=None;self.entry_turn=None;self.entry_send=None
        self.tool_results={};self.topic_calls=0
        super().__init__(binary)

    def created_topic(self,payload):
        assert payload=={'chat_id':100,'name':'InterviewAlpha'},payload
        self.topic_calls+=1
        return {'message_thread_id':77,'name':'InterviewAlpha'}

    def handle_extra_rpc(self,message,send):
        method=message.get('method');params=message.get('params',{});ident=message.get('id')
        if method is None and str(ident).startswith('interview-'):
            self.tool_results[ident]=message['result'];return True
        if method=='turn/start' and not params['threadId'].startswith('summary-'):
            root=self.row('SELECT codex_thread_id FROM sessions WHERE thread_id=1')
            if root and root[0][0]==params['threadId']:
                self.entry=params['threadId'];self.entry_turn='interview-front';self.entry_send=send
                send({'id':ident,'result':{'turn':{'id':self.entry_turn}}})
                send({'method':'turn/started','params':{'threadId':self.entry,'turn':{'id':self.entry_turn,'status':'inProgress'}}})
                return True
        if method=='turn/steer' and params.get('threadId')=='native-topic-7':
            self.inputs['native-topic-7']+='\n'+'\n'.join(part.get('text','') for part in params['input'])
        if super().handle_extra_rpc(message,send):
            return True
        return Routing.handle_extra_rpc(self,message,send)

    def tool(self,name,operation,args,ok=True):
        ident='interview-'+name
        self.entry_send({'id':ident,'method':'item/tool/call','params':{'threadId':self.entry,'turnId':self.entry_turn,'callId':ident,'tool':'interaction_controller','arguments':{'operation':operation,'arguments':args}}})
        self.wait(lambda:ident in self.tool_results)
        result=self.tool_results[ident];assert result['success'] is ok,result
        return json.loads(result['contentItems'][0]['text']) if ok else result


def qualify(binary,busy,large=False):
    j=Interview(binary)
    try:
        j.prepare();j.routed=True
        if large:
            # A real long source exceeded the creation budget even though each
            # observer window was independently bounded. Keep original positions.
            for index in range(14):
                j.histories['native-topic-7'].insert(index,{'id':f'long-source-{index}',
                    'status':'completed','items':[{'type':'agentMessage',
                    'text':'Attributed original source evidence. 叙事 '+('"\\\n'*1100)}]})
        j.admit('/interaction observe #Alpha',topic=1);j.wait(lambda:j.card() is not None)
        original=j.admit('Please interview the work behind the selected Alpha finding. Explain the correction and the alternatives; do not continue or publish its task.',topic=1)
        j.wait(lambda:j.entry_send is not None)
        if busy:
            j.admit('#Alpha Continue the original work with requirement17.',topic=1)
            j.wait(lambda:j.instructions(7) and j.instructions(7)[0][1]=='accepted')
            j.holding['native-topic-7']=j.runs['native-topic-7']
            j.admit('/interaction observe #Alpha',topic=1);j.wait(lambda:j.card()['revision']==2)
        card=copy.deepcopy(j.card())
        inventory=j.tool('inventory','candidates',{'mode':'inventory','selectors':['Alpha']})
        selected=inventory['candidates'][0]
        assert selected['label']=='Alpha' and selected['evidence']['semantic']['revision']==card['revision']
        caps=j.tool('capabilities','capabilities',{});assert 'interview' in caps['operations']
        args={'input_ref':original,'name':'InterviewAlpha','reason':'An explicitly requested interview is independent from continuing the source work.','context_selectors':['Alpha']}
        absent=j.tool('absent-status','creation_status',{'input_ref':original})
        assert absent['phase']=='not_found' and absent['input_ref']==original
        j.tool('foreign-status','creation_status',{'input_ref':original+100000},ok=False)
        assert j.topic_calls==0 and j.row('SELECT count(*) FROM interaction_creations')==[(0,)]
        j.tool('missing-source','interview',{**args,'context_selectors':[]},ok=False)
        assert j.topic_calls==0
        source_before=j.row('SELECT id,codex_thread_id,busy FROM sessions WHERE thread_id=7')
        history_before=copy.deepcopy(j.histories['native-topic-7']);goals_before=copy.deepcopy(j.goals)
        mutations_before=[m for m in j.rpc if m.get('method') in ('turn/start','turn/steer','turn/interrupt') and m.get('params',{}).get('threadId')=='native-topic-7']
        result=j.tool('create','interview',args)
        assert result['kind']=='created_destination' and result['selector']=='t77'
        retained=j.tool('retained-status','creation_status',{'input_ref':original})
        assert retained['phase']=='ready'
        # A corrupt retained intent must never be mistaken for permission to
        # start again. This fixture alone owns its synthetic SQLite database.
        with sqlite3.connect(j.root/'state.sqlite') as db:
            saved=db.execute('SELECT id,doc FROM interaction_creations').fetchone()
            db.execute("UPDATE interaction_creations SET doc='not-json' WHERE id=?",(saved[0],))
        j.tool('corrupt-status','creation_status',{'input_ref':original},ok=False)
        with sqlite3.connect(j.root/'state.sqlite') as db:
            db.execute('UPDATE interaction_creations SET doc=? WHERE id=?',(saved[1],saved[0]))
        assert j.topic_calls==1
        j.wait(lambda:j.instructions(77) and j.instructions(77)[0][1]=='accepted')
        operation=result['operation'];context=operation['intent']['context'];source=context['sources'][0]
        assert context['purpose']=='interview' and context['derivation']=='pinned_observer_sources'
        assert context['exact_native_fork'] is False
        assert source['source_session']==source_before[0][0] and source['source_generation_at_selection']
        assert source['interpretation']==card and source['pinned_originals']['revision']==card['source_revision']
        assert source['pinned_originals']['boundary']==card['boundary']
        assert source['local_work_running_at_selection'] is busy
        assert 'Preserve original narrative' in json.dumps(source['pinned_originals'])
        assert 'Correct the decision' in json.dumps(source['pinned_originals'])
        if large:
            captured=json.loads(j.row('SELECT source_doc FROM interaction_observer_attempts WHERE source_revision=?',(card['source_revision'],))[0][0])
            assert len(json.dumps(captured,ensure_ascii=False).encode())>48000
            assert len(json.dumps(context,ensure_ascii=False,separators=(',',':')).encode())<=48000
            assert source['pinned_originals']['text_truncated'] is True
            assert source['pinned_originals']['interview_excerpt']['truncated_entry_indices']
            assert len(source['pinned_originals']['entries'])==len(captured['entries'])
            assert source['pinned_originals']['turn_ids']==captured['turn_ids']
        child=j.row('SELECT codex_thread_id,session_prompt FROM sessions WHERE thread_id=77')[0]
        assert child[0]!='native-topic-7' and 'independent interview' in child[1]
        packet=j.packets[child[0]][0]['input'][0]['text']
        assert 'pinned_observer_sources' in packet and 'not an exact native fork' in packet
        assert 'Please interview' in packet and 'Transport provenance:' in packet
        assert j.topic_calls==1 and not [m for m in j.rpc if m.get('method')=='thread/fork']
        assert j.row('SELECT id,codex_thread_id,busy FROM sessions WHERE thread_id=7')==source_before
        assert j.histories['native-topic-7']==history_before and j.goals==goals_before
        assert [m for m in j.rpc if m.get('method') in ('turn/start','turn/steer','turn/interrupt') and m.get('params',{}).get('threadId')=='native-topic-7']==mutations_before
        child_send=next(send for native,send in j.clients.values() if native==child[0])
        child_send({'id':'interview-source-request','method':'item/tool/call','params':{'threadId':child[0],'turnId':j.runs[child[0]],'callId':'interview-source-request','tool':'interaction_controller','arguments':{'operation':'session_request','arguments':{'parent_instruction':j.instructions(77)[0][0],'request_id':'cannot-mutate-source','selector':'Alpha','text':'Continue the original source task from this interview.'}}}})
        j.wait(lambda:'interview-source-request' in j.tool_results)
        assert j.tool_results['interview-source-request']['success'] is False
        assert j.row('SELECT count(*) FROM interaction_session_requests')==[(0,)]
        assert [m for m in j.rpc if m.get('method') in ('turn/start','turn/steer','turn/interrupt') and m.get('params',{}).get('threadId')=='native-topic-7']==mutations_before
        # Later source progress is observable but never changes the interview cut.
        j.admit('#Alpha A later correction: requirement23 now supersedes the earlier rate.',topic=1)
        j.wait(lambda:len(j.instructions(7))==(2 if busy else 1) and j.instructions(7)[-1][1]=='accepted')
        j.holding['native-topic-7']=j.runs['native-topic-7']
        j.admit('/interaction observe #Alpha',topic=1);j.wait(lambda:j.card()['revision']>card['revision'])
        status=j.tool('status','creation_status',{'input_ref':original})
        assert status['intent']['context']==context
        assert 'requirement23' not in json.dumps(context) and 'requirement23' in j.inputs['native-topic-7']
        rows=j.instructions(77);requests=sorted({request for _,_,doc in rows for request in json.loads(doc)['requests']})
        claim={'requests':requests,'covered':[row[0] for row in rows],'completed':requests}
        j.broadcast(child[0],'item/completed',{'turnId':j.runs[child[0]],'item':{'id':'interview-answer','type':'agentMessage','phase':'final_answer','text':'The interview explains the pinned correction and retained alternatives, with their source references.\n<daimon-result>'+json.dumps(claim)+'</daimon-result>'}})
        j.broadcast(child[0],'turn/completed',{'turn':{'id':j.runs[child[0]],'status':'completed'}})
        j.wait(lambda:j.has('The interview explains the pinned correction',1))
        assert j.topic_calls==1 and j.row('SELECT busy FROM sessions WHERE thread_id=7')==[(1,)]
        assert j.row('SELECT codex_thread_id FROM sessions WHERE thread_id=7')==[('native-topic-7',)]
        print('PASS selected observer finding → independent attributed text interview → correlated answer; '+('busy' if busy else 'idle')+' source preserved, bounded originals/session/generation/revision/cut frozen, later changes separate, no native-fork claim or publication/peer activation; zero models.')
    finally:j.close()


if __name__=='__main__':
    binary=str(Path(sys.argv[1]).resolve());qualify(binary,False);qualify(binary,True);qualify(binary,True,large=True)
