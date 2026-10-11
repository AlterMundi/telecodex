#!/usr/bin/env python3
"""Receipt-only reactions through real HTTP, SQLite and listener restarts.

All identities, tokens and payloads are synthetic. The native question probe
uses a fake App Server, not a live model. Private Telegram delivery is unproven.
"""
import collections
import http.server
import json
import pathlib
import signal
import sqlite3
import subprocess
import sys
import tempfile
import threading
import time


def reaction(identifier, old, new, *, actor=100, chat=100, target=500):
    event = dict(chat=dict(id=chat, type='private'), message_id=target,
                 date=1791756000 + identifier, old_reaction=old, new_reaction=new)
    if actor is None:
        event['actor_chat'] = dict(id=-200, type='channel', title='Synthetic actor')
    else:
        event['user'] = dict(id=actor, is_bot=False, first_name='Fixture')
    return dict(update_id=identifier, message_reaction=event)


def main(binary):
    with tempfile.TemporaryDirectory(prefix='telecodex-reactions-') as directory:
        root = pathlib.Path(directory)
        updates, calls, lock = collections.deque(), [], threading.RLock()

        class Telegram(http.server.BaseHTTPRequestHandler):
            def do_POST(self):
                payload = json.loads(self.rfile.read(int(self.headers.get('Content-Length', '0'))) or b'{}')
                method = self.path.rsplit('/', 1)[-1]
                with lock:
                    calls.append((method, payload))
                    if method == 'getMe':
                        result = dict(id=123, is_bot=True, first_name='Fixture', has_topics_enabled=True)
                    elif method == 'getUpdates':
                        result = [updates.popleft()] if updates else []
                    else:
                        result = True
                if method == 'getUpdates' and not result:
                    time.sleep(.02)
                data = json.dumps(dict(ok=True, result=result)).encode()
                self.send_response(200)
                self.send_header('Content-Length', str(len(data)))
                self.end_headers()
                try:
                    self.wfile.write(data)
                except (BrokenPipeError, ConnectionResetError):
                    pass

            def log_message(self, *_):
                pass

        server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Telegram)
        threading.Thread(target=server.serve_forever, daemon=True).start()
        token = root / 'bot.token'
        token.write_text('synthetic-reaction-token')
        token.chmod(0o600)
        native = root / 'codex'
        native.write_text(f'#!/bin/sh\ntouch "{root}/unexpected-native-call"\nexit 7\n')
        native.chmod(0o700)
        config = root / 'local.toml'
        process, database = None, None

        def write_config(enabled=None):
            capture = '' if enabled is None else f'capture_message_reactions={str(enabled).lower()}\n'
            config.write_text(f'''db_path="{root}/state.sqlite"
startup_admin_ids=[100]
poll_timeout_seconds=1
[telegram]
bot_token_file="{token}"
api_base="http://127.0.0.1:{server.server_port}"
{capture}[codex]
binary="{native}"
default_cwd="{root}"
auto_attach_latest_history=false
import_cli_history=false
import_desktop_history=false
''')

        def start():
            return subprocess.Popen([binary, str(config)], cwd=root, stdout=subprocess.PIPE,
                                    stderr=subprocess.PIPE, text=True,
                                    env=dict(HOME=str(root), CODEX_HOME=str(root / 'codex-home'),
                                             PATH='/usr/bin:/bin', LANG='C.UTF-8', RUST_LOG='warn'))

        def stop():
            nonlocal process
            if process is not None:
                process.send_signal(signal.SIGINT)
                output, error = process.communicate(timeout=8)
                assert process.returncode == 0, output + error
                assert 'synthetic-reaction-token' not in output + error
                process = None

        def wait(predicate):
            deadline = time.monotonic() + 12
            while time.monotonic() < deadline:
                with lock:
                    if predicate():
                        return
                assert process.poll() is None, 'bridge exited unexpectedly'
                time.sleep(.02)
            raise AssertionError('timed out waiting for reaction capture')

        def polls(after=0):
            return [payload for method, payload in calls[after:] if method == 'getUpdates']

        def enqueue(update):
            with lock:
                updates.append(update)

        def saved(identifier):
            return database.execute('SELECT payload_json,status,turn_id FROM incoming_updates WHERE update_id=?',
                                    (identifier,)).fetchone()

        try:
            write_config()
            process = start()
            wait(lambda: polls())
            assert polls()[0]['allowed_updates'] == ['message', 'callback_query']
            database = sqlite3.connect(root / 'state.sqlite')
            # An already queued reaction may arrive even without a subscription.
            queued = reaction(1, [], [dict(type='emoji', emoji='👍')])
            enqueue(queued)
            wait(lambda: saved(1) is not None and saved(1)[1] == 'handled')
            assert json.loads(saved(1)[0])['message_reaction'] == queued['message_reaction']
            stop()

            write_config(True)
            after = len(calls)
            process = start()
            wait(lambda: polls(after))
            assert polls(after)[0]['allowed_updates'] == ['message', 'callback_query', 'message_reaction']
            assert polls(after)[0]['offset'] == 2
            emoji = dict(type='emoji', emoji='💚')
            custom = dict(type='custom_emoji', custom_emoji_id='synthetic-custom-id')
            future = dict(type='future-kind', nested=dict(value='preserve me'))
            events = [reaction(2, [], [emoji]), reaction(3, [emoji], [custom]),
                      reaction(4, [custom], []), reaction(5, [], [emoji], actor=999),
                      reaction(6, [], [emoji], actor=None),
                      reaction(7, [], [future], chat=200, target=999)]
            for event in events:
                enqueue(event)
            wait(lambda: saved(7) is not None and saved(7)[1] == 'handled')
            for event in events:
                row = saved(event['update_id'])
                assert row[1:] == ('handled', None)
                assert json.loads(row[0])['message_reaction'] == event['message_reaction']
            # Duplicate admission must not replay an event or alter the journal.
            enqueue(events[-1])
            wait(lambda: not updates)
            wait(lambda: any(p['offset'] == 8 for p in polls(after)))
            assert database.execute('SELECT COUNT(*) FROM incoming_updates').fetchone() == (7,)
            assert database.execute('SELECT COUNT(*) FROM sessions').fetchone() == (0,)
            assert database.execute('SELECT COUNT(*) FROM turns').fetchone() == (0,)
            assert not (root / 'unexpected-native-call').exists()
            assert not any(method not in ('getMe', 'getUpdates', 'setMyCommands') for method, _ in calls)
            stop()

            write_config(False)
            after = len(calls)
            process = start()
            wait(lambda: polls(after))
            assert polls(after)[0]['allowed_updates'] == ['message', 'callback_query']
            assert polls(after)[0]['offset'] == 8
            enqueue(events[-1])
            enqueue(reaction(8, [emoji], []))
            wait(lambda: saved(8) is not None and saved(8)[1] == 'handled')
            assert database.execute('SELECT COUNT(*) FROM incoming_updates').fetchone() == (8,)
            assert database.execute('SELECT COUNT(*) FROM sessions').fetchone() == (0,)
            assert not (root / 'unexpected-native-call').exists()
            print('PASS: opt-in subscription, complete payloads, additions/replacements/removals, '
                  'unknown actors/targets, deduplication and restart offsets; zero native calls/replies')
        finally:
            stop()
            if database is not None:
                database.close()
            server.shutdown()
            server.server_close()


