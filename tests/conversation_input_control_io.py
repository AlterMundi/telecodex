#!/usr/bin/env python3
"""Original non-Telegram text through the actual owner socket/native worker.

All accounts, Telegram, native peers and output are synthetic; zero model calls.
"""
import json
import errno
import os
import time
import re
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
        self.hold_telegram=False
        self.telegram_methods=[]
        self.native_answers=[]
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
        if method is None and message.get('id') in ('canonical-question','canonical-approval'):
            self.native_answers.append(message)
            return True
        if method=='thread/resume' and self.hold_resume:
            self.resume_seen.set()
            assert self.resume_release.wait(timeout=10),'Synthetic resume was never released'
        selected=params.get('clientUserMessageId','')
        if method not in ('turn/start','turn/steer') or not (selected.startswith('conversation:') or self.hold_telegram and selected.startswith('telegram:')):
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
            correction,_=journey.submit(binding,'Correction admitted while root prepares: '+change)
            journey.wait(lambda:journey.row('SELECT phase FROM conversation_inputs WHERE id=?',(correction['id'],))==[('steering',)])
            with sqlite3.connect(journey.root/'state.sqlite') as db:
                if change=='clear':
                    db.execute('UPDATE sessions SET codex_thread_id=NULL,force_fresh_thread=1 WHERE thread_id=1')
                else:
                    db.execute("INSERT INTO bot_state(key,value) VALUES('native_writer_hold:100:1','fixture-hold')")
            journey.resume_release.set()
            journey.wait(lambda:journey.row('SELECT phase FROM conversation_inputs WHERE id=?',(pending['id'],))==[('retained_unsent',)])
            journey.wait(lambda:journey.row('SELECT phase FROM conversation_inputs WHERE id=?',(correction['id'],))==[('retained_unsent',)])
            assert len(journey.packets)==3,'Retired/held root or correction reached native write'
            with sqlite3.connect(journey.root/'state.sqlite') as db:
                if change=='clear':
                    assert db.execute('SELECT codex_thread_id,force_fresh_thread FROM sessions WHERE thread_id=1').fetchone()==(None,1)
                    db.execute('UPDATE sessions SET codex_thread_id=?,force_fresh_thread=0 WHERE thread_id=1',(journey.web_thread,))
                else:
                    db.execute("DELETE FROM bot_state WHERE key='native_writer_hold:100:1'")
            if change=='clear':
                assert not journey.read('input','submit',json.dumps(pending),ok=False)['ok']
                binding=journey.read('presentation','list','0','0')['result']['binding']
                assert binding!=pending['binding'],'Clear/restore revived the old browser generation'
            retry={**pending,'binding':binding}
            repeated=journey.read('input','submit',json.dumps(retry))['result']
            assert not repeated['new_input'] and repeated['phase']=='retained_unsent'
            if change=='clear':assert repeated['retired']
        journey.hold_resume=False
        assert not journey.messages,'Retired or held browser input published an error to Telegram'
        print('PASS actual owner socket → durable original browser text → same native worker/steer; exact >4KiB input, accepted receipts, reconnect dedup, changed-original rejection, uncertain steer not replayed and resolved by late acknowledgment, no fabricated Telegram update/publication; zero model calls')
        print('PASS held actual resume + entry clear/restore and maintenance hold: no native submission, no restored binding, retained known-unsent input and no reconnect replay')
    finally:
        journey.close()


