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


if __name__ == '__main__':
    binary=str(pathlib.Path(sys.argv[1]).resolve())
    revisions_and_originals(binary)
    legacy_observer_session_links(binary)
