#!/usr/bin/env python3
"""Actual bridge/SQLite/native-peer/Telegram-HTTP projection; all identities synthetic."""
import json
from pathlib import Path
import sys
import threading
from conversation_routing_io import CanonicalRouting


class Projected(CanonicalRouting):
    def __init__(self, binary):
        self.worker_sequence = 0
        self.direct_packets = []
        self.echo_held, self.release_echo = threading.Event(), threading.Event()
        self.hold_echo = False
        self.drop_answer = False
        self.reject_echo = False
        self.rejections = 0
        super().__init__(binary)

    def handle_extra_rpc(self, message, send):
        method, params = message.get('method'), message.get('params', {})
        native = params.get('threadId', '')
        if self.routing and native in ('native-topic-7', 'native-topic-8') and method in ('turn/start', 'turn/steer'):
            text = '\n'.join(part.get('text', '') for part in params['input'])
            if text.startswith('[Interaction controller]'):
                provenance = json.loads(next(line.removeprefix('Transport provenance: ')
                    for line in text.splitlines() if line.startswith('Transport provenance: ')))
                assert provenance['original_human_author'] == 100
                self.worker_packets.setdefault(native, []).append(params)
            else:
                self.direct_packets.append(params)
            if method == 'turn/start':
                self.worker_sequence += 1
                self.workers[native] = 'projected-turn-'+str(self.worker_sequence)
                send({'id': message['id'], 'result': {'turn': {'id': self.workers[native]}}})
                self.broadcast(native, 'turn/started', {'turn': {'id': self.workers[native], 'status': 'inProgress'}})
            else:
                send({'id': message['id'], 'result': {'turnId': self.workers[native]}})
            return True
        return super().handle_extra_rpc(message, send)

    def before_send_message_reply(self, payload):
        text = payload['text']
        if self.hold_echo and 'First projected input' in text:
            self.echo_held.set()
            assert self.release_echo.wait(timeout=30), 'Synthetic mirror hold expired'
        if self.drop_answer and 'Lost mirror acknowledgment.' == text:
            self.drop_answer = False
            return False
        if self.reject_echo and 'Retryable projected input' in text:
            self.reject_echo = False
            self.rejections += 1
            return {'ok': False, 'error_code': 429, 'description': 'Synthetic known rejection',
                    'parameters': {'retry_after': 1}}

    def answer(self, topic, text, *, duplicate=False, direct=False):
        native = 'native-topic-'+str(topic)
        turn = self.workers[native]
        rows = [row for row in self.instructions(topic) if json.loads(row[2]).get('native_turn') == turn]
        requests = sorted({request for _, _, doc in rows for request in json.loads(doc)['requests']})
        claim = {'requests': requests, 'covered': [row[0] for row in rows], 'completed': requests}
        body = text if direct else text+'\n<daimon-result>'+json.dumps(claim)+'</daimon-result>'
        item = {'turnId': turn, 'item': {'id': turn+'-answer', 'type': 'agentMessage',
                                       'phase': 'final_answer', 'text': body}}
        self.broadcast(native, 'item/completed', item)
        if duplicate:
            self.broadcast(native, 'item/completed', item)
        self.broadcast(native, 'turn/completed', {'turn': {'id': turn, 'status': 'completed'}})

    def retained(self, text):
        return any(text in item['preview'] for item in
                   self.read('presentation', 'list', '0', '0')['result']['items'])

    def primary(self, text):
        return any(item['id'].startswith('delivery:') and item.get('current') is True
                   and not item.get('correction_pending') and text in item['preview']
                   for item in self.read('presentation', 'list', '0', '0')['result']['items'])

    def sent(self, text, topic):
        return sum(message['message_thread_id'] == topic and message['text'] == text
                   for message, _ in self.messages)