def mixed_origin_returns(binary):
    journey=Inputs(binary)
    forbidden={'sendMessage','sendMessageDraft','editMessageText','sendPhoto','sendDocument','sendVideo','sendAudio','sendVoice','sendChatAction'}
    try:
        journey.start();journey.admit('Seed existing conversation',topic=1)
        journey.wait(lambda:journey.has('Initial goal turn delivered.',1));journey.idle()
        binding=journey.read('presentation','list','0','0')['result']['binding']
        journey.hold_telegram=True
        journey.admit('A Telegram-root task remains active.',topic=1)
        journey.wait(lambda:len(journey.packets)==1)
        assert journey.row('SELECT count(*) FROM conversation_returns')==[(0,)]
        web,_=journey.submit(binding,'Continue this active task from the canonical conversation.')
        journey.wait(lambda:journey.row('SELECT phase FROM conversation_inputs WHERE id=?',(web['id'],))==[('accepted',)])
        assert len(journey.packets)==2 and journey.packets[1][0]=='turn/steer','Browser correction waited for the root task'
        journey.telegram_methods.clear();journey.messages.clear()
        thread=journey.web_thread;turn=journey.web_turn
        journey.broadcast(thread,'item/completed',{'turnId':turn,'item':{'type':'agentMessage','id':'canonical-progress','phase':'commentary','text':'Browser-influenced progress must stay canonical.'}})
        send=next(send for selected,send in journey.clients.values() if selected==thread)
        send({'id':'canonical-question','method':'item/tool/requestUserInput','params':{'threadId':thread,'turnId':turn,'itemId':'canonical-choice','isBlocking':True,'questions':[{'id':'scope','header':'Scope','question':'Which scope should this private continuation use?','isOther':True,'isSecret':False,'options':[{'label':'Selected scope','description':'Only this request'}]}]}})
        send({'id':'canonical-approval','method':'item/commandExecution/requestApproval','params':{'threadId':thread,'turnId':turn,'itemId':'canonical-command','command':'synthetic command requiring an explicit human choice','reason':'Do not grant this automatically'}})
        journey.wait(lambda:journey.row("SELECT count(*) FROM human_interactions WHERE state='pending' AND json_extract(payload_json,'$.source.kind') IN ('question','approval')")==[(2,)])
        assert not journey.native_answers,'A native request was answered without human selection'
        runtime=''.join(v['value'] for k,v in sorted(journey.packets[0][1]['additionalContext'].items()) if k=='telecodex.runtime' or k.startswith('telecodex.runtime.'))
        output=Path(re.search(r'\n([^\n]+/out)\n',runtime).group(1))
        artifact=output/'canonical-deliverable.txt';artifact.write_text('Canonical-only generated artifact')
        journey.broadcast(thread,'item/completed',{'turnId':turn,'item':{'type':'agentMessage','id':'canonical-final','phase':'final_answer','text':'The canonical result and its artifact remain retained.'}})
        journey.broadcast(thread,'turn/completed',{'turn':{'id':turn,'status':'completed'}})
        journey.idle()
        assert journey.row("SELECT count(*) FROM interaction_native_messages WHERE text='The canonical result and its artifact remain retained.'")==[(1,)]
        assert artifact.read_text()=='Canonical-only generated artifact'
        assert not forbidden.intersection(journey.telegram_methods),(journey.telegram_methods,journey.messages)
        assert not any(answer.get('result',{}).get('decision') in ('accept','acceptForSession') for answer in journey.native_answers)
        journey.stop();journey.start();journey.telegram_methods.clear()
        journey.admit('Continue the same native context after restart.',topic=1)
        journey.wait(lambda:len(journey.packets)==3)
        journey.finish(journey.web_thread,journey.web_turn);journey.idle()
        journey.wait(lambda:journey.has('Initial goal turn delivered.',1))
        assert sum(m['text']=='Initial goal turn delivered.' for m,_ in journey.messages)==1
        assert journey.packets[-1][1]['input'][0]['text']=='Continue the same native context after restart.'
        assert journey.row('SELECT count(*) FROM conversation_inputs')==[(1,)],'Direct continuation entered web ingress'
        print('PASS active Telegram-root + browser steer retains canonical progress/final/artifact and unanswered native requests; after restart a raw direct continuation restores its own topic response once; zero models')
    finally:
        journey.close()


def late_artifact_return(binary):
    journey=Inputs(binary)
    try:
        journey.start();journey.admit('Seed existing conversation',topic=1)
        journey.wait(lambda:journey.has('Initial goal turn delivered.',1));journey.idle()
        binding=journey.read('presentation','list','0','0')['result']['binding']
        journey.hold_telegram=True
        journey.admit('Produce an artifact on the original return surface.',topic=1)
        journey.wait(lambda:len(journey.packets)==1)
        runtime=''.join(v['value'] for k,v in sorted(journey.packets[0][1]['additionalContext'].items()) if k=='telecodex.runtime' or k.startswith('telecodex.runtime.'))
        output=Path(re.search(r'\n([^\n]+/out)\n',runtime).group(1))
        artifact=output/'late-deliverable.txt';artifact.write_text('Retained across return-selection race')
        journey.finish(journey.web_thread,journey.web_turn)
        # The ordinary final text has completed. The artifact is now waiting at
        # the shared outbound limiter, after its retained attempt record.
        journey.wait(lambda:journey.row("SELECT count(*) FROM audit_log WHERE action='artifact_delivery_attempted'")==[(1,)])
        uploads={'sendPhoto','sendDocument','sendVideo','sendAudio','sendVoice'}
        assert not uploads.intersection(journey.telegram_methods)
        original,_=journey.submit(binding,'Select the canonical return while the artifact is still unsent.')
        journey.wait(lambda:journey.row('SELECT phase FROM conversation_inputs WHERE id=?',(original['id'],))==[('accepted',)])
        assert artifact.read_text()=='Retained across return-selection race','Successful native completion deleted an unuploaded artifact'
        assert journey.row("SELECT status FROM turns WHERE id=2")==[('completed',)]
        assert not uploads.intersection(journey.telegram_methods),journey.telegram_methods
        journey.finish(journey.web_thread,journey.web_turn);journey.idle()
        assert artifact.exists()
        print('PASS late return selection during actual artifact cooldown: no upload attempt, native completion retained and unuploaded file survives ordinary finalization cleanup; zero models')
    finally:
        journey.close()


