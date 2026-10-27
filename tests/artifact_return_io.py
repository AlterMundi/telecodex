#!/usr/bin/env python3
"""Routed native output -> immutable files -> authorized HTTP bytes; zero models."""
import base64
import fcntl
import hashlib
import json
from pathlib import Path
import re
import sys
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from fastapi.testclient import TestClient
from dashboard.app import create_app
from dashboard.companion import Grants
from dashboard.provider import Provider
from conversation_routing_io import CanonicalRouting


class ArtifactRouting(CanonicalRouting):
    def __init__(self,binary):
        self.expected={}
        super().__init__(binary)
    def handle_extra_rpc(self,message,send):
        p=message.get('params',{})
        if self.routing and message.get('method')=='turn/start' and p.get('threadId')=='native-topic-7':
            context=''.join(v['value'] for k,v in sorted(p['additionalContext'].items()) if k=='telecodex.runtime' or k.startswith('telecodex.runtime.'))
            out=Path(re.search(r'\n([^\n]+/out)\n',context).group(1))
            self.artifact_out=out
            self.expected={'alpha.txt':b'Exact native generated requirement17\n',
                'image.png':base64.b64decode('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+jGQAAAABJRU5ErkJggg==')}
            for name,data in self.expected.items():(out/name).write_bytes(data)
            private=self.root/'private-source';private.write_bytes(b'PRIVATE UNRELATED BYTES')
            (out/'escape.txt').symlink_to(private)
        return super().handle_extra_rpc(message,send)


def prepare(j,projection=False):
    j.start()
    for topic in (7,8,1):
        j.admit('Seed owned native context',topic=topic)
        j.wait(lambda:j.has('Initial goal turn delivered.',topic));j.idle()
    j.stop()
    if projection:j.config.write_text(j.config.read_text().replace('[interaction]\n','[interaction]\ntelegram_projection=true\n'))
    retained=j.root/'retained';retained.mkdir(mode=0o700)
    j.config.write_text(j.config.read_text()+f'\n[interaction.artifact_store]\ndirectory={json.dumps(str(retained))}\nmax_file_bytes=2048\nmax_total_bytes=8192\n'+j.target_config)
    j.start();j.wait_control_socket(j.socket);j.routing=True
    binding=j.read('presentation','list','0','0')['result']['binding']
    return retained,binding


def files_before_execution_ends(binary):
    j=ArtifactRouting(str(Path(binary).resolve()))
    try:
        _,binding=prepare(j)
        j.submit(binding,'#Alpha Return files while keeping the native execution active.')
        j.wait(lambda:j.instructions(7) and j.instructions(7)[0][1]=='accepted')
        native=j.row('SELECT codex_thread_id FROM sessions WHERE thread_id=7')[0][0]
        j.answer_worker(7,'First response with files, work continues.',item='checkpoint-one',finish=False)
        j.wait(lambda:len(j.read('presentation','artifacts','list')['result']['items'])==3)
        assert j.row("SELECT COUNT(*) FROM turns WHERE status='running'")[0][0]>0
        first=j.read('presentation','artifacts','list')['result']['items']
        out=j.artifact_out
        (out/'second.txt').write_bytes(b'Second response deliverable')
        j.answer_worker(7,'Second response adds a file without repeating the first.',item='checkpoint-two',finish=False,phase=None)
        j.wait(lambda:any(v['name']=='second.txt' for v in j.read('presentation','artifacts','list')['result']['items']))
        after=j.read('presentation','artifacts','list')['result']['items']
        assert len([v for v in after if v['name']=='alpha.txt'])==1,'Prior file was reattributed/repeated'
        assert j.row("SELECT native_item FROM interaction_artifacts WHERE name='second.txt'")==[('checkpoint-two',)]
        assert all(any(v['id']==old['id'] for v in after) for old in first)
        j.broadcast(native,'turn/completed',{'turn':{'id':j.workers[native],'status':'completed'}})
        j.idle()
        print('PASS files arrive during running execution; later response adds files without reattribution or repeated native work')
    finally:j.close()


def same_session_progress(binary):
    j=ArtifactRouting(str(Path(binary).resolve()));held=None
    try:
        retained,binding=prepare(j)
        held=(retained/'.lock').open('w');(retained/'.lock').chmod(0o600);fcntl.flock(held,fcntl.LOCK_EX)
        j.submit(binding,'#Alpha First output with files.')
        j.wait(lambda:j.instructions(7) and j.instructions(7)[0][1]=='accepted')
        j.answer_worker(7,'Primary before copying completes.')
        j.wait(lambda:any(s['id'].startswith('delivery:') for s in j.read('presentation','list','0','0')['result']['items']))
        j.submit(binding,'#Alpha Continue the conversation while files are preparing.')
        j.wait(lambda:len(j.instructions(7))==2 and j.instructions(7)[1][1]=='accepted',seconds=5)
        fcntl.flock(held,fcntl.LOCK_UN);held.close();held=None
        j.answer_worker(7,'Next conversation turn progressed.')
        j.idle()
        print('PASS same-session successor input progresses while prior files await storage; zero models')
    finally:
        if held is not None:fcntl.flock(held,fcntl.LOCK_UN);held.close()
        j.close()


