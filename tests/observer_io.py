#!/usr/bin/env python3
"""Derived observer cards through synthetic Telegram/native/SQLite boundaries.
No real model, Telegram or other being is contacted.
"""
import json
import pathlib
import sys
import threading
from merge_io import Merge


class Observer(Merge):
    def __init__(self, binary):
        self.hold_source = False
        self.block_summary = False
        self.summary_held = threading.Event()
        self.release_summary = threading.Event()
        super().__init__(binary)

    def handle_extra_rpc(self, request, send):
        p = request.get('params', {})
        if self.block_summary and request.get('method') == 'turn/start' and p.get('threadId', '').startswith('summary-'):
            self.summary_held.set()
            self.release_summary.wait(timeout=20)
        return super().handle_extra_rpc(request, send)

    def finish(self, thread, turn, continuation=False):
        if self.hold_source and thread == 'native-topic-7':
            self.holding[thread] = turn
            return
        return super().finish(thread, turn, continuation)

    def prepare(self):
        self.ready()
        self.admit('Correct the decision using source evidence, preserving alternatives.', topic=7)
        self.wait(lambda: len(self.histories['native-topic-7']) == 2)
        self.idle()
        self.stop()
        with self.config.open('a') as f:
            f.write('''\n[interaction]
human_id=100
chat_id=100
topic_id=1
[[interaction.targets]]
name="Alpha"
topic_id=7
[[interaction.targets]]
name="Beta"
topic_id=8
''')
        self.messages.clear()
        self.start()

    def card(self):
        rows = self.row('SELECT doc FROM interaction_observer_cards ORDER BY revision DESC LIMIT 1')
        return json.loads(rows[0][0]) if rows else None


def revisions_and_originals(binary):
    j = Observer(binary)
    try:
        j.prepare()
        before_turns = j.row('SELECT count(*) FROM turns')
        j.admit('/interaction observe #Alpha', topic=1)
        j.wait(lambda: j.card() is not None)
        first = j.card()
        assert first['revision'] == 1 and not first['gap'] and first['native'] == 'native-topic-7'
        assert first['source_revision'] and first['boundary'] and first['supports']
        assert j.summaries == 1 and j.row('SELECT count(*) FROM turns') == before_turns
        j.wait(lambda: j.has('Derived observer card', 1))
        assert j.has('no local change since source read', 1)
        j.messages.clear()
        before = len(j.rpc)
        j.admit('/interaction summary #Alpha', topic=1)
        j.wait(lambda: j.has('Derived observer card', 1))
        assert not j.rpc[before:], 'Inspecting a retained card touched the native source'
        j.messages.clear()
        j.admit('/interaction sources #Alpha', topic=1)
        j.wait(lambda: j.has('Original support', 1))
        assert j.has('Preserve original narrative', 1) and j.has('Correct the decision', 1)
        j.admit('/interaction observe #Alpha', topic=1)
        j.wait(lambda: j.has('Source unchanged; no synthesis call', 1))
        assert j.summaries == 1 and j.card()['revision'] == 1
        j.hold_source = True
        j.admit('A new factual correction: retain rate 17.',topic=7)
        j.wait(lambda:'native-topic-7' in j.holding)
        j.wait(lambda:j.row("SELECT phase FROM interaction_instructions ORDER BY sequence DESC LIMIT 1") == [('accepted',)])
        j.block_summary = True
        j.admit('/interaction observe #Alpha', topic=1)
        j.wait(j.summary_held.is_set)
        source_turn = j.holding['native-topic-7']
        j.broadcast('native-topic-7','item/completed',{'turnId':source_turn,'item':{'type':'agentMessage','id':'during-summary','phase':'commentary','text':'New source activity while the observer is synthesizing'}})
        j.wait(lambda:j.row("SELECT count(*) FROM interaction_native_messages WHERE item_id='during-summary'") == [(1,)])
        j.admit('/interaction observe #Alpha', topic=1)
        j.wait(lambda: j.has('Observer attempt is summarizing', 1))
        assert j.summaries == 2, 'Concurrent refresh duplicated a synthesis call'
        j.release_summary.set()
        j.wait(lambda: j.card()['revision'] == 2)
        raw = json.loads(j.row("SELECT source_doc FROM interaction_observer_attempts WHERE phase='committed' ORDER BY rowid DESC LIMIT 1")[0][0])
        assert raw['delta_indices'] == [2] and len(raw['entries'])+len(raw['in_progress_entries']) == 3
        assert j.card()['boundary'] == first['boundary'], 'An in-progress source advanced the completed cursor'
        assert raw['unfinished_observed'] is True
        j.wait(lambda:j.has('new local activity since source read',1))
        j.hold_source = False
        j.holding.pop('native-topic-7')
        j.finish('native-topic-7',source_turn)
        j.idle()
        j.histories['native-topic-7'] = [dict(turn, id='after-gap-'+str(n)) for n,turn in enumerate(j.histories['native-topic-7'][-2:])]
        j.admit('/interaction observe #Alpha', topic=1)
        j.wait(lambda: j.card()['revision'] == 3)
        assert j.card()['gap'], 'Missing previous cursor was silently treated as contiguous history'
        j.stop()
        j.start()
        j.messages.clear()
        count = j.summaries
        j.admit('/interaction summary #Alpha', topic=1)
        j.wait(lambda: j.has('Revision: 3', 1))
        assert j.summaries == count and j.has('continuity gap: true', 1)
        assert j.has('native reconciliation required after restart',1)
        j.messages.clear()
        j.admit('/interaction observe #Alpha',topic=1)
        j.wait(lambda:j.has('Source unchanged; no synthesis call',1))
        assert j.has('no local change since source read',1) and j.summaries == count
        assert j.row('SELECT count(*) FROM turns') == [(before_turns[0][0]+1,)]
        print('PASS observer original support, changed-source deltas, unchanged/concurrent deduplication, atomic revision/cursor, visible gaps and read-only restart inspection')
    finally:
        j.release_summary.set()
        j.close()


