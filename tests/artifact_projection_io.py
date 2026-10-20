#!/usr/bin/env python3
"""Immutable source-file projection over the real queue and multipart HTTP; zero models."""
from email import policy
from email.parser import BytesParser
from pathlib import Path
import sys
import threading
import re
from artifact_return_io import ArtifactRouting, prepare
from topic_projection_io import Projected


class ProjectedFiles(ArtifactRouting,Projected):
    def __init__(self,binary):
        self.uploads=[]
        self.upload_seen=threading.Event();self.release_upload=threading.Event()
        self.hold=True;self.lose_image=True;self.versions=[]
        super().__init__(binary)

    def handle_extra_rpc(self,message,send):
        handled=super().handle_extra_rpc(message,send)
        p=message.get('params',{})
        if self.routing and message.get('method')=='turn/start' and p.get('threadId')=='native-topic-7':
            out=Path(re.search(r'\n([^\n]+/out)\n',p['additionalContext']['telecodex.runtime']['value']).group(1))
            self.expected['alpha.txt']=f'Immutable file version {len(self.versions)+1}\n'.encode()
            (out/'alpha.txt').write_bytes(self.expected['alpha.txt'])
            self.versions.append(self.expected['alpha.txt'])
        return handled

    def multipart_reply(self,method,headers,raw):
        message=BytesParser(policy=policy.default).parsebytes(
            ('Content-Type: '+headers['Content-Type']+'\r\nMIME-Version: 1.0\r\n\r\n').encode()+raw)
        fields={};file=None
        for part in message.iter_parts():
            name=part.get_param('name',header='content-disposition')
            data=part.get_payload(decode=True)
            if part.get_filename():file={'name':part.get_filename(),'bytes':data,'field':name}
            else:fields[name]=data.decode()
        assert fields['chat_id']=='100' and fields['message_thread_id']=='7',fields
        assert file and file['bytes'] in (self.versions if file['name']=='alpha.txt' else [self.expected[file['name']]]),file
        assert fields['caption'].startswith('File · '+file['name'])
        assert 'response' in fields['caption'] and 'daimon-result' not in fields['caption']
        assert b'PRIVATE UNRELATED BYTES' not in raw
        file['caption']=fields['caption'];self.uploads.append(file)
        if self.hold:
            self.upload_seen.set();assert self.release_upload.wait(timeout=25)
            self.hold=False
        if self.lose_image and file['name']=='image.png':
            self.lose_image=False
            return False  # Server observed the upload, but its ACK is lost.
        return {'ok':True,'result':{'message_id':900+len(self.uploads),
            'chat':{'id':100,'type':'private'},'message_thread_id':7}}


def qualify(binary):
    j=ProjectedFiles(str(Path(binary).resolve()))
    try:
        _,binding=prepare(j,projection=True)
        original,_=j.submit(binding,'#Alpha Create deliverables and preserve their exact bytes.')
        j.wait(lambda:j.instructions(7) and j.instructions(7)[0][1]=='accepted')
        j.answer_worker(7,'First file response independent of source upload.')
        assert j.upload_seen.wait(timeout=12),'No source file publication'
        assert any('First file response' in item['preview'] for item in j.read('presentation','list','0','0')['result']['items'])
        direct='Direct source-topic follow-up while an earlier file upload is held.'
        j.admit(direct,topic=7)
        j.wait(lambda:len(j.direct_packets)==1)
        assert j.direct_packets[-1]['input'][0]['text']==direct
        assert len(j.instructions(7))==1,'Direct input entered the router journal'
        j.answer(7,'Direct response independent of source upload.',direct=True)
        j.wait(lambda:j.has('Direct response independent of source upload.',7),seconds=5);j.idle()
        assert not any('Direct response independent' in item['preview'] for item in j.read('presentation','list','0','0')['result']['items'])
        j.submit(binding,'#Alpha Continue while the earlier upload is held.')
        j.wait(lambda:len(j.instructions(7))==2 and j.instructions(7)[-1][1]=='accepted')
        j.answer_worker(7,'Second response before source upload finishes.');j.idle()
        j.wait(lambda:j.has('Second response before source upload finishes.',7),seconds=5)
        j.release_upload.set()
        j.wait(lambda:len(j.uploads)==6)
        j.wait(lambda:j.row("SELECT COUNT(*) FROM topic_publications WHERE kind='artifact' AND phase IN ('sent','uncertain')")==[(6,)])
        assert j.row("SELECT COUNT(*) FROM topic_publications WHERE kind='artifact' AND phase='uncertain'")==[(1,)]
        assert sorted(file['field'] for file in j.uploads)==['document','document','document','photo','photo','photo']
        assert len(j.instructions(7))==2
        texts=[file for file in j.uploads if file['name']=='alpha.txt']
        assert set(file['bytes'] for file in texts)==set(j.versions)
        for file in texts:
            expected=('First file response','Direct response','Second response')[j.versions.index(file['bytes'])]
            assert expected in file['caption'],file
        before=j.row('SELECT COUNT(*) FROM turns')
        j.stop();j.start();j.wait_control_socket(j.socket)
        # Exact retained input deduplication does not recapture or resend outputs.
        j.submit(binding,original['text'],ident=original['id'])
        assert j.row('SELECT COUNT(*) FROM turns')==before
        assert len(j.uploads)==6
        assert len(j.read('presentation','artifacts','list')['result']['items'])==6
        assert any(row['kind']=='artifact' and row['phase']=='uncertain' for row in j.read('topics','publications')['result']['items'])
        print('PASS exact immutable multipart source files, useful media types, independent same-session text/direct/web continuation, direct input and file-audience isolation, scoped partial output, durable lost-ACK uncertainty and restart without work/file replay; zero models')
    finally:
        j.release_upload.set();j.close()


if __name__=='__main__':qualify(sys.argv[1])
