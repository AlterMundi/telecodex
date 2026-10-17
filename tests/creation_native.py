#!/usr/bin/env python3
"""Opt-in creation through the retained stable native integrating conversation.
Uses synthetic Telegram and one fresh worker; no receiving human deployment.
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
    output=args.state/'qualification-creation.json'
    assert not output.exists(), 'Inspect the retained attempt; never replay it'
    assert json.loads((args.state/'qualification-semantic-inspected.json').read_text())['success'] is True
    with sqlite3.connect(args.state/'state.sqlite') as db:
        previous=db.execute('SELECT thread_id,codex_thread_id,busy FROM sessions ORDER BY thread_id').fetchall()
    assert {r[0] for r in previous}=={1,7,8,9,10} and all(r[2]==0 for r in previous)
    assert not (args.state/'creation-gamma.json').exists()
    sys.path.insert(0,str(pathlib.Path(__file__).resolve().parents[1]))
    from scripts.activity_indicator import Native
    reader=Native(args.codex)
    try:
        for _,native,_ in previous:
            node=reader.rpc('thread/read',{'threadId':native,'includeTurns':False})['thread']
            assert node['id']==native and node['status']['type'] in ('idle','notLoaded')
    finally:reader.close()
    entry=next(n for t,n,_ in previous if t==10)
    report={'success':False,'binary_sha256':hashlib.sha256(pathlib.Path(args.binary).read_bytes()).hexdigest(),
            'scope':'retained stable integrating entry and one newly created native worker',
            'telegram_boundary':'synthetic','currency_cost':'unknown','topic_requests':0}
    output.write_text(json.dumps(report,indent=2)+'\n')
    j=NativeJourney(args,reuse=True)
    def created_topic(payload):
        assert payload['chat_id']==100 and payload['name']=='Gamma'
        report['topic_requests']+=1
        output.write_text(json.dumps(report,indent=2)+'\n')
        return {'message_thread_id':77,'name':'Gamma'}
    j.created_topic=created_topic
    try:
        j.start()
        started=time.monotonic()
        original=j.admit('''Start a clearly distinct new work topic called Gamma. Alpha and Beta remain their existing tasks; this is a separate cube-sum task. No network, goals or subagents.
The Gamma worker should compute cubes of [1,2,3,4] and write the internal test file creation-gamma.json with exactly inputs, cubes, total and provenance. Provenance must contain human (the original human author ID) and via (the relay identifier), obtained from the controller packet. This internal fixture is not a Telegram deliverable. Report CREATED-GAMMA when the file reflects the result, retaining the controller coverage contract.
Integrating conversation: inspect inventory and current controller capabilities, then use create on this retained original input with name Gamma, a distinct-task reason and context_selectors []. Do not dispatch this task to Alpha or Beta, perform the cube calculation yourself or rewrite human text. After inspecting the creation outcome, reply CREATION-ADMITTED without claiming the worker completed until its own result arrives.''',topic=10)
        report['original_update']=original
        output.write_text(json.dumps(report,indent=2)+'\n')
        j.wait(lambda:j.has('CREATION-ADMITTED',10),180)
        j.wait(lambda:j.has('CREATED-GAMMA',10),240)
        j.idle()
        assert report['topic_requests']==1
        assert j.row('SELECT thread_id,codex_thread_id,busy FROM sessions WHERE thread_id IN (1,7,8,9,10) ORDER BY thread_id')==previous
        assert j.row('SELECT codex_thread_id FROM sessions WHERE thread_id=10')==[(entry,)]
        assert j.row('SELECT phase FROM interaction_creations WHERE input_update=?',(original,))==[('ready',)]
        assert j.row('SELECT count(*) FROM interaction_enrollments WHERE target_topic=77')==[(1,)]
        rows=j.row('SELECT i.id,i.doc,i.phase FROM interaction_instructions i JOIN interaction_sources s ON s.id=i.input_id WHERE s.update_id=?',(original,))
        assert len(rows)==1 and rows[0][2]=='accepted'
        instruction=json.loads(rows[0][1]);assert instruction['target']['thread_id']==77
        source=json.loads(j.row('SELECT doc FROM interaction_sources WHERE update_id=?',(original,))[0][0])
        assert instruction['text']==source['text']
        j.wait(lambda:j.row('SELECT d.phase FROM interaction_deliveries d JOIN interaction_request_links l ON l.request_id=d.request_id WHERE l.instruction_id=?',(rows[0][0],))==[('delivered',)],30)
        deliveries=j.row('SELECT d.phase,d.doc FROM interaction_deliveries d JOIN interaction_request_links l ON l.request_id=d.request_id WHERE l.instruction_id=?',(rows[0][0],))
        assert len(deliveries)==1 and deliveries[0][0]=='delivered'
        delivery=json.loads(deliveries[0][1]);assert delivery['origin']==source['origin'] and delivery['completed'] and delivery['covered']==[rows[0][0]]
        result=json.loads((args.state/'creation-gamma.json').read_text())
        assert result=={'inputs':[1,2,3,4],'cubes':[1,8,27,64],'total':100,'provenance':{'human':100,'via':'interaction-controller'}},result
        reader=Native(args.codex)
        try:history=reader.rpc('thread/read',{'threadId':entry,'includeTurns':True})['thread']
        finally:reader.close()
        turns=[t for t in history['turns'] if any('CREATION-ADMITTED' in i.get('text','') for i in t.get('items',[]) if i.get('type')=='agentMessage')]
        assert len(turns)==1 and turns[0]['status']=='completed'
        assert all(i['type'] in ('userMessage','agentMessage','reasoning','dynamicToolCall') for i in turns[0]['items'])
        calls=[i for i in turns[0]['items'] if i.get('type')=='dynamicToolCall']
        assert calls and all(c.get('success') is True for c in calls)
        creates=[c for c in calls if c.get('tool')=='interaction_controller' and c.get('arguments',{}).get('operation')=='create']
        assert len(creates)==1 and creates[0]['arguments']['arguments']['input_update']==original
        report.update(success=True,total_seconds=round(time.monotonic()-started,3),stable_entry_preserved=True,original_text_preserved=True,
                      result=result,tool_calls=[{'tool':c['tool'],'arguments':c.get('arguments'),'success':c.get('success')} for c in calls])
    except Exception as error:
        report['failure']=repr(error)
        raise
    finally:
        try:
            if j.process and j.process.poll() is None:
                for topic in (10,77):
                    if j.row('SELECT busy FROM sessions WHERE thread_id=?',(topic,))==[(1,)]:j.admit('/stop',topic=topic)
                j.wait(lambda:j.row('SELECT count(*) FROM sessions WHERE busy=1')==[(0,)],45)
            selected=j.row('SELECT thread_id,codex_thread_id FROM sessions ORDER BY thread_id')
            j.close()
        except Exception as error:report.update(success=False,cleanup='unconfirmed: '+repr(error));selected=[]
        reader=Native(args.codex)
        try:
            statuses={}
            for topic,native in selected:
                if not native:report['success']=False;continue
                node=reader.rpc('thread/read',{'threadId':native,'includeTurns':False})['thread']
                statuses[str(topic)]=node.get('status',{}).get('type')
                if node.get('id')!=native or statuses[str(topic)] not in ('idle','notLoaded'):report['success']=False
            report['terminal_statuses']=statuses
        finally:reader.close()
        output.write_text(json.dumps(report,indent=2)+'\n')
        print(json.dumps({k:v for k,v in report.items() if k not in ('tool_calls','failure')},indent=2),flush=True)
    assert report['success'],report.get('failure','Creation qualification incomplete')


def continue_retained(args):
    """Continue the existing Gamma context after the test's premature close.
    This never recreates a topic, replays an uncertain result or repeats a task.
    """
    prior=json.loads((args.state/'qualification-creation.json').read_text())
    output=args.state/'qualification-creation-continuation.json'
    assert not output.exists()
    assert prior['success'] is False and prior.get('failure')=='AssertionError()' and prior['topic_requests']==1
    assert prior['binary_sha256']==hashlib.sha256(pathlib.Path(args.binary).read_bytes()).hexdigest()
    before=(args.state/'creation-gamma.json').read_bytes()
    result=json.loads(before)
    assert result=={'inputs':[1,2,3,4],'cubes':[1,8,27,64],'total':100,'provenance':{'human':100,'via':'interaction-controller'}}
    report={'success':False,'scope':'continue the retained created native worker; no task, topic or uncertain-result replay','telegram_boundary':'synthetic','currency_cost':'unknown','initial_return_phase':'uncertain','result':result}
    output.write_text(json.dumps(report,indent=2)+'\n')
    j=NativeJourney(args,reuse=True)
    try:
        before_bindings=j.row('SELECT thread_id,codex_thread_id,busy FROM sessions ORDER BY thread_id')
        assert {t for t,_,_ in before_bindings}=={1,7,8,9,10,77} and all(b==0 for _,_,b in before_bindings)
        assert j.row('SELECT phase FROM interaction_creations WHERE input_update=?',(prior['original_update'],))==[('ready',)]
        j.start()
        started=time.monotonic()
        update=j.admit('#Gamma Read the existing creation-gamma.json without modifying it. Confirm its retained total and original human author/relay provenance. Report GAMMA-CONTINUED and follow the coverage contract. Continue this existing context; do not repeat prior effects, create topics or activate goals/subagents.',topic=10)
        report['continuation_update']=update
        output.write_text(json.dumps(report,indent=2)+'\n')
        j.wait(lambda:j.has('GAMMA-CONTINUED',10),180)
        query='SELECT d.phase,d.doc FROM interaction_deliveries d JOIN interaction_request_links l ON l.request_id=d.request_id JOIN interaction_instructions i ON i.id=l.instruction_id JOIN interaction_sources s ON s.id=i.input_id WHERE s.update_id=?'
        j.wait(lambda:[p for p,_ in j.row(query,(update,))]==['delivered'],30)
        j.idle()
        assert j.row('SELECT thread_id,codex_thread_id,busy FROM sessions ORDER BY thread_id')==before_bindings
        assert j.row('SELECT count(*) FROM interaction_creations')==[(1,)]
        assert (args.state/'creation-gamma.json').read_bytes()==before
        original_query='SELECT d.phase FROM interaction_deliveries d JOIN interaction_request_links l ON l.request_id=d.request_id JOIN interaction_instructions i ON i.id=l.instruction_id JOIN interaction_sources s ON s.id=i.input_id WHERE s.update_id=?'
        assert j.row(original_query,(prior['original_update'],))==[('uncertain',)]
        delivery=json.loads(j.row(query,(update,))[0][1])
        assert delivery['origin']['thread_id']==10 and delivery['origin']['message_id']==update and delivery['covered'] and delivery['declared']
        report.update(success=True,seconds=round(time.monotonic()-started,3),same_bindings=True,original_file_unchanged=True,continuation_return_phase='delivered',answer=delivery['body'])
    except Exception as error:report['failure']=repr(error);raise
    finally:
        try:
            if j.process and j.process.poll() is None and j.row('SELECT busy FROM sessions WHERE thread_id=77')==[(1,)]:
                j.admit('/stop',topic=77);j.wait(lambda:j.row('SELECT busy FROM sessions WHERE thread_id=77')==[(0,)],30)
            j.close()
        except Exception as error:report.update(success=False,cleanup=repr(error))
        output.write_text(json.dumps(report,indent=2)+'\n')
        print(json.dumps(report,indent=2),flush=True)
    assert report['success']


if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('binary');p.add_argument('--state',required=True,type=pathlib.Path)
    p.add_argument('--codex',required=True);p.add_argument('--native-home',required=True,type=pathlib.Path)
    p.add_argument('--run-native',action='store_true');p.add_argument('--continue-retained',action='store_true');args=p.parse_args();args.resume=False
    if not args.run_native:p.error('--run-native is required')
    if args.native_home.resolve()!=pathlib.Path(os.environ.get('CODEX_HOME',pathlib.Path.home()/'.codex')).resolve():p.error('Select only this process existing native home')
    if args.continue_retained:continue_retained(args)
    else:qualify(args)
