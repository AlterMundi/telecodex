#!/usr/bin/env python3
"""First browser text/voice needs no Telegram seed or paid bootstrap model."""
import hashlib,json,sqlite3,sys,uuid
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from fastapi.testclient import TestClient
from dashboard.app import create_app
from dashboard.provider import Provider
from conversation_requests_http_io import Requests
from conversation_voice_http_io import audio

class ColdEntry(Requests):
    def prepare(self, history=False):
        self.start()
        for topic in (7,8):
            self.admit('Seed only the owned work context',topic=topic)
            self.wait(lambda:self.has('Initial goal turn delivered.',topic));self.idle()
        self.stop()
        with sqlite3.connect(self.root/'state.sqlite') as db:
            db.row_factory=sqlite3.Row
            assert not db.execute('SELECT 1 FROM sessions WHERE thread_id=1').fetchone()
            original=dict(db.execute('SELECT * FROM sessions WHERE thread_id=7').fetchone())
            original.pop('id');original.update(thread_id=1,codex_thread_id=None,force_fresh_thread=1,busy=0,last_assistant_text=None,session_title='Cold conversation',session_prompt='Discuss with the human while work sessions continue. Use interaction_controller for original input and attributed routing.')
            db.execute('INSERT INTO sessions('+','.join(original)+') VALUES('+','.join('?' for _ in original)+')',list(original.values()))
        script=self.root/'asr.py'
        script.write_text('''import hashlib,json,pathlib,sys,time
root=pathlib.Path(sys.argv[1]);digest=hashlib.sha256(pathlib.Path(sys.argv[-1]).read_bytes()).hexdigest()
with (root/'asr-calls').open('a') as f:f.write(digest+'\\n')
while (root/'hold-asr').exists() and not (root/'release-asr').exists():time.sleep(.02)
print(json.dumps({'text':'Continue requirement17 in this selected work.','final':True}))
''')
        self.config.write_text(self.config.read_text()+self.target_config+'\n[voice]\ntranscriber_label="synthetic/no-inference"\ntimeout_seconds=25\ntranscription_command='+json.dumps(['/usr/bin/python3',str(script),str(self.root)])+'\n')
        if history:
            with sqlite3.connect(self.root/'state.sqlite') as db:
                db.execute('UPDATE sessions SET force_fresh_thread=0 WHERE thread_id=1')
            self.config.write_text(self.config.read_text().replace('auto_attach_latest_history=false','auto_attach_latest_history=true'))
            sessions=self.root/'codex-home'/'sessions';sessions.mkdir(parents=True,exist_ok=True)
            (sessions/'rollout-unrelated.jsonl').write_text(json.dumps({'type':'session_meta','payload':{'id':'unrelated-cwd-history','timestamp':'2026-03-13T09:00:00Z','cwd':str(self.root),'source':'exec'}})+'\n')
        fake=self.root/'codex'
        fake.write_text(fake.read_text().replace("if sys.argv[1:]!=['app-server','proxy']: sys.exit(7)","""if sys.argv[1:3]==['exec','review']:
    import json,pathlib,time
    root=pathlib.Path(__file__).parent
    print(json.dumps({'type':'thread.started','thread_id':'temporary-review-thread'}),flush=True)
    (root/'review-started').touch()
    while not (root/'release-review').exists():time.sleep(.02)
    print(json.dumps({'type':'item.completed','item':{'type':'agent_message','text':'Synthetic review completed.'}}),flush=True)
    sys.exit(0)
if sys.argv[1:]!=['app-server','proxy']: sys.exit(7)"""))
        self.start();self.wait_control_socket(self.socket);self.routing=True
        self.messages.clear();self.telegram_methods.clear()

