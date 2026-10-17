#!/usr/bin/env python3
"""Integrating conversation through real HTTP/native proxy/SQLite boundaries.
Every human, token, model, conversation and peer here is synthetic.
"""
import json
import pathlib
import re
import sqlite3
import sys
import threading
from goals_io import Goals


class Routing(Goals):
    def __init__(self, binary):
        self.question_held = threading.Event()
        self.release_question = threading.Event()
        self.declare_task = False
        self.block_typing = False
        self.typing_held = threading.Event()
        self.release_typing = threading.Event()
        self.drop_result = False
        self.cooldown_once = False
        self.cooldown_seen = threading.Event()
        self.reject_next = False
        self.retire_on_reject = False
        self.rejected_turn = None
        self.rejected_inputs = set()
        self.routed = False
        self.block_binding = False
        self.allow_native_launch = False
        self.block_legacy_progress = False
        self.legacy_progress_held = threading.Event()
        self.release_legacy_progress = threading.Event()
        self.block_goal_pause = False
        self.goal_pause_held = threading.Event()
        self.release_goal_pause = threading.Event()
        self.binding_held = threading.Event()
        self.release_binding = threading.Event()
        self.packets = {}
        self.runs = {}
        self.activity_unavailable = False
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
        if self.routed and m.get('method') == 'thread/read':
            if self.activity_unavailable:
                send({'id':m['id'],'error':{'code':-32000,'message':'Synthetic disconnected native status'}})
            else:
                send({'id':m['id'],'result':{'thread':{'id':m['params']['threadId'],'status':{'type':'active'}}}})
            return True
        if self.block_goal_pause and m.get('method') == 'thread/goal/set' and m.get('params',{}).get('status') == 'paused' and any(active_send is send for _,active_send in list(self.clients.values())):
            self.goal_pause_held.set()
            self.release_goal_pause.wait(timeout=30)
        if self.routed and self.block_binding and m.get('method') == 'thread/resume':
            self.binding_held.set()
            self.release_binding.wait(timeout=30)
        if self.routed and self.allow_native_launch and m.get('method') == 'turn/start' and m['params'].get('clientUserMessageId','').startswith('telegram:'):
            p = m['params']
            thread = p['threadId']
            turn = 'legacy-'+str(len(self.runs)+1)
            self.runs[thread] = turn
            self.packets.setdefault(thread, []).append(p)
            send({'id':m['id'],'result':{'turn':{'id':turn}}})
            self.broadcast(thread,'turn/started',{'turn':{'id':turn,'status':'inProgress'}})
            return True
        if self.routed and m.get('id') == 900 and not m.get('method'):
            self.answers.append(m)
            return True
        if not self.routed or m.get('method') not in ('turn/start', 'turn/steer', 'turn/interrupt'):
            return False
        p = m['params']
        thread = p['threadId']
        if m['method'] in ('turn/start', 'turn/steer'):
            text = '\n'.join(part.get('text', '') for part in p['input'])
            provenance = json.loads(next(line.removeprefix('Transport provenance: ') for line in text.splitlines() if line.startswith('Transport provenance: ')))
            assert text.startswith('[Interaction controller]')
            assert provenance['via'] == 'interaction-controller'
            if provenance.get('author') is None:
                assert provenance['original_human_author'] == 100
                assert provenance['source_channel'] == 'telegram:123'
            else:
                assert provenance['original_human_author'] is None
                assert provenance['authorized_human'] == 100
                assert provenance['author']['kind'] in ('session_request','session_result')
                assert provenance['source_channel'] in ('local-session','local-session-result')
            assert provenance['source_message']['thread_id'] in (1, 7, 8)
        if m['method'] == 'turn/interrupt':
            assert p['turnId'] == self.runs[thread]
            send({'id': m['id'], 'result': {}})
            self.broadcast(thread, 'turn/completed', {'turn': {'id': p['turnId'], 'status': 'interrupted'}})
        elif m['method'] == 'turn/start':
            if self.declare_task:
                self.declare_task = False
                context = p['additionalContext']['telecodex.runtime']['value']
                manifest = pathlib.Path(re.search(r'explicitly declare it in (\S+human-requests\.json)', context).group(1))
                manifest.write_text(json.dumps({'schema': 'telecodex.human-requests/v1', 'requests': [
                    {'id': 'physical-fixture', 'title': 'Physical fixture observation',
                     'action': 'Synthetic required observation; no real human is contacted.', 'links': []}]}))
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
                if self.retire_on_reject:
                    self.retire_on_reject = False
                    with sqlite3.connect(self.root / 'state.sqlite') as db:
                        db.execute("INSERT OR REPLACE INTO bot_state(key,value) VALUES('stop_admission_epoch:100','fixture-retired-generation')")
                send({'id': m['id'], 'error': {'code': -32600, 'message': 'Synthetic finished generation'}})
                self.rejected_inputs.add(p['clientUserMessageId'])
            else:
                send({'id': m['id'], 'result': {'turnId': self.runs[thread]}})
        return True

    def before_send_chat_action_reply(self, payload):
        if self.block_typing:
            self.typing_held.set()
            self.release_typing.wait(timeout=10)

    def before_send_message_reply(self, payload):
        if self.block_legacy_progress and 'Legacy work progress' in payload['text']:
            self.legacy_progress_held.set()
            self.release_legacy_progress.wait(timeout=30)
        if self.drop_result and 'Lost receipt result' in payload['text']:
            self.drop_result = False
            return False
        if self.cooldown_once and 'Interaction status' in payload['text']:
            self.cooldown_once = False
            self.cooldown_seen.set()
            return {'ok': False, 'error_code': 429, 'description': 'Synthetic cooldown',
                    'parameters': {'retry_after': 3}}
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
        j.wait(lambda: j.has('does not report incorporating your correction', 1))
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
        # Admission precedes native transport. End the old turn only after
        # both matched rejection responses have actually been requeued; ending
        # it earlier correctly yields uncertain input rather than safe replay.
        j.wait(lambda: len(j.instructions(7)) == 3 and all(
            row[0] in j.rejected_inputs and row[1] == 'queued'
            for row in j.instructions(7)[1:]))
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


