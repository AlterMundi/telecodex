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
from dashboard.companion import Grants
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
        config={'receiver_config':str(j.config),'state_dir':str(j.root/'dashboard'),'origin':'https://example.test','bot_id':123,'companion':{'enabled':True,'grants_dir':str(j.root/'trace-grants')}}
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
        grant_file=j.root/'trace-device.json'
        Grants(config,Provider(config)).enroll('Trace scope regression',grant_file,60)
        credential=json.loads(grant_file.read_text())
        device_trace=client.get(trace_url,headers={'origin':config['origin'],'authorization':credential['authorization']})
        assert device_trace.status_code==200,device_trace.text
        assert device_trace.json()['read_only'] and device_trace.json()['id']==amendment['id']
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
        j.wait(lambda:j.row('SELECT count(*) FROM interaction_deliveries')==[(2,)])
        observed=client.get(trace_url,headers=headers)
        assert observed.status_code==200,observed.text
        result_trace=observed.json()['routes'][0]['results']
        assert result_trace and result_trace[0]['instruction_covered']
        assert result_trace[0]['completed'] and 'requirement17' in result_trace[0]['text_excerpt']
        sources=client.get('/api/interaction',headers=headers)
        assert sources.status_code==200,sources.text
        answers=[item for item in sources.json()['items'] if item['kind']=='worker_result']
        assert len(answers)==2,answers
        beta_answer=next(item for item in answers if item['target']['thread_id']==8)
        assert len(beta_answer['request_ids'])==2,beta_answer
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
        # Valid escaped originals must remain readable within the existing
        # owner-control frame, even when sent to multiple destinations.
        long_text='Bounded trace '+('\x01'*32000)
        long_input,_=submit(long_text)
        j.wait(lambda:read(long_input['id'])['phase']=='accepted')
        j.call('route-trace-long-dispatch','dispatch',{'input_ref':long_input['id'],'destinations':[{'selector':name,'relation':'new','requests':[]}for name in ('Alpha','Beta')]})
        def long_trace():
            response=client.get('/api/interaction/inputs/'+long_input['id']+'/trace',headers=headers)
            assert response.status_code==200,response.text
            return response.json()
        j.wait(lambda:len(long_trace()['routes'])==2)
        observation=long_trace()
        assert len(json.dumps(observation).encode())<262144
        before_read=j.row('SELECT count(*) FROM turns')
        def complete_text(source,part,next_offset):
            while next_offset is not None:
                response=client.get('/api/interaction/inputs/'+long_input['id']+'/trace/'+source,params={'offset':next_offset},headers=headers)
                assert response.status_code==200,response.text
                selected=response.json();assert selected['offset']==next_offset
                part+=selected['text'];next_offset=selected['next_offset']
            return part
        assert complete_text('original',observation['text'],observation['text_next_offset'])==long_text
        for step in observation['routes']:
            assert complete_text(step['instruction'],step['forwarded_original'],step['forwarded_next_offset'])==long_text
        assert j.row('SELECT count(*) FROM turns')==before_read
        assert j.row('SELECT count(*) FROM incoming_updates')==journal
        assert not j.messages,'Canonical direct routing fabricated Telegram publication'
        print('PASS actual HTTP/owner/SQLite/native direct selectors, busy-worker amendments, explicit override, immutable ambiguous choice, reversed results/reply, unavailable cancellation scope and ordinary integrator ingress; zero models.')
    finally:
        if client:client.close()
        j.close()

