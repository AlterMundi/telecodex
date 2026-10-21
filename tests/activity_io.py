#!/usr/bin/env python3
"""Activity observer regressions using real files, SQLite and loopback Telegram HTTP."""
import importlib.util
import http.server
import json
import os
import pathlib
import sqlite3
import subprocess
import tempfile
import threading
import time
import unittest
from unittest import mock

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
                default = True if method in ('deleteMessage', 'editForumTopic') else {
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

    def pending_database(self):
        database=sqlite3.connect(self.root/'pending.sqlite');database.row_factory=sqlite3.Row
        database.executescript("""CREATE TABLE users(tg_user_id INTEGER,allowed INTEGER);
            INSERT INTO users VALUES(100,1),(101,1),(102,0);
            CREATE TABLE sessions(id INTEGER,chat_id INTEGER,thread_id INTEGER,codex_thread_id TEXT,creator_user_id INTEGER);
            INSERT INTO sessions VALUES(1,100,7,'origin',100),(2,100,8,'other',100);
            CREATE TABLE app_instance_lock(key TEXT,instance_id TEXT,heartbeat_at TEXT);
            CREATE TABLE human_interactions(session_id INTEGER,requester INTEGER,native_thread TEXT,kind TEXT,state TEXT,instance_id TEXT);
            CREATE TABLE incoming_updates(update_id INTEGER,status TEXT,payload_json TEXT,updated_at TEXT);
            INSERT INTO human_interactions VALUES(1,100,'origin','native','pending','bridge'),
                (1,100,'origin','task','pending','old'),(1,101,'origin','task','pending','old'),
                (1,102,'origin','task','pending','old'),(2,100,'other','task','pending','old');""")
        database.execute("INSERT INTO app_instance_lock VALUES('main','bridge',datetime('now'))")
        database.commit();return database

    def test_pending_metadata_is_scoped_and_native_channels_require_a_fresh_owner(self):
        database=self.pending_database()
        try:
            pending=activity.pending_requests(database,100,7,100)
            self.assertEqual(pending['pending'],2)
            self.assertEqual(activity.pending_requests(database,100,7,101)['pending'],1)
            self.assertIsNone(activity.pending_requests(database,100,7,102))
            self.assertEqual(activity.pending_requests(database,100,8,100)['pending'],1)
            self.assertIsNone(activity.pending_requests(database,101,7,100))
            database.execute("UPDATE app_instance_lock SET heartbeat_at=datetime('now','-300 seconds')")
            self.assertEqual(activity.pending_requests(database,100,7,100)['pending'],1)
            database.execute("UPDATE human_interactions SET state='unknown' WHERE kind='native'")
            self.assertIn('answer outcome unconfirmed',activity.pending_requests(database,100,7,100)['line'])
            database.execute("UPDATE sessions SET codex_thread_id='replacement' WHERE id=1")
            self.assertEqual(activity.pending_requests(database,100,7,100)['pending'],0)
            self.assertIn('source session changed',activity.pending_requests(database,100,7,100)['line'])
        finally:database.close()

    def test_pending_card_reopens_and_removes_its_button_without_claiming_work(self):
        pending={'pending':1,'line':'📋 Human requests · 1 pending · /pending'}
        text=activity.with_pending(None,pending)
        self.assertNotIn('Working',text)
        self.publisher.update('100:7',100,7,'turn-a',text,100,markup=activity.pending_markup(pending))
        self.assertEqual(self.calls[-1][1]['reply_markup']['inline_keyboard'][0][0]['callback_data'],'qpending')
        self.publisher.update('100:7',100,7,'turn-a','Working',140)
        self.assertEqual(self.calls[-1][1]['reply_markup'],{'inline_keyboard':[]})
        self.publisher.update('100:7',100,7,'turn-a',None,180)
        self.assertEqual(self.calls[-1][0],'deleteMessage')

    def test_status_reads_requesting_humans_pending_metadata_and_supports_old_schemas(self):
        database=self.pending_database()
        try:
            requests=activity.StatusRequests(database,self.telegram,self.root/'status-pending.json')
            update={'message':{'chat':{'id':100},'message_thread_id':7,'message_id':20,'from':{'id':101},'text':'/status'}}
            database.execute("INSERT INTO incoming_updates VALUES(1,'handled',?,datetime('now'))",(json.dumps(update),));database.commit()
            requests.respond({'100:7':'◻️ Idle'},lambda:'weekly 78%',{'100:7':'origin'})
            self.assertIn('1 pending',self.calls[-1][1]['text'])
            self.assertNotIn('2 pending',self.calls[-1][1]['text'])
            self.assertEqual(self.calls[-1][1]['reply_markup']['inline_keyboard'][0][0]['callback_data'],'qpending')
            database.execute('DROP TABLE human_interactions')
            self.assertIsNone(activity.pending_requests(database,100,7,100))
        finally:database.close()

    def test_explicit_update_receipt_explains_wait_deadline_and_liveness(self):
        path = self.root / 'update.json'
        record = {'activation':'waiting_for_idle', 'topic_key':'100:7',
                  'waiting_for':'conversations_idle', 'worker_pid':os.getpid(),
                  'busy_conversations':2, 'deadline_at':3700, 'at':100}
        activity.atomic_json(path, record)
        line = activity.pending_update_line(path, '100:7', 100)
        self.assertIn('waiting for 2 conversation(s) to finish', line)
        self.assertIn('deadline in 1h 0m', line)
        self.assertIsNone(activity.pending_update_line(path, '100:8', 100))
        report, active, automatic = activity.combine_update_status('◻️ Idle', None, None, line)
        self.assertEqual((report, active, automatic), (line, line, line))
        self.assertIn('heartbeat stale', activity.pending_update_line(path, '100:7', 130))
        worker = subprocess.Popen(['/usr/bin/true'])
        worker.wait()
        record.update(worker_pid=worker.pid)
        activity.atomic_json(path, record)
        self.assertIn('process unavailable', activity.pending_update_line(path, '100:7', 100))
        record.update(worker_pid=os.getpid())
        record.update(at=3701)
        activity.atomic_json(path, record)
        self.assertIn('deadline elapsed', activity.pending_update_line(path, '100:7', 3701))
        record.update(activation='active')
        activity.atomic_json(path, record)
        self.assertIsNone(activity.pending_update_line(path, '100:7', 3701))
        record.update(activation='waiting_for_idle', at=100)
        activity.atomic_json(path, record)
        path.chmod(0o644)
        self.assertIsNone(activity.pending_update_line(path, '100:7', 100))

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

    def test_command_activity_is_live_scoped_and_received_status_does_not_dispatch(self):
        database = sqlite3.connect(':memory:')
        database.row_factory = sqlite3.Row
        database.executescript("""CREATE TABLE bot_state(key TEXT PRIMARY KEY,value TEXT);
            CREATE TABLE app_instance_lock(key TEXT,instance_id TEXT,heartbeat_at TEXT);
            CREATE TABLE incoming_updates(update_id INTEGER PRIMARY KEY,payload_json TEXT,status TEXT,updated_at TEXT);
            CREATE TABLE users(tg_user_id INTEGER PRIMARY KEY,allowed INTEGER);
            CREATE TABLE sessions(chat_id INTEGER,thread_id INTEGER,codex_thread_id TEXT,creator_user_id INTEGER);
            INSERT INTO users VALUES(1,1);
            INSERT INTO sessions VALUES(100,7,'parent',1);""")
        now=time.time()
        stamp=time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime(now))
        database.execute("INSERT INTO app_instance_lock VALUES('main','instance',?)", (stamp,))
        data={'instance_id':'instance','operation':'operation','phase':'synthesizing_handoff','started_at':now-40}
        database.execute("INSERT INTO bot_state VALUES('command_activity:100:7',?)", (json.dumps(data),))
        command=activity.command_activity(database,100,7,now)
        self.assertIn('Synthesizing focused handoff',command['line'])
        self.assertIsNone(activity.command_activity(database,100,8,now))
        self.assertIsNone(activity.command_activity(database,100,7,now+70))
        requests=activity.StatusRequests(database,self.telegram,self.root/'command-requests.json')
        payload={'message':{'chat':{'id':100},'message_thread_id':7,'from':{'id':1},'message_id':1001,'text':'/status'}}
        database.execute("INSERT INTO incoming_updates VALUES(1,?,'received',datetime('now'))",(json.dumps(payload),))
        contexts={'100:7':{'command':'operation','active':True,'turn':'operation'}}
        requests.respond({'100:7':command['line']},lambda:'weekly 75% available',{'100:7':'parent'},contexts)
        self.assertIn('Synthesizing focused handoff',self.calls[-1][1]['text'])
        self.assertEqual(database.execute('SELECT status FROM incoming_updates').fetchone()[0],'received')
        requests.respond({'100:7':command['line']},lambda:'weekly',{'100:7':'parent'},contexts)
        self.assertEqual(len(self.calls),1)
        database.execute("UPDATE app_instance_lock SET instance_id='new-instance'")
        self.assertIsNone(activity.command_activity(database,100,7,now))
        # Completion removes the lease; automatic cards use existing cleanup.
        self.publisher.update('100:7',100,7,'operation',command['line'],now)
        database.execute('DELETE FROM bot_state')
        self.publisher.update('100:7',100,7,'operation',None,now+1)
        self.assertEqual(self.calls[-1][0],'deleteMessage')
        database.close()

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

    def test_unchanged_cards_survive_restart_but_changed_buttons_are_published(self):
        self.publisher.update('100:7', 100, 7, 'a', 'Waiting for your answer', 100)
        self.publisher = activity.Publisher(self.telegram, self.path)
        self.publisher.update('100:7', 100, 7, 'a', 'Waiting for your answer', 140)
        self.publisher.update('100:7', 100, 7, 'a', 'Waiting for your answer', 180, requested=True)
        self.assertEqual(len(self.calls), 1)
        markup = {'inline_keyboard': [[{'text': 'Review', 'callback_data': 'test'}]]}
        self.publisher.update('100:7', 100, 7, 'a', 'Waiting for your answer', 220, markup=markup)
        self.assertEqual([m for m, _ in self.calls], ['sendMessage', 'editMessageText'])
        self.assertEqual(self.calls[-1][1]['reply_markup'], markup)
        self.publisher.update('100:7', 100, 7, 'a', 'Waiting for your answer', 260)
        self.assertEqual(self.calls[-1][1]['reply_markup'], {'inline_keyboard': []})
        self.assertNotIn('Waiting for your answer', self.path.read_text())

    def test_rejected_edit_is_not_hammered_and_cleanup_respects_retry_after(self):
        self.publisher.update('100:7', 100, 7, 'a', 'Working', 100)
        self.answers = [{'ok': False, 'error_code': 400, 'description': 'message is not modified'}]
        self.publisher.entries['100:7'].pop('accepted_payload')  # Pre-upgrade receipt.
        self.publisher.update('100:7', 100, 7, 'a', 'Working', 140)
        self.publisher.update('100:7', 100, 7, 'a', 'Working', 180)
        self.assertEqual(len(self.calls), 2)
        self.answers = [{'ok': False, 'error_code': 400, 'description': 'invalid presentation'}]
        self.publisher.update('100:7', 100, 7, 'a', 'New state', 220)
        self.publisher = activity.Publisher(self.telegram, self.path)
        self.publisher.update('100:7', 100, 7, 'a', 'New state', 260)
        self.assertEqual(len(self.calls), 3)
        self.answers = [{'ok': False, 'error_code': 429, 'parameters': {'retry_after': 3600}}]
        self.publisher.update('100:7', 100, 7, 'a', None, time.time())
        self.publisher.update('100:7', 100, 7, 'b', 'New turn', time.time()+60)
        self.assertEqual(self.calls[-1][0], 'deleteMessage')
        self.assertEqual(len(self.calls), 4)  # No edit fallback or replacement send.

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

    def test_native_goal_read_is_scoped_and_never_changes_idle_to_working(self):
        native = object.__new__(activity.Native)
        calls = []
        goal = {'threadId': 'native-a', 'status': 'paused', 'objective': '  Shared resources\n projection  '}
        def rpc(method, params):
            calls.append((method, params))
            return {'goal': goal}
        native.rpc = rpc
        self.assertIn('paused · Shared resources projection', activity.goal_line(native.goal('native-a')))
        self.assertEqual(calls, [('thread/goal/get', {'threadId': 'native-a'})])
        with self.assertRaises(ValueError):
            native.goal('native-b')
        goal['status'] = 'blocked'
        self.assertIn('blocked', activity.goal_line(goal))
        for status in ('active', 'paused', 'blocked', 'usageLimited', 'budgetLimited', 'complete'):
            goal['status'] = status
            self.assertIn(status, activity.goal_line(goal))
        goal['objective'] = 'x' * 1000
        self.assertLess(len(activity.goal_line(goal)), 220)
        self.assertIsNone(activity.goal_line(None))
        goal['status'] = 'invented'
        self.assertIsNone(activity.goal_line(goal))
        reader = activity.Rollout(self.root / 'unused', 'native-a')
        self.assertIsNone(activity.status_line({'type': 'idle'}, reader, [], 100, automatic=True))

    def test_observer_methods_cannot_consume_updates_or_start_turns(self):
        for method in ('getUpdates', 'getMe', 'sendDocument'):
            with self.assertRaises(ValueError):
                self.telegram.call(method, {})
        native = object.__new__(activity.Native)
        for method in ('turn/start', 'turn/steer', 'thread/resume', 'thread/goal/set', 'thread/goal/clear'):
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

    def test_unloaded_native_session_is_idle_and_child_work_remains_visible(self):
        reader = activity.Rollout(self.root / 'unused', 'parent')
        now = time.time()
        reader.work_running = False
        # Unloading a stored thread is a successful native read, not a lost connection.
        self.assertEqual(activity.status_report({'type': 'notLoaded'}, reader, [], now), '◻️ Idle')
        self.assertIsNone(activity.status_line({'type': 'notLoaded'}, reader, [], now, automatic=True))
        self.assertIn('Waiting for 1 agent', activity.status_report(
            {'type': 'notLoaded'}, reader, [{'type': 'active'}], now))
        for kind in ('unknown', 'systemError'):
            self.assertIn('unavailable', activity.status_report({'type': kind}, reader, [], now))

    def test_automatic_indicator_shows_silent_work_and_ignores_tool_identity_churn(self):
        reader = activity.Rollout(self.root / 'unused', 'parent')
        now = int(time.time())
        def exchange(kind, at):
            reader.consume({'timestamp': time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime(at)),
                'type': 'event_msg', 'payload': {'type': 'item_completed', 'item': kind}})
        exchange({'type': 'UserMessage', 'id': 'request'}, now)
        self.assertIn('Working', activity.status_line({'type': 'active'}, reader, [], now))
        self.assertIsNone(activity.status_line({'type': 'active'}, reader, [], now, automatic=True))
        self.assertIsNone(activity.status_line({'type': 'active'}, reader, [], now+29, automatic=True))
        self.assertIn('Working', activity.status_line({'type': 'active'}, reader, [], now+30, automatic=True))
        reader.calls['pending-a'] = 'tools'
        self.assertIn('tool call', activity.status_line({'type': 'active'}, reader, [], now+31, automatic=True))
        reader.calls = {'pending-b': 'tools'}
        self.assertIn('tool call', activity.status_line({'type': 'active'}, reader, [], now+32, automatic=True))
        exchange({'type': 'AgentMessage', 'phase': 'commentary'}, now+33)
        self.assertIsNone(activity.status_line({'type': 'active'}, reader, [], now+33, automatic=True))
        self.assertIsNone(activity.status_line({'type': 'active'}, reader, [], now+62, automatic=True))
        self.assertIn('tool call', activity.status_line({'type': 'active'}, reader, [], now+63, automatic=True))
        reader.calls.clear()
        exchange({'type': 'AgentMessage', 'phase': 'final_answer'}, now+64)
        self.assertIsNone(activity.status_line({'type': 'active'}, reader, [], now+94, automatic=True))
        self.assertIsNone(activity.status_line({'type': 'idle'}, reader, [], now+94, automatic=True))

    def test_status_cache_revalidates_binding_and_marks_stale_snapshots_unconfirmed(self):
        db = sqlite3.connect(':memory:')
        db.row_factory = sqlite3.Row
        db.executescript('''CREATE TABLE incoming_updates(update_id INTEGER PRIMARY KEY,
            payload_json TEXT,status TEXT,updated_at TEXT);
            CREATE TABLE users(tg_user_id INTEGER PRIMARY KEY,allowed INTEGER);
            CREATE TABLE sessions(chat_id INTEGER,thread_id INTEGER,codex_thread_id TEXT,creator_user_id INTEGER);
            INSERT INTO users VALUES(1,1);
            INSERT INTO sessions VALUES(100,7,'original',1);''')
        requests = activity.StatusRequests(db, self.telegram, self.root / 'status-requests.json')
        cache = activity.StatusCache()
        quota = activity.WeeklyLimit()
        quota.available, quota.resets_at = 75, time.time()+3600
        contexts = {'100:7': {'turn': 'request-a', 'active': True}}
        cache.update({'100:7':'Working'}, {'100:7':'original'}, quota, contexts)
        def add(identifier):
            db.execute("INSERT INTO incoming_updates VALUES(?,?,?,datetime('now'))", (identifier,
                json.dumps({'message':{'chat':{'id':100}, 'message_thread_id':7, 'from':{'id':1},
                'message_id':identifier+1000,'text':'/status'}}), 'handled'))
        add(1)
        db.execute("UPDATE sessions SET codex_thread_id='replacement'")
        cache.respond(requests, self.publisher)
        self.assertEqual(self.calls, [])
        cache.update({'100:7':'Working'}, {'100:7':'replacement'}, quota, contexts)
        reports, bindings, available, reset, observed, contexts = cache.value
        cache.value = (reports, bindings, available, reset, observed-30, contexts)
        cache.respond(requests, self.publisher)
        self.assertIn('unconfirmed', self.calls[0][1]['text'])
        self.assertIn('weekly n/a', self.calls[0][1]['text'])
        self.assertEqual(self.publisher.snapshot(), {})
        cache.respond(requests, self.publisher)
        self.assertEqual(len(self.calls),1)
        db.close()

    def test_goal_status_is_literal_escaped_and_topic_scoped_over_http(self):
        db = sqlite3.connect(':memory:')
        db.row_factory = sqlite3.Row
        db.executescript("CREATE TABLE incoming_updates(update_id INTEGER PRIMARY KEY,\n            payload_json TEXT,status TEXT,updated_at TEXT);\n            CREATE TABLE users(tg_user_id INTEGER PRIMARY KEY,allowed INTEGER);\n            CREATE TABLE sessions(chat_id INTEGER,thread_id INTEGER,codex_thread_id TEXT,creator_user_id INTEGER);\n            INSERT INTO users VALUES(1,1);\n            INSERT INTO sessions VALUES(100,7,'native-a',1);\n            INSERT INTO sessions VALUES(100,8,'native-b',1);")
        try:
            requests = activity.StatusRequests(db, self.telegram, self.root / 'goal-requests.json')
            cache, quota = activity.StatusCache(), activity.WeeklyLimit()
            for number, topic in ((1, 7), (2, 8)):
                payload = {'message': {'chat': {'id': 100}, 'message_thread_id': topic,
                    'from': {'id': 1}, 'message_id': number, 'text': '/status'}}
                db.execute("INSERT INTO incoming_updates VALUES(?,?,'handled',datetime('now'))",
                           (number, json.dumps(payload)))
            cache.update({'100:7': activity.with_goal('◻️ Idle', activity.goal_line({'status': 'paused', 'objective': 'Project <A>'})),
                          '100:8': activity.with_goal('◻️ Idle', activity.goal_line({'status': 'blocked', 'objective': 'Project B'}))},
                         {'100:7': 'native-a', '100:8': 'native-b'}, quota)
            cache.respond(requests, self.publisher)
            self.assertEqual(len(self.calls), 2)
            self.assertIn('paused · Project &lt;A&gt;', self.calls[0][1]['text'])
            self.assertNotIn('Project B', self.calls[0][1]['text'])
            self.assertIn('blocked · Project B', self.calls[1][1]['text'])
            self.assertNotIn('Project &lt;A&gt;', self.calls[1][1]['text'])
            self.assertFalse(self.publisher.entries, 'paused/blocked goals published a Working card')
        finally:
            db.close()

    def test_automatic_work_card_keeps_goal_metadata_and_retires_at_idle(self):
        now = time.time()
        reader = activity.Rollout(self.root / 'goal-card.jsonl', 'parent')
        reader.started = time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime(now - 90))
        goal = activity.goal_line({'status': 'active', 'objective': 'Check <goal> ' + 'x' * 250})
        text = activity.with_goal(activity.status_line({'type': 'active'}, reader, [], now,
                                                       automatic=True), goal)
        self.publisher.update('100:7', 100, 7, 'request-a', text, now)
        self.assertIn('Working', self.calls[0][1]['text'])
        self.assertIn('Goal · active · Check &lt;goal&gt;', self.calls[0][1]['text'])
        self.assertIn('…', self.calls[0][1]['text'])
        self.assertEqual(self.calls[0][1]['message_thread_id'], 7)
        blocked = activity.goal_line({'status': 'blocked', 'objective': 'Waiting for a person'})
        idle = activity.with_goal(activity.status_line({'type': 'idle'}, reader, [], now + 35,
                                                      automatic=True), blocked)
        self.assertIsNone(idle, 'goal metadata created automatic work while idle')
        self.publisher.update('100:7', 100, 7, 'request-a', idle, now + 35)
        self.assertEqual(self.publisher.snapshot(), {})
        self.assertEqual(self.calls[-1][0], 'deleteMessage')

    def test_status_command_is_answered_while_native_observation_is_not_running(self):
        path = self.root / 'bridge.sqlite3'
        db = sqlite3.connect(path)
        db.executescript('''CREATE TABLE incoming_updates(update_id INTEGER PRIMARY KEY,
            payload_json TEXT,status TEXT,updated_at TEXT);
            CREATE TABLE users(tg_user_id INTEGER PRIMARY KEY,allowed INTEGER);
            CREATE TABLE sessions(chat_id INTEGER,thread_id INTEGER,codex_thread_id TEXT,creator_user_id INTEGER);
            INSERT INTO users VALUES(1,1);
            INSERT INTO sessions VALUES(100,7,'bound',1);''')
        db.commit()
        cache = activity.StatusCache()
        quota = activity.WeeklyLimit()
        quota.available, quota.resets_at = 78, time.time()+86400
        cache.update({'100:7':'Working'}, {'100:7':'bound'}, quota)
        stopped = threading.Event()
        worker = threading.Thread(target=activity.watch_status_requests,
            args=({'db_path':str(path)}, self.root, cache, stopped, self.telegram))
        worker.start()
        try:
            deadline=time.monotonic()+2
            while not (self.root/'status-requests.json').exists():
                self.assertLess(time.monotonic(), deadline)
                time.sleep(.01)
            started=time.monotonic()
            db.execute("INSERT INTO incoming_updates VALUES(1,?,'handled',datetime('now'))", (json.dumps(
                {'message':{'chat':{'id':100},'message_thread_id':7,'from':{'id':1},
                'message_id':1001,'text':'/status'}}),))
            db.commit()
            while not self.calls:
                self.assertLess(time.monotonic()-started, 1.5)
                time.sleep(.01)
            self.assertIn('Working',self.calls[0][1]['text'])
            self.assertIn('weekly 78%',self.calls[0][1]['text'])
            self.assertEqual(self.calls[0][1]['reply_parameters'], {'message_id':1001})
        finally:
            stopped.set()
            worker.join(timeout=2)
            db.close()
        self.assertFalse(worker.is_alive())

    def test_active_status_commands_share_the_automatic_message_and_finish_cleanup(self):
        db = sqlite3.connect(':memory:')
        db.row_factory = sqlite3.Row
        db.executescript('''CREATE TABLE incoming_updates(update_id INTEGER PRIMARY KEY,
            payload_json TEXT,status TEXT,updated_at TEXT);
            CREATE TABLE users(tg_user_id INTEGER PRIMARY KEY,allowed INTEGER);
            CREATE TABLE sessions(chat_id INTEGER,thread_id INTEGER,codex_thread_id TEXT,creator_user_id INTEGER);
            INSERT INTO users VALUES(1,1);
            INSERT INTO sessions VALUES(100,7,'bound',1);''')
        requests = activity.StatusRequests(db, self.telegram, self.root / 'requests.json')
        cache, quota = activity.StatusCache(), activity.WeeklyLimit()
        quota.available, quota.resets_at = 78, time.time() + 86400
        cache.update({'100:7': 'Working'}, {'100:7': 'bound'}, quota,
                     {'100:7': {'turn': 'request-a', 'active': True}})
        now = time.time()
        self.publisher.update('100:7', 100, 7, 'request-a', 'Working', now)
        original = self.publisher.snapshot()['100:7']['message_id']
        for update in (1, 2):
            db.execute("INSERT INTO incoming_updates VALUES(?,?,'handled',datetime('now'))", (update,
                json.dumps({'message': {'chat': {'id': 100}, 'message_thread_id': 7,
                    'from': {'id': 1}, 'message_id': 1000 + update, 'text': '/status'}})))
            cache.respond(requests, self.publisher)
        self.assertEqual([method for method, _ in self.calls],
                         ['sendMessage', 'editMessageText'])
        self.assertEqual([receipt['message_id'] for receipt in requests.state['attempted'].values()],
                         [original, original])
        restarted = activity.Publisher(self.telegram, self.path)
        # A new commentary exchange would suppress automatic creation, but the
        # explicitly requested card must keep updating for this same request.
        restarted.update('100:7', 100, 7, 'request-a', None, now + 40,
                         active_text='Waiting for 1 agent · weekly 78% available')
        self.assertEqual(self.calls[-1][0], 'editMessageText')
        self.assertEqual(self.calls[-1][1]['message_id'], original)
        self.assertIn('Waiting for 1 agent', self.calls[-1][1]['text'])
        restarted.update('100:7', 100, 7, 'request-a', None, now + 48)
        self.assertEqual(self.calls[-1][0], 'deleteMessage')
        self.assertEqual(restarted.snapshot(), {})
        # The explicit request does not bypass silence grace for a later turn.
        restarted.update('100:7', 100, 7, 'request-b', None, now + 50, active_text='Working')
        self.assertEqual(restarted.snapshot(), {})
        db.close()

    def test_concurrent_manual_and_automatic_status_create_only_one_card(self):
        barrier = threading.Barrier(2)
        def publish(requested):
            barrier.wait(timeout=2)
            self.publisher.update('100:7', 100, 7, 'request-a', 'Working', time.time(),
                                  requested=requested, reply_to=1001 if requested else None)
        threads = [threading.Thread(target=publish, args=(requested,)) for requested in (True, False)]
        for thread in threads:
            thread.start()
        for thread in threads:
            thread.join(timeout=3)
            self.assertFalse(thread.is_alive())
        self.assertEqual(sum(method == 'sendMessage' for method, _ in self.calls), 1)
        entry = self.publisher.snapshot()['100:7']
        self.assertTrue(entry['requested'])
        self.assertEqual(entry['message_id'], 101)

    def test_reactivated_child_shows_automatic_status_after_quiet_parent_final(self):
        path = self.root / 'reactivated.jsonl'
        now = time.time()
        stamp = lambda seconds: time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime(now - seconds))
        records = [
            {'type': 'session_meta', 'payload': {'id': 'parent'}},
            {'timestamp': stamp(90), 'type': 'event_msg',
             'payload': {'type': 'task_started', 'turn_id': 'request-a'}},
            {'timestamp': stamp(80), 'type': 'event_msg', 'payload': {'type': 'item_completed',
             'item': {'type': 'AgentMessage', 'phase': 'final_answer'}}},
            {'timestamp': stamp(75), 'type': 'event_msg', 'payload': {'type': 'item_completed',
             'item': {'type': 'SubAgentActivity', 'kind': 'completed', 'agent_thread_id': 'child'}}},
            {'timestamp': stamp(31), 'type': 'event_msg', 'payload': {'type': 'item_completed',
             'item': {'type': 'SubAgentActivity', 'kind': 'interacted', 'agent_thread_id': 'child'}}},
        ]
        path.write_text(''.join(json.dumps(r) + '\n' for r in records))
        index = sqlite3.connect(':memory:')
        index.row_factory = sqlite3.Row
        index.execute('CREATE TABLE threads(id TEXT,source TEXT,archived INTEGER)')
        index.execute('INSERT INTO threads VALUES(?,?,0)', ('child', json.dumps(
            {'subagent': {'thread_spawn': {'parent_thread_id': 'parent'}}})))
        reader = activity.Rollout(path, 'parent')
        reader.refresh()
        self.assertFalse(reader.work_running)
        self.assertEqual(reader.agents, {'child'})
        children = [{'type': 'active'} for child in reader.agents
                    if activity.linked_child(index, child, 'parent')]
        line = activity.status_line({'type': 'idle'}, reader, children, now, automatic=True)
        self.assertIn('Waiting for 1 agent', line)
        self.publisher.update('100:7', 100, 7, reader.request_id, line, now)
        self.assertEqual(self.calls[0][0], 'sendMessage')  # No /status command.
        self.assertTrue(self.publisher.snapshot()['100:7']['message_id'])
        # Routine subagent events must not restart the thirty-second silence grace.
        self.assertEqual(reader.last_exchange, stamp(80))
        records[-1]['payload']['item']['kind'] = 'completed'
        with path.open('a') as stream:
            stream.write(json.dumps(records[-1]) + '\n')
        reader.refresh()
        self.assertEqual(reader.agents, set())
        line = activity.status_line({'type': 'idle'}, reader, [], now + 8, automatic=True)
        self.publisher.update('100:7', 100, 7, reader.request_id, line, now + 8)
        self.assertEqual(self.calls[-1][0], 'deleteMessage')
        index.close()

    def test_same_turn_card_moves_below_new_exchanges_then_edits_in_place(self):
        old = {'exchange': 'commentary-a', 'input_id': 1000}
        new = {'exchange': 'commentary-b', 'input_id': 1000}
        self.publisher.update('100:7', 100, 7, 'request-a', 'Working', 100,
                              requested=True, position=old)
        first = self.publisher.snapshot()['100:7']['message_id']
        # Same native request, but an assistant message is now below the card.
        self.publisher.update('100:7', 100, 7, 'request-a', None, 101,
                              active_text='Working', position=new)
        self.assertEqual([m for m, _ in self.calls], ['sendMessage', 'deleteMessage'])
        self.assertEqual(self.calls[-1][1]['message_id'], first)
        self.assertEqual(self.publisher.snapshot(), {})
        self.publisher.update('100:7', 100, 7, 'request-a', 'Working', 132,
                              active_text='Working', position=new)
        last = self.publisher.snapshot()['100:7']['message_id']
        self.assertNotEqual(first, last)
        self.publisher.update('100:7', 100, 7, 'request-a', 'Waiting for agent', 163,
                              position=new)
        self.assertEqual(self.calls[-1][0], 'editMessageText')
        self.assertEqual(self.calls[-1][1]['message_id'], last)
        # A slash command creates no native user item, but still moves the card.
        command = dict(new, input_id=1004)
        self.publisher.update('100:7', 100, 7, 'request-a', None, 164,
                              active_text='Working', position=command)
        self.assertEqual(self.calls[-1][0], 'deleteMessage')
        self.assertEqual(self.publisher.snapshot(), {})

    def test_status_command_repositions_card_after_standard_session_details(self):
        db = sqlite3.connect(':memory:'); db.row_factory = sqlite3.Row
        db.executescript('''CREATE TABLE incoming_updates(update_id INTEGER PRIMARY KEY,
            payload_json TEXT,status TEXT,updated_at TEXT);
            CREATE TABLE users(tg_user_id INTEGER PRIMARY KEY,allowed INTEGER);
            CREATE TABLE sessions(chat_id INTEGER,thread_id INTEGER,codex_thread_id TEXT,creator_user_id INTEGER);
            INSERT INTO users VALUES(1,1);
            INSERT INTO users VALUES(2,0);
            INSERT INTO sessions VALUES(100,7,'bound',1);''')
        requests = activity.StatusRequests(db, self.telegram, self.root / 'requests.json')
        old = {'exchange': 'commentary', 'input_id': 1000}
        now = time.time()
        self.publisher.update('100:7', 100, 7, 'request-a', 'Working', now, position=old)
        original = self.publisher.snapshot()['100:7']['message_id']
        for update, sender, topic in ((1, 1, 7), (2, 2, 7), (3, 1, 8)):
            db.execute("INSERT INTO incoming_updates VALUES(?,?,'handled',datetime('now'))", (update,
                json.dumps({'message': {'chat': {'id': 100}, 'message_thread_id': topic,
                    'from': {'id': sender}, 'message_id': 1000 + update,
                    'date': int(now), 'text': '/status'}})))
        latest = activity.latest_input(db, 100, 7)
        self.assertEqual(latest['message_id'], 1001)
        self.assertEqual(latest['at'], int(now))
        requests.respond({'100:7': 'Working'}, lambda: 'weekly 78% available', {'100:7': 'bound'},
                         {'100:7': {'active': True, 'turn': 'request-a', 'position': old}}, self.publisher)
        self.assertEqual([m for m, _ in self.calls], ['sendMessage', 'deleteMessage', 'sendMessage'])
        self.assertNotEqual(original, self.publisher.snapshot()['100:7']['message_id'])
        self.assertEqual(self.calls[-1][1]['reply_parameters'], {'message_id': 1001})
        self.assertEqual(self.publisher.snapshot()['100:7']['position']['input_id'], 1001)
        db.close()

    def test_async_question_does_not_mark_continuing_native_work_completed(self):
        reader = activity.Rollout(self.root / 'unused', 'parent')
        now = time.time()
        stamp = lambda seconds: time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime(now - seconds))
        reader.consume({'timestamp': stamp(90), 'type': 'event_msg',
                        'payload': {'type': 'task_started', 'turn_id': 'request-a'}})
        reader.consume({'timestamp': stamp(31), 'type': 'event_msg', 'payload': {
            'type': 'item_completed', 'item': {'type': 'AgentMessage',
                'phase': 'final_answer', 'delivery': 'async', 'questions': [{}]}}})
        line = activity.status_line({'type': 'active'}, reader, [], now, automatic=True)
        self.assertIn('Working', line)
        reader.consume({'timestamp': stamp(0), 'type': 'event_msg',
                        'payload': {'type': 'task_complete'}})
        self.assertIsNone(activity.status_line({'type': 'idle'}, reader, [], now, automatic=True))

    def test_manual_request_respects_retry_deadline_and_does_not_replay_ambiguous_send(self):
        self.answers = [{'ok': False, 'error_code': 429, 'parameters': {'retry_after': 60}}]
        self.publisher.update('100:7', 100, 7, 'request-a', 'Working', 100, requested=True)
        self.publisher.update('100:7', 100, 7, 'request-a', 'Working', 140, requested=True)
        self.assertEqual(len(self.calls), 1)
        self.answers = [None]
        self.publisher.update('100:7', 100, 7, 'request-a', 'Working', 161, requested=True)
        restarted = activity.Publisher(self.telegram, self.path)
        restarted.update('100:7', 100, 7, 'request-a', 'Working', 200, requested=True)
        self.assertEqual(len(self.calls), 2)



    def marker_database(self):
        database = sqlite3.connect(self.root / 'markers.sqlite')
        database.row_factory = sqlite3.Row
        database.executescript('''CREATE TABLE users(tg_user_id INTEGER,allowed INTEGER);
            INSERT INTO users VALUES(100,1);
            CREATE TABLE sessions(id INTEGER,chat_id INTEGER,thread_id INTEGER,
                creator_user_id INTEGER,codex_thread_id TEXT,force_fresh_thread INTEGER,session_title TEXT);
            INSERT INTO sessions VALUES(1,100,7,100,'one',0,'Unverified summary');
            CREATE TABLE turns(session_id INTEGER,from_user_id INTEGER,started_at TEXT);
            CREATE TABLE bot_state(key TEXT,value TEXT);''')
        database.execute('INSERT INTO bot_state VALUES(?,?)', ('topic_catalog:100:7',
            json.dumps({'name': {'value': 'Human title', 'source': {'kind': 'rename_receipt'}},
                        'lifecycle': {'value': 'open'}, 'icon_color': {'value': 123}})))
        self.addCleanup(database.close)
        return database

    def marker_snapshot(self, now):
        return {'connected': True, 'observations': {'100:7': {'native': 'one', 'human': 100,
            'observed_at': now, 'goal_state': 'active', 'execution': 'idle'}}}

    def test_known_name_with_unknown_lifecycle_still_shows_observed_native_goal(self):
        database = self.marker_database()
        doc = json.loads(database.execute('SELECT value FROM bot_state').fetchone()[0])
        doc.pop('lifecycle')
        database.execute('UPDATE bot_state SET value=?', (json.dumps(doc),))
        markers = activity.TopicMarkers(self.telegram, self.root / 'unknown-lifecycle.json')
        now = time.time()
        markers.update(database, self.marker_snapshot(now), now=now)
        self.assertEqual(self.calls, [('editForumTopic',
            {'chat_id': 100, 'message_thread_id': 7, 'name': '🎯 Human title'})])
        self.assertEqual(json.loads(database.execute('SELECT value FROM bot_state').fetchone()[0]), doc)
        self.assertNotIn('lifecycle', doc)  # No inferred open/reopen operation.

    def test_topic_markers_preserve_canonical_names_and_icons_and_restore_without_loop(self):
        database = self.marker_database()
        path = self.root / 'markers.json'
        markers = activity.TopicMarkers(self.telegram, path)
        now = time.time()
        snapshot = self.marker_snapshot(now)
        before = database.execute('SELECT value FROM bot_state').fetchone()[0]
        markers.update(database, snapshot, now=now)
        self.assertEqual(self.calls, [('editForumTopic',
            {'chat_id': 100, 'message_thread_id': 7, 'name': '🎯 Human title'})])
        self.assertEqual(database.execute('SELECT value FROM bot_state').fetchone()[0], before)
        restarted = activity.TopicMarkers(self.telegram, path)
        restarted.update(database, snapshot, now=now + 1)
        self.assertEqual(len(self.calls), 1)
        # Graceful retirement restores only our own acknowledged decoration,
        # without waiting thirty seconds or changing the canonical/native name.
        restarted.update(database, snapshot, restore=True, now=now + 2)
        self.assertEqual(self.calls[-1][1]['name'], 'Human title')
        restarted.update(database, snapshot, restore=True, now=now + 3)
        self.assertEqual(len(self.calls), 2)

    def test_uncertain_topic_marker_is_not_replayed_or_restored_after_restart(self):
        database = self.marker_database()
        path = self.root / 'markers.json'
        markers = activity.TopicMarkers(self.telegram, path)
        now = time.time()
        snapshot = self.marker_snapshot(now)
        self.answers = [None]  # Server accepted body, connection lost before ACK.
        markers.update(database, snapshot, now=now)
        self.assertEqual(markers.state['100:7']['phase'], 'uncertain')
        restarted = activity.TopicMarkers(self.telegram, path)
        snapshot['observations']['100:7']['observed_at'] = now + 90
        restarted.update(database, snapshot, now=now + 90)
        restarted.update(database, snapshot, restore=True, now=now + 91)
        self.assertEqual(len(self.calls), 1)
        # A genuine newly observed human name supersedes the old projection.
        database.execute('UPDATE bot_state SET value=?', (json.dumps({
            'name': {'value': 'A new human title', 'source': {'kind': 'telegram_update', 'update_id': 99}},
            'lifecycle': {'value': 'open'}}),))
        restarted.update(database, snapshot, now=now + 92)
        self.assertEqual(self.calls[-1][1]['name'], '🎯 A new human title')
        self.assertEqual(len(self.calls), 2)

    def test_topic_marker_rejection_binding_changes_revocation_and_long_names(self):
        database = self.marker_database()
        markers = activity.TopicMarkers(self.telegram, self.root / 'markers.json')
        now = time.time()
        snapshot = self.marker_snapshot(now)
        self.answers = [{'ok': False, 'error_code': 429, 'parameters': {'retry_after': 60}}]
        markers.update(database, snapshot, now=now)
        snapshot['observations']['100:7']['observed_at'] = now + 40
        markers.update(database, snapshot, now=now + 40)
        self.assertEqual(len(self.calls), 1)
        snapshot['observations']['100:7']['observed_at'] = now + 61
        markers.update(database, snapshot, now=now + 61)
        self.assertEqual(len(self.calls), 2)
        database.execute("UPDATE sessions SET codex_thread_id='different'")
        markers.update(database, snapshot, now=now + 92)
        self.assertEqual(self.calls[-1][1]['name'], 'Human title')  # No stale working claim.
        database.execute('UPDATE users SET allowed=0')
        markers.update(database, snapshot, now=now + 123)
        self.assertEqual(len(self.calls), 3)
        database.execute('UPDATE users SET allowed=1')
        database.execute("UPDATE sessions SET codex_thread_id='one'")
        database.execute('UPDATE bot_state SET value=?', (json.dumps({
            'name': {'value': 'a'*128}, 'lifecycle': {'value': 'open'}}),))
        snapshot['observations']['100:7']['observed_at'] = now + 124
        markers.update(database, snapshot, now=now + 124)
        self.assertEqual(len(self.calls), 3)  # Never truncate a human name to add a marker.

    def test_finite_marker_retirement_uses_receipts_without_a_native_connection(self):
        database = self.marker_database()
        database.commit()
        state = self.root / 'marker-state'
        state.mkdir(mode=0o700)
        markers = activity.TopicMarkers(self.telegram, state / 'topic-markers.json')
        now = time.time()
        markers.update(database, self.marker_snapshot(now), now=now)
        config = self.root / 'marker-config.toml'
        config.write_text(f'db_path = {json.dumps(str(self.root / "markers.sqlite"))}\n'
            '[telegram]\n' + f'bot_token_file = {json.dumps(str(self.root / "bot.token"))}\n'
            f'api_base = {json.dumps(self.telegram.base)}\n')
        result = subprocess.run(['python3', '-B', str(pathlib.Path(activity.__file__)),
            '--config', str(config), '--state-dir', str(state), '--restore-topic-markers'],
            capture_output=True, text=True, timeout=5, check=True)
        self.assertEqual(json.loads(result.stdout)['outcomes']['100:7'], 'applied')
        self.assertEqual(self.calls[-1][1]['name'], 'Human title')
        self.assertEqual(len(self.calls), 2)
        restored = activity.TopicMarkers(self.telegram, state / 'topic-markers.json')
        other = sqlite3.connect(self.root / 'another-database.sqlite')
        other.row_factory = sqlite3.Row
        try:
            with self.assertRaises(ValueError):
                restored.update(other, {}, restore=True)
        finally:
            other.close()
        self.assertEqual(len(self.calls), 2)


