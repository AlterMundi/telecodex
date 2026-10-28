#!/usr/bin/env python3
"""Ordinary canonical result/question returns through the native wrapper.
Synthetic peers/accounts only; this does not claim real-provider or hearing acceptance.
"""
import json
from pathlib import Path
import sys
from conversation_routing_io import CanonicalRouting
from workflow_native_returns_io import Returning


class RoutedReturning(Returning):
    def __init__(self, binary):
        self.fail_resume = False
        self.failed_resumes = 0
        super().__init__(binary)

    def handle_extra_rpc(self, message, send):
        method = message.get('method'); p = message.get('params', {}); ident = message.get('id')
        if self.fail_resume and method == 'thread/resume' and p.get('threadId') == self.web_thread:
            self.failed_resumes += 1
            send({'id': ident, 'error': {'code': -32000, 'message': 'Synthetic metadata unavailable before native input'}})
            return True
        if method == 'turn/start' and 'toolOutput' in p:
            assert p['input'] == [] and 'clientUserMessageId' not in p
            assert p['toolOutput']['name'] == 'conversation_attention'
            assert p['toolOutput']['retain'] and p['toolOutput']['namespace'] is None
            output = json.loads(p['toolOutput']['output'])
            assert output['schema'] == 'telecodex.native-attention/v1'
            assert output['original_input_ref'] == self.original and 'workflow' not in output
            assert output['evidence']['internal_evidence'] and not output['evidence']['publication_grant']
            self.internal_packets.append(p)
            turn = self.web_turn if len(self.internal_packets) == 1 else 'ordinary-return-' + str(len(self.internal_packets))
            self.internal[turn] = (send, output)
            send({'id': ident, 'result': {'turn': {'id': turn}}})
            if len(self.internal_packets) > 1:
                self.web_turn = turn
                self.broadcast(self.web_thread, 'turn/started', {'turn': {'id': turn, 'status': 'inProgress'}})
            call = 'route-ordinary-source-' + turn
            send({'id': call, 'method': 'item/tool/call', 'params': {
                'threadId': self.web_thread, 'turnId': turn, 'callId': call,
                'tool': 'interaction_controller', 'arguments': {'operation': 'attention_source', 'arguments': {
                    'return_id': output['return_id'], 'revision': output['evidence']['revision']}}}})
            return True
        if method is None and str(ident).startswith('route-ordinary-source-'):
            assert message['result']['success'], message
            source = json.loads(message['result']['contentItems'][0]['text'])
            turn = str(ident).removeprefix('route-ordinary-source-')
            send, output = self.internal[turn]
            if output['evidence']['source']['kind'] == 'native_question':
                assert 'Which receiving target?' in source['text'] and source['answer_allowed'] is False
            else:
                assert source['text'] in ('Ordinary Alpha result.', 'Ordinary Beta result.')
            call = 'route-ordinary-report-' + turn
            send({'id': call, 'method': 'item/tool/call', 'params': {
                'threadId': self.web_thread, 'turnId': turn, 'callId': call,
                'tool': 'interaction_controller', 'arguments': {'operation': 'attention_reported', 'arguments': {
                    'return_id': output['return_id'], 'revision': output['evidence']['revision']}}}})
            return True
        if method is None and str(ident).startswith('route-ordinary-report-'):
            assert message['result']['success'], message
            turn = str(ident).removeprefix('route-ordinary-report-')
            self.broadcast(self.web_thread, 'item/completed', {'turnId': turn, 'item': {
                'id': 'ordinary-offer-' + turn, 'type': 'agentMessage', 'phase': 'final_answer',
                'text': 'Ordinary integrating offer: ' + self.internal[turn][1]['evidence']['reason']}})
            self.broadcast(self.web_thread, 'turn/completed', {'turn': {'id': turn, 'status': 'completed'}})
            return True
        # Preserve actual-human worker provenance; the workflow-only fixture
        # intentionally rejects this source kind and is not our parent handler.
        return CanonicalRouting.handle_extra_rpc(self, message, send)


def setup(binary, enabled):
    j = RoutedReturning(binary)
    try:
        j.start()
        for topic in (7, 8, 1):
            j.admit('Seed synthetic owned context', topic=topic)
            j.wait(lambda: j.has('Initial goal turn delivered.', topic)); j.idle()
        j.stop()
        config = j.config.read_text() + j.target_config
        if enabled:
            config = config.replace('[interaction]\n', '[interaction]\nnative_routed_returns=true\n')
        j.config.write_text(config)
        j.start(); j.wait_control_socket(j.socket); j.routing = True
        page = j.read('presentation', 'list', '0', '0')['result']
        assert page['workflow_attention']['automatic_routed_native_return'] is enabled
        binding = page['binding']
        j.journal = j.row('SELECT count(*) FROM incoming_updates')
        j.messages.clear(); j.telegram_methods.clear()
        original, _ = j.submit(binding, 'Alpha and Beta independently return their selected results here.')
        j.original = original['id']
        j.wait(lambda: j.row('SELECT phase FROM conversation_inputs WHERE id=?', (j.original,)) == [('accepted',)])
        j.call('route-ordinary-dispatch', 'dispatch', {'input_ref': j.original, 'destinations': [
            {'selector': name, 'relation': 'new', 'requests': []} for name in ('Alpha', 'Beta')]})
        j.wait(lambda: all(j.instructions(t) and j.instructions(t)[0][1] == 'accepted' for t in (7, 8)))
        return j
    except BaseException:
        j.close()
        raise