def publication_recovery(binary):
    j = Routing(binary)
    try:
        j.prepare()
        original = j.admit('#Alpha Retain a known-unsent output', topic=1)
        j.wait(lambda: len(j.instructions(7)) == 1 and j.instructions(7)[0][1] == 'accepted')
        j.cooldown_once = True
        j.admit('/interaction', topic=1)
        j.wait(lambda: j.cooldown_seen.is_set())
        j.answer(7, 'Known-unsent canonical result')
        j.wait(lambda: j.row("SELECT phase FROM interaction_deliveries") == [('preparing',)])
        j.stop()
        assert j.row('SELECT phase FROM interaction_deliveries') == [('preparing',)]
        assert not j.has('Known-unsent canonical result', 1)
        j.start()
        j.admit('/interaction', topic=1)
        j.wait(lambda: j.row('SELECT phase FROM interaction_deliveries') == [('reserved',)])
        j.wait(lambda: j.has('Result: reserved', 1))
        page = j.admit('/interaction status 1', topic=1)
        j.wait(lambda: any(p.get('reply_parameters', {}).get('message_id') == page for _,p in j.messages))
        page_text = next(m['text'] for m,p in j.messages if p.get('reply_parameters', {}).get('message_id') == page)
        assert '#Beta (#t8)' in page_text and '#Alpha (#t7)' not in page_text
        delivery = j.row('SELECT id FROM interaction_deliveries')[0][0]
        before_starts = sum(m.get('method') == 'turn/start' for m in j.rpc)
        j.admit('/interaction deliver ' + delivery, topic=1)
        j.wait(lambda: j.row('SELECT phase FROM interaction_deliveries') == [('delivered',)])
        actual = [(m,p) for m,p in j.messages if m['text'].startswith('#Alpha') and 'Known-unsent canonical result' in m['text']]
        assert len(actual) == 1 and actual[0][1]['reply_parameters']['message_id'] == original
        j.admit('/interaction result ' + delivery, topic=1)
        j.wait(lambda: j.has('Retained worker result for #Alpha', 1))
        j.admit('/interaction deliver ' + delivery, topic=1)
        j.wait(lambda: j.has('Result is delivered.', 1))
        assert sum(m.get('method') == 'turn/start' for m in j.rpc) == before_starts
        assert len(j.instructions(7)) == 1
        print('PASS restart before transmission, explicit unsent result recovery, original reply, read-only inspection, delivered replay refusal')
    finally:
        j.release_question.set()
        j.close()

    j = Routing(binary)
    try:
        j.prepare()
        j.admit('#Beta A result with a lost transmission receipt', topic=1)
        j.wait(lambda: len(j.instructions(8)) == 1 and j.instructions(8)[0][1] == 'accepted')
        j.drop_result = True
        j.answer(8, 'Lost receipt result')
        j.wait(lambda: j.row('SELECT phase FROM interaction_deliveries') == [('uncertain',)])
        j.wait(lambda: j.has('result transmission is unconfirmed', 1))
        delivery = j.row('SELECT id FROM interaction_deliveries')[0][0]
        j.stop()
        j.start()
        j.admit('/interaction deliver ' + delivery, topic=1)
        j.wait(lambda: j.has('Result is uncertain.', 1))
        j.admit('/interaction result ' + delivery, topic=1)
        j.wait(lambda: j.has('Transmission: uncertain', 1))
        attempted = [m for m,_ in j.messages if m['text'].startswith('#Beta') and 'Lost receipt result' in m['text']]
        assert len(attempted) == 1
        assert j.row('SELECT phase FROM interaction_deliveries') == [('uncertain',)]
        assert len(j.instructions(8)) == 1
        print('PASS lost HTTP receipt, explicit uncertainty notice, restart without replay, retained canonical inspection')
    finally:
        j.release_question.set()
        j.close()