class Browser:
    def __init__(self,j,companion=False):
        config={'receiver_config':str(j.config),'state_dir':str(j.root/'dashboard'),'origin':'https://example.test','bot_id':123}
        if companion:config['companion']={'enabled':True,'grants_dir':str(j.root/'companion-grants')}
        self.provider=Provider(config)
        self.client=TestClient(create_app(config,self.provider,lambda raw,bot:int(raw)),base_url=config['origin'])
        if companion:
            from dashboard.companion import Grants
            destination=j.root/'device.json'
            Grants(config,self.provider).enroll('Synthetic visual companion',destination,3600)
            device=json.loads(destination.read_text())
            self.device_headers={'origin':config['origin'],'authorization':device['authorization'],'x-dashboard-csrf':device['csrf']}
        self.headers={'origin':config['origin']}
        r=self.client.post('/api/session',json={'init_data':'100'},headers=self.headers);assert r.status_code==200,r.text
        self.headers['authorization']='Bearer '+r.json()['session_token']
        r=self.client.get('/api/interaction/inputs',headers=self.headers);assert r.status_code==200,r.text
        self.binding=r.json()['binding'];self.headers['x-dashboard-csrf']=r.json()['csrf']
    def text(self,text,reply=None):
        body={'id':str(uuid.uuid4()),'binding':self.binding,'text':text}
        if reply:body['reply']=reply
        r=self.client.post('/api/interaction/inputs',json=body,headers=self.headers);assert r.status_code==200,r.text
        return body['id'],r.json()
    def status(self,ident):
        r=self.client.get('/api/interaction/inputs/'+ident,headers=self.headers);assert r.status_code==200,r.text;return r.json()
    def voice(self,reply=None):
        clip=audio();ident=str(uuid.uuid4())
        h={**self.headers,'content-type':'audio/wav','x-input-id':ident,'x-input-binding':self.binding,'x-audio-sha256':hashlib.sha256(clip).hexdigest()}
        if reply:h.update({'x-reply-source':('context:' if reply.get('context_only') else '')+reply['source'],'x-reply-revision':reply['revision']})
        r=self.client.post('/api/interaction/voice',content=clip,headers=h);assert r.status_code==200,r.text
        return ident,r.json()
    def get(self,path):
        r=self.client.get(path,headers=self.headers);assert r.status_code==200,r.text;return r.json()

def first_text(binary):
    j=ColdEntry(binary);b=None
    try:
        j.prepare();b=Browser(j);binding=b.binding
        initial=j.row('SELECT codex_thread_id FROM sessions WHERE thread_id=1');assert initial==[(None,)]
        assert b.get('/api/interaction')['binding']==binding and not j.packets
        original,_=b.text('#Alpha Start requirement13 without waking the front conversation.')
        j.wait(lambda:j.instructions(7) and j.instructions(7)[0][1]=='accepted')
        first=b.status(original);assert first['phase']=='routed' and not j.packets
        assert j.row('SELECT codex_thread_id FROM sessions WHERE thread_id=1')==[(None,)]
        worker=next(send for thread,send in j.clients.values() if thread=='native-topic-7')
        worker({'id':'canonical-cold-question','method':'item/tool/requestUserInput','params':{'threadId':'native-topic-7','turnId':j.workers['native-topic-7'],'itemId':'first-choice','isBlocking':True,'questions':[{'id':'scope','header':'Scope','question':'Choose scope while the front context does not exist.','options':[{'label':'Selected scope','description':'Only this request'}]}]}})
        j.wait(lambda:j.row("SELECT count(*) FROM human_interactions WHERE state='pending'")==[(1,)])
        question=b.get('/api/interaction/requests')['items'][0];question=b.get('/api/interaction/requests/'+question['id']);assert question['actionable']
        (j.root/'hold-asr').touch();clip,_=b.voice(first['continuation'])
        j.wait(lambda:(j.root/'asr-calls').exists())
        j.hold_new_native=True
        front,_=b.text('Discuss the plan while Alpha continues independently.')
        assert j.new_native_seen.wait(timeout=8)
        correction,_=b.text('A later written correction must reach this first turn.')
        j.new_native_release.set()
        j.wait(lambda:b.status(front)['phase']=='accepted' and b.status(correction)['phase']=='accepted')
        assert len(j.packets)==1 and 'later written correction' in str(j.packets[0])
        assert b.get('/api/interaction')['binding']==binding
        assert b.status(clip)['phase']=='preparing' and not b.status(clip)['retired']
        assert b.status(original)['continuation']==first['continuation'],'Own initial native assignment changed the explicit reply source'
        current=b.get('/api/interaction/requests/'+question['id']);assert current['actionable'] and current['revision']==question['revision']
        answer=b.client.post('/api/interaction/requests/'+question['id'],json={'binding':binding,'revision':question['revision'],'action':{'kind':'option','index':0}},headers=b.headers);assert answer.status_code==200,answer.text
        j.wait(lambda:bool(j.native_answers))
        (j.root/'release-asr').touch()
        j.wait(lambda:len(j.instructions(7))==2 and j.instructions(7)[1][1]=='accepted')
        assert b.status(clip)['routing']['phase']=='accepted' and len(j.packets)==1
        assert not j.messages
        print('PASS actual cold HTTP/text entry: direct worker with no front context, pending worker question survives first native creation, held ASR focus and later preparation-time steer, stable binding/reply source, canonical return; zero models.')
    finally:
        if b:b.client.close()
        j.close()

