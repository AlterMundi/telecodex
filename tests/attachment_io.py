#!/usr/bin/env python3
"""Owner-local collector handoff; real IPC/HTTP/SQLite, simulated native work.

No real model, Telegram, peer message or coordinated Stop is used.
"""
import copy
import json
import subprocess
import sys
import threading
import sqlite3
import pathlib

from stopping_io import Stopping
from routing_io import Routing


def attach(j, topic, native, turn, ok=True):
    result = subprocess.run([j.binary, '--control', str(j.config), 'attach-active',
                             '100', str(topic), native, turn],
                            capture_output=True, text=True, timeout=35)
    assert (result.returncode == 0) == ok, (result.stdout, result.stderr)
    return json.loads(result.stdout)


def restart_collectors(binary):
    j = Stopping(binary)
    try:
        j.seed()
        j.admit('Original unfinished third request', topic=9)
        j.wait(lambda: j.holding.get('native-topic-9'))
        j.preload('native-topic-9', 'blocked')
        before = copy.deepcopy(j.goals)
        originals = dict(j.holding)
        starts = sum(r.get('method') == 'turn/start' for r in j.rpc)
        j.stop()
        assert j.holding == originals and j.goals == before
        with j.config.open('a') as f:
            f.write('\n[interaction]\nhuman_id=100\nchat_id=100\ntopic_id=1\nmax_active_workers=2\n')
            for topic in (7,8,9):
                f.write(f'[[interaction.targets]]\nname="Worker{topic}"\ntopic_id={topic}\n')
        j.start()
        checkpoint = len(j.rpc)
        for topic in (7, 8, 9):
            native = 'native-topic-'+str(topic)
            response = attach(j, topic, native, originals[native])
            assert response['result']['state'] == 'attachment_queued'
        j.wait(lambda: sum(r.get('method') == 'thread/turns/list' and r['params'].get('limit') == 1 for r in j.rpc[checkpoint:]) == 3)
        assert j.goals == before
        for topic in (7, 8, 9):
            native = 'native-topic-'+str(topic)
            j.finish(native, originals[native])
        j.wait(lambda: j.has('Continuation delivered.', topic=7))
        j.wait(lambda: j.has('Initial goal turn delivered.', topic=8))
        j.wait(lambda: j.has('Initial goal turn delivered.', topic=9))
        j.idle()
        assert sum(r.get('method') == 'turn/start' for r in j.rpc) == starts
        assert not any(r.get('method') in ('thread/goal/set', 'thread/goal/clear', 'turn/interrupt') for r in j.rpc[checkpoint:])
        assert j.goals['native-topic-8'] == before['native-topic-8']
        assert j.goals['native-topic-9'] == before['native-topic-9']
        assert j.row("SELECT count(*) FROM incoming_updates WHERE status='undetermined'") == [(3,)]
        assert j.row('SELECT count(*) FROM stop_operations') == [(0,)]
        print('PASS exact native collectors survive restart, deliver active/paused/blocked work without goal mutation, kickoff or journal replay')
    finally:
        j.close()


def refuses_changed_writer(binary):
    j = Stopping(binary)
    try:
        j.start()
        j.admit('Original ordinary work')
        j.wait(lambda: j.holding.get('native-topic-7'))
        original = j.holding['native-topic-7']
        j.stop(); j.start()
        checkpoint = len(j.rpc)
        attach(j, 7, 'different-native', original, ok=False)
        attach(j, 7, 'native-topic-7', 'different-turn', ok=False)
        # Valid binding to another admitted human is still not this controller's.
        j.admit('Other human work', topic=8, user=101)
        j.wait(lambda: j.holding.get('native-topic-8'))
        attach(j, 8, 'native-topic-8', j.holding['native-topic-8'], ok=False)
        previous = j.handle_extra_rpc
        def completed_before_subscription(request, send):
            p = request.get('params', {})
            if request.get('method') == 'thread/turns/list' and p.get('limit') == 1:
                j.holding.pop('native-topic-7', None)
                send({'id': request['id'], 'result': {'data': [], 'nextCursor': None}})
                return True
            return previous(request, send)
        j.handle_extra_rpc = completed_before_subscription
        attach(j, 7, 'native-topic-7', original)
        j.wait(lambda: j.row("SELECT count(*) FROM turns WHERE prompt='Owner-local collector attachment; no new native input.' AND status='failed'") == [(1,)])
        assert not any(r.get('method') in ('thread/goal/set','thread/goal/clear','turn/interrupt') for r in j.rpc[checkpoint:])
        assert sum(r.get('method')=='turn/start' for r in j.rpc)==2
        print('PASS wrong physical turn, native binding, owner and completion-before-subscription race cause no kickoff or goal mutation')
    finally:
        j.close()