def legacy_observer_session_links(binary):
    j = Observer(binary)
    try:
        j.prepare()
        j.stop()
        import sqlite3
        with sqlite3.connect(j.root / 'state.sqlite') as db:
            session = db.execute('SELECT id FROM sessions WHERE thread_id=7').fetchone()[0]
            db.executescript("""DROP TABLE interaction_observer_cards;
                DROP TABLE interaction_observer_attempts;
                CREATE TABLE interaction_observer_attempts(id TEXT PRIMARY KEY,session_id INTEGER NOT NULL REFERENCES sessions(id),human_id INTEGER NOT NULL,native_thread TEXT NOT NULL,source_revision TEXT NOT NULL,policy TEXT NOT NULL,expected_revision INTEGER NOT NULL,source_doc TEXT NOT NULL,observed_at TEXT NOT NULL,phase TEXT NOT NULL,instance_id TEXT NOT NULL);
                CREATE TABLE interaction_observer_cards(session_id INTEGER NOT NULL REFERENCES sessions(id),native_thread TEXT NOT NULL,revision INTEGER NOT NULL,attempt_id TEXT NOT NULL UNIQUE REFERENCES interaction_observer_attempts(id),doc TEXT NOT NULL,PRIMARY KEY(session_id,native_thread,revision));""")
            original = json.dumps({'native':'native-topic-7','revision':'legacy-source','entries':['Legacy original evidence'],'in_progress_entries':[]})
            card = json.dumps({'revision':1,'native':'native-topic-7','source_revision':'legacy-source','boundary':'legacy-turn','policy':'telecodex.observer/v1','summary':'Legacy retained interpretation','supports':[0],'gap':False,'bounded':True,'observed_at':'previous-source','committed_at':'previous-commit'})
            db.execute("INSERT INTO interaction_observer_attempts VALUES('legacy',?,100,'native-topic-7','legacy-source','telecodex.observer/v1',0,?,'previous-source','committed','previous-instance')",(session,original))
            db.execute("INSERT INTO interaction_observer_cards VALUES(?,'native-topic-7',1,'legacy',?)",(session,card))
        j.messages.clear()
        j.start()
        j.admit('/interaction summary #Alpha',topic=1)
        j.wait(lambda:j.has('Legacy retained interpretation',1))
        j.stop()
        with sqlite3.connect(j.root / 'state.sqlite') as db:
            db.execute('PRAGMA foreign_keys=ON')
            assert db.execute('PRAGMA foreign_key_check').fetchall() == []
            db.execute('DELETE FROM sessions WHERE id=?',(session,))
            assert db.execute('SELECT source_doc FROM interaction_observer_attempts').fetchone()[0] == original
            assert db.execute('SELECT doc FROM interaction_observer_cards').fetchone()[0] == card
        print('PASS legacy observer schema migrates on actual startup; session removal preserves original evidence and summaries')
    finally:
        j.close()


