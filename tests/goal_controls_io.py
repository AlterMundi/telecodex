#!/usr/bin/env python3
"""Durable human/proposal controls through controller, native wire and SQLite.
Native responses and human accounts are synthetic; no real goal/model is touched.
"""
import copy
import json
from pathlib import Path
import sys
import threading
from goal_inspection_io import Inspection


class Controls(Inspection):
    def __init__(self, binary):
        self.revisions = {}
        self.lose_receipt = False
        self.owning_metadata = 0
        self.drop_control_notice = False
        super().__init__(binary)

    def start(self):
        if self.routed:
            text = self.config.read_text()
            if 'telegram_projection=true' not in text:
                self.config.write_text(text.replace('[interaction]\n', '[interaction]\ntelegram_projection=true\n'))
        return super().start()

    def before_send_message_reply(self, payload):
        if self.drop_control_notice and 'Integrator action' in payload['text']:
            self.drop_control_notice = False
            return False  # Delivery may have happened; do not resend this notice.
        inherited = getattr(super(), 'before_send_message_reply', None)
        return inherited(payload) if inherited else None

    def handle_extra_rpc(self, message, send):
        method = message.get('method')
        params = message.get('params', {})
        if method in ('thread/goal/snapshot', 'thread/goal/conditionalSet', 'thread/goal/conditionalClear'):
            thread = params['threadId']
            if method != 'thread/goal/snapshot' and any(active_send is send for _,active_send in self.clients.values()):
                self.owning_metadata += 1
            revision = str(self.revisions.get(thread, 0))
            if method == 'thread/goal/snapshot':
                send({'id': message['id'], 'result': {'goal': copy.deepcopy(self.goals.get(thread)), 'revision': revision}})
            elif params['expectedRevision'] != revision:
                send({'id': message['id'], 'error': {'code': -32600, 'message': 'Selected goal definition changed'}})
            else:
                assert params['origin'] == 'user'
                self.revisions[thread] = int(revision) + 1
                if method.endswith('Clear'):
                    cleared = self.goals.pop(thread, None) is not None
                    result = {'cleared': cleared, 'revision': str(int(revision) + 1)}
                else:
                    goal = self.goals[thread]
                    for source, target in (('objective', 'objective'), ('status', 'status'), ('tokenBudget', 'tokenBudget')):
                        if source in params:
                            goal[target] = params[source]
                    result = {'goal': copy.deepcopy(goal), 'revision': str(int(revision) + 1)}
                if self.lose_receipt:
                    send({'id': message['id'], 'error': {'code': -32603, 'message': 'Synthetic error after metadata effect; outcome must remain uncertain'}})
                else:
                    send({'id': message['id'], 'result': result})
            return True
        return super().handle_extra_rpc(message, send)


