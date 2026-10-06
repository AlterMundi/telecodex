#!/usr/bin/env python3
"""Ingress regressions with actual HTTP, WebSocket and SQLite I/O. No live calls.

Usage: python3 independent-io-probes.py /absolute/telecodex-binary
Every token, HOME, CODEX_HOME, workspace, SQLite DB and server is synthetic.
Reports behavior without encoding candidate-specific expected assertions.
"""
import base64
import hashlib
import html
import http.server
import json
import pathlib
import signal
import socketserver
import sqlite3
import struct
import subprocess
import sys
import tempfile
import threading
import time

BINARY = str(pathlib.Path(sys.argv[1]).resolve())


def token_probe():
    with tempfile.TemporaryDirectory(prefix='telecodex-review-token-') as name:
        root = pathlib.Path(name)
        token = 'synthetic-secret-token'

        class Handler(http.server.BaseHTTPRequestHandler):
            def do_POST(self):
                self.rfile.read(int(self.headers.get('Content-Length', '0')))
                body = json.dumps({'ok': True, 'result': token}).encode()
                self.send_response(200)
                self.send_header('Content-Length', str(len(body)))
                self.end_headers()
                self.wfile.write(body)

            def log_message(self, *args):
                pass

        server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Handler)
        threading.Thread(target=server.serve_forever, daemon=True).start()
        tokenfile = root / 'bot.token'
        tokenfile.write_text(token)
        tokenfile.chmod(0o600)
        cfg = root / 'local.toml'
        cfg.write_text(f'''db_path="{root}/state.sqlite"
startup_admin_ids=[100]
[telegram]
bot_token_file="{tokenfile}"
api_base="http://127.0.0.1:{server.server_port}"
[codex]
binary="/bin/true"
default_cwd="{root}"
import_cli_history=false
import_desktop_history=false
''')
        env = {'PATH': '/usr/bin:/bin', 'HOME': str(root),
               'CODEX_HOME': str(root / 'codex-home'), 'LANG': 'C.UTF-8'}
        try:
            result = subprocess.run([BINARY, str(cfg)], cwd=root, env=env,
                                    capture_output=True, text=True, timeout=5)
            assert result.returncode != 0
            assert token not in result.stderr, 'malformed success leaked the bot token'
            print(json.dumps({'scenario': 'malformed_success_token',
                              'exit': result.returncode,
                              'token_in_stderr': token in result.stderr,
                              'stderr': result.stderr.strip()}), flush=True)
        finally:
            server.shutdown()
            server.server_close()


