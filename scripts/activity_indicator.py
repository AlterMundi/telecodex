#!/usr/bin/env python3
"""Read-only native activity observer; never consumes Telegram updates or runs turns."""
import argparse
import base64
import datetime as dt
import fcntl
import html
import json
import os
import pathlib
import queue
import signal
import sqlite3
import stat
import struct
import subprocess
import threading
import time
import tomllib
import urllib.error
import urllib.request


def atomic_json(path, value):
    temporary = path.with_suffix('.tmp')
    with open(temporary, 'w', opener=lambda p, f: os.open(p, f, 0o600)) as stream:
        json.dump(value, stream)
        stream.flush()
        os.fsync(stream.fileno())
    os.replace(temporary, path)


def readonly(path):
    connection = sqlite3.connect(pathlib.Path(path).resolve().as_uri() + '?mode=ro', uri=True)
    connection.row_factory = sqlite3.Row
    connection.execute('PRAGMA query_only=ON')
    return connection


class Native:
    """Attach to the existing native daemon through its WebSocket stdio proxy."""
    def __init__(self, binary):
        self.process = subprocess.Popen([binary, 'app-server', 'proxy'],
                                        stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                        stderr=subprocess.DEVNULL)
        self.messages = queue.Queue(maxsize=128)
        self.sequence = 0
        self.closed = threading.Event()
        self.write_lock = threading.Lock()
        threading.Thread(target=self.read, daemon=True).start()
        try:
            self.messages.get(timeout=5)  # Verified WebSocket handshake.
            self.rpc('initialize', {'clientInfo': {'name': 'telecodex-activity', 'version': '1.0'},
                                    'capabilities': {'experimentalApi': False}})
            self.send({'method': 'initialized'})
        except Exception:
            self.close()
            raise

    def read_exact(self, size):
        value = self.process.stdout.read(size)
        if len(value) != size:
            raise EOFError()
        return value

    def read(self):
        try:
            key = base64.b64encode(os.urandom(16)).decode()
            request = (f'GET / HTTP/1.1\r\nHost: localhost\r\nUpgrade: websocket\r\n'
                       f'Connection: Upgrade\r\nSec-WebSocket-Key: {key}\r\n'
                       'Sec-WebSocket-Version: 13\r\n\r\n')
            self.process.stdin.write(request.encode())
            self.process.stdin.flush()
            header = bytearray()
            while not header.endswith(b'\r\n\r\n'):
                header.extend(self.read_exact(1))
                if len(header) > 8192:
                    raise ValueError('oversized handshake')
            import hashlib
            expected = base64.b64encode(hashlib.sha1(
                (key + '258EAFA5-E914-47DA-95CA-C5AB0DC85B11').encode()).digest())
            if not header.startswith(b'HTTP/1.1 101 ') or expected not in header:
                raise ValueError('invalid handshake')
            self.messages.put(True, timeout=1)
            partial = bytearray()
            while not self.closed.is_set():
                head = self.read_exact(2)
                opcode, length = head[0] & 15, head[1] & 127
                if length == 126:
                    length = struct.unpack('!H', self.read_exact(2))[0]
                elif length == 127:
                    length = struct.unpack('!Q', self.read_exact(8))[0]
                if length > 8 * 1024 * 1024 or head[1] & 128:
                    raise ValueError('unsupported native frame')
                data = self.read_exact(length)
                if opcode == 8:
                    break
                if opcode in (9, 10):
                    if opcode == 9:
                        self.frame(data, 10)
                    continue
                partial.extend(data)
                if len(partial) > 8 * 1024 * 1024:
                    raise ValueError('oversized native message')
                if head[0] & 128:
                    message = json.loads(partial)
                    partial.clear()
                    # This client has no authority to answer server requests.
                    if 'method' not in message and 'id' in message:
                        self.messages.put(message, timeout=1)
        except (OSError, ValueError, EOFError, queue.Full):
            pass
        finally:
            self.closed.set()

    def frame(self, data, opcode=1):
        mask = os.urandom(4)
        size = len(data)
        length = bytes([128 | size]) if size < 126 else (
            b'\xfe' + struct.pack('!H', size) if size < 65536 else
            b'\xff' + struct.pack('!Q', size))
        frame = bytes([128 | opcode]) + length + mask
        frame += bytes(value ^ mask[i % 4] for i, value in enumerate(data))
        with self.write_lock:
            self.process.stdin.write(frame)
            self.process.stdin.flush()

    def send(self, value):
        self.frame(json.dumps(value).encode())

    def rpc(self, method, params):
        if method not in ('initialize', 'thread/read'):
            raise ValueError('observer method forbidden')
        self.sequence += 1
        self.send({'id': self.sequence, 'method': method, 'params': params})
        deadline = time.monotonic() + 4
        while time.monotonic() < deadline:
            value = self.messages.get(timeout=max(.01, deadline - time.monotonic()))
            if value.get('id') == self.sequence:
                if 'error' in value:
                    return None
                return value.get('result', {})
        raise TimeoutError()

    def status(self, thread_id):
        response = self.rpc('thread/read', {'threadId': thread_id, 'includeTurns': False})
        if response is None:
            return {'type': 'unknown'}
        thread = response.get('thread', {})
        if thread.get('id') != thread_id:
            raise ValueError('native thread mismatch')
        return thread.get('status', {'type': 'unknown'})

    def close(self):
        self.closed.set()
        if self.process.poll() is None:
            self.process.terminate()  # Only this proxy, never the native daemon.
            try:
                self.process.wait(timeout=2)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait()
        self.process.stdin.close()
        self.process.stdout.close()


