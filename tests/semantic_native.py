#!/usr/bin/env python3
"""Opt-in actual semantic dispatch on the retained synthetic native fixture.
Creates one stable controller-enabled integrating entry; preserves earlier entries
and worker bindings. Completed attempts are never replayed. Telegram is synthetic.
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


def selected_topics(destinations):
    aliases={'alpha':7,'beta':8,'t7':7,'t8':8}
    return {aliases[d['selector'].lstrip('#').casefold()] for d in destinations}


def inspect_completed(args):
    """Verify retained completed work after the selector-spelling assertion only.
    This path starts no listener, model turn or worker and never changes reports.
    """
    prior=json.loads((args.state/'qualification-semantic.json').read_text())
    output=args.state/'qualification-semantic-inspected.json'
    assert not output.exists()
    assert prior['success'] is False and prior.get('failure')==''
    assert prior['binary_sha256']==hashlib.sha256(pathlib.Path(args.binary).read_bytes()).hexdigest()
    assert set(prior['terminal_statuses'])=={'1','7','8','9','10'}
    assert all(v in ('idle','notLoaded') for v in prior['terminal_statuses'].values())
    with sqlite3.connect(args.state/'state.sqlite') as db:
        bindings=db.execute('SELECT thread_id,codex_thread_id,busy FROM sessions ORDER BY thread_id').fetchall()
        assert {r[0] for r in bindings}=={1,7,8,9,10} and all(r[2]==0 for r in bindings)
        original=prior['original_update']
        source=json.loads(db.execute('SELECT doc FROM interaction_sources WHERE update_id=?',(original,)).fetchone()[0])
        rows=db.execute('SELECT i.id,i.doc,i.phase FROM interaction_instructions i JOIN interaction_sources s ON s.id=i.input_id WHERE s.update_id=?',(original,)).fetchall()
        assert len(rows)==2 and all(phase=='accepted' and json.loads(doc)['text']==source['text'] for _,doc,phase in rows)
        assert {json.loads(doc)['target']['thread_id'] for _,doc,_ in rows}=={7,8}
        deliveries=db.execute('SELECT d.phase,d.doc,d.message_id FROM interaction_deliveries d WHERE d.request_id IN (SELECT l.request_id FROM interaction_request_links l JOIN interaction_instructions i ON i.id=l.instruction_id JOIN interaction_sources s ON s.id=i.input_id WHERE s.update_id=?)',(original,)).fetchall()
        assert len(deliveries)==2 and all(phase=='delivered' and mid is not None for phase,_,mid in deliveries)
        docs=[json.loads(doc) for _,doc,_ in deliveries]
        assert {doc['body'] for doc in docs}=={'SEMANTIC-ALPHA','SEMANTIC-BETA'}
        assert all(doc['origin']==source['origin'] and doc['completed'] and doc['declared'] and doc['current'] and not doc['correction_pending'] for doc in docs)
        assert {c for doc in docs for c in doc['covered']}=={ident for ident,_,_ in rows}
    sys.path.insert(0,str(pathlib.Path(__file__).resolve().parents[1]))
    from scripts.activity_indicator import Native
    reader=Native(args.codex)
    try:
        statuses={}
        for topic,native,_ in bindings:
            node=reader.rpc('thread/read',{'threadId':native,'includeTurns':topic==10})['thread']
            assert node['id']==native and node['status']['type'] in ('idle','notLoaded')
            statuses[str(topic)]=node['status']['type']
            if topic==10:history=node
    finally:reader.close()
    calls=[i for t in history['turns'] for i in t.get('items',[]) if i.get('type')=='dynamicToolCall']
    assert calls and all(c.get('success') is True for c in calls)
    assert all(i['type'] in ('userMessage','agentMessage','reasoning','dynamicToolCall') for t in history['turns'] for i in t.get('items',[]))
    dispatch=[c for c in calls if c.get('tool')=='interaction_controller' and c.get('arguments',{}).get('operation')=='dispatch']
    assert len(dispatch)==1 and dispatch[0]['arguments']['arguments']['input_update']==original
    assert selected_topics(dispatch[0]['arguments']['arguments']['destinations'])=={7,8}
    alpha=json.loads((args.state/'semantic-alpha.json').read_text())
    beta=json.loads((args.state/'semantic-beta.json').read_text())
    assert alpha=={'rate':19,'amounts':[76,133,38],'total':247,'provenance':{'human':100,'via':'interaction-controller'}}
    assert beta=={'red':14,'blue':22,'total':36}
    result=dict(prior,success=True,failure=None,inspection='Read-only verification of completed original attempt; no model calls repeated. Original assertion expected label spelling but native selected equivalent stable t7/t8 routes.',
                actual_tool_calls=[{'tool':c['tool'],'arguments':c.get('arguments'),'success':c.get('success')} for c in calls],
                alpha=alpha,beta=beta,original_text_preserved=True,delivered_to_original_entry=True,terminal_statuses=statuses)
    output.write_text(json.dumps(result,indent=2)+'\n')
    print('PASS retained actual native semantic selection, atomic two-target dispatch, original human text and provenance, independently checked calculation files, worker coverage, correlated synthetic returns and terminal status; no completed model work replayed',flush=True)


def qualify(args):
    output=args.state/'qualification-semantic.json'
    assert not output.exists(), 'Inspect the retained attempt; do not replay it'
    assert json.loads((args.state/'qualification-observer-tools-resume.json').read_text())['success'] is True
    with sqlite3.connect(args.state/'state.sqlite') as db:
        previous=db.execute('SELECT thread_id,codex_thread_id,busy FROM sessions ORDER BY thread_id').fetchall()
    assert {r[0] for r in previous}=={1,7,8,9} and all(r[2]==0 for r in previous)
    assert all(not (args.state/name).exists() for name in ('semantic-alpha.json','semantic-beta.json'))
    sys.path.insert(0,str(pathlib.Path(__file__).resolve().parents[1]))
    from scripts.activity_indicator import Native
    reader=Native(args.codex)
    try:
        for _,native,_ in previous:
            node=reader.rpc('thread/read',{'threadId':native,'includeTurns':False})['thread']
            assert node['id']==native and node['status']['type'] in ('idle','notLoaded')
    finally:reader.close()
    report={'success':False,'binary_sha256':hashlib.sha256(pathlib.Path(args.binary).read_bytes()).hexdigest(),
            'scope':'one controller-enabled native integrating entry and two retained synthetic workers',
            'entry_topic':10,'telegram_boundary':'synthetic','currency_cost':'unknown'}
    output.write_text(json.dumps(report,indent=2)+'\n')
    j=NativeJourney(args,reuse=True)
    entry=None
    try:
        config=j.config.read_text()
        saved=args.state/'local-before-semantic.toml'
        assert not saved.exists()
        saved.write_text(config)
        prefix,section=config.split('[interaction]\n',1)
        section,count=re.subn(r'^topic_id=9$', 'topic_id=10',section,count=1,flags=re.M)
        assert count==1
        j.config.write_text(prefix+'[interaction]\n'+section)
        j.start()
        started=time.monotonic()
        original=j.admit('''Please have Alpha and Beta work in parallel on their respective existing calculations. This is an isolated functional qualification; no network, goals or subagents. Each assigned worker should perform only its own named task in this fixture cwd and retain the controller result-envelope contract.
Alpha: calculate quantities [4,7,2] at rate 19. Write the internal fixture semantic-alpha.json with rate, amounts and total, plus provenance containing the actual original human author and relay identifier from the controller packet. Report SEMANTIC-ALPHA after writing it.
Beta: group red=[8,6] and blue=[10,12], write internal fixture semantic-beta.json with red, blue and total, and report SEMANTIC-BETA after writing it.
These are internal test files, not Telegram deliverables. Integrating conversation: use your controller tools to select this retained original input and hand it to both existing destinations; do not implement their work yourself or rewrite the human input. After inspecting retained dispatch outcomes, answer SEMANTIC-DISPATCHED without claiming their completion until actual worker results arrive.''',topic=10)
        report['original_update']=original
        output.write_text(json.dumps(report,indent=2)+'\n')
        j.wait(lambda:j.row('SELECT count(*) FROM interaction_sources WHERE update_id=?',(original,))==[(1,)],180)
        report['admission_seconds']=round(time.monotonic()-started,3)
        j.wait(lambda:j.has('SEMANTIC-DISPATCHED',10),180)
        j.wait(lambda:j.has('SEMANTIC-ALPHA',10) and j.has('SEMANTIC-BETA',10),240)
        j.idle()
        entry=j.row('SELECT codex_thread_id FROM sessions WHERE thread_id=10')[0][0]
        reader=Native(args.codex)
        try:history=reader.rpc('thread/read',{'threadId':entry,'includeTurns':True})['thread']
        finally:reader.close()
        calls=[i for t in history['turns'] for i in t.get('items',[]) if i.get('type')=='dynamicToolCall']
        assert calls and all(c.get('success') is True for c in calls)
        assert all(i['type'] in ('userMessage','agentMessage','reasoning','dynamicToolCall') for t in history['turns'] for i in t.get('items',[])), 'Integrator implemented worker work itself'
        dispatch=[c for c in calls if c.get('tool')=='interaction_controller' and c.get('arguments',{}).get('operation')=='dispatch']
        assert len(dispatch)==1 and dispatch[0]['arguments']['arguments']['input_update']==original
        assert selected_topics(dispatch[0]['arguments']['arguments']['destinations'])=={7,8}
        rows=j.row('SELECT i.doc,i.phase FROM interaction_instructions i JOIN interaction_sources s ON s.id=i.input_id WHERE s.update_id=?',(original,))
        assert len(rows)==2 and all(phase=='accepted' for _,phase in rows)
        source=json.loads(j.row('SELECT doc FROM interaction_sources WHERE update_id=?',(original,))[0][0])
        assert all(json.loads(doc)['text']==source['text'] for doc,_ in rows)
        alpha=json.loads((args.state/'semantic-alpha.json').read_text())
        beta=json.loads((args.state/'semantic-beta.json').read_text())
        assert alpha['rate']==19 and alpha['amounts']==[76,133,38] and alpha['total']==247
        assert alpha['provenance']=={'human':100,'via':'interaction-controller'}, alpha
        assert beta=={'red':14,'blue':22,'total':36}, beta
        assert j.row('SELECT thread_id,codex_thread_id,busy FROM sessions WHERE thread_id IN (1,7,8,9) ORDER BY thread_id')==previous
        report.update(success=True,total_seconds=round(time.monotonic()-started,3),entry_native=entry,
                      actual_tool_calls=[{'tool':c['tool'],'arguments':c.get('arguments'),'success':c.get('success')} for c in calls],
                      alpha=alpha,beta=beta,original_text_preserved=True,previous_bindings_preserved=True)
    except Exception as error:
        report['failure']=repr(error)
        raise
    finally:
        try:
            if j.process and j.process.poll() is None:
                for topic in (10,7,8):
                    if j.row('SELECT busy FROM sessions WHERE thread_id=?',(topic,))==[(1,)]:
                        j.admit('/stop',topic=topic)  # This fixture's selected turns only.
                j.wait(lambda:j.row('SELECT count(*) FROM sessions WHERE busy=1')==[(0,)],45)
            entry_rows=j.row('SELECT codex_thread_id FROM sessions WHERE thread_id=10')
            if entry_rows:entry=entry_rows[0][0]
            j.close()
        except Exception as error:report.update(success=False,cleanup='unconfirmed: '+str(error))
        reader=Native(args.codex)
        try:
            statuses={}
            for topic,native in [(t,n) for t,n,_ in previous]+([(10,entry)] if entry else []):
                node=reader.rpc('thread/read',{'threadId':native,'includeTurns':False})['thread']
                statuses[str(topic)]=node.get('status',{}).get('type')
                if node.get('id')!=native or statuses[str(topic)] not in ('idle','notLoaded'):report['success']=False
            report['terminal_statuses']=statuses
        finally:reader.close()
        output.write_text(json.dumps(report,indent=2)+'\n')
        print(json.dumps({k:v for k,v in report.items() if k not in ('entry_native','actual_tool_calls','failure')},indent=2),flush=True)
    assert report['success'],report.get('failure','Native semantic qualification incomplete')


if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('binary')
    p.add_argument('--state',required=True,type=pathlib.Path)
    p.add_argument('--codex',required=True)
    p.add_argument('--native-home',required=True,type=pathlib.Path)
    p.add_argument('--run-native',action='store_true')
    p.add_argument('--inspect-completed',action='store_true',help='Read-only verification after the original selector-spelling assertion; never reruns completed work')
    args=p.parse_args()
    args.resume=False
    if not args.run_native:p.error('--run-native is required')
    if args.native_home.resolve()!=pathlib.Path(os.environ.get('CODEX_HOME',pathlib.Path.home()/'.codex')).resolve():p.error('Select only this process existing native home')
    if args.inspect_completed:inspect_completed(args)
    else:qualify(args)
