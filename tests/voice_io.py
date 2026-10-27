#!/usr/bin/env python3
"""Committed voice through real decoder/process/HTTP/native/SQLite boundaries.

Audio is a generated tone or silence; ASR/model outputs and identities are
synthetic. These tests verify the adapter, not actual speech recognition.
"""
import hashlib
import io
import json
import math
import pathlib
import subprocess
import sqlite3
import struct
import sys
import wave
from semantic_routing_io import Semantic


class Voice(Semantic):
    def __init__(self, binary):
        super().__init__(binary)
        self.extra_env = {'SYSTEMD_EXEC_PID':'synthetic-service'}
        self.media_files = {}
        self.transcripts = {}
        self.voice_configured = False
        self.transcriber = self.root/'asr-fixture.py'
        self.transcriber.write_text('''import hashlib,json,os,pathlib,subprocess,sys,time
source=pathlib.Path(sys.argv[-1]); digest=hashlib.sha256(source.read_bytes()).hexdigest()
root=pathlib.Path(sys.argv[1]); item=json.loads((root/'transcripts.json').read_text())[digest]
with (root/'asr-calls').open('a') as calls: calls.write(digest+'\\n')
with (root/'asr-paths').open('a') as paths: paths.write(json.dumps([digest,str(source)])+'\\n')
if item.get('gate'):
    child=subprocess.Popen(['/usr/bin/python3','-c','import time;time.sleep(60)'],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
    (root/('asr-processes-'+digest)).write_text(json.dumps([os.getpid(),child.pid]))
    until=time.monotonic()+20
    while not pathlib.Path(item['gate']).exists() and time.monotonic()<until: time.sleep(.02)
print(json.dumps({'text':item['text'],**item.get('flags',{})}))
''')

        fake=self.root/'codex'
        source=fake.read_text().replace("    print('Logged in using synthetic local auth')", "    import pathlib,time\n    root=pathlib.Path(__file__).parent\n    if (root/'auth-hold').exists():\n        (root/'auth-seen').touch()\n        until=time.monotonic()+15\n        while not (root/'auth-release').exists() and time.monotonic()<until: time.sleep(.02)\n    print('Logged in using synthetic local auth')")
        fake.write_text(source)

    def start(self):
        if self.routed and not self.voice_configured:
            with self.config.open('a') as f:
                f.write('\n[voice]\ntranscriber_label="synthetic/asr-no-inference"\ntimeout_seconds=10\ntranscription_command='+json.dumps(['/usr/bin/python3',str(self.transcriber),str(self.root)])+'\n')
            self.voice_configured = True
        super().start()

    def voice(self, text, *, gate=None, silence=False, topic=1, user=100, flags=None):
        with self.lock:
            self.sequence += 1
            update = self.sequence
            file_id = 'clip-'+str(update)+'.wav'
            stream=io.BytesIO()
            with wave.open(stream,'wb') as wav:
                wav.setnchannels(1);wav.setsampwidth(2);wav.setframerate(16000)
                wav.writeframes(b''.join(struct.pack('<h',0 if silence else int(5000*math.sin(2*math.pi*(400+update)*i/16000))) for i in range(1600)))
            data=stream.getvalue();self.media_files[file_id]=data
            digest=hashlib.sha256(data).hexdigest()
            self.transcripts[digest]={'text':text,'gate':str(gate) if gate else None,'flags':flags or {}}
            (self.root/'transcripts.json').write_text(json.dumps(self.transcripts))
            payload={'update_id':update,'message':{'message_id':update,'message_thread_id':topic,'from':{'id':user,'is_bot':False,'first_name':'Fixture'},'chat':{'id':100,'type':'private'},'voice':{'file_id':file_id,'mime_type':'audio/wav'}}}
            self.updates.append(payload)
        return update,payload,digest

    def calls(self):
        p=self.root/'asr-calls'
        return p.read_text().splitlines() if p.exists() else []


