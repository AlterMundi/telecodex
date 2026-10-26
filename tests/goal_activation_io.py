#!/usr/bin/env python3
"""Goal activation and return through real controllers/collectors, synthetic native peers."""
import copy
import json
from pathlib import Path
import subprocess
import sys
import uuid
from goal_controls_io import Controls

class Activation(Controls):
    deny_alpha_resume=False
    deny_followup=False
    lose_followup=False
    def owner(self,*args):
        result=subprocess.run([self.binary,'--control',str(self.config),*args],capture_output=True,text=True,timeout=20)
        assert result.returncode==0,result.stderr
        return json.loads(result.stdout)['result']

    def handle_extra_rpc(self,message,send):
        method=message.get('method');p=message.get('params',{})
        if self.routed and method=='thread/resume' and p.get('threadId')=='native-topic-7' and self.deny_alpha_resume:
            self.deny_alpha_resume=False
            send({'id':message['id'],'error':{'code':-32600,'message':'Synthetic transient resume failure before goal submission'}})
            return True
        if self.routed and method in ('turn/start','turn/steer') and p.get('clientUserMessageId','').startswith('conversation:'):
            thread=p['threadId'];turn=self.runs.setdefault(thread,'web-'+str(len(self.runs)+1))
            self.packets.setdefault(thread,[]).append(p)
            send({'id':message['id'],'result':{'turn':{'id':turn}} if method=='turn/start' else {'turnId':turn}})
            if method=='turn/start':self.broadcast(thread,'turn/started',{'turn':{'id':turn,'status':'inProgress'}})
            return True
        if method in ('turn/start','turn/steer') and p.get('clientUserMessageId','').startswith('goal-followup:'):
            thread=p['threadId']
            selected=p['clientUserMessageId'].split(':')[1]
            checkpoint=json.loads(self.row('SELECT result FROM interaction_goal_controls WHERE id=?',(selected,))[0][0])
            assert checkpoint['native_receipt']['revision'] and checkpoint['followup_admitted'] is None
            if self.deny_followup:
                self.deny_followup=False
                send({'id':message['id'],'error':{'code':-32600,'message':'Synthetic follow-up rejection after committed goal control'}})
                return True
            turn=p['expectedTurnId'] if method=='turn/steer' else 'followup-'+str(len(self.rpc))
            assert p['additionalContext'] and p['input'][0]['text']
            self.runs[thread]=turn;self.holding[thread]=turn
            self.packets.setdefault(thread,[]).append(p)
            if self.lose_followup:
                self.lose_followup=False
                send({'id':message['id'],'error':{'code':-32603,'message':'Synthetic internal failure after follow-up effect'}})
            else:
                send({'id':message['id'],'result':{'turn':{'id':turn}} if method=='turn/start' else {'turnId':turn}})
                if method=='turn/start':self.broadcast(thread,'turn/started',{'turn':{'id':turn,'status':'inProgress'}})
            return True
        if method=='thread/goal/conditionalActivate':
            thread=p['threadId'];revision=str(self.revisions.get(thread,0))
            assert p['additionalContext'] and p['origin']==self.expected_origin
            assert len(p['additionalContext'])<=8 and len(json.dumps(p['additionalContext'],separators=(',',':')).encode())<=32768
            if revision!=p['expectedRevision']:
                send({'id':message['id'],'error':{'code':-32600,'message':'goal changed'}});return True
            existing=self.holding.get(thread)
            turn=existing or 'goal-'+str(len(self.rpc))
            goal=self.goals.setdefault(thread,{'threadId':thread,'objective':p.get('objective',''),'status':'paused','tokenBudget':100,'tokensUsed':0,'timeUsedSeconds':0,'createdAt':1,'updatedAt':1})
            goal.update(status='active')
            if 'objective' in p:goal['objective']=p['objective']
            self.revisions[thread]=int(revision)+1
            self.runs[thread]=turn;self.holding[thread]=turn
            self.packets.setdefault(thread,[]).append(p)
            send({'id':message['id'],'result':{'goal':copy.deepcopy(goal),'revision':str(int(revision)+1),'activation':{'kind':'steered' if existing else 'started','turnId':turn}}})
            self.broadcast(thread,'thread/goal/updated',{'goal':copy.deepcopy(goal)})
            if not existing:self.broadcast(thread,'turn/started',{'turn':{'id':turn,'status':'inProgress'}})
            return True
        return super().handle_extra_rpc(message,send)

    def finish_goal(self,thread,text,complete_goal=True):
        turn=self.runs[thread]
        self.broadcast(thread,'item/completed',{'turnId':turn,'item':{'id':turn+'-final','type':'agentMessage','phase':'final_answer','text':text}})
        if complete_goal:
            self.goals[thread]['status']='complete'
            self.revisions[thread]+=1
            self.broadcast(thread,'thread/goal/updated',{'goal':copy.deepcopy(self.goals[thread])})
        self.broadcast(thread,'turn/completed',{'turn':{'id':turn,'status':'completed'}})
        self.holding.pop(thread,None)


