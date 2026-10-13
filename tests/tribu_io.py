#!/usr/bin/env python3
"""Dedicated writer, deadline and rollback journeys; every account is synthetic."""
import base64
import hashlib
import json
import pathlib
import socket
import sqlite3
import struct
import sys
import threading
import time

from questions_io import Journey


class TribuJourney(Journey):
    def __init__(self, binary, mode):
        super().__init__(binary, mode)
        self.submitted = threading.Event()
        self.interrupted = threading.Event()
        self.release = threading.Event()
        self.sends = []
        self.turn_sequence = 0
        self.metadata_completed = False
        self.private = self.root / 'private'
        self.private.mkdir(mode=0o700)

    def prepare(self):
        # Wait for the existing command consumer and shutdown handler, not
        # merely a schema write during startup. This submits no native turn.
        self.start()
        probe = self.admit('/status', topic=0)
        def initialized():
            try:
                return self.row('SELECT status FROM incoming_updates WHERE update_id=?', (probe,)) == [('handled',)]
            except sqlite3.OperationalError:
                return False
        self.wait(initialized)
        self.stop()
        with sqlite3.connect(self.root / 'state.sqlite') as db:
            columns = {r[1] for r in db.execute('PRAGMA table_info(sessions)')}
            row = {'chat_id':100,'thread_id':7,'creator_user_id':100,'cwd':str(self.root),
                   'session_title':'Tribu' if self.mode == 'unknown' else 'I/O','force_fresh_thread':1,'busy':0,
                   'approval_policy':'never','sandbox_mode':'read-only','search_mode':'disabled',
                   'add_dirs_json':'[]','created_at':'2026-01-01T00:00:00Z','updated_at':'2026-01-01T00:00:00Z'}
            assert set(row) <= columns
            db.execute('INSERT INTO sessions ('+','.join(row)+') VALUES ('+','.join('?' for _ in row)+')',list(row.values()))
        with self.config.open('a') as f:
            f.write(f'''\n[tribu]
socket_path="{self.private}/attention.sock"
owner_client_file="{self.private}/owner.json"
being_ref="synthetic-being"
body_label="fixture.codex@fixture"
embodiment_id="synthetic-embodiment"
human_id=100
chat_id=100
topic_id=7
activated_after=1
deadline_seconds=2
max_pending=20
''')
        self.start()
        try:
            self.wait(self.ready)
        except Exception:
            print(self.process.communicate(timeout=3)[1], file=sys.stderr)
            raise

    def ready(self):
        try:
            self.local_status()
            return True
        except (OSError, json.JSONDecodeError):
            return False

    def local_status(self):
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as s:
            s.settimeout(3)
            s.connect(str(self.private / 'attention.sock'))
            s.sendall(b'{"op":"status"}\n')
            return json.loads(s.recv(8192))['result']

    def native_connection(self, sock):
        stream = sock.makefile('rb')
        headers = {}
        while True:
            line = stream.readline()
            if line == b'\r\n': break
            if not line: return
            if b':' in line:
                key, value = line.split(b':', 1)
                headers[key.strip().lower()] = value.strip()
        accept = base64.b64encode(hashlib.sha1(headers[b'sec-websocket-key'] + b'258EAFA5-E914-47DA-95CA-C5AB0DC85B11').digest())
        sock.sendall(b'HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: '+accept+b'\r\n\r\n')
        def send(value):
            data = json.dumps(value).encode()
            sock.sendall((bytes([129,len(data)]) if len(data)<126 else b'\x81\x7e'+struct.pack('!H',len(data)))+data)
        def finish(turn, status):
            send({'method':'turn/completed','params':{'threadId':'tribu-native','turn':{'id':turn,'status':status}}})
        while True:
            first = stream.read(2)
            if len(first)<2: return
            size = first[1]&127
            if size==126: size=struct.unpack('!H',stream.read(2))[0]
            elif size==127: size=struct.unpack('!Q',stream.read(8))[0]
            mask=stream.read(4); data=stream.read(size)
            if first[0]&15==8: return
            if first[0]&15!=1: continue
            m=json.loads(bytes(v^mask[i%4] for i,v in enumerate(data)))
            method, ident, p=m.get('method'),m.get('id'),m.get('params',{})
            with self.lock: self.rpc.append(m)
            if method in ('thread/start','thread/resume'):
                assert p['config']['features.goals'] is False
                assert p['config']['features.multi_agent'] is False
                if method=='thread/resume': assert p['threadId']=='tribu-native'
                send({'id':ident,'result':{'thread':{'id':'tribu-native'},'model':'fixture-model'}})
            elif method=='turn/start':
                context=p['additionalContext']['telecodex.tribu.deadline']['value']
                assert '2 seconds' in context and 'absolute UTC deadline:' in context
                assert p['threadId']=='tribu-native'
                with self.lock:
                    self.turn_sequence+=1
                    turn='turn-'+str(self.turn_sequence)
                    self.sends.append((sock,send,turn))
                send({'id':ident,'result':{'turn':{'id':turn}}})
                self.submitted.set()
                if self.mode=='queue' and self.turn_sequence==1:
                    self.release.wait(5)
                if self.mode=='queue' or self.mode=='recovery' and self.turn_sequence>1: finish(turn,'completed')
            elif method=='turn/interrupt':
                self.interrupted.set()
                send({'id':ident,'result':{}})
                if self.mode not in ('unknown','recovery'):
                    with self.lock:
                        primary=self.sends[0]
                    primary[1]({'method':'turn/completed','params':{'threadId':'tribu-native','turn':{'id':p['turnId'],'status':'interrupted'}}})
            elif method=='turn/steer':
                raise AssertionError('dedicated private input must queue, never steer')
            elif method=='thread/turns/list':
                data = [{'id':'turn-1','status':'completed','items':[]}] if self.metadata_completed else []
                send({'id':ident,'result':{'data':data,'nextCursor':None}})
            elif ident is not None: send({'id':ident,'result':{}})