def first_voice(binary):
    j=ColdEntry(binary);b=None
    try:
        j.prepare(history=True);b=Browser(j);clip,_=b.voice()
        j.wait(lambda:b.status(clip)['phase']=='accepted')
        trace=b.get('/api/interaction/inputs/'+clip+'/trace')
        assert trace['voice'] and trace['text']=='Continue requirement17 in this selected work.'
        assert trace['routes']==[] and trace['read_only']
        assert len((j.root/'asr-calls').read_text().splitlines())==1,'Trace inspection repeated ASR'
        assert len(j.packets)==1 and '[Committed voice input]' in str(j.packets[0])
        assert j.row('SELECT codex_thread_id FROM sessions WHERE thread_id=1')[0][0]!='unrelated-cwd-history'
        assert b.get('/api/interaction')['binding']==b.binding
        assert not j.messages
        print('PASS first committed browser voice creates its own attributed native conversation, without a Telegram seed or bootstrap model; zero models.')
    finally:
        if b:b.client.close()
        j.close()

def temporary_review(binary):
    j=ColdEntry(binary);b=None
    try:
        j.prepare();b=Browser(j);(j.root/'hold-asr').touch();clip,_=b.voice()
        j.wait(lambda:(j.root/'asr-calls').exists())
        j.admit('/review --uncommitted',topic=1)
        j.wait(lambda:(j.root/'review-started').exists())
        j.wait(lambda:j.row("SELECT native_thread_id FROM turns WHERE native_thread_id='temporary-review-thread'")!=[])
        assert j.row('SELECT codex_thread_id FROM sessions WHERE thread_id=1')==[(None,)]
        assert b.status(clip)['phase']=='preparing' and not b.status(clip)['retired']
        assert b.get('/api/interaction')['binding']==b.binding
        (j.root/'release-review').touch();j.wait(lambda:j.has('Synthetic review completed.',1));j.idle();j.messages.clear()
        (j.root/'release-asr').touch();j.wait(lambda:b.status(clip)['phase']=='accepted')
        assert j.row('SELECT codex_thread_id FROM sessions WHERE thread_id=1')[0][0]!='temporary-review-thread'
        assert len(j.packets)==1,(j.packets,j.messages)
        assert not j.messages,j.messages
        print('PASS temporary review thread does not bind the cold entry or carry pending voice; the real human voice establishes its own context; zero models.')
    finally:
        (j.root/'release-review').touch()
        if b:b.client.close()
        j.close()

def sonic_registry(binary):
    """The sound score shares actual routing membership, without native reads."""
    j=ColdEntry(binary);b=None
    try:
        j.prepare();j.stop()
        j.config.write_text(j.config.read_text().replace('[interaction]\n','[interaction]\ndiscover_owned_sessions=true\n'))
        with sqlite3.connect(j.root/'state.sqlite') as db:
            db.row_factory=sqlite3.Row
            row=dict(db.execute('SELECT * FROM sessions WHERE thread_id=7').fetchone())
            row.pop('id');row.update(thread_id=19,codex_thread_id='synthetic-discovered-binding',session_title='Discovered19',busy=0)
            db.execute('INSERT INTO sessions('+','.join(row)+') VALUES('+','.join('?'for _ in row)+')',list(row.values()))
        j.start();j.wait_control_socket(j.socket);b=Browser(j)
        choices=b.get('/api/interaction/inputs')['routing_choices']
        expected={'100:1',*(f"100:{int(v['selector'][1:])}"for v in choices)}
        sound=b.get('/api/interaction/sonic')
        assert {v['id']for v in sound['sources']}==expected
        discovered=next(v for v in sound['sources']if v['id']=='100:19')
        assert discovered['label']=='Discovered19' and not discovered['fresh']
        assert not sound['configured'] and not sound['connected']
        with sqlite3.connect(j.root/'state.sqlite') as db:
            db.execute('UPDATE sessions SET force_fresh_thread=1 WHERE thread_id=19')
        assert '100:19'not in {v['id']for v in b.get('/api/interaction/sonic')['sources']}
        assert not j.packets and b.provider.native is None
        assert j.row('SELECT codex_thread_id FROM sessions WHERE thread_id=1')==[(None,)]
        print('PASS actual control/SQLite/HTTP sound membership matches discovered routing choices, retires replaced binding, and starts no native observer or cold integrator; synthetic native peers, zero models.')
    finally:
        if b:b.client.close()
        j.close()

if __name__=='__main__':
    binary=str(Path(sys.argv[1]).resolve());first_text(binary);first_voice(binary);temporary_review(binary);sonic_registry(binary)
