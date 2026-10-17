#!/usr/bin/env python3
"""Authenticated HTTP -> real owner socket/SQLite/native; synthetic peers only."""
from pathlib import Path
import sqlite3
import sys
import uuid
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from fastapi.testclient import TestClient
from dashboard.app import create_app
from dashboard.provider import Provider
from conversation_input_control_io import Inputs


def qualify(binary):
    journey = Inputs(str(Path(binary).resolve()))
    client = None
    try:
        journey.start()
        journey.admit('Seed integrating conversation', topic=1)
        journey.wait(lambda:journey.has('Initial goal turn delivered.',1))
        journey.idle(); journey.wait_control_socket(journey.socket)
        journey.messages.clear()  # The seeded Telegram reply precedes canonical ingress.
        config = {'receiver_config':str(journey.config), 'state_dir':str(journey.root/'dashboard'),
                  'origin':'https://example.test', 'bot_id':123}
        provider = Provider(config)
        def open_client():
            return TestClient(create_app(config, provider, lambda raw,bot:int(raw)), base_url=config['origin'])
        client = open_client()
        headers = {'origin':config['origin']}
        assert client.get('/api/interaction/inputs').status_code == 401
        def login():
            token = client.post('/api/session', json={'init_data':'100'}, headers={'origin':config['origin']}).json()['session_token']
            headers['authorization']='Bearer '+token
            page = client.get('/api/interaction/inputs', headers=headers).json()
            headers['x-dashboard-csrf']=page['csrf']
            return page
        page = login(); assert page['items'] == []
        initial_journal=journey.row('SELECT count(*) FROM incoming_updates')
        # Worst-case escaping stays within both bounded HTTP and owner frames.
        text = ' '+('\x01'*32765)+'  '
        body = {'binding':page['binding'],'id':str(uuid.uuid4()),'text':text}
        def post(body=body, selected=None):
            return client.post('/api/interaction/inputs',json=body,headers=headers if selected is None else selected)
        assert post(selected={k:v for k,v in headers.items() if k!='x-dashboard-csrf'}).status_code == 403
        assert post(selected={**headers,'origin':'https://other.test'}).status_code == 403
        assert not journey.packets
        response=post(); assert response.status_code==200,response.text
        assert response.json()['text']==text
        journey.wait(lambda:len(journey.packets)==1)
        assert text in journey.packets[0][1]['input'][0]['text']
        journey.wait(lambda:journey.row('SELECT phase FROM conversation_inputs WHERE id=?',(body['id'],))==[('accepted',)])
        assert post().json()['new_input'] is False
        assert post({**body,'text':'Forged replacement'}).status_code==503
        assert post({**body,'id':str(uuid.uuid4()),'text':'x'*32769}).status_code==400
        assert post({**body,'id':str(uuid.uuid4()),'text':'x','human':200}).status_code==400
        status=client.get('/api/interaction/inputs/'+body['id'],headers=headers)
        assert status.status_code==200 and status.json()['text']==text
        assert len(journey.packets)==1
        # Bounded newest-first history survives restart; page overlap does not replay.
        for n in range(9):
            reply=post({**body,'id':str(uuid.uuid4()),'text':f'Correction {n}'})
            assert reply.status_code==200,reply.text
        journey.wait(lambda:len(journey.packets)==10)
        page=client.get('/api/interaction/inputs',headers=headers).json()
        assert len(page['items'])==8 and page['more']
        prior=client.get('/api/interaction/inputs?before='+str(page['cursor']),headers=headers).json()
        assert len(prior['items'])==2 and not prior['more']
        assert not set(i['id'] for i in page['items']) & set(i['id'] for i in prior['items'])
        assert prior['items'][-1]['preview']==text[:1000]
        forward=client.get('/api/interaction/inputs?before='+str(prior['items'][-1]['sequence'])+'&direction=after',headers=headers).json()
        assert len(forward['items'])==8 and forward['more']
        assert forward['items'][0]['id']==prior['items'][0]['id']
        assert forward['newest']<page['newest']
        # An uncertain native receipt is inspected without another physical submission.
        journey.drop_ack=True
        uncertain={**body,'id':str(uuid.uuid4()),'text':'Lost native acknowledgement'}
        assert post(uncertain).status_code==200
        journey.wait(lambda:journey.row('SELECT phase FROM conversation_inputs WHERE id=?',(uncertain['id'],))==[('uncertain',)],seconds=18)
        count=len(journey.packets)
        assert client.get('/api/interaction/inputs/'+uncertain['id'],headers=headers).json()['phase']=='uncertain'
        client.close();client=open_client(); login()
        assert client.get('/api/interaction/inputs/'+uncertain['id'],headers=headers).json()['phase']=='uncertain'
        assert len(journey.packets)==count
        # Chunked oversize input is rejected before reaching control or persistence.
        response=client.post('/api/interaction/inputs',content=iter([b'x'*150000,b'x'*150000]),headers={**headers,'content-type':'application/json'})
        assert response.status_code==413
        assert client.post('/api/session',content=b'x'*17000,headers=headers).status_code==413
        with sqlite3.connect(journey.root/'state.sqlite') as db:
            db.execute('UPDATE users SET allowed=0 WHERE tg_user_id=100')
        assert client.get('/api/interaction/inputs',headers=headers).status_code==401
        assert post().status_code==401
        assert len(journey.packets)==count
        assert journey.row('SELECT count(*) FROM incoming_updates')==initial_journal
        assert not journey.messages,'Canonical input published to Telegram'
        print('PASS authenticated HTTP/owner/native: exact32KiB escaped text, CSRF/Origin, dedup, history/reopen, uncertain status without replay, bounded chunked ingress and revoked owner; zero models.')
    finally:
        if client:client.close()
        journey.close()

if __name__=='__main__':qualify(sys.argv[1])