def telegram_monitor(binary):
    from voice_io import Voice
    j=Voice(str(Path(binary).resolve()));client=None
    try:
        socket=j.root/'control.sock'
        j.config.write_text(j.config.read_text().replace('[telegram]',f'[session_control]\nsocket_path="{socket}"\nhuman_id=100\n[telegram]'))
        j.prepare();j.wait_control_socket(socket)
        original,_,digest=j.voice('NATURAL-ROUTE Keep the original spoken requirement31 for both tasks.')
        j.wait(lambda:len(j.outputs)==5)
        j.wait(lambda:all(len(j.instructions(t))==1 and j.instructions(t)[0][1]=='accepted' for t in (7,8)))
        config={'receiver_config':str(j.config),'state_dir':str(j.root/'dashboard'),'origin':'https://example.test','bot_id':123,'companion':{'enabled':True,'grants_dir':str(j.root/'monitor-grants')}}
        client=TestClient(create_app(config,Provider(config),lambda raw,bot:int(raw)),base_url=config['origin'])
        headers={'origin':config['origin']}
        token=client.post('/api/session',json={'init_data':'100'},headers=headers).json()['session_token'];headers['authorization']='Bearer '+token
        base='/api/interaction/telegram-inputs'
        assert client.get(base).status_code==401
        assert client.get(base+'?before=-1',headers=headers).status_code==400
        page=client.get(base,headers=headers);assert page.status_code==200,page.text
        page=page.json();assert page['schema']=='telecodex.telegram-input-list/v1'
        item=next(i for i in page['items'] if i['id']=='telegram:'+str(original))
        assert item['voice'] and item['read_only'] and item['capture_order']>0 and not item['ordering_unknown']
        assert item['preview']=='NATURAL-ROUTE Keep the original spoken requirement31 for both tasks.'
        url='/api/interaction/inputs/'+item['id']+'/trace'
        before=j.row('SELECT count(*) FROM turns');calls=j.calls()
        trace=client.get(url,headers=headers);assert trace.status_code==200,trace.text
        trace=trace.json();assert trace['text']==item['preview'] and trace['voice']
        assert {r['label'] for r in trace['routes']}=={'Alpha','Beta'}
        for step in trace['routes']:
            assert step['native_acceptance'] and step['forwarded_original']==trace['text']
            part=client.get(url+'/'+step['instruction']+'?offset=0',headers=headers)
            assert part.status_code==200 and part.json()['text']==trace['text']
        assert client.get(url+'/original?offset=0',headers=headers).json()['text']==trace['text']
        # Explicit selector text is retained separately from the exact stripped
        # text forwarded to its worker. No model is needed to route this input.
        correction=j.admit('#Alpha Correct the spoken requirement to37.',topic=1)
        j.wait(lambda:len(j.instructions(7))==2 and j.instructions(7)[1][1]=='accepted')
        correction_url='/api/interaction/inputs/telegram:'+str(correction)+'/trace'
        observed=client.get(correction_url,headers=headers);assert observed.status_code==200,observed.text
        observed=observed.json();assert observed['text']=='#Alpha Correct the spoken requirement to37.'
        assert len(observed['routes'])==1 and observed['routes'][0]['label']=='Alpha'
        assert observed['routes'][0]['forwarded_original']=='Correct the spoken requirement to37.'
        # Non-entry history is not exposed even for the same allowed human.
        foreign=j.row("SELECT update_id FROM incoming_updates WHERE json_extract(payload_json,'$.message.message_thread_id')=7 ORDER BY update_id LIMIT 1")[0][0]
        assert client.get('/api/interaction/inputs/telegram:'+str(foreign)+'/trace',headers=headers).status_code==503
        for bad in ('telegram:01','telegram:-1','telegram:9223372036854775808'):
            assert client.get('/api/interaction/inputs/'+bad+'/trace',headers=headers).status_code==400
        # Pending originals are visible before they have a destination; paging
        # neither selects a route nor duplicates a retained source.
        pending=[j.admit('#Missing Retain unrouted input '+str(n),topic=1) for n in range(9)]
        j.wait(lambda:j.row('SELECT count(*) FROM incoming_updates WHERE update_id>=?',(pending[-1],))==[(1,)])
        page=client.get(base,headers=headers).json();assert len(page['items'])==8 and page['more']
        older=client.get(base+'?before='+str(page['cursor']),headers=headers).json()
        ids={i['id'] for i in page['items']+older['items']}
        assert len(ids)==11 and 'telegram:'+str(foreign) not in ids
        forward=client.get(base+'?before='+str(original)+'&direction=after',headers=headers).json()
        assert len(forward['items'])==8 and forward['more']
        empty=client.get('/api/interaction/inputs/telegram:'+str(pending[-1])+'/trace',headers=headers)
        assert empty.status_code==200 and empty.json()['routes']==[]
        # Old captures do not acquire an invented chronology; retired voice is
        # still inspectable history, never re-prepared or admitted by a read.
        with sqlite3.connect(j.root/'state.sqlite') as db:
            db.execute("DELETE FROM original_ingress_order WHERE channel='telegram' AND event_id=?",(str(original),))
            db.execute("UPDATE interaction_voice_inputs SET phase='retired' WHERE update_id=?",(original,))
        old=client.get(base+'?before='+str(correction),headers=headers).json()['items'][0]
        assert old['id']==item['id'] and old['ordering_unknown'] and old['capture_order'] is None
        assert old['retired'] and client.get(url,headers=headers).json()['text']==trace['text']
        grant_file=j.root/'monitor-device.json';Grants(config,Provider(config)).enroll('Monitor read scope',grant_file,60)
        credential=json.loads(grant_file.read_text())
        device={'origin':config['origin'],'authorization':credential['authorization']}
        assert client.get(base,headers=device).status_code==403
        assert client.get(url,headers=device).status_code==403
        assert j.calls()==calls==[digest] and j.row('SELECT count(*) FROM turns')==before
        assert len(j.instructions(7))==2 and len(j.instructions(8))==1
        print('PASS Telegram monitor: actual HTTP/owner/SQLite/native text and committed voice, two routes and exact selector stripping, pending/retired/legacy originals, bounded cursor paging, owner/entry/device isolation; reads cause no ASR, model or native dispatch.')
    finally:
        if client:client.close()
        j.close()

if __name__=='__main__':
    qualify(sys.argv[1])
    telegram_monitor(sys.argv[1])
