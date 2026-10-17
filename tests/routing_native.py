#!/usr/bin/env python3
"""Opt-in real native-model qualification with a synthetic Telegram boundary.

Uses the selected existing native daemon via its supported stdio proxy. Creates
only new qualification conversations; never selects or stops existing human work.
No real Telegram account, peer messaging, global Stop or goal activation.
State and native session history are retained; this is not live Telegram acceptance.
"""
import argparse
import hashlib
import json
import os
import pathlib
import signal
import sqlite3
import re
import subprocess
import sys
import time

from questions_io import Journey


class NativeJourney(Journey):
    def __init__(self, args):
        self.codex = str(pathlib.Path(args.codex).resolve())
        self.native_home = str(pathlib.Path(args.native_home).resolve())
        saved_config = (args.state / 'local.toml').read_text() if args.resume else None
        super().__init__(str(pathlib.Path(args.binary).resolve()), 'real-native', state=args.state, resume=args.resume)
        text = self.config.read_text()
        if saved_config is not None:
            endpoint = re.search(r'^api_base=.*$', text, re.M).group(0)
            text = re.sub(r'^api_base=.*$', lambda _: endpoint, saved_config, flags=re.M)
            self.sequence = self.row('SELECT coalesce(max(update_id),0) FROM incoming_updates')[0][0]
            self.message_id = 1000000 + self.sequence
        text = text.replace('binary=' + json.dumps(str(self.root / 'codex')), 'binary=' + json.dumps(self.codex))
        text = text.replace('default_model="fixture-model"\n', '')
        self.config.write_text(text)
        self.log = None

    def start(self):
        env = dict(os.environ, CODEX_HOME=self.native_home, RUST_LOG='warn,telecodex::codex=debug')
        self.log = (self.root / 'listener.log').open('a')
        self.process = subprocess.Popen([self.binary, str(self.config)], cwd=self.root,
                                        env=env, stdout=self.log, stderr=subprocess.STDOUT)

    def stop(self):
        if self.process:
            if self.process.poll() is None:
                self.process.send_signal(signal.SIGINT)
                self.process.wait(timeout=15)
            self.process = None
        if self.log:
            self.log.close()
            self.log = None

    def wait(self, condition, seconds=180):
        # A timeout preserves the real process for scoped cleanup; it is never a retry.
        super().wait(condition, seconds)

    def idle(self):
        self.wait(lambda: self.row('SELECT count(*) FROM sessions WHERE busy=1') == [(0,)])

    def has(self, text, topic):
        return any(text in m['text'] and m.get('message_thread_id') == topic for m,_ in self.messages)

    def instructions(self, topic):
        return self.row('SELECT i.id,i.phase,i.doc FROM interaction_instructions i JOIN sessions s ON s.id=i.session_id WHERE s.thread_id=? ORDER BY i.sequence', (topic,))

    def staged(self, label, topic, text, report, reply=None):
        started = time.monotonic()
        update = self.admit(text, topic=1, reply=reply)
        self.wait(lambda: self.row('SELECT count(*) FROM interaction_sources WHERE update_id=?', (update,)) == [(1,)], 15)
        resolved = time.monotonic()
        self.wait(lambda: any(json.loads(r[2])['origin']['message_id'] == update and r[1] == 'accepted' for r in self.instructions(topic)), 90)
        report['latency_seconds'][label] = {'source_admitted': round(resolved-started, 3),
                                           'native_acceptance_observed': round(time.monotonic()-started, 3)}
        return update

    def cleanup_workers(self):
        # Release the bounded fixture wait before asking this listener to interrupt
        # its own selected turns. Nothing here calls coordinated Stop.
        (self.root / 'release-alpha').touch()
        if self.process and self.process.poll() is None:
            for topic in (7, 8, 1):
                if self.row('SELECT busy FROM sessions WHERE thread_id=?', (topic,)) == [(1,)]:
                    self.admit('/stop', topic=topic)
            self.wait(lambda: self.row('SELECT count(*) FROM sessions WHERE busy=1') == [(0,)], 30)