def qualify(binary):
    j=Activation(binary)
    try:
        j.prepare(seed_entry=True);j.allow_native_launch=True
        j.wait_control_socket(j.socket)
        binding=j.owner('presentation','list','0','0')['binding']
        def submit(text):
            ident=str(uuid.uuid4());j.owner('input','submit',json.dumps({'id':ident,'binding':binding,'text':text}))
            j.wait(lambda:j.row('SELECT phase FROM conversation_inputs WHERE id=?',(ident,))==[('accepted',)])
            return ident
        submit('Please coordinate native goal work.')
        entry=j.access(1)
        def prepare(action,selector,text,objective=None,followup=None):
            ident=submit(text)
            reference=next(v['input_ref'] for v in j.cli(entry,'inputs')['inputs'] if v['input_ref']==ident)
            observed=j.cli(entry,'goal_inspect',{'selector':selector})
            return j.cli(entry,'goal_prepare',{'selector':selector,'target_binding':observed['target_binding'],'input_ref':reference,'reason':'Explicit synthetic human goal command','plan':{'action':action,'expected':observed['snapshot'],'objective':objective,'token_budget':None,'followup':followup}})['control']['id']
        initial_starts=sum(m.get('method')=='turn/start' and m.get('params',{}).get('threadId')=='native-topic-7' for m in j.rpc)
        selected=prepare('start','Alpha','Start Alpha with the goal: solve the selected complex problem. Then explain the next step.','Solve the selected complex problem.',followup='explain the next step.')
        j.deny_alpha_resume=True
        j.cli(entry,'goal_execute',{'id':selected})
        j.wait(lambda:j.row('SELECT phase FROM interaction_goal_controls WHERE id=?',(selected,))==[('known_unsent',)])
        failed=j.cli(entry,'goal_status',{'id':selected})
        original_count=j.row('SELECT count(*) FROM conversation_inputs')
        assert not any(m.get('method')=='thread/goal/conditionalActivate' for m in j.rpc)
        j.cli(entry,'goal_execute',{'id':selected})
        j.wait(lambda:j.row('SELECT phase FROM interaction_goal_controls WHERE id=?',(selected,))==[('acknowledged',)])
        retried=j.cli(entry,'goal_status',{'id':selected})
        assert retried['intent']['attempt']!=failed['intent']['attempt']
        assert retried['intent']['previous_outcomes'][0]['phase']=='known_unsent'
        assert j.row('SELECT count(*) FROM conversation_inputs')==original_count,'No second human authorization/input was fabricated'

        first=j.cli(entry,'goal_status',{'id':selected})
        assert first['intent']['worker_execution']['native']=='native-topic-7'
        packet=j.packets['native-topic-7'][-1]
        assert 'controller fallback' in json.dumps(packet['additionalContext'])
        assert 'explain the next step.' in packet['additionalContext']['telecodex.goal.followup']['value']
        assert first['result']['followup_admitted'] is True
        assert sum(m.get('method')=='turn/start' and m.get('params',{}).get('threadId')=='native-topic-7' for m in j.rpc)==initial_starts,'No artificial kickoff/priming turn'
        count=sum(m.get('method')=='thread/goal/conditionalActivate' for m in j.rpc)
        j.cli(entry,'goal_execute',{'id':selected})
        assert sum(m.get('method')=='thread/goal/conditionalActivate' for m in j.rpc)==count
        j.finish_goal('native-topic-7','The first goal result returned once.')
        j.wait(lambda:j.row('SELECT busy FROM sessions WHERE thread_id=7')==[(0,)])
        visible=j.cli(entry,'conversation')['presentation']['items']
        assert sum(item.get('preview')=='The first goal result returned once.' and item.get('kind')=='worker_result' for item in visible)==1,visible
        # A busy direct session receives a goal/context on its own collector.
        j.admit('Existing direct Beta work.',topic=8);j.wait(lambda:'native-topic-8' in j.runs)
        existing=j.runs['native-topic-8'];j.holding['native-topic-8']=existing
        selected=prepare('start','Beta','Start Beta with the goal: finish the current work.','Finish the current work.')
        before=sum(m.get('method')=='turn/start' for m in j.rpc)
        result=j.cli(entry,'goal_execute',{'id':selected})
        assert result['phase']=='acknowledged' and j.runs['native-topic-8']==existing
        assert sum(m.get('method')=='turn/start' for m in j.rpc)==before
        # A routed correction uses its canonical delivery, never a second native goal answer.
        correction=submit('Use requirement seventeen in the current Beta work.')
        routed=j.cli(entry,'dispatch',{'input_ref':correction,'destinations':[{'selector':'Beta','relation':'new','requests':[]}]})
        j.wait(lambda:j.instructions(8) and j.instructions(8)[-1][1]=='accepted')
        instruction=json.loads(j.instructions(8)[-1][2])
        footer=json.dumps({'requests':instruction['requests'],'covered':[instruction['id']],'completed':instruction['requests'],'deferred':[]},separators=(',',':'))
        j.finish_goal('native-topic-8','The corrected Beta goal result.\n<daimon-result>'+footer+'</daimon-result>')
        j.wait(lambda:j.row('SELECT busy FROM sessions WHERE thread_id=8')==[(0,)])
        j.wait(lambda:j.row("SELECT count(*) FROM interaction_deliveries")==[(1,)])
        assert j.row("SELECT phase FROM interaction_deliveries")==[('reserved',)],'A canonical web result has no Telegram/hearing receipt'
        visible=j.cli(entry,'conversation')['presentation']['items']
        matching=[item for item in visible if item.get('preview')=='The corrected Beta goal result.']
        assert len(matching)==1 and matching[0]['id'].startswith('delivery:'),visible
        # A later direct input reselects Telegram without router admission.
        selected=prepare('resume','Alpha','Resume the current Alpha goal.')
        j.cli(entry,'goal_execute',{'id':selected})
        j.wait(lambda:j.row('SELECT phase FROM interaction_goal_controls WHERE id=?',(selected,))==[('acknowledged',)])
        j.admit('Continue directly here in Alpha.',topic=7)
        j.wait(lambda:j.row("SELECT source_input FROM conversation_returns WHERE native_thread='native-topic-7'")[0][0].startswith('telegram:'))
        # Selecting a surface precedes the actual native steer receipt. Do not
        # finish its synthetic turn while the original is still entering it.
        j.wait(lambda:j.row("SELECT status FROM incoming_updates WHERE json_extract(payload_json,'$.message.text')='Continue directly here in Alpha.'")==[('started',)])
        j.finish_goal('native-topic-7','This later goal reply belongs to direct Alpha.')
        j.wait(lambda:j.row('SELECT busy FROM sessions WHERE thread_id=7')==[(0,)])
        visible=j.cli(entry,'conversation')['presentation']['items']
        assert not any(item.get('preview')=='This later goal reply belongs to direct Alpha.' for item in visible)
        j.wait(lambda:j.has('This later goal reply belongs to direct Alpha.',7))
        assert j.row('SELECT count(*) FROM interaction_sources')==[(1,)],'Only the explicit correction was a router input'
        # Metadata controls admit only the actual selected clause after the CAS,
        # preserving inactive/absent goal state and the same canonical return path.
        for action,clause,objective in [('pause','Explain the paused work.',None),('edit','Explain the revised work.','Revised paused objective.'),('clear','Discuss a fresh idea.',None)]:
            selected=prepare(action,'Alpha',f'{action} Alpha goal {objective or ""}, then {clause}',objective=objective,followup=clause)
            original_count=j.row('SELECT count(*) FROM conversation_inputs')
            j.cli(entry,'goal_execute',{'id':selected})
            j.wait(lambda:j.row('SELECT phase FROM interaction_goal_controls WHERE id=?',(selected,))==[('acknowledged',)])
            result=j.cli(entry,'goal_status',{'id':selected})
            assert result['result']['followup_admitted'] is True
            assert j.row('SELECT count(*) FROM conversation_inputs')==original_count
            assert clause in j.packets['native-topic-7'][-1]['input'][0]['text'], (action,clause,j.packets['native-topic-7'][-1]['input'][0]['text'],result['result'])
            assert j.goals.get('native-topic-7',{}).get('status')==('paused' if action!='clear' else None)
            j.finish_goal('native-topic-7',action+' continuation result.',complete_goal=False)
            j.wait(lambda:j.row('SELECT busy FROM sessions WHERE thread_id=7')==[(0,)])
            visible=j.cli(entry,'conversation')['presentation']['items']
            assert sum(item.get('preview')==action+' continuation result.' for item in visible)==1
        # Existing busy writer: no extra physical turn and no goal reactivation.
        previous_beta=j.runs['native-topic-8']
        j.admit('Direct Beta work before pausing.',topic=8)
        j.wait(lambda:j.row('SELECT busy FROM sessions WHERE thread_id=8')==[(1,)] and j.runs.get('native-topic-8')!=previous_beta)
        existing=j.runs['native-topic-8'];j.holding['native-topic-8']=existing
        selected=prepare('pause','Beta','Pause Beta, then explain what remains.',followup='explain what remains.')
        before=sum(m.get('method')=='turn/start' for m in j.rpc)
        j.cli(entry,'goal_execute',{'id':selected})
        j.wait(lambda:j.row('SELECT phase FROM interaction_goal_controls WHERE id=?',(selected,))==[('acknowledged',)])
        assert j.runs['native-topic-8']==existing and j.goals['native-topic-8']['status']=='paused'
        assert sum(m.get('method')=='turn/start' for m in j.rpc)==before
        j.finish_goal('native-topic-8','Busy paused continuation.',complete_goal=False)
        j.wait(lambda:j.row('SELECT busy FROM sessions WHERE thread_id=8')==[(0,)])
        j.wait(lambda:j.has('Busy paused continuation.',8),seconds=5)
        assert sum('Busy paused continuation.' in m['text'] and m.get('message_thread_id')==8 for m,_ in j.messages)==1
        visible=j.cli(entry,'conversation')['presentation']['items']
        assert sum(item.get('preview')=='Busy paused continuation.' for item in visible)==1
        # A rejected follow-up does not undo or replay its acknowledged clear.
        selected=prepare('clear','Beta','Clear Beta, then explain the outcome.',followup='explain the outcome.')
        j.deny_followup=True
        j.cli(entry,'goal_execute',{'id':selected})
        j.wait(lambda:j.row('SELECT phase FROM interaction_goal_controls WHERE id=?',(selected,))==[('partial',)])
        partial=j.cli(entry,'goal_status',{'id':selected})
        assert partial['result']['followup_outcome']=='known_unsent'
        assert 'native-topic-8' not in j.goals
        before=len(j.rpc);j.cli(entry,'goal_execute',{'id':selected})
        assert len(j.rpc)==before,'Partial metadata/follow-up control cannot replay'
        # A separate native-state fixture supplies a paused Alpha goal. An
        # internal failure after input effect must remain uncertain, never false.
        j.goals['native-topic-7']=copy.deepcopy(j.goals.get('native-topic-9',{'threadId':'native-topic-7','objective':'Other retained work.','status':'paused','tokenBudget':100,'tokensUsed':1,'timeUsedSeconds':1,'createdAt':1,'updatedAt':1}))
        j.goals['native-topic-7'].update(threadId='native-topic-7',status='paused')
        j.revisions['native-topic-7']+=1
        selected=prepare('clear','Alpha','Clear Alpha, then tell me the remaining context.',followup='tell me the remaining context.')
        j.lose_followup=True
        j.cli(entry,'goal_execute',{'id':selected})
        j.wait(lambda:j.row('SELECT phase FROM interaction_goal_controls WHERE id=?',(selected,))==[('partial',)])
        partial=j.cli(entry,'goal_status',{'id':selected})
        assert partial['result']['followup_outcome']=='uncertain' and partial['result']['followup_admitted'] is None
        before=len(j.rpc);j.cli(entry,'goal_execute',{'id':selected})
        assert len(j.rpc)==before,'Unknown follow-up cannot replay its committed control'
        thread='native-topic-9';j.broadcast(thread,'turn/completed',{'turn':{'id':j.runs[thread],'status':'completed'}});j.idle()
        print('PASS cold/busy goal activation and inactive-goal followups, durable metadata checkpoint, rejected/uncertain followup retention, immutable retries, canonical return and direct-surface selection; synthetic native peers')
    finally:j.close()

if __name__=='__main__':qualify(str(Path(sys.argv[1]).resolve()))
