#!/usr/bin/env python3
"""Explicit context integration through HTTP, native WebSocket and SQLite.
All histories, account IDs and model responses are synthetic.
"""
import json
import pathlib
import sqlite3
import sys
from goals_io import Goals


class Merge(Goals):
    def __init__(self, binary):
        super().__init__(binary)
        self.histories = {}
        self.inputs = {}
        self.summaries = 0
        self.steering = 'accept'
        self.created = 20
        self.repeat_cursor = False

    def created_topic(self, payload):
        self.created += 1
        return {'message_thread_id': self.created, 'name': payload['name'], 'icon_color': 0}

    def handled(self, update):
        self.wait(lambda:self.row('SELECT status FROM incoming_updates WHERE update_id=?',(update,))==[('handled',)])

    def handle_extra_rpc(self, request, send):
        method = request.get('method'); p = request.get('params', {}); ident = request.get('id')
        if method == 'config/read':
            send({'id': ident, 'result': {'config': {'mcp_servers': {'fixture': {'enabled': True}}}}})
            return True
        if method == 'thread/turns/list' and p.get('itemsView') == 'full':
            assert p['sortDirection'] == 'desc'
            history = list(reversed(self.histories[p['threadId']]))
            if self.holding.get(p['threadId']):
                history.insert(0,{'id':self.holding[p['threadId']],'status':'inProgress','items':[{'type':'userMessage','content':[{'type':'text','text':self.inputs[p['threadId']]}]},{'type':'agentMessage','text':'A provisional research intuition; not an agreement.'}]})
            result={'data':history}
            if self.repeat_cursor:result['nextCursor']='repeated'
            send({'id': ident, 'result': result})
            return True
        if method == 'thread/start' and p.get('ephemeral'):
            assert p['sandbox'] == 'read-only' and p['dynamicTools'] == []
            assert p['config']['features.shell_tool'] is False
            assert p['config']['mcp_servers.fixture.enabled'] is False
            assert 'firsthand experience' in p['baseInstructions']
            self.summaries += 1
            send({'id': ident, 'result': {'thread': {'id': f'summary-{self.summaries}'}}})
            return True
        if method == 'turn/start' and p['threadId'].startswith('summary-'):
            context = json.loads(p['input'][0]['text'])
            assert len(context['source_entries']) >= 2
            text = 'Prior understanding retained. Received research is attributed. Discarded suggestion remains discarded; alternatives and open questions remain.'
            output = json.dumps({'summary': context['selected_topic'] + '\n' + text,
                                 'recent_indices': [0, len(context['source_entries'])-1]})
            send({'id': ident, 'result': {'turn': {'id': 'summary-turn'}}})
            send({'method': 'item/completed', 'params': {'threadId': p['threadId'], 'turnId': 'summary-turn',
                 'item': {'type': 'agentMessage', 'id': 'summary-answer', 'phase': 'final_answer', 'text': output}}})
            send({'method': 'turn/completed', 'params': {'threadId': p['threadId'], 'turn': {'id': 'summary-turn', 'status': 'completed'}}})
            return True
        if method == 'turn/start':
            self.inputs[p['threadId']] = p['input'][0]['text']
        if method == 'turn/steer' and p['input'][0]['text'].startswith('[Context integration /merge '):
            policy=self.steering.get(p['threadId'],'accept') if isinstance(self.steering,dict) else self.steering
            if policy == 'drop':
                return True  # Deliberately missing ACK, never retried.
            if policy == 'reject':
                send({'id': ident, 'error': {'code': -32600, 'message': 'Not admitted'}})
                return True
            self.inputs[p['threadId']] += '\n' + p['input'][0]['text']
        return False

    def finish(self, thread, turn, continuation=False):
        text = 'Both original perspectives and open alternatives are retained.' if '[Context integration /merge ' in self.inputs.get(thread, '') else 'Source research and correction retained.'
        self.histories.setdefault(thread, []).append({'id': turn, 'status': 'completed', 'items': [
            {'type': 'userMessage', 'content': [{'type': 'text', 'text': self.inputs.get(thread, '')}]},
            {'type': 'agentMessage', 'text': text}]})
        self.broadcast(thread, 'item/completed', {'turnId': turn, 'item': {'id': turn+'-answer', 'type': 'agentMessage', 'phase': 'final_answer', 'text': text}})
        self.broadcast(thread, 'turn/completed', {'turn': {'id': turn, 'status': 'completed'}})

    def ready(self):
        self.start()
        self.wait(lambda: bool(self.row("SELECT name FROM sqlite_master WHERE name='merge_operations'")))
        for topic, name, prompt in [(7, 'Memory research', 'Preserve original narrative. Discard the redundant continuity test. Keep architecture alternatives open.'),
                                    (8, 'Living context', 'Explore source revisits and inspiration. Divergent interpretations need not agree.')]:
            self.admit(prompt, topic=topic)
            self.wait(lambda: bool(self.row("SELECT id FROM turns WHERE status='completed' AND session_id IN (SELECT id FROM sessions WHERE thread_id=?)", (topic,))))
            self.idle()
            self.handled(self.admit('/rename '+name,topic=topic))

    def op(self, command):
        before=self.row('SELECT count(*) FROM merge_operations')[0][0]
        self.admit(command)
        self.wait(lambda:self.row('SELECT count(*) FROM merge_operations')[0][0] == before+1)
        self.wait(lambda:self.row("SELECT phase FROM merge_operations ORDER BY created_at DESC LIMIT 1") == [('ready',)])
        return self.row('SELECT id FROM merge_operations ORDER BY created_at DESC LIMIT 1')[0][0]


