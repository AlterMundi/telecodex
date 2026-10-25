#!/usr/bin/env python3
"""Resume committed creation intents at known boundaries, never unknown effects.
Every user, HTTP, SQLite and native/model peer is synthetic.
"""
import json
import pathlib
import sqlite3
import signal
import sys
from creation_io import Creation
from semantic_routing_io import Semantic


class Recovery(Creation):
    def __init__(self,binary):
        self.resume_mode=False
        self.fail_first=False
        self.inspect_resume=False
        self.reject_resume=False
        super().__init__(binary)

    def wait(self,condition,seconds=15):
        try:return super().wait(condition,seconds)
        except AssertionError as failure:
            # Preserve the actual failed boundary before the fixture is removed.
            # Stop only this synthetic process; do not retry or relax the assertion.
            diagnostic={}
            if self.process:
                if self.process.poll() is None:self.process.send_signal(signal.SIGINT)
                _,error=self.process.communicate(timeout=8)
                self.process=None
                diagnostic['stderr']=error.replace('synthetic-question-token','[redacted]')
            with sqlite3.connect(self.root/'state.sqlite') as db:
                for table in ('incoming_updates','sessions','turns','interaction_creations'):
                    diagnostic[table]=db.execute('SELECT * FROM '+table+' ORDER BY rowid DESC LIMIT 8').fetchall()
            raise AssertionError(str(failure)+'; retained synthetic boundary='+json.dumps(diagnostic,default=str)) from failure

    def crash(self):
        # This fixture owns the loopback listener and synthetic native peers.
        # Simulate process loss while a committed intent is still before HTTP.
        self.process.kill()
        _,error=self.process.communicate(timeout=8)
        assert 'synthetic-question-token' not in error
        self.process=None
        # Advance only this dead fixture's recovery lease, as artifacts_io does.
        # Never bypass the live-owner fence or wait two minutes in each test.
        with sqlite3.connect(self.root/'state.sqlite') as db:
            db.execute("UPDATE app_instance_lock SET heartbeat_at=datetime('now','-1 hour')")
            db.execute("UPDATE turns SET started_at=datetime('now','-1 hour') WHERE status='running'")
            db.execute("UPDATE sessions SET updated_at=datetime('now','-1 hour') WHERE busy=1")

    def call(self,ident,operation,arguments):
        if ident=='inputs' and self.resume_mode:
            return Semantic.call(self,'create','creation_resume',{'input_update':self.input_ref})
        return super().call(ident,operation,arguments)

    def handle_extra_rpc(self,m,send):
        if m.get('method') is None and m.get('id')=='create' and self.resume_mode and (self.inspect_resume or self.reject_resume):
            assert m['result']['success'] is self.inspect_resume
            if self.inspect_resume:assert json.loads(m['result']['contentItems'][0]['text'])['phase']=='uncertain'
            self.creation_outputs.append(m)
            send({'method':'turn/completed','params':{'threadId':self.entry,'turn':{'id':self.entry_turn,'status':'completed'}}})
            return True
        if m.get('method') is None and m.get('id')=='create' and self.fail_first:
            assert m['result']['success'] is False
            self.fail_first=False
            self.creation_outputs.append(m)
            send({'method':'turn/completed','params':{'threadId':self.entry,'turn':{'id':self.entry_turn,'status':'completed'}}})
            return True
        return super().handle_extra_rpc(m,send)


def known_unsent(binary):
    j=Recovery(binary)
    try:
        j.prepare()
        j.cooldown_once=True
        j.admit('/interaction status',topic=1)
        j.wait(j.cooldown_seen.is_set)
        original=j.admit('NATURAL-ROUTE Start the independent Gamma task.',topic=1)
        j.wait(lambda:j.row('SELECT phase FROM interaction_creations')==[('prepared',)])
        j.crash()
        assert j.topic_calls==0
        j.resume_mode=True
        j.start()
        j.admit('Continue the already committed original creation intent.',topic=1)
        j.wait(lambda:len(j.creation_outputs)==4)
        j.wait(lambda:len(j.instructions(77))==1 and j.instructions(77)[0][1]=='accepted')
        assert j.topic_calls==1
        assert json.loads(j.instructions(77)[0][2])['origin']['message_id']==original
        assert j.row('SELECT count(*) FROM interaction_creations')==[(1,)]
        print('PASS explicit recovery of interrupted known-unsent creation uses the same original intent and performs one topic mutation')
    finally:j.close()


