#!/usr/bin/env python3
"""Actual bridge, native WebSocket peer, SQLite and Telegram HTTP. Zero inference."""
import copy
import json
from pathlib import Path
import sys
import threading
import time
import socket
import sqlite3
from goals_io import Goals


class Recovery(Goals):
    def __init__(self, binary):
        self.history = {}
        self.sockets = {}
        self.lose_initial = False
        self.freeze_live = False
        self.reject_once = False
        self.rejected = 0
        self.delayed = False
        self.steered = threading.Event()
        super().__init__(binary, 'hold')

    def native_connection(self, sock):
        self.sockets[id(sock)] = sock
        try:
            super().native_connection(sock)
        finally:
            self.sockets.pop(id(sock), None)

    def disconnect_writer(self):
        selected = [self.sockets[ident] for ident in list(self.clients) if ident in self.sockets]
        assert len(selected) == 1, 'Only the selected synthetic collector may disconnect'
        selected[0].shutdown(socket.SHUT_RDWR)

    def broadcast(self, thread, method, params):
        turns = self.history.setdefault(thread, [])
        if method == 'turn/started':
            ident = params['turn']['id']
            if not any(t['id'] == ident for t in turns):
                turns.append({'id': ident, 'status': 'inProgress', 'items': []})
        if method == 'item/completed':
            turn = next(t for t in turns if t['id'] == params['turnId'])
            if not any(i['id'] == params['item']['id'] for i in turn['items']):
                turn['items'].append(copy.deepcopy(params['item']))
        if method == 'turn/completed':
            next(t for t in turns if t['id'] == params['turn']['id'])['status'] = params['turn']['status']
        if not self.freeze_live:
            super().broadcast(thread, method, params)

    def handle_extra_rpc(self, message, send):
        method, p = message.get('method'), message.get('params', {})
        if method == 'turn/start' and self.lose_initial:
            self.lose_initial = False
            self.native_turns += 1
            turn = 'accepted-without-start-ack'
            self.history[p['threadId']] = [{'id': turn, 'status': 'completed', 'items': [
                {'type': 'userMessage', 'id': 'retained-human', 'clientId': p['clientUserMessageId'], 'content': p['input']},
                {'type': 'agentMessage', 'id': 'retained-final', 'phase': 'final_answer', 'text': 'Initial receipt recovered.'}]}]
            self.disconnect_writer()
            return True
        if method == 'thread/turns/list':
            send({'id': message['id'], 'result': {'data': copy.deepcopy(self.history.get(p['threadId'], []))[::-1]}})
            return True
        if method == 'turn/steer':
            turn = self.holding[p['threadId']]
            assert p['expectedTurnId'] == turn
            current = next(t for t in self.history[p['threadId']] if t['id'] == turn)
            current['items'].append({'type': 'userMessage', 'id': 'later-user',
                                     'clientId': p['clientUserMessageId'], 'content': p['input']})
            self.steered.set()
            def reply():
                if self.delayed:
                    time.sleep(11)
                send({'id': message['id'], 'result': {'turnId': turn}})
            threading.Thread(target=reply, daemon=True).start()
            return True

    def before_send_message_reply(self, payload):
        if self.reject_once and payload['text'] == 'Recovered answer.':
            self.reject_once = False
            self.rejected += 1
            return {'ok': False, 'error_code': 429, 'description': 'Synthetic explicit rejection',
                    'parameters': {'retry_after': 1}}

    def exact(self, text):
        return sum(m['text'] == text for m, _ in self.messages)

    def complete(self, text, topic=7):
        native = 'native-topic-'+str(topic)
        turn = self.holding.pop(native)
        self.broadcast(native, 'item/completed', {'turnId': turn, 'item': {
            'type': 'agentMessage', 'id': turn+'-final', 'phase': 'final_answer', 'text': text}})
        self.broadcast(native, 'turn/completed', {'turn': {'id': turn, 'status': 'completed'}})