def semantic_voice_and_responsive_corrections(binary):
    j=Voice(binary)
    try:
        j.prepare()
        gate=j.root/'release-asr'
        original,payload,digest=j.voice('NATURAL-ROUTE Use requirement 10 for Alpha and Beta.',gate=gate,flags={'streaming_model':'gemini-3.5-transcribe-live'})
        j.wait(lambda:digest in j.calls())
        source_paths=j.root/'asr-paths'
        j.wait(lambda:source_paths.exists() and any(json.loads(line)[0]==digest for line in source_paths.read_text().splitlines()))
        downloaded=pathlib.Path(next(json.loads(line)[1] for line in source_paths.read_text().splitlines() if json.loads(line)[0]==digest))
        assert downloaded.exists() and hashlib.sha256(downloaded.read_bytes()).hexdigest()==digest
        assert not j.instructions(7) and not j.instructions(8), 'Partial/preparing ASR admitted work'
        correction=j.admit('#Alpha Change the requirement to 17.',topic=1)
        j.wait(lambda:len(j.instructions(7))==1 and j.instructions(7)[0][1]=='accepted')
        assert not gate.exists(), 'Control waited for ASR'
        gate.touch()
        j.wait(lambda:len(j.outputs)==5)
        j.wait(lambda:len(j.instructions(7))==2 and all(i[1]=='accepted' for i in j.instructions(7)) and len(j.instructions(8))==1 and j.instructions(8)[0][1]=='accepted')
        with j.lock:j.updates.append(payload)
        j.wait(lambda:j.row('SELECT phase FROM interaction_voice_inputs WHERE update_id=?',(original,))==[('committed',)])
        assert j.calls().count(digest)==1
        retained=json.loads(j.row('SELECT payload_json FROM incoming_updates WHERE update_id=?',(original,))[0][0])
        assert not retained['message'].get('text') and retained['message']['voice']['file_id']==payload['message']['voice']['file_id']
        source=json.loads(j.row('SELECT doc FROM interaction_sources WHERE event_id=?',(str(original),))[0][0])
        assert source['transcription']['audio_sha256']==digest and source['text'].startswith('NATURAL-ROUTE')
        assert source['transcription']['audio_millis']==100
        assert source['transcription']['actual_transcriber']=='gemini-3.5-transcribe-live'
        assert source['transcription']['configured_transcriber']=='synthetic/asr-no-inference'
        j.wait(lambda:not downloaded.exists())
        packet=j.packets['native-topic-7'][-1]['input'][0]['text']
        provenance=json.loads(packet.split('Transport provenance: ',1)[1].split('\nCurrent instruction:',1)[0])
        assert provenance['transcription']['audio_sha256']==digest
        assert provenance['transcription']['actual_transcriber']=='gemini-3.5-transcribe-live'
        assert provenance['input_ref']==original and isinstance(provenance['capture_order'],int)
        assert 'capture_order records source chronology' in packet
        assert str(correction) in packet and str(original) in packet
        explicit,_,explicit_digest=j.voice('#Alpha Incorporate the explicit voice correction19.')
        j.wait(lambda:len(j.instructions(7))==3 and all(i[1]=='accepted' for i in j.instructions(7)))
        assert j.row('SELECT busy FROM sessions WHERE thread_id=1')==[(1,)], 'Voice correction waited for the front dialogue'
        assert j.calls().count(explicit_digest)==1 and len(j.instructions(8))==1
        assert 'explicit voice correction19' in j.packets['native-topic-7'][-1]['input'][0]['text']
        j.entry_send({'method':'turn/completed','params':{'threadId':j.entry,'turn':{'id':j.entry_turn,'status':'completed'}}})
        j.answer(8,'Beta voice result.');j.answer(7,'Alpha retained correction17.')
        j.wait(lambda:j.has('Beta voice result.',1) and j.has('Alpha retained correction17.',1))
        j.idle()
        print('PASS final voice identity/provenance, semantic two-target dispatch, slow-ASR text correction, original chronology, duplicate/repeated-selection dedup and attributed reverse results; zero model calls')
    finally:j.close()