class TopicActivityIO(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.root = pathlib.Path(self.temporary.name)
        self.path = self.root / 'bridge.sqlite'
        self.database = sqlite3.connect(self.path)
        self.database.row_factory = sqlite3.Row
        self.database.executescript('''CREATE TABLE users(tg_user_id INTEGER,allowed INTEGER);
            INSERT INTO users VALUES(100,1),(101,0);
            CREATE TABLE sessions(id INTEGER,chat_id INTEGER,thread_id INTEGER,
                creator_user_id INTEGER,codex_thread_id TEXT,force_fresh_thread INTEGER,session_title TEXT);
            CREATE TABLE turns(session_id INTEGER,from_user_id INTEGER,started_at TEXT);
            CREATE TABLE bot_state(key TEXT,value TEXT);
            INSERT INTO sessions VALUES(1,100,7,100,'one',0,'Summary only'),
                (2,100,8,100,'two',0,NULL),(3,100,9,100,'three',0,NULL),
                (4,100,10,100,'four',0,NULL),(5,100,11,100,NULL,0,NULL),
                (6,100,12,101,'denied',0,NULL);''')
        self.now = time.time()
        def observation(native, goal, execution='idle'):
            return {'native': native, 'human': 100, 'goal_state': goal,
                    'execution': execution, 'observed_at': self.now,
                    'last_conversation_at': None}
        self.snapshot = {'schema': 'telecodex.topic-observations/v1',
                         'source': activity.inventory_source(self.database), 'connected': True,
                         'observations': {'100:7': observation('one', 'active'),
                            '100:8': observation('two', 'absent', 'working'),
                            '100:9': observation('three', 'paused'),
                            '100:10': observation('four', 'blocked')}}

    def tearDown(self):
        self.database.close()
        self.temporary.cleanup()

    def inventory(self, **kwargs):
        return activity.topic_activity_inventory(self.database, self.snapshot, now=self.now, **kwargs)

    def test_goal_work_passive_and_unknown_are_separate_with_evidence_only_recency(self):
        self.snapshot['observations']['100:9']['last_conversation_at'] = self.now - 60
        self.database.execute('INSERT INTO bot_state VALUES(?,?)', ('topic_catalog:100:7',
            json.dumps({'name': {'value': 'Human title', 'source': {'kind': 'telegram_update'}}})))
        view = self.inventory()
        self.assertEqual(view['counts'], {'active_goal': 1, 'working_without_active_goal': 1,
            'passive': 2, 'unknown': 1, 'passive_recent': 1, 'passive_recency_unknown': 1})
        items = {row['key']: row for row in view['items']}
        self.assertEqual(items['100:7']['marker'], '🎯')  # Active goal, idle execution.
        self.assertEqual(items['100:7']['execution'], 'idle')
        self.assertEqual(items['100:7']['telegram']['name']['value'], 'Human title')
        self.assertEqual(items['100:7']['title_summary'], 'Summary only')
        self.assertEqual(items['100:8']['marker'], '⚙')
        self.assertEqual(items['100:9']['goal_state'], 'paused')
        self.assertEqual(items['100:9']['marker'], '')
        self.assertIsNone(items['100:10']['recent_conversation'])
        self.assertNotIn('100:12', items)
        # Tool/metadata polling is not conversation and cannot make an old tail recent.
        self.snapshot['observations']['100:9']['last_conversation_at'] = self.now - 8*86400
        self.assertEqual(self.inventory()['counts']['passive_recent'], 0)
        self.assertEqual(self.inventory(days=9)['counts']['passive_recent'], 1)

    def test_current_bindings_authorization_and_per_row_freshness_are_revalidated(self):
        self.snapshot['observations']['100:7']['observed_at'] -= 21
        self.snapshot['observations']['100:8']['goal_state'] = 'unknown'
        self.database.execute("UPDATE sessions SET codex_thread_id='replacement' WHERE id=3")
        self.database.execute('UPDATE sessions SET force_fresh_thread=1 WHERE id=4')
        self.assertEqual(self.inventory()['counts']['unknown'], 5)
        self.snapshot['observations']['100:7']['observed_at'] = self.now + 1
        self.assertEqual(self.inventory()['counts']['unknown'], 5)  # Future clocks unconfirmed.
        self.database.execute('UPDATE users SET allowed=0 WHERE tg_user_id=100')
        self.assertEqual(self.inventory()['items'], [])

    def test_disconnect_keeps_history_without_live_claims_or_cached_counts(self):
        self.snapshot['connected'] = False
        self.snapshot['counts'] = {'active_goal': 999}
        self.snapshot['observations']['100:9']['last_conversation_at'] = self.now - 60
        view = self.inventory()
        self.assertEqual(view['counts']['unknown'], 5)
        passive = next(row for row in view['items'] if row['key'] == '100:9')
        self.assertEqual(passive['goal_state'], 'unknown')
        self.assertTrue(passive['recent_conversation'])

    def test_goal_absence_and_unsupported_read_are_different_without_mutation(self):
        native = object.__new__(activity.Native)
        calls = []
        responses = iter(({'goal': None}, None, {}, {'goal': {'threadId': 'one', 'status': 'blocked'}}))
        native.rpc = lambda method, params: (calls.append((method, params)) or next(responses))
        self.assertEqual([native.goal_observation('one')['state'] for _ in range(4)],
                         ['absent', 'unknown', 'unknown', 'blocked'])
        self.assertTrue(all(method == 'thread/goal/get' for method, _ in calls))

    def test_rollout_recency_does_not_reset_on_task_start_or_change_on_tools(self):
        reader = activity.Rollout(self.root / 'unused', 'one')
        stamp = '2026-10-20T00:00:00Z'
        reader.consume({'timestamp': stamp, 'payload': {'type': 'item_completed',
            'item': {'type': 'UserMessage', 'id': 'human'}}})
        expected = reader.last_conversation_at
        reader.consume({'timestamp': '2026-10-20T01:00:00Z',
                        'payload': {'type': 'task_started', 'turn_id': 'later'}})
        reader.consume({'timestamp': '2026-10-20T02:00:00Z', 'type': 'response_item',
                        'payload': {'type': 'function_call', 'call_id': 'call', 'name': 'read'}})
        self.assertEqual(reader.last_conversation_at, expected)

    def test_finite_cli_reads_cached_inventory_without_token_native_or_publication(self):
        self.database.commit()
        state = self.root / 'state'
        state.mkdir(mode=0o700)
        activity.atomic_json(state / 'topics.json', self.snapshot)
        config = self.root / 'config.toml'
        config.write_text(f'db_path = {json.dumps(str(self.path))}\n')
        result = subprocess.run(['python3', '-B', str(pathlib.Path(activity.__file__)),
            '--config', str(config), '--state-dir', str(state), '--inventory'],
            capture_output=True, text=True, timeout=5, check=True)
        view = json.loads(result.stdout)
        self.assertEqual(view['counts']['active_goal'], 1)
        self.assertFalse((state / 'observer.lock').exists())
        self.assertFalse((state / 'messages.json').exists())
        self.snapshot['source'] = 'a different database'
        activity.atomic_json(state / 'topics.json', self.snapshot)
        self.assertEqual(activity.read_topic_inventory(self.database, state / 'topics.json')['counts']['unknown'], 5)
        (state / 'topics.json').chmod(0o644)
        with self.assertRaises(ValueError):
            activity.read_topic_inventory(self.database, state / 'topics.json')

    def test_observer_sweep_persists_actual_binding_observations_before_retirement(self):
        from types import SimpleNamespace
        import contextlib
        import io
        self.database.executescript('''ALTER TABLE turns ADD COLUMN status TEXT;
            ALTER TABLE turns ADD COLUMN id INTEGER;
            CREATE TABLE incoming_updates(update_id INTEGER,payload_json TEXT,updated_at TEXT);''')
        self.database.commit()
        home = self.root / 'native'
        (home / 'sessions').mkdir(parents=True)
        rollout = home / 'sessions' / 'one.jsonl'
        rollout.write_text(json.dumps({'type': 'session_meta', 'payload': {'id': 'one'}})+'\n')
        with sqlite3.connect(home / 'state_5.sqlite') as database:
            database.execute('CREATE TABLE threads(id TEXT,archived INTEGER,rollout_path TEXT,source TEXT)')
            database.execute('INSERT INTO threads VALUES(?,0,?,NULL)', ('one', str(rollout)))
        database.close()
        config = self.root / 'config.toml'
        config.write_text(f'db_path = {json.dumps(str(self.path))}\n[codex]\n')
        state = self.root / 'observer'
        class Native:
            def __init__(self, binary):
                pass
            def rpc(self, method, params):
                assert method == 'account/rateLimits/read'
                return None
            def status(self, thread):
                assert thread == 'one'
                return {'type': 'idle'}
            def goal_observation(self, thread):
                assert thread == 'one'
                return {'state': 'active', 'goal': {'status': 'active', 'objective': 'Private objective'}}
            def close(self):
                pass
        args = SimpleNamespace(config=config, state_dir=state, inventory=False,
            restore_topic_markers=False, probe=True, topic_markers=False,
            status_requests=False, pending_update_record=None, activity_days=7)
        with (mock.patch.object(activity, 'Native', Native),
              mock.patch.object(activity.signal, 'signal'),
              mock.patch.dict(os.environ, {'CODEX_HOME': str(home)}),
              contextlib.redirect_stdout(io.StringIO())):
            activity.run(args)
        retained = json.loads((state / 'topics.json').read_text())
        self.assertFalse(retained['connected'])  # Probe ended, no live observer claimed.
        self.assertEqual(retained['observations']['100:7']['native'], 'one')
        self.assertEqual(retained['observations']['100:7']['goal_state'], 'active')
        self.assertNotIn('Private objective', (state / 'topics.json').read_text())
        health = json.loads((state / 'health.json').read_text())
        self.assertEqual(health['topic_activity']['counts']['active_goal'], 1)
        self.assertFalse((state / 'messages.json').exists())


if __name__ == '__main__':
    unittest.main()
