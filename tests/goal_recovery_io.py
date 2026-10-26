#!/usr/bin/env python3
"""Shared recovery over real sockets/native protocol/SQLite, synthetic peers.
Telegram seeds existing session fixtures only. The tested backend then runs
without Telegram credentials or interaction, and without an author model.
Tests advance retained due timestamps, while unit tests qualify backoff math.
"""
import copy
import json
from pathlib import Path
import sqlite3
import sys
import time
import uuid
from goal_activation_io import Activation


class Recovery(Activation):
    expected_origin = 'automatic'

    def __init__(self, binary):
        self.heads = {}
        super().__init__(binary)

    def handle_extra_rpc(self, message, send):
        method = message.get('method')
        params = message.get('params', {})
        if self.routed and method == 'thread/read':
            native = params['threadId']
            state = 'active' if self.holding.get(native) else ('systemError' if self.heads.get(native, {}).get('status') == 'failed' else 'idle')
            send({'id': message['id'], 'result': {'thread': {'id': native, 'status': {'type': state}}}})
            return True
        if self.routed and method == 'thread/turns/list':
            native = params['threadId']
            active = self.holding.get(native)
            head = {'id': active, 'status': 'inProgress', 'items': []} if active else self.heads.get(native)
            send({'id': message['id'], 'result': {'data': [head] if head else [], 'nextCursor': None}})
            return True
        return super().handle_extra_rpc(message, send)

    def capacity_failure(self, native):
        turn = self.runs[native]
        self.goals[native].update(status='blocked', updatedAt=self.goals[native]['updatedAt']+1)
        self.revisions[native] += 1
        error = {'message': 'Selected model is at capacity. Please try a different model.', 'codexErrorInfo': 'serverOverloaded'}
        self.heads[native] = {'id': turn, 'status': 'failed', 'items': [], 'error': error}
        self.holding.pop(native, None)
        self.broadcast(native, 'thread/goal/updated', {'goal': copy.deepcopy(self.goals[native]), 'turnId': turn})
        self.broadcast(native, 'turn/completed', {'turn': copy.deepcopy(self.heads[native])})
        self.wait(lambda: self.row('SELECT busy FROM sessions WHERE codex_thread_id=?', (native,)) == [(0,)])
        return turn

    def advance_due(self):
        with sqlite3.connect(self.root/'state.sqlite') as db:
            db.execute("UPDATE goal_retries SET next_at=0 WHERE phase='waiting'")


