#!/usr/bin/env python3
"""Opt-in old-to-new listener handoff with one cheap, isolated native goal.

Synthetic Telegram only. Never selects existing human sessions or global Stop.
Retains failed observations and native work; an existing report is inspect-only.
"""
import argparse
import hashlib
import json
import os
import pathlib
import shutil
import subprocess
import time

from routing_native import NativeJourney


def qualify(args):
    retained=args.verify_retained
    previous_report=args.state/'qualification-handoff.json'
    if retained:
        prior=json.loads(previous_report.read_text())
        assert prior['success'] is False and prior.get('active_writer_survived_disconnect')
        assert not (args.state/'qualification-collector-verification.json').exists(), 'Inspect the retained verification; never repeat it'
        prior_artifact=(args.state/'handoff-finished').read_bytes()
        assert prior_artifact==b'DONE'
    else:
        assert not args.state.exists(), 'Inspect retained work; never replay this attempt'
    args.resume = False
    assert hashlib.sha256(pathlib.Path(args.candidate).read_bytes()).hexdigest()==args.candidate_sha256, 'Candidate changed; do not run unknown source'
    old = args.binary
    j = NativeJourney(args, reuse=retained)
    # The shared build cache may have another writer. Freeze both artifacts
    # before any native call; never qualify a changing executable path.
    old_snapshot = args.state/'previous-listener'
    candidate_snapshot = args.state/'candidate-listener'
    if not retained:
        shutil.copy2(old, old_snapshot)
        shutil.copy2(args.candidate, candidate_snapshot)
    else:
        assert hashlib.sha256(old_snapshot.read_bytes()).hexdigest()==prior['old_sha256']
    assert hashlib.sha256(candidate_snapshot.read_bytes()).hexdigest()==args.candidate_sha256
    j.binary = str(old_snapshot.resolve())
    if not retained:
        config = j.config.read_text().replace('[codex]\n', '[codex]\ndefault_model="gpt-6-luna"\ndefault_reasoning_effort="low"\n')
        socket = args.state/'control.sock'
        config = config.replace('[telegram]', '[session_control]\nsocket_path='+json.dumps(str(socket))+'\nhuman_id=100\n[telegram]')
        j.config.write_text(config)
    socket=args.state/'control.sock'
    report = {'success': False, 'model': 'gpt-6-luna', 'effort': 'low',
              'scope': 'one new isolated goal; old consumer disconnect, collector-only exact physical turn attachment by candidate; synthetic Telegram',
              'old_sha256': hashlib.sha256(pathlib.Path(old).read_bytes()).hexdigest(),
              'candidate_sha256': args.candidate_sha256,
              'candidate_source_reference': args.candidate_source_reference,
              'live_telegram_acceptance': False}
    output = args.state / ('qualification-collector-verification.json' if retained else 'qualification-handoff.json')
    marker='collector-verification' if retained else 'handoff'
    if retained:
        report['scope']='distinct read-only verification of retained completed artifact, same native binding; no goal resumption or replay of prior command'
        report['prior_update']=prior['original_update']
    def save():
        output.write_text(json.dumps(report, indent=2)+'\n')
    save()
    import sys
    sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
    from scripts.activity_indicator import Native
    reader = Native(args.codex)
    def native():
        result = reader.rpc('thread/read', {'threadId': report['native_thread'], 'includeTurns': True})['thread']
        assert result['id'] == report['native_thread']
        return result
    def await_effect(predicate, label, seconds=90):
        until = time.monotonic()+seconds
        while not predicate():
            assert j.process is None or j.process.poll() is None, 'Owned listener exited; inspect listener.log'
            if time.monotonic() > until:
                report['missing_observation'] = label
                save()
                raise AssertionError('Observation expired; inspect retained native work: '+label)
            time.sleep(.2)
    try:
        j.start()
        j.wait_control_socket(socket)
        if retained:
            assert reader.goal(prior['native_thread'])['status']=='complete'
            assert j.row('SELECT model,reasoning_effort FROM sessions WHERE thread_id=7')==[('gpt-6-luna','low')]
        prompt='/goal This is one finite isolated listener-handoff qualification. Use gpt-6-luna low; no subagents, network or unrelated files. Run exactly one local Python command which writes handoff-started, then waits up to 120 seconds for handoff-release, checking every 0.2 seconds. After release, write handoff-finished with text DONE. Then mark this goal complete using the native goal tool and reply HANDOFF-COMPLETED. Do not create another goal or repeat the command. All three filenames are relative to this isolated working directory.'
        if retained:
            prompt="This is a distinct read-only collector verification of the previous completed task. Keep the completed goal unchanged: do not create, update or resume any goal, do not repeat prior commands or rewrite handoff-started, handoff-release or handoff-finished. No network, subagents or unrelated files. Run one local Python command that reads handoff-finished and verifies its exact contents are DONE; write only the new collector-verification-started marker; wait at most 120 seconds for collector-verification-release (checking every 0.2 seconds). When released, write the new collector-verification-finished marker with VERIFIED, then reply exactly COLLECTOR-VERIFIED DONE. This verification and its new markers are the only new work."
        update=j.admit(prompt)
        report['original_update'] = update
        await_effect(lambda: (args.state/(marker+'-started')).exists(), 'native command started')
        report['native_thread'] = j.row('SELECT codex_thread_id FROM sessions WHERE thread_id=7')[0][0]
        before = native()
        active = [t for t in before['turns'] if t['status']=='inProgress']
        assert len(active)==1
        report['native_turn'] = active[0]['id']
        report['goal_before'] = reader.goal(report['native_thread'])
        assert report['goal_before']['status']==('complete' if retained else 'active')
        save()
        # Disconnect only this owned synthetic consumer, not the native daemon.
        j.stop()
        disconnected = native()
        assert any(t['id']==report['native_turn'] and t['status']=='inProgress' for t in disconnected['turns'])
        disconnected_goal=reader.goal(report['native_thread'])
        for key in ('objective','status','tokenBudget','createdAt'):
            assert disconnected_goal[key]==report['goal_before'][key], key
        for key in ('tokensUsed','timeUsedSeconds'):
            assert disconnected_goal[key]>=report['goal_before'][key], key
        report['active_writer_survived_disconnect'] = True
        save()
        j.binary = str(candidate_snapshot.resolve())
        j.start()
        # No primary chat is configured in this synthetic listener; startup
        # notifications are deliberately absent. Poll readiness via the actual
        # admitted control response rather than a nonexistent notification.
        checkpoint = len(j.messages)
        j.wait_control_socket(socket)
        response = subprocess.run([j.binary, '--control', str(j.config), 'attach-active', '100', '7', report['native_thread'], report['native_turn']], capture_output=True, text=True, timeout=35)
        assert response.returncode == 0, (response.stdout, response.stderr)
        report['attachment_receipt'] = json.loads(response.stdout)
        assert report['attachment_receipt']['result']['state']=='attachment_queued'
        await_effect(lambda: j.row("SELECT count(*) FROM bot_state WHERE key LIKE 'collector_attachment:%' AND json_extract(value,'$.native_turn')=?",(report['native_turn'],))==[(1,)], 'verified physical collector attachment')
        adopted = native()
        assert [t['id'] for t in adopted['turns']] == [t['id'] for t in before['turns']], 'Adoption must not start or replay a turn'
        after_goal = reader.goal(report['native_thread'])
        for key in ('objective','createdAt','tokenBudget'):
            assert after_goal[key] == report['goal_before'][key], key
        assert after_goal['tokensUsed'] >= report['goal_before']['tokensUsed'], 'Accounting must not reset'
        report['adopted_without_duplicate_turn'] = True
        save()
        (args.state/(marker+'-release')).touch()
        await_effect(lambda: (args.state/(marker+'-finished')).exists(), 'original native command completion')
        await_effect(lambda: any(('COLLECTOR-VERIFIED DONE' if retained else 'HANDOFF-COMPLETED') in m['text'] for m,_ in j.messages[checkpoint:]), 'candidate delivered original native answer')
        j.idle()
        report['goal_after'] = reader.goal(report['native_thread'])
        assert report['goal_after']['status']=='complete'
        final = native()
        assert final.get('status',{}).get('type')=='idle'
        assert any(t['id']==report['native_turn'] and t['status']=='completed' for t in final['turns'])
        # Native goals may legitimately continue in another physical turn;
        # that is distinct from replaying a human kickoff at adoption.
        report['retained_original_ingress'] = j.row('SELECT status FROM incoming_updates WHERE update_id=?',(update,))
        if retained:
            assert (args.state/'handoff-finished').read_bytes()==prior_artifact
            assert j.row('SELECT codex_thread_id FROM sessions WHERE thread_id=7')==[(prior['native_thread'],)]
            report['prior_artifact_unchanged']=True
            report['same_native_binding']=True
        report['success'] = True
    except Exception as error:
        report['failure'] = repr(error)
        raise
    finally:
        # Release the finite test command even if verification failed. Do not
        # replay, pause or interrupt a goal to make the report green.
        (args.state/(marker+'-release')).touch()
        if 'native_thread' in report:
            try:
                report['last_native_status'] = native().get('status')
                report['last_goal'] = reader.goal(report['native_thread'])
            except Exception as error:
                report['last_observation_error'] = repr(error)
        save()
        reader.close()
        j.close()
        print(json.dumps(report, indent=2), flush=True)


if __name__ == '__main__':
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('binary', help='Exact previous listener binary')
    p.add_argument('candidate', help='Exact candidate listener binary')
    p.add_argument('--candidate-sha256', required=True)
    p.add_argument('--candidate-source-reference', required=True)
    p.add_argument('--state', type=pathlib.Path, required=True)
    p.add_argument('--codex', required=True)
    p.add_argument('--native-home', required=True)
    p.add_argument('--verify-retained', action='store_true', help='Distinct read-only verification of an already completed retained fixture; never repeat the original goal or task')
    p.add_argument('--run-native', action='store_true')
    args = p.parse_args()
    if not args.run_native:
        p.error('--run-native required; one actual inexpensive native goal is created')
    if pathlib.Path(args.native_home).resolve()!=pathlib.Path(os.environ.get('CODEX_HOME',pathlib.Path.home()/'.codex')).resolve():
        p.error('Select only the current body native home')
    qualify(args)
