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
        config={'receiver_config':str(j.config),'state_dir':str(j.root/'dashboard'),'origin':'https://example.test','bot_id':123}
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
        assert j.row('SELECT count(*) FROM incoming_updates')==journal
        assert not j.messages,'Canonical controls fabricated a Telegram publication'
        print('PASS authenticated root/routed native controls: full proposal paging, exact multi-question answers, offered approvals only, duplicate revision inspection, simultaneous steer, escaped32KiB answer, restart expiration and no Telegram identity/publication; zero models.')
    finally:
        if client:client.close()
        j.close()

if __name__=='__main__':qualify(sys.argv[1])