def scenario(mode):
    retain_commentary = mode.startswith('retained_commentary')
    drafts = mode.endswith('_drafts')
    show_unfinished = '_hidden' not in mode
    commentary_only = mode.endswith('_only')
    commentary = ['First completed progress.', 'Second progress: ' + 'x' * 3800]
    final_answer = 'Final answer.'
    acknowledge_steer = mode == 'accepted_steer'
    missing_binding = mode == 'missing_saved_binding'
    with tempfile.TemporaryDirectory(prefix='telecodex-review-steer-') as name:
        root = pathlib.Path(name)
        started = threading.Event()
        steered = threading.Event()
        rpc = []
        rpc_lock = threading.Lock()

        class Native(socketserver.BaseRequestHandler):
            def handle(self):
                stream = self.request.makefile('rb')
                headers = {}
                while True:
                    line = stream.readline()
                    if line == b'\r\n':
                        break
                    if not line:
                        return
                    if b':' in line:
                        key, value = line.split(b':', 1)
                        headers[key.strip().lower()] = value.strip()
                accept = base64.b64encode(hashlib.sha1(
                    headers[b'sec-websocket-key'] +
                    b'258EAFA5-E914-47DA-95CA-C5AB0DC85B11').digest())
                self.request.sendall(
                    b'HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\n'
                    b'Connection: Upgrade\r\nSec-WebSocket-Accept: ' + accept + b'\r\n\r\n')

                def send(value):
                    data = json.dumps(value).encode()
                    header = b'\x81' + (bytes([len(data)]) if len(data) < 126 else
                                         b'\x7e' + struct.pack('!H', len(data)))
                    self.request.sendall(header + data)

                while True:
                    head = stream.read(2)
                    if len(head) != 2:
                        return
                    length = head[1] & 127
                    if length == 126:
                        length = struct.unpack('!H', stream.read(2))[0]
                    elif length == 127:
                        length = struct.unpack('!Q', stream.read(8))[0]
                    mask = stream.read(4) if head[1] & 128 else None
                    data = stream.read(length)
                    if mask:
                        data = bytes(byte ^ mask[i % 4] for i, byte in enumerate(data))
                    if head[0] & 15 == 8:
                        return
                    value = json.loads(data)
                    method, request_id = value.get('method'), value.get('id')
                    with rpc_lock:
                        rpc.append((method, value.get('params')))
                    if method == 'initialize':
                        send({'id': request_id, 'result': {}})
                    elif method == 'account/rateLimits/read':
                        send({'id': request_id, 'result': {}})
                    elif method in ('thread/start', 'thread/resume'):
                        if missing_binding and method == 'thread/resume':
                            send({'id': request_id, 'error': {'code': -32600,
                                  'message': 'no rollout found for thread id missing-native-context'}})
                        else:
                            send({'id': request_id, 'result': {'thread': {'id': 'synthetic-native-thread'}}})
                            send({'method': 'thread/started', 'params': {'thread': {'id': 'synthetic-native-thread'}}})
                    elif method == 'turn/start':
                        send({'id': request_id, 'result': {'turn': {'id': 'synthetic-native-turn'}}})
                        started.set()
                        if retain_commentary:
                            for index, text in enumerate(commentary if commentary_only else [*commentary, final_answer]):
                                item_id = f'agent-{index}'
                                send({'method': 'item/started', 'params': {'item': {
                                    'type': 'agentMessage', 'id': item_id}}})
                                midpoint = len(text) // 2
                                for delta in (text[:midpoint], text[midpoint:]):
                                    send({'method': 'item/agentMessage/delta', 'params': {
                                        'itemId': item_id, 'delta': delta}})
                                send({'method': 'item/completed', 'params': {'item': {
                                    'type': 'agentMessage', 'id': item_id, 'text': text,
                                    'phase': 'commentary' if index < 2 else 'final_answer'}}})
                                if index < 2:
                                    send({'method': 'item/completed', 'params': {'item': {
                                        'type': 'commandExecution', 'command': 'synthetic command',
                                        'status': 'completed', 'aggregatedOutput': 'synthetic tool progress'}}})
                        if commentary_only:
                            send({'method': 'turn/completed', 'params': {'turn': {
                                'id': 'synthetic-native-turn', 'status': 'completed'}}})
                        if steered.is_set():
                            send({'method': 'turn/completed', 'params': {'turn': {
                                'id': 'synthetic-native-turn', 'status': 'failed',
                                'error': {'message': 'synthetic later failure'}}}})
                    elif method == 'turn/steer':
                        steered.set()
                        if acknowledge_steer:
                            send({'id': request_id, 'result': {'turnId': 'synthetic-native-turn'}})
                        time.sleep(.1)
                        send({'method': 'turn/completed', 'params': {'turn': {
                            'id': 'synthetic-native-turn', 'status': 'failed',
                            'error': {'message': 'synthetic failure after accepting input'}}}})

        class NativeServer(socketserver.ThreadingTCPServer):
            allow_reuse_address = True
            daemon_threads = True

        native = NativeServer(('127.0.0.1', 0), Native)
        threading.Thread(target=native.serve_forever, daemon=True).start()
        fake = root / 'codex'
        fake.write_text(f'''#!/usr/bin/python3
import os,socket,sys,threading
if sys.argv[1:]==['login','status']:
    print('Logged in using synthetic local auth'); sys.exit(0)
if sys.argv[1:]!=['app-server','proxy']: sys.exit(7)
s=socket.create_connection(('127.0.0.1',{native.server_address[1]}),timeout=20)
def upload():
    while True:
        data=os.read(0,8192)
        if not data: break
        s.sendall(data)
threading.Thread(target=upload,daemon=True).start()
while True:
    data=s.recv(8192)
    if not data: break
    os.write(1,data)
''')
        fake.chmod(0o700)
        polls, message_id = 0, 30
        permanent, outbound, methods = {}, [], []
        lock = threading.Lock()

        def update(number, text):
            return {'update_id': number, 'message': {'message_id': number,
                    'message_thread_id': 7,
                    'from': {'id': 100, 'is_bot': False, 'first_name': 'Synthetic'},
                    'chat': {'id': 100, 'type': 'private'}, 'text': text}}

        class Telegram(http.server.BaseHTTPRequestHandler):
            def do_POST(self):
                nonlocal polls, message_id
                payload = json.loads(self.rfile.read(int(self.headers.get('Content-Length', '0'))) or b'{}')
                method = self.path.rsplit('/', 1)[-1]
                with lock:
                    methods.append(method)
                if method == 'getMe':
                    result = {'id': 123, 'is_bot': True, 'first_name': 'Fixture',
                              'username': 'FixtureBot', 'has_topics_enabled': True}
                elif method == 'getUpdates':
                    with lock:
                        polls += 1
                        poll = polls
                    if poll == 1:
                        result = [update(1, '/status' if missing_binding else 'first human input')]
                    elif poll == 2 and not retain_commentary:
                        if missing_binding:
                            with sqlite3.connect(root / 'state.sqlite') as db:
                                changed = db.execute("UPDATE sessions SET codex_thread_id='missing-native-context',force_fresh_thread=0 WHERE chat_id=100 AND thread_id=7").rowcount
                                assert changed == 1
                        else:
                            started.wait(12)
                        result = [update(2, 'second human input')]
                    else:
                        time.sleep(.15)
                        result = []
                elif method in ('sendMessage', 'editMessageText'):
                    with lock:
                        if method == 'sendMessage':
                            message_id += 1
                            target_id = message_id
                        else:
                            target_id = payload['message_id']
                        permanent[target_id] = html.unescape(payload['text'])
                        outbound.append((method, target_id, permanent[target_id]))
                    result = {'message_id': target_id, 'chat': {'id': 100, 'type': 'private'}}
                else:
                    result = True
                data = json.dumps({'ok': True, 'result': result}).encode()
                self.send_response(200)
                self.send_header('Content-Length', str(len(data)))
                self.end_headers()
                try:
                    self.wfile.write(data)
                except (BrokenPipeError, ConnectionResetError):
                    pass

            def log_message(self, *args):
                pass

        telegram = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Telegram)
        threading.Thread(target=telegram.serve_forever, daemon=True).start()
        token = root / 'bot.token'
        token.write_text('synthetic-secret')
        token.chmod(0o600)
        cfg = root / 'local.toml'
        cfg.write_text(f'''db_path="{root}/state.sqlite"
startup_admin_ids=[100]
poll_timeout_seconds=1
edit_debounce_ms=100
[telegram]
bot_token_file="{token}"
api_base="http://127.0.0.1:{telegram.server_port}"
use_message_drafts={str(drafts).lower()}
show_unfinished_messages={str(show_unfinished).lower()}
[codex]
binary="{fake}"
default_cwd="{root}"
shared_app_server=true
auto_attach_latest_history=false
import_cli_history=false
import_desktop_history=false
''')
        env = {'PATH': '/usr/bin:/bin', 'HOME': str(root),
               'CODEX_HOME': str(root / 'codex-home'), 'LANG': 'C.UTF-8', 'RUST_LOG': 'warn'}
        process = subprocess.Popen([BINARY, str(cfg)], cwd=root, env=env,
                                   stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        rows, sessions = [], []
        deadline = time.monotonic() + 18
        try:
            while time.monotonic() < deadline and process.poll() is None:
                try:
                    with sqlite3.connect(root / 'state.sqlite') as db:
                        rows = db.execute('SELECT update_id,status,turn_id FROM incoming_updates ORDER BY update_id').fetchall()
                        sessions = db.execute('SELECT codex_thread_id FROM sessions').fetchall()
                    if retain_commentary and rows == [(1, 'settled', 1)]:
                        break
                    if len(rows) == 2:
                        if missing_binding and rows[1][1] == 'undetermined':
                            break
                        if not missing_binding and rows[0][1] == 'undetermined' and rows[1][1] in ('handled', 'undetermined', 'settled'):
                            break
                except sqlite3.Error:
                    pass
                time.sleep(.1)
            process.send_signal(signal.SIGINT)
            out, err = process.communicate(timeout=5)
            with rpc_lock:
                starts = [p['input'][0]['text'] for m, p in rpc if m == 'turn/start']
                steers = [p['input'][0]['text'] for m, p in rpc if m == 'turn/steer']
                threads = [m for m, p in rpc if m in ('thread/start', 'thread/resume')]
            if retain_commentary:
                assert rows == [(1, 'settled', 1)], (mode, rows, err)
                with lock:
                    messages = [text for text in permanent.values()
                                if not text.startswith("Current Codex session:")]
                    edits = list(outbound)
                    sent_methods = list(methods)
                if not show_unfinished:
                    assert 'sendMessageDraft' not in sent_methods, (mode, sent_methods)
                    assert 'editMessageText' not in sent_methods, (mode, edits)
                assert len(messages) == (3 if commentary_only else 4), (mode, messages)
                assert messages[0] == commentary[0], (mode, messages[0])
                assert sum(text.count('x') for text in messages[1:3]) == 3800
                assert messages[1].startswith('Second progress: ')
                if not commentary_only:
                    assert messages[3] == final_answer, (mode, messages[3])
                expected_finals = 0 if commentary_only else 1
                assert sum(text == final_answer for text in messages) == expected_finals
                first_commit = next(i for i, (_, _, text) in enumerate(edits)
                                    if text == commentary[0])
                first_id = edits[first_commit][1]
                assert all(target != first_id for _, target, _ in edits[first_commit + 1:])
                with sqlite3.connect(root / 'state.sqlite') as db:
                    assert db.execute('select status,assistant_text from turns').fetchall() == [
                        ('completed', commentary[-1] if commentary_only else final_answer)]
                print(json.dumps({'scenario': mode, 'permanent_message_count': len(messages),
                                  'first_commentary_preserved': True,
                                  'long_commentary_characters': 3800,
                                  'final_answer_count': expected_finals, 'exit': process.returncode}), flush=True)
                return
            assert len(rows) == 2, (mode, rows)
            assert rows[1][1] == 'undetermined', (mode, rows)
            if missing_binding:
                assert sessions == [('missing-native-context',)], sessions
                assert threads == ['thread/resume'], threads
                assert starts == [], starts
            else:
                assert rows[0][1] == 'undetermined', rows
                assert rows[1][2] == rows[0][2] and rows[1][2] is not None, rows
                assert sessions == [('synthetic-native-thread',)], sessions
                assert starts == ['first human input'], starts
                assert steers == ['second human input'], steers
            print(json.dumps({'scenario': mode, 'updates': rows, 'session_bindings': sessions,
                              'native_thread_methods': threads,
                              'native_turn_start_inputs': starts, 'native_steer_inputs': steers,
                              'exit': process.returncode, 'log_tail': err[-400:]}), flush=True)
        finally:
            if process.poll() is None:
                process.kill()
                process.wait()
            telegram.shutdown()
            telegram.server_close()
            native.shutdown()
            native.server_close()


token_probe()
scenario('accepted_steer')
scenario('unacknowledged_steer')
scenario('missing_saved_binding')
scenario('retained_commentary_drafts')
scenario('retained_commentary_preview')
scenario('retained_commentary_only')
scenario('retained_commentary_hidden')
scenario('retained_commentary_hidden_drafts')
scenario('retained_commentary_hidden_only')