def qualify(binary):
    j = Projected(str(Path(binary).resolve()))
    try:
        j.start()
        for topic in (7, 8, 1):
            j.admit('Seed existing context', topic=topic)
            j.wait(lambda: j.has('Initial goal turn delivered.', topic)); j.idle()
        j.stop()
        j.config.write_text(j.config.read_text().replace('[interaction]\n',
            '[interaction]\ntelegram_projection=true\n')+j.target_config)
        j.start(); j.wait_control_socket(j.socket); j.routing = True
        binding = j.read('presentation', 'list', '0', '0')['result']['binding']
        j.messages.clear()
        j.hold_echo = True
        original, _ = j.submit(binding, '#Alpha First projected input')
        j.wait(lambda: j.instructions(7) and j.instructions(7)[0][1] == 'accepted')
        assert j.echo_held.wait(timeout=10)
        j.answer(7, 'Primary result while mirror is held.', duplicate=True)
        j.wait(lambda: j.primary('Primary result while mirror is held.'))
        assert j.sent('Primary result while mirror is held.', 7) == 0
        j.submit(binding, '#Beta Independent parallel input')
        j.wait(lambda: j.instructions(8) and j.instructions(8)[0][1] == 'accepted')
        j.answer(8, 'Independent mirror and primary result.')
        j.wait(lambda: j.sent('Independent mirror and primary result.', 8) == 1)
        j.hold_echo = False; j.release_echo.set()
        j.wait(lambda: j.sent('Primary result while mirror is held.', 7) == 1)
        j.wait(lambda: j.row("SELECT COUNT(*) FROM topic_publications WHERE phase!='sent'") == [(0,)])
        j.idle()
        j.submit(binding, original['text'], ident=original['id'])
        assert len(j.instructions(7)) == 1
        assert j.row("SELECT COUNT(*) FROM topic_publications WHERE kind='input'") == [(2,)]
        assert j.sent('Primary result while mirror is held.', 7) == 1
        assert not any('daimon-result' in message['text'] for message, _ in j.messages)

        # Publication is never ingress: Telegram echoes its own bot-authored item.
        before = j.row('SELECT COUNT(*) FROM turns')
        echoed = next(message for message, _ in j.messages if message['text'] == 'Primary result while mirror is held.')
        with j.lock:
            j.sequence += 1; update = j.sequence
            j.updates.append({'update_id': update, 'message': echoed})
        j.wait(lambda: j.row('SELECT status FROM incoming_updates WHERE update_id=?', (update,)) == [('handled',)])
        assert j.row('SELECT COUNT(*) FROM turns') == before
        assert len(j.instructions(7)) == 1

        # Ordinary direct input uses the same native session, without router wrapping.
        direct_update=j.admit('Direct work-topic text.', topic=7)
        j.wait(lambda: len(j.direct_packets)==1 and j.row('SELECT native_turn FROM native_input_receipts WHERE update_id=?',(direct_update,))==[(j.workers['native-topic-7'],)])
        assert j.direct_packets[0]['input'][0]['text'] == 'Direct work-topic text.'
        j.answer(7, 'Direct answer once.', direct=True, duplicate=True)
        j.wait(lambda: j.sent('Direct answer once.', 7) == 1); j.idle()
        assert len(j.instructions(7)) == 1
        assert not j.retained('Direct answer once.')

        j.submit(binding, '#Alpha Input whose answer loses its mirror receipt')
        j.wait(lambda: len(j.instructions(7)) == 2 and j.instructions(7)[-1][1] == 'accepted')
        j.drop_answer = True
        j.answer(7, 'Lost mirror acknowledgment.')
        j.wait(lambda: j.primary('Lost mirror acknowledgment.'))
        j.wait(lambda: j.row("SELECT COUNT(*) FROM topic_publications WHERE phase='uncertain'") == [(1,)])
        j.idle(); j.stop(); j.start(); j.wait_control_socket(j.socket)
        assert j.sent('Lost mirror acknowledgment.', 7) == 1
        inspection = j.read('topics', 'publications')['result']
        assert any(row['phase'] == 'uncertain' for row in inspection['items'])

        j.reject_echo = True
        j.submit(binding, '#Alpha Retryable projected input')
        j.wait(lambda: len(j.instructions(7)) == 3 and j.instructions(7)[-1][1] == 'accepted')
        j.answer(7, 'New work after uncertain publication.')
        j.wait(lambda: j.sent('New work after uncertain publication.', 7) == 1)
        j.wait(lambda: j.row("SELECT COUNT(*) FROM topic_publications WHERE phase IN ('ready','publishing')") == [(0,)])
        assert j.rejections == 1
        assert j.sent('Lost mirror acknowledgment.', 7) == 1
        assert len(j.worker_packets['native-topic-7']) == 3
        print('PASS canonical event/destination identity, attributed echoes, independent return/parallel topic, direct switch, bot feedback, duplicate/restart/lost ACK and known rejection')
    finally:
        j.release_echo.set()
        j.close()


if __name__ == '__main__':
    qualify(sys.argv[1])
