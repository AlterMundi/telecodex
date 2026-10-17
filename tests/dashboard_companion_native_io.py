"""Device enrollment to canonical native input/answers; synthetic peers, zero models."""
import json
from pathlib import Path
import sys

from fastapi.testclient import TestClient
from conversation_cold_entry_http_io import ColdEntry, Browser

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from dashboard.app import create_app
from dashboard.companion import Grants
from dashboard.private_state import read_json
from dashboard.provider import Provider


def qualify(binary):
    j = ColdEntry(str(Path(binary).resolve())); b = None
    try:
        j.prepare()
        config = {'receiver_config':str(j.config),'state_dir':str(j.root/'device-dashboard'),
            'origin':'https://example.test','bot_id':123,
            'companion':{'enabled':True,'grants_dir':str(j.root/'device-grants')}}
        provider = Provider(config); grants = Grants(config,provider)
        receipt = grants.enroll('Selected native fixture', j.root/'device.json', 3600)
        device = read_json(j.root/'device.json')
        b = Browser.__new__(Browser)
        b.client = TestClient(create_app(config,provider),base_url=config['origin'])
        b.headers = {'origin':config['origin'],'authorization':device['authorization'],
            'x-dashboard-csrf':device['csrf']}
        b.binding = b.get('/api/interaction/inputs')['binding']
        assert not j.packets and j.row('SELECT codex_thread_id FROM sessions WHERE thread_id=1') == [(None,)]
        saved, value = b.text('#Unknown Retain this until the human clarifies its target.')
        assert value['phase'] == 'awaiting_target'
        refused = b.client.post('/api/interaction/inputs/'+saved+'/route',
            json={'binding':b.binding,'selector':'t999'},headers=b.headers)
        assert refused.status_code == 200, refused.text
        assert b.status(saved)['phase'] == 'awaiting_target' and not b.status(saved)['routing']
        assert j.row('SELECT count(*) FROM sessions WHERE thread_id=999') == [(0,)]
        original, value = b.text('#Alpha Work independently on this selected task.')
        j.wait(lambda:j.instructions(7) and j.instructions(7)[0][1]=='accepted')
        worker = next(send for thread,send in j.clients.values() if thread=='native-topic-7')
        worker({'id':'canonical-device-owned-question','method':'item/tool/requestUserInput','params':{
            'threadId':'native-topic-7','turnId':j.workers['native-topic-7'],
            'itemId':'device-question','isBlocking':True,'questions':[{
                'id':'scope','header':'Scope','question':'Which requirement should this task use?','options':None}]}})
        j.wait(lambda:j.row("SELECT count(*) FROM human_interactions WHERE state='pending'")==[(1,)])
        view=b.get('/api/interaction/requests')['items'][0]
        view=b.get('/api/interaction/requests/'+view['id'])
        clip,_=b.voice({'source':'request:'+view['id'],'revision':view['revision']})
        j.wait(lambda:b.status(clip)['phase']=='answered')
        j.wait(lambda:any(v['id']=='canonical-device-owned-question'for v in j.native_answers))
        answer=next(v for v in j.native_answers if v['id']=='canonical-device-owned-question')['result']['answers']['scope']['answers'][0]
        assert '[Committed voice input]' in answer and 'Continue requirement17' in answer
        assert b.status(clip)['decision']['source_input']==clip
        assert not j.packets and j.row('SELECT codex_thread_id FROM sessions WHERE thread_id=1')==[(None,)]
        for path in ('/api/state','/api/stop','/api/interaction/requests/'+view['id']):
            response=b.client.post(path,json={},headers=b.headers)
            assert response.status_code==403,(path,response.text)
        calls=(j.root/'asr-calls').read_bytes()
        inputs=j.row('SELECT count(*) FROM conversation_inputs')
        grants.revoke(receipt['grant_id'])
        assert b.client.get('/api/interaction',headers=b.headers).status_code==401
        assert b.client.post('/api/interaction/voice',content=b'not buffered',headers=b.headers).status_code==401
        assert (j.root/'asr-calls').read_bytes()==calls and j.row('SELECT count(*) FROM conversation_inputs')==inputs
        print('PASS actual device enrollment/HTTP/SQLite/native text and committed voice question answer; unlisted destination, administrative and approval routes refused; revocation before buffering/ASR; cold front unchanged; zero models.')
    finally:
        if b:b.client.close()
        j.close()


if __name__=='__main__':
    qualify(sys.argv[1])