def silence_and_restart(binary):
    j=Voice(binary)
    try:
        j.prepare()
        silent,_,_=j.voice('This hallucinated sentence must never reach the model.',silence=True)
        j.wait(lambda:j.row('SELECT status FROM incoming_updates WHERE update_id=?',(silent,))==[('undetermined',)])
        assert not j.calls() and not j.instructions(7) and not j.instructions(8)
        for flags in ({'partial':True},{'final':False}):
            partial,_,_=j.voice('#Alpha This partial hypothesis must not dispatch.',flags=flags)
            j.wait(lambda:j.row('SELECT status FROM incoming_updates WHERE update_id=?',(partial,))==[('undetermined',)])
            assert not j.instructions(7) and not j.instructions(8)
        gate=j.root/'asr-held'
        original,_,digest=j.voice('#Alpha Retained across disconnect.',gate=gate)
        j.wait(lambda:digest in j.calls())
        j.stop();j.start()
        j.wait(lambda:j.row('SELECT phase FROM interaction_voice_inputs WHERE update_id=?',(original,))==[('uncertain',)])
        gate.touch()
        assert j.row('SELECT status FROM incoming_updates WHERE update_id=?',(original,))==[('undetermined',)]
        assert j.calls().count(digest)==1 and not j.instructions(7)
        print('PASS digital silence before ASR, explicit partial output rejected, interrupted preparation retained, restart without transcription/work replay')
    finally:j.close()



def late_interrupt_keeps_newer_execution(binary):
    j=Voice(binary)
    try:
        j.prepare()
        j.admit('#Alpha Original owned work.',topic=1)
        j.wait(lambda:len(j.instructions(7))==1 and j.instructions(7)[0][1]=='accepted')
        old=j.runs['native-topic-7']
        gate=j.root/'late-control'
        update,_,digest=j.voice('#Alpha /interrupt',gate=gate)
        j.wait(lambda:digest in j.calls())
        j.answer(7,'Original work completed.')
        j.wait(lambda:j.row('SELECT busy FROM sessions WHERE thread_id=7')==[(0,)])
        j.admit('#Alpha Newer independent work.',topic=1)
        j.wait(lambda:len(j.instructions(7))==2 and j.instructions(7)[1][1]=='accepted')
        new=j.runs['native-topic-7']
        assert old!=new
        gate.touch()
        j.wait(lambda:j.row('SELECT status FROM incoming_updates WHERE update_id=?',(update,))==[('handled',)])
        assert not [m for m in j.rpc if m.get('method')=='turn/interrupt' and m['params']['turnId']==new]
        assert j.row('SELECT busy FROM sessions WHERE thread_id=7')==[(1,)]
        j.answer(7,'Newer work survives delayed control.',covered=[j.instructions(7)[1][0]])
        j.wait(lambda:j.has('Newer work survives delayed control.',1))
        j.idle()
        print('PASS delayed voice interrupt cannot cancel a newer worker execution')
    finally:j.close()


def post_commit_authority_change(binary):
    for change in ('binding','owner'):
        j=Voice(binary)
        try:
            j.prepare()
            (j.root/'auth-hold').touch()
            update,_,_=j.voice('Continue this in the integrating conversation.')
            j.wait(lambda:(j.root/'auth-seen').exists())
            assert j.row('SELECT phase FROM interaction_voice_inputs WHERE update_id=?',(update,))==[('committed',)]
            with sqlite3.connect(j.root/'state.sqlite') as db:
                if change=='binding':db.execute("UPDATE sessions SET codex_thread_id='replacement' WHERE thread_id=1")
                else:db.execute('UPDATE users SET allowed=0 WHERE tg_user_id=100')
            (j.root/'auth-release').touch()
            j.wait(lambda:j.row('SELECT status FROM incoming_updates WHERE update_id=?',(update,))==[('undetermined',)])
            assert not [m for m in j.rpc if m.get('method')=='turn/start' and m['params']['threadId'] not in ('native-topic-7','native-topic-8')]
            assert j.row('SELECT phase FROM interaction_voice_inputs WHERE update_id=?',(update,))==[('retired',)]
        finally:j.close()
    print('PASS binding replacement and owner revocation during post-commit authentication await admit no native input')


def restart_reaps_owned_asr_group(binary):
    j=Voice(binary)
    try:
        j.prepare()
        gate=j.root/'restart-held'
        update,_,digest=j.voice('#Alpha Must not replay after restart.',gate=gate)
        process_file=j.root/('asr-processes-'+digest)
        j.wait(lambda:process_file.exists())
        pids=json.loads(process_file.read_text())
        j.admit('/restart_bot',topic=1)
        assert j.process.wait(timeout=10)==0
        _,error=j.process.communicate(timeout=2)
        assert 'synthetic-question-token' not in error
        j.process=None
        def terminated(pid):
            result=subprocess.run(['ps','-o','stat=','-p',str(pid)],capture_output=True,text=True)
            return not result.stdout.strip() or result.stdout.strip().startswith('Z')
        j.wait(lambda:all(terminated(pid) for pid in pids))
        j.start()
        j.wait(lambda:j.row('SELECT phase FROM interaction_voice_inputs WHERE update_id=?',(update,))==[('uncertain',)])
        gate.touch()
        assert j.calls().count(digest)==1 and not j.instructions(7)
        assert j.row('SELECT status FROM incoming_updates WHERE update_id=?',(update,))==[('undetermined',)]
        print('PASS explicit listener restart terminates owned ASR and descendant before exit; no transcription/work replay')
    finally:j.close()



