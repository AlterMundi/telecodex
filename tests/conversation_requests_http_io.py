#!/usr/bin/env python3
"""Canonical requests through authenticated HTTP/owner/native; zero models."""
import json
from pathlib import Path
import sqlite3
import sys
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from fastapi.testclient import TestClient
from dashboard.app import create_app
from dashboard.provider import Provider
from conversation_routing_io import CanonicalRouting

class Requests(CanonicalRouting):
    def handle_extra_rpc(self,message,send):
        method=message.get('method');params=message.get('params',{})
        if self.routing and getattr(self,'hold_direct',False) and method in ('turn/start','turn/steer') and params.get('threadId')=='native-topic-8' and params.get('clientUserMessageId','').startswith('telegram:'):
            if method=='turn/start':
                self.workers['native-topic-8']='internal-direct-turn'
                send({'id':message['id'],'result':{'turn':{'id':'internal-direct-turn'}}})
                self.broadcast('native-topic-8','turn/started',{'turn':{'id':'internal-direct-turn','status':'inProgress'}})
            else:send({'id':message['id'],'result':{'turnId':'internal-direct-turn'}})
            return True
        if message.get('method') is None and str(message.get('id')).startswith('canonical-'):
            self.native_answers.append(message);return True
        return super().handle_extra_rpc(message,send)

