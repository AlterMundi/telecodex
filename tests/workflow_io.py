#!/usr/bin/env python3
"""Finite workflow across controller/native-proxy/SQLite/presentation boundaries.
Every account, peer and model response is synthetic; no provider/model work.
"""
from datetime import datetime, timedelta, timezone
import json
from pathlib import Path
import sys
from conversation_routing_io import CanonicalRouting


class WorkflowJourney(CanonicalRouting):
    def handle_extra_rpc(self, message, send):
        method = message.get('method'); params = message.get('params', {})
        if self.routing and method in ('turn/start', 'turn/steer'):
            text = '\n'.join(part.get('text', '') for part in params['input'])
            if text.startswith('[Interaction controller]'):
                provenance = json.loads(next(line.removeprefix('Transport provenance: ')
                                             for line in text.splitlines()
                                             if line.startswith('Transport provenance: ')))
                author = provenance['author']
                assert author['kind'] == 'workflow_step'
                assert provenance['original_human_author'] is None
                assert provenance['authorized_human'] == 100
                assert provenance['source_channel'] == 'local-session'
                assert provenance['original_ingress_update'] is None
                thread = params['threadId']
                self.worker_packets.setdefault(thread, []).append(params)
                if method == 'turn/start':
                    turn = 'workflow-worker-' + str(len(self.workers) + 1)
                    self.workers[thread] = turn
                    send({'id': message['id'], 'result': {'turn': {'id': turn}}})
                    self.broadcast(thread, 'turn/started', {'turn': {'id': turn, 'status': 'inProgress'}})
                else:
                    send({'id': message['id'], 'result': {'turnId': self.workers[thread]}})
                return True
        return super().handle_extra_rpc(message, send)

    def worker_tool(self, topic, ident, operation, arguments, *, ok=True):
        native = self.row('SELECT codex_thread_id FROM sessions WHERE thread_id=?', (topic,))[0][0]
        send = next(send for selected, send in self.clients.values() if selected == native)
        send({'id': ident, 'method': 'item/tool/call', 'params': {
            'threadId': native, 'turnId': self.workers[native], 'callId': ident,
            'tool': 'interaction_controller',
            'arguments': {'operation': operation, 'arguments': arguments}}})
        self.wait(lambda: ident in self.tool_results)
        result = self.tool_results[ident]
        assert result['success'] is ok, result
        return json.loads(result['contentItems'][0]['text']) if ok else result


def qualify(binary):
    j = WorkflowJourney(binary)
    try:
        j.start()
        for topic in (7, 8, 1):
            j.admit('Seed this synthetic owned native context', topic=topic)
            j.wait(lambda: j.has('Initial goal turn delivered.', topic)); j.idle()
        j.stop(); j.config.write_text(j.config.read_text() + j.target_config)
        j.start(); j.wait_control_socket(j.socket); j.routing = True
        binding = j.read('presentation', 'list', '0', '0')['result']['binding']
        journal = j.row('SELECT count(*) FROM incoming_updates')
        j.messages.clear(); j.telegram_methods.clear()
        original, _ = j.submit(binding, 'Alpha produces the checkpoint, then Beta consumes and verifies it. Report the checkpoint and return the final integration here.')
        j.wait(lambda: j.row('SELECT phase FROM conversation_inputs WHERE id=?', (original['id'],)) == [('accepted',)])
        plan = {'input_ref': original['id'], 'request_id': 'selected-workflow',
                'expires_at': (datetime.now(timezone.utc) + timedelta(hours=1)).isoformat(),
                'steps': [
                    {'id': 'produce', 'selector': 't7', 'after': None, 'scope': 'Produce the selected checkpoint.',
                     'checkpoint': 'Current checkpoint', 'milestone': 'Checkpoint ready'},
                    {'id': 'verify', 'selector': 't8', 'after': 'produce', 'scope': 'Consume the exact predecessor and verify receiving.',
                     'checkpoint': 'Receiving verification', 'milestone': None}]}
        view = j.call('route-declare', 'workflow_declare', plan)
        ident = view['id']
        assert view['steps'][0]['phase'] == 'ready' and view['steps'][1]['phase'] == 'waiting'
        assert j.call('route-repeat-plan', 'workflow_declare', plan) == view
        j.call('route-start', 'workflow_advance', {'workflow': ident, 'revision': view['revision']})
        j.wait(lambda: j.instructions(7) and j.instructions(7)[0][1] == 'accepted')
        assert not j.instructions(8)
        j.answer_worker(7, 'The exact selected predecessor checkpoint.')
        # This is the actual committed-result callback and existing worker queue,
        # with no integrating model/tool call between A's return and B's admission.
        j.wait(lambda: j.instructions(8) and j.instructions(8)[0][1] == 'accepted')
        assert len(j.instructions(7)) == len(j.instructions(8)) == 1
        current = j.call('route-status', 'workflow_status', {'workflow': ident})
        assert current['phase'] == 'active' and current['steps'][0]['phase'] == 'completed'
        source = current['steps'][0]['result']
        part = j.worker_tool(8, 'route-worker-source', 'workflow_result', {
            'workflow': ident, 'step': 'produce', 'revision': source['revision']})
        assert part['text'] == 'The exact selected predecessor checkpoint.' and part['internal_evidence']
        j.worker_tool(8, 'route-worker-chain', 'workflow_declare', plan, ok=False)
        j.call('route-too-early', 'workflow_finish', {'workflow': ident, 'revision': current['revision'],
                                                   'checkpoint': 'Cannot skip required receiving.'}, ok=False)
        page = j.read('presentation', 'list', '0', '0')['result']
        event = next(e for e in page['workflow_attention']['envelopes'] if e['workflow'] == ident)
        assert event['reason'] == 'reporting_milestone' and 'text' not in event['source']
        assert page['workflow_attention']['automatic_native_return'] is False
        for item in page['items']:
            assert 'The exact selected predecessor checkpoint.' not in item.get('preview', '')
        acknowledgment = {'workflow': ident, 'revision': event['revision']}
        j.call('route-report', 'workflow_reported', acknowledgment)
        j.call('route-repeat-report', 'workflow_reported', acknowledgment)
        assert not j.read('presentation', 'list', '0', '0')['result']['workflow_attention']['envelopes']
        j.answer_worker(8, 'Receiving verification retained.')
        j.wait(lambda: j.row("SELECT count(*) FROM interaction_results WHERE body='Receiving verification retained.'") == [(1,)])
        final = j.call('route-final-status', 'workflow_status', {'workflow': ident})
        assert final['phase'] == 'awaiting_integration'
        done = j.call('route-integrate', 'workflow_finish', {'workflow': ident, 'revision': final['revision'],
                                                          'checkpoint': 'Integrated the current receiving verification.'})
        assert done['phase'] == 'completed'
        assert j.row('SELECT count(*) FROM interaction_public_deliveries') == [(0,)]
        assert j.row('SELECT count(*) FROM incoming_updates') == journal
        assert len(j.worker_packets['native-topic-7']) == len(j.worker_packets['native-topic-8']) == 1
        assert not j.messages, 'Internal workflow evidence was published to Telegram'
        j.finish(j.web_thread, j.web_turn); j.idle()
        print('PASS actual controller → A → canonical-result callback → B → on-demand predecessor → final integration; exact milestones/acknowledgments, internal audience and original identity; zero models')
    finally:
        j.close()


if __name__ == '__main__':
    qualify(str(Path(sys.argv[1]).resolve()))
