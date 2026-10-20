#!/usr/bin/env python3
"""Live CLI fallback through real Unix/HTTP/WebSocket/SQLite; synthetic model peers.
Existing entry/workers have no persisted interaction tools. No real model/capture.
"""
import json
from pathlib import Path
import shutil
import subprocess
import sys
from session_requests_io import Sessions

class Warm(Sessions):
    def __init__(self,binary,pack=False):
        self.expression_pack_text=''
        super().__init__(binary)
        self.socket=self.root/'control.sock'
        self.config.write_text(self.config.read_text().replace('[telegram]',
            f'[session_control]\nsocket_path="{self.socket}"\nhuman_id=100\n[telegram]'))
        if pack:
            self.expression_pack_text='''
[interaction.expression_pack]
schema="daimon.ascii-expression/v1"
id="test-fields"
revision="1"
authorship="Synthetic fixture, not another being"
[interaction.expression_pack.defaults.cli]
expression="ripples"
variant="field"
columns=80
rows=24
[interaction.expression_pack.defaults.telegram]
expression="ripples"
variant="field"
columns=28
rows=10
[interaction.expression_pack.defaults.app]
expression="ripples"
variant="field"
columns=40
rows=12
[interaction.expression_pack.expressions.ripples]
alt="A field of ripples"
[interaction.expression_pack.expressions.ripples.variants.field]
frames=["~ : ~", " :~: "]
frame_ms=100
'''
    def start(self):
        if self.routed and self.expression_pack_text and '[interaction.expression_pack]' not in self.config.read_text():
            self.config.write_text(self.config.read_text()+self.expression_pack_text)
        return super().start()
    def access(self,topic):
        local=self.row("SELECT t.id FROM turns t JOIN sessions s ON s.id=t.session_id WHERE s.thread_id=? AND t.status='running' ORDER BY t.id DESC LIMIT 1",(topic,))[0][0]
        files=list(self.root.glob(f'.telecodex/turns/{local}-*/controller-access.json'))
        assert len(files)==1
        return files[0]
    def cli(self,path,operation,arguments=None,*,ok=True):
        token=json.loads(path.read_text())['token']
        result=subprocess.run([self.binary,'--controller',str(path),json.dumps({'operation':operation,'arguments':arguments or {}})],capture_output=True,text=True,timeout=30)
        assert token not in result.stdout and token not in result.stderr
        assert (result.returncode==0)==ok,result.stderr
        if not ok:return result
        outer=json.loads(result.stdout);assert outer['ok'] and outer['result']['success'],outer
        return json.loads(outer['result']['contentItems'][0]['text'])

