#!/usr/bin/env python3
"""Internal native workflow returns through the real wrapper and synthetic peer.
This proves wiring/receipts/audiences, not real native admission or incorporation.
"""
from datetime import datetime, timedelta, timezone
import json
import signal
import threading
from pathlib import Path
import sys
from workflow_io import WorkflowJourney


class Returning(WorkflowJourney):
    def __init__(self, binary):
        self.internal_packets = []
        self.internal = {}
        self.return_turns = {}
        super().__init__(binary)

    def stop(self):
        if self.process:
            self.process.send_signal(signal.SIGINT)
            self.last_stdout, self.last_stderr = self.process.communicate(timeout=8)
            assert 'synthetic-question-token' not in self.last_stderr
            self.process = None

    def close(self):
        if sys.exc_info()[0]:
            self.stop()
            print('Wrapper stdout:', getattr(self, 'last_stdout', '')[-12000:], file=sys.stderr)
            print('Wrapper stderr:', getattr(self, 'last_stderr', '')[-12000:], file=sys.stderr)
            print('Retained turns:', self.row('SELECT id,status,substr(assistant_text,1,500) FROM turns ORDER BY id DESC LIMIT 12'), file=sys.stderr)
            print('Return receipts:', self.row("SELECT json_extract(value,'$.phase') FROM interaction_workflows,json_each(doc,'$.native_returns')"), file=sys.stderr)
            print('Last RPC methods:', [(v.get('method'), v.get('id')) for v in self.rpc[-8:]], file=sys.stderr)
        super().close()

    def wait(self, condition, seconds=15):
        try:
            return super().wait(condition, seconds)
        except AssertionError as error:
            raise AssertionError(str(error).split('; messages=', 1)[0][:1000]) from None

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
        native = [item for item in page['items'] if item['kind'] == 'integrator_answer'
                  and item['preview'].startswith('Integrated selected workflow event:')]
        assert any(item['preview'] == 'Initial goal turn delivered.' for item in page['items']), 'Seeded canonical history was erased'
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


class CompletionRace(Returning):
    def __init__(self, binary):
        self.recovery_seen = threading.Event()
        self.rollout = None
        super().__init__(binary)

    def handle_extra_rpc(self, message, send):
        method = message.get('method'); p = message.get('params', {}); ident = message.get('id')
        if method == 'turn/start' and 'toolOutput' in p:
            assert p['input'] == [] and 'clientUserMessageId' not in p
            self.internal_packets.append(p)
            self.old_turn = self.web_turn
            self.detached = 'exact-detached-turn'
            self.rollout = self.root / 'owned-recovery.jsonl'
            rows = [{'type': 'session_meta', 'payload': {'id': self.web_thread}},
                    {'type': 'event_msg', 'payload': {'type': 'task_started', 'turn_id': self.detached}}]
            self.rollout.write_text(''.join(json.dumps(row) + '\n' for row in rows))
            send({'id': ident, 'result': {'turn': {'id': self.detached}}})
            self.broadcast(self.web_thread, 'turn/started', {'turn': {'id': self.detached, 'status': 'inProgress'}})
            self.broadcast(self.web_thread, 'item/completed', {'turnId': self.detached, 'item': {
                'id': 'new-physical-private-progress', 'type': 'agentMessage', 'phase': 'commentary',
                'text': 'A different physical writer is active.'}})
            self.broadcast(self.web_thread, 'item/completed', {'turnId': self.old_turn, 'item': {
                'id': 'old-human-final', 'type': 'agentMessage', 'phase': 'final_answer',
                'text': 'Original human collector retained its own answer.'}})
            self.broadcast(self.web_thread, 'turn/completed', {'turn': {'id': self.old_turn, 'status': 'completed'}})
            return True
        if self.rollout and method == 'thread/turns/list' and p['threadId'] == self.web_thread:
            send({'id': ident, 'error': {'code': -32601, 'message': 'list_turns is unsupported'}})
            return True
        if self.rollout and method == 'thread/read' and p['threadId'] == self.web_thread:
            assert p['includeTurns'] is False
            send({'id': ident, 'result': {'thread': {'id': self.web_thread, 'path': str(self.rollout)}}})
            self.recovery_seen.set()
            return True
        return super().handle_extra_rpc(message, send)


