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
        j.histories['native-topic-7'].append({'id':'new-source-turn','status':'completed','items':[
            {'type':'userMessage','content':[{'type':'text','text':'A new factual correction: retain rate 17.'}]},
            {'type':'agentMessage','text':'The revised requirement is retained as source evidence.'}]})
        j.block_summary = True
        j.admit('/interaction observe #Alpha', topic=1)
        j.wait(j.summary_held.is_set)
        j.admit('/interaction observe #Alpha', topic=1)
        j.wait(lambda: j.has('Observer attempt is summarizing', 1))
        assert j.summaries == 2, 'Concurrent refresh duplicated a synthesis call'
        j.release_summary.set()
        j.wait(lambda: j.card()['revision'] == 2)
        raw = json.loads(j.row("SELECT source_doc FROM interaction_observer_attempts WHERE phase='committed' ORDER BY rowid DESC LIMIT 1")[0][0])
        assert raw['delta_indices'] == [2] and len(raw['entries']) == 3
        assert j.card()['boundary'] == 'new-source-turn'
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
        assert j.row('SELECT count(*) FROM turns') == before_turns
        print('PASS observer original support, changed-source deltas, unchanged/concurrent deduplication, atomic revision/cursor, visible gaps and read-only restart inspection')
    finally:
        j.release_summary.set()
        j.close()


if __name__ == '__main__':
    revisions_and_originals(str(pathlib.Path(sys.argv[1]).resolve()))