def cosmetic_wait_does_not_delay_next_turn(binary):
    j = Routing(binary)
    try:
        j.prepare()
        j.block_typing = True
        j.admit('#Alpha First work', topic=1)
        j.wait(lambda: j.typing_held.is_set() and len(j.instructions(7)) == 1 and j.instructions(7)[0][1] == 'accepted')
        j.answer(7, 'First result')
        j.wait(lambda: j.row('SELECT count(*) FROM sessions WHERE busy=1') == [(0,)], seconds=3)
        j.admit('#Alpha Next independent request', topic=1)
        j.wait(lambda: len(j.instructions(7)) == 2 and j.instructions(7)[1][1] == 'accepted', seconds=3)
        assert not j.release_typing.is_set()
        j.release_typing.set()
        j.answer(7, 'Second result', item='second')
        j.idle()
        print('PASS delayed typing receipt does not delay turn settlement or next native input')
    finally:
        j.release_typing.set()
        j.release_question.set()
        j.close()


def routed_human_declaration(binary):
    j = Routing(binary)
    try:
        j.prepare()
        j.declare_task = True
        j.admit('#Alpha Work requiring an external fixture observation', topic=1)
        j.wait(lambda: len(j.instructions(7)) == 1 and j.instructions(7)[0][1] == 'accepted')
        j.answer(7, 'Automated work finished; fixture observation remains')
        j.wait(lambda: j.has('Human-action declaration recorded in #Alpha', 1))
        assert j.row("SELECT s.thread_id,h.requester,h.kind,h.state FROM human_interactions h JOIN sessions s ON s.id=h.session_id") == [(7,100,'task','pending')]
        def listed(rows):
            return next(((m,p) for m,p in rows if any('Physical fixture observation' in b.get('text','') for row in p.get('reply_markup',{}).get('inline_keyboard',[]) for b in row)), None)
        j.admit('/pending', topic=7)
        j.wait(lambda: listed(j.messages) is not None)
        control=listed(j.messages)
        j.admit(callback=j.button(control,':open'),topic=7)
        j.wait(lambda: j.has('Human request: Physical fixture observation',7))
        j.idle()
        j.stop()
        j.start()
        before=len(j.messages)
        j.admit('/pending', topic=7)
        j.wait(lambda: listed(j.messages[before:]) is not None)
        assert len(j.instructions(7)) == 1
        print('PASS routed required-action declaration retains actual worker ownership and survives restart in existing pending UI')
    finally:
        j.release_question.set()
        j.close()


