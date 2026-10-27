#!/usr/bin/env python3
"""Opt-in small real-model dependency journey in the retained synthetic fixture.
Explicit inexpensive model/effort selection; never rerun an existing attempt.
"""
import argparse
import hashlib
import json
import os
import pathlib
import re
import sqlite3
import subprocess
import sys
import time
from routing_native import NativeJourney


def qualify(args):
    output=args.state/'qualification-session-requests.json'
    assert not output.exists(), 'Inspect/continue the retained operation; never replay it'
    assert json.loads((args.state/'qualification-creation-continuation.json').read_text())['success']
    assert not (args.state/'session-helper-gamma.json').exists()
    assert not (args.state/'session-source-delta.json').exists()
    catalog=json.loads(subprocess.check_output([args.codex,'debug','models'],text=True))
    models=catalog.get('models',catalog.get('data',[]))
    selected=next((m for m in models if m.get('slug',m.get('model',m.get('id')))==args.model),None)
    assert selected is not None, 'Selected inexpensive model unavailable; no implicit fallback'
    efforts=selected.get('supported_reasoning_levels',selected.get('supportedReasoningEfforts',[]))
    assert args.effort in {r.get('effort',r.get('reasoningEffort')) for r in efforts}
    before_file=(args.state/'creation-gamma.json').read_bytes()
    with sqlite3.connect(args.state/'state.sqlite') as db:
        before=db.execute('SELECT thread_id,codex_thread_id,busy FROM sessions ORDER BY thread_id').fetchall()
        assert {r[0] for r in before}=={1,7,8,9,10,77} and all(r[2]==0 for r in before)
        previous_settings=db.execute('SELECT thread_id,model,reasoning_effort FROM sessions WHERE thread_id IN (10,77) ORDER BY thread_id').fetchall()
        old_uncertain=db.execute("SELECT id,phase FROM interaction_deliveries WHERE phase='uncertain' ORDER BY id").fetchall()
    sys.path.insert(0,str(pathlib.Path(__file__).resolve().parents[1]))
    from scripts.activity_indicator import Native

    def statuses():
        reader=Native(args.codex)
        try:
            with sqlite3.connect(args.state/'state.sqlite') as db:
                bindings=db.execute('SELECT thread_id,codex_thread_id FROM sessions WHERE thread_id IN (10,77,88) ORDER BY thread_id').fetchall()
            result={}
            for topic,native in bindings:
                node=reader.rpc('thread/read',{'threadId':native,'includeTurns':False})['thread']
                assert node['id']==native
                result[str(topic)]=node.get('status',{}).get('type')
            return result
        finally:reader.close()

    assert all(v in ('idle','notLoaded') for v in statuses().values())
    report={'success':False,'model':args.model,'reasoning_effort':args.effort,'worker_capacity':1,
            'binary_sha256':hashlib.sha256(pathlib.Path(args.binary).read_bytes()).hexdigest(),
            'scope':'one fresh native caller and existing retained helper; synthetic Telegram only',
            'currency_cost':'unknown','previous_settings':previous_settings,'topic_requests':0}
    def save():output.write_text(json.dumps(report,indent=2)+'\n')
    save()
    j=NativeJourney(args,reuse=True)
    # Set both new-worker defaults and only the two selected retained test rows.
    # Native bindings and previous results/history stay unchanged.
    config=re.sub(r'^default_(model|reasoning_effort)=.*\n','',j.config.read_text(),flags=re.M)
    config=config.replace('[codex]\n','[codex]\ndefault_model='+json.dumps(args.model)+'\ndefault_reasoning_effort='+json.dumps(args.effort)+'\n')
    config=re.sub(r'^max_active_workers=.*\n','',config,flags=re.M).replace('[interaction]\n','[interaction]\nmax_active_workers=1\n')
    j.config.write_text(config)
    with sqlite3.connect(args.state/'state.sqlite') as db:
        changed=db.execute('UPDATE sessions SET model=?,reasoning_effort=? WHERE creator_user_id=100 AND busy=0 AND thread_id IN (10,77)',(args.model,args.effort)).rowcount
        assert changed==2
    def created(payload):
        assert payload['chat_id']==100 and payload['name']=='Delta'
        report['topic_requests']+=1;save()
        return {'message_thread_id':88,'name':'Delta'}
    j.created_topic=created

    def await_effect(predicate,label):
        while True:
            try:j.wait(predicate,seconds=30);return
            except AssertionError as error:
                if 'boundary did not complete' not in str(error):raise
                state=statuses();report['pending']={'effect':label,'native_status':state};save()
                print(json.dumps({'pending':label,'native_status':state}),flush=True)
                # Observation expiry is not cancellation. Inspect actual retained
                # admission and native outcomes before deciding work is terminal.
                busy=j.row('SELECT count(*) FROM sessions WHERE thread_id IN (10,77,88) AND busy=1')[0][0]
                pending=j.row("SELECT count(*) FROM interaction_instructions WHERE phase IN ('queued','submitting','steering')")[0][0]
                publications=j.row("SELECT count(*) FROM interaction_deliveries WHERE phase IN ('preparing','sending','reserved')")[0][0]
                original=j.row('SELECT status FROM incoming_updates WHERE update_id=?',(report['original_update'],))
                if not busy and not pending and not publications and original and original[0][0] in ('settled','handled','undetermined') and all(v in ('idle','notLoaded') for v in state.values()):
                    raise AssertionError('Selected native work is terminal without required effect: '+label) from None
    try:
        j.start();started=time.monotonic()
        original=j.admit('''Create one clearly distinct work topic named Delta for this small local session-dependency test. No network, goals, subagents or unrelated files. Alpha and Beta remain untouched.
Integrator: inspect inventory and controller capabilities, then create Delta from this exact original input with context_selectors []. Do not perform the worker task or send it to another topic yourself. Report DELTA-CREATION-ADMITTED once creation is admitted.
Delta worker task: ask the existing Gamma session to compute squares of [2,4,6] and their sum. Use interaction_controller capabilities and inputs, then session_request on the accepted original human parent, request_id gamma-square-dependency-v1, selector /self#Gamma. The selected session-authored question must ask Gamma to write the internal fixture file session-helper-gamma.json with inputs, squares, total and provenance. Provenance must contain kind (transport author kind), original_human_author (null for this session-authored request), authorized_human, local_request and source_topic from the actual transport provenance author.session.thread_id. Gamma should reply GAMMA-DEPENDENCY-COMPLETED and declare completed request coverage.
Delta: after session_request admission, finish this first turn with DELTA-DEFERRED, declaring the parent request deferred rather than completed. Do not wait, poll, calculate the sum yourself or write your final file yet. There is only one worker slot; the helper runs after you finish. When its later session_result context arrives, write session-source-delta.json with total from that result and provenance containing kind, original_human_author (null), authorized_human, local_request and helper_topic from the returned transport author.session.thread_id. These are internal fixture files, not Telegram deliverables. Then reply DELTA-COMPLETED with normal controller coverage, marking the original request completed. Do not repeat the helper request or change its id.''',topic=10)
        report['original_update']=original;save()
        await_effect(lambda:j.row("SELECT name FROM sqlite_master WHERE name='interaction_session_requests'") and j.row('SELECT count(*) FROM interaction_session_requests')[0][0]==1,'one native local dependency')
        await_effect(lambda:(args.state/'session-helper-gamma.json').exists() and (args.state/'session-source-delta.json').exists(),'helper and resumed caller artifacts')
        def completed_return():
            rows=j.row('SELECT d.phase,d.doc FROM interaction_deliveries d JOIN interaction_requests r ON r.id=d.request_id JOIN interaction_sources s ON s.id=r.input_id WHERE s.update_id=?',(original,))
            return any(phase=='delivered' and json.loads(raw)['completed'] and 'DELTA-COMPLETED' in json.loads(raw)['body'] for phase,raw in rows)
        await_effect(completed_return,'committed completed caller return')
        await_effect(lambda:j.row("SELECT count(*) FROM interaction_deliveries d JOIN interaction_session_requests c ON c.target_instruction=d.request_id WHERE c.phase='returned' AND NOT EXISTS(SELECT 1 FROM interaction_public_deliveries p WHERE p.id=d.id)")[0][0]==1,'retained internal helper return')
        j.idle()
        assert report['topic_requests']==1
        assert j.row('SELECT thread_id,codex_thread_id,busy FROM sessions WHERE thread_id!=88 ORDER BY thread_id')==before
        assert j.row("SELECT id,phase FROM interaction_deliveries WHERE phase='uncertain' ORDER BY id")==old_uncertain
        assert (args.state/'creation-gamma.json').read_bytes()==before_file
        settings=j.row('SELECT thread_id,model,reasoning_effort FROM sessions WHERE thread_id IN (10,77,88) ORDER BY thread_id')
        assert len(settings)==3 and all(model==args.model and effort==args.effort for _,model,effort in settings)
        calls=j.row('SELECT doc,phase FROM interaction_session_requests');assert len(calls)==1 and calls[0][1]=='returned'
        call=json.loads(calls[0][0]);assert call['caller']['thread_id']==88 and call['return_instruction']
        helper=json.loads((args.state/'session-helper-gamma.json').read_text());source=json.loads((args.state/'session-source-delta.json').read_text())
        assert helper=={'inputs':[2,4,6],'squares':[4,16,36],'total':56,'provenance':{'kind':'session_request','original_human_author':None,'authorized_human':100,'local_request':call['id'],'source_topic':88}},helper
        assert source=={'total':56,'provenance':{'kind':'session_result','original_human_author':None,'authorized_human':100,'local_request':call['id'],'helper_topic':77}},source
        report.update(success=True,seconds=round(time.monotonic()-started,3),caller_task=source,helper_task=helper,
                      exact_selected_settings=settings,retained_bindings_preserved=True,original_uncertain_return_preserved=True)
    except Exception as error:
        report['failure']=repr(error);raise
    finally:
        # Only this finite synthetic fixture's selected test work may be cancelled.
        # No global Stop or human receiving listener is selected.
        try:
            if j.process and j.process.poll() is None:
                for topic in (88,77,10):
                    if j.row('SELECT busy FROM sessions WHERE thread_id=?',(topic,))==[(1,)]:j.admit('/stop',topic=topic)
                j.wait(lambda:j.row('SELECT count(*) FROM sessions WHERE thread_id IN (10,77,88) AND busy=1')==[(0,)],seconds=60)
            j.close()
            report['terminal_statuses']=statuses()
            if any(v not in ('idle','notLoaded') for v in report['terminal_statuses'].values()):report['success']=False
        except Exception as error:report.update(success=False,cleanup=repr(error))
        save();print(json.dumps({k:v for k,v in report.items() if k!='failure'},indent=2),flush=True)
    assert report['success'],report.get('failure','Native dependency qualification incomplete')