def qualify_completion_race(binary):
    j = CompletionRace(binary)
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
        j.messages.clear()
        original, _ = j.submit(binding, 'Produce the selected checkpoint and return the final integration here.')
        j.original = original['id']
        j.wait(lambda: j.row('SELECT phase FROM conversation_inputs WHERE id=?', (j.original,)) == [('accepted',)])
        plan = {'input_ref': j.original, 'request_id': 'completion-race-workflow',
                'expires_at': (datetime.now(timezone.utc) + timedelta(hours=1)).isoformat(), 'steps': [
                    {'id': 'produce', 'selector': 't7', 'after': None, 'scope': 'Produce the selected checkpoint.',
                     'checkpoint': 'Exact checkpoint', 'milestone': None}]}
        view = j.call('route-race-declare', 'workflow_declare', plan)
        j.call('route-race-start', 'workflow_advance', {'workflow': view['id'], 'revision': view['revision']})
        j.wait(lambda: j.instructions(7) and j.instructions(7)[0][1] == 'accepted')
        j.answer_worker(7, 'Selected predecessor evidence.')
        j.wait(lambda: len(j.internal_packets) == 1)
        j.wait(lambda: j.row('SELECT busy FROM sessions WHERE thread_id=1') == [(0,)])
        doc = json.loads(j.row('SELECT doc FROM interaction_workflows')[0][0])
        receipt = doc['native_returns'][0]
        assert receipt['phase'] == 'accepted_uncollected'
        old, local = receipt['local'], receipt['collection_local']
        assert old != local and receipt['native_turn'] == j.detached
        assert j.row('SELECT first_turn FROM native_collections WHERE local_turn=?', (old,)) == [(j.old_turn,)]
        assert j.row('SELECT status FROM turns WHERE id=?', (old,)) == [('completed',)]
        # Two actual existing heartbeat observations establish active then terminal
        # recovery. No new wakeup, timer or provider input is installed for this test.
        j.wait(lambda: j.row("SELECT detail FROM native_collections WHERE local_turn=?", (local,)) == [('Exact workflow turn active; terminal canonical result pending',)], seconds=40)
        assert j.recovery_seen.is_set()
        rows = [{'type': 'response_item', 'payload': {'type': 'message', 'id': 'late-native-final',
                 'role': 'assistant', 'phase': 'final_answer', 'content': [{'type': 'output_text', 'text': 'Late canonical workflow report.'}]}},
                {'type': 'event_msg', 'payload': {'type': 'task_complete', 'turn_id': j.detached}}]
        with j.rollout.open('a') as stream:
            stream.write(''.join(json.dumps(row) + '\n' for row in rows))
        j.wait(lambda: j.row('SELECT phase FROM native_collections WHERE local_turn=?', (local,)) == [('complete',)], seconds=40)
        page = j.read('presentation', 'list', '0', '0')['result']
        late = [item for item in page['items'] if item['preview'] == 'Late canonical workflow report.']
        assert len(late) == 1 and late[0]['local_turn'] == local and late[0]['current']
        assert late[0]['native_item'] == 'late-native-final' and late[0]['native_turn'] == j.detached
        assert any(item['preview'] == 'Original human collector retained its own answer.' and item['local_turn'] == old for item in page['items'])
        assert not any(item['preview'] == 'A different physical writer is active.' for item in page['items'])
        doc = json.loads(j.row('SELECT doc FROM interaction_workflows')[0][0])
        assert doc['native_returns'][0]['phase'] == 'accepted_reconciled'
        assert doc['phase'] == 'active', 'A recovered helper report completed overall integration'
        assert len(j.internal_packets) == 1 and j.row('SELECT count(*) FROM conversation_inputs') == [(1,)]
        assert j.row('SELECT count(*) FROM incoming_updates') == journal and not j.messages
        assert j.row('SELECT first_turn FROM native_collections WHERE local_turn=?', (old,)) == [(j.old_turn,)]
        print('PASS completion-race exact ACK → distinct finite observation → owned regular rollout fallback → late canonical final in same Conversation; old human collector preserved, no source replay or fabricated input; synthetic native boundary, zero models')
    finally:
        j.close()


if __name__ == '__main__':
    executable = str(Path(sys.argv[1]).resolve())
    qualify(executable)
    qualify_completion_race(executable)
