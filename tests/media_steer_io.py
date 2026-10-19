#!/usr/bin/env python3
"""Direct forwarded media reaches an active native turn; zero live/model calls."""
import base64
import hashlib
import pathlib
import sys
from routing_io import Routing

PNG=base64.b64decode('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+jQ1cAAAAASUVORK5CYII=')

class Media(Routing):
    def __init__(self,binary,mode):
        self.media_mode=mode
        self.media_packets=[]
        super().__init__(binary)
        self.media_files={'capture.png':PNG}
    def photo(self):
        with self.lock:
            self.sequence+=1
            ident=self.sequence
            self.updates.append({'update_id':ident,'message':{
                'message_id':ident,'message_thread_id':7,
                'from':{'id':100,'is_bot':False,'first_name':'Human'},
                'chat':{'id':100,'type':'private'},'caption':'Inspect this forwarded screenshot now.',
                'forward_origin':{'type':'hidden_user','date':100,'sender_user_name':'Synthetic original'},
                'photo':[{'file_id':'capture.png','file_unique_id':'fixture-image','width':1,'height':1,'file_size':len(PNG)}]}})
            return ident
    def handle_extra_rpc(self,message,send):
        params=message.get('params',{})
        if message.get('method')=='turn/steer' and any(p.get('type')=='localImage' for p in params.get('input',[])):
            self.media_packets.append(params)
            path=pathlib.Path(next(p['path'] for p in params['input'] if p.get('type')=='localImage'))
            assert hashlib.sha256(path.read_bytes()).digest()==hashlib.sha256(PNG).digest()
            if self.media_mode=='rejected':
                send({'id':message['id'],'error':{'code':-32600,'message':'Synthetic definite rejection'}})
                return True
            if self.media_mode=='uncertain':return True
        return super().handle_extra_rpc(message,send)

def qualify(binary,mode):
    j=Media(binary,mode)
    try:
        j.prepare(telegram_ingress=False);j.allow_native_launch=True
        j.admit('Hold this existing direct turn.',topic=7)
        j.wait(lambda:len(j.packets.get('native-topic-7',[]))==1)
        initial=j.row('SELECT count(*) FROM turns')[0][0]
        update=j.photo()
        j.wait(lambda:len(j.media_packets)==1)
        media=j.media_packets[0]
        path=pathlib.Path(next(p['path'] for p in media['input'] if p.get('type')=='localImage'))
        assert media['expectedTurnId']==j.runs['native-topic-7']
        assert path.exists()
        assert 'Inspect this forwarded screenshot now.' in media['input'][0]['text']
        assert str(path) in media['input'][0]['text']
        if mode=='rejected':
            j.wait(lambda:j.has('Files downloaded and queued',7))
            assert path.exists(),'Definite rejection deleted the queued input'
            j.answer(7,'First execution finished.')
            j.wait(lambda:j.row('SELECT count(*) FROM turns')[0][0]==initial+1)
            j.wait(lambda:len(j.packets['native-topic-7'])==2)
            assert path.exists(),'Fallback lost the exact downloaded file'
            assert any(p.get('type')=='localImage' and p['path']==str(path) for p in j.packets['native-topic-7'][-1]['input'])
            j.answer(7,'Queued image processed.')
        elif mode=='uncertain':
            j.wait(lambda:j.row('SELECT status FROM incoming_updates WHERE update_id=?',(update,))==[('undetermined',)],seconds=15)
            assert path.exists()
            assert j.row('SELECT count(*) FROM turns')[0][0]==initial,'Uncertain media was replayed'
            # Native completion is deliberately absent; disconnect retains input.
            j.stop()
            assert path.exists(),'Unknown native completion deleted input'
            print('PASS uncertain media retained without replay or file deletion')
            return
        else:
            j.wait(lambda:j.row('SELECT status FROM incoming_updates WHERE update_id=?',(update,))==[('started',)])
            j.admit('This later text describes the same screenshot.',topic=7)
            j.wait(lambda:len(j.packets['native-topic-7'])==3)
            assert path.exists(),'Later steering retired the file too early'
            assert j.row('SELECT count(*) FROM turns')[0][0]==initial,'Accepted photo created an extra turn'
            j.answer(7,'Image and follow-up processed in the same execution.')
        j.idle();j.wait(lambda:not path.exists())
        assert not j.errors,j.errors
        print('PASS '+mode+' direct forwarded image: exact bytes, active-turn delivery or definite-rejection fallback, and completion cleanup')
    finally:j.close()

if __name__=='__main__':
    for mode in ('accepted','rejected','uncertain'):qualify(str(pathlib.Path(sys.argv[1]).resolve()),mode)