def preparation_wait_is_visible(binary):
    j = Routing(binary)
    try:
        j.prepare()
        j.block_binding = True
        original = j.admit('#Alpha Work after native preparation', topic=1)
        j.wait(j.binding_held.is_set)
        j.wait(lambda: j.has('native submission has not started', 1), 15)
        assert j.instructions(7)[0][1] == 'queued'
        waiting = next((m,p) for m,p in j.messages if 'native submission has not started' in m['text'])
        assert waiting[1]['reply_parameters']['message_id'] == original
        assert not j.runs, 'A preparation notice must not submit native work'
        j.admit('/interaction', topic=1)
        j.wait(lambda: j.has('Interaction status', 1))
        j.release_binding.set()
        j.wait(lambda: j.instructions(7)[0][1] == 'accepted')
        j.answer(7, 'Work completed after preparation')
        j.wait(lambda: j.has('Work completed after preparation', 1))
        j.idle()
        assert len(j.instructions(7)) == 1
        print('PASS preparation delay stays visible without cancellation or replay; entry remains usable')
    finally:
        j.release_binding.set()
        j.close()


def retired_admission_and_writer_hold(binary):
    j = Routing(binary)
    try:
        j.prepare()
        original = j.admit('#Alpha Initial work', topic=1)
        j.wait(lambda: len(j.instructions(7)) == 1 and j.instructions(7)[0][1] == 'accepted')
        with sqlite3.connect(j.root / 'state.sqlite') as db:
            db.execute("INSERT OR REPLACE INTO bot_state(key,value) VALUES('native_writer_hold:100:7','synthetic-maintenance')")
        j.admit('Correction during a held writer', topic=1, reply=original)
        j.wait(lambda: len(j.instructions(7)) == 2 and j.instructions(7)[1][1] == 'retained_unsent')
        held = j.instructions(7)[1][0]
        assert len(j.packets['native-topic-7']) == 1
        with sqlite3.connect(j.root / 'state.sqlite') as db:
            db.execute("DELETE FROM bot_state WHERE key='native_writer_hold:100:7'")
        j.admit('/interaction retry '+held, topic=1)
        j.wait(lambda: j.instructions(7)[1][1] == 'accepted')
        assert len(j.packets['native-topic-7']) == 2
        j.reject_next = j.retire_on_reject = True
        j.admit('Correction whose generation is retired after definite rejection', topic=1, reply=original)
        j.wait(lambda: len(j.instructions(7)) == 3 and j.instructions(7)[2][1] == 'retained_unsent')
        retired = j.instructions(7)[2][0]
        assert len(j.packets['native-topic-7']) == 3
        assert len(j.runs) == 1, 'Retired async fallback must not start another native turn'
        j.answer(7, 'Earlier effects remain; retired correction is not covered', covered=[r[0] for r in j.instructions(7)[:2]])
        j.idle()
        j.admit('/interaction retry '+retired, topic=1)
        j.wait(lambda: j.instructions(7)[2][1] == 'accepted')
        assert len(j.packets['native-topic-7']) == 4
        assert json.loads(j.instructions(7)[2][2])['admission_epoch'] == 'fixture-retired-generation'
        j.answer(7, 'Explicit recovery incorporated the retained correction', item='recovered')
        j.wait(lambda: j.has('Explicit recovery incorporated',1))
        j.idle()
        print('PASS writer hold and retired async generation retain input; only explicit recovery adopts fresh admission')
    finally:
        j.close()