def qualify(args):
    if args.resume:
        # This mode continues only this test's proven known-unsent first input.
        # Retain the original report, bindings, input identity and polling offsets.
        prior = json.loads((args.state / 'qualification.json').read_text())
        assert prior.get('success') is False
        assert set(prior.get('native_terminal_status', {})) == {'7', '8'}
        assert all(value == 'idle' for value in prior['native_terminal_status'].values())
        assert not (args.state / 'qualification-resume.json').exists(), 'Inspect an existing attempt before continuing again'
        with sqlite3.connect(args.state / 'state.sqlite') as db:
            bindings = db.execute('SELECT thread_id,codex_thread_id,busy FROM sessions').fetchall()
            retained = db.execute('SELECT phase FROM interaction_instructions').fetchall()
        assert {row[0] for row in bindings} == {7,8} and all(row[2] == 0 for row in bindings)
        assert retained == [('retained_unsent',)], retained
        sys.path.insert(0,str(pathlib.Path(__file__).resolve().parents[1]))
        from scripts.activity_indicator import Native
        reader = Native(str(pathlib.Path(args.codex).resolve()))
        try:
            for _, thread, _ in bindings:
                observed = reader.rpc('thread/read', {'threadId':thread,'includeTurns':False}) or {}
                node = observed.get('thread', {})
                assert node.get('id') == thread and node.get('status',{}).get('type') == 'idle'
        finally:
            reader.close()
    j = NativeJourney(args)
    report = {'scope': 'real native workers; synthetic Telegram identities and HTTP receipts',
              'live_telegram_acceptance': False, 'latency_seconds': {}, 'poll_resolution_ms': 20,
              'currency_cost': 'not established by native account metadata', 'success': False}
    try:
        j.start()
        with open(f'/proc/{j.process.pid}/exe','rb') as loaded:
            report['loaded_binary_sha256']=hashlib.file_digest(loaded,'sha256').hexdigest()
        report['qualification_sha256']=hashlib.sha256(pathlib.Path(__file__).read_bytes()).hexdigest()
        report['core_commit']=subprocess.check_output(['git','rev-parse','HEAD'],cwd=pathlib.Path(__file__).resolve().parents[1],text=True).strip()
        patch = subprocess.check_output(['git','diff','--','src'],cwd=pathlib.Path(__file__).resolve().parents[1])
        report['uncommitted_core_patch_sha256'] = hashlib.sha256(patch).hexdigest() if patch else None
        # Establish genuine independent native bindings before enabling the router.
        for topic, name in (() if args.resume else ((7, 'ALPHA'), (8, 'BETA'))):
            j.admit(f'This is an isolated native integration qualification. Reply exactly READY-{name}. No tools, goals, subagents or external actions.', topic=topic)
            j.wait(lambda name=name,topic=topic: j.has('READY-'+name, topic))
            j.idle()
        if not args.resume:
            j.stop()
        with j.config.open('a') as f:
            if not args.resume:
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
        j.messages.clear()
        report['configuration_sha256']=hashlib.sha256(j.config.read_bytes()).hexdigest()
        if not args.resume:
            j.start()
        if args.resume:
            rows = j.instructions(7)
            assert len(rows) == 1 and rows[0][1] == 'retained_unsent', rows
            assert j.instructions(8) == [] and not (j.root / 'alpha.started').exists()
            assert not (j.root / 'alpha.json').exists()
            (j.root / 'release-alpha').unlink(missing_ok=True)
            alpha = json.loads(rows[0][2])['origin']['message_id']
            started = time.monotonic()
            j.admit('/interaction retry '+rows[0][0], topic=1)
            j.wait(lambda: j.instructions(7)[0][1] == 'accepted', 90)
            report['latency_seconds']['alpha_resume'] = {'native_acceptance_observed': round(time.monotonic()-started,3)}
        else:
            alpha = j.staged('alpha', 7, '''#Alpha Isolated functional qualification; work only in this fixture cwd. No network, goals or subagents.
These files are internal test fixtures, not deliverables for Telegram. Use one Python command to create alpha.started and wait for release-alpha to exist, with a 90-second deadline. Do not finish or write the result before release. Another native session must continue independently while you wait.
After release, apply the latest admitted requirements. Initially the rate is 10 and quantities are [4,7,2]. Write alpha.json with fields rate, amounts (quantity times rate), total, and provenance {human: original human author ID, via: transport identifier}. Obtain provenance from the controller packet, distinguishing its relay from the original human author. Do not infer the author from the machine account. Finally report QUALIFIED-ALPHA and follow the controller result-envelope contract.''', report)
        j.wait(lambda: (j.root / 'alpha.started').exists())
        j.staged('correction', 7, 'Change the rate to 17 before writing alpha.json. Retain quantities [4,7,2] and the actual original-author/relay provenance. This amends the existing request. Report completion only after the file reflects this correction.', report, reply=alpha)
        beta = j.staged('beta', 8, '''#Beta Isolated functional qualification. No network, goals or subagents; do not modify alpha fixtures. In this cwd, write the internal fixture beta.json with exactly keys red, blue and total: grouped totals for red=[7,5] and blue=[3,9] plus their combined total. This is not a Telegram deliverable file. Report QUALIFIED-BETA and follow the controller result-envelope contract.''', report)
        # Plain entry conversation remains independently usable while Alpha waits.
        j.admit('This is an isolated responsiveness check of the integrating entry. Reply FRONT-RESPONSIVE, without tools or changing any worker task.', topic=1)
        j.wait(lambda: j.has('QUALIFIED-BETA', 1) and j.has('FRONT-RESPONSIVE', 1))
        assert not (j.root / 'alpha.json').exists(), 'Alpha ignored its release boundary'
        observed_beta = json.loads((j.root / 'beta.json').read_text())
        # Verify actual grouped values independently of native coverage metadata.
        assert observed_beta.get('red') == 12 and observed_beta.get('blue') == 12 and observed_beta.get('total') == 24, observed_beta
        (j.root / 'release-alpha').touch()
        j.wait(lambda: j.has('QUALIFIED-ALPHA', 1))
        j.idle()
        observed_alpha = json.loads((j.root / 'alpha.json').read_text())
        assert observed_alpha == {'rate': 17, 'amounts': [68,119,34], 'total': 221,
                                  'provenance': {'human': 100, 'via': 'interaction-controller'}}, observed_alpha
        request = json.loads(j.instructions(7)[0][2])['requests'][0]
        delivered = j.row('SELECT doc,phase FROM interaction_deliveries WHERE request_id=? ORDER BY rowid DESC', (request,))
        assert delivered and delivered[0][1] == 'delivered', delivered
        declaration = json.loads(delivered[0][0])
        assert declaration['declared'] and declaration['current'] and declaration['completed'], declaration
        # Native acceptance, declaration and observable file contents are separate checks.
        for marker, original in (('QUALIFIED-ALPHA',alpha),('QUALIFIED-BETA',beta)):
            actual = [(m,p) for m,p in j.messages if marker in m['text']]
            assert len(actual) == 1 and actual[0][1].get('reply_parameters',{}).get('message_id') == original, marker
        report.update(success=True, behavior={'alpha':observed_alpha,'beta':observed_beta},
                      independent_entry=True, reverse_worker_completion=True,
                      native_thread_count=len(j.row('SELECT DISTINCT codex_thread_id FROM sessions WHERE codex_thread_id IS NOT NULL')))
    except Exception as error:
        report['failure'] = str(error)
        raise
    finally:
        try:
            j.cleanup_workers()
            report['scoped_cleanup'] = 'fixture wait released; listener-owned worker turns idle'
            # Reuse the read-only native observer, restricted to newly created
            # qualification bindings. Do not enumerate unrelated native history.
            sys.path.insert(0,str(pathlib.Path(__file__).resolve().parents[1]))
            from scripts.activity_indicator import Native
            reader=Native(j.codex)
            try:
                report['native_terminal_status']={}
                report['native_token_usage']={}
                for topic,thread in j.row('SELECT thread_id,codex_thread_id FROM sessions WHERE codex_thread_id IS NOT NULL'):
                    observed=reader.rpc('thread/read',{'threadId':thread,'includeTurns':False})
                    if not observed or observed.get('thread',{}).get('id')!=thread:
                        raise AssertionError('Selected qualification native status was not confirmed')
                    node=observed['thread'];status=node.get('status',{}).get('type')
                    report['native_terminal_status'][str(topic)]=status
                    if status not in ('idle','notLoaded'):
                        raise AssertionError('Selected qualification native turn remains '+str(status))
                    if node.get('path'):
                        path=pathlib.Path(node['path']).resolve()
                        base=pathlib.Path(j.native_home)/'sessions'
                        if path.is_relative_to(base.resolve()) and thread in path.name:
                            with path.open('rb') as history:
                                history.seek(0,2);size=history.tell();history.seek(max(0,size-2*1024*1024))
                                if size>2*1024*1024:history.readline()
                                for line in history:
                                    if len(line)>1024*1024:continue
                                    try: value=json.loads(line)
                                    except ValueError:continue
                                    data=value.get('payload',{})
                                    if data.get('type')=='token_count':
                                        usage=(data.get('info') or {}).get('total_token_usage')
                                        if usage:report['native_token_usage'][str(topic)]=usage
            finally:reader.close()
        except Exception as error:
            report['scoped_cleanup'] = 'unconfirmed: '+str(error)
            report['success'] = False
            report.setdefault('failure', 'Scoped cleanup was not confirmed')
        report['preparation_stages'] = [dict(stage=stage, elapsed_ms=int(elapsed))
            for stage,elapsed in re.findall(r'stage="(initialize|thread_binding|thread_bound)" elapsed_ms=(\d+)', (j.root / 'listener.log').read_text())]
        try:
            j.close()
        except Exception as error:
            report['success'] = False
            report['listener_cleanup'] = 'unconfirmed: '+str(error)
            report.setdefault('failure', 'Fixture listener shutdown was not confirmed')
        (j.root / ('qualification-resume.json' if args.resume else 'qualification.json')).write_text(json.dumps(report, indent=2)+'\n')
        print(json.dumps({k:v for k,v in report.items() if k != 'failure'}, indent=2), flush=True)
    if not report['success']:
        raise AssertionError(report.get('failure','Native qualification did not succeed'))


if __name__ == '__main__':
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('binary')
    p.add_argument('--state', required=True, type=pathlib.Path, help='New private qualification directory; retained after execution')
    p.add_argument('--resume', action='store_true', help='Resume the existing fixture only after proven pre-submission retention')
    p.add_argument('--codex', required=True)
    p.add_argument('--native-home', required=True)
    p.add_argument('--run-native', action='store_true', help='Explicitly enable authenticated native model calls')
    args = p.parse_args()
    if not args.run_native:
        p.error('--run-native is required; this test uses the selected real native account')
    selected_home=pathlib.Path(os.environ.get('CODEX_HOME',pathlib.Path.home()/'.codex')).resolve()
    if args.native_home and pathlib.Path(args.native_home).resolve()!=selected_home:
        p.error('--native-home must select this process existing native home; no other body is inferred')
    qualify(args)