def continue_retained(args):
    """Clarify the actual retained worker role; zero helper work was admitted."""
    prior=json.loads((args.state/'qualification-session-requests.json').read_text())
    output=args.state/'qualification-session-requests-continuation.json'
    assert not output.exists(), 'Inspect existing continuation; no replay'
    assert prior['success'] is False and prior['topic_requests']==1
    assert prior['failure']=="AssertionError('Selected native work is terminal without required effect: one native local dependency')"
    assert prior['model']==args.model and prior['reasoning_effort']==args.effort
    before_file=(args.state/'creation-gamma.json').read_bytes()
    with sqlite3.connect(args.state/'state.sqlite') as db:
        before=db.execute('SELECT thread_id,codex_thread_id,busy FROM sessions ORDER BY thread_id').fetchall()
        assert {r[0] for r in before}=={1,7,8,9,10,77,88} and all(r[2]==0 for r in before)
        assert db.execute('SELECT count(*) FROM interaction_session_requests').fetchone()==(0,)
        settings=db.execute('SELECT thread_id,model,reasoning_effort FROM sessions WHERE thread_id IN (10,77,88) ORDER BY thread_id').fetchall()
        assert all(model==args.model and effort==args.effort for _,model,effort in settings)
        reply=db.execute("SELECT d.message_id FROM interaction_deliveries d JOIN interaction_requests r ON r.id=d.request_id JOIN sessions s ON s.id=r.session_id WHERE s.thread_id=88 AND d.phase='delivered' ORDER BY d.rowid DESC LIMIT 1").fetchone()[0]
        uncertain=db.execute("SELECT id,phase FROM interaction_deliveries WHERE phase='uncertain' ORDER BY id").fetchall()
    assert not (args.state/'session-helper-gamma.json').exists() and not (args.state/'session-source-delta.json').exists()
    sys.path.insert(0,str(pathlib.Path(__file__).resolve().parents[1]))
    from scripts.activity_indicator import Native
    def statuses():
        reader=Native(args.codex)
        try:
            result={}
            for topic,native,_ in before:
                if topic not in (10,77,88):continue
                node=reader.rpc('thread/read',{'threadId':native,'includeTurns':False})['thread']
                assert node['id']==native;result[str(topic)]=node.get('status',{}).get('type')
            return result
        finally:reader.close()
    assert all(s in ('idle','notLoaded') for s in statuses().values())
    report={'success':False,'scope':'clarify retained Delta worker role, no new topic or previously executed helper task',
            'model':args.model,'reasoning_effort':args.effort,'worker_capacity':1,'currency_cost':'unknown',
            'original_update':prior['original_update'],'binary_sha256':hashlib.sha256(pathlib.Path(args.binary).read_bytes()).hexdigest()}
    def save():output.write_text(json.dumps(report,indent=2)+'\n')
    save();j=NativeJourney(args,reuse=True)
    def await_effect(predicate,label):
        while True:
            try:j.wait(predicate,seconds=30);return
            except AssertionError as error:
                if 'boundary did not complete' not in str(error):raise
                state=statuses();report['pending']={'effect':label,'native_status':state};save();print(json.dumps(report['pending']),flush=True)
                busy=j.row('SELECT count(*) FROM sessions WHERE thread_id IN (10,77,88) AND busy=1')[0][0]
                pending=j.row("SELECT count(*) FROM interaction_instructions WHERE phase IN ('queued','submitting','steering')")[0][0]
                pubs=j.row("SELECT count(*) FROM interaction_deliveries WHERE phase IN ('preparing','sending','reserved')")[0][0]
                original=j.row('SELECT status FROM incoming_updates WHERE update_id=?',(report['steering_update'],))
                if not busy and not pending and not pubs and original and original[0][0] in ('settled','handled','undetermined') and all(v in ('idle','notLoaded') for v in state.values()):raise AssertionError('Retained native work is terminal without effect: '+label) from None
    try:
        j.start();started=time.monotonic()
        update=j.admit("""#Delta Role clarification for the same original request: you are the Delta worker in existing topic t88. The integrating entrance already created your topic and its clauses in the original message were for that entrance only. Do not recreate any topic or perform an integrator operation.
Continue only your original Delta worker task: use capabilities and inputs, then session_request with the latest accepted human clarification instruction as parent, request_id gamma-square-dependency-v1, selector /self#Gamma. Ask Gamma to compute squares of [2,4,6] and their sum and write session-helper-gamma.json as specified in the original task, retaining actual session_request provenance (kind, original_human_author:null, authorized_human, local_request, source_topic). No helper request has yet been admitted, and no helper/file work has been performed; do not rerun completed work or change the stable request id to hide uncertainty.
After admission, finish this turn with DELTA-DEFERRED and no completed original request. With one worker slot Gamma executes after your turn ends. Do not wait or poll. When its later session_result context arrives, write session-source-delta.json with total and actual returned provenance (kind, original_human_author:null, authorized_human, local_request, helper_topic), then report DELTA-COMPLETED and completed original request coverage. Internal fixture files only. No network, goals, subagents, unrelated files or uploads.""",topic=10,reply=reply)
        report['steering_update']=update;save()
        await_effect(lambda:j.row('SELECT count(*) FROM interaction_session_requests')[0][0]==1,'one retained-worker local dependency')
        await_effect(lambda:(args.state/'session-helper-gamma.json').exists() and (args.state/'session-source-delta.json').exists(),'helper and resumed caller artifacts')
        def completed_return():
            rows=j.row('SELECT d.phase,d.doc FROM interaction_deliveries d JOIN interaction_requests r ON r.id=d.request_id JOIN interaction_sources s ON s.id=r.input_id WHERE s.update_id=?',(prior['original_update'],))
            return any(phase=='delivered' and json.loads(raw)['completed'] and 'DELTA-COMPLETED' in json.loads(raw)['body'] for phase,raw in rows)
        await_effect(completed_return,'committed completed caller return')
        await_effect(lambda:j.row("SELECT count(*) FROM interaction_deliveries d JOIN interaction_session_requests c ON c.target_instruction=d.request_id WHERE c.phase='returned' AND NOT EXISTS(SELECT 1 FROM interaction_public_deliveries p WHERE p.id=d.id)")[0][0]==1,'retained internal helper return')
        j.idle()
        assert j.row('SELECT thread_id,codex_thread_id,busy FROM sessions ORDER BY thread_id')==before
        assert j.row("SELECT id,phase FROM interaction_deliveries WHERE phase='uncertain' ORDER BY id")==uncertain
        assert (args.state/'creation-gamma.json').read_bytes()==before_file
        raw,phase=j.row('SELECT doc,phase FROM interaction_session_requests')[0];call=json.loads(raw)
        assert phase=='returned' and call['caller']['thread_id']==88 and call['return_instruction']
        helper=json.loads((args.state/'session-helper-gamma.json').read_text());source=json.loads((args.state/'session-source-delta.json').read_text())
        assert helper=={'inputs':[2,4,6],'squares':[4,16,36],'total':56,'provenance':{'kind':'session_request','original_human_author':None,'authorized_human':100,'local_request':call['id'],'source_topic':88}},helper
        assert source=={'total':56,'provenance':{'kind':'session_result','original_human_author':None,'authorized_human':100,'local_request':call['id'],'helper_topic':77}},source
        report.update(success=True,seconds=round(time.monotonic()-started,3),helper_task=helper,caller_task=source,same_bindings=True,no_topic_or_completed_task_replay=True)
    except Exception as error:report['failure']=repr(error);raise
    finally:
        try:
            if j.process and j.process.poll() is None:
                for topic in (88,77,10):
                    if j.row('SELECT busy FROM sessions WHERE thread_id=?',(topic,))==[(1,)]:j.admit('/stop',topic=topic)
                j.wait(lambda:j.row('SELECT count(*) FROM sessions WHERE thread_id IN (10,77,88) AND busy=1')==[(0,)],60)
            j.close();report['terminal_statuses']=statuses()
            if any(v not in ('idle','notLoaded') for v in report['terminal_statuses'].values()):report['success']=False
        except Exception as error:report.update(success=False,cleanup=repr(error))
        save();print(json.dumps({k:v for k,v in report.items() if k!='failure'},indent=2),flush=True)
    assert report['success'],report.get('failure','Retained native dependency qualification incomplete')