def candidate_reconciliation(binary):
    import sqlite3
    j = Observer(binary)
    try:
        j.prepare()
        j.admit('Retain the second original Beta source perspective.',topic=8)
        j.wait(lambda:len(j.histories['native-topic-8']) == 2)
        j.idle()
        before_turns = j.row('SELECT count(*) FROM turns')
        j.admit('/interaction candidates',topic=1)
        j.wait(lambda:j.row('SELECT count(*) FROM interaction_observer_cards') == [(2,)])
        j.wait(lambda:j.has('Derived observer card for #Alpha',1) and j.has('Derived observer card for #Beta',1))
        assert j.summaries == 2 and j.row('SELECT count(*) FROM turns') == before_turns
        j.messages.clear()
        before_rpc = len(j.rpc)
        j.admit('/interaction candidates',topic=1)
        j.wait(lambda:j.has('Derived observer card for #Beta',1))
        assert j.has('recent locally reconciled card reused',1)
        assert len(j.rpc) == before_rpc and j.summaries == 2, 'Unchanged candidates incurred a native/model call'
        # Expired sampling is reconciled even without a local event; unchanged
        # original history still must not incur another synthesis.
        with sqlite3.connect(j.root / 'state.sqlite') as db:
            db.execute("UPDATE interaction_observer_checks SET checked_at='2020-01-01T00:00:00Z' WHERE native_thread='native-topic-7'")
        j.messages.clear()
        j.admit('/interaction candidates',topic=1)
        j.wait(lambda:j.has('source reconciled; unchanged summary reused',1))
        assert j.summaries == 2
        j.admit('A factual correction that changes Alpha evidence.',topic=7)
        j.wait(lambda:len(j.histories['native-topic-7']) == 3)
        j.idle()
        before_turns = j.row('SELECT count(*) FROM turns')
        j.messages.clear()
        j.admit('/interaction candidates',topic=1)
        j.wait(lambda:j.row('SELECT count(*) FROM interaction_observer_cards') == [(3,)])
        j.wait(lambda:j.has('source reconciled; summary updated',1))
        assert j.summaries == 3 and j.row('SELECT count(*) FROM turns') == before_turns
        j.stop()
        j.start()
        j.messages.clear()
        j.admit('/interaction candidates',topic=1)
        j.wait(lambda:j.has('Derived observer card for #Beta',1))
        assert j.summaries == 3
        assert j.has('source reconciled; unchanged summary reused',1)
        assert not j.has('native reconciliation required after restart',1)
        # External native history changes have no local canonical event. A
        # newer held observation must invalidate reuse of the recent old card.
        j.histories['native-topic-7'][-1]['items'].append({'type':'agentMessage','text':'An external-only factual change.'})
        j.block_summary = True
        j.admit('/interaction observe #Alpha',topic=1)
        j.wait(j.summary_held.is_set)
        count = j.summaries
        j.messages.clear()
        j.admit('/interaction candidates',topic=1)
        j.wait(lambda:j.has('refresh retained as summarizing',1))
        assert j.has('newer native source synthesis is pending or uncertain',1)
        assert j.summaries == count, 'A retained external source caused duplicate synthesis'
        j.release_summary.set()
        j.wait(lambda:j.row('SELECT count(*) FROM interaction_observer_cards') == [(4,)])
        print('PASS candidate query seeds bounded cards, reuses recent unchanged sources without native calls, refreshes expired/event-stale/restarted sources without duplicate synthesis or worker execution')
    finally:
        j.release_summary.set()
        j.close()


