#!/usr/bin/env python3
"""Internal native workflow returns through the real wrapper and synthetic peer.
This proves wiring/receipts/audiences, not real native admission or incorporation.
"""
from datetime import datetime, timedelta, timezone
import json
from pathlib import Path
import sys
from workflow_io import WorkflowJourney


class Returning(WorkflowJourney):
    def __init__(self, binary):
        self.internal_packets = []
        self.internal = {}
        self.return_turns = {}
        super().__init__(binary)

    def handle_extra_rpc(self, message, send):
        method = message.get('method'); p = message.get('params', {}); ident = message.get('id')
        if method == 'turn/start' and 'toolOutput' in p:
            assert p['input'] == [] and 'clientUserMessageId' not in p
            assert p['toolOutput']['name'] == 'workflow_attention' and p['toolOutput']['retain']
            assert p['toolOutput']['namespace'] is None
            output = json.loads(p['toolOutput']['output'])
            assert output['schema'] == 'telecodex.workflow-native-return/v1'
            assert output['original_input_ref'] == self.original
            assert output['evidence']['internal_evidence'] and not output['evidence']['publication_grant']
            self.internal_packets.append(p)
            turn = self.web_turn if len(self.internal_packets) == 1 else 'idle-native-return'
            self.return_turns[turn] = output
            self.internal[turn] = (send, output)
            # First return joins the actual active human execution; the second
            # starts a new callback-only root after that execution has finished.
            send({'id': ident, 'result': {'turn': {'id': turn}}})
            if len(self.internal_packets) > 1:
                self.web_turn = turn
                self.broadcast(self.web_thread, 'turn/started', {'turn': {'id': turn, 'status': 'inProgress'}})
            call = 'route-internal-source-' + turn
            send({'id': call, 'method': 'item/tool/call', 'params': {
                'threadId': self.web_thread, 'turnId': turn, 'callId': call,
                'tool': 'interaction_controller', 'arguments': {'operation': 'workflow_result', 'arguments': {
                    'workflow': output['workflow'], 'step': output['evidence']['step'],
                    'revision': output['evidence']['source']['revision']}}}})
            return True
        if method is None and str(ident).startswith('route-internal-source-'):
            assert message['result']['success'], message
            source = json.loads(message['result']['contentItems'][0]['text'])
            turn = str(ident).removeprefix('route-internal-source-')
            send, output = self.internal[turn]
            assert source['text'] in ('Selected predecessor evidence.', 'Selected receiving verification.')
            call = 'route-internal-report-' + turn
            send({'id': call, 'method': 'item/tool/call', 'params': {
                'threadId': self.web_thread, 'turnId': turn, 'callId': call,
                'tool': 'interaction_controller', 'arguments': {'operation': 'workflow_reported', 'arguments': {
                    'workflow': output['workflow'], 'revision': output['evidence']['revision']}}}})
            return True
        if method is None and str(ident).startswith('route-internal-report-'):
            assert message['result']['success'], message
            turn = str(ident).removeprefix('route-internal-report-')
            if self.internal[turn][1]['evidence']['reason'] == 'integration_required':
                call = 'route-internal-status-' + turn
                send({'id': call, 'method': 'item/tool/call', 'params': {
                    'threadId': self.web_thread, 'turnId': turn, 'callId': call,
                    'tool': 'interaction_controller', 'arguments': {'operation': 'workflow_status',
                        'arguments': {'workflow': self.internal[turn][1]['workflow']}}}})
            else:
                self.finish_return(turn)
            return True
        if method is None and str(ident).startswith('route-internal-status-'):
            assert message['result']['success'], message
            view = json.loads(message['result']['contentItems'][0]['text'])
            assert view['phase'] == 'awaiting_integration'
            turn = str(ident).removeprefix('route-internal-status-')
            send = self.internal[turn][0]
            call = 'route-internal-finish-' + turn
            send({'id': call, 'method': 'item/tool/call', 'params': {
                'threadId': self.web_thread, 'turnId': turn, 'callId': call,
                'tool': 'interaction_controller', 'arguments': {'operation': 'workflow_finish',
                    'arguments': {'workflow': view['id'], 'revision': view['revision'],
                                  'checkpoint': 'Integrated the current receiving verification.'}}}})
            return True
        if method is None and str(ident).startswith('route-internal-finish-'):
            assert message['result']['success'], message
            self.finish_return(str(ident).removeprefix('route-internal-finish-'))
            return True
        return super().handle_extra_rpc(message, send)

    def finish_return(self, turn):
        self.broadcast(self.web_thread, 'item/completed', {'turnId': turn, 'item': {
            'id': 'integrated-' + turn, 'type': 'agentMessage', 'phase': 'final_answer',
            'text': 'Integrated selected workflow event: ' + self.internal[turn][1]['evidence']['reason']}})
        self.broadcast(self.web_thread, 'turn/completed', {'turn': {'id': turn, 'status': 'completed'}})


