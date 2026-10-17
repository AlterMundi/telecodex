#!/usr/bin/env python3
"""Canonical source reads through the actual owner-local listener; all peers synthetic."""
import json
from pathlib import Path
import subprocess
import sys
from semantic_routing_io import Semantic


class Presentation(Semantic):
    def __init__(self,binary):
        super().__init__(binary)
        self.socket=self.root/'control.sock'
        self.config.write_text(self.config.read_text().replace('[telegram]',
            f'[session_control]\nsocket_path="{self.socket}"\nhuman_id=100\n[telegram]'))

    def read(self,*arguments,ok=True):
        run=subprocess.run([self.binary,'--control',str(self.config),'presentation',*map(str,arguments)],
            capture_output=True,text=True,timeout=15)
        assert (run.returncode==0)==ok,run.stderr
        return json.loads(run.stdout)


def qualify(binary,dashboard=False):
    j=Presentation(binary)
    try:
        j.prepare()
        j.admit('NATURAL-ROUTE Work on Alpha and Beta independently.',topic=1)
        j.wait(lambda:len(j.outputs)==5)
        j.wait(lambda:all(len(j.instructions(t))==1 and j.instructions(t)[0][1]=='accepted' for t in (7,8)))
        j.entry_send({'method':'item/completed','params':{'threadId':j.entry,'turnId':j.entry_turn,
            'item':{'id':'integrator-answer','type':'agentMessage','phase':'final_answer','text':'Both requests are underway.'}}})
        j.entry_send({'method':'turn/completed','params':{'threadId':j.entry,'turn':{'id':j.entry_turn,'status':'completed'}}})
        j.answer(8,'Beta result.')
        j.answer(7,'Alpha result.')
        j.wait(lambda:j.row("SELECT count(*) FROM interaction_deliveries WHERE phase='delivered'")==[(2,)])
        j.idle()
        packets=sum(map(len,j.packets.values()))
        listing=j.read('list',0,0)['result']
        assert len(listing['items'])==3
        assert {i['kind'] for i in listing['items']}=={'worker_result','integrator_answer'}
        assert all(i['label'] and i['sequence']>0 for i in listing['items'])
        recent=j.read('before',0,0)['result']
        assert {i['id'] for i in recent['items']}=={i['id'] for i in listing['items']}
        assert recent['newest']==listing['newest'] and not recent['more']
        selected=next(i for i in listing['items'] if i['preview']=='Alpha result.')
        part=j.read('part',selected['id'],selected['revision'],0)['result']
        assert part['text']=='Alpha result.' and part['current'] and part['completed']
        assert part['playback']=='not_observed'
        assert not j.read('list',listing['cursor']['delivery'],listing['cursor']['native'])['result']['items']
        rejected=j.read('part',selected['id'],'0'*64,0,ok=False)
        assert not rejected['ok']
        if dashboard:
            sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
            import speech_render as fixtures
            from fastapi.testclient import TestClient
            from dashboard.app import create_app
            from dashboard.provider import Provider
            fixture=fixtures.Render()
            fixture.setUp()
            try:
                config={'receiver_config':str(j.config),'state_dir':str(j.root/'dashboard'),
                    'origin':'https://example.test','prefix':'/sessions','bot_id':123,
                    'native_home':str(j.root/'native'),
                    'speech':{'binding':str(fixture.binding),'timeout_seconds':5}}
                provider=Provider(config)
                with TestClient(create_app(config,provider,lambda raw,bot:int(raw)),base_url='https://example.test') as client:
                    headers={'origin':'https://example.test'}
                    token=client.post('/api/session',json={'init_data':'100'},headers=headers).json()['session_token']
                    headers['authorization']='Bearer '+token
                    headers['x-dashboard-csrf']=client.get('/api/alerts',headers=headers).json()['csrf']
                    observed=client.get('/api/interaction',headers=headers)
                    assert observed.status_code==200,observed.text
                    assert observed.json()['can_listen'] and observed.json()['csrf']==headers['x-dashboard-csrf']
                    history=client.get('/api/interaction?direction=before',headers=headers)
                    assert history.status_code==200 and len(history.json()['items'])==3
                    body={'source':selected['id'],'revision':selected['revision'],'offset':0}
                    audio=client.post('/api/interaction/audio',json=body,headers=headers)
                    assert audio.status_code==200,audio.text
                    assert audio.content==fixture.data and audio.headers['x-playback-observation']=='not_observed'
                    assert b'Alpha result.' in fixture.calls[0][2]
                    resumed=client.post('/api/interaction/audio',json=body,headers=headers)
                    assert resumed.content==audio.content and len(fixture.calls)==1
                print('PASS authenticated HTTP → actual owner socket → canonical SQLite result → actual renderer/cache → synthetic speech HTTP; reconnect reuses audio')
            finally:
                fixture.tearDown()
        assert sum(map(len,j.packets.values()))==packets,'Presentation started native work'
        print('PASS owner-local canonical worker/entry source reads, current revision/parts/cursors and no new native submissions; zero model/external-provider calls')
    finally:
        j.close()


if __name__=='__main__':
    qualify(str(Path(sys.argv[1]).resolve()),'--dashboard' in sys.argv[2:])
