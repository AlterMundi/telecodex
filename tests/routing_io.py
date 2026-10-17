#!/usr/bin/env python3
"""Integrating conversation through real HTTP/native proxy/SQLite boundaries.
Every human, token, model, conversation and peer here is synthetic.
"""
import json
import pathlib
import sys
import threading
from goals_io import Goals


class Routing(Goals):
    def __init__(self, binary):
        self.question_held = threading.Event()
        self.release_question = threading.Event()
        self.reject_next = False
        self.rejected_turn = None
        self.routed = False
        self.packets = {}
        self.runs = {}
        super().__init__(binary)

    def prepare(self):
        self.start()
        for topic in (7, 8):
            self.admit('Seed existing native conversation', topic=topic)
            self.wait(lambda: self.has('Initial goal turn delivered.', topic))
            self.idle()
        self.stop()
        with self.config.open('a') as f:
            f.write('''\n[interaction]
human_id=100
chat_id=100
topic_id=1
[[interaction.targets]]
name="Alpha"
topic_id=7
[[interaction.targets]]
name="Beta"
topic_id=8
''')
        self.messages.clear()
        self.routed = True
        self.start()

    def handle_extra_rpc(self, m, send):
        if self.routed and m.get('id') == 900 and not m.get('method'):
            self.answers.append(m)
            return True
        if not self.routed or m.get('method') not in ('turn/start', 'turn/steer', 'turn/interrupt'):
            return False
        p = m['params']
        thread = p['threadId']
        if m['method'] == 'turn/interrupt':
            assert p['turnId'] == self.runs[thread]
            send({'id': m['id'], 'result': {}})
            self.broadcast(thread, 'turn/completed', {'turn': {'id': p['turnId'], 'status': 'interrupted'}})
        elif m['method'] == 'turn/start':
            turn = 'routed-' + str(len(self.runs) + 1)
            self.runs[thread] = turn
            self.packets.setdefault(thread, []).append(p)
            send({'id': m['id'], 'result': {'turn': {'id': turn}}})
            self.broadcast(thread, 'turn/started', {'turn': {'id': turn, 'status': 'inProgress'}})
        else:
            self.packets.setdefault(thread, []).append(p)
            if self.reject_next:
                self.reject_next = False
                self.rejected_turn = self.runs[thread]
            if self.rejected_turn == self.runs[thread]:
                send({'id': m['id'], 'error': {'code': -32600, 'message': 'Synthetic finished generation'}})
            else:
                send({'id': m['id'], 'result': {'turnId': self.runs[thread]}})
        return True

    def before_send_message_reply(self, payload):
        if self.routed and payload['text'].startswith('Codex question'):
            self.question_held.set()
            self.release_question.wait(timeout=10)

    def instructions(self, topic):
        return self.row('SELECT i.id,i.phase,i.doc FROM interaction_instructions i JOIN sessions s ON s.id=i.session_id WHERE s.thread_id=? ORDER BY i.sequence', (topic,))

    def answer(self, topic, text, *, covered=None, item='answer', finish=True):
        thread = f'native-topic-{topic}'
        turn = self.runs[thread]
        rows = self.instructions(topic)
        ids = [r[0] for r in rows] if covered is None else covered
        requests = sorted({req for r in rows for req in json.loads(r[2])['requests']})
        claim = {'requests': requests, 'covered': ids, 'completed': requests}
        self.broadcast(thread, 'item/completed', {'turnId': turn, 'item': {
            'id': item, 'type': 'agentMessage', 'phase': 'final_answer',
            'text': text + '\n<daimon-result>' + json.dumps(claim) + '</daimon-result>'}})
        if finish:
            self.broadcast(thread, 'turn/completed', {'turn': {'id': turn, 'status': 'completed'}})