def qualify(binary):
    j = Returning(binary)
    try:
        j.start()
        for topic in (7, 8, 1):
            j.admit('Seed synthetic owned native context', topic=topic)
            j.wait(lambda: j.has('Initial goal turn delivered.', topic)); j.idle()
        j.stop()
        j.config.write_text(j.config.read_text().replace('[interaction]\n', '[interaction]\nnative_workflow_returns=true\n') + j.target_config)
        j.start(); j.wait_control_socket(j.socket); j.routing = True
        binding = j.read('presentation', 'list', '0', '0')['result']['binding']
        journal = j.row('SELECT count(*) FROM incoming_updates')
        j.messages.clear(); j.telegram_methods.clear()
        original, _ = j.submit(binding, 'A produces a checkpoint; B verifies it. Report the checkpoint and return here with the final verification.')
        j.original = original['id']
        j.wait(lambda: j.row('SELECT phase FROM conversation_inputs WHERE id=?', (j.original,)) == [('accepted',)])
        plan = {'input_ref': j.original, 'request_id': 'native-return-workflow',
                'expires_at': (datetime.now(timezone.utc) + timedelta(hours=1)).isoformat(), 'steps': [
                    {'id': 'produce', 'selector': 't7', 'after': None, 'scope': 'Produce the checkpoint.',
                     'checkpoint': 'Exact checkpoint', 'milestone': 'Checkpoint ready'},
                    {'id': 'verify', 'selector': 't8', 'after': 'produce', 'scope': 'Verify the exact checkpoint.',
                     'checkpoint': 'Receiving verification', 'milestone': None}]}
        view = j.call('route-declare', 'workflow_declare', plan)
        j.call('route-start', 'workflow_advance', {'workflow': view['id'], 'revision': view['revision']})
        j.wait(lambda: j.instructions(7) and j.instructions(7)[0][1] == 'accepted')
        j.answer_worker(7, 'Selected predecessor evidence.')
        j.wait(lambda: len(j.internal_packets) == 1)
        j.wait(lambda: j.row('SELECT busy FROM sessions WHERE thread_id=1') == [(0,)])
        j.wait(lambda: j.instructions(8) and j.instructions(8)[0][1] == 'accepted')
        j.answer_worker(8, 'Selected receiving verification.')
        j.wait(lambda: len(j.internal_packets) == 2)
        j.wait(lambda: j.row('SELECT busy FROM sessions WHERE thread_id=1') == [(0,)])
        page = j.read('presentation', 'list', '0', '0')['result']
        native = [item for item in page['items'] if item['kind'] == 'integrator_answer']
        assert len(native) == 2, native
        assert any(item.get('workflow_return') for item in native)
        assert not page['workflow_attention']['envelopes']
        assert j.row('SELECT count(*) FROM incoming_updates') == journal
        assert j.row('SELECT count(*) FROM conversation_inputs') == [(1,)]
        assert not j.messages, 'Internal context acquired an old Telegram publication binding'
        view = json.loads(j.row('SELECT doc FROM interaction_workflows')[0][0])
        assert view['phase'] == 'completed'
        assert len(view['native_returns']) == 2 and all(r['phase'] == 'accepted' for r in view['native_returns'])
        assert len(j.worker_packets['native-topic-7']) == len(j.worker_packets['native-topic-8']) == 1
        j.idle()
        print('PASS active internal turn/start + idle callback-only turn, actual controller source reads/offer acknowledgment, retained ACKs, same Conversation publication and no fabricated human/Telegram ingress; synthetic native boundary, zero models')
    finally:
        j.close()


if __name__ == '__main__':
    qualify(str(Path(sys.argv[1]).resolve()))