def native_launch_joins_routing(binary, *, preparing=False):
    j = Routing(binary)
    try:
        j.prepare()
        j.allow_native_launch = True
        j.release_question.set()
        j.block_binding = preparing
        j.admit('/goal Complete isolated existing native work', topic=7)
        if preparing:
            j.wait(j.binding_held.is_set)
        else:
            j.wait(lambda: 'native-topic-7' in j.runs)
            j.block_legacy_progress = True
            j.broadcast('native-topic-7','item/completed',{'turnId':j.runs['native-topic-7'],'item':{'id':'legacy-progress','type':'agentMessage','phase':'commentary','text':'Legacy work progress'}})
            j.wait(j.legacy_progress_held.is_set)
        original = j.admit('#Alpha Correct the work already running: retain the rate 17 requirement', topic=1)
        j.wait(lambda: len(j.instructions(7)) == 1)
        if preparing:
            j.wait(lambda: j.instructions(7)[0][1] == 'steering')
            j.release_binding.set()
        j.wait(lambda: j.instructions(7)[0][1] == 'accepted')
        assert not j.release_legacy_progress.is_set()
        inputs = j.instructions(7)
        doc = json.loads(inputs[0][2])
        assert j.row('SELECT s.thread_id FROM turns t JOIN sessions s ON s.id=t.session_id WHERE t.id=?',(doc['local_turn'],)) == [(7,)]
        assert len([r for r in j.rpc if r.get('method') == 'turn/start' and r['params'].get('clientUserMessageId') == doc['id']]) == 0
        native = j.runs['native-topic-7']
        j.broadcast('native-topic-7','item/completed',{'turnId':native,'item':{'id':'legacy-earlier','type':'agentMessage','phase':'final_answer','text':'Legacy earlier answer without new input coverage'}})
        j.wait(lambda: j.row("SELECT count(*) FROM interaction_native_messages WHERE item_id='legacy-earlier'") == [(1,)])
        assert j.row('SELECT count(*) FROM interaction_deliveries') == [(0,)]
        j.wait(lambda: j.has('remains unresolved',1))
        assert not j.has('Legacy earlier answer',1)
        for selected,send in list(j.clients.values()):
            if selected == 'native-topic-7':
                send({'id':900,'method':'item/tool/requestUserInput','params':{'threadId':selected,'turnId':native,'itemId':'mixed-question','isBlocking':True,'questions':[{'id':'scope','header':'Scope','question':'Confirm the new refinement?','isOther':True,'isSecret':False,'options':[{'label':'Small','description':'One step'}]}]}})
        j.wait(lambda: j.row('SELECT count(*) FROM question_messages') == [(1,)])
        j.wait(lambda: j.controls(1,topic=1) is not None)
        question = j.controls(1,topic=1)
        j.admit(callback=j.button(question,':0'),topic=1)
        j.wait(lambda: len(j.answers) == 1)
        j.broadcast('native-topic-7','serverRequest/resolved',{'requestId':900})
        j.release_legacy_progress.set()
        j.goals['native-topic-7'].update(status='complete')
        j.broadcast('native-topic-7','thread/goal/updated',{'goal':j.goals['native-topic-7']})
        j.answer(7,'Mixed native work incorporated the routed correction')
        j.wait(lambda: j.has('Mixed native work incorporated',1) and j.has('Mixed native work incorporated',7))
        j.idle()
        assert len(j.instructions(7)) == 1
        assert j.row('SELECT count(*) FROM turns t JOIN sessions s ON s.id=t.session_id WHERE s.thread_id=7') == [(2,)]
        returned = [(m,p) for m,p in j.messages if 'Mixed native work incorporated' in m['text'] and m.get('message_thread_id') == 1]
        assert len(returned) == 1 and returned[0][1]['reply_parameters']['message_id'] == original
        assert not any('daimon-result' in m['text'] for m,_ in j.messages)
        print('PASS existing native goal joins routing '+('during preparation' if preparing else 'while its original presentation is stalled')+'; exact return, explicit coverage and question destination preserved')
    finally:
        j.release_binding.set()
        j.release_legacy_progress.set()
        j.close()