def qualify(binary):
    j=ArtifactRouting(str(Path(binary).resolve()));client=None;held=None
    try:
        retained,binding=prepare(j)
        held=(retained/'.lock').open('w');(retained/'.lock').chmod(0o600);fcntl.flock(held,fcntl.LOCK_EX)
        original,_=j.submit(binding,'#Alpha Make exactly two deliverables for requirement17.')
        j.wait(lambda:j.instructions(7) and j.instructions(7)[0][1]=='accepted')
        j.answer_worker(7,'Generated the requested text and image.')
        def reply():
            return next((s for s in j.read('presentation','list','0','0')['result']['items']
                         if s['id'].startswith('delivery:') and 'Generated the requested' in s['preview']),None)
        j.wait(lambda:reply() is not None)
        before=reply();assert before['current'] and not before.get('artifacts')
        j.wait(lambda:j.row("SELECT phase FROM interaction_artifact_jobs") in ([('running',)],[('pending',)]))
        j.idle();j.stop();j.start();j.wait_control_socket(j.socket)
        # Only the retained byte job may resume; the original native work is done.
        native_turns=j.row('SELECT COUNT(*) FROM turns')
        fcntl.flock(held,fcntl.LOCK_UN);held.close();held=None
        j.wait(lambda:len(j.read('presentation','artifacts','list')['result']['items'])==3)
        j.idle()
        assert j.row('SELECT COUNT(*) FROM turns')==native_turns,'Restart replayed native work'
        after=reply();assert after['revision']==before['revision'],'File arrival invalidated spoken text'
        assert len(after['artifacts'])==3
        assert not (retained/hashlib.sha256(b'PRIVATE UNRELATED BYTES').hexdigest()).exists()
        config={'receiver_config':str(j.config),'state_dir':str(j.root/'dashboard'),
            'origin':'https://example.test','bot_id':123,'companion':{'enabled':True,'grants_dir':str(j.root/'grants')}}
        provider=Provider(config)
        client=TestClient(create_app(config,provider,lambda raw,bot:int(raw)),base_url=config['origin'])
        login=client.post('/api/session',json={'init_data':'100'},headers={'origin':config['origin']})
        headers={'authorization':'Bearer '+login.json()['session_token']}
        artifacts=client.get('/api/interaction/artifacts',headers=headers)
        assert artifacts.status_code==200,artifacts.text
        all_files=artifacts.json()['items'];assert len(all_files)==3
        selected=[file for file in all_files if file['available']];assert len(selected)==2
        denied=next(file for file in all_files if not file['available']);assert denied['name']=='escape.txt'
        assert client.get('/api/interaction/artifacts/'+denied['id'],headers=headers).status_code==409
        assert client.get('/api/interaction',headers=headers).json()['can_download_files']
        turns=j.row('SELECT COUNT(*) FROM turns')
        for file in selected:
            response=client.get('/api/interaction/artifacts/'+file['id'],headers=headers)
            assert response.status_code==200,response.text
            assert response.content==j.expected[file['name']]
            assert hashlib.sha256(response.content).hexdigest()==file['sha256']
        assert j.row('SELECT COUNT(*) FROM turns')==turns,'A download executed model work'
        target=j.root/'device.json';grants=Grants(config,provider);grants.enroll('Synthetic file client',target,60)
        credential=json.loads(target.read_text());device={'authorization':credential['authorization']}
        url='/api/interaction/artifacts/'+selected[0]['id']
        assert client.get(url,headers=device).status_code==403
        grants.artifacts(credential['grant_id'],True)
        assert client.get(url,headers=device).content==j.expected[selected[0]['name']]
        grants.artifacts(credential['grant_id'],False)
        assert client.get(url,headers=device).status_code==403
        # Supported direct /new renews the worker binding, without routing or
        # requiring the old physical thread to be current for retained bytes.
        j.admit('/new',topic=7)
        j.wait(lambda:j.row('SELECT codex_thread_id FROM sessions WHERE thread_id=7')==[(None,)])
        assert client.get(url,headers=headers).content==j.expected[selected[0]['name']]
        assert len(j.instructions(7))==1
        print('PASS actual routed file capture, immutable text/image bytes, partial symlink refusal, independent primary return/stable speech revision, authorized download, device denial/revocation and supported worker renewal; zero models')
    finally:
        if held is not None:fcntl.flock(held,fcntl.LOCK_UN);held.close()
        if client:client.close()
        j.close()


if __name__=='__main__':
    files_before_execution_ends(sys.argv[1])
    same_session_progress(sys.argv[1])
    qualify(sys.argv[1])