def qualify(binary, recovery=False):
    j = setup(binary, True)
    try:
        # Visible commentary must not reserve an integrating model return.
        j.broadcast('native-topic-7', 'item/completed', {'turnId': j.workers['native-topic-7'], 'item': {
            'id': 'ordinary-progress', 'type': 'agentMessage', 'phase': 'commentary', 'text': 'Still working.'}})
        j.wait(lambda: any(i['preview'] == 'Still working.' for i in j.read('presentation', 'list', '0', '0')['result']['items']))
        assert not j.internal_packets
        j.answer_worker(7, 'Ordinary Alpha result.')
        j.wait(lambda: len(j.internal_packets) == 1)
        j.wait(lambda: j.row('SELECT busy FROM sessions WHERE thread_id=1') == [(0,)])
        if recovery:
            j.fail_resume = True
        send = next(send for selected, send in j.clients.values() if selected == 'native-topic-8')
        send({'id': 'canonical-question', 'method': 'item/tool/requestUserInput', 'params': {
            'threadId': 'native-topic-8', 'turnId': j.workers['native-topic-8'], 'itemId': 'ordinary-choice',
            'isBlocking': True, 'questions': [{'id': 'target', 'header': 'Target', 'question': 'Which receiving target?',
                                             'isOther': True, 'isSecret': False, 'options': None}]}})
        if recovery:
            j.wait(lambda: j.row("SELECT count(*) FROM human_interactions WHERE json_extract(native_return,'$.returns[0].phase')='retained_unsent'") == [(1,)])
            retained = json.loads(j.row('SELECT native_return FROM human_interactions WHERE native_return IS NOT NULL')[0][0])['returns'][0]
            assert j.failed_resumes > 0 and len(j.internal_packets) == 1
            j.fail_resume = False
        j.wait(lambda: len(j.internal_packets) == 2, seconds=35 if recovery else 15)
        j.wait(lambda: j.row('SELECT busy FROM sessions WHERE thread_id=1') == [(0,)])
        assert j.row("SELECT state FROM human_interactions WHERE json_extract(payload_json,'$.source.kind')='question'") == [('pending',)]
        assert not j.native_answers, 'An integrating offer answered/cancelled a native question'
        if recovery:
            receipt = json.loads(j.row('SELECT native_return FROM human_interactions WHERE native_return IS NOT NULL')[0][0])['returns'][0]
            assert receipt['id'] == retained['id'] and receipt['envelope'] == retained['envelope']
            assert receipt['attempts'] == 2 and receipt['phase'] == 'accepted'
        j.answer_worker(8, 'Ordinary Beta result.')
        j.wait(lambda: len(j.internal_packets) == 3)
        j.wait(lambda: j.row('SELECT busy FROM sessions WHERE thread_id=1') == [(0,)])
        page = j.read('presentation', 'list', '0', '0')['result']
        offers = [i for i in page['items'] if i['preview'].startswith('Ordinary integrating offer:')]
        assert len(offers) == 3, offers
        idle = [i for i in offers if i['native_turn'].startswith('ordinary-return-')]
        assert len(idle) == 2 and all(i.get('native_returns') for i in idle), idle
        # An active actual-human execution keeps its ordinary publication rights.
        # Only callback-only executions freeze an internal source-set grant.
        assert all('workflow_returns' not in i for i in offers)
        assert j.row('SELECT count(*) FROM interaction_workflows') == [(0,)]
        assert j.row('SELECT count(*) FROM conversation_inputs') == [(1,)]
        assert j.row('SELECT count(*) FROM incoming_updates') == j.journal and not j.messages
        assert all(len(j.worker_packets['native-topic-' + str(t)]) == 1 for t in (7, 8))
        j.idle()
        print('PASS ordinary active-result / idle-question / idle-result native returns; exact controller reads and incorporation, pending question preserved, no fabricated input/workflow or progress narration; recovery=' + str(recovery))
    finally:
        j.close()


def qualify_disabled(binary):
    j = setup(binary, False)
    try:
        j.answer_worker(7, 'Ordinary Alpha result.')
        j.answer_worker(8, 'Ordinary Beta result.')
        j.wait(lambda: j.row('SELECT count(*) FROM interaction_deliveries') == [(2,)])
        j.finish(j.web_thread, j.web_turn); j.idle()
        assert not j.internal_packets
        assert j.row('SELECT count(*) FROM interaction_deliveries WHERE native_return IS NOT NULL') == [(0,)]
        j.stop()
        j.config.write_text(j.config.read_text().replace('[interaction]\n', '[interaction]\nnative_routed_returns=true\n'))
        j.start(); j.wait_control_socket(j.socket)
        page = j.read('presentation', 'list', '0', '0')['result']
        assert sum(i['kind'] == 'worker_result' for i in page['items']) == 2
        assert j.row('SELECT count(*) FROM interaction_deliveries WHERE native_return IS NOT NULL') == [(0,)]
        assert not j.internal_packets and j.row('SELECT count(*) FROM interaction_workflows') == [(0,)]
        print('PASS default-off preserves canonical results; enabling/restart does not backfill or manufacture a workflow')
    finally:
        j.close()


if __name__ == '__main__':
    executable = str(Path(sys.argv[1]).resolve())
    qualify(executable)
    qualify(executable, recovery=True)
    qualify_disabled(executable)