def canonical_turn_skips_unused_limits_history(binary):
    """A slow quota source must not hold web dialogue; legacy previews still read it."""
    journey=Inputs(binary);opened=threading.Event();release=threading.Event();done=threading.Event();writer=None
    try:
        journey.config.write_text(journey.config.read_text().replace('show_unfinished_messages=false','show_unfinished_messages=true').replace('[interaction]\n','[interaction]\ntelegram_projection=false\ntelegram_entry_projection=false\n'))
        journey.start();journey.admit('Seed the selected entry.',topic=1)
        journey.wait(lambda:journey.has('Initial goal turn delivered.',1));journey.idle()
        journey.wait_control_socket(journey.socket)
        binding=journey.read('presentation','list','0','0')['result']['binding']
        history=journey.root/'codex-home'/'sessions';history.mkdir(parents=True,exist_ok=True)
        fifo=history/'blocked-limits.jsonl';os.mkfifo(fifo,0o600)
        def delayed_history():
            while not done.is_set():
                try:fd=os.open(fifo,os.O_WRONLY|os.O_NONBLOCK)
                except OSError as error:
                    if error.errno!=errno.ENXIO:raise
                    done.wait(.01);continue
                opened.set();fifo.unlink()
                try:
                    release.wait(10)
                    os.write(fd,(json.dumps({'timestamp':'2026-10-27T07:00:00Z','payload':{'type':'token_count','rate_limits':{'secondary':{'used_percent':17,'window_minutes':10080,'resets_at':1793200000}}}})+'\n').encode())
                except BrokenPipeError:pass
                finally:os.close(fd)
                return
        writer=threading.Thread(target=delayed_history,daemon=True);writer.start()
        journey.packets.clear()
        original,_=journey.submit(binding,'Answer in the web conversation without waiting for a quota banner.')
        journey.wait(lambda:opened.is_set()or bool(journey.packets),seconds=4)
        assert not opened.is_set(),'Canonical web turn opened blocking quota history before native admission'
        journey.wait(lambda:journey.row('SELECT phase FROM conversation_inputs WHERE id=?',(original['id'],))==[('accepted',)])
        journey.finish(journey.web_thread,journey.web_turn);journey.idle();journey.stop()
        # The legacy unfinished-preview path still has a visible quota banner.
        fake=journey.root/'codex';body=fake.read_text()
        marker="if sys.argv[1:]!=['app-server','proxy']: sys.exit(7)"
        replacement="""if sys.argv[1:]==['app-server']:
    import json
    def emit(value):print(json.dumps(value),flush=True)
    for raw in sys.stdin:
        message=json.loads(raw);method=message.get('method');ident=message.get('id')
        if ident is None:continue
        result={}
        if method in ('thread/start','thread/resume'):result={'thread':{'id':'legacy-limits-thread'},'model':'fixture-model','modelProvider':'fixture'}
        elif method=='thread/goal/get':result={'goal':None}
        elif method=='turn/start':result={'turn':{'id':'legacy-limits-turn'}}
        emit({'id':ident,'result':result})
        if method=='turn/start':
            emit({'method':'turn/started','params':{'threadId':'legacy-limits-thread','turn':{'id':'legacy-limits-turn','status':'inProgress'}}})
            emit({'method':'item/completed','params':{'threadId':'legacy-limits-thread','turnId':'legacy-limits-turn','item':{'id':'legacy-answer','type':'agentMessage','phase':'final_answer','text':'Legacy quota preview completed.'}}})
            emit({'method':'turn/completed','params':{'threadId':'legacy-limits-thread','turn':{'id':'legacy-limits-turn','status':'completed'}}})
    sys.exit(0)
"""+marker
        assert marker in body;fake.write_text(body.replace(marker,replacement))
        legacy=journey.config.read_text().split('\n[interaction]\n',1)[0]
        legacy=re.sub(r'\[session_control\]\n.*?(?=\[telegram\])','',legacy,flags=re.S)
        legacy=legacy.replace('shared_app_server=true','shared_app_server=false').replace('[telegram]\n','[telegram]\nuse_message_drafts=false\n')
        journey.config.write_text(legacy)
        journey.start();journey.admit('Keep the legacy preview quota.',topic=7)
        journey.wait(lambda:opened.is_set(),seconds=5)
        release.set();journey.wait(lambda:journey.has('Legacy quota preview completed.',7))
        assert any('83%'in message['text']for message,_ in journey.messages),'Legacy quota banner disappeared'
        journey.idle()
        print('PASS canonical web admission bypasses blocked quota history; legacy unfinished preview still reads and displays the quota; zero models.')
    finally:
        release.set();done.set()
        if writer:writer.join(2)
        journey.close()


if __name__=='__main__':
    binary=str(Path(sys.argv[1]).resolve())
    qualify(binary)
    mixed_origin_returns(binary)
    late_artifact_return(binary)
    canonical_turn_skips_unused_limits_history(binary)
