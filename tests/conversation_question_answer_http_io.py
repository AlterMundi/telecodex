#!/usr/bin/env python3
"""Original text/voice answers the selected native question; zero inference."""
import hashlib
import json
from pathlib import Path
import sqlite3
import sys
import uuid
from conversation_cold_entry_http_io import ColdEntry, Browser
from conversation_voice_http_io import audio


def qualify(binary):
    j = ColdEntry(str(Path(binary).resolve())); b = None
    try:
        j.prepare(); b = Browser(j)
        task, _ = b.text('#Alpha Work on the selected requirement.')
        j.wait(lambda: j.instructions(7) and j.instructions(7)[0][1] == 'accepted')
        def emit(topic, ident, count=2):
            thread = j.row('SELECT codex_thread_id FROM sessions WHERE thread_id=?', (topic,))[0][0]
            send = next(send for selected, send in j.clients.values() if selected == thread)
            send({'id': ident, 'method': 'item/tool/requestUserInput', 'params': {
                'threadId': thread, 'turnId': j.workers[thread], 'itemId': ident,
                'isBlocking': True, 'questions': [
                    {'id': 'answer-' + str(n), 'header': 'Selected question ' + str(n),
                     'question': 'Provide the exact details for this question only.', 'options': None}
                    for n in range(count)]}})
            j.wait(lambda: any(v['kind'] == 'question' and v['label'] == '#' + ('Alpha' if topic == 7 else 'Beta')
                               and v['state'] == 'pending' for v in b.get('/api/interaction/requests')['items']))
            row = next(v for v in b.get('/api/interaction/requests')['items']
                       if v['kind'] == 'question' and v['label'] == '#' + ('Alpha' if topic == 7 else 'Beta') and v['state'] == 'pending')
            return b.get('/api/interaction/requests/' + row['id'])
        def reference(view):
            return {'source': 'request:' + view['id'], 'revision': view['revision']}
        def post(text, selected, ident=None):
            body = {'id': ident or str(uuid.uuid4()), 'binding': b.binding, 'text': text, 'reply': selected}
            return body, b.client.post('/api/interaction/inputs', json=body, headers=b.headers)
        question = emit(7, 'canonical-selected-question')
        frozen = reference(question)
        turns = j.row('SELECT count(*) FROM turns')
        text = '  Use only the selected scope.  '
        original, result = post(text, frozen); assert result.status_code == 200, result.text
        result = result.json()
        assert result['phase'] == 'answered' and result['decision']['source_input'] == original['id']
        assert result['native_acceptance'] == 'unconfirmed' and not j.native_answers
        repeated = post(text, frozen, original['id'])[1]
        assert repeated.status_code == 200 and not repeated.json()['new_input']
        assert post('Different answer under the same source identity', frozen, original['id'])[1].status_code == 503
        second = b.get('/api/interaction/requests/' + question['id'])
        assert second['question_index'] == 1 and second['revision'] != question['revision']
        assert post('An old selected question is not the next one.', frozen)[1].status_code == 503
        voice, _ = b.voice(reference(second))
        j.wait(lambda: b.status(voice)['phase'] == 'answered')
        j.wait(lambda: any(v['id'] == 'canonical-selected-question' for v in j.native_answers))
        answered = next(v for v in j.native_answers if v['id'] == 'canonical-selected-question')['result']['answers']
        assert answered['answer-0']['answers'] == [text]
        spoken = answered['answer-1']['answers'][0]
        assert '[Committed voice input]' in spoken and voice in spoken
        assert 'Continue requirement17 in this selected work.' in spoken
        assert b.status(voice)['decision']['source_input'] == voice
        assert j.row('SELECT count(*) FROM turns') == turns and not j.packets
        assert j.row('SELECT codex_thread_id FROM sessions WHERE thread_id=1') == [(None,)]
        before = (j.root / 'asr-calls').read_text().splitlines()
        clip = audio()
        def retry_voice(ident, reply):
            headers = {**b.headers, 'content-type': 'audio/wav', 'x-input-id': ident,
                       'x-input-binding': b.binding, 'x-audio-sha256': hashlib.sha256(clip).hexdigest(),
                       'x-reply-source': reply['source'], 'x-reply-revision': reply['revision']}
            return b.client.post('/api/interaction/voice', content=clip, headers=headers)
        repeated = retry_voice(voice, reference(second))
        assert repeated.status_code == 200 and repeated.json()['phase'] == 'answered'
        assert not repeated.json()['new_input'] and (j.root / 'asr-calls').read_text().splitlines() == before

        # An explicit topic takes precedence even after the selected question closes.
        override, response = post('#Beta Start independent work instead of answering.', frozen)
        assert response.status_code == 200, response.text
        j.wait(lambda: j.instructions(8) and j.instructions(8)[0][1] == 'accepted')
        assert not b.status(override['id']).get('decision')
        beta = emit(8, 'canonical-changing-question')
        (j.root / 'hold-asr').touch()
        slow, _ = b.voice(reference(beta))
        j.wait(lambda: len((j.root / 'asr-calls').read_text().splitlines()) == len(before) + 1)
        written, response = post('Newer explicit answer to the first Beta question.', reference(beta))
        assert response.status_code == 200, response.text
        next_beta = b.get('/api/interaction/requests/' + beta['id'])
        assert next_beta['question_index'] == 1
        (j.root / 'release-asr').touch()
        j.wait(lambda: b.status(slow)['phase'] == 'retained_unsent')
        assert not b.status(slow).get('decision')
        assert b.get('/api/interaction/requests/' + beta['id'])['question_index'] == 1
        _, response = post('Answer the current second question.', reference(next_beta))
        assert response.status_code == 200, response.text
        j.wait(lambda: any(v['id'] == 'canonical-changing-question' for v in j.native_answers))
        answers = next(v for v in j.native_answers if v['id'] == 'canonical-changing-question')['result']['answers']
        assert answers['answer-0']['answers'] == ['Newer explicit answer to the first Beta question.']
        assert answers['answer-1']['answers'] == ['Answer the current second question.']

        worker_send=next(send for selected,send in j.clients.values() if selected=='native-topic-8')
        worker_send({'id':'canonical-selected-approval','method':'item/commandExecution/requestApproval','params':{
            'threadId':'native-topic-8','turnId':j.workers['native-topic-8'],'itemId':'approval-source',
            'command':'echo synthetic-only','availableDecisions':['accept','decline']}})
        j.wait(lambda:any(v['kind']=='approval' for v in b.get('/api/interaction/requests')['items']))
        approval=next(v for v in b.get('/api/interaction/requests')['items'] if v['kind']=='approval')
        approval=b.get('/api/interaction/requests/'+approval['id'])
        assert post('Accept',reference(approval))[1].status_code==503
        assert not any(v['id']=='canonical-selected-approval' for v in j.native_answers)

        late = emit(7, 'canonical-retired-question', 1)
        (j.root / 'release-asr').unlink()
        count = len((j.root / 'asr-calls').read_text().splitlines())
        retired, _ = b.voice(reference(late))
        j.wait(lambda: len((j.root / 'asr-calls').read_text().splitlines()) == count + 1)
        with sqlite3.connect(j.root / 'state.sqlite') as db:
            db.execute('UPDATE users SET allowed=0 WHERE tg_user_id=100')
            db.execute('UPDATE users SET allowed=1 WHERE tg_user_id=100')
        (j.root / 'release-asr').touch()
        j.wait(lambda: b.status(retired)['phase'] in ('retained_unsent', 'preparation_unconfirmed'))
        assert not b.status(retired).get('decision')
        assert not any(v['id'] == 'canonical-retired-question' for v in j.native_answers)
        recorded = json.loads(j.row('SELECT doc FROM conversation_decisions WHERE request_id=? AND revision=?',
                                    (second['id'], second['revision']))[0][0])
        assert recorded['source_input'] == voice and recorded['action']['text'] == spoken
        j.stop(); j.start(); j.wait_control_socket(j.socket)
        assert b.status(original['id'])['phase'] == 'answered'
        assert b.status(voice)['decision']['source_input'] == voice
        b.binding = b.get('/api/interaction/inputs')['binding']
        repeated = retry_voice(voice, reference(second))
        assert repeated.status_code == 200 and not repeated.json()['new_input'], repeated.text
        assert len((j.root / 'asr-calls').read_text().splitlines()) == count + 1
        assert not j.packets and j.topic_calls == 0
        print('PASS actual HTTP/SQLite/native selected-question text/voice answers, atomic original/decision receipts, closed-question exact retries, no front model/new turn, ASR provenance, explicit topic override, stale-question ASR refusal, grant ABA and restart without replay; zero models.')
    finally:
        if b: b.client.close()
        j.close()


if __name__ == '__main__':
    qualify(sys.argv[1])