def qualify(binary,pack=False):
    j=Warm(binary,pack=pack)
    try:
        j.prepare(seed_entry=True)
        j.allow_native_launch=True
        j.admit('Ordinary work already in progress.',topic=7)
        j.wait(lambda:'native-topic-7' in j.runs)
        assert 'Controller access for this execution' not in j.packets['native-topic-7'][-1]['input'][0]['text']
        # All native bindings were seeded BEFORE registering dynamic tools.
        before=sum(m.get('method')=='turn/start' for m in j.rpc)
        j.admit('Discuss the existing work, without yet dispatching.',topic=1)
        j.wait(lambda:'native-topic-9' in j.runs)
        entry=j.access(1)
        caps=j.cli(entry,'capabilities');assert 'dispatch' in caps['operations']
        pose={'gesture':'play','rest':'ripples','pack':'test-fields','revision':'1'} if pack else {'gesture':'wave','rest':'calm'}
        if pack:pose['catalog']=caps['operations']['companion_hint']['arguments']['catalog']
        worker_pose=pose if pack else {'gesture':'nod','rest':'delighted'}
        if pack:
            assert caps['operations']['companion_hint']['arguments']['rest']==['ripples']
            j.cli(entry,'companion_hint',{**pose,'revision':'old'},ok=False)
        assert 'companion_hint' in caps['operations']
        assert j.cli(entry,'companion_hint',pose)['new_registration']
        assert not j.cli(entry,'companion_hint',pose)['new_registration']
        j.cli(entry,'companion_hint',{'gesture':'script','rest':'calm'},ok=False)
        j.cli(entry,'companion_hint',{**pose,'gesture':'none'},ok=False)
        original=j.cli(entry,'inputs')['inputs'][-1]['input_ref']
        assert sum(m.get('method')=='turn/start' for m in j.rpc)==before+1
        j.cli(entry,'dispatch',{'input_ref':original,'destinations':[{'selector':'Alpha','relation':'new','requests':[]}]})
        j.wait(lambda:j.instructions(7) and j.instructions(7)[0][1]=='accepted')
        assert 'Controller access for this execution' in j.packets['native-topic-7'][-1]['input'][0]['text']
        worker=j.access(7)
        symlink=j.root/'linked-access.json';symlink.symlink_to(worker)
        denied=subprocess.run([j.binary,'--controller',str(symlink),json.dumps({'operation':'capabilities','arguments':{}})],capture_output=True,text=True,timeout=10)
        assert denied.returncode!=0
        worker.chmod(0o644);j.cli(worker,'capabilities',ok=False);worker.chmod(0o600)
        caps=j.cli(worker,'capabilities');assert 'session_request' in caps['operations'] and 'dispatch' not in caps['operations']
        assert 'companion_hint' in caps['operations']
        assert j.cli(worker,'companion_hint',worker_pose)['new_registration']
        # An existing busy worker invokes the same nonblocking dependency handler.
        parent=j.instructions(7)[0][0]
        args={'parent_instruction':parent,'request_id':'warm-dependency','selector':'Beta','text':'Provide the selected evidence for this task.'}
        result=j.cli(worker,'session_request',args)
        j.wait(lambda:j.instructions(8) and j.instructions(8)[0][1]=='accepted')
        repeat=j.cli(worker,'session_request',args);assert not repeat['new_admission'] and repeat['request']['id']==result['request']['id']
        j.cli(worker,'dispatch',{'input_ref':original,'destinations':[]},ok=False)
        j.cli(entry,'global_stop',ok=False)
        # Tokens never appear in terminal output; forged tokens cannot invoke tools.
        forged=j.root/'forged-access.json';value=json.loads(worker.read_text());value['token']='0'*64
        forged.write_text(json.dumps(value));forged.chmod(0o600)
        j.cli(forged,'capabilities',ok=False)
        # A changed logical/native binding closes access even while the turn is live.
        with __import__('sqlite3').connect(j.root/'state.sqlite') as db:
            db.execute("UPDATE sessions SET codex_thread_id='changed-test-binding' WHERE thread_id=7")
        j.cli(worker,'capabilities',ok=False)
        with __import__('sqlite3').connect(j.root/'state.sqlite') as db:
            db.execute("UPDATE sessions SET codex_thread_id='native-topic-7' WHERE thread_id=7")
        stale=j.root/'expired-access.json';shutil.copyfile(worker,stale);stale.chmod(0o600)
        j.answer(8,'The selected helper evidence.')
        j.wait(lambda:len(j.instructions(7))==2 and j.instructions(7)[1][1]=='accepted')
        j.answer(7,'Work incorporating the helper evidence.')
        j.wait(lambda:j.row("SELECT count(*) FROM interaction_deliveries WHERE phase!='delivered'")==[(0,)])
        j.wait(lambda:not worker.exists())
        j.cli(stale,'capabilities',ok=False)
        sources=j.cli(entry,'conversation')['presentation']['items']
        assert any(s.get('companion_hint')==worker_pose for s in sources),sources
        thread='native-topic-9';turn=j.runs[thread]
        j.broadcast(thread,'item/completed',{'turnId':turn,'item':{'id':'warm-entry-final','type':'agentMessage','phase':'final_answer','text':'The work returned.'}})
        j.broadcast(thread,'turn/completed',{'turn':{'id':turn,'status':'completed'}})
        j.idle();j.wait(lambda:not entry.exists())
        # Ordinary direct inputs are not enrolled just because their topic is known.
        prior_packets=len(j.packets['native-topic-8'])
        j.admit('Ordinary direct work.',topic=8)
        j.wait(lambda:len(j.packets['native-topic-8'])>prior_packets)
        j.wait(lambda:j.row("SELECT count(*) FROM turns t JOIN sessions s ON s.id=t.session_id WHERE s.thread_id=8 AND t.status='running'")==[(1,)])
        # Dormant access may exist locally, but ordinary direct input is unchanged.
        packet=j.packets['native-topic-8'][-1]
        assert not any('Controller access for this execution' in item.get('text','') for item in packet['input'])
        runtime=packet.get('additionalContext',{}).get('telecodex.runtime',{}).get('value','')
        assert 'controller fallback' not in runtime and 'scoped live controller' not in runtime
        j.broadcast('native-topic-8','turn/completed',{'turn':{'id':j.runs['native-topic-8'],'status':'completed'}})
        j.idle()
        assert len(j.instructions(8))==1
        print('PASS older entry+workers, live same-controller route/dependency, idempotent return, role/binding/expiry denial, direct-topic independence')
    finally:j.close()

if __name__=='__main__':
    qualify(str(Path(sys.argv[1]).resolve()))
    qualify(str(Path(sys.argv[1]).resolve()),pack=True)
