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
    def owner(self,*args):
        result=subprocess.run([self.binary,'--control',str(self.config),*args],capture_output=True,text=True,timeout=20)
        assert result.returncode==0,result.stderr
        return json.loads(result.stdout)['result']

    def handle_extra_rpc(self,message,send):
        method=message.get('method');p=message.get('params',{})
        if self.routed and method in ('turn/start','turn/steer') and p.get('clientUserMessageId','').startswith('conversation:'):
            thread=p['threadId'];turn=self.runs.setdefault(thread,'web-'+str(len(self.runs)+1))
            self.packets.setdefault(thread,[]).append(p)
            send({'id':message['id'],'result':{'turn':{'id':turn}} if method=='turn/start' else {'turnId':turn}})
            if method=='turn/start':self.broadcast(thread,'turn/started',{'turn':{'id':turn,'status':'inProgress'}})
            return True
        if method=='thread/goal/conditionalActivate':
            thread=p['threadId'];revision=str(self.revisions.get(thread,0))
            assert p['additionalContext'] and p['origin']=='user'
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

    def finish_goal(self,thread,text):
        turn=self.runs[thread]
        self.broadcast(thread,'item/completed',{'turnId':turn,'item':{'id':turn+'-final','type':'agentMessage','phase':'final_answer','text':text}})
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
        j.cli(entry,'goal_execute',{'id':selected})
        j.wait(lambda:j.row('SELECT phase FROM interaction_goal_controls WHERE id=?',(selected,))==[('acknowledged',)])
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
        j.wait(lambda:j.row("SELECT count(*) FROM interaction_deliveries WHERE phase!='delivered'")==[(0,)])
        visible=j.cli(entry,'conversation')['presentation']['items']
        matching=[item for item in visible if item.get('preview')=='The corrected Beta goal result.']
        assert len(matching)==1 and matching[0]['id'].startswith('delivery:'),visible
        # A later direct input reselects Telegram without router admission.
        selected=prepare('resume','Alpha','Resume the current Alpha goal.')
        j.cli(entry,'goal_execute',{'id':selected})
        j.wait(lambda:j.row('SELECT phase FROM interaction_goal_controls WHERE id=?',(selected,))==[('acknowledged',)])
        j.admit('Continue directly here in Alpha.',topic=7)
        j.wait(lambda:j.row("SELECT source_input FROM conversation_returns WHERE native_thread='native-topic-7'")[0][0].startswith('telegram:'))
        j.finish_goal('native-topic-7','This later goal reply belongs to direct Alpha.')
        j.wait(lambda:j.row('SELECT busy FROM sessions WHERE thread_id=7')==[(0,)])
        visible=j.cli(entry,'conversation')['presentation']['items']
        assert not any(item.get('preview')=='This later goal reply belongs to direct Alpha.' for item in visible)
        assert j.row('SELECT count(*) FROM interaction_sources')==[(1,)],'Only the explicit correction was a router input'
        thread='native-topic-9';j.broadcast(thread,'turn/completed',{'turn':{'id':j.runs[thread],'status':'completed'}});j.idle()
        print('PASS cold/busy conditional goal activation, no priming turn, immutable retry, canonical goal result and later direct-surface selection; synthetic native peers')
    finally:j.close()

if __name__=='__main__':qualify(str(Path(sys.argv[1]).resolve()))