def modes(binary):
    j=Merge(binary)
    try:
        j.ready();before=j.row('SELECT thread_id,codex_thread_id FROM sessions ORDER BY thread_id')
        op=j.op('/merge "Living context" --focus "Research and imagination"')
        j.wait(lambda:j.row('SELECT phase FROM merge_legs WHERE operation=?',(op,)) == [('observed',)])
        j.idle()
        assert j.row('SELECT thread_id,codex_thread_id FROM sessions ORDER BY thread_id')==before
        packet=json.loads(j.row('SELECT doc FROM merge_operations WHERE id=?',(op,))[0][0])
        assert packet['policy']=='merge-research/v1' and len(packet['sources'])==2
        assert all(s['snapshot']['revision'] and s['snapshot']['boundary'] for s in packet['sources'])
        assert 'not your firsthand experiences' in j.inputs['native-topic-7']
        op=j.op('/merge "Living context" --both')
        j.wait(lambda:j.row('SELECT phase FROM merge_legs WHERE operation=? ORDER BY id',(op,))==[('observed',),('observed',)])
        j.idle()
        outputs=j.row('SELECT packet FROM merge_legs WHERE operation=? ORDER BY id',(op,))
        assert outputs[0][0]!=outputs[1][0] and 'recipient Memory research' in outputs[0][0] and 'recipient Living context' in outputs[1][0]
        snapshots=json.loads(j.row('SELECT doc FROM merge_operations WHERE id=?',(op,))[0][0])['sources']
        assert any('Previously received context' in e for s in snapshots for e in s['snapshot']['entries'])
        op=j.op('/merge "Living context" --new "Shared exploration"')
        j.wait(lambda:j.row('SELECT phase FROM merge_legs WHERE operation=?',(op,))==[('observed',)])
        j.idle()
        new=j.row("SELECT thread_id,codex_thread_id,session_title FROM sessions WHERE session_title='Shared exploration'")
        assert len(new)==1 and new[0][1] not in [s[1] for s in before]
        assert j.row('SELECT thread_id,codex_thread_id FROM sessions WHERE thread_id IN (7,8) ORDER BY thread_id')==before
        starts=j.native_turns;j.handled(j.admit('/merge resume '+op));assert j.native_turns==starts
        j.stop();j.start();j.handled(j.admit('/merge status'));assert j.native_turns==starts
        print('PASS three modes, actual native context/response, retained source bindings/titles, versioned provenance, imported-context attribution, two perspectives and restart non-replay')
    finally:j.close()


