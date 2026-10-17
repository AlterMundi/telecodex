#!/usr/bin/env python3
"""Opt-in real native tool invocation/restoration on the retained synthetic fixture.
Adds one new integrating entry while preserving all previous native bindings and
reports. No source worker is resumed; only two selected entry model turns run.
Telegram remains synthetic. An existing attempt is never overwritten or replayed.
"""
import argparse
import hashlib
import json
import os
import pathlib
import re
import sqlite3
import sys
import time
from routing_native import NativeJourney


def qualify(args):
    initial=args.state/'qualification-observer-tools.json'
    output=args.state/('qualification-observer-tools-resume.json' if args.resume else 'qualification-observer-tools.json')
    assert not output.exists(), 'Inspect the retained attempt; do not replay qualification'
    if args.resume:
        prior=json.loads(initial.read_text())
        assert prior['success'] is False and prior.get('failure')=='observer method forbidden'
        assert prior['binary_sha256']==hashlib.sha256(pathlib.Path(args.binary).read_bytes()).hexdigest()

    assert json.loads((args.state/'qualification-observer.json').read_text())['success'] is True
    with sqlite3.connect(args.state/'state.sqlite') as db:
        previous=db.execute('SELECT thread_id,codex_thread_id,busy FROM sessions WHERE thread_id IN (1,7,8) ORDER BY thread_id').fetchall()
        entry_rows=db.execute('SELECT codex_thread_id,busy FROM sessions WHERE thread_id=9').fetchall()
        source_turns=db.execute('SELECT s.thread_id,count(t.id) FROM sessions s LEFT JOIN turns t ON t.session_id=s.id WHERE s.thread_id IN (1,7,8) GROUP BY s.id ORDER BY s.thread_id').fetchall()
    assert {r[0] for r in previous}=={1,7,8} and all(r[2]==0 for r in previous)
    assert (len(entry_rows)==1 and entry_rows[0][1]==0) if args.resume else not entry_rows
    sys.path.insert(0,str(pathlib.Path(__file__).resolve().parents[1]))
    from scripts.activity_indicator import Native
    reader=Native(args.codex)
    try:
        for _,native,_ in previous:
            node=reader.rpc('thread/read',{'threadId':native,'includeTurns':False})['thread']
            assert node['id']==native and node['status']['type'] in ('idle','notLoaded')
    finally:reader.close()
    report={'success':False,'binary_sha256':hashlib.sha256(pathlib.Path(args.binary).read_bytes()).hexdigest(),
            'scope':'one new native integrating entry; existing synthetic source bindings retained',
            'telegram_boundary':'synthetic','currency_cost':'unknown','entry_topic':9,'turns':[]}
    output.write_text(json.dumps(report,indent=2)+'\n')
    j=NativeJourney(args,reuse=True)
    entry=entry_rows[0][0] if args.resume else None
    try:
        config=j.config.read_text()
        if not args.resume:
            saved=args.state/'local-before-observer-tools.toml'
            assert not saved.exists(), 'Original configuration already retained'
            saved.write_text(config)
            prefix,section=config.split('[interaction]\n',1)
            section,count=re.subn(r'^topic_id=1$', 'topic_id=9',section,count=1,flags=re.M)
            assert count==1
            j.config.write_text(prefix+'[interaction]\n'+section)
        else:
            import tomllib
            assert tomllib.loads(config)['interaction']['topic_id']==9
            reader=Native(args.codex)
            try:
                restored=reader.rpc('thread/read',{'threadId':entry,'includeTurns':True})['thread']
                assert restored['id']==entry and restored['status']['type'] in ('idle','notLoaded')
                first=[t for t in restored['turns'] if any('NATIVE-OBSERVER-FIRST' in i.get('text','') for i in t.get('items',[]) if i.get('type')=='agentMessage')]
                assert len(first)==1 and first[0]['status']=='completed'
                calls=[i for i in first[0]['items'] if i.get('type')=='dynamicToolCall']
                assert calls and all(c.get('tool')=='interaction_candidates' and c.get('success') is True and c.get('arguments',{}).get('mode')=='inventory' for c in calls)
                assert all(i['type'] in ('userMessage','agentMessage','reasoning','dynamicToolCall') for i in first[0]['items'])
                report['turns'].append({'marker':'NATIVE-OBSERVER-FIRST','seconds':None,'native_turn':first[0]['id'],'tool_calls':[{'tool':c['tool'],'success':c['success'],'arguments':c['arguments']} for c in calls],'native_item_types':[i['type'] for i in first[0]['items']],'evidence':'Retained completed first turn recovered by supported thread/read, no model call repeated'})
            finally:reader.close()
        for marker in (('NATIVE-OBSERVER-RESUMED',) if args.resume else ('NATIVE-OBSERVER-FIRST','NATIVE-OBSERVER-RESUMED')):
            j.start()
            started=time.monotonic()
            before=j.row('SELECT count(*) FROM turns WHERE session_id IN (SELECT id FROM sessions WHERE thread_id=9)')[0][0]
            j.admit('Call interaction_candidates with mode inventory and selectors ["Alpha"]. Read the returned attributed source interpretation. Report its source revision and the recorded rate correction. Do not use other tools, write files, dispatch work or operate any source worker. If the tool is unavailable say so explicitly. End your concise answer with '+marker+'.',topic=9)
            j.wait(lambda:j.row("SELECT count(*) FROM turns WHERE status='completed' AND session_id IN (SELECT id FROM sessions WHERE thread_id=9)")==[(before+1,)],180)
            j.wait(lambda:j.has(marker,9),30)
            j.idle()
            selected=j.row('SELECT codex_thread_id FROM sessions WHERE thread_id=9')[0][0]
            if entry is None:entry=selected
            assert selected==entry, 'Resume replaced the native integrating binding'
            reader=Native(args.codex)
            try:
                history=reader.rpc('thread/read',{'threadId':entry,'includeTurns':True})
            finally:reader.close()
            assert history['thread']['id']==entry
            turns=[t for t in history['thread']['turns'] if any(marker in i.get('text','') for i in t.get('items',[]) if i.get('type')=='agentMessage')]
            assert len(turns)==1 and turns[0]['status']=='completed'
            assert all(i['type'] in ('userMessage','agentMessage','reasoning','dynamicToolCall') for i in turns[0]['items'])
            calls=[item for item in turns[0]['items'] if item.get('type')=='dynamicToolCall']
            assert calls and all(c.get('tool')=='interaction_candidates' and c.get('success') is True for c in calls), 'No successful actual native observer tool invocation'
            assert all(c.get('arguments',{}).get('mode')=='inventory' for c in calls), 'Unexpected non-inventory operation'
            final=j.row("SELECT assistant_text FROM turns WHERE session_id IN (SELECT id FROM sessions WHERE thread_id=9) ORDER BY id DESC LIMIT 1")[0][0]
            assert '17' in final, 'The known attributed rate correction was omitted'
            assert j.row('SELECT thread_id,codex_thread_id,busy FROM sessions WHERE thread_id IN (1,7,8) ORDER BY thread_id')==previous
            assert j.row('SELECT s.thread_id,count(t.id) FROM sessions s LEFT JOIN turns t ON t.session_id=s.id WHERE s.thread_id IN (1,7,8) GROUP BY s.id ORDER BY s.thread_id')==source_turns
            report['turns'].append({'marker':marker,'seconds':round(time.monotonic()-started,3),'native_turn':turns[0]['id'],
                'tool_calls':[{'tool':c['tool'],'success':c['success'],'arguments':c['arguments']} for c in calls],
                'answer':final,'native_item_types':[i['type'] for i in turns[0]['items']],'source_bindings_preserved':True,'source_turn_counts_preserved':True})
            output.write_text(json.dumps(report,indent=2)+'\n')
            j.stop()
        report.update(success=True,entry_native=entry,restoration_same_native_binding=True)
    except Exception as error:
        report['failure']=str(error)
        raise
    finally:
        try:
            if j.process and j.process.poll() is None and j.row('SELECT busy FROM sessions WHERE thread_id=9')==[(1,)]:
                j.admit('/stop',topic=9)  # This test's new entry only, never global Stop.
                j.wait(lambda:j.row('SELECT busy FROM sessions WHERE thread_id=9')==[(0,)],30)
            j.close()
        except Exception as error:report.update(success=False,cleanup='unconfirmed: '+str(error))
        reader=Native(args.codex)
        try:
            statuses={}
            for topic,native,_ in previous:
                node=reader.rpc('thread/read',{'threadId':native,'includeTurns':False})['thread']
                statuses[str(topic)]=node.get('status',{}).get('type')
                if node.get('id')!=native or statuses[str(topic)] not in ('idle','notLoaded'):report['success']=False
            if entry:
                node=reader.rpc('thread/read',{'threadId':entry,'includeTurns':False})['thread']
                report['entry_terminal_status']=node.get('status',{}).get('type')
                if node.get('id')!=entry or report['entry_terminal_status'] not in ('idle','notLoaded'):report['success']=False
            report['source_terminal_status']=statuses
        finally:reader.close()
        output.write_text(json.dumps(report,indent=2)+'\n')
        print(json.dumps({k:v for k,v in report.items() if k not in ('entry_native','turns','failure')},indent=2),flush=True)
    assert report['success'],report.get('failure','Native tool qualification incomplete')


if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('binary')
    p.add_argument('--state',required=True,type=pathlib.Path)
    p.add_argument('--codex',required=True)
    p.add_argument('--native-home',required=True,type=pathlib.Path)
    p.add_argument('--run-native',action='store_true')
    p.add_argument('--resume',action='store_true',help='Continue only the retained completed first turn after the known inspection-method error')
    args=p.parse_args()
    if not args.run_native:p.error('--run-native is required; this uses the selected real native account')
    if args.native_home.resolve()!=pathlib.Path(os.environ.get('CODEX_HOME',pathlib.Path.home()/'.codex')).resolve():p.error('Select only this process existing native home')
    qualify(args)
