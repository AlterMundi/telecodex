#!/usr/bin/env python3
"""Committed browser audio through actual HTTP/owner/SQLite/decoder/native; no models."""
import hashlib
import io
import json
import math
from pathlib import Path
import sqlite3
import struct
import sys
import uuid
import wave
from unittest.mock import patch
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from fastapi.testclient import TestClient
from dashboard.app import create_app
from dashboard.provider import Provider
from conversation_requests_http_io import Requests

def audio(frequency=440,silence=False):
    target=io.BytesIO()
    with wave.open(target,'wb') as wav:
        wav.setparams((1,2,16000,0,'NONE','not compressed'))
        wav.writeframes(b''.join(struct.pack('<h',0 if silence else int(5000*math.sin(2*math.pi*frequency*i/16000))) for i in range(3200)))
    return target.getvalue()

def qualify(binary):
    j=Requests(str(Path(binary).resolve()));client=None
    try:
        j.start()
        for topic in (7,8,1):
            j.admit('Seed owned native context',topic=topic);j.wait(lambda:j.has('Initial goal turn delivered.',topic));j.idle()
        j.stop()
        transcriber=j.root/'asr.py';transcriber.write_text('''import hashlib,json,pathlib,sys,time
root=pathlib.Path(sys.argv[1]);digest=hashlib.sha256(pathlib.Path(sys.argv[-1]).read_bytes()).hexdigest()
item=json.loads((root/'transcripts.json').read_text())[digest]
with (root/'asr-calls').open('a') as output:output.write(digest+'\\n')
while item.get('hold') and not (root/('release-'+digest)).exists():time.sleep(.02)
print(json.dumps({'text':item['text'],'final':True}))
''')
        j.config.write_text(j.config.read_text()+j.target_config+'\n[voice]\ntranscriber_label="synthetic/no-inference"\ntimeout_seconds=20\ntranscription_command='+json.dumps(['/usr/bin/python3',str(transcriber),str(j.root)])+'\n')
        j.start();j.wait_control_socket(j.socket);j.routing=True
        binding=j.read('presentation','list','0','0')['result']['binding'];j.messages.clear()
        config={'receiver_config':str(j.config),'state_dir':str(j.root/'dashboard'),'origin':'https://example.test','bot_id':123}
        client=TestClient(create_app(config,Provider(config),lambda raw,bot:int(raw)),base_url=config['origin'])
        headers={'origin':config['origin']}
        token=client.post('/api/session',json={'init_data':'100'},headers=headers).json()['session_token'];headers['authorization']='Bearer '+token
        page=client.get('/api/interaction',headers=headers).json();assert page['can_speak'];headers['x-dashboard-csrf']=page['csrf']
        transcripts={}
        def calls():
            p=j.root/'asr-calls';return p.read_text().splitlines() if p.exists() else []
        def register(data,text,hold=False):
            digest=hashlib.sha256(data).hexdigest();transcripts[digest]={'text':text,'hold':hold};(j.root/'transcripts.json').write_text(json.dumps(transcripts));return digest
        def upload(data,ident=None,selected=None,digest=None):
            ident=ident or str(uuid.uuid4());digest=digest or hashlib.sha256(data).hexdigest()
            h={**(headers if selected is None else selected),'content-type':'audio/wav','x-input-binding':binding,'x-input-id':ident,'x-audio-sha256':digest}
            return ident,client.post('/api/interaction/voice',content=data,headers=h)
        def status(ident):
            result=client.get('/api/interaction/inputs/'+ident,headers=headers);assert result.status_code==200,result.text;return result.json()
        data=audio();digest=register(data,'Earlier spoken requirement13.',True)
        assert upload(data,selected={k:v for k,v in headers.items() if k!='authorization'})[1].status_code==401
        assert upload(data,selected={k:v for k,v in headers.items() if k!='x-dashboard-csrf'})[1].status_code==403
        assert upload(data,digest='0'*64)[1].status_code==400
        assert upload(b'x'*(20*1024*1024+1))[1].status_code==413
        failed_sync=str(uuid.uuid4())
        with patch('dashboard.provider.sync_directory',side_effect=OSError('Synthetic directory-sync failure')):
            assert upload(data,failed_sync)[1].status_code==503
        assert not calls() and j.row('SELECT count(*) FROM conversation_inputs WHERE id=?',(failed_sync,))==[(0,)]
        original,response=upload(data);assert response.status_code==200,response.text
        assert response.json()['phase']=='preparing' and response.json()['text']==''
        j.wait(lambda:digest in calls());assert not j.packets,'Partial ASR admitted work'
        repeated=upload(data,original)[1];assert repeated.status_code==200 and not repeated.json()['new_input']
        assert upload(audio(555),original)[1].status_code==400
        assert calls().count(digest)==1
        later=str(uuid.uuid4());later_response=client.post('/api/interaction/inputs',json={'binding':binding,'id':later,'text':'Later correction: requirement17.'},headers=headers)
        assert later_response.status_code==200,later_response.text
        j.wait(lambda:status(later)['phase']=='accepted');assert len(j.packets)==1
        j.call('route-newer-text','dispatch',{'input_ref':later,'destinations':[{'selector':'Alpha','relation':'new','requests':[]}]})
        j.wait(lambda:j.instructions(7) and j.instructions(7)[0][1]=='accepted')
        (j.root/('release-'+digest)).touch();j.wait(lambda:status(original)['phase']=='accepted')
        retained=status(original);assert retained['text']=='Earlier spoken requirement13.'
        proof=retained['voice']['transcription'];assert proof['capture_id']==original and 'telegram_file_id' not in proof and proof['audio_sha256']==digest
        assert 'not verified written speech' in j.packets[-1][1]['input'][0]['text']
        request=json.loads(j.instructions(7)[0][2])['requests']
        j.call('route-older-voice','dispatch',{'input_ref':original,'destinations':[{'selector':'Alpha','relation':'amends','requests':request}]})
        j.wait(lambda:len(j.instructions(7))==2 and j.instructions(7)[1][1]=='accepted')
        worker_packet=j.worker_packets['native-topic-7'][-1]['input'][0]['text']
        assert '"capture_id":"'+original+'"' in worker_packet and 'Earlier spoken requirement13.' in worker_packet
        orders=j.row("SELECT event_id,id FROM original_ingress_order WHERE channel='conversation' ORDER BY id")
        assert orders[0][0]==original and orders[1][0]==later and orders[0][1]<orders[1][1]
        assert '"capture_order":'+str(orders[0][1]) in worker_packet
        assert calls().count(digest)==1 and len(j.packets)==2
        # Silence fails before invoking ASR and leaves ordinary text controls usable.
        silent=audio(silence=True);register(silent,'Never use hallucinated silence')
        silent_id,r=upload(silent);assert r.status_code==200
        j.wait(lambda:status(silent_id)['phase']=='preparation_unconfirmed');assert len(calls())==1 and len(j.packets)==2
        # Restoring a revoked grant cannot revive an in-flight voice capture.
        data2=audio(660);digest2=register(data2,'Must not run after authority retirement',True)
        retired,r=upload(data2);assert r.status_code==200;j.wait(lambda:digest2 in calls())
        with sqlite3.connect(j.root/'state.sqlite') as db:
            db.execute('UPDATE users SET allowed=0 WHERE tg_user_id=100');db.execute('UPDATE users SET allowed=1 WHERE tg_user_id=100')
        (j.root/('release-'+digest2)).touch();j.wait(lambda:status(retired)['phase']=='preparation_unconfirmed')
        assert status(retired)['retired'] and len(j.packets)==2
        assert upload(data2,retired)[1].json()['new_input'] is False and calls().count(digest2)==1
        # Restart closes only fixture processes, retaining uncertainty without ASR/native replay.
        data3=audio(770);digest3=register(data3,'Never replay after restart',True)
        interrupted,r=upload(data3);assert r.status_code==200;j.wait(lambda:digest3 in calls())
        j.stop();j.start();j.wait_control_socket(j.socket)
        assert status(interrupted)['phase']=='preparation_unconfirmed'
        assert upload(data3,interrupted)[1].json()['new_input'] is False and calls().count(digest3)==1
        assert not j.messages,'Canonical microphone fabricated Telegram publication'
        print('PASS committed browser voice: actual HTTP/owner/SQLite/decoder/native, one retained clip identity, final ASR only, source order before later text, attributed worker routing, silence, duplicate/changed upload, grant ABA and restart without replay; zero models.')
    finally:
        if client:client.close()
        j.close()

if __name__=='__main__':qualify(sys.argv[1])