def known_topic(binary):
    j=Recovery(binary)
    try:
        j.prepare()
        with sqlite3.connect(j.root/'state.sqlite') as db:
            db.execute("CREATE TRIGGER fixture_initialization_failure BEFORE UPDATE ON sessions WHEN NEW.thread_id=77 AND NEW.force_fresh_thread=1 BEGIN SELECT RAISE(ABORT,'Synthetic transient initialization failure'); END")
        j.fail_first=True
        original=j.admit('NATURAL-ROUTE Start a distinct Gamma task.',topic=1)
        j.wait(lambda:len(j.creation_outputs)==1)
        j.wait(lambda:j.row('SELECT phase FROM interaction_creations')==[('retained_topic',)])
        j.idle()
        assert j.topic_calls==1 and j.instructions(77)==[]
        j.stop()
        with sqlite3.connect(j.root/'state.sqlite') as db:db.execute('DROP TRIGGER fixture_initialization_failure')
        j.creation_outputs.clear();j.resume_mode=True
        j.start()
        j.admit('Finish initializing the already confirmed topic for the original task.',topic=1)
        j.wait(lambda:len(j.creation_outputs)==4)
        j.wait(lambda:len(j.instructions(77))==1 and j.instructions(77)[0][1]=='accepted')
        assert j.topic_calls==1
        assert json.loads(j.instructions(77)[0][2])['origin']['message_id']==original
        assert j.row('SELECT count(*) FROM interaction_enrollments')==[(1,)]
        print('PASS confirmed-topic recovery finishes initialization and atomic enrollment after restart without another topic RPC')
    finally:j.close()


def unknown_not_replayed(binary):
    j=Recovery(binary)
    j.unknown=True
    try:
        j.prepare()
        j.admit('NATURAL-ROUTE Create one distinct Gamma task.',topic=1)
        j.wait(lambda:len(j.creation_outputs)==4)
        j.idle();j.stop()
        j.creation_outputs.clear();j.resume_mode=True;j.inspect_resume=True
        j.start()
        j.admit('Inspect explicit recovery of the original uncertain creation.',topic=1)
        j.wait(lambda:len(j.creation_outputs)==1)
        j.idle()
        assert j.topic_calls==1 and j.row("SELECT phase FROM interaction_creations")==[('uncertain',)]
        assert j.row('SELECT count(*) FROM interaction_enrollments')==[(0,)]
        print('PASS explicit recovery of uncertain topic mutation only inspects retained outcome and sends no second request')
    finally:j.close()


def retired_not_revived(binary):
    j=Recovery(binary)
    try:
        j.prepare();j.cooldown_once=True
        j.admit('/interaction status',topic=1);j.wait(j.cooldown_seen.is_set)
        j.admit('NATURAL-ROUTE Create a distinct Gamma task.',topic=1)
        j.wait(lambda:j.row('SELECT phase FROM interaction_creations')==[('prepared',)])
        j.crash()
        with sqlite3.connect(j.root/'state.sqlite') as db:
            db.execute("INSERT OR REPLACE INTO bot_state(key,value) VALUES('stop_admission_epoch:100','synthetic-retired-generation')")
        j.resume_mode=True;j.reject_resume=True
        j.start()
        j.admit('Attempt explicit recovery of the retired original intent.',topic=1)
        j.wait(lambda:len(j.creation_outputs)==1)
        j.idle()
        assert j.topic_calls==0 and j.row("SELECT phase FROM interaction_creations")==[('retained_unsent',)]
        print('PASS recovery cannot revive a retired original through a fresh request; no topic or worker mutation and no Stop invoked')
    finally:j.close()


if __name__=='__main__':
    binary=str(pathlib.Path(sys.argv[1]).resolve())
    known_unsent(binary)
    known_topic(binary)
    unknown_not_replayed(binary)
    retired_not_revived(binary)