def admission_is_checked_at_native_write(binary):
    j = Routing(binary)
    try:
        j.prepare()
        j.allow_native_launch = True
        j.admit('/goal Keep existing work separate from later input',topic=7)
        j.wait(lambda: 'native-topic-7' in j.runs and j.goals.get('native-topic-7',{}).get('status') == 'active')
        j.block_goal_pause = True
        j.admit('/goal pause',topic=7)
        j.wait(j.goal_pause_held.is_set)
        j.admit('#Alpha Input admitted before a later writer hold',topic=1)
        j.wait(lambda: len(j.instructions(7)) == 1 and j.instructions(7)[0][1] == 'steering')
        assert len(j.packets['native-topic-7']) == 1
        with sqlite3.connect(j.root / 'state.sqlite') as db:
            db.execute("INSERT OR REPLACE INTO bot_state(key,value) VALUES('native_writer_hold:100:7','synthetic-late-hold')")
        j.release_goal_pause.set()
        j.wait(lambda: j.instructions(7)[0][1] == 'retained_unsent')
        assert len(j.packets['native-topic-7']) == 1, 'A queued steer crossed its retired native write boundary'
        with sqlite3.connect(j.root / 'state.sqlite') as db:
            db.execute("DELETE FROM bot_state WHERE key='native_writer_hold:100:7'")
        j.goals['native-topic-7'].update(status='complete')
        j.broadcast('native-topic-7','thread/goal/updated',{'goal':j.goals['native-topic-7']})
        native = j.runs['native-topic-7']
        j.broadcast('native-topic-7','item/completed',{'turnId':native,'item':{'id':'legacy-untouched','type':'agentMessage','phase':'final_answer','text':'Original work remained separate'}})
        j.broadcast('native-topic-7','turn/completed',{'turn':{'id':native,'status':'completed'}})
        j.idle()
        assert j.instructions(7)[0][1] == 'retained_unsent'
        print('PASS late native writer hold rejects a queued steer before RPC without interrupting original work')
    finally:
        j.release_goal_pause.set()
        j.close()


def observer_status_is_read_only(binary):
    j = Routing(binary)
    try:
        j.prepare()
        j.declare_task = True
        j.admit('#Alpha Work with a retained external observation',topic=1)
        j.wait(lambda: len(j.instructions(7)) == 1 and j.instructions(7)[0][1] == 'accepted')
        j.answer(7,'Work with retained observation finished')
        j.idle()
        j.wait(lambda: j.row('SELECT count(*) FROM human_interactions') == [(1,)])
        before = len(j.rpc)
        instructions = j.instructions(7)
        with sqlite3.connect(j.root / 'state.sqlite') as db:
            local = json.loads(instructions[0][2])['local_turn']
            db.execute('INSERT INTO bot_state(key,value) VALUES(?,?)',(f'interaction_presentation_gap:{local}','retained-native-items'))
        j.admit('/interaction',topic=1)
        j.wait(lambda: j.has('Presentation incomplete',1))
        text = '\n'.join(m['text'] for m,_ in j.messages if m.get('message_thread_id') == 1 and 'Interaction status' in m['text'])
        assert 'Physical fixture observation' in text and 'Needs attention: 1 (1 pending)' in text
        assert 'Local source cut:' in text and 'Last retained native item:' in text
        assert 'Native activity: active (sampled' in text
        assert 'native history has not been reconciled' in text
        assert j.instructions(7) == instructions
        assert not any(r.get('method') in ('turn/start','turn/steer','thread/resume') for r in j.rpc[before:])
        with sqlite3.connect(j.root / 'state.sqlite') as db:
            db.execute("UPDATE sessions SET codex_thread_id='replacement-native' WHERE thread_id=7")
        j.messages.clear()
        j.admit('/interaction',topic=1)
        j.wait(lambda: j.has('historical worker coverage for prior binding',1))
        text = '\n'.join(m['text'] for m,_ in j.messages if m.get('message_thread_id') == 1)
        assert 'Input: accepted (prior binding; native native-topic-7/' in text
        assert 'Source: native-topic-7/' in text and 'Native source binding: replacement-native' in text
        j.activity_unavailable = True
        j.messages.clear()
        j.admit('/interaction',topic=1)
        j.wait(lambda: j.has('unknown; unavailable or changed source',1))
        assert not j.has('Native activity: idle',1)
        assert j.instructions(7) == instructions
        print('PASS observer pending/source cuts/gaps and disconnected activity are attributable read-only observations; no native work or false idle')
    finally:
        j.close()


if __name__ == '__main__':
    binary = str(pathlib.Path(sys.argv[1]).resolve())
    parallel_and_corrections(binary)
    clarification(binary)
    rejected_generation(binary)
    publication_recovery(binary)
    cosmetic_wait_does_not_delay_next_turn(binary)
    routed_human_declaration(binary)
    preparation_wait_is_visible(binary)
    retired_admission_and_writer_hold(binary)
    native_launch_joins_routing(binary)
    native_launch_joins_routing(binary,preparing=True)
    admission_is_checked_at_native_write(binary)
    observer_status_is_read_only(binary)