def qualify(binary):
    j = Controls(binary)
    try:
        j.prepare(seed_entry=True)
        j.allow_native_launch = True
        j.admit("Pause Alpha's goal.", topic=1)
        j.wait(lambda: 'native-topic-9' in j.runs)
        entry = j.access(1)
        j.goals['native-topic-7'] = {'threadId': 'native-topic-7', 'objective': 'Selected complex work', 'status': 'active', 'tokenBudget': 100, 'tokensUsed': 80, 'timeUsedSeconds': 5, 'createdAt': 1, 'updatedAt': 2}

        j.admit('Ordinary direct Alpha work is already active.', topic=7)
        j.wait(lambda: 'native-topic-7' in j.runs)

        def original(text=None):
            if text:
                old = j.cli(entry, 'inputs')['inputs'][0]['input_ref']
                j.admit(text, topic=1)
                j.wait(lambda: j.cli(entry, 'inputs')['inputs'][0]['input_ref'] != old)
            return j.cli(entry, 'inputs')['inputs'][0]['input_ref']

        def prepare(action, reference=None, reason='Selected human goal control', followup=None):
            observed = j.cli(entry, 'goal_inspect', {'selector': 'Alpha'})
            args = {'selector': 'Alpha', 'target_binding': observed['target_binding'], 'reason': reason,
                    'plan': {'action': action, 'expected': observed['snapshot'], 'objective': None, 'token_budget': None, 'followup': followup}}
            if reference is not None:
                args['input_ref'] = reference
            result = j.cli(entry, 'goal_prepare', args)
            return result, args

        def view(command):
            import subprocess
            result=subprocess.run([j.binary,'--control',str(j.config),'presentation',*command],capture_output=True,text=True,timeout=20)
            assert result.returncode==0,result.stderr
            return json.loads(result.stdout)['result']

        first_ref = original()
        first, args = prepare('pause', first_ref)
        control = first['control']['id']
        assert first['new_intent'] and first['control']['phase'] == 'prepared'
        assert not j.cli(entry, 'goal_prepare', args)['new_intent']
        assert first['control']['intent']['original']['source']['text'] == "Pause Alpha's goal."
        j.cli(entry, 'dispatch', {'input_ref': first_ref, 'destinations': [{'selector': 'Alpha', 'relation': 'new', 'requests': []}]}, ok=False)
        j.goals['native-topic-7']['objective'] = 'Changed independently'
        j.revisions['native-topic-7'] = 1
        rejected = j.cli(entry, 'goal_execute', {'id': control})
        assert rejected['phase'] == 'rejected' and j.goals['native-topic-7']['status'] == 'active'
        pause, _ = prepare('pause', original('Pause the current Alpha goal.'))
        paused = j.cli(entry, 'goal_execute', {'id': pause['control']['id']})
        assert paused['phase'] == 'acknowledged' and j.goals['native-topic-7']['status'] == 'paused'
        j.wait(lambda: j.row("SELECT phase FROM topic_publications WHERE kind='control' AND json_extract(source,'$[0]')=?", (pause['control']['id'],)) == [('sent',)])
        notices = [(m,p) for m,p in j.messages if 'Native session acknowledged the goal change.' in m['text']]
        assert len(notices) == 1 and notices[0][0].get('message_thread_id') == 7
        assert 'Integrator action' in notices[0][0]['text'] and 'goal pause' in notices[0][0]['text']
        assert pause['control']['id'] not in notices[0][0]['text']
        assert j.goals['native-topic-7']['tokensUsed'] == 80
        assert j.owning_metadata == 2, 'Busy target metadata must use its current subscribed writer'
        count = sum(m.get('method') == 'thread/goal/conditionalSet' for m in j.rpc)
        j.cli(entry, 'goal_execute', {'id': pause['control']['id']})
        assert sum(m.get('method') == 'thread/goal/conditionalSet' for m in j.rpc) == count
        assert j.row("SELECT COUNT(*) FROM topic_publications WHERE kind='control' AND json_extract(source,'$[0]')=?", (pause['control']['id'],)) == [(1,)]
        cancelled, _ = prepare('pause', reason='Assistant proposal that the human declines', followup='Ask the selected question afterward.')
        goal_views=view(['goals','list','0'])
        proposed_view=next(v for v in goal_views['items'] if v['id']==cancelled['control']['id'])
        assert proposed_view['can_confirm'] and proposed_view['proposal'] and 'plan' not in proposed_view
        before=len(j.rpc)
        detail=view(['goals','detail',cancelled['control']['id']])
        assert detail['reason']=='Assistant proposal that the human declines'
        assert detail['plan']['followup']=='Ask the selected question afterward.'
        assert detail['entry_binding'][0]==100 and detail['label']!='Integrator'
        assert len(j.rpc)==before,'Human review does not read/mutate native state or run inference'
        declined = original('No, cancel that proposal.')
        decision = {'id': cancelled['control']['id'], 'input_ref': declined, 'execute': False}
        assert j.cli(entry, 'goal_decide', decision)['control']['phase'] == 'cancelled'
        assert not j.cli(entry, 'goal_decide', decision)['new_decision']
        j.cli(entry, 'goal_decide', {**decision, 'execute': True}, ok=False)
        assert j.cli(entry, 'goal_execute', {'id': cancelled['control']['id']})['phase'] == 'cancelled'
        assert not view(['goals','detail',cancelled['control']['id']])['can_confirm']
        assert not any(v['id']==cancelled['control']['id'] for v in view(['goals','list','0'])['items'])
        earlier = original('Could clearing this goal be useful?')
        proposal, _ = prepare('clear', reason='Assistant suggests clearing the now obsolete goal')
        proposed = proposal['control']['id']
        assert proposal['control']['phase'] == 'awaiting_confirmation'
        j.cli(entry, 'goal_execute', {'id': proposed})
        assert 'native-topic-7' in j.goals
        j.cli(entry, 'goal_decide', {'id': proposed, 'input_ref': earlier, 'execute': True}, ok=False)
        confirm = original('Yes, clear that goal.')
        decision = {'id': proposed, 'input_ref': confirm, 'execute': True}
        assert j.cli(entry, 'goal_decide', decision)['new_decision']
        assert not j.cli(entry, 'goal_decide', decision)['new_decision']
        cleared = j.cli(entry, 'goal_execute', {'id': proposed})
        assert cleared['phase'] == 'acknowledged' and 'native-topic-7' not in j.goals
        assert cleared['intent']['confirmation']['original']['source']['text'] == 'Yes, clear that goal.'
        # Two retained human controls obey capture chronology even if invoked backwards.
        j.goals['native-topic-7'] = {'threadId': 'native-topic-7', 'objective': 'New paused work', 'status': 'paused', 'tokenBudget': 100, 'tokensUsed': 80, 'timeUsedSeconds': 5, 'createdAt': 3, 'updatedAt': 3}
        older, _ = prepare('pause', original('Pause Alpha once more.'))
        newer, _ = prepare('clear', original('Instead clear that Alpha goal.'))
        assert j.cli(entry, 'goal_execute', {'id': older['control']['id']})['phase'] == 'superseded'
        assert j.cli(entry, 'goal_execute', {'id': newer['control']['id']})['phase'] == 'acknowledged'
        # A lost/internal response after the effect is not a rejection or retry.
        j.goals['native-topic-7'] = {'threadId': 'native-topic-7', 'objective': 'New work', 'status': 'active', 'tokenBudget': 100, 'tokensUsed': 81, 'timeUsedSeconds': 5, 'createdAt': 3, 'updatedAt': 3}
        uncertain, _ = prepare('pause', original('Pause the new Alpha goal.'))
        j.lose_receipt = True
        j.drop_control_notice = True
        before = len(j.rpc)
        result = j.cli(entry, 'goal_execute', {'id': uncertain['control']['id']})
        assert result['phase'] == 'uncertain' and j.goals['native-topic-7']['status'] == 'paused'
        j.wait(lambda: j.row("SELECT phase FROM topic_publications WHERE kind='control' AND json_extract(source,'$[0]')=?", (uncertain['control']['id'],)) == [('uncertain',)])
        attempts = len(j.rpc)
        j.cli(entry, 'goal_execute', {'id': uncertain['control']['id']})
        assert len(j.rpc) == attempts and attempts > before
        assert j.row("SELECT COUNT(*) FROM topic_publications WHERE kind='control' AND json_extract(source,'$[0]')=?", (uncertain['control']['id'],)) == [(1,)]
        assert not any(m.get('method') in ('thread/goal/set', 'thread/goal/clear', 'turn/interrupt') for m in j.rpc)
        j.broadcast('native-topic-7', 'turn/completed', {'turn': {'id': j.runs['native-topic-7'], 'status': 'completed'}})
        thread = 'native-topic-9'
        j.broadcast(thread, 'turn/completed', {'turn': {'id': j.runs[thread], 'status': 'completed'}})
        j.idle()
        print('PASS explicit conditional control, stale native refusal, preserved usage, original exclusivity, later single-use confirmation and uncertain non-replay; synthetic peers')
    finally:
        j.close()


