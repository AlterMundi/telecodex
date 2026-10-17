#!/usr/bin/env python3
"""Original non-Telegram text through the actual owner socket/native worker.

All accounts, Telegram, native peers and output are synthetic; zero model calls.
"""
import json
from pathlib import Path
import subprocess
import sqlite3
import sys
import threading
import uuid
from goals_io import Goals


class Inputs(Goals):
    def __init__(self,binary):
        self.packets=[]
        self.drop_ack=False
        self.web_thread=None
        self.web_turn=None
        self.late_ack=None
        self.hold_resume=False
        self.resume_seen=threading.Event()
        self.resume_release=threading.Event()
        super().__init__(binary)
        self.socket=self.root/'control.sock'
        config=self.config.read_text().replace('[telegram]',f'[session_control]\nsocket_path="{self.socket}"\nhuman_id=100\n[telegram]')
        self.config.write_text(config+'\n[interaction]\nhuman_id=100\nchat_id=100\ntopic_id=1\n')

    def read(self,*arguments,ok=True):
        run=subprocess.run([self.binary,'--control',str(self.config),*arguments],capture_output=True,text=True,timeout=15)
        assert (run.returncode==0)==ok,run.stderr
        return json.loads(run.stdout)

    def submit(self,binding,text,ident=None,ok=True):
        body={'id':ident or str(uuid.uuid4()),'binding':binding,'text':text}
        return body,self.read('input','submit',json.dumps(body),ok=ok)

    def handle_extra_rpc(self,message,send):
        method=message.get('method');params=message.get('params',{})
        if method=='thread/resume' and self.hold_resume:
            self.resume_seen.set()
            assert self.resume_release.wait(timeout=10),'Synthetic resume was never released'
        if method not in ('turn/start','turn/steer') or not params.get('clientUserMessageId','').startswith('conversation:'):
            return False
        self.packets.append((method,params))
        self.web_thread=params['threadId']
        if method=='turn/start':
            self.web_turn='conversation-turn-'+str(len(self.packets))
            send({'id':message['id'],'result':{'turn':{'id':self.web_turn}}})
            self.broadcast(self.web_thread,'turn/started',{'turn':{'id':self.web_turn,'status':'inProgress'}})
        elif not self.drop_ack:
            send({'id':message['id'],'result':{'turnId':self.web_turn}})
        else:
            self.late_ack=(send,message['id'],self.web_turn)
        return True


def qualify(binary):
    journey=Inputs(binary)
    try:
        journey.start()
        journey.admit('Seed existing integrating conversation',topic=1)
        journey.wait(lambda:journey.has('Initial goal turn delivered.',1))
        journey.idle()
        journey.wait_control_socket(journey.socket)
        initial_journal=journey.row('SELECT count(*) FROM incoming_updates')
        binding=journey.read('presentation','list','0','0')['result']['binding']
        journey.messages.clear()
        text='  Original browser text. ¿Y la corrección? 🦉\n'+'A complex requirement remains exact. '*200
        original,result=journey.submit(binding,text)
        assert result['ok'] and result['result']['new_input']
        journey.wait(lambda:journey.row('SELECT phase FROM conversation_inputs WHERE id=?',(original['id'],))==[('accepted',)])
        assert len(journey.packets)==1 and journey.packets[0][0]=='turn/start'
        assert text in journey.packets[0][1]['input'][0]['text']
        correction,_=journey.submit(binding,'A later correction must enter this running native turn.')
        journey.wait(lambda:journey.row('SELECT phase FROM conversation_inputs WHERE id=?',(correction['id'],))==[('accepted',)])
        assert len(journey.packets)==2 and journey.packets[1][0]=='turn/steer'
        repeated=journey.read('input','submit',json.dumps(correction))['result']
        assert not repeated['new_input'] and repeated['phase']=='accepted'
        forged={**correction,'text':'Replacement of the retained original'}
        assert not journey.read('input','submit',json.dumps(forged),ok=False)['ok']
        assert len(journey.packets)==2
        journey.drop_ack=True
        uncertain,_=journey.submit(binding,'This synthetic steer loses its acknowledgment.')
        journey.wait(lambda:journey.row('SELECT phase FROM conversation_inputs WHERE id=?',(uncertain['id'],))==[('uncertain',)],seconds=18)
        retained=journey.read('input','submit',json.dumps(uncertain))['result']
        assert retained['phase']=='uncertain' and not retained['new_input']
        assert len(journey.packets)==3
        send,ident,turn=journey.late_ack
        send({'id':ident,'result':{'turnId':turn}})
        journey.wait(lambda:journey.row('SELECT phase FROM conversation_inputs WHERE id=?',(uncertain['id'],))==[('accepted',)])
        assert len(journey.packets)==3,'Late acknowledgment caused another submission'
        assert journey.row('SELECT count(*) FROM incoming_updates')==initial_journal,'Browser input fabricated a Telegram update'
        journey.finish(journey.web_thread,journey.web_turn)
        journey.idle()
        assert not journey.messages,'Browser work was published to Telegram'
        # Revocation while the actual native resume is held must not be undone
        # by its late ThreadStarted callback. No global Stop is used.
        for change in ('clear','hold'):
            journey.hold_resume=True
            journey.resume_seen.clear();journey.resume_release.clear()
            pending,_=journey.submit(binding,'Do not submit after '+change)
            assert journey.resume_seen.wait(timeout=8),'Native resume boundary not reached'
            with sqlite3.connect(journey.root/'state.sqlite') as db:
                if change=='clear':
                    db.execute('UPDATE sessions SET codex_thread_id=NULL,force_fresh_thread=1 WHERE thread_id=1')
                else:
                    db.execute("INSERT INTO bot_state(key,value) VALUES('native_writer_hold:100:1','fixture-hold')")
            journey.resume_release.set()
            journey.wait(lambda:journey.row('SELECT phase FROM conversation_inputs WHERE id=?',(pending['id'],))==[('retained_unsent',)])
            assert len(journey.packets)==3,'Retired/held input reached native write'
            with sqlite3.connect(journey.root/'state.sqlite') as db:
                if change=='clear':
                    assert db.execute('SELECT codex_thread_id,force_fresh_thread FROM sessions WHERE thread_id=1').fetchone()==(None,1)
                    db.execute('UPDATE sessions SET codex_thread_id=?,force_fresh_thread=0 WHERE thread_id=1',(journey.web_thread,))
                else:
                    db.execute("DELETE FROM bot_state WHERE key='native_writer_hold:100:1'")
            repeated=journey.read('input','submit',json.dumps(pending))['result']
            assert not repeated['new_input'] and repeated['phase']=='retained_unsent'
            if change=='clear':assert repeated['retired']
        journey.hold_resume=False
        assert not journey.messages,'Retired or held browser input published an error to Telegram'
        print('PASS actual owner socket → durable original browser text → same native worker/steer; exact >4KiB input, accepted receipts, reconnect dedup, changed-original rejection, uncertain steer not replayed and resolved by late acknowledgment, no fabricated Telegram update/publication; zero model calls')
        print('PASS held actual resume + entry clear/restore and maintenance hold: no native submission, no restored binding, retained known-unsent input and no reconnect replay')
    finally:
        journey.close()


if __name__=='__main__':
    qualify(str(Path(sys.argv[1]).resolve()))