def armed_question_probe(binary):
    from questions_io import Journey

    journey = Journey(binary, 'reactions')
    try:
        raw = journey.config.read_text().replace('[telegram]\n',
                                                 '[telegram]\ncapture_message_reactions=true\n')
        journey.config.write_text(raw)
        journey.start()
        journey.admit('/plan ask a question')
        journey.wait(lambda: journey.controls(1))
        journey.admit(callback=journey.button(journey.controls(1), ':0'))
        journey.wait(lambda: journey.controls(2))
        journey.admit(callback=journey.button(journey.controls(2), ':text'))
        journey.wait(lambda: any('Send your answer as text' in item[1]['text'] for item in journey.messages))
        binding = journey.row('SELECT codex_thread_id FROM sessions')
        with journey.lock:
            journey.sequence += 1
            identifier = journey.sequence
            journey.updates.append(reaction(identifier, [], [dict(type='emoji', emoji='👍')]))
        journey.wait(lambda: journey.row('SELECT status,turn_id FROM incoming_updates WHERE update_id=?',
                                         (identifier,)) == [('handled', None)])
        assert not journey.answers, 'reaction answered an armed native question'
        assert not any(item.get('method') == 'turn/steer' for item in journey.rpc)
        assert journey.row('SELECT codex_thread_id FROM sessions') == binding
        journey.admit('Exact free text')
        journey.wait(lambda: len(journey.answers) == 1)
        assert journey.answers[0]['result']['answers']['notes']['answers'] == ['Exact free text']
        journey.wait(lambda: journey.row('SELECT busy FROM sessions') == [(0,)])
        assert sum(item.get('method') == 'turn/start' for item in journey.rpc) == 1
        print('PASS: reaction capture preserves native binding and armed question; subsequent text answers normally')
    finally:
        journey.close()


if __name__ == '__main__':
    candidate = str(pathlib.Path(sys.argv[1]).resolve())
    main(candidate)
    armed_question_probe(candidate)