def synthesis_profile_and_global_budget(binary):
    j = Observer(binary)
    try:
        j.prepare()
        j.admit('A second retained Beta perspective.', topic=8)
        j.wait(lambda:len(j.histories['native-topic-8'])==2)
        j.idle()
        for n in range(2):
            j.admit('A third independent source perspective '+str(n), topic=9)
            j.wait(lambda: len(j.histories.get('native-topic-9', [])) == n+1)
            j.idle()
        j.stop()
        config=j.config.read_text().replace('[interaction]\n','[interaction]\nobserver_model="observer-fixture-model"\nobserver_reasoning_effort="low"\n',1)
        config+='\n[[interaction.targets]]\nname="Gamma"\ntopic_id=9\n'
        j.config.write_text(config)
        before=j.row('SELECT thread_id,codex_thread_id,model,reasoning_effort FROM sessions WHERE thread_id IN (7,8,9) ORDER BY thread_id')
        j.start();j.messages.clear();j.block_summary=True
        j.admit('/interaction observe #Alpha', topic=1)
        j.admit('/interaction observe #Beta', topic=1)
        j.wait(lambda:j.summaries==2)
        j.wait(lambda:len([r for r in j.rpc if r.get('method')=='turn/start' and r['params'].get('threadId','').startswith('summary-')])==2)
        update=j.admit('/interaction observe #Gamma', topic=1)
        j.handled(update)
        # A retained third attempt proves it reached the shared synthesis queue.
        j.wait(lambda:j.row("SELECT count(*) FROM interaction_observer_attempts WHERE phase='summarizing'")==[(3,)])
        assert j.summaries==2, 'Concurrent commands bypassed receiver-wide observer capacity'
        # Inspection of an already pending source must not queue behind synthesis.
        j.admit('/interaction observe #Alpha', topic=1)
        j.wait(lambda:j.has('Observer attempt is summarizing',1))
        assert j.summaries==2
        # Ordinary foreground input still executes while both synthesis slots hold.
        j.admit('Foreground conversation stays responsive.', topic=10)
        j.wait(lambda:len(j.histories.get('native-topic-10',[]))==1)
        assert j.summaries==2
        j.release_summary.set()
        j.wait(lambda:j.row('SELECT count(*) FROM interaction_observer_cards')==[(3,)])
        starts=[r['params'] for r in j.rpc if r.get('method')=='thread/start' and r.get('params',{}).get('ephemeral')]
        turns=[r['params'] for r in j.rpc if r.get('method')=='turn/start' and r.get('params',{}).get('threadId','').startswith('summary-')]
        assert len(starts)==len(turns)==3 and all(p['model']=='observer-fixture-model' for p in starts)
        assert all(p['effort']=='low' for p in turns)
        after=j.row('SELECT thread_id,codex_thread_id,model,reasoning_effort FROM sessions WHERE thread_id IN (7,8,9) ORDER BY thread_id')
        assert after==before, 'Observer settings mutated original work-session bindings or preferences'
        print('PASS explicit observer model/effort, receiver-wide two-synthesis bound, responsive pending inspection and foreground work, unchanged source preferences')
    finally:
        j.release_summary.set();j.close()