def scenario(binary, mode):
    j=TribuJourney(binary,mode)
    try:
        j.prepare()
        assert j.local_status()['paused'] is False
        unauthorized = j.admit('another admin is not the paired human', user=101)
        j.wait(lambda: j.row('SELECT status FROM incoming_updates WHERE update_id=?',(unauthorized,)) == [('handled',)])
        assert j.turn_sequence == 0
        if mode == 'queue':
            # A detached collector cannot allow a new writer, even after restart
            # or through a callback. Only the deployment controller releases it.
            hold_key = 'native_writer_hold:100:7'
            with sqlite3.connect(j.root / 'state.sqlite') as db:
                db.execute('INSERT INTO bot_state(key,value) VALUES(?,?)', (hold_key, '{}'))
            for restarted in (False, True):
                if restarted:
                    j.stop(); j.start(); j.wait(j.ready)
                held = j.admit('do not open another writer')
                j.wait(lambda: j.row('SELECT status FROM incoming_updates WHERE update_id=?', (held,)) == [('handled',)])
                source = next(m[0] for m in reversed(j.messages) if m[0].get('message_thread_id') == 7)
                pressed = j.admit(callback=(source, 'goal-resume:stale-button'))
                j.wait(lambda: j.row('SELECT status FROM incoming_updates WHERE update_id=?', (pressed,)) == [('handled',)])
                assert j.turn_sequence == 0 and not j.row('SELECT id FROM tribu_admissions')
            with sqlite3.connect(j.root / 'state.sqlite') as db:
                db.execute('DELETE FROM bot_state WHERE key=?', (hold_key,))
        first=j.admit('first human request')
        j.wait(j.submitted.is_set)
        if mode=='queue':
            j.admit('second private request')
            j.wait(lambda:len(j.row('SELECT id FROM tribu_admissions'))==2)
            assert j.turn_sequence==1
            j.release.set()
            j.wait(lambda:j.row('SELECT state FROM tribu_admissions ORDER BY created_at')==[('completed',),('completed',)])
            assert j.row('SELECT codex_thread_id FROM sessions WHERE thread_id=7')==[('tribu-native',)]
            assert not any(m.get('method')=='turn/steer' for m in j.rpc)
        elif mode=='pause':
            started=time.monotonic()
            j.admit('/tribu pause')
            j.wait(j.interrupted.is_set)
            assert time.monotonic()-started<1.5, 'pause waited for the deadline'
            j.wait(lambda:j.row('SELECT state FROM tribu_admissions')==[('interrupted',)])
            j.stop();j.start()
            j.wait(j.ready)
            assert j.local_status()['paused'] is True
            j.admit('must not wake while rolled back')
            time.sleep(.15)
            assert j.turn_sequence==1
        else:
            j.wait(j.interrupted.is_set)
            j.wait(lambda:j.row('SELECT state FROM tribu_admissions')==[('undetermined',)],seconds=15)
            j.admit('must not replay an unknown turn')
            j.wait(lambda:any('uncertain native turn' in m[1]['text'] for m in j.messages))
            j.stop();j.start()
            j.wait(j.ready)
            assert j.local_status()['undetermined_count']==1
            assert j.turn_sequence==1
            if mode=='recovery':
                with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as s:
                    s.settimeout(5);s.connect(str(j.private/'attention.sock'))
                    s.sendall(b'{"op":"reconcile"}\n')
                    state=json.loads(s.recv(8192))
                    assert state['ok'] and state['result']['undetermined_count']==1
                assert j.turn_sequence==1
                j.metadata_completed=True
                j.admit('new human request after verified completion')
                j.wait(lambda:j.turn_sequence==2)
                j.wait(lambda:j.local_status()['undetermined_count']==0)
                assert j.row('SELECT codex_thread_id FROM sessions WHERE thread_id=7')==[('tribu-native',)]
                assert j.row('SELECT state FROM tribu_admissions ORDER BY created_at')==[('completed',),('completed',)]
        assert all(m.get('params',{}).get('clientUserMessageId')==f'private:123:{first}' or mode in ('queue','recovery') for m in j.rpc if m.get('method')=='turn/start')
        print(json.dumps({'scenario':'tribu_'+mode,'persistent_writer':True,'no_automatic_replay':True}),flush=True)
    finally:j.close()


if __name__=='__main__':
    for mode in sys.argv[2:] or ['queue','pause','unknown','recovery']:
        scenario(str(pathlib.Path(sys.argv[1]).resolve()),mode)