class Rollout:
    """Bounded initial tail and incremental metadata, restricted to linked threads."""
    def __init__(self, path, thread_id):
        self.path = pathlib.Path(path)
        self.thread_id = thread_id
        self.offset = 0
        self.identity = None
        self.calls = {}
        self.agents = set()
        self.turn_id = None
        self.started = None
        self.last_signal = None
        self.skipping = False

    def consume(self, value):
        p = value.get('payload', {})
        if not isinstance(p, dict):
            return
        self.last_signal = value.get('timestamp', self.last_signal)
        kind = p.get('type')
        if kind == 'task_started':
            self.calls.clear()
            self.turn_id = p.get('turn_id')
            self.started = value.get('timestamp')
        elif kind in ('task_complete', 'turn_aborted'):
            self.calls.clear()
        elif value.get('type') == 'response_item':
            call_id = p.get('call_id')
            if kind in ('function_call', 'custom_tool_call') and call_id:
                # Names are classified, never copied into the public status.
                name = str(p.get('name', '')).rsplit('.', 1)[-1]
                self.calls[call_id] = 'agents' if name in ('wait', 'wait_agent') else 'tools'
            elif kind in ('function_call_output', 'custom_tool_call_output'):
                self.calls.pop(call_id, None)
        if kind == 'item_completed':
            item = p.get('item', {})
            if not isinstance(item, dict):
                return
            if item.get('type') == 'SubAgentActivity':
                agent = item.get('agent_thread_id')
                if agent and item.get('kind') == 'started':
                    self.agents.add(agent)
                elif agent and item.get('kind') in ('completed', 'errored', 'shutdown'):
                    self.agents.discard(agent)
            elif item.get('type') == 'CollabAgentToolCall':
                self.agents.update(item.get('receiver_thread_ids', []))

    def refresh(self):
        with self.path.open('rb') as stream:
            info = os.fstat(stream.fileno())
            identity = (info.st_dev, info.st_ino)
            if self.identity != identity or info.st_size < self.offset:
                first = json.loads(stream.readline(1024 * 1024))
                if first.get('type') != 'session_meta' or first.get('payload', {}).get('id') != self.thread_id:
                    raise ValueError('rollout binding mismatch')
                self.calls.clear()
                self.agents.clear()
                self.started = self.turn_id = self.last_signal = None
                self.skipping = False
                self.offset = max(0, info.st_size - 4 * 1024 * 1024)
                self.identity = identity
                stream.seek(self.offset)
                if self.offset:
                    stream.readline()  # Skip an incomplete leading record.
                    self.offset = stream.tell()
            stream.seek(self.offset)
            consumed = 0
            while consumed < 4 * 1024 * 1024:
                position = stream.tell()
                line = stream.readline(1024 * 1024)
                if not line:
                    break
                if self.skipping or (len(line) == 1024 * 1024 and not line.endswith(b'\n')):
                    self.skipping = not line.endswith(b'\n')
                    self.calls.clear()  # A skipped output cannot support an exact count.
                    consumed += len(line)
                    self.offset = stream.tell()
                    continue
                if not line.endswith(b'\n'):
                    stream.seek(position)
                    break
                consumed += len(line)
                self.offset = stream.tell()
                try:
                    self.consume(json.loads(line))
                except (ValueError, TypeError):
                    continue


