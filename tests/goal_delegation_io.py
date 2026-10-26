#!/usr/bin/env python3
"""Shared delegated goal controls over owner socket/native wire/SQLite.
Native peers and fixture enrollment are synthetic. Telegram seeds preexisting
sessions only; the tested runtime has its adapter disabled and no credentials.
No real model, live human, native goal or Telegram account is touched.
"""
import json
from pathlib import Path
import sys
import uuid
from goal_activation_io import Activation


def qualify(binary):
    j = Activation(binary)
    try:
        j.prepare(seed_entry=True)
        j.stop()
        j.config.write_text(j.config.read_text().replace('[[interaction.targets]]\nname="Beta"\ntopic_id=8\n', '').replace(
            '[telegram]\n', '[telegram]\nenabled=false\n').replace(
            'bot_token_file=', 'unused_previous_token_file='))
        # TelegramConfig currently tolerates unknown historical config keys;
        # remove this old binding entirely rather than relying on that behavior.
        j.config.write_text('\n'.join(line for line in j.config.read_text().splitlines()
                                     if not line.startswith('unused_previous_token_file='))+'\n')
        j.telegram_methods = []
        j.runs.clear()
        j.allow_native_launch = True
        j.expected_origin = 'automatic'
        j.start()
        j.wait_control_socket(j.socket)
        original = str(uuid.uuid4())
        binding = j.owner('presentation', 'list', '0', '0')['binding']
        j.owner('input', 'submit', json.dumps({'id': original, 'binding': binding,
                                              'text': 'Discuss the delegated work from the independent app.'}))
        j.wait(lambda: j.row('SELECT phase FROM conversation_inputs WHERE id=?', (original,)) == [('accepted',)])
        entry = j.access(1)
        counts = {table: j.row(f'SELECT COUNT(*) FROM {table}')
                  for table in ('incoming_updates', 'conversation_inputs', 'original_ingress_order')}
        grant = {'request_id': str(uuid.uuid4()), 'expected_revision': None,
                 'source': 'https://example.test/actual-human-delegation',
                 'scope': 'owned_sessions', 'agents': True, 'recovery': False}
        granted = j.owner('goal', 'delegation', 'set', json.dumps(grant))
        revision = granted['delegation']['revision']
        j.goals['native-topic-7'] = {'threadId': 'native-topic-7', 'objective': 'Preserve cumulative work',
                                    'status': 'paused', 'tokenBudget': 100, 'tokensUsed': 80,
                                    'timeUsedSeconds': 5, 'createdAt': 1, 'updatedAt': 2}

        def prepare(access, selector, action, reason):
            observed = j.cli(access, 'goal_inspect', {'selector': selector})
            args = {'selector': selector, 'target_binding': observed['target_binding'], 'reason': reason,
                    'plan': {'action': action, 'expected': observed['snapshot'],
                             'objective': None, 'token_budget': None, 'followup': None}}
            value = j.cli(access, 'goal_prepare', args)['control']
            assert value['phase'] == 'prepared' and not value['intent']['proposal']
            assert value['intent']['original'] is None
            assert value['intent']['delegation']['revision'] == revision
            assert value['intent']['actor']['kind'] == 'daimon'
            return value['id']

        for selector, native in [('self', 'native-topic-9'), ('t8', 'native-topic-8')]:
            j.goals[native] = {**j.goals['native-topic-7'], 'threadId': native, 'status': 'active'}
            control = prepare(entry, selector, 'pause', 'Delegate can select its own integrating goal and an owned unenrolled goal')
            assert j.cli(entry, 'goal_execute', {'id': control})['phase'] == 'acknowledged'
            assert j.goals[native]['status'] == 'paused'
        selected = prepare(entry, 't7', 'resume', 'Daimon continues the delegated objective')
        j.cli(entry, 'goal_execute', {'id': selected})
        j.wait(lambda: j.row('SELECT phase FROM interaction_goal_controls WHERE id=?', (selected,)) == [('acknowledged',)])
        worker = j.access(7)
        caps = j.cli(worker, 'capabilities')['operations']
        assert 'goal_prepare' in caps and 'goal_execute' in caps and 'dispatch' not in caps
        selected = prepare(worker, 'self', 'pause', 'Daimon pauses its own goal without stopping its current turn')
        assert j.cli(worker, 'goal_execute', {'id': selected})['phase'] == 'acknowledged'
        assert j.goals['native-topic-7']['status'] == 'paused'
        assert j.goals['native-topic-7']['tokensUsed'] == 80
        assert j.goals['native-topic-7']['tokenBudget'] == 100
        stale = prepare(worker, 'self', 'resume', 'This prepared continuation will be revoked')
        revoke = {**grant, 'request_id': str(uuid.uuid4()), 'expected_revision': revision, 'agents': False}
        revoked = j.owner('goal', 'delegation', 'set', json.dumps(revoke))
        before = len(j.rpc)
        j.cli(worker, 'goal_execute', {'id': stale}, ok=False)
        assert len(j.rpc) == before
        retry = j.owner('goal', 'delegation', 'set', json.dumps(grant))
        assert not retry['new_revision'] and retry['current'] == revoked['delegation']
        for table, count in counts.items():
            assert j.row(f'SELECT COUNT(*) FROM {table}') == count, f'Fabricated ingress in {table}'
        assert not j.telegram_methods, j.telegram_methods
        assert not any(m.get('method') in ('turn/interrupt', 'thread/goal/set', 'thread/goal/clear') for m in j.rpc)
        j.finish_goal('native-topic-7', 'Delegated work returned through the shared backend.', complete_goal=False)
        thread = 'native-topic-9'
        j.broadcast(thread, 'turn/completed', {'turn': {'id': j.runs[thread], 'status': 'completed'}})
        j.idle()
        assert not j.telegram_methods, j.telegram_methods
        print('PASS delegated resume/self-pause, automatic native origin, accounting, revocation and idempotency; no Telegram API or fabricated ingress in the tested runtime; synthetic native peer')
    finally:
        j.close()


if __name__ == '__main__':
    qualify(str(Path(sys.argv[1]).resolve()))