def verify_retained(args):
    """Read completed artifacts and qualify a new cumulative coverage declaration."""
    output=args.state/'qualification-session-requests-verified.json'
    assert not output.exists(), 'Inspect the existing verification; never replay it'
    prior=json.loads((args.state/'qualification-session-requests-continuation.json').read_text())
    assert prior['success'] is False and prior.get('observation_outcome')
    assert prior['model']==args.model and prior['reasoning_effort']==args.effort
    files={name:(args.state/name).read_bytes() for name in ('creation-gamma.json','session-helper-gamma.json','session-source-delta.json')}
    with sqlite3.connect(args.state/'state.sqlite') as db:
        bindings=db.execute('SELECT thread_id,codex_thread_id,busy FROM sessions ORDER BY thread_id').fetchall();assert all(not r[2] for r in bindings)
        calls=db.execute('SELECT doc,phase FROM interaction_session_requests').fetchall();assert len(calls)==1 and calls[0][1]=='returned'
        call=json.loads(calls[0][0])
        source=json.loads(files['session-source-delta.json']);helper=json.loads(files['session-helper-gamma.json'])
        assert source=={'total':56,'provenance':{'kind':'session_result','original_human_author':None,'authorized_human':100,'local_request':call['id'],'helper_topic':77}}
        assert helper=={'inputs':[2,4,6],'squares':[4,16,36],'total':56,'provenance':{'kind':'session_request','original_human_author':None,'authorized_human':100,'local_request':call['id'],'source_topic':88}}
        reply=db.execute("SELECT d.message_id FROM interaction_deliveries d JOIN interaction_requests r ON r.id=d.request_id JOIN sessions s ON s.id=r.session_id WHERE s.thread_id=88 AND d.phase='delivered' ORDER BY d.rowid DESC LIMIT 1").fetchone()[0]
        protected=db.execute("SELECT id,phase FROM interaction_deliveries WHERE phase IN ('uncertain','withheld') ORDER BY id").fetchall()
        highest=db.execute('SELECT coalesce(max(message_id),0) FROM interaction_deliveries').fetchone()[0]
        settings=db.execute('SELECT model,reasoning_effort FROM sessions WHERE thread_id IN (10,77,88)').fetchall();assert all((m,e)==(args.model,args.effort) for m,e in settings)
    sys.path.insert(0,str(pathlib.Path(__file__).resolve().parents[1]))
    from scripts.activity_indicator import Native
    def statuses():
        reader=Native(args.codex)
        try:
            result={}
            for topic,native,_ in bindings:
                if topic not in (10,77,88):continue
                node=reader.rpc('thread/read',{'threadId':native,'includeTurns':False})['thread'];assert node['id']==native;result[str(topic)]=node['status']['type']
            return result
        finally:reader.close()
    assert all(v in ('idle','notLoaded') for v in statuses().values())
    report={'success':False,'scope':'read-only verification of completed retained dependency and explicit cumulative coverage; no helper/task/topic replay',
            'model':args.model,'reasoning_effort':args.effort,'currency_cost':'unknown','original_update':prior['original_update'],
            'binary_sha256':hashlib.sha256(pathlib.Path(args.binary).read_bytes()).hexdigest()}
    def save():output.write_text(json.dumps(report,indent=2)+'\n')
    save();j=NativeJourney(args,reuse=True)
    try:
        j.start();started=time.monotonic()
        update=j.admit("""#Delta Verify the already completed original dependency work, without repeating it: read existing session-helper-gamma.json and session-source-delta.json. Do not rewrite either file, ask Gamma again, create a topic, use network, start goals/subagents or do any new computation.
Your retained files already contain total56 and the attributed session_request/session_result provenance. Read and confirm the inputs, squares, total and provenance from those files. The previous canonical completion was withheld because it declared coverage only for its latest context. For this new verification, actually carry forward the applicable original worker-task constraints and subsequent human clarification, incorporate the returned helper context and this read-only verification instruction, and explicitly list ALL those actual instruction IDs in covered, not just the latest one. Obtain them from the current controller packet's original/recent input context; never invent IDs or claim a missing constraint. Address and complete the same original request. Reply DELTA-VERIFIED-COMPLETED, summarize the existing total/provenance and append the normal cumulative controller coverage declaration. These files remain internal fixture evidence; do not upload or claim file delivery.""",topic=10,reply=reply)
        report['verification_update']=update;save()
        def completed():
            rows=j.row('SELECT d.phase,d.doc,d.message_id FROM interaction_deliveries d JOIN interaction_requests r ON r.id=d.request_id JOIN interaction_sources s ON s.id=r.input_id WHERE s.update_id=?',(prior['original_update'],))
            return next(((json.loads(raw),message) for phase,raw,message in rows if phase=='delivered' and json.loads(raw)['completed'] and json.loads(raw)['current'] and 'DELTA-VERIFIED-COMPLETED' in json.loads(raw)['body']),None)
        while True:
            try:j.wait(lambda:completed() is not None,30);break
            except AssertionError as error:
                if 'boundary did not complete' not in str(error):raise
                state=statuses();report['pending']={'native_status':state,'effect':'committed cumulative verification return'};save();print(json.dumps(report['pending']),flush=True)
                busy=j.row('SELECT count(*) FROM sessions WHERE thread_id IN (10,77,88) AND busy=1')[0][0]
                pending=j.row("SELECT count(*) FROM interaction_instructions WHERE phase IN ('queued','submitting','steering')")[0][0]
                pubs=j.row("SELECT count(*) FROM interaction_deliveries WHERE phase IN ('preparing','sending','reserved')")[0][0]
                original=j.row('SELECT status FROM incoming_updates WHERE update_id=?',(update,))
                if not busy and not pending and not pubs and original and original[0][0] in ('settled','handled','undetermined') and all(v in ('idle','notLoaded') for v in state.values()):raise AssertionError('Native verification terminal without a current completed return') from None
        j.idle();delivery,message=completed();assert message>highest
        assert j.row('SELECT thread_id,codex_thread_id,busy FROM sessions ORDER BY thread_id')==bindings
        assert j.row('SELECT doc,phase FROM interaction_session_requests')==calls
        assert j.row("SELECT id,phase FROM interaction_deliveries WHERE phase IN ('uncertain','withheld') ORDER BY id")==protected
        assert all((args.state/name).read_bytes()==data for name,data in files.items())
        required={row[0] for row in j.row('SELECT i.id FROM interaction_instructions i JOIN sessions s ON s.id=i.session_id WHERE s.thread_id=88')}
        assert required.issubset(set(delivery['covered']))
        report.update(success=True,seconds=round(time.monotonic()-started,3),covered_instructions=len(required),canonical_answer=delivery['body'],same_bindings=True,helper_requests=1,all_files_unchanged=True,protected_uncertain_and_withheld_returns_unchanged=True,new_monotonic_peer_receipt=True)
    except Exception as error:report['failure']=repr(error);raise
    finally:
        try:
            if j.process and j.process.poll() is None and j.row('SELECT busy FROM sessions WHERE thread_id=88')==[(1,)]:
                j.admit('/stop',topic=88);j.wait(lambda:j.row('SELECT busy FROM sessions WHERE thread_id=88')==[(0,)],60)
            j.close();report['terminal_statuses']=statuses()
            if any(v not in ('idle','notLoaded') for v in report['terminal_statuses'].values()):report['success']=False
        except Exception as error:report.update(success=False,cleanup=repr(error))
        save();print(json.dumps({k:v for k,v in report.items() if k!='failure'},indent=2),flush=True)
    assert report['success'],report.get('failure','Retained native verification incomplete')


if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('binary');p.add_argument('--state',required=True,type=pathlib.Path)
    p.add_argument('--codex',required=True);p.add_argument('--native-home',required=True,type=pathlib.Path)
    p.add_argument('--model',choices=['gpt-6-luna','gpt-6.1-sol'],default='gpt-6-luna')
    p.add_argument('--effort',choices=['low','medium'],default='low')
    p.add_argument('--run-native',action='store_true');p.add_argument('--continue-retained',action='store_true');p.add_argument('--verify-retained',action='store_true');args=p.parse_args();args.resume=False
    if not args.run_native:p.error('--run-native is required')
    if args.native_home.resolve()!=pathlib.Path(os.environ.get('CODEX_HOME',pathlib.Path.home()/'.codex')).resolve():p.error('Select only this process existing native home')
    if args.verify_retained:verify_retained(args)
    elif args.continue_retained:continue_retained(args)
    else:qualify(args)