def timestamp(value):
    try:
        parsed = dt.datetime.fromisoformat(value.replace('Z', '+00:00'))
        return parsed.timestamp()
    except (AttributeError, ValueError, TypeError):
        return None


def linked_child(database, child, parent):
    """Validate native ancestry before reading child status; rollout references are data."""
    visited = set()
    for _ in range(8):
        if child in visited:
            return False
        visited.add(child)
        row = database.execute('SELECT source FROM threads WHERE id=? AND archived=0', (child,)).fetchone()
        if not row:
            return False
        try:
            source = json.loads(row['source'])
            ancestor = source['subagent']['thread_spawn']['parent_thread_id']
        except (ValueError, TypeError, KeyError):
            return False
        if ancestor == parent:
            return True
        child = ancestor
    return False


def status_line(status, rollout, children, now):
    active_children = sum(value.get('type') == 'active' for value in children)
    kind = status.get('type', 'unknown')
    active = kind == 'active' or active_children > 0
    if not active:
        return None
    flags = status.get('activeFlags', [])
    if 'waitingOnApproval' in flags:
        summary = 'Waiting for approval'
    elif 'waitingOnUserInput' in flags:
        summary = 'Waiting for your answer'
    elif active_children and (kind != 'active' or 'agents' in rollout.calls.values()):
        summary = f'Waiting for {active_children} agent(s)'
    elif rollout.calls:
        summary = f'{len(rollout.calls)} tool call(s) active'
        if active_children:
            summary += f' · {active_children} agent(s) active'
    elif active_children:
        summary = f'Working · {active_children} agent(s) active'
    else:
        summary = 'Working'
    started = timestamp(rollout.started)
    elapsed = max(0, int(now - started)) if started is not None else 0
    duration = f'{elapsed // 60}m' if elapsed >= 60 else f'{elapsed}s'
    updated = dt.datetime.fromtimestamp(now, dt.timezone.utc).strftime('%H:%M:%S UTC')
    return f'⏳ {summary} · {duration} · updated {updated}'


