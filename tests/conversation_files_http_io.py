"""Actual HTTP/original/native handoff with opaque files; synthetic peer, zero models."""
import hashlib,json,os,sys,uuid
from pathlib import Path
from urllib.parse import quote
from conversation_cold_entry_http_io import ColdEntry,Browser
from conversation_voice_http_io import audio

def received_originals(packet,refs):
    if isinstance(packet,tuple):packet=packet[1]
    text='\n'.join(part.get('text','') for part in packet['input'])
    marker='[Retained original file attachments]\n'
    assert marker in text, 'Native packet lacks retained original paths'
    selected=json.loads(text.split(marker,1)[1].splitlines()[0])
    assert [f['file_id'] for f in selected]==[f['file_id'] for f in refs]
    for original,ref in zip(selected,refs):
        data=Path(original['local_path']).read_bytes()
        assert len(data)==ref['size'] and hashlib.sha256(data).hexdigest()==ref['sha256']

def qualify(binary):
    j=ColdEntry(str(Path(binary).resolve()));b=None
    try:
        j.prepare();b=Browser(j,companion=True);b.client.cookies.clear()
        def reference(name,data,mime='application/octet-stream'):
            return {'file_id':str(uuid.uuid4()),'name':name,'size':len(data),'mime':mime,'sha256':hashlib.sha256(data).hexdigest()}
        def upload(data,ref,headers=None):return b.client.post('/api/interaction/files',content=data,headers={**(b.device_headers if headers is None else headers),'content-type':'application/octet-stream','x-file-reference':quote(json.dumps(ref),safe='')})
        def submit(text,refs,ident=None):
            ident=ident or str(uuid.uuid4());return ident,b.client.post('/api/interaction/inputs',json={'id':ident,'binding':b.binding,'text':text,'files':refs},headers=b.device_headers)
        raw=Path(os.environ['TELECODEX_ORIGINAL_FILE_FIXTURE']).read_bytes() if os.environ.get('TELECODEX_ORIGINAL_FILE_FIXTURE') else b'%PDF-1.4\noriginal scanned fixture\n%%EOF';ref=reference('croquis.pdf',raw,'application/pdf')
        assert upload(raw,ref,{'origin':'https://example.test'}).status_code==401
        assert upload(raw,ref,{k:v for k,v in b.device_headers.items() if k!='x-dashboard-csrf'}).status_code==403
        assert upload(raw+b'x',ref).status_code==400
        assert upload(raw,{**ref,'name':'../escape.pdf'}).status_code==400
        assert upload(raw,ref).status_code==200
        assert not j.packets and j.row('SELECT count(*) FROM conversation_inputs')==[(0,)]
        text=b'notes\n';binary_data=bytes(range(256));refs=[ref,reference('notes.txt',text,'text/plain'),reference('sample.bin',binary_data)]
        assert upload(text,refs[1]).status_code==200 and upload(binary_data,refs[2]).status_code==200
        ident,r=submit('Read the original PDF and notes for Alpha.',refs);assert r.status_code==200,r.text
        j.wait(lambda:b.status(ident)['phase']=='accepted')
        received_originals(j.packets[-1],refs)
        packet=str(j.packets[-1]);assert all(f['sha256'] in packet and f['name'] in packet for f in refs),packet
        got=b.client.get(f'/api/interaction/inputs/{ident}/files/0',headers=b.device_headers);assert got.status_code==200 and got.content==raw
        assert got.headers['content-type']=='application/octet-stream' and got.headers['x-content-type-options']=='nosniff'
        assert b.client.get(f'/api/interaction/inputs/{ident}/files/0').status_code==401
        assert b.client.get(f'/api/interaction/inputs/{ident}/files/3',headers=b.device_headers).status_code==404
        before=len(j.packets);assert submit('Read the original PDF and notes for Alpha.',refs,ident)[1].status_code==200;assert len(j.packets)==before
        assert submit('Read the original PDF and notes for Alpha.',list(reversed(refs)),ident)[1].status_code in (400,503)
        j.call('route-files-alpha','dispatch',{'input_ref':ident,'destinations':[{'selector':'Alpha','relation':'new','requests':[]}]})
        j.wait(lambda:j.instructions(7)and j.instructions(7)[0][1]=='accepted')
        received_originals(j.worker_packets['native-topic-7'][-1],refs)
        packet=str(j.worker_packets['native-topic-7'][-1]);assert all(f['sha256'] in packet and f['name'] in packet for f in refs),packet
        # A second original reaches the busy front/worker without treating binary files as images.
        other,r=submit('Use this same original PDF for Alpha too.',[ref]);assert r.status_code==200,r.text
        j.wait(lambda:b.status(other)['phase']=='accepted');received_originals(j.packets[-1],[ref])
        j.call('route-files-alpha-busy','dispatch',{'input_ref':other,'destinations':[{'selector':'Alpha','relation':'new','requests':[]}]})
        j.wait(lambda:len(j.instructions(7))==2 and j.instructions(7)[-1][1]=='accepted');received_originals(j.worker_packets['native-topic-7'][-1],[ref])
        voice=audio();vid=str(uuid.uuid4());headers={**b.device_headers,'content-type':'audio/wav','x-input-binding':b.binding,'x-input-id':vid,'x-audio-sha256':hashlib.sha256(voice).hexdigest(),'x-file-references':quote(json.dumps([ref]),safe='')}
        r=b.client.post('/api/interaction/voice',content=voice,headers=headers);assert r.status_code==200,r.text
        j.wait(lambda:b.status(vid)['phase']=='accepted');assert b.status(vid)['files']==[ref]
        from unittest.mock import patch
        from dashboard import files as file_store
        with patch.object(file_store,'MAX_STORED_BYTES',1):
            assert upload(raw,ref).status_code==200
            assert upload(b'new',reference('new.bin',b'new')).status_code==507
            assert b.client.get(f'/api/interaction/inputs/{ident}/files/0',headers=b.device_headers).content==raw
        b.client.close();b=None;j.stop();j.start();j.wait_control_socket(j.socket);b=Browser(j)
        assert b.status(ident)['files']==refs
        assert b.client.get(f'/api/interaction/inputs/{ident}/files/0',headers=b.headers).content==raw
        print('PASS original PDF/text/binary upload, access/CSRF, opaque download, immutable retries, text+voice, cold/busy native front and routed worker, restart retention; zero models')
    finally:
        if b:b.client.close()
        j.close()
if __name__=='__main__':qualify(sys.argv[1] if len(sys.argv)>1 else 'target/debug/telecodex')
