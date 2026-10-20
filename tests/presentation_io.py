"""Authenticated HTTP to actual renderer/cache; synthetic source and speech peers."""
import concurrent.futures
import copy
import hashlib
import fcntl
import json
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import speech_render as fixtures
from fastapi.testclient import TestClient
from dashboard.app import create_app
from dashboard.provider import Provider, ProviderUnavailable


class Receiver:
    def __init__(self):
        self.allowed = True
        self.changed = False
        self.parts = 0
        self.question = None
        self.pending = None
        self.source = {'schema':'telecodex.presentation-part/v1',
            'id':'delivery:11111111-1111-1111-1111-111111111111', 'revision':'a'*64,
            'kind':'worker_result', 'label':'#Alfa', 'entry_binding':[100, {'chat_id':100,'thread_id':1},1,'entry-native'],
            'text':'Alfa sigue trabajando. Beta ya respondió.', 'offset':0, 'next_offset':None,
            'current':True, 'correction_pending':False, 'completed':True, 'playback':'not_observed'}
    def authorized(self, human): return human == 100 and self.allowed
    def close(self): pass
    def snapshot(self): return {'sessions':[]}
    def presentation_list(self, delivery, native, direction='after'):
        result=copy.deepcopy(self.source)
        result['preview']=result.pop('text')
        return {'schema':'telecodex.presentation-list/v1','items':[result]}
    def presentation_part(self, source, revision, offset):
        self.parts += 1
        if source.startswith('input:'):
            return Provider.presentation_part(self, source, revision, offset)
        if source.startswith('request:'):
            return Provider.presentation_part(self, source, revision, offset)
        if self.changed or source != self.source['id'] or revision != self.source['revision'] or offset != 0:
            raise ProviderUnavailable('Source changed')
        return copy.deepcopy(self.source)

    input_id = staticmethod(Provider.input_id)
    clarification_source = staticmethod(Provider.clarification_source)

    goal_controls = Provider.goal_controls
    goal_control_detail = Provider.goal_control_detail

    def presentation(self, command):
        if command=='presentation goals list 0':
            return {'schema':'telecodex.goal-control-list/v1','items':[],'cursor':0,'more':False}
        if command=='presentation goals detail 11111111-1111-4111-8111-111111111111':
            return {'id':'11111111-1111-4111-8111-111111111111','phase':'awaiting_confirmation','can_confirm':True}
        assert command=='input status '+self.pending['id']
        return copy.deepcopy(self.pending)

    def conversation_input_status(self, ident):
        assert ident==self.pending['id']
        return Provider.decorate_input(copy.deepcopy(self.pending))

    def conversation_request_detail(self, ident, offset, revision):
        view = copy.deepcopy(self.question)
        if not view or ident != view['id'] or revision != view['revision'] or self.changed:
            raise ProviderUnavailable('Question changed')
        body = view['body']
        if offset > len(body):
            raise ValueError('Invalid question offset')
        view.update(body=body[offset:offset+4000], offset=offset,
                    next_offset=offset+4000 if offset+4000 < len(body) else None)
        return view