def qualify(binary):
    j = Recovery(binary)
    try:
        j.prepare(seed_entry=True)
        # An allowed participant retains ordinary /status access without gaining
        # owner-only recovery evidence or control authority.
        with sqlite3.connect(j.root/'state.sqlite') as db:
            db.execute("INSERT INTO users SELECT 101,'user',1,created_at,updated_at FROM users WHERE tg_user_id=100")
        before_status = len(j.messages)
        j.admit('/status', topic=7, user=101)
        j.wait(lambda: any('Current Telegram session:' in message['text'] and message.get('message_thread_id') == 7 for message, _ in j.messages[before_status:]))
        j.stop()
        text = j.config.read_text().replace('[telegram]\n', '[telegram]\nenabled=false\n')
        text = '\n'.join(line for line in text.splitlines() if not line.startswith('bot_token_file='))+'\n'
        text += '\n[goal_recovery]\ninterval_seconds=5\n'
        j.config.write_text(text)
        j.telegram_methods = []
        j.runs.clear()
        j.allow_native_launch = True
        j.start()
        j.wait_control_socket(j.socket)
        binding = j.owner('presentation', 'list', '0', '0')['binding']
        original = str(uuid.uuid4())
        j.owner('input', 'submit', json.dumps({'id': original, 'binding': binding, 'text': 'Coordinate this synthetic qualification through the independent app.'}))
        j.wait(lambda: j.row('SELECT phase FROM conversation_inputs WHERE id=?', (original,)) == [('accepted',)])
        entry = j.access(1)
        j.owner('goal', 'delegation', 'set', json.dumps({'request_id': str(uuid.uuid4()), 'expected_revision': None,
                'source': 'https://example.test/retained-human-recovery-delegation', 'scope': 'owned_sessions', 'agents': True, 'recovery': True}))
        selected = {'threadId': 'native-topic-7', 'objective': 'Preserve this goal through provider outages', 'status': 'paused',
                    'tokenBudget': 100, 'tokensUsed': 80, 'timeUsedSeconds': 5, 'createdAt': 1, 'updatedAt': int(time.time())}
        j.goals['native-topic-7'] = copy.deepcopy(selected)
        untouched = {**selected, 'threadId': 'native-topic-8', 'objective': 'Explicitly paused unrelated work'}
        j.goals['native-topic-8'] = copy.deepcopy(untouched)
        observed = j.cli(entry, 'goal_inspect', {'selector': 't7'})
        intent = j.cli(entry, 'goal_prepare', {'selector': 't7', 'target_binding': observed['target_binding'],
            'reason': 'Initial delegated continuation before the synthetic provider fails',
            'plan': {'action': 'resume', 'expected': observed['snapshot'], 'objective': None, 'token_budget': None, 'followup': None}})['control']['id']
        j.cli(entry, 'goal_execute', {'id': intent})
        j.wait(lambda: j.row('SELECT phase FROM interaction_goal_controls WHERE id=?', (intent,)) == [('acknowledged',)])
        counts = {table: j.row(f'SELECT COUNT(*) FROM {table}') for table in ('incoming_updates', 'conversation_inputs', 'original_ingress_order')}
        starts = sum(m.get('method') == 'turn/start' for m in j.rpc)
        failed = j.capacity_failure('native-topic-7')
        j.wait(lambda: j.row("SELECT COUNT(*) FROM goal_retries WHERE phase='waiting'") == [(1,)])
        due = j.row('SELECT next_at,attempts FROM goal_retries')[0]
        assert due[0] >= int(time.time())+20 and due[1] == 0, due
        j.advance_due()
        j.wait(lambda: j.runs['native-topic-7'] != failed)
        j.wait(lambda: j.row("SELECT COUNT(*) FROM goal_retries WHERE phase='acknowledged'") == [(1,)])
        notices = j.row("SELECT COUNT(*) FROM topic_publications WHERE body LIKE 'Turn failed:%'")
        j.capacity_failure('native-topic-7')
        j.wait(lambda: j.row("SELECT COUNT(*) FROM goal_retries WHERE phase='waiting'") == [(1,)])
        assert j.row("SELECT COUNT(*) FROM topic_publications WHERE body LIKE 'Turn failed:%'") == notices, 'Routine recovery failure spammed a new topic notice'
        later = j.row('SELECT next_at,attempts FROM goal_retries ORDER BY rowid DESC LIMIT 1')[0]
        assert later[1] == 1 and later[0] >= int(time.time())+45, later
        previous = j.runs['native-topic-7']
        j.advance_due()
        j.wait(lambda: j.runs['native-topic-7'] != previous)
        j.wait(lambda: j.row("SELECT COUNT(*) FROM goal_retries WHERE phase='acknowledged'") == [(1,)])
        assert j.goals['native-topic-7']['objective'] == selected['objective']
        assert j.goals['native-topic-7']['tokenBudget'] == 100 and j.goals['native-topic-7']['tokensUsed'] == 80
        assert j.goals['native-topic-8'] == untouched, 'Recovery resumed an explicitly paused unrelated goal'
        j.finish_goal('native-topic-7', 'RECOVERED-USEFUL-RESULT: completed the selected synthetic work once.')
        j.wait(lambda: j.row("SELECT COUNT(*) FROM goal_retries WHERE phase='returned'") == [(1,)])
        assert sum(m.get('method') == 'turn/start' for m in j.rpc) == starts, 'Recovery invented a second human kickoff'
        for table, count in counts.items():
            assert j.row(f'SELECT COUNT(*) FROM {table}') == count, f'Recovery fabricated ingress in {table}'
        projected = j.owner('topics', 'list', '1')
        current = next(v for v in projected['items'] if v['key']['thread_id'] == 7)
        assert current['goal_recovery']['state'] == 'returned' and current['goal_recovery']['enabled']
        # Deliberately retain an uncertain control on the earliest of two due
        # targets. It must fence only that target, not abort the scheduler batch.
        for topic in (7, 8):
            native = f'native-topic-{topic}'
            j.goals[native].update(status='paused')
            j.revisions[native] = j.revisions.get(native, 0)+1
            observed = j.cli(entry, 'goal_inspect', {'selector': f't{topic}'})
            control = j.cli(entry, 'goal_prepare', {'selector': f't{topic}', 'target_binding': observed['target_binding'],
                'reason': 'Qualify independent recovery admission despite another target uncertainty',
                'plan': {'action': 'resume', 'expected': observed['snapshot'], 'objective': None, 'token_budget': None, 'followup': None}})['control']['id']
            j.cli(entry, 'goal_execute', {'id': control})
            j.wait(lambda: j.row('SELECT phase FROM interaction_goal_controls WHERE id=?', (control,)) == [('acknowledged',)])
            j.capacity_failure(native)
        j.wait(lambda: j.row("SELECT COUNT(*) FROM goal_retries WHERE phase='waiting'") == [(2,)])
        with sqlite3.connect(j.root/'state.sqlite') as db:
            # Fault injection: preserve the genuine last control as uncertain.
            db.execute("UPDATE interaction_goal_controls SET phase='uncertain' WHERE id=(SELECT control_id FROM goal_retries WHERE target_topic=7 AND phase='returned' ORDER BY rowid DESC LIMIT 1)")
            db.execute("UPDATE goal_retries SET next_at=CASE WHEN target_topic=7 THEN -1 ELSE 0 END WHERE phase='waiting'")
        failed7, failed8 = j.runs['native-topic-7'], j.runs['native-topic-8']
        j.wait(lambda: j.runs['native-topic-8'] != failed8)
        assert j.runs['native-topic-7'] == failed7, 'An uncertain target was replayed'
        j.wait(lambda: j.row("SELECT COUNT(*) FROM goal_retries WHERE target_topic=7 AND phase='waiting' AND next_at>0") == [(1,)])
        j.finish_goal('native-topic-8', 'INDEPENDENT-RECOVERY-RESULT: uncertainty on Alpha did not starve Beta.')
        j.wait(lambda: j.row("SELECT COUNT(*) FROM goal_retries WHERE target_topic=8 AND phase='returned'") == [(1,)])
        thread = 'native-topic-9'
        j.broadcast(thread, 'turn/completed', {'turn': {'id': j.runs[thread], 'status': 'completed'}})
        j.idle()
        assert not j.telegram_methods, j.telegram_methods
        assert not any(m.get('method') == 'turn/interrupt' for m in j.rpc)
        print('PASS two capacity failures, durable growing backoff and shared native continuation/return, preserved accounting and explicit pauses; zero Telegram API or fabricated ingress during tested runtime; synthetic native peer, not real provider acceptance')
    finally:
        j.close()


if __name__ == '__main__':
    qualify(str(Path(sys.argv[1]).resolve()))
