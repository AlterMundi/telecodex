#!/usr/bin/env python3
"""Actual HTTP/controller/native-image/SQLite journey; synthetic native/ASR, zero models."""
import copy
from datetime import datetime,timezone,timedelta
import hashlib
import json
from pathlib import Path
import sqlite3
import struct
import sys
import uuid
import zlib
from conversation_cold_entry_http_io import ColdEntry,Browser
from conversation_voice_http_io import audio


def png(width=160,height=96,flip=False):
    def chunk(kind,data):return struct.pack('>I',len(data))+kind+data+struct.pack('>I',zlib.crc32(kind+data))
    pixels=b''.join(b'\0'+b''.join(bytes((30,80,230) if (x<width//2)^flip else (230,45,50)) for x in range(width)) for _ in range(height))
    return b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',width,height,8,2,0,0,0))+chunk(b'IDAT',zlib.compress(pixels))+chunk(b'IEND',b'')


def reference(data,x=20,y=30):
    now=datetime.now(timezone.utc).isoformat();sha=hashlib.sha256(data).hexdigest()
    return {'source_kind':'selected_image','source_id':str(uuid.uuid4()),'image_sha256':sha,'width':160,'height':96,
        'captured_at':now,'selected_at':now,'selected_revision':sha,'state':'selected','point':{'x':x,'y':y}}


def qualify(binary):
    j=ColdEntry(str(Path(binary).resolve()));b=None
    try:
        j.prepare();b=Browser(j,companion=True)
        data=png();ref=reference(data);sha=ref['image_sha256']
        def upload(data,headers=None):
            return b.client.post('/api/interaction/visual',content=data,headers={**(b.headers if headers is None else headers),'content-type':'image/png','x-image-sha256':hashlib.sha256(data).hexdigest()})
        def submit(text,selected,ident=None):
            ident=ident or str(uuid.uuid4());body={'id':ident,'binding':b.binding,'text':text,'visual':selected}
            return ident,b.client.post('/api/interaction/inputs',json=body,headers=b.headers)
        assert upload(data,{k:v for k,v in b.headers.items()if k!='authorization'}).status_code==401
        assert upload(data,{k:v for k,v in b.headers.items()if k!='x-dashboard-csrf'}).status_code==403
        assert upload(data[:-4]).status_code==400
        assert upload(b'x'*(2*1024*1024+1)).status_code==413
        r=upload(data,b.device_headers);assert r.status_code==200,r.text
        assert b.client.get('/api/state',headers=b.device_headers).status_code==403
        assert r.json()['work_admitted']is False and not j.packets
        assert j.row('SELECT count(*) FROM conversation_inputs')==[(0,)]
        for bad in ({**ref,'point':{'x':True,'y':0}},{**ref,'point':{'x':160,'y':0}},
                    {**ref,'selected_revision':'b'*64},{**ref,'state':'disconnected'},
                    {**ref,'state':'ambiguous'},{**ref,'width':159},{**ref,'state':'view_changed'},
                    {**ref,'image_sha256':'0'*64,'selected_revision':'0'*64}):
            ident,r=submit('That one.',bad);assert r.status_code==400,r.text
            assert r.json().get('code')=='visual_reselection'and r.json().get('submission_attempted')is False
            assert j.row('SELECT count(*) FROM conversation_inputs WHERE id=?',(ident,))==[(0,)]
        stale=copy.deepcopy(ref);stale['selected_at']=stale['captured_at']=(datetime.now(timezone.utc)-timedelta(minutes=10)).isoformat()
        assert submit('That one.',stale)[1].status_code==400
        stale_frame={**ref,'source_kind':'camera_frame','captured_at':(datetime.now(timezone.utc)-timedelta(seconds=10)).isoformat()}
        assert submit('That one.',stale_frame)[1].status_code==400
        j.hold_new_native=True
        original,r=submit('Describe this selected object in Alpha.',ref);assert r.status_code==200,r.text
        assert j.new_native_seen.wait(timeout=5),'Synthetic initial binding was not observed'
        during=png(flip=True);assert upload(during).status_code==200
        whole={**reference(during),'point':None}
        prepared,r=submit('Discuss this independent newer view.',whole);assert r.status_code==200,r.text
        j.hold_new_native=False;j.new_native_release.set()
        j.wait(lambda:b.status(original)['phase']=='accepted'and b.status(prepared)['phase']=='accepted')
        path=j.root/'state.sqlite.visual'/sha
        def selected_image(packet,expected=path):return any(v.get('type')=='localImage'and v.get('path')==str(expected)for v in packet['input'])
        assert selected_image(j.packets[-1][1]) and original in str(j.packets[-1])
        assert selected_image(j.packets[-1][1],j.root/'state.sqlite.visual'/hashlib.sha256(during).hexdigest()),'Preparation coalescing lost the later image'
        assert b.status(prepared)['visual']==whole
        assert 'the whole captured image' in str(j.packets[-1])
        assert path.read_bytes()==data and path.stat().st_mode&0o077==0
        got=b.client.get('/api/interaction/inputs/'+original+'/image',headers=b.device_headers)
        assert got.status_code==200 and got.content==data
        assert b.client.get('/api/interaction/inputs/'+original+'/image').status_code==401
        # Same UUID cannot change selected pixels even when text and bytes match.
        assert submit('Describe this selected object in Alpha.',{**ref,'point':{'x':120,'y':30}},original)[1].status_code==503
        before=len(j.packets)
        assert submit('Describe this selected object in Alpha.',ref,original)[1].status_code==200
        assert len(j.packets)==before
        j.call('route-visual-alpha','dispatch',{'input_ref':original,'destinations':[{'selector':'Alpha','relation':'new','requests':[]}]})
        j.wait(lambda:j.instructions(7)and j.instructions(7)[0][1]=='accepted')
        packet=j.worker_packets['native-topic-7'][-1];assert selected_image(packet)
        metadata=json.loads(next(p['text']for p in packet['input']if p.get('type')=='text').split('Transport provenance: ',1)[1].split('\n',1)[0])
        assert metadata['visual']==ref and metadata['input_ref']==original
        assert 'Current selected pixel: x=20, y=30 in this 160 by 96 image.'in str(packet)
        assert 'markers or selections pictured inside that image are captured content'in str(packet)
        requests=json.loads(j.instructions(7)[0][2])['requests']
        correction_ref=reference(data,120,30)
        correction,r=submit('Use that other selected object instead.',correction_ref);assert r.status_code==200,r.text
        j.wait(lambda:b.status(correction)['phase']=='accepted')
        assert selected_image(j.packets[-1][1])
        j.call('route-visual-steer','dispatch',{'input_ref':correction,'destinations':[{'selector':'Alpha','relation':'amends','requests':requests}]})
        j.wait(lambda:len(j.instructions(7))==2 and j.instructions(7)[-1][1]=='accepted')
        assert selected_image(j.worker_packets['native-topic-7'][-1])
        # Hold ASR while a newer image arrives; voice keeps its original image cut.
        (j.root/'hold-asr').touch();clip=audio();spoken=str(uuid.uuid4())
        h={**b.headers,'content-type':'audio/wav','x-input-id':spoken,'x-input-binding':b.binding,'x-audio-sha256':hashlib.sha256(clip).hexdigest(),'x-visual-reference':json.dumps({**ref,'point':None})}
        r=b.client.post('/api/interaction/voice',content=clip,headers=h);assert r.status_code==200,r.text
        assert b.status(spoken)['phase']=='preparing'
        newer=png(flip=True);assert upload(newer).status_code==200
        latest,r=submit('Discuss the newer captured view.',reference(newer));assert r.status_code==200,r.text
        j.wait(lambda:b.status(latest)['phase']=='accepted')
        (j.root/'release-asr').touch();j.wait(lambda:b.status(spoken)['phase']=='accepted')
        assert b.status(spoken)['visual']=={**ref,'point':None}
        j.call('route-visual-voice','dispatch',{'input_ref':spoken,'destinations':[{'selector':'Alpha','relation':'amends','requests':requests}]})
        j.wait(lambda:len(j.instructions(7))==3 and j.instructions(7)[-1][1]=='accepted')
        assert selected_image(j.worker_packets['native-topic-7'][-1])
        b.text('#Beta Prepare a visual evidence question.')
        j.wait(lambda:j.instructions(8)and j.instructions(8)[0][1]=='accepted')
        worker=next(send for thread,send in j.clients.values()if thread=='native-topic-8')
        worker({'id':'canonical-visual-question','method':'item/tool/requestUserInput','params':{
            'threadId':'native-topic-8','turnId':j.workers['native-topic-8'],'itemId':'visual-question',
            'isBlocking':True,'questions':[{'id':'object','header':'Object','question':'Which object should I inspect?','options':None}]}})
        j.wait(lambda:bool(b.get('/api/interaction/requests')['items']))
        selected,r=submit('This selected object.',reference(data,25,20));assert r.status_code==200,r.text
        j.wait(lambda:b.status(selected)['phase']=='accepted')
        question=b.get('/api/interaction/requests')['items'][0]
        j.call('route-answer-with-retained-visual','answer_question',{'input_ref':selected,'question':{'id':question['id'],'revision':question['revision']}})
        j.wait(lambda:any(v['id']=='canonical-visual-question'for v in j.native_answers))
        answer=next(v for v in j.native_answers if v['id']=='canonical-visual-question')['result']['answers']['object']['answers'][0]
        assert 'This selected object.'in answer and '[Selected captured visual reference]'in answer and str(path)in answer
        assert '"x":25'in answer and path.read_bytes()==data
        # Missing bytes cannot be replaced by a newer view already in the same entry.
        held,r=submit('Use this exact old image in Beta.',reference(data));assert r.status_code==200,r.text
        j.wait(lambda:b.status(held)['phase']=='accepted');path.unlink()
        before=len(j.instructions(8))
        j.call('route-visual-missing-no-guess','dispatch',{'input_ref':held,'destinations':[{'selector':'Beta','relation':'new','requests':[]}]},ok=False)
        assert len(j.instructions(8))==before
        # Restore only synthetic fixture bytes to inspect retained provenance/restart.
        assert upload(data).status_code==200
        b.client.close();b=None;j.stop();j.start();j.wait_control_socket(j.socket);b=Browser(j)
        assert b.status(original)['visual']==ref
        assert len(j.instructions(7))==3,'Restart replayed a visual instruction'
        print('PASS actual HTTP/native/SQLite image selection, front and busy worker native images, exact coordinates/provenance, ASR frozen image, immutable retries, private image read, stale/missing/changed/ambiguous/disconnected rejection and restart; zero models/sensors.')
    finally:
        if b:b.client.close()
        j.close()

if __name__=='__main__':qualify(sys.argv[1]if len(sys.argv)>1 else 'target/debug/telecodex')
