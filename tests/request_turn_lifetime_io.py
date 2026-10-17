#!/usr/bin/env python3
"""A retired native request must never interrupt a later goal turn; no models."""
import json
from pathlib import Path
import subprocess
import sys
import time
from conversation_input_control_io import Inputs

def qualify(binary):
    j=Inputs(str(Path(binary).resolve()))
    try:
        j.start();j.admit('Seed integrating context',topic=1)
        j.wait(lambda:j.has('Initial goal turn delivered.',1));j.idle()
        binding=j.read('presentation','list','0','0')['result']['binding']
        original,_=j.submit(binding,'Start synthetic native work that can continue to another goal turn.')
        j.wait(lambda:j.row('SELECT phase FROM conversation_inputs WHERE id=?',(original['id'],))==[('accepted',)])
        thread,old=j.web_thread,j.web_turn
        goal={'threadId':thread,'objective':'Synthetic goal continuation','status':'active','tokenBudget':None,
              'tokensUsed':0,'timeUsedSeconds':0,'createdAt':int(time.time()),'updatedAt':int(time.time())}
        j.broadcast(thread,'thread/goal/updated',{'goal':goal})
        send=next(send for selected,send in j.clients.values() if selected==thread)
        send({'id':'canonical-question','method':'item/tool/requestUserInput','params':{
            'threadId':thread,'turnId':old,'itemId':'old-turn-question','isBlocking':False,
            'questions':[{'id':'old','header':'Old turn','question':'This request belongs to the earlier physical turn.','options':None}]}})
        send({'id':'canonical-approval','method':'item/commandExecution/requestApproval','params':{
            'threadId':thread,'turnId':old,'itemId':'old-turn-approval','command':'echo old-turn-only','availableDecisions':['accept','decline']}})
        j.wait(lambda:j.row("SELECT count(*) FROM human_interactions WHERE state='pending'")==[(2,)])
        page=j.read('requests','list','0')['result'];item=next(v for v in page['items'] if v['kind']=='question')
        selected=j.read('requests','detail',item['id'],'0')['result']
        new='later-goal-turn'
        j.broadcast(thread,'turn/completed',{'turn':{'id':old,'status':'completed'}})
        j.broadcast(thread,'turn/started',{'turn':{'id':new,'status':'inProgress'}})
        j.broadcast(thread,'item/completed',{'turnId':new,'item':{'id':'new-turn-proof','type':'agentMessage','phase':'commentary','text':'The later physical turn is active.'}})
        j.wait(lambda:j.row('SELECT count(*) FROM interaction_native_messages WHERE native_turn=?',(new,))==[(1,)])
        request={'binding':binding,'id':selected['id'],'revision':selected['revision'],'action':{'kind':'cancel'}}
        result=subprocess.run([j.binary,'--control',str(j.config),'requests','decide',json.dumps(request)],capture_output=True,text=True,timeout=15)
        reply=json.loads(result.stdout)
        if reply['ok']:
            j.wait(lambda:any(m.get('method')=='turn/interrupt' for m in j.rpc))
        interrupted=[m for m in j.rpc if m.get('method')=='turn/interrupt' and m['params']['turnId']==new]
        assert not interrupted,'A cancellation of an old native question interrupted the later goal turn'
        assert not reply['ok'],'A retired request still accepted a decision'
        j.wait(lambda:j.row("SELECT count(*) FROM human_interactions WHERE state='expired'")==[(2,)])
        assert not j.native_answers,'A retired approval/question response reached the native peer'
        print('PASS physical-turn request retirement: late old-question cancellation does not interrupt goal continuation; question and approval channels expire; zero model calls.')
    finally:j.close()

if __name__=='__main__':qualify(sys.argv[1])
