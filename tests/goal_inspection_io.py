#!/usr/bin/env python3
"""Read-only goal inspection through the actual controller; synthetic native peer."""
import copy
from pathlib import Path
import sqlite3
import sys
from warm_controller_io import Warm


class Inspection(Warm):
    unsupported = False

    def handle_extra_rpc(self, message, send):
        if message.get('method') == 'thread/goal/snapshot':
            if self.unsupported:
                send({'id': message['id'], 'error': {'code': -32601, 'message': 'Snapshot unavailable'}})
            else:
                thread = message['params']['threadId']
                send({'id': message['id'], 'result': {'goal': copy.deepcopy(self.goals.get(thread)), 'revision': 'fixture-definition:1'}})
            return True
        return super().handle_extra_rpc(message, send)


def qualify(binary):
    j = Inspection(binary)
    try:
        j.prepare(seed_entry=True)
        j.allow_native_launch = True
        j.admit('Inspect existing work without activating it.', topic=1)
        j.wait(lambda: 'native-topic-9' in j.runs)
        entry = j.access(1)
        assert 'goal_inspect' in j.cli(entry, 'capabilities')['operations']
        before = len(j.rpc)
        empty = j.cli(entry, 'goal_inspect', {'selector': '#Alpha'})
        assert empty['availability'] == 'observed' and empty['snapshot']['goal'] is None, empty
        assert empty['snapshot']['revision'] == 'fixture-definition:1'
        assert empty['native_activity'] == 'active', empty
        j.goals['native-topic-7'] = {'threadId': 'native-topic-7', 'objective': 'Preserve this work', 'status': 'blocked', 'tokenBudget': 100, 'tokensUsed': 80, 'timeUsedSeconds': 5, 'createdAt': 1, 'updatedAt': 2}
        blocked = j.cli(entry, 'goal_inspect', {'selector': '/self#Alpha'})
        assert blocked['snapshot']['goal']['status'] == 'blocked'
        assert blocked['snapshot']['goal']['tokensUsed'] == 80
        assert blocked['native_activity'] == 'active'
        j.unsupported = True
        unknown = j.cli(entry, 'goal_inspect', {'selector': 'Alpha'})
        assert unknown['availability'] == 'unknown' and 'snapshot' not in unknown, unknown
        count = len(j.rpc)
        assert j.cli(entry, 'goal_inspect', {'selector': 'Missing'})['kind'] == 'clarification_required'
        assert len(j.rpc) == count
        with sqlite3.connect(j.root/'state.sqlite') as db:
            db.execute('UPDATE sessions SET creator_user_id=999 WHERE thread_id=7')
        j.cli(entry, 'goal_inspect', {'selector': 'Alpha'}, ok=False)
        assert len(j.rpc) == count
        assert not any(m.get('method') in ('thread/goal/set', 'thread/goal/clear', 'thread/resume', 'turn/start', 'turn/steer') for m in j.rpc[before:])
        thread = 'native-topic-9'
        j.broadcast(thread, 'turn/completed', {'turn': {'id': j.runs[thread], 'status': 'completed'}})
        j.idle()
        print('PASS readonly native snapshot, separate execution/goal state, unknown capability, ownership and no mutation; synthetic native peer')
    finally:
        j.close()


if __name__ == '__main__':
    qualify(str(Path(sys.argv[1]).resolve()))
