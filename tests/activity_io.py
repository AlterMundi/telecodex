#!/usr/bin/env python3
"""Activity observer regressions using real files, SQLite and loopback Telegram HTTP."""
import importlib.util
import http.server
import json
import os
import pathlib
import sqlite3
import tempfile
import threading
import time
import unittest

spec = importlib.util.spec_from_file_location('activity', pathlib.Path(__file__).resolve().parents[1] / 'scripts/activity_indicator.py')
activity = importlib.util.module_from_spec(spec)
spec.loader.exec_module(activity)


class ActivityIO(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.root = pathlib.Path(self.temporary.name)
        self.calls = []
        self.answers = []
        owner = self

        class Handler(http.server.BaseHTTPRequestHandler):
            def do_POST(self):
                payload = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
                method = self.path.rsplit('/', 1)[-1]
                owner.calls.append((method, payload))
                default = True if method == 'deleteMessage' else {
                    'message_id': payload.get('message_id', 100 + sum(m == 'sendMessage' for m, _ in owner.calls)),
                    'chat': {'id': payload['chat_id']},
                    'message_thread_id': payload.get('message_thread_id', 7)}
                value = owner.answers.pop(0) if owner.answers else {'ok': True, 'result': default}
                if value is None:
                    self.connection.shutdown(2)  # Actual ambiguous transport failure.
                    return
                body = json.dumps(value).encode()
                self.send_response(200)
                self.send_header('Content-Length', str(len(body)))
                self.end_headers()
                self.wfile.write(body)

            def log_message(self, *_):
                pass

        self.server = http.server.ThreadingHTTPServer(('127.0.0.1', 0), Handler)
        threading.Thread(target=self.server.serve_forever, daemon=True).start()
        token = self.root / 'bot.token'
        token.write_text('synthetic-activity-token')
        token.chmod(0o600)
        self.telegram = activity.Telegram({'bot_token_file': str(token), 'api_base': f'http://127.0.0.1:{self.server.server_port}'})
        self.path = self.root / 'messages.json'
        self.publisher = activity.Publisher(self.telegram, self.path)

    def tearDown(self):
        self.server.shutdown()
        self.server.server_close()
        self.temporary.cleanup()

    def test_one_silent_message_survives_observer_restart_and_retires_itself(self):
        self.publisher.update('100:7', 100, 7, 'turn-a', 'Working', 100)
        message = self.publisher.entries['100:7']['message_id']
        restarted = activity.Publisher(self.telegram, self.path)
        restarted.update('100:7', 100, 7, 'turn-a', 'Waiting for agents', 140)
        restarted.update('100:7', 100, 7, 'turn-a', 'Too soon', 141)
        restarted.update('100:7', 100, 7, 'turn-a', None, 180)
        self.assertEqual([method for method, _ in self.calls], ['sendMessage', 'editMessageText', 'deleteMessage'])
        self.assertTrue(self.calls[0][1]['disable_notification'])
        self.assertEqual(self.calls[0][1]['message_thread_id'], 7)
        self.assertTrue(all(payload['message_id'] == message for _, payload in self.calls[1:]))
        self.assertEqual(json.loads(self.path.read_text()), {})
        receipt = json.loads((self.root / 'retirements.json').read_text())['100:7']
        self.assertEqual(receipt['outcome'], 'deleted')
        self.assertEqual(receipt['message_id'], message)

    def test_ambiguous_send_is_not_repeated_after_restart(self):
        self.answers = [None]
        self.publisher.update('100:7', 100, 7, 'turn-a', 'Working', 100)
        restarted = activity.Publisher(self.telegram, self.path)
        restarted.update('100:7', 100, 7, 'turn-a', 'Still working', 150)
        self.assertEqual(len(self.calls), 1)
        self.assertTrue(restarted.entries['100:7']['attempted'])

    def test_new_human_request_reposts_once_and_final_answer_retires_active_outer_task(self):
        reader = activity.Rollout(self.root / 'unused', 'parent')
        now = time.time()
        stamp = lambda: time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime(now - 20))
        event = lambda item: {'timestamp': stamp(), 'type': 'event_msg',
                              'payload': {'type': 'item_completed', 'item': item}}
        reader.consume(event({'type': 'UserMessage', 'id': 'request-a'}))
        line = activity.status_line({'type': 'active'}, reader, [], now)
        self.assertNotIn('updated', line)
        self.publisher.update('100:7', 100, 7, reader.request_id, line, now)
        original_message = self.publisher.entries['100:7']['message_id']
        reader.consume(event({'type': 'AgentMessage', 'phase': 'final_answer'}))
        self.assertIsNone(activity.status_line({'type': 'active'}, reader, [], now))
        # A child or a pending tool must keep the indicator alive after a final.
        self.assertIn('Waiting for 1 agent', activity.status_line({'type': 'active'}, reader, [{'type': 'active'}], now))
        reader.calls['pending'] = 'tools'
        self.assertIn('tool call', activity.status_line({'type': 'active'}, reader, [], now))
        reader.calls.clear()
        reader.consume(event({'type': 'UserMessage', 'id': 'request-b'}))
        line = activity.status_line({'type': 'active'}, reader, [], now)
        self.publisher.update('100:7', 100, 7, reader.request_id, line, now)
        self.publisher.update('100:7', 100, 7, reader.request_id, line, now + 1)
        self.assertEqual([m for m, _ in self.calls], ['sendMessage', 'deleteMessage', 'sendMessage'])
        self.assertNotEqual(original_message, self.publisher.entries['100:7']['message_id'])
        self.assertEqual(self.calls[1][1]['message_id'], original_message)
        reader.consume(event({'type': 'AgentMessage', 'phase': 'final_answer'}))
        self.publisher.update('100:7', 100, 7, reader.request_id,
                              activity.status_line({'type': 'active'}, reader, [], now), now)
        self.assertEqual(self.calls[-1][0], 'deleteMessage')
        self.assertEqual(self.publisher.entries, {})

    def test_status_only_answers_new_handled_authorized_bound_commands_once(self):
        database = sqlite3.connect(':memory:')
        database.row_factory = sqlite3.Row
        database.executescript('''CREATE TABLE incoming_updates(update_id INTEGER PRIMARY KEY,
            payload_json TEXT,status TEXT,updated_at TEXT);
            CREATE TABLE users(tg_user_id INTEGER PRIMARY KEY,allowed INTEGER);
            INSERT INTO users VALUES(1,1),(2,0);''')
        def add(identifier, text='/status', user=1, topic=7, status='handled'):
            payload = {'message': {'chat': {'id': 100}, 'message_thread_id': topic,
                'from': {'id': user}, 'message_id': identifier + 1000, 'text': text}}
            database.execute("INSERT INTO incoming_updates VALUES(?,?,?,datetime('now'))",
                             (identifier, json.dumps(payload), status))
        add(1)  # History must not trigger an unsolicited response at activation.
        path = self.root / 'status-requests.json'
        requests = activity.StatusRequests(database, self.telegram, path)
        for identifier, options in [(2, {}), (3, {'user': 2}), (4, {'topic': 99}),
                                    (5, {'text': 'secret prompt'}), (6, {'status': 'processing'})]:
            add(identifier, **options)
        reports = {'100:7': '◻️ Idle'}
        quota = lambda: 'weekly 75% available · reset in 2d 3h'
        requests.respond(reports, quota)
        self.assertEqual(len(self.calls), 1)
        payload = self.calls[0][1]
        self.assertEqual(payload['text'], '◻️ Idle · weekly 75% available · reset in 2d 3h')
        self.assertEqual(payload['reply_parameters'], {'message_id': 1002})
        self.assertEqual(payload['message_thread_id'], 7)
        self.assertNotIn('secret', path.read_text())
        restarted = activity.StatusRequests(database, self.telegram, path)
        restarted.respond(reports, quota)
        self.assertEqual(len(self.calls), 1)
        database.execute("UPDATE incoming_updates SET status='handled' WHERE update_id=6")
        self.answers = [None]
        restarted.respond(reports, quota)
        activity.StatusRequests(database, self.telegram, path).respond(reports, quota)
        self.assertEqual(len(self.calls), 2)  # Unknown delivery is never replayed.
        database.close()

    def test_disconnect_and_recovery_replace_live_claim_without_waiting_for_cadence(self):
        self.publisher.update('100:7', 100, 7, 'turn-a', 'Working', 100)
        self.publisher.update('100:7', 100, 7, 'turn-a', '⚠️ Connection unavailable', 101)
        self.publisher.update('100:7', 100, 7, 'turn-a', 'Working', 102)
        self.assertEqual([method for method, _ in self.calls], ['sendMessage', 'editMessageText', 'editMessageText'])
        self.assertIn('Connection unavailable', self.calls[1][1]['text'])
        self.assertEqual(self.calls[2][1]['text'], 'Working')

    def test_wrong_topic_receipt_never_becomes_an_edit_target(self):
        self.answers = [{'ok': True, 'result': {'message_id': 900, 'chat': {'id': 100}, 'message_thread_id': 8}}]
        self.publisher.update('100:7', 100, 7, 'turn-a', 'Working', 100)
        self.publisher.update('100:7', 100, 7, 'turn-a', 'Still working', 150)
        self.publisher.update('100:7', 100, 7, 'turn-a', None, 200)
        self.assertEqual([method for method, _ in self.calls], ['sendMessage'])

    def test_delete_failure_makes_status_inactive_without_touching_assistant_messages(self):
        self.publisher.update('100:7', 100, 7, 'turn-a', 'Working', 100)
        self.answers = [{'ok': False, 'error_code': 400, 'description': "message can't be deleted"}, {'ok': True, 'result': {}}]
        self.publisher.update('100:7', 100, 7, 'turn-a', None, 140)
        self.assertEqual(self.calls[-1][0], 'editMessageText')
        self.assertIn('Activity monitoring ended', self.calls[-1][1]['text'])
        self.assertEqual(self.calls[-1][1]['message_id'], 101)
        self.assertEqual(self.publisher.entries, {})
        self.assertEqual(json.loads((self.root / 'retirements.json').read_text())['100:7']['outcome'], 'marked_inactive')

    def test_explicit_rate_limit_delays_safe_retry(self):
        self.answers = [{'ok': False, 'error_code': 429, 'parameters': {'retry_after': 60}}]
        self.publisher.update('100:7', 100, 7, 'turn-a', 'Working', 100)
        self.publisher.update('100:7', 100, 7, 'turn-a', 'Still working', 140)
        self.assertEqual(len(self.calls), 1)
        self.publisher.update('100:7', 100, 7, 'turn-a', 'Still working', 161)
        self.assertEqual(len(self.calls), 2)
        self.assertEqual(self.publisher.entries['100:7']['message_id'], 102)

    def test_incremental_native_tools_agents_partial_records_and_binding(self):
        path = self.root / 'rollout.jsonl'
        records = [
            {'type': 'session_meta', 'payload': {'id': 'native-a'}},
            {'timestamp': '2026-10-06T12:00:00Z', 'type': 'event_msg', 'payload': {'type': 'task_started', 'turn_id': 'turn-a'}},
            {'type': 'response_item', 'payload': {'type': 'function_call', 'call_id': 'tool-1', 'name': 'secret_command', 'arguments': 'NEVER DISPLAY'}},
            {'type': 'event_msg', 'payload': {'type': 'item_completed', 'item': {'type': 'SubAgentActivity', 'kind': 'started', 'agent_thread_id': 'child-a'}}},
        ]
        path.write_text(''.join(json.dumps(record) + '\n' for record in records))
        reader = activity.Rollout(path, 'native-a')
        reader.refresh()
        self.assertEqual(len(reader.calls), 1)
        line = activity.status_line({'type': 'active'}, reader, [{'type': 'active'}], activity.timestamp('2026-10-06T12:02:00Z'))
        self.assertIn('1 tool call(s) active', line)
        self.assertIn('1 agent(s) active', line)
        self.assertNotIn('secret', line)
        self.assertNotIn('NEVER DISPLAY', line)
        partial = json.dumps({'type': 'response_item', 'payload': {'type': 'function_call_output', 'call_id': 'tool-1'}})
        with path.open('a') as stream:
            stream.write(partial[:20])
        offset = reader.offset
        reader.refresh()
        self.assertEqual(reader.offset, offset)
        with path.open('a') as stream:
            stream.write(partial[20:] + '\n')
        reader.refresh()
        self.assertEqual(reader.calls, {})
        self.assertIsNone(activity.status_line({'type': 'idle'}, reader, [{'type': 'idle'}], time.time()))
        self.assertIn('Waiting for 1 agent', activity.status_line({'type': 'idle'}, reader, [{'type': 'active'}], time.time()))
        with self.assertRaises(ValueError):
            activity.Rollout(path, 'other-native-thread').refresh()

    def test_sqlite_observation_is_readonly_and_never_reads_input_payloads(self):
        path = self.root / 'bridge.sqlite'
        with sqlite3.connect(path) as connection:
            connection.execute('CREATE TABLE sessions(id INTEGER, codex_thread_id TEXT)')
            connection.execute("INSERT INTO sessions VALUES(1,'bound-thread')")
        connection = activity.readonly(path)
        try:
            self.assertEqual(connection.execute('SELECT codex_thread_id FROM sessions').fetchone()[0], 'bound-thread')
            with self.assertRaises(sqlite3.OperationalError):
                connection.execute('DELETE FROM sessions')
        finally:
            connection.close()

    def test_observer_methods_cannot_consume_updates_or_start_turns(self):
        for method in ('getUpdates', 'getMe', 'sendDocument'):
            with self.assertRaises(ValueError):
                self.telegram.call(method, {})
        native = object.__new__(activity.Native)
        for method in ('turn/start', 'turn/steer', 'thread/resume'):
            with self.assertRaises(ValueError):
                native.rpc(method, {})
        self.assertEqual(self.calls, [])

    def test_child_status_requires_native_ancestry_not_a_rollout_reference(self):
        database = sqlite3.connect(':memory:')
        database.row_factory = sqlite3.Row
        database.execute('CREATE TABLE threads(id TEXT,source TEXT,archived INTEGER)')
        for child, parent in [('child-a', 'parent-a'), ('nested', 'child-a'), ('foreign', 'parent-b'), ('cycle', 'cycle')]:
            source = json.dumps({'subagent': {'thread_spawn': {'parent_thread_id': parent}}})
            database.execute('INSERT INTO threads VALUES(?,?,0)', (child, source))
        try:
            self.assertTrue(activity.linked_child(database, 'child-a', 'parent-a'))
            self.assertTrue(activity.linked_child(database, 'nested', 'parent-a'))
            self.assertFalse(activity.linked_child(database, 'foreign', 'parent-a'))
            self.assertFalse(activity.linked_child(database, 'cycle', 'parent-a'))
            self.assertFalse(activity.linked_child(database, 'missing', 'parent-a'))
        finally:
            database.close()

    def test_weekly_available_uses_duration_and_codex_bucket(self):
        window = lambda used, duration: {'usedPercent': used, 'windowDurationMins': duration}
        response = {'rateLimits': {'primary': window(80, 300), 'secondary': window(25, 10080)}}
        self.assertEqual(activity.weekly_available(response), 75)
        response['rateLimitsByLimitId'] = {'codex': {'primary': window(40, 10080), 'secondary': window(70, 300)},
                                         'other': {'secondary': window(90, 10080)}}
        self.assertEqual(activity.weekly_available(response), 60)
        self.assertEqual(activity.weekly_label(60), 'weekly 60% available · reset n/a')
        for value in (None, {}, {'rateLimits': {'secondary': window(40, 300)}},
                      {'rateLimits': {'secondary': window(float('nan'), 10080)}}):
            self.assertIsNone(activity.weekly_available(value))

    def test_weekly_read_is_cached_and_failure_never_keeps_an_old_percentage(self):
        class FakeNative:
            count = 0
            def rpc(self, method, params):
                self.count += 1
                self.method, self.params = method, params
                if self.count > 1:
                    raise TimeoutError()
                return {'rateLimits': {'secondary': {'usedPercent': 20, 'windowDurationMins': 10080, 'resetsAt': 200000}}}
        native, limit = FakeNative(), activity.WeeklyLimit()
        self.assertEqual(limit.read(native, 100), 80)
        self.assertEqual(limit.read(native, 159), 80)
        self.assertEqual(native.count, 1)
        self.assertEqual(limit.resets_at, 200000)
        self.assertIsNone(limit.read(native, 160))
        self.assertIsNone(limit.resets_at)
        self.assertEqual(native.method, 'account/rateLimits/read')
        self.assertIsNone(native.params)
        self.assertEqual(activity.weekly_label(limit.available), 'weekly n/a')

    def test_weekly_reset_uses_same_window_and_countdown_changes_without_refetch(self):
        window = lambda duration, reset: {'usedPercent': 25, 'windowDurationMins': duration, 'resetsAt': reset}
        response = {'rateLimits': {'primary': window(300, 100), 'secondary': window(10080, 200000)}}
        self.assertEqual(activity.weekly_snapshot(response), (75, 200000))
        response['rateLimitsByLimitId'] = {'codex': {'primary': window(10080, 300000)}}
        self.assertEqual(activity.weekly_snapshot(response), (75, 300000))
        for seconds, expected in [(183600, '2d 3h'), (7500, '2h 5m'), (180, '3m'), (59, '<1m'), (-60, '<1m')]:
            self.assertEqual(activity.weekly_label(75, 200000, 200000 - seconds),
                             f'weekly 75% available · reset in {expected}')
        for invalid in [None, True, '300000', float('nan'), float('inf'), -10]:
            response['rateLimitsByLimitId']['codex']['primary']['resetsAt'] = invalid
            self.assertEqual(activity.weekly_snapshot(response), (75, None))


if __name__ == '__main__':
    unittest.main()
