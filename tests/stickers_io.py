#!/usr/bin/env python3
"""Sticker ingress through real HTTP, SQLite and native App Server pipes.

All identities, tokens, replies and model responses are synthetic. The image is
a 2x2 lossless WebP with transparent and translucent pixels, made with libwebp.
No live Telegram, Matrix or model call is made.
"""
import base64
import collections
import hashlib
import http.server
import json
import os
import pathlib
import signal
import sqlite3
import subprocess
import sys
import tempfile
import threading
import time


IMAGE = base64.b64decode('UklGRj4AAABXRUJQVlA4TDEAAAAvAUAAEB+gkG0EGA/ssF6BQBLO/hRjKWQbARrMff5khyAgKLpuuQDUP/ELN2CI6H8EAA==')
IMAGE_HASH = hashlib.sha256(IMAGE).hexdigest()


def question_capture_probe(binary):
    """A quoted sticker must not become an armed native question's text answer."""
    from questions_io import Journey

    journey = Journey(binary, 'stickers')
    try:
        journey.start()
        journey.admit('/plan ask a question')
        journey.wait(lambda: journey.controls(1))
        journey.admit(callback=journey.button(journey.controls(1), ':0'))
        journey.wait(lambda: journey.controls(2))
        journey.admit(callback=journey.button(journey.controls(2), ':text'))
        journey.wait(lambda: any('Send your answer as text' in message[1]['text'] for message in journey.messages))
        with journey.lock:
            journey.sequence += 1
            journey.updates.append(dict(update_id=journey.sequence, message=dict(
                message_id=journey.sequence, message_thread_id=7, chat=dict(id=100, type='private'),
                text='Look at this sticker.', reply_to_message=dict(message_id=5000, sticker=dict(
                    file_id='motion', file_unique_id='unique-motion', type='regular', width=512,
                    height=512, is_animated=True, is_video=False)),
                **{'from': dict(id=100, is_bot=False, first_name='Fixture')})))
        journey.wait(lambda: any('no usable still image' in message[1]['text'] for message in journey.messages))
        assert not journey.answers, 'sticker reply was consumed as the armed text answer'
        journey.admit('Exact free text')
        journey.wait(lambda: len(journey.answers) == 1)
        assert journey.answers[0]['result']['answers']['notes']['answers'] == ['Exact free text']
        journey.wait(lambda: journey.row('SELECT busy FROM sessions') == [(0,)])
        assert sum(request.get('method') == 'turn/start' for request in journey.rpc) == 1
        print('PASS: quoted sticker preserves armed native question; later text answers normally')
    finally:
        journey.close()