def parallel_and_corrections(binary):
    j = Routing(binary)
    try:
        j.prepare()
        original = j.admit('#Alpha Analyze a complex task', topic=1)
        j.admit('#Beta Work independently', topic=1)
        j.wait(lambda: len(j.runs) == 2)
        j.wait(lambda: all(r[1] == 'accepted' for topic in (7, 8) for r in j.instructions(topic)))
        j.admit('Change the Alpha requirement now', topic=1, reply=original)
        j.wait(lambda: len(j.instructions(7)) == 2 and j.instructions(7)[1][1] == 'accepted')
        assert len(j.packets['native-topic-7']) == 2, j.packets
        assert len(j.packets['native-topic-8']) == 1, j.packets
        # Beta finishes first; Alpha's old result must not pass as current.
        j.answer(8, 'Beta result')
        j.wait(lambda: j.has('Beta result', 1))
        j.answer(7, 'Obsolete Alpha result', covered=[j.instructions(7)[0][0]], item='old', finish=False)
        j.wait(lambda: j.has('correction is still pending', 1))
        assert not j.has('Obsolete Alpha result', 1)
        j.answer(7, 'Corrected Alpha result', item='corrected')
        j.wait(lambda: j.has('Corrected Alpha result', 1))
        j.idle()
        result, payload = next((m,p) for m,p in j.messages if 'Corrected Alpha result' in m['text'])
        assert payload['reply_parameters']['message_id'] == original
        assert not payload['reply_parameters']['allow_sending_without_reply']
        j.admit('Further refine the same work', topic=1, reply=result['message_id'])
        j.wait(lambda: len(j.instructions(7)) == 3)
        doc = json.loads(j.instructions(7)[2][2])
        assert doc['requests'] == json.loads(j.instructions(7)[0][2])['requests']
        j.wait(lambda: j.instructions(7)[2][1] == 'accepted')
        thread = 'native-topic-7'
        for selected, send in list(j.clients.values()):
            if selected == thread:
                send({'id': 900, 'method': 'item/tool/requestUserInput', 'params': {
                    'threadId': thread, 'turnId': j.runs[thread], 'itemId': 'question',
                    'isBlocking': True, 'questions': [{'id': 'scope', 'header': 'Scope',
                        'question': 'Which refinement?', 'isOther': True, 'isSecret': False,
                        'options': [{'label': 'Small', 'description': 'One step'}]}]}})
        j.wait(lambda: j.question_held.is_set())
        j.admit('#Alpha Continue while the question publication is stalled', topic=1, reply=original)
        j.wait(lambda: len(j.instructions(7)) == 4 and j.instructions(7)[3][1] == 'accepted', seconds=3)
        assert not j.release_question.is_set()
        j.release_question.set()
        j.wait(lambda: j.row('SELECT count(*) FROM question_messages') == [(1,)])
        j.wait(lambda: j.controls(1, topic=1) is not None)
        question = j.controls(1, topic=1)
        j.admit(callback=j.button(question, ':0'), topic=1)
        j.wait(lambda: len(j.answers) == 1)
        assert j.answers[0]['result']['answers']['scope']['answers'] == ['Small']
        j.broadcast(thread, 'serverRequest/resolved', {'requestId': 900})
        j.answer(7, 'Refined result', item='refined')
        j.wait(lambda: j.has('Refined result', 1))
        j.idle()
        direct = j.admit('Direct text in an enrolled session', topic=8)
        j.wait(lambda: len(j.instructions(8)) == 2 and j.instructions(8)[1][1] == 'accepted')
        direct_doc = json.loads(j.instructions(8)[1][2])
        assert direct_doc['origin']['thread_id'] == 8 and direct_doc['origin']['message_id'] == direct
        j.admit('#Beta /interrupt', topic=1)
        j.wait(lambda: j.has('turn interrupted', 8))
        j.idle()
        interrupted = [m for m in j.rpc if m.get('method') == 'turn/interrupt']
        assert len(interrupted) == 1 and interrupted[0]['params']['threadId'] == 'native-topic-8'
        assert not any('daimon-result' in m['text'] for m,_ in j.messages)
        assert not j.has('Beta result',7) and not j.has('Corrected Alpha result',7)
        print('PASS two native sessions, reverse completion, live correction, stale suppression, exact return, reply routing and native question')
    finally:
        j.release_question.set()
        j.close()


def clarification(binary):
    j = Routing(binary)
    try:
        j.prepare()
        j.stop()
        j.config.write_text(j.config.read_text().replace('name="Beta"', 'name="Alpha"'))
        j.start()
        original = j.admit('#Alpha Retain this original input', topic=1)
        j.wait(lambda: j.has('Which conversation?', 1))
        question = next(m for m,_ in reversed(j.messages) if 'Which conversation?' in m['text'])
        assert not j.runs
        j.admit('#t8', topic=1, reply=question['message_id'])
        j.wait(lambda: len(j.instructions(8)) == 1 and j.instructions(8)[0][1] == 'accepted')
        assert not j.instructions(7)
        source = json.loads(j.row('SELECT doc FROM interaction_sources')[0][0])
        assert source['text'] == 'Retain this original input'
        assert source['origin']['message_id'] == original
        j.admit('#t7', topic=1, reply=question['message_id'])
        j.wait(lambda: j.has('already resolved or cancelled', 1))
        assert not j.instructions(7)
        before = len(j.messages)
        j.admit('#missing Cancel this input', topic=1)
        j.wait(lambda: any('Which conversation?' in m['text'] for m,_ in j.messages[before:]))
        question = next(m for m,_ in reversed(j.messages) if 'Which conversation?' in m['text'])
        j.admit('/cancel', topic=1, reply=question['message_id'])
        j.wait(lambda: j.has('Pending routing cancelled', 1))
        assert len(j.instructions(8)) == 1
        print('PASS duplicate names, stable clarification, immutable original return, resolved question replay refusal, cancellation')
    finally:
        j.release_question.set()
        j.close()


def rejected_generation(binary):
    j = Routing(binary)
    try:
        j.prepare()
        original = j.admit('#Alpha Original work', topic=1)
        j.wait(lambda: len(j.instructions(7)) == 1 and j.instructions(7)[0][1] == 'accepted')
        old = j.runs['native-topic-7']
        j.reject_next = True
        j.admit('First changed requirement', topic=1, reply=original)
        j.admit('Second changed requirement', topic=1, reply=original)
        j.wait(lambda: len(j.instructions(7)) == 3)
        j.broadcast('native-topic-7', 'turn/completed', {'turn': {'id': old, 'status': 'completed'}})
        j.wait(lambda: all(row[1] == 'accepted' for row in j.instructions(7)))
        docs = [json.loads(r[2]) for r in j.instructions(7)]
        assert docs[1]['native_turn'] == docs[2]['native_turn'] != old
        starts = [r for r in j.rpc if r.get('method') == 'turn/start' and r['params']['threadId'] == 'native-topic-7' and r['params'].get('clientUserMessageId') in {doc['id'] for doc in docs}]
        assert len(starts) == 2, starts
        j.answer(7, 'Latest requirements incorporated')
        j.wait(lambda: j.has('Latest requirements incorporated', 1))
        j.idle()
        print('PASS rejected generation queues one continuation with both corrections and original request identity')
    finally:
        j.release_question.set()
        j.close()


if __name__ == '__main__':
    binary = str(pathlib.Path(sys.argv[1]).resolve())
    parallel_and_corrections(binary)
    clarification(binary)
    rejected_generation(binary)
