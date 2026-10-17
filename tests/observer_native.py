#!/usr/bin/env python3
"""Opt-in factual observer qualification on the completed isolated routing fixture.
Reuses its exact source bindings. Only the ephemeral tool-disabled summarizer
runs a new model turn; no source worker or unrelated native thread is resumed.
The Telegram boundary remains synthetic. Existing reports are never overwritten.
"""
import argparse
import hashlib
import json
import os
import pathlib
import sqlite3
import sys
import time
from routing_native import NativeJourney


def qualify(args):
    prior = json.loads((args.state / 'qualification-resume.json').read_text())
    assert prior.get('success') is True, 'A completed routing fixture is required'
    output = args.state / 'qualification-observer.json'
    assert not output.exists(), 'Inspect the retained observer attempt; do not replay an existing qualification'
    with sqlite3.connect(args.state / 'state.sqlite') as db:
        bindings = db.execute('SELECT thread_id,codex_thread_id,busy FROM sessions ORDER BY thread_id').fetchall()
        before_turns = db.execute('SELECT count(*) FROM turns').fetchone()[0]
    assert {row[0] for row in bindings} == {1,7,8} and all(row[2] == 0 for row in bindings)
    sys.path.insert(0,str(pathlib.Path(__file__).resolve().parents[1]))
    from scripts.activity_indicator import Native
    reader = Native(args.codex)
    try:
        source = next(row[1] for row in bindings if row[0] == 7)
        observed = reader.rpc('thread/read', {'threadId':source,'includeTurns':False})
        assert observed.get('thread',{}).get('id') == source
        assert observed['thread']['status']['type'] in ('idle','notLoaded')
    finally:
        reader.close()
    report = {'success':False,'binary_sha256':hashlib.sha256(pathlib.Path(args.binary).read_bytes()).hexdigest(),
              'source_fixture':'completed routing qualification; exact retained bindings',
              'telegram_boundary':'synthetic','source_worker_resumed':False,'currency_cost':'unknown',
              'factual_quality_review':'pending review of summary and original support'}
    output.write_text(json.dumps(report,indent=2)+'\n')
    j = NativeJourney(args,reuse=True)
    try:
        j.start()
        j.wait(lambda: bool(j.row("SELECT name FROM sqlite_master WHERE name='interaction_observer_cards'")))
        assert j.row('SELECT count(*) FROM interaction_observer_attempts') == [(0,)], 'Existing attempts must be inspected, never overwritten'
        started = time.monotonic()
        j.admit('/interaction observe #Alpha',topic=1)
        def card():
            rows=j.row('SELECT doc FROM interaction_observer_cards ORDER BY revision DESC LIMIT 1')
            return json.loads(rows[0][0]) if rows else None
        j.wait(lambda:card() is not None,300)
        selected = card()
        report['summary_seconds'] = round(time.monotonic()-started,3)
        assert selected['native'] == source and selected['revision'] == 1
        original = json.loads(j.row("SELECT source_doc FROM interaction_observer_attempts WHERE phase='committed'")[0][0])
        entries = original['entries']+original['in_progress_entries']
        assert all(0 <= n < len(entries) for n in selected['supports'])
        assert 'Change the rate to 17' in '\n'.join(entries), 'Known correction is absent from the captured source'
        assert '17' in selected['summary'], 'Known rate correction was omitted from the summary'
        assert j.row('SELECT count(*) FROM turns') == [(before_turns,)]
        assert j.row('SELECT thread_id,codex_thread_id,busy FROM sessions ORDER BY thread_id') == bindings
        j.admit('/interaction observe #Alpha',topic=1)
        j.wait(lambda:j.has('Source unchanged; no synthesis call',1),90)
        assert j.row('SELECT count(*) FROM interaction_observer_attempts') == [(1,)]
        assert j.row('SELECT count(*) FROM interaction_observer_cards') == [(1,)]
        report.update(success=True,card=selected,selected_original_support=[{'index':n,'text':entries[n]} for n in selected['supports']],
                      unchanged_source_reused=True,source_bindings_preserved=True,source_turn_count_preserved=True)
    except Exception as error:
        report['failure']=str(error)
        raise
    finally:
        try:
            j.close()
        except Exception as error:
            report.update(success=False,listener_cleanup='unconfirmed: '+str(error))
        reader = Native(args.codex)
        try:
            observed = reader.rpc('thread/read',{'threadId':source,'includeTurns':False})
            report['source_native_terminal_status']=observed.get('thread',{}).get('status',{}).get('type')
            if observed.get('thread',{}).get('id') != source or report['source_native_terminal_status'] not in ('idle','notLoaded'):
                report['success']=False
        finally:
            reader.close()
        output.write_text(json.dumps(report,indent=2)+'\n')
        print(json.dumps({k:v for k,v in report.items() if k not in ('card','selected_original_support','failure')},indent=2),flush=True)
    assert report['success'],report.get('failure','Observer qualification did not complete')


if __name__ == '__main__':
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('binary')
    p.add_argument('--state',required=True,type=pathlib.Path)
    p.add_argument('--codex',required=True)
    p.add_argument('--native-home',required=True)
    p.add_argument('--run-native',action='store_true')
    args=p.parse_args()
    args.resume=False
    if not args.run_native:p.error('--run-native is required; this uses the selected real native account')
    selected=pathlib.Path(os.environ.get('CODEX_HOME',pathlib.Path.home()/'.codex')).resolve()
    if pathlib.Path(args.native_home).resolve()!=selected:p.error('Select only this process existing native home')
    qualify(args)