def main(binary):
    """Qualify delivery, still-preview provenance, isolation and failure behavior."""
    with tempfile.TemporaryDirectory(prefix='telecodex-stickers-') as directory:
        root = pathlib.Path(directory)
        updates, calls, downloads = collections.deque(), [], []
        lock = threading.RLock()
        sequence, message_id = 0, 500
        files = {'static': 'stickers/full.webp', 'preview': 'thumbnails/preview.webp',
                 'broken': 'stickers/broken.webp', 'interrupted': 'stickers/interrupted.webp'}

        class Telegram(http.server.BaseHTTPRequestHandler):
            def respond(self, response):
                """Return a bounded JSON Bot API fixture response."""
                body = json.dumps(response).encode()
                self.send_response(200)
                self.send_header('Content-Length', str(len(body)))
                self.end_headers()
                try:
                    self.wfile.write(body)
                except (BrokenPipeError, ConnectionResetError):
                    pass

            def do_POST(self):
                """Record requests and emulate updates, getFile and topic-local messages."""
                nonlocal message_id
                payload = json.loads(self.rfile.read(int(self.headers.get('Content-Length', '0'))) or b'{}')
                method = self.path.rsplit('/', 1)[-1]
                with lock:
                    calls.append((method, payload))
                    if method == 'getMe':
                        result = dict(id=123, is_bot=True, first_name='Fixture', has_topics_enabled=True)
                    elif method == 'getUpdates':
                        result = [updates.popleft()] if updates else []
                    elif method == 'getFile':
                        if payload['file_id'] == 'broken':
                            self.respond(dict(ok=False, description='synthetic-sticker-token download rejected'))
                            return
                        result = dict(file_id=payload['file_id'], file_path=files[payload['file_id']],
                                      file_size=len(IMAGE))
                    elif method in ('sendMessage', 'editMessageText'):
                        message_id += 1
                        result = dict(message_id=payload.get('message_id', message_id),
                                      message_thread_id=payload.get('message_thread_id'),
                                      chat=dict(id=payload['chat_id'], type='private'), text=payload['text'],
                                      **{'from': dict(id=123, is_bot=True, first_name='Fixture')})
                    else:
                        result = True
                if method == 'getUpdates' and not result:
                    time.sleep(.02)
                self.respond(dict(ok=True, result=result))

            def do_GET(self):
                """Serve actual image bytes or an interrupted body for atomic-copy checks."""
                with lock:
                    downloads.append(self.path.rsplit('/', 1)[-1])
                self.send_response(200)
                self.send_header('Content-Length', str(len(IMAGE)))
                self.end_headers()
                if self.path.endswith('interrupted.webp'):
                    self.wfile.write(IMAGE[:12])
                    self.close_connection = True
                else:
                    self.wfile.write(IMAGE)

            def log_message(self, *_):
                """Keep fixture tokens in HTTP paths out of test output."""
                pass

        server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Telegram)
        threading.Thread(target=server.serve_forever, daemon=True).start()
        native = root / 'codex'
        native.write_text('''#!/usr/bin/python3
import hashlib,json,pathlib,sys,time,uuid
root=pathlib.Path(__file__).parent
if sys.argv[1:]==['login','status']:
    print('Logged in using synthetic local auth');sys.exit(0)
for line in sys.stdin:
    request=json.loads(line)
    with (root/'rpc.jsonl').open('a') as stream:stream.write(json.dumps(request)+'\\n')
    if 'id' not in request:continue
    method=request['method'];params=request.get('params',{})
    if method in ('initialize','account/rateLimits/read'):result={}
    elif method=='thread/start':
        result={'thread':{'id':'sticker-thread-'+str(uuid.uuid4())}}
    elif method in ('thread/resume','thread/read'):
        result={'thread':{'id':params['threadId'],'turns':[]}}
    elif method=='turn/start':
        prompt='\\n'.join(item['text'] for item in params['input'] if item['type']=='text')
        paths=[pathlib.Path(item['path']) for item in params['input'] if item['type']=='localImage']
        receipt={'thread':params['threadId'],'prompt':prompt,
            'paths':[str(path) for path in paths],
            'hashes':[hashlib.sha256(path.read_bytes()).hexdigest() for path in paths]}
        with (root/'receipts.jsonl').open('a') as stream:stream.write(json.dumps(receipt)+'\\n')
        turn='sticker-turn-'+str(time.time_ns())
        print(json.dumps({'id':request['id'],'result':{'turn':{'id':turn}}}),flush=True)
        if 'Hold this turn for a sticker reply.' in prompt:
            deadline=time.monotonic()+20
            while not (root/'release').exists() and time.monotonic()<deadline:time.sleep(.02)
        print(json.dumps({'method':'item/completed','params':{'threadId':params['threadId'],
            'turnId':turn,'item':{'type':'agentMessage','id':'answer',
            'phase':'final_answer','text':'Synthetic sticker input received.'}}}),flush=True)
        print(json.dumps({'method':'turn/completed','params':{'threadId':params['threadId'],
            'turn':{'id':turn,'status':'completed'}}}),flush=True)
        continue
    else:raise RuntimeError('unexpected RPC '+method)
    print(json.dumps({'id':request['id'],'result':result}),flush=True)
''')
        native.chmod(0o700)
        token = root / 'bot.token'
        token.write_text('synthetic-sticker-token')
        token.chmod(0o600)
        config = root / 'local.toml'
        config.write_text(f'''db_path="{root}/state.sqlite"
startup_admin_ids=[100,101]
poll_timeout_seconds=1
[telegram]
bot_token_file="{token}"
api_base="http://127.0.0.1:{server.server_port}"
lifecycle_notifications=false
[codex]
binary="{native}"
default_cwd="{root}"
auto_attach_latest_history=false
import_cli_history=false
import_desktop_history=false
''')
        process, db = None, None

        def start():
            """Start the candidate using an isolated workspace and synthetic native harness."""
            return subprocess.Popen([binary, str(config)], cwd=root, stdout=subprocess.PIPE,
                                    stderr=subprocess.PIPE, text=True,
                                    env=dict(os.environ, HOME=str(root), CODEX_HOME=str(root / 'codex-home'),
                                             RUST_LOG='warn'))

        def stop():
            """Reap the bridge even if graceful shutdown times out."""
            process.send_signal(signal.SIGINT)
            try:
                _, error = process.communicate(timeout=8)
            except subprocess.TimeoutExpired:
                process.kill()
                _, error = process.communicate(timeout=8)
                assert 'synthetic-sticker-token' not in error
                raise
            assert 'synthetic-sticker-token' not in error

        def wait(predicate):
            """Bound fixture waits and report a bridge exit immediately."""
            deadline = time.monotonic() + 20
            while time.monotonic() < deadline:
                if predicate():
                    return
                assert process.poll() is None, 'bridge exited unexpectedly'
                time.sleep(.02)
            raise AssertionError('sticker journey timed out')

        def receipts():
            """Read images and prompts captured at the native turn/start boundary."""
            path = root / 'receipts.jsonl'
            return [json.loads(line) for line in path.read_text().splitlines()] if path.exists() else []

        def sticker(*, animated=False, video=False, thumbnail=True, file='static'):
            """Build an incoming sticker with format flags and optional still preview."""
            value = dict(file_id=file, file_unique_id='unique-' + file, type='regular',
                         width=512, height=512, is_animated=animated, is_video=video,
                         emoji='💛', set_name='SyntheticStickers', file_size=len(IMAGE))
            if thumbnail:
                value['thumbnail'] = dict(file_id='preview', width=128, height=128)
            return value

        def send(*, direct=None, quoted=None, text=None, topic=7, user=100, chat=100):
            """Queue human input with explicit requester, chat and topic provenance."""
            nonlocal sequence
            sequence += 1
            message = dict(message_id=sequence, message_thread_id=topic,
                           chat=dict(id=chat, type='private'),
                           **{'from': dict(id=user, is_bot=False, first_name='Fixture')})
            if direct is not None:
                message['sticker'] = direct
            if quoted is not None:
                message['reply_to_message'] = dict(message_id=5000, sticker=quoted)
            if text is not None:
                message['text'] = text
            update = dict(update_id=sequence, message=message)
            with lock:
                updates.append(update)
            return update

        def settled(update):
            """Wait for the journal to leave all dispatch/in-flight states."""
            def done():
                row = db.execute('SELECT status FROM incoming_updates WHERE update_id=?',
                                 (update['update_id'],)).fetchone()
                return row and row[0] not in ('received', 'processing', 'queued', 'started', 'steering', 'answering')
            wait(done)

        def delivered(update, count):
            """Require native receipt and completed journal before inspecting image bytes."""
            wait(lambda: len(receipts()) == count)
            settled(update)
            receipt = receipts()[-1]
            assert receipt['hashes'] and all(value == IMAGE_HASH for value in receipt['hashes'])
            assert all(pathlib.Path(path).suffix == '.webp' for path in receipt['paths'])
            return receipt

        def notice(update, fragment):
            """Require failure feedback in the originating topic and no native image turn."""
            before = len(receipts())
            after = len(calls)
            with lock:
                updates.append(update)
            wait(lambda: any(method == 'sendMessage' and fragment in payload['text']
                             and payload.get('message_thread_id') == update['message']['message_thread_id']
                             and payload['chat_id'] == update['message']['chat']['id']
                             for method, payload in calls[after:]))
            settled(update)
            assert len(receipts()) == before

        try:
            process = start()
            wait(lambda: any(method == 'setMyCommands' for method, _ in calls))
            db = sqlite3.connect(root / 'state.sqlite')
            direct = send(direct=sticker())
            first = delivered(direct, 1)
            assert 'The human sent a Telegram sticker.' in first['prompt']
            assert 'full static sticker image' in first['prompt']
            assert '💛' in first['prompt'] and 'SyntheticStickers' in first['prompt']
            journal = json.loads(db.execute('SELECT payload_json FROM incoming_updates WHERE update_id=1').fetchone()[0])
            assert journal['message']['sticker']['file_unique_id'] == 'unique-static'
            with lock:
                updates.append(direct)
            wait(lambda: not updates)
            time.sleep(.2)
            assert len(receipts()) == 1, 'duplicate update started another turn'
            quoted = send(quoted=sticker(), text='What does the quoted sticker mean?')
            second = delivered(quoted, 2)
            assert second['thread'] == first['thread']
            assert 'Telegram quoted-sticker (static)' in second['prompt']
            journal = json.loads(db.execute('SELECT payload_json FROM incoming_updates WHERE update_id=?',
                                           (quoted['update_id'],)).fetchone()[0])
            assert journal['message']['reply_to_message']['sticker']['set_name'] == 'SyntheticStickers'
            both = send(direct=sticker(), quoted=sticker(), topic=8)
            third = delivered(both, 3)
            assert len(third['paths']) == 2 and third['paths'][0] != third['paths'][1]
            assert third['thread'] != first['thread'], 'another topic reused the native binding'
            other = send(direct=sticker(), user=101, chat=200)
            fourth = delivered(other, 4)
            assert fourth['thread'] not in (first['thread'], third['thread'])
            assert db.execute('SELECT t.from_user_id,s.chat_id,s.thread_id FROM turns t '
                              'JOIN sessions s ON s.id=t.session_id ORDER BY t.id DESC LIMIT 1').fetchone() == (101, 200, 7)
            for number, flags in enumerate([dict(animated=True), dict(video=True)], 5):
                update = send(direct=sticker(file='motion-must-not-download', **flags))
                receipt = delivered(update, number)
                assert 'Only a still thumbnail' in receipt['prompt']
                assert 'was not rendered' in receipt['prompt']
                assert any(method == 'sendMessage' and 'still thumbnails only' in payload['text']
                           and payload.get('message_thread_id') == 7 for method, payload in calls)
            assert not any(method == 'getFile' and payload['file_id'] == 'motion-must-not-download'
                           for method, payload in calls)
            preview_reply = send(quoted=sticker(animated=True), text='Explain this animated reply.')
            receipt = delivered(preview_reply, 7)
            assert 'Telegram quoted-sticker (animated): Only a still thumbnail' in receipt['prompt']
            for value, quoted_value, fragment in [
                    (sticker(animated=True, thumbnail=False), None, 'no usable still image'),
                    (None, sticker(video=True, thumbnail=False), 'no usable still image'),
                    (sticker(), sticker(video=True, thumbnail=False), 'no usable still image'),
                    (sticker(), sticker(file='broken'), 'could not download'),
                    (sticker(file='broken'), None, 'could not download'),
                    (sticker(file='interrupted'), None, 'could not download')]:
                # Queue inside notice() so the failure cannot race its observation window.
                sequence += 1
                update = dict(update_id=sequence, message=dict(message_id=sequence,
                    message_thread_id=7, chat=dict(id=100, type='private'), text='See this sticker.',
                    **{'from': dict(id=100, is_bot=False, first_name='Fixture')}))
                if value is not None:
                    update['message']['sticker'] = value
                if quoted_value is not None:
                    update['message']['reply_to_message'] = dict(message_id=5000, sticker=quoted_value)
                notice(update, fragment)
                assert db.execute('SELECT codex_thread_id FROM sessions WHERE chat_id=100 AND thread_id=7').fetchone()[0] == first['thread']
            wait(lambda: not any(path.is_file() for path in (root / '.telecodex/inbox').rglob('*')))
            download_count = len(downloads)
            denied = send(direct=sticker(), user=999)
            settled(denied)
            assert len(receipts()) == 7
            assert len(downloads) == download_count, 'unallowed user caused an attachment download'
            hold = send(text='Hold this turn for a sticker reply.')
            wait(lambda: len(receipts()) == 8)
            followup = send(quoted=sticker(), text='Explain this quoted sticker.')
            wait(lambda: db.execute('SELECT status FROM incoming_updates WHERE update_id=?',
                                    (followup['update_id'],)).fetchone() == ('queued',))
            (root / 'release').touch()
            settled(hold)
            delivered(followup, 9)
            rpc = [json.loads(line) for line in (root / 'rpc.jsonl').read_text().splitlines()]
            assert not any(request.get('method') == 'turn/steer' for request in rpc)
            stop()
            process = start()
            wait(lambda: sum(method == 'setMyCommands' for method, _ in calls) == 2)
            time.sleep(.2)
            assert len(receipts()) == 9, 'restart replayed sticker input'
            assert db.execute('SELECT COUNT(*) FROM incoming_updates').fetchone()[0] == sequence
            assert db.execute('SELECT COUNT(*) FROM turns').fetchone()[0] == 9
            print('PASS: full/quoted static WebP, unchanged transparent image bytes at native input, '
                  'two images, user/chat/topic isolation, metadata journal, still-preview labels, '
                  'missing-preview/download/interruption feedback, binding preservation, '
                  'duplicate/restart safety and queued media reply; no live model calls')
        finally:
            (root / 'release').touch()
            try:
                if process and process.poll() is None:
                    stop()
            finally:
                if db:
                    db.close()
                server.shutdown()
                server.server_close()


if __name__ == '__main__':
    binary = str(pathlib.Path(sys.argv[1]).resolve())
    main(binary)
    question_capture_probe(binary)