class Presentation(unittest.TestCase):
    def setUp(self):
        self.fixture = fixtures.Render()
        self.fixture.setUp()
        self.root = self.fixture.root
        self.provider = Receiver()
        self.config = {'state_dir':str(self.root/'dashboard'), 'origin':'https://example.test',
            'prefix':'/sessions','bot_id':123,'speech':{'binding':str(self.fixture.binding),'timeout_seconds':5}}
        self.client = self.client_for()
        self.headers = {'origin':'https://example.test'}
        self.body = {'source':self.provider.source['id'],'revision':self.provider.source['revision'],'offset':0}

    def client_for(self):
        return TestClient(create_app(self.config,self.provider,lambda raw,bot:int(raw)),base_url='https://example.test')

    def login(self):
        reply = self.client.post('/api/session',json={'init_data':'100'},headers=self.headers)
        token = reply.json()['session_token']
        self.headers['authorization']='Bearer '+token
        csrf = self.client.get('/api/state',headers=self.headers).json()['csrf']
        self.headers['x-dashboard-csrf']=csrf

    def audio(self, body=None):
        return self.client.post('/api/interaction/audio',json=self.body if body is None else body,headers=self.headers)

    def tearDown(self):
        self.client.close()
        self.fixture.tearDown()

    def test_authentication_source_selection_and_reconnect_reuse_actual_audio(self):
        self.assertEqual(self.audio().status_code,401)
        self.login()
        missing={k:v for k,v in self.headers.items() if k!='x-dashboard-csrf'}
        self.assertEqual(self.client.post('/api/interaction/audio',json=self.body,headers=missing).status_code,403)
        self.assertEqual(self.audio({**self.body,'text':'browser-chosen words'}).status_code,409)
        self.assertEqual(len(self.fixture.calls),0)
        first=self.audio()
        self.assertEqual(first.status_code,200,first.text)
        self.assertEqual(first.content,self.fixture.data)
        self.assertEqual(first.headers['x-audio-sha256'],hashlib.sha256(first.content).hexdigest())
        self.assertEqual(first.headers['x-playback-observation'],'not_observed')
        self.assertNotIn(str(self.root),str(dict(first.headers)))
        self.assertIn(self.provider.source['text'].encode(),self.fixture.calls[0][2])
        self.assertIn(('#Alfa. '+self.provider.source['text']).encode(),self.fixture.calls[0][2])
        canonical = self.client.get('/api/interaction/part',params=self.body,headers=self.headers)
        self.assertEqual(canonical.json()['text'],self.provider.source['text'])
        self.client.close()
        self.client=self.client_for()
        self.login()
        resumed=self.audio()
        self.assertEqual(resumed.content,first.content)
        self.assertEqual(len(self.fixture.calls),1)
        self.assertGreaterEqual(self.provider.parts,4)

    def test_worker_progress_keeps_audible_session_label_without_completion_claim(self):
        self.provider.source.update(kind='worker_progress',id='native:1',current=None,completed=None)
        self.body['source']=self.provider.source['id']
        self.test_authentication_source_selection_and_reconnect_reuse_actual_audio()

    def test_matrix_reply_keeps_label_source_and_cached_audio_without_completion_claim(self):
        self.provider.source.update(kind='matrix_result',id='matrix:11111111-1111-1111-1111-111111111111',completed=None)
        self.body['source']=self.provider.source['id']
        self.test_authentication_source_selection_and_reconnect_reuse_actual_audio()

    def test_source_label_change_during_synthesis_refuses_old_labeled_audio(self):
        self.login()
        self.fixture.hold=True
        with concurrent.futures.ThreadPoolExecutor(max_workers=1) as pool:
            pending=pool.submit(self.audio)
            self.assertTrue(self.fixture.entered.wait(timeout=4))
            self.provider.source['label']='#Beta'
            self.fixture.release.set()
            self.assertEqual(pending.result(timeout=8).status_code,409)
        self.assertIn(b'#Alfa. ',self.fixture.calls[0][2])
        self.assertEqual(len(self.fixture.calls),1)

    def test_worker_continuation_and_integrator_audio_do_not_repeat_a_label(self):
        self.login()
        for changes in ({'offset':1600},{'kind':'integrator_answer','offset':0}):
            self.provider.source.update(changes)
            # Use the actual renderer; select the certified part directly so
            # this test need not fake canonical pagination in the HTTP peer.
            import asyncio
            from dashboard.speech import Speech
            renderer=Speech(self.config['speech'])
            asyncio.run(renderer.render(self.provider.source))
            self.assertNotIn(b'#Alfa. ',self.fixture.calls[-1][2])

    def test_correction_during_synthesis_prevents_stale_audio_response(self):
        self.login()
        self.fixture.hold=True
        with concurrent.futures.ThreadPoolExecutor(max_workers=1) as pool:
            pending=pool.submit(self.audio)
            self.assertTrue(self.fixture.entered.wait(timeout=4))
            self.provider.changed=True
            self.fixture.release.set()
            response=pending.result(timeout=8)
        self.assertEqual(response.status_code,409)
        self.assertNotEqual(response.headers.get('content-type'),'audio/wav')
        # Completed synthesis remains retained; changing authority never repeats worker work.
        states=list((self.root/'cache').rglob('state.json'))
        self.assertEqual(json.loads(states[0].read_text())['phase'],'rendered')
        self.assertEqual(len(self.fixture.calls),1)

    def test_owner_revocation_during_synthesis_prevents_private_audio(self):
        self.login()
        self.fixture.hold=True
        with concurrent.futures.ThreadPoolExecutor(max_workers=1) as pool:
            pending=pool.submit(self.audio)
            self.assertTrue(self.fixture.entered.wait(timeout=4))
            self.provider.allowed=False
            self.fixture.release.set()
            response=pending.result(timeout=8)
        self.assertEqual(response.status_code,401)
        self.assertNotEqual(response.content,self.fixture.data)
        self.assertEqual(len(self.fixture.calls),1)

    def destination_question(self):
        self.provider.pending={'id':'11111111-1111-4111-8111-111111111111',
            'entry_binding':copy.deepcopy(self.provider.source['entry_binding']),'binding':'b'*64,
            'phase':'awaiting_target','clarifiable':True,'retired':False,'routing':None,
            'text':'#Unknown Preserve the exact original 🦉. '*100,
            'routing_choices':[{'selector':'t7','label':'Alpha'},{'selector':'t8','label':'Beta'}]}
        source=Provider.clarification_source(self.provider.pending)
        self.body={'source':source['id'],'revision':source['revision'],'offset':0}
        return source

    def test_destination_question_reuses_parts_cache_and_logical_entry_generation(self):
        source=self.destination_question();self.login()
        # Enough valid bounded labels to exercise multipart speech, without
        # reading the entire long original aloud or inventing a destination.
        self.provider.pending['routing_choices']=[{'selector':'t'+str(i),'label':str(i)+'🦉'*195}for i in range(9)]
        source=Provider.clarification_source(self.provider.pending)
        self.body['revision']=source['revision']
        original=self.provider.pending['text'];parts=[];offset=0
        while offset is not None:
            selected={**self.body,'offset':offset}
            response=self.client.get('/api/interaction/part',params=selected,headers=self.headers)
            self.assertEqual(response.status_code,200,response.text)
            part=response.json();self.assertEqual(part['kind'],'route_clarification')
            parts.append(part['text']);self.assertLessEqual(len(part['text']),1600)
            audio=self.audio(selected);self.assertEqual(audio.status_code,200,audio.text)
            self.assertEqual(self.audio(selected).content,audio.content)
            offset=part['next_offset']
        self.assertEqual(''.join(parts),source['text'])
        self.assertEqual(self.provider.pending['text'],original)
        self.assertEqual(len(self.fixture.calls),len(parts))
        self.provider.pending['entry_binding'][3]='newly-established-native'
        latest=Provider.clarification_source(self.provider.pending)
        self.assertEqual(latest['revision'],source['revision'])
        self.assertEqual(latest['entry_binding'],source['entry_binding'])

    def test_resolved_or_retired_destination_never_synthesizes(self):
        self.destination_question();self.login()
        for changes in ({'clarifiable':False},{'phase':'routed'},{'retired':True}):
            prior=copy.deepcopy(self.provider.pending);self.provider.pending.update(changes)
            self.assertEqual(self.audio().status_code,409)
            self.provider.pending=prior
        self.assertEqual(len(self.fixture.calls),0)

    def test_destination_or_choice_change_during_synthesis_refuses_old_audio(self):
        self.destination_question();self.login();self.fixture.hold=True
        with concurrent.futures.ThreadPoolExecutor(max_workers=1)as pool:
            pending=pool.submit(self.audio)
            self.assertTrue(self.fixture.entered.wait(timeout=4))
            self.provider.pending['routing_choices'][0]['label']='Changed destination'
            self.fixture.release.set()
            self.assertEqual(pending.result(timeout=8).status_code,409)
        self.assertEqual(len(self.fixture.calls),1)

    def question(self):
        self.provider.question = {'id':'b'*32, 'revision':'c'*64, 'kind':'question',
            'actionable':True, 'label':'#Alfa', 'entry_binding':self.provider.source['entry_binding'],
            'body':'¿Qué opción elegís? 🦉\n' * 230}
        self.body = {'source':'request:'+'b'*32, 'revision':'c'*64, 'offset':0}

    def test_question_parts_reconstruct_all_unicode_and_use_existing_audio_cache(self):
        self.question()
        self.login()
        offset = 0
        parts = []
        while offset is not None:
            selected = {**self.body,'offset':offset}
            response = self.client.get('/api/interaction/part',params=selected,headers=self.headers)
            self.assertEqual(response.status_code,200,response.text)
            part = response.json()
            self.assertEqual(part['kind'],'native_question')
            self.assertEqual(part['playback'],'not_observed')
            self.assertLessEqual(len(part['text']),1702)
            parts.append(part['text'][len('#Alfa. '):] if offset==0 else part['text'])
            audio = self.audio(selected)
            self.assertEqual(audio.status_code,200,audio.text)
            self.assertEqual(audio.headers['x-presentation-source'],selected['source'])
            self.assertEqual(audio.headers['x-presentation-offset'],str(offset))
            self.assertEqual(audio.headers['x-presentation-next-offset'],str(part['next_offset']) if part['next_offset'] is not None else '')
            self.assertEqual(self.audio(selected).content,audio.content)
            offset = part['next_offset']
        self.assertEqual(''.join(parts),self.provider.question['body'])
        self.assertEqual(len(self.fixture.calls),len(parts),'Reopening audio uses retained synthesis')

    def test_closed_approval_changed_and_invalid_question_selections_never_synthesize(self):
        self.question()
        self.login()
        for changes in ({'actionable':False},{'kind':'approval'},{'revision':'d'*64}):
            original=copy.deepcopy(self.provider.question)
            self.provider.question.update(changes)
            self.assertEqual(self.audio().status_code,409)
            self.provider.question=original
        for changes in ({'offset':-1},{'offset':True},{'source':'request:bad'},{'revision':'bad'}):
            self.assertEqual(self.audio({**self.body,**changes}).status_code,409)
        self.assertEqual(len(self.fixture.calls),0)

    def test_question_advanced_during_synthesis_cannot_return_old_question_audio(self):
        self.question()
        self.login()
        self.fixture.hold=True
        with concurrent.futures.ThreadPoolExecutor(max_workers=1) as pool:
            pending=pool.submit(self.audio)
            self.assertTrue(self.fixture.entered.wait(timeout=4))
            self.provider.question['revision']='d'*64
            self.fixture.release.set()
            response=pending.result(timeout=8)
        self.assertEqual(response.status_code,409)
        self.assertEqual(len(self.fixture.calls),1)

    def test_total_deadline_reaps_renderer_and_retains_unknown_request_without_retry(self):
        self.client.close()
        self.config['speech']['timeout_seconds']=1
        self.client=self.client_for()
        self.login()
        self.fixture.hold=True
        try:
            response=self.audio()
            self.assertEqual(response.status_code,409)
            self.assertTrue(self.fixture.entered.is_set())
            state=next((self.root/'cache').rglob('state.json'))
            self.assertEqual(json.loads(state.read_text())['phase'],'intent')
            with state.with_name('.lock').open('rb') as lock:
                fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
            self.assertEqual(self.audio().status_code,409)
            self.assertEqual(len(self.fixture.calls),1)
        finally:
            self.fixture.release.set()


if __name__=='__main__':
    unittest.main()
