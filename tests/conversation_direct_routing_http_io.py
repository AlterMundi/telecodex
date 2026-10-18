#!/usr/bin/env python3
"""Explicit canonical routes through HTTP, SQLite and native peers; zero models."""
import json
from pathlib import Path
import sqlite3
import sys
import uuid
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from fastapi.testclient import TestClient
from dashboard.app import create_app
from dashboard.provider import Provider
from conversation_routing_io import CanonicalRouting


def qualify(binary):
    j=CanonicalRouting(str(Path(binary).resolve()));client=None
    try:
        j.start()
        for topic in (7,8,1):
            j.admit('Seed owned native context',topic=topic)
            j.wait(lambda:j.has('Initial goal turn delivered.',topic));j.idle()
        j.stop();j.config.write_text(j.config.read_text()+j.target_config)
        j.start();j.wait_control_socket(j.socket);j.routing=True
        journal=j.row('SELECT count(*) FROM incoming_updates');j.messages.clear();j.telegram_methods.clear()
        config={'receiver_config':str(j.config),'state_dir':str(j.root/'dashboard'),'origin':'https://example.test','bot_id':123}
        client=TestClient(create_app(config,Provider(config),lambda raw,bot:int(raw)),base_url=config['origin'])
        headers={'origin':config['origin']}
        token=client.post('/api/session',json={'init_data':'100'},headers=headers).json()['session_token'];headers['authorization']='Bearer '+token
        page=client.get('/api/interaction/inputs',headers=headers);assert page.status_code==200,page.text
        page=page.json();binding=page['binding'];headers['x-dashboard-csrf']=page['csrf']
        assert page['routing_choices']==[{'selector':'t7','label':'Alpha'},{'selector':'t8','label':'Beta'}]
        def submit(text,reply=None):
            body={'id':str(uuid.uuid4()),'binding':binding,'text':text}
            if reply:body['reply']=reply
            result=client.post('/api/interaction/inputs',json=body,headers=headers)
            assert result.status_code==200,result.text
            return body,result.json()
        def read(ident):
            response=client.get('/api/interaction/inputs/'+ident,headers=headers)
            assert response.status_code==200,response.text
            return response.json()
        def route(ident,selector,code=200):
            response=client.post('/api/interaction/inputs/'+ident+'/route',json={'binding':binding,'selector':selector},headers=headers)
            assert response.status_code==code,response.text
            return response.json()
        original,result=submit('#Alpha Keep requirement13 exact.')
        j.wait(lambda:j.instructions(7) and j.instructions(7)[0][1]=='accepted')
        result=read(original['id']);assert result['routing']['phase']=='accepted' and result['phase']=='routed'
        assert not j.packets,'An explicit human route unnecessarily called the front conversation'
        assert result['continuation']['source']=='input:'+original['id']
        amendment,_=submit('Replace requirement13 with requirement17.',result['continuation'])
        j.wait(lambda:len(j.instructions(7))==2 and j.instructions(7)[1][1]=='accepted')
        first,second=[json.loads(row[2]) for row in j.instructions(7)]
        assert second['relation']=={'kind':'amends','requests':first['requests']},second
        beta,_=submit('#Beta Independent requirement23.',result['continuation'])
        j.wait(lambda:j.instructions(8) and j.instructions(8)[0][1]=='accepted')
        assert json.loads(j.instructions(8)[0][2])['relation']=={'kind':'new'}
        assert len(j.worker_packets['native-topic-7'])==2
        trace_url='/api/interaction/inputs/'+amendment['id']+'/trace'
        before_trace=j.row('SELECT count(*) FROM turns')
        trace=client.get(trace_url,headers=headers)
        assert trace.status_code==200,trace.text
        trace=trace.json();assert trace['text']==amendment['text'] and trace['read_only']
        assert len(trace['routes'])==1
        step=trace['routes'][0]
        assert step['label']=='Alpha' and step['instruction']==second['id']
        assert step['forwarded_original']==second['text']==amendment['text']
        assert step['relation']==second['relation'] and step['native_acceptance']
        assert not step['results'] and j.row('SELECT count(*) FROM turns')==before_trace
        assert client.get(trace_url).status_code==401
        assert client.get('/api/interaction/inputs/not-a-uuid/trace',headers=headers).status_code==400
        packet=j.worker_packets['native-topic-7'][-1]['input'][0]['text']
        assert 'Replace requirement13 with requirement17.' in packet
        provenance=json.loads(next(line.removeprefix('Transport provenance: ') for line in packet.splitlines() if line.startswith('Transport provenance: ')))
        assert provenance['selection']['kind']=='reply'
        assert provenance['selection']['reply']==result['continuation']
        unknown,held=submit('#Unknown Keep original text through clarification.')
        assert held['phase']=='awaiting_target' and held['routing'] is None
        observed=client.get('/api/interaction/inputs/'+unknown['id']+'/trace',headers=headers)
        assert observed.status_code==200 and observed.json()['routes']==[]
        selected=route(unknown['id'],'t8');assert selected['routing']['target']['thread_id']==8
        j.wait(lambda:len(j.instructions(8))==2 and j.instructions(8)[1][1]=='accepted')
        assert route(unknown['id'],'t8')['routing']['target']['thread_id']==8
        route(unknown['id'],'t7',409)
        assert len(j.instructions(8))==2 and not j.packets
        # Results return independently of worker start order and can be selected.
        j.answer_worker(8,'Beta result arrives before Alpha.')
        j.answer_worker(7,'Alpha result incorporates requirement17.')
        j.wait(lambda:j.row('SELECT count(*) FROM interaction_deliveries')==[(3,)])
        observed=client.get(trace_url,headers=headers)
        assert observed.status_code==200,observed.text
        result_trace=observed.json()['routes'][0]['results']
        assert result_trace and result_trace[0]['instruction_covered']
        assert result_trace[0]['completed'] and 'requirement17' in result_trace[0]['text_excerpt']
        sources=client.get('/api/interaction',headers=headers)
        assert sources.status_code==200,sources.text
        source=next(item for item in sources.json()['items'] if item['kind']=='worker_result' and item['target']['thread_id']==7)
        _,followup=submit('Continue this completed result.',{'source':source['id'],'revision':source['revision']})
        j.wait(lambda:len(j.instructions(7))==3 and j.instructions(7)[2][1]=='accepted')
        assert json.loads(j.instructions(7)[2][2])['relation']=={'kind':'amends','requests':first['requests']}
        refreshed=client.get(trace_url,headers=headers)
        assert refreshed.status_code==200,refreshed.text
        assert refreshed.json()['routes'][0]['results'][0]['current'] is False
        assert refreshed.json()['routes'][0]['results'][0]['correction_pending'] is True
        assert not j.packets
        # Unknown cancellation scope is distinct from a confirmed absent turn.
        interrupt,held=submit('#Unknown /interrupt');assert held['phase']=='awaiting_target'
        interrupted=route(interrupt['id'],'t8')
        assert interrupted['routing']['phase']=='scope_unavailable',interrupted
        # A quoted hashtag retains normal integrator ingress.
        front,_=submit('The quoted #Alpha label is ordinary discussion.')
        j.wait(lambda:j.row('SELECT phase FROM conversation_inputs WHERE id=?',(front['id'],))==[('accepted',)])
        assert len(j.packets)==1
        assert j.row('SELECT count(*) FROM incoming_updates')==journal
        assert not j.messages,'Canonical direct routing fabricated Telegram publication'
        print('PASS actual HTTP/owner/SQLite/native direct selectors, busy-worker amendments, explicit override, immutable ambiguous choice, reversed results/reply, unavailable cancellation scope and ordinary integrator ingress; zero models.')
    finally:
        if client:client.close()
        j.close()

if __name__=='__main__':qualify(sys.argv[1])