def active_and_partial(binary):
    j=Merge(binary)
    try:
        j.ready();j.mode='hold'
        for topic in (7,8):j.admit('Continue original research',topic=topic)
        j.wait(lambda:len(j.holding)==2)
        # Preserve a paused native goal while both writers are running.
        j.goals['native-topic-8']={'threadId':'native-topic-8','objective':'Original work','status':'paused','tokensUsed':4,'timeUsedSeconds':2}
        old_goal=dict(j.goals['native-topic-8']);j.steering={'native-topic-8':'drop'}
        op=j.op('/merge "Living context" --both')
        j.wait(lambda:j.row('SELECT phase FROM merge_legs WHERE operation=? ORDER BY id',(op,))==[('accepted',),('unconfirmed',)],seconds=30)
        doc=json.loads(j.row('SELECT doc FROM merge_operations WHERE id=?',(op,))[0][0])
        assert all(s['snapshot']['in_progress_entries'] for s in doc['sources'])
        assert 'provisional statements' in j.row('SELECT packet FROM merge_legs WHERE operation=? ORDER BY id',(op,))[0][0]
        assert j.goals['native-topic-8']==old_goal
        starts=j.native_turns;j.handled(j.admit('/merge resume '+op));assert j.native_turns==starts
        # Native inputs are not retried merely because their ACK was lost.
        assert len([r for r in j.rpc if r.get('method')=='turn/steer' and r['params']['input'][0]['text'].startswith('[Context integration /merge ')])==2
        j.mode='complete'
        for thread,turn in list(j.holding.items()):j.finish(thread,turn)
        j.idle()
        print('PASS active writers, paused goal preservation, partial bidirectional acceptance/uncertainty and explicit resume refusing replay')
    finally:j.close()


def scope_and_detached_restart(binary):
    j=Merge(binary)
    try:
        j.ready();starts=j.native_turns
        j.admit('/merge "Living context"',user=101)
        j.wait(lambda:any('Merge did not finish' in m['text'] for m,_ in j.messages));assert j.native_turns==starts
        j.admit('/merge Missing')
        j.wait(lambda:j.has('0 matching sources'));assert not j.row('SELECT id FROM merge_operations')
        with sqlite3.connect(j.root/'state.sqlite') as db:db.execute('UPDATE sessions SET busy=1 WHERE thread_id=7')
        op=j.op('/merge "Living context"')
        j.wait(lambda:j.row('SELECT phase FROM merge_legs WHERE operation=?',(op,))==[('detached_queue',)])
        retained=j.row('SELECT packet FROM merge_legs WHERE operation=?',(op,))
        j.stop();j.start();assert j.native_turns==starts
        with sqlite3.connect(j.root/'state.sqlite') as db:db.execute('UPDATE sessions SET busy=0 WHERE thread_id=7')
        j.admit('/merge resume '+op)
        j.wait(lambda:j.row('SELECT phase FROM merge_legs WHERE operation=?',(op,))==[('observed',)])
        assert j.row('SELECT packet FROM merge_legs WHERE operation=?',(op,))==retained
        assert j.native_turns==starts+1
        print('PASS source authorization/missing-source refusal, detached writer preservation, restart and explicit immutable unsent recovery')
    finally:j.close()


def incomplete_source_and_metadata(binary):
    import importlib.util
    spec=importlib.util.spec_from_file_location('merge_activity',pathlib.Path(__file__).resolve().parents[1]/'scripts/activity_indicator.py')
    activity=importlib.util.module_from_spec(spec);spec.loader.exec_module(activity)
    j=Merge(binary)
    try:
        j.ready();j.repeat_cursor=True;starts=j.native_turns
        update=j.admit('/merge "Living context" --new "Must not exist"');j.handled(update)
        assert j.row('SELECT phase FROM merge_operations')==[('preparation_failed',)]
        assert not j.row('SELECT id FROM merge_legs') and j.created==20 and j.native_turns==starts and j.summaries==0
        j.repeat_cursor=False
        with sqlite3.connect(j.root/'state.sqlite') as db:db.execute('UPDATE sessions SET busy=1 WHERE thread_id=7')
        op=j.op('/merge "Living context"');j.wait(lambda:j.row('SELECT phase FROM merge_legs WHERE operation=?',(op,))==[('detached_queue',)])
        with sqlite3.connect(j.root/'state.sqlite') as db:
            line=activity.merge_requests(db,100,7,100)
            assert 'waiting for detached writer' in line and 'narrative' not in line
            assert activity.merge_requests(db,100,7,101) is None
            assert activity.merge_requests(db,100,9,100) is None
            db.execute("UPDATE sessions SET codex_thread_id='replacement' WHERE thread_id=7")
            assert activity.merge_requests(db,100,7,100) is None
        j.handled(j.admit('/merge resume '+op));assert j.native_turns==starts
        assert j.row('SELECT phase FROM merge_legs WHERE operation=?',(op,))==[('detached_queue',)]
        print('PASS incomplete native pagination refuses before synthesis/topic mutation; metadata stays requester/topic/current-binding scoped and cannot follow replacement threads')
    finally:j.close()


if __name__=='__main__':
    binary=str(pathlib.Path(sys.argv[1]).resolve())
    modes(binary);active_and_partial(binary);scope_and_detached_restart(binary);incomplete_source_and_metadata(binary)