def failed_synthesis_diagnostic(binary):
    j = Observer(binary)
    try:
        j.prepare()
        original = j.handle_extra_rpc
        def invalid_support(request, send):
            def substitute(value):
                if value.get('method') == 'item/completed':
                    item = value.get('params', {}).get('item', {})
                    if item.get('id') == 'summary-answer':
                        item['text'] = json.dumps({'summary': 'Private derived text', 'recent_indices': [999]})
                send(value)
            return original(request, substitute)
        j.handle_extra_rpc = invalid_support
        j.admit('/interaction observe #Alpha', topic=1)
        j.wait(lambda:j.row('SELECT phase FROM interaction_observer_attempts') == [('uncertain',)])
        j.wait(lambda:j.has('Observer refresh could not be confirmed',1))
        j.admit('/interaction observe #Alpha',topic=1)
        j.wait(lambda:j.has('Observer attempt is uncertain',1))
        assert j.summaries == 1 and j.card() is None
        import signal
        j.process.send_signal(signal.SIGINT)
        output, error = j.process.communicate(timeout=8)
        j.process = None
        logs = output + error
        assert 'support_outside_source' in logs
        assert 'Private derived text' not in logs
        assert 'Preserve original narrative' not in logs
        print('PASS failed synthesis emits bounded diagnostic without private text or automatic retry')
    finally:
        j.close()


def owner_observer_commands(binary):
    import subprocess
    j = Observer(binary)
    try:
        j.prepare(); j.stop()
        socket = j.root / 'observer-control.sock'
        j.config.write_text(j.config.read_text() + f'\n[session_control]\nsocket_path="{socket}"\nhuman_id=101\n')
        def control(*args):
            return subprocess.run([binary,'--control',str(j.config),'observer',*args],capture_output=True,text=True,timeout=15)
        j.start(); j.wait_control_socket(socket)
        assert control('observe','#Alpha').returncode != 0
        assert j.summaries == 0, 'Different configured owner reached the observer'
        j.stop(); j.config.write_text(j.config.read_text().replace('human_id=101','human_id=100'))
        j.start(); j.wait_control_socket(socket)
        before = j.row('SELECT thread_id,codex_thread_id FROM sessions ORDER BY thread_id')
        before_turns = j.row('SELECT count(*) FROM turns')
        assert control('observe','#Missing').returncode != 0
        assert control('observe','#Alpha','--unknown').returncode != 0
        original = j.handle_extra_rpc
        def invalid_support(request, send):
            def substitute(value):
                item = value.get('params',{}).get('item',{})
                if value.get('method') == 'item/completed' and item.get('id') == 'summary-answer':
                    item['text'] = json.dumps({'summary':'Derived text','recent_indices':[999]})
                send(value)
            return original(request, substitute)
        j.handle_extra_rpc = invalid_support
        reply = control('observe','#Alpha')
        assert reply.returncode == 0, reply.stderr
        assert json.loads(reply.stdout)['result']['synthesis_confirmed'] is False
        j.wait(lambda:j.row('SELECT phase FROM interaction_observer_attempts') == [('uncertain',)])
        assert control('observe','#Alpha').returncode == 0
        j.wait(lambda:bool(j.row("SELECT id FROM audit_log WHERE action='canonical_routing_notice' AND details_json LIKE '%Observer attempt is uncertain%'")))
        assert j.summaries == 1
        j.handle_extra_rpc = original
        assert control('observe','#Alpha','--retry').returncode == 0
        j.wait(lambda:j.card() is not None)
        assert j.row('SELECT phase FROM interaction_observer_attempts ORDER BY rowid') == [('uncertain',),('committed',)]
        assert j.summaries == 2
        for command, notice in [('summary','Derived observer card'),('sources','Original support')]:
            reply = control(command,'#Alpha')
            assert reply.returncode == 0, reply.stderr
            inspected = json.loads(reply.stdout)['result']
            assert inspected['model_call'] is False and notice in inspected['text']
        assert j.summaries == 2 and j.row('SELECT count(*) FROM turns') == before_turns
        assert j.row('SELECT thread_id,codex_thread_id FROM sessions ORDER BY thread_id') == before
        print('PASS owner observer commands enforce human/source scope, retain uncertainty, allow explicit retry and never dispatch source work')
    finally:
        j.close()


if __name__ == '__main__':
    binary=str(pathlib.Path(sys.argv[1]).resolve())
    revisions_and_originals(binary)
    legacy_observer_session_links(binary)
    candidate_reconciliation(binary)
    synthesis_profile_and_global_budget(binary)
    failed_synthesis_diagnostic(binary)
    owner_observer_commands(binary)