def qualify(binary):
    j=Requests(str(Path(binary).resolve()));client=None
    try:
        j.start()
        for topic in (7,8,1):
            j.admit('Seed owned native context',topic=topic)
            j.wait(lambda:j.has('Initial goal turn delivered.',topic));j.idle()
        j.stop();j.config.write_text(j.config.read_text()+j.target_config)
        j.start();j.wait_control_socket(j.socket);j.routing=True
        binding=j.read('presentation','list','0','0')['result']['binding']
        journal=j.row('SELECT count(*) FROM incoming_updates');j.messages.clear();j.telegram_methods.clear()
        original,_=j.submit(binding,'Work with Alpha while this integrating conversation remains available.')
        j.wait(lambda:j.row('SELECT phase FROM conversation_inputs WHERE id=?',(original['id'],))==[('accepted',)])
        j.call('route-owner-requests','dispatch',{'input_ref':original['id'],'destinations':[{'selector':'Alpha','relation':'new','requests':[]}]})
        j.wait(lambda:j.instructions(7) and j.instructions(7)[0][1]=='accepted')
        root_send=next(send for thread,send in j.clients.values() if thread==j.web_thread)
        worker_send=next(send for thread,send in j.clients.values() if thread=='native-topic-7')
        long_question='Choose the exact scope. '+('Full source context remains visible. '*240)
        root_send({'id':'canonical-question','method':'item/tool/requestUserInput','params':{
            'threadId':j.web_thread,'turnId':j.web_turn,'itemId':'owner-choice','isBlocking':True,
            'questions':[{'id':'scope','header':'Scope','question':long_question,'options':[{'label':'Selected scope','description':'Only this request'}]},
                         {'id':'details','header':'Details','question':'Provide the exact details.','isOther':True,'options':None}]}})
        worker_send({'id':'canonical-approval','method':'item/commandExecution/requestApproval','params':{
            'threadId':'native-topic-7','turnId':j.workers['native-topic-7'],'itemId':'owner-command',
            'command':'echo synthetic-only','reason':'Explicit human selection required','availableDecisions':['accept','decline']}})
        j.wait(lambda:j.row("SELECT count(*) FROM human_interactions WHERE state='pending'")==[(2,)])
        config={'receiver_config':str(j.config),'state_dir':str(j.root/'dashboard'),'origin':'https://example.test','bot_id':123,'companion':{'enabled':True,'grants_dir':str(j.root/'internal-device-grants')}}
        provider=Provider(config);client=TestClient(create_app(config,provider,lambda raw,bot:int(raw)),base_url=config['origin'])
        headers={'origin':config['origin']}
        assert client.get('/api/interaction/requests').status_code==401
        token=client.post('/api/session',json={'init_data':'100'},headers=headers).json()['session_token'];headers['authorization']='Bearer '+token
        page=client.get('/api/interaction/requests',headers=headers);assert page.status_code==200,page.text
        page=page.json();headers['x-dashboard-csrf']=page['csrf']
        assert len(page['items'])==2 and not page['more']
        assert {item['label'] for item in page['items']}=={'Conversation','#Alpha'}
        assert all('body' not in item and 'choices' not in item for item in page['items'])
        question=next(item for item in page['items'] if item['kind']=='question')
        approval=next(item for item in page['items'] if item['kind']=='approval')
        def detail(ident,**params):
            r=client.get('/api/interaction/requests/'+ident,params=params,headers=headers)
            assert r.status_code==200,r.text;return r.json()
        question=detail(question['id']);approval=detail(approval['id'])
        assert question['actionable'] and approval['actionable'] and not j.native_answers
        text=question['body'];part=question
        while part['next_offset'] is not None:
            part=detail(question['id'],offset=part['next_offset'],revision=question['revision']);text+=part['body']
        assert long_question in text and 'Selected scope — Only this request' in text
        assert client.get('/api/interaction/requests/'+question['id'],params={'offset':4000,'revision':'0'*64},headers=headers).status_code==409
        assert {choice['action']['choice'] for choice in approval['choices']}=={'a','d'}
        def decide(view,action,selected=None):
            return client.post('/api/interaction/requests/'+view['id'],json={'binding':binding,'revision':view['revision'],'action':action},headers=headers if selected is None else selected)
        assert decide(approval,{'kind':'approval','choice':'a'},selected={k:v for k,v in headers.items() if k!='x-dashboard-csrf'}).status_code==403
        assert decide(approval,{'kind':'approval','choice':'s'}).status_code==409
        assert not j.native_answers,'An unoffered approval choice reached the native channel'
        # Both request responders remain live while the root and worker accept steering.
        correction,_=j.submit(binding,'Incorporate requirement17 while the questions are pending.')
        j.wait(lambda:j.row('SELECT phase FROM conversation_inputs WHERE id=?',(correction['id'],))==[('accepted',)])
        request=json.loads(j.instructions(7)[0][2])['requests']
        j.call('route-owner-pending-steer','dispatch',{'input_ref':correction['id'],'destinations':[{'selector':'Alpha','relation':'amends','requests':request}]})
        j.wait(lambda:len(j.instructions(7))==2 and j.instructions(7)[1][1]=='accepted')
        assert len(j.worker_packets['native-topic-7'])==2 and not j.native_answers
        first={'kind':'option','index':0}
        selected=decide(question,first);assert selected.status_code==200,selected.text
        assert selected.json()['new_decision']
        repeated=decide(question,first);assert repeated.status_code==200 and not repeated.json()['new_decision']
        assert decide(question,{'kind':'answer','text':'Replace the committed first answer'}).status_code==409
        next_question=detail(question['id']);assert next_question['question_index']==1 and question['revision'] in next_question['recorded_revisions']
        answer=' '+('\x01'*32765)+'  '
        final=decide(next_question,{'kind':'answer','text':answer});assert final.status_code==200,final.text
        j.wait(lambda:any(row['id']=='canonical-question' for row in j.native_answers))
        native=next(row for row in j.native_answers if row['id']=='canonical-question')
        assert native['result']['answers']=={'scope':{'answers':['Selected scope']},'details':{'answers':[answer]}}
        assert decide(next_question,{'kind':'answer','text':answer}).status_code==200
        accepted=decide(approval,{'kind':'approval','choice':'a'});assert accepted.status_code==200,accepted.text
        j.wait(lambda:any(row['id']=='canonical-approval' for row in j.native_answers))
        assert next(row for row in j.native_answers if row['id']=='canonical-approval')['result']['decision']=='accept'
        assert decide(approval,{'kind':'approval','choice':'a'}).json()['new_decision'] is False
        assert len(j.native_answers)==2,'A recorded revision replayed a native response'
        assert j.row('SELECT count(*) FROM incoming_updates')==journal
        assert not j.messages,'Canonical controls fabricated a Telegram publication'
        # Ordinary direct work stays out of For you, but its current pending is
        # available through the separate owned-session surface and same responder.
        j.hold_direct=True;j.admit('Direct Beta task, never routed by the integrator.',topic=8)
        j.wait(lambda:'native-topic-8' in j.workers)
        beta_send=next(send for thread,send in j.clients.values() if thread=='native-topic-8')
        beta_send({'id':'canonical-internal-direct-question','method':'item/tool/requestUserInput','params':{
            'threadId':'native-topic-8','turnId':j.workers['native-topic-8'],'itemId':'direct-choice','isBlocking':True,
            'questions':[{'id':'direct','header':'Direct Beta','question':'Which direct requirement should continue?','options':None}]}})
        j.wait(lambda:bool(j.row("SELECT id FROM human_interactions WHERE state='pending' AND native_thread='native-topic-8'")))
        # Ledger persistence precedes show_question and live responder publication.
        # Observing the row is not evidence that its native answer channel exists.
        def direct_question_ready():
            response=client.get('/api/interaction/internal',headers=headers)
            assert response.status_code==200,response.text
            return any(v['target']['thread_id']==8 and v.get('actionable') is True for v in response.json()['items'])
        j.wait(direct_question_ready)
        internal=client.get('/api/interaction/internal',headers=headers);assert internal.status_code==200,internal.text
        pending=next(v for v in internal.json()['items'] if v['target']['thread_id']==8)
        assert pending['kind']=='question' and pending['actionable'] and pending['binding']==binding,pending
        assert pending['id'] not in {v['id'] for v in client.get('/api/interaction/requests',headers=headers).json()['items']}
        assert client.get('/api/interaction/requests/'+pending['id'],headers=headers).status_code==409
        selected=client.get('/api/interaction/internal/requests/'+pending['id'],headers=headers)
        assert selected.status_code==200,selected.text
        view=selected.json();assert view['actionable'] and view['revision']==pending['revision']
        assert 'direct requirement' in view['body']
        from dashboard.companion import Grants
        grants=Grants(config,provider);grant=grants.enroll('Selected internal pending fixture',j.root/'internal-device.json',3600)
        device=json.loads((j.root/'internal-device.json').read_text())
        limited={'origin':config['origin'],'authorization':device['authorization'],'x-dashboard-csrf':device['csrf']}
        route='/api/interaction/internal/requests/'+view['id']
        def decision(action):return {'binding':binding,'revision':view['revision'],'action':action}
        before_answers=len(j.native_answers)
        for action in ({'kind':'approval','choice':'a'},{'kind':'cancel'}):
            assert client.post(route,headers=limited,json=decision(action)).status_code==403
        assert len(j.native_answers)==before_answers,'Conversation-only access acquired approval/cancellation authority'
        reply=decision({'kind':'answer','text':'Continue the direct requirement once.'})
        assert client.post(route,headers={k:v for k,v in limited.items() if k!='x-dashboard-csrf'},json=reply).status_code==403
        result=client.post(route,headers=limited,json=reply);assert result.status_code==200,result.text
        j.wait(lambda:any(v['id']=='canonical-internal-direct-question' for v in j.native_answers))
        count=len(j.native_answers)
        repeated=client.post(route,headers=limited,json=reply);assert repeated.status_code==200,repeated.text
        assert not repeated.json()['new_decision'] and len(j.native_answers)==count
        assert client.get(route+'/receipt',headers=limited,params={'revision':view['revision']}).json()['recorded']
        grants.revoke(grant['grant_id']);assert client.get('/api/interaction/internal',headers=limited).status_code==401
        assert client.get('/api/interaction/internal',headers=headers).status_code==200
        j.broadcast('native-topic-8','turn/completed',{'turn':{'id':j.workers['native-topic-8'],'status':'completed'}})
        # A new pending request expires on listener restart and never recreates its response channel.
        root_send({'id':'canonical-restart-question','method':'item/tool/requestUserInput','params':{
            'threadId':j.web_thread,'turnId':j.web_turn,'itemId':'restart-choice','isBlocking':False,
            'questions':[{'id':'restart','header':'Restart','question':'Should remain expired after restart.','options':None}]}})
        j.wait(lambda:j.row("SELECT count(*) FROM human_interactions WHERE state='pending'")==[(1,)])
        pending=next(item for item in client.get('/api/interaction/requests',headers=headers).json()['items'] if item['state']=='pending')
        pending=detail(pending['id']);j.stop();j.start();j.wait_control_socket(j.socket)
        expired=detail(pending['id']);assert expired['state']=='expired' and not expired['actionable']
        count=len(j.native_answers)
        assert decide(pending,{'kind':'answer','text':'Do not replay an expired responder'}).status_code==409
        assert len(j.native_answers)==count
        # Decision evidence retains the exact question/answer after live rows disappear.
        recorded=json.loads(j.row('SELECT doc FROM conversation_decisions WHERE request_id=? AND revision=?',(question['id'],question['revision']))[0][0])
        assert recorded['proposal']['question_id']=='scope' and recorded['proposal']['question_index']==0
        assert recorded['resolved']['answer']=='Selected scope'
        with sqlite3.connect(j.root/'state.sqlite') as db:
            db.execute('PRAGMA foreign_keys=ON')
            db.execute('DELETE FROM sessions WHERE thread_id=7')
        archived=client.get('/api/interaction/requests/'+approval['id']+'/receipt',params={'revision':approval['revision']},headers=headers)
        assert archived.status_code==200 and archived.json()['recorded']
        assert decide(approval,{'kind':'approval','choice':'a'}).json()['new_decision'] is False
        assert len(j.native_answers)==count and j.row('SELECT count(*) FROM sessions WHERE thread_id=7')==[(0,)]
        assert j.row('SELECT count(*) FROM incoming_updates')==[(journal[0][0]+1,)],'Only the explicit direct Beta original may add Telegram ingress'
        assert j.messages and all(m.get('message_thread_id')==8 for m,_ in j.messages),'Canonical controls published outside the ordinary direct Beta topic'
        print('PASS authenticated root/routed native controls: full proposal paging, exact multi-question answers, offered approvals only, duplicate revision inspection, simultaneous steer, escaped32KiB answer, restart expiration, separate direct-session device decisions and no fabricated Telegram ingress/publication; zero models.')
    finally:
        if client:client.close()
        j.close()

if __name__=='__main__':qualify(sys.argv[1])