def qualify(binary):
    j = Recovery(str(Path(binary).resolve()))
    try:
        j.start()
        j.admit('Keep the native writer active.')
        j.wait(lambda: bool(j.holding))
        j.wait(lambda: j.row('SELECT count(*) FROM native_collections') == [(1,)])
        j.delayed = True
        update = j.admit('Later correction with a delayed native ACK.')
        assert j.steered.wait(10)
        j.wait(lambda: j.has('Your message is retained while Codex acceptance is being checked.'), seconds=15)
        j.wait(lambda: j.row('SELECT count(*) FROM native_input_receipts WHERE update_id=?', (update,)) == [(1,)], seconds=5)
        j.command('/status', 'Codex accepted 1 retained inputs')
        j.freeze_live = True
        j.reject_once = True
        j.complete('Recovered answer.')  # Native persistence exists; the live notification was lost.
        # Recovery only reads history. No second turn/start or turn/steer is permitted.
        j.wait(lambda: j.row("SELECT count(*) FROM topic_publications WHERE body='Recovered answer.' AND phase='sent'") == [(1,)], seconds=45)
        assert j.rejected == 1
        # HTTP fixture records rejected attempts too: two requests, one accepted publication.
        assert j.exact('Recovered answer.') == 2
        j.wait(lambda: j.row('SELECT count(*) FROM sessions WHERE busy=1') == [(0,)], seconds=10)
        j.freeze_live = False
        j.admit('Continue normally after the recovered completion.')
        j.wait(lambda: j.native_turns == 2)
        j.complete('Subsequent direct reply.')
        j.wait(lambda: j.exact('Subsequent direct reply.') == 1)
        j.idle()
        starts = sum(r.get('method') == 'turn/start' for r in j.rpc)
        steers = sum(r.get('method') == 'turn/steer' for r in j.rpc)
        assert (starts, steers) == (2, 1)
        j.stop(); j.start()
        time.sleep(.6)
        assert j.process.poll() is None
        assert j.exact('Recovered answer.') == 2
        assert sum(r.get('method') == 'turn/start' for r in j.rpc) == starts
        assert sum(r.get('method') == 'turn/steer' for r in j.rpc) == steers
        assert not any(r.get('method') in ('thread/goal/set', 'thread/goal/clear', 'turn/interrupt') for r in j.rpc)
        print('PASS late native ACK, scoped status, lost live completion recovery, explicit 429 retry, restart deduplication and zero native replay/control')
    finally:
        j.close()


def lost_initial(binary):
    j=Recovery(str(Path(binary).resolve()))
    try:
        j.lose_initial=True
        j.start()
        original=j.admit('An original input whose initial native acknowledgment is lost.')
        j.wait(lambda:j.row("SELECT count(*) FROM turns WHERE status='failed'")==[(1,)])
        j.wait(lambda:j.has('Initial receipt recovered.'),seconds=45)
        assert j.exact('Initial receipt recovered.')==1
        assert j.row('SELECT count(*) FROM native_input_receipts WHERE update_id=?',(original,))==[(1,)]
        assert sum(r.get('method')=='turn/start' for r in j.rpc)==1
        assert not any(r.get('method') in ('turn/steer','thread/goal/set','thread/goal/clear','turn/interrupt') for r in j.rpc)
        print('PASS accepted start without any native turn acknowledgment, exact original client receipt and recovered final without replay')
    finally:j.close()


def lost_socket(binary):
    j=Recovery(str(Path(binary).resolve()))
    try:
        j.start();j.admit('Keep this exact writer alive after its collector disconnects.')
        j.wait(lambda:bool(j.holding))
        j.wait(lambda:j.row('SELECT count(*) FROM native_collections')==[(1,)])
        j.disconnect_writer()
        j.wait(lambda:j.row("SELECT count(*) FROM turns WHERE status='failed'")==[(1,)])
        j.wait(lambda:j.row("SELECT count(*) FROM bot_state WHERE key LIKE 'collector_attachment:%'")==[(1,)],seconds=45)
        j.complete('Reattached writer reply.')
        j.wait(lambda:j.exact('Reattached writer reply.')==1)
        j.idle()
        assert sum(r.get('method')=='turn/start' for r in j.rpc)==1
        assert not any(r.get('method') in ('turn/steer','thread/goal/set','thread/goal/clear','turn/interrupt') for r in j.rpc)
        print('PASS disconnected collector reattaches same physical writer, delivers final once and preserves native execution')
    finally:j.close()