def overlapping_first_voices_preserve_entry_generation(binary):
    j=Voice(binary)
    try:
        j.prepare()
        a=j.root/'first-voice';b=j.root/'later-voice'
        first,_,first_digest=j.voice('NATURAL-ROUTE Start Alpha and Beta.',gate=a)
        j.wait(lambda:first_digest in j.calls())
        second,_,second_digest=j.voice('#Alpha Add the later voice correction.',gate=b)
        j.wait(lambda:second_digest in j.calls())
        assert j.row('SELECT native FROM interaction_voice_inputs ORDER BY update_id')==[(None,),(None,)]
        a.touch()
        j.wait(lambda:len(j.outputs)==5 and all(len(j.instructions(t))==1 and j.instructions(t)[0][1]=='accepted' for t in (7,8)))
        b.touch()
        j.wait(lambda:len(j.instructions(7))==2 and j.instructions(7)[1][1]=='accepted')
        assert j.row('SELECT phase FROM interaction_voice_inputs ORDER BY update_id')==[('committed',),('committed',)]
        assert 'later voice correction' in json.loads(j.instructions(7)[1][2])['text']
        assert j.calls().count(first_digest)==j.calls().count(second_digest)==1
        j.entry_send({'method':'turn/completed','params':{'threadId':j.entry,'turn':{'id':j.entry_turn,'status':'completed'}}})
        j.answer(8,'Beta first-voice result.');j.answer(7,'Alpha received both first voices.')
        j.wait(lambda:j.has('Beta first-voice result.',1) and j.has('Alpha received both first voices.',1))
        j.idle()
        print('PASS overlapping first voices share their own initial native binding and the later correction reaches its selected worker')
    finally:j.close()


def full_download_storage_preserves_input_and_text_control(binary):
    import tomllib
    j=Voice(binary)
    try:
        j.prepare()
        database=tomllib.loads(j.config.read_text())['db_path']
        directory=pathlib.Path(str(database)+'.telegram-voice');directory.mkdir(mode=0o700,exist_ok=True)
        protected=directory/'unconfirmed-fixture'
        with protected.open('wb') as stream:stream.truncate(256*1024*1024)
        protected.chmod(0o600)
        update,payload,digest=j.voice('#Alpha This recording must not be downloaded or transcribed.')
        j.wait(lambda:j.row('SELECT phase FROM interaction_voice_inputs WHERE update_id=?',(update,))==[('uncertain',)])
        assert not j.calls() and not j.instructions(7)
        retained=json.loads(j.row('SELECT payload_json FROM incoming_updates WHERE update_id=?',(update,))[0][0])
        assert retained['message']['voice']['file_id']==payload['message']['voice']['file_id']
        assert protected.stat().st_size==256*1024*1024
        assert {p.name for p in directory.iterdir()}=={'admission.lock','unconfirmed-fixture'}
        j.admit('#Alpha Text remains available while audio storage is full.',topic=1)
        j.wait(lambda:len(j.instructions(7))==1 and j.instructions(7)[0][1]=='accepted')
        j.answer(7,'Text completed independently of full audio storage.')
        j.wait(lambda:j.has('Text completed independently of full audio storage.',1))
        assert not j.calls();j.idle()
        print('PASS full protected Telegram voice storage retains original identity without ASR/download while text dispatch/results remain available')
    finally:j.close()


if __name__=='__main__':
    binary=str(pathlib.Path(sys.argv[1]).resolve())
    full_download_storage_preserves_input_and_text_control(binary)
    semantic_voice_and_responsive_corrections(binary)
    silence_and_restart(binary)
    late_interrupt_keeps_newer_execution(binary)
    post_commit_authority_change(binary)
    restart_reaps_owned_asr_group(binary)
    overlapping_first_voices_preserve_entry_generation(binary)