class Telegram:
    def __init__(self, config):
        token_path = pathlib.Path(config['bot_token_file'])
        descriptor = os.open(token_path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
        with os.fdopen(descriptor) as stream:
            info = os.fstat(stream.fileno())
            if not stat.S_ISREG(info.st_mode) or info.st_uid != os.getuid() or info.st_mode & 0o077:
                raise ValueError('token file must be owner-only and regular')
            self.token = stream.read(4096).strip()
            if not self.token or len(self.token) >= 4096:
                raise ValueError('invalid bounded token file')
        self.base = config.get('api_base', 'https://api.telegram.org').rstrip('/')
        self.next_chat = {}

    def call(self, method, payload):
        if method not in ('sendMessage', 'editMessageText', 'deleteMessage'):
            raise ValueError('observer Telegram method forbidden')
        chat = payload['chat_id']
        now = time.monotonic()
        time.sleep(max(0, self.next_chat.get(chat, now) - now))
        self.next_chat[chat] = time.monotonic() + (3.5 if chat < 0 else 1)
        request = urllib.request.Request(f'{self.base}/bot{self.token}/{method}',
                                         data=json.dumps(payload).encode(),
                                         headers={'Content-Type': 'application/json'})
        try:
            with urllib.request.urlopen(request, timeout=8) as response:
                value = json.load(response)
        except urllib.error.HTTPError as error:
            try:
                value = json.loads(error.read(65536))
            except ValueError:
                value = {'ok': False}
        except (OSError, ValueError):
            return None  # Unknown delivery; do not log the credential-bearing URL.
        return value


class Publisher:
    def __init__(self, telegram, path):
        self.telegram, self.path = telegram, path
        self.entries = json.loads(path.read_text()) if path.exists() else {}

    def save(self):
        atomic_json(self.path, self.entries)

    def update(self, key, chat, topic, turn, text, now):
        entry = self.entries.get(key)
        if entry and entry.get('turn') != turn and not entry.get('message_id'):
            entry = None  # A new native turn does not replay the old status send.
        if text is None:
            if entry:
                self.retire(key, entry)
            return
        if entry is None:
            entry = {'chat_id': chat, 'topic': topic, 'turn': turn, 'attempted': False}
            self.entries[key] = entry
        warning = text.startswith('⚠️')
        if entry.get('unconfirmed', False) != warning:
            entry['next_update'] = 0
        entry['unconfirmed'] = warning
        if now < entry.get('next_update', 0):
            return
        entry['next_update'] = now + 30
        payload = {'chat_id': chat, 'text': html.escape(text), 'parse_mode': 'HTML'}
        message_id = entry.get('message_id')
        if message_id:
            entry['turn'] = turn
            payload['message_id'] = message_id
            result = self.telegram.call('editMessageText', payload)
        elif not entry['attempted']:
            entry['attempted'] = True
            self.save()  # Persist before crossing the Telegram boundary.
            payload['disable_notification'] = True
            if topic:
                payload['message_thread_id'] = topic
            result = self.telegram.call('sendMessage', payload)
            if result and result.get('ok'):
                message = result.get('result', {})
                if message.get('chat', {}).get('id') == chat and message.get('message_thread_id', 0) == topic:
                    entry['message_id'] = message['message_id']
                    entry['turn'] = turn
                else:
                    entry['binding_error'] = True
        else:
            return  # An ambiguous send is never automatically duplicated.
        if result and not result.get('ok'):
            code = result.get('error_code')
            if code == 429:
                entry['next_update'] = now + max(30, result.get('parameters', {}).get('retry_after', 30))
                if not message_id:
                    entry['attempted'] = False  # Explicitly rejected, not ambiguous.
            elif message_id and code == 400 and 'message to edit not found' in result.get('description', '').lower():
                entry.pop('message_id')
                entry['attempted'] = False
        self.save()

    def retire(self, key, entry):
        if time.time() < entry.get('cleanup_retry', 0):
            return
        entry['cleanup_retry'] = time.time() + 30
        self.save()
        message_id = entry.get('message_id')
        if message_id:
            payload = {'chat_id': entry['chat_id'], 'message_id': message_id}
            result = self.telegram.call('deleteMessage', payload)
            absent = result and result.get('error_code') == 400 and 'message to delete not found' in result.get('description', '').lower()
            if (not result or not result.get('ok')) and not absent:
                # Keep failed cleanup tracked across restart; never leave "Working".
                payload.update(text='◻️ No active work · activity monitoring ended', parse_mode='HTML')
                result = self.telegram.call('editMessageText', payload)
                if not result or not result.get('ok'):
                    return
        self.entries.pop(key, None)
        self.save()


def run(args):
    config = tomllib.loads(pathlib.Path(args.config).read_text())
    state = pathlib.Path(args.state_dir)
    state.mkdir(parents=True, exist_ok=True, mode=0o700)
    if state.stat().st_uid != os.getuid() or state.stat().st_mode & 0o077:
        raise ValueError('activity state directory must be owner-only')
    lock = open(state / 'observer.lock', 'a', opener=lambda p, f: os.open(p, f, 0o600))
    fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
    bridge = readonly(config['db_path'])
    native_home = pathlib.Path(os.environ.get('CODEX_HOME', pathlib.Path.home() / '.codex'))
    native_db = readonly(native_home / 'state_5.sqlite')
    publisher = None if args.probe else Publisher(Telegram(config['telegram']), state / 'messages.json')
    stopped = threading.Event()
    for signum in (signal.SIGINT, signal.SIGTERM):
        signal.signal(signum, lambda *_: stopped.set())
    readers, native = {}, None
    try:
        while not stopped.is_set():
            observations = []
            try:
                if native is None:
                    native = Native(str(config['codex'].get('binary', 'codex')))
                topics = bridge.execute('''SELECT s.chat_id,s.thread_id,s.codex_thread_id,
                    (SELECT status FROM turns WHERE session_id=s.id ORDER BY id DESC LIMIT 1) AS turn_status
                    FROM sessions s JOIN users u ON u.tg_user_id=s.creator_user_id
                    WHERE s.codex_thread_id IS NOT NULL AND u.allowed=1''').fetchall()
                known = set()
                for topic in topics:
                    tid = topic['codex_thread_id']
                    key = f"{topic['chat_id']}:{topic['thread_id']}"
                    known.add(key)
                    row = native_db.execute('SELECT rollout_path FROM threads WHERE id=? AND archived=0', (tid,)).fetchone()
                    if not row:
                        if publisher and key in publisher.entries:
                            publisher.retire(key, publisher.entries[key])
                        continue
                    path = pathlib.Path(row['rollout_path']).resolve()
                    if not path.is_relative_to((native_home / 'sessions').resolve()):
                        raise ValueError('rollout outside native session directory')
                    reader = readers.setdefault(path, Rollout(path, tid))
                    try:
                        reader.refresh()
                    except (OSError, ValueError):
                        if publisher and key in publisher.entries:
                            publisher.update(key, topic['chat_id'], topic['thread_id'], reader.turn_id,
                                '⚠️ Native activity unavailable · rollout unconfirmed', time.time())
                        continue
                    status = native.status(tid)
                    children = [native.status(child) for child in sorted(reader.agents)[:32]
                                if linked_child(native_db, child, tid)]
                    now = time.time()
                    text = status_line(status, reader, children, now)
                    if status.get('type') in ('unknown', 'systemError') and publisher and key in publisher.entries:
                        text = '⚠️ Native activity unavailable · state unconfirmed'
                    if text is None and status.get('type') == 'idle' and topic['turn_status'] == 'running':
                        text = '⏳ Finishing delivery · native work ended'
                    # Fast turns stay clean; existing active turns qualify immediately.
                    started = timestamp(reader.started)
                    if text and started is not None and now - started < 8:
                        text = None
                    observations.append({'status': status.get('type'), 'visible': bool(text)})
                    if publisher:
                        publisher.update(key, topic['chat_id'], topic['thread_id'], reader.turn_id, text, now)
                if publisher:
                    for key in list(publisher.entries):
                        if key not in known:
                            publisher.retire(key, publisher.entries[key])
                atomic_json(state / 'health.json', {'updated_at': time.time(), 'connected': True,
                            'topics': len(observations), 'active': sum(x['visible'] for x in observations),
                            'published': sum(bool(x.get('message_id')) for x in publisher.entries.values()) if publisher else 0})
                if args.probe:
                    print(json.dumps({'connected': True, 'topics': observations}))
                    return
            except (OSError, ValueError, sqlite3.Error, queue.Empty, TimeoutError):
                if native:
                    native.close()
                native = None
                if publisher:
                    for key, entry in list(publisher.entries.items()):
                        publisher.update(key, entry['chat_id'], entry['topic'], entry['turn'],
                                         '⚠️ Native connection unavailable · activity unconfirmed', time.time())
                atomic_json(state / 'health.json', {'updated_at': time.time(), 'connected': False})
                if args.probe:
                    raise RuntimeError('native activity probe unavailable') from None
            stopped.wait(8)
    finally:
        if native:
            native.close()
        if publisher:
            for key, entry in list(publisher.entries.items()):
                publisher.retire(key, entry)
        bridge.close()
        native_db.close()
        lock.close()


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--config', required=True)
    parser.add_argument('--state-dir', required=True)
    parser.add_argument('--probe', action='store_true', help='read-only; no token or Telegram calls')
    try:
        run(parser.parse_args())
    except Exception as error:
        # Exception strings can contain private filenames or credential-bearing URLs.
        raise SystemExit(f'activity observer failed ({type(error).__name__})') from None