def projection_modes(binary):
    j=Recovery(str(Path(binary).resolve()))
    try:
        j.start()
        for topic in (7,8):
            j.admit('Seed a direct source.',topic=topic)
            j.wait(lambda:'native-topic-'+str(topic) in j.holding)
            j.complete('Seed answer '+str(topic),topic)
            j.wait(lambda:j.has('Seed answer '+str(topic),topic));j.idle()
        j.stop()
        with sqlite3.connect(j.root/'state.sqlite') as d:
            rows=d.execute('SELECT t.id,t.session_id FROM turns t JOIN sessions s ON s.id=t.session_id ORDER BY s.thread_id').fetchall()
            for (local,session),body,direct in zip(rows,('Disabled mirror must not send.','Direct queue survives restart.'),(False,True)):
                if not direct:d.execute('DELETE FROM direct_topic_turns WHERE local_turn=?',(local,))
                d.execute("INSERT INTO topic_publications(local_turn,session_id,human_id,kind,source,body,phase,instance_id) VALUES(?,?,100,'notice',?,?,'ready','')",(local,session,body,body))
        # No integrator opt-in or enrollment is configured; only the direct row can publish.
        j.start()
        j.wait(lambda:j.has('Direct queue survives restart.',8))
        j.wait(lambda:j.row("SELECT phase FROM topic_publications WHERE body='Disabled mirror must not send.'")==[('retired',)])
        assert j.exact('Disabled mirror must not send.')==0
        print('PASS old opt-in mirror remains disabled while new direct queue resumes without routing enrollment')
    finally:j.close()


def shared_participant(binary):
    class Group(Recovery):
        def admit(self,text=None,*,topic=7,user=100,callback=None,reply=None):
            assert callback is None and reply is None
            with self.lock:
                self.sequence+=1
                self.updates.append({'update_id':self.sequence,'message':{'message_id':self.sequence,'message_thread_id':topic,'from':{'id':user,'is_bot':False,'first_name':'Fixture'},'chat':{'id':-100,'type':'supergroup'},'text':text}})
                return self.sequence
    j=Group(str(Path(binary).resolve()))
    try:
        j.config.write_text(j.config.read_text().replace('startup_admin_ids=[100]','startup_admin_ids=[100,101]'))
        j.start();j.admit('Owner source.')
        j.wait(lambda:bool(j.holding));j.complete('Owner reply.')
        j.wait(lambda:j.exact('Owner reply.')==1);j.idle()
        # Authorize the second synthetic participant through the existing allow command.
        with sqlite3.connect(j.root/'state.sqlite') as d:
            d.execute("INSERT OR REPLACE INTO users(tg_user_id,role,allowed,created_at,updated_at) VALUES(101,'admin',1,datetime('now'),datetime('now'))")
        j.admit('Allowed participant source.',user=101)
        j.wait(lambda:j.native_turns==2)
        j.complete('Allowed participant reply.')
        j.wait(lambda:j.exact('Allowed participant reply.')==1);j.idle()
        j.command('/status','Current Telegram session',user=101)
        assert not any('Native delivery status owner mismatch' in m['text'] for m,_ in j.messages)
        print('PASS allowed group participant retains native admission, reply and standard status')
    finally:j.close()


if __name__ == '__main__':
    import concurrent.futures
    with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:
        checks=[pool.submit(check,sys.argv[1]) for check in (qualify,lost_initial,lost_socket,projection_modes,shared_participant)]
        for check in checks:check.result()