def routed_return_survives(binary):
    j = Routing(binary)
    try:
        j.prepare()
        original = j.admit('#Alpha Retain this routed request through collector replacement', topic=1)
        j.wait(lambda: j.instructions(7) and j.instructions(7)[0][1] == 'accepted')
        instruction = json.loads(j.instructions(7)[0][2])
        physical = instruction['native_turn']
        local = instruction['local_turn']
        j.stop()
        with sqlite3.connect(j.root/'state.sqlite') as db:
            db.execute("UPDATE turns SET status='failed',completed_at='2026-01-01' WHERE id=?",(local,))
        with j.config.open('a') as f:
            f.write(f'\n[session_control]\nsocket_path="{j.root}/control.sock"\nhuman_id=100\n')
        j.start()
        j.wait(lambda: (j.root/'control.sock').exists())
        retained_ingress=j.row('SELECT status FROM incoming_updates WHERE update_id=?',(original,))
        # Routing's simulated worker records turns in runs rather than holding.
        previous = j.handle_extra_rpc
        def live_writer(request, send):
            if request.get('method')=='thread/turns/list':
                send({'id':request['id'],'result':{'data':[{'id':physical,'status':'inProgress'}], 'nextCursor':None}})
                return True
            return previous(request,send)
        j.handle_extra_rpc=live_writer
        checkpoint=len(j.rpc)
        attach(j, 7, 'native-topic-7', physical)
        j.wait(lambda: any(r.get('method')=='thread/turns/list' and r['params'].get('limit')==1 for r in j.rpc[checkpoint:]))
        j.wait(lambda: j.row("SELECT status FROM turns WHERE id=?",(local,))==[('running',)] and j.row("SELECT count(*) FROM bot_state WHERE key=?",('collector_attachment:'+str(local),))==[(1,)])
        j.broadcast('native-topic-7','item/completed',{'turnId':physical,'item':{'id':'plain-after-handoff','type':'agentMessage','phase':'final_answer','text':'Plain routed evidence after collector replacement'}})
        j.wait(lambda: j.has('Plain routed evidence after collector replacement',1))
        assert not j.has('Plain routed evidence after collector replacement',7)
        j.answer(7, 'Original routed result after collector replacement', item='after-handoff')
        j.wait(lambda: j.has('Original routed result after collector replacement',1))
        j.wait(lambda: j.row("SELECT count(*) FROM interaction_deliveries WHERE phase='delivered'")==[(2,)])
        j.idle()
        assert j.instructions(7)[0][1]=='accepted'
        assert json.loads(j.instructions(7)[0][2])['local_turn']==local
        assert j.row('SELECT DISTINCT local_turn FROM interaction_results')==[(local,)]
        assert not j.has('Original routed result after collector replacement',7)
        assert not any(r.get('method') in ('turn/start','turn/steer','thread/goal/set','thread/goal/clear','turn/interrupt') for r in j.rpc[checkpoint:])
        assert j.row('SELECT status FROM incoming_updates WHERE update_id=?',(original,))==retained_ingress
        print('PASS original routed execution/coverage/result return survive collector replacement without new acceptance or replay')
    finally:
        j.close()


def changed_binding_during_resume(binary):
    j=Stopping(binary)
    reached=threading.Event();release=threading.Event()
    try:
        j.start();j.admit('Original direct work')
        j.wait(lambda: j.holding.get('native-topic-7'))
        physical=j.holding['native-topic-7']
        j.stop();j.start()
        previous=j.handle_extra_rpc
        def delayed_resume(request,send):
            if request.get('method')=='thread/resume':
                reached.set();release.wait(timeout=15)
                send({'id':request['id'],'result':{'thread':{'id':'native-topic-7','turns':[]},'model':'fixture-model'}})
                return True
            return previous(request,send)
        j.handle_extra_rpc=delayed_resume
        checkpoint=len(j.rpc)
        attach(j,7,'native-topic-7',physical)
        j.wait(reached.is_set)
        # Simulate the competing /new binding transaction while native resume
        # is awaiting its response; the collector must never undo that change.
        with sqlite3.connect(j.root/'state.sqlite') as db:
            db.execute('UPDATE sessions SET codex_thread_id=NULL,force_fresh_thread=1 WHERE thread_id=7')
        release.set()
        j.wait(lambda: j.row("SELECT count(*) FROM turns WHERE prompt='Owner-local collector attachment; no new native input.' AND status='failed'")==[(1,)])
        assert j.row('SELECT codex_thread_id,force_fresh_thread FROM sessions WHERE thread_id=7')==[(None,1)]
        assert not any(r.get('method') in ('turn/start','thread/goal/set','thread/goal/clear','turn/interrupt') for r in j.rpc[checkpoint:])
        print('PASS binding changed during awaited native resume remains cleared; collector cannot restore it')
    finally:
        release.set();j.close()


if __name__ == '__main__':
    binary=str(pathlib.Path(sys.argv[1]).resolve())
    restart_collectors(binary)
    refuses_changed_writer(binary)
    routed_return_survives(binary)
    changed_binding_during_resume(binary)