def staged_pause(binary):
    class Staged(Controls):
        stage = False

        def handle_extra_rpc(self, message, send):
            if self.stage and message.get('method') == 'turn/start' and message['params']['threadId'] == 'native-topic-7':
                self.start_seen.set()
                def later():
                    assert self.release_start.wait(timeout=30)
                    send({'id': message['id'], 'result': {'turn': {'id': 'staged-native-turn'}}})
                    self.broadcast('native-topic-7', 'turn/started', {'turn': {'id': 'staged-native-turn', 'status': 'inProgress'}})
                    self.finish('native-topic-7', 'staged-native-turn')
                threading.Thread(target=later, daemon=True).start()
                return True
            return super().handle_extra_rpc(message, send)
    j = Staged(binary)
    try:
        j.prepare(seed_entry=True)
        j.allow_native_launch = True
        j.admit('Coordinate this goal-control test.', topic=1)
        j.wait(lambda: 'native-topic-9' in j.runs)
        entry = j.access(1)
        j.stage = True
        j.admit('/goal Synthetic staged goal', topic=7)
        j.wait(j.start_seen.is_set)
        j.admit('Pause Alpha before its prepared kickoff completes.', topic=1)
        j.wait(lambda: j.cli(entry, 'inputs')['inputs'][0]['text_excerpt'] == 'Pause Alpha before its prepared kickoff completes.')
        reference = j.cli(entry, 'inputs')['inputs'][0]['input_ref']
        observed = j.cli(entry, 'goal_inspect', {'selector': 'Alpha'})
        assert observed['snapshot']['goal']['status'] == 'paused'
        prepared = j.cli(entry, 'goal_prepare', {'selector': 'Alpha', 'target_binding': observed['target_binding'], 'input_ref': reference, 'reason': 'Explicit pause supersedes the staged activation', 'plan': {'action': 'pause', 'expected': observed['snapshot'], 'objective': None, 'token_budget': None, 'followup': None}})
        receipt = j.cli(entry, 'goal_execute', {'id': prepared['control']['id']})
        assert receipt['phase'] == 'acknowledged' and j.owning_metadata == 1
        checkpoint = len(j.rpc)
        j.release_start.set()
        j.wait(lambda: j.has('Initial goal turn delivered.', 7))
        j.wait(lambda: j.row('SELECT busy FROM sessions WHERE thread_id=7') == [(0,)])
        assert j.goals['native-topic-7']['status'] == 'paused'
        assert not any(m.get('method') == 'thread/goal/set' and m.get('params', {}).get('status') == 'active' for m in j.rpc[checkpoint:])
        thread = 'native-topic-9'
        j.broadcast(thread, 'turn/completed', {'turn': {'id': j.runs[thread], 'status': 'completed'}})
        j.idle()
        print('PASS conditional pause through owning subscriber prevents later prepared-goal reactivation; synthetic peers')
    finally:
        j.release_start.set()
        j.close()


if __name__ == '__main__':
    binary = str(Path(sys.argv[1]).resolve())
    qualify(binary)
    staged_pause(binary)
