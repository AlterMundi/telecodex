"""Receiving-owned metadata and authenticated owner-local conversation controls.

No bot update consumer, independent model client or arbitrary process kill.
"""
import hashlib
from contextlib import closing
import json
import os
from pathlib import Path
import socket
import stat
import struct
import threading
import time
import tomllib

from scripts.activity_indicator import Native, Rollout, linked_child, readonly, timestamp
from scripts.speech_render import sync_directory


class ProviderUnavailable(Exception):
    pass


def receiving_authority(receiver):
    """Only dependencies that select this dashboard's actor/data/control scope."""
    telegram = receiver.get('telegram', {})
    return json.dumps({
        'db_path': receiver.get('db_path'),
        'session_control': receiver.get('session_control'),
        'interaction': receiver.get('interaction'),
        'voice': receiver.get('voice'),
        'telegram': {key: telegram.get(key) for key in
                     ('api_base', 'bot_token', 'bot_token_file')},
        'native_binary': receiver.get('codex', {}).get('binary', 'codex'),
    }, sort_keys=True, separators=(',', ':')).encode()


def control(path, nonce, command='all'):
    """One bounded connect-only request; a lost acknowledgement is not retried."""
    meta = Path(path).lstat()
    parent = Path(path).parent.lstat()
    if (not stat.S_ISSOCK(meta.st_mode) or meta.st_uid != os.getuid()
            or meta.st_mode & 0o077 or not stat.S_ISDIR(parent.st_mode)
            or parent.st_uid != os.getuid() or parent.st_mode & 0o077):
        raise ProviderUnavailable('Receiving control socket unavailable')
    with socket.socket(socket.AF_UNIX) as stream:
        stream.settimeout(12)
        stream.connect(str(path))
        _, uid, _ = struct.unpack('3i', stream.getsockopt(socket.SOL_SOCKET, socket.SO_PEERCRED, 12))
        if uid != os.getuid():
            raise ProviderUnavailable('Receiving control owner mismatch')
        stream.sendall(json.dumps({'command': command, 'nonce': nonce}).encode() + b'\n')
        data = bytearray()
        while b'\n' not in data:
            block = stream.recv(min(4096, 262145 - len(data)))
            if not block or len(data) + len(block) > 262144:
                raise ProviderUnavailable('Receiving operation acknowledgement unavailable')
            data.extend(block)
        reply = json.loads(data)
        if reply.get('ok') is not True:
            raise ProviderUnavailable('Receiving operation was not confirmed; inspect its status')
        return reply['result']


class Host:
    def __init__(self, storage):
        self.storage = storage
        self.previous = None

    def sample(self):
        now = time.monotonic()
        result = {'observed_at': time.time(), 'cpu_percent': None,
                  'network_rx_per_second': None, 'network_tx_per_second': None}
        try:
            cpu = list(map(int, Path('/proc/stat').read_text().splitlines()[0].split()[1:9]))
            # Guest time is already included in user/nice; do not count it twice.
            net = [line.split(':', 1) for line in Path('/proc/net/dev').read_text().splitlines()[2:]]
            rx = sum(int(values.split()[0]) for name, values in net if name.strip() != 'lo')
            tx = sum(int(values.split()[8]) for name, values in net if name.strip() != 'lo')
            current = now, sum(cpu), cpu[3] + cpu[4], rx, tx
            if self.previous:
                at, total, idle, prior_rx, prior_tx = self.previous
                elapsed = now - at
                if current[1] > total:
                    result['cpu_percent'] = round(100 * (1 - (current[2] - idle) / (current[1] - total)), 1)
                if elapsed > 0 and rx >= prior_rx and tx >= prior_tx:
                    result['network_rx_per_second'] = round((rx - prior_rx) / elapsed)
                    result['network_tx_per_second'] = round((tx - prior_tx) / elapsed)
            self.previous = current
            memory = {parts[0].rstrip(':'): int(parts[1]) * 1024
                      for line in Path('/proc/meminfo').read_text().splitlines()
                      if len(parts := line.split()) >= 2}
            result['memory_total'] = memory['MemTotal']
            result['memory_available'] = memory['MemAvailable']
            result['load_average'] = list(os.getloadavg())
            result['cpu_count'] = os.cpu_count()
            result['uptime_seconds'] = int(float(Path('/proc/uptime').read_text().split()[0]))
        except (OSError, ValueError, KeyError):
            result['metrics_unavailable'] = True
        result['storage'] = []
        for label, path in self.storage.items():
            try:
                disk = os.statvfs(path)
                result['storage'].append({'label': label, 'total': disk.f_blocks * disk.f_frsize,
                                          'available': disk.f_bavail * disk.f_frsize})
            except OSError:
                result['storage'].append({'label': label, 'unavailable': True})
        # Aggregate counters may count VPN/virtual links too; this is not ISP usage.
        result['network_scope'] = 'Aggregate non-loopback interfaces; virtual links may count traffic twice'
        return result


class Provider:
    def __init__(self, config):
        self.config = config
        self.receiver_path = Path(config['receiver_config'])
        self.receiver_bytes = self.receiver_path.read_bytes()
        self.receiver = tomllib.loads(self.receiver_bytes.decode())
        self.binding_bytes = receiving_authority(self.receiver)
        self.human = int(self.receiver['session_control']['human_id'])
        self.native_home = Path(config.get('native_home', Path.home() / '.codex'))
        self.native = None
        self.lock = threading.Lock()
        self.cache = None
        self.readers = {}
        self.host = Host(config.get('storage', {'Host': '/'}))

    def assert_binding(self):
        try:
            current = receiving_authority(tomllib.loads(self.receiver_path.read_text()))
        except (OSError, ValueError):
            raise ProviderUnavailable('Receiving configuration unavailable') from None
        if current != self.binding_bytes:
            raise ProviderUnavailable('Receiving grant changed; operator must requalify the dashboard')

    def close(self):
        if self.native:
            self.native.close()
            self.native = None

    def authorized(self, human):
        try:
            self.assert_binding()
        except (OSError, ProviderUnavailable):
            return False
        with closing(readonly(self.receiver['db_path'])) as db:
            row = db.execute('SELECT allowed FROM users WHERE tg_user_id=?', (human,)).fetchone()
            return human == self.human and bool(row and row[0])

    def bindings(self):
        self.assert_binding()
        with closing(readonly(self.receiver['db_path'])) as db:
            rows = db.execute('''SELECT s.id,s.chat_id,s.thread_id,s.session_title,s.codex_thread_id,
                s.busy,s.updated_at FROM sessions s JOIN users u ON u.tg_user_id=s.creator_user_id
                WHERE s.creator_user_id=? AND u.allowed=1 ORDER BY s.id LIMIT 129''', (self.human,)).fetchall()
            if len(rows) > 128:
                raise ProviderUnavailable('Session scope exceeds qualified bound')
            return [dict(row) for row in rows]

    def presentation(self, command):
        """Use the configured controller and verify its returned entry provenance."""
        self.assert_binding()
        profile = self.receiver.get('interaction', {})
        if profile.get('human_id') != self.human or not self.authorized(self.human):
            raise ProviderUnavailable('Interaction presentation is not configured for this account')
        result = control(self.receiver['session_control']['socket_path'], None, command)
        self.assert_binding()
        if not self.authorized(self.human):
            raise ProviderUnavailable('Receiving presentation grant changed')
        items = result.get('items', []) if result.get('schema') == 'telecodex.presentation-list/v1' else [result]
        expected = {'chat_id':profile['chat_id'], 'thread_id':profile['topic_id']}
        for item in items:
            selected = item.get('entry_binding', [])
            if len(selected) != 4 or selected[0] != self.human or selected[1] != expected:
                raise ProviderUnavailable('Loaded integrating entry does not match the receiving configuration')
        return result

    def presentation_list(self, delivery, native, direction='after'):
        if type(delivery) is not int or type(native) is not int or min(delivery, native) < 0:
            raise ValueError('Invalid presentation cursor')
        if direction not in ('after', 'before'):
            raise ValueError('Invalid presentation direction')
        operation = 'before' if direction == 'before' else 'list'
        return self.presentation(f'presentation {operation} {delivery} {native}')

    def presentation_part(self, source, revision, offset):
        import re
        if (not isinstance(source, str) or not re.fullmatch(r'(?:delivery:[a-f0-9-]{36}|native:[1-9][0-9]{0,18})', source)
                or not isinstance(revision, str) or not re.fullmatch(r'[a-f0-9]{64}', revision)
                or type(offset) is not int or not 0 <= offset <= 10**9):
            raise ValueError('Invalid presentation selection')
        return self.presentation(f'presentation part {source} {revision} {offset}')

    @staticmethod
    def input_id(ident):
        import uuid
        if not isinstance(ident, str) or str(uuid.UUID(ident)) != ident:
            raise ValueError('Invalid conversation input identity')
        return ident

    def conversation_inputs(self, before=0, direction='before'):
        if type(before) is not int or not 0 <= before <= 2**63 - 1:
            raise ValueError('Invalid input history cursor')
        if direction not in ('before', 'after'):
            raise ValueError('Invalid input history direction')
        return self.presentation(f'input list {before} {direction}')

    def conversation_input_status(self, ident):
        return self.presentation(f'input status {self.input_id(ident)}')

    def conversation_input_submit(self, body):
        if not isinstance(body, dict) or set(body) != {'binding', 'id', 'text'}:
            raise ValueError('Select the conversation and provide one original message')
        self.input_id(body['id'])
        import re
        if (not isinstance(body['binding'], str) or not re.fullmatch('[a-f0-9]{64}', body['binding'])
                or not isinstance(body['text'], str) or not body['text'].strip()
                or '\0' in body['text'] or len(body['text'].encode('utf-8')) > 32768):
            raise ValueError('A message must contain text and fit within 32 KiB')
        # The Rust journal owns idempotency and exact-source checks. No retry.
        return self.presentation('input submit ' + json.dumps(body, ensure_ascii=False))

    def conversation_voice_submit(self, metadata, data):
        import re
        if not isinstance(metadata, dict) or set(metadata) != {'binding', 'id', 'audio_sha256', 'mime'}:
            raise ValueError('Select one original voice recording')
        self.input_id(metadata['id'])
        if (not isinstance(data, bytes) or not 0 < len(data) <= 20*1024*1024
                or any(not isinstance(metadata[k], str) or not re.fullmatch('[a-f0-9]{64}', metadata[k]) for k in ('binding', 'audio_sha256'))
                or metadata['mime'] not in ('audio/webm', 'audio/webm;codecs=opus', 'audio/ogg', 'audio/ogg;codecs=opus', 'audio/mp4', 'audio/wav', 'audio/mpeg')
                or hashlib.sha256(data).hexdigest() != metadata['audio_sha256']):
            raise ValueError('Invalid bounded voice recording')
        self.assert_binding()
        if not self.receiver.get('voice') or not self.authorized(self.human):
            raise ProviderUnavailable('Voice input unavailable')
        directory = Path(str(self.receiver['db_path'])+'.voice')
        directory.mkdir(mode=0o700, parents=False, exist_ok=True)
        info = directory.lstat()
        if not stat.S_ISDIR(info.st_mode) or info.st_uid != os.getuid() or info.st_mode & 0o077:
            raise ProviderUnavailable('Voice recording storage unavailable')
        # Persist the newly created directory name before retaining any clip/capture.
        sync_directory(directory.parent)
        import secrets
        temporary = directory / ('.'+secrets.token_hex(16))
        destination = directory / metadata['id']
        try:
            fd = os.open(temporary, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
            with os.fdopen(fd, 'wb') as stream:
                stream.write(data); stream.flush(); os.fsync(stream.fileno())
            try:
                os.link(temporary, destination, follow_symlinks=False)
            except FileExistsError:
                fd = os.open(destination, os.O_RDONLY | os.O_NOFOLLOW)
                with os.fdopen(fd, 'rb') as stream:
                    info = os.fstat(stream.fileno())
                    if not stat.S_ISREG(info.st_mode) or info.st_uid != os.getuid() or info.st_mode & 0o077:
                        raise ProviderUnavailable('Voice recording storage changed')
                    if hashlib.sha256(stream.read(20*1024*1024+1)).hexdigest() != metadata['audio_sha256']:
                        raise ValueError('Original voice identity cannot be replaced')
            fd = os.open(directory, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
            try: os.fsync(fd)
            finally: os.close(fd)
        finally:
            temporary.unlink(missing_ok=True)
        # The controller reserves original order/authority before asynchronous ASR.
        # A retry names the same clip and only inspects existing capture state.
        return self.presentation('input voice submit '+json.dumps(metadata))

    @staticmethod
    def request_id(ident):
        import re
        if not isinstance(ident, str) or not re.fullmatch('[a-f0-9]{32}', ident):
            raise ValueError('Invalid native request identity')
        return ident

    def conversation_requests(self, after=0):
        if type(after) is not int or not 0 <= after <= 2**63 - 1:
            raise ValueError('Invalid native request cursor')
        return self.presentation(f'requests list {after}')

    def conversation_request_detail(self, ident, offset=0, revision=None):
        import re
        self.request_id(ident)
        if type(offset) is not int or not 0 <= offset <= 10**9:
            raise ValueError('Invalid request text offset')
        if revision is not None and (not isinstance(revision, str) or not re.fullmatch('[a-f0-9]{64}', revision)):
            raise ValueError('Invalid request revision')
        if offset and revision is None:
            raise ValueError('Select the request revision before continuing')
        suffix = ' ' + revision if revision is not None else ''
        return self.presentation(f'requests detail {ident} {offset}{suffix}')

    def conversation_decision_receipt(self, ident, revision):
        import re
        self.request_id(ident)
        if not isinstance(revision, str) or not re.fullmatch('[a-f0-9]{64}', revision):
            raise ValueError('Invalid decision revision')
        return self.presentation(f'requests receipt {ident} {revision}')

    def conversation_request_decide(self, ident, body):
        import re
        self.request_id(ident)
        if not isinstance(body, dict) or set(body) != {'binding', 'revision', 'action'}:
            raise ValueError('Select one exact request decision')
        for key in ('binding', 'revision'):
            if not isinstance(body[key], str) or not re.fullmatch('[a-f0-9]{64}', body[key]):
                raise ValueError('Invalid request binding or revision')
        action = body['action']
        if not isinstance(action, dict):
            raise ValueError('Invalid request action')
        kind = action.get('kind')
        valid = (
            kind == 'answer' and set(action) == {'kind', 'text'}
            and isinstance(action['text'], str) and action['text'].strip()
            and '\0' not in action['text'] and len(action['text'].encode('utf-8')) <= 32768
            or kind == 'option' and set(action) == {'kind', 'index'}
            and type(action['index']) is int and 0 <= action['index'] < 10
            or kind == 'approval' and set(action) == {'kind', 'choice'} and action['choice'] in ('a','s','d','c')
            or kind == 'cancel' and set(action) == {'kind'})
        if not valid:
            raise ValueError('Invalid request action')
        return self.presentation('requests decide ' + json.dumps({**body, 'id':ident}, ensure_ascii=False))

    @staticmethod
    def identity(row):
        return hashlib.sha256(json.dumps([row['id'], row['chat_id'], row['thread_id'],
                                          row['codex_thread_id']]).encode()).hexdigest()[:24]

    def stop_status(self):
        with closing(readonly(self.receiver['db_path'])) as db:
            row = db.execute('''SELECT id,phase,document FROM stop_operations WHERE human=?
                AND phase!='superseded' ORDER BY rowid DESC LIMIT 1''', (self.human,)).fetchone()
            if not row:
                return {'state': 'no_operation'}
            op = json.loads(row['document'])
            return {'operation': row['id'], 'state': row['phase'],
                    'targets': [{'state': t['phase'], 'detail': t['reason']} for t in op['targets']],
                    'external_processes': 'Native inactivity does not certify host subprocess inactivity'}

    def snapshot(self):
        if not self.lock.acquire(blocking=False):
            raise ProviderUnavailable('Metadata refresh in progress; retry shortly')
        try:
            rows = self.bindings()
            scope = [(self.identity(row), row['session_title']) for row in rows]
            if self.cache and time.time() - self.cache['observed_at'] < 8 and self.cache['_scope'] == scope:
                return {key: value for key, value in self.cache.items() if not key.startswith('_')}
            started = time.monotonic()
            self.refresh_started = started
            sessions = []
            native_db = readonly(self.native_home / 'state_5.sqlite')
            try:
                if self.native is None:
                    self.native = Native(self.receiver['codex'].get('binary', 'codex'))
            except Exception:
                self.close()
            for row in rows:
                tid = row['codex_thread_id']
                node = {'id': self.identity(row), 'title': row['session_title'] or 'Untitled session',
                        'status': 'unknown', 'bridge_busy': bool(row['busy']), 'goal_state': 'unavailable',
                        'children': [], 'children_state': 'unavailable', 'elapsed_seconds': None}
                if row['chat_id'] == self.human and row['thread_id'] > 0:
                    username = self.config.get('bot_username')
                    if username:
                        node['topic_url'] = f"https://t.me/{username}"
                        node['topic_link_scope'] = 'bot_chat'
                elif str(row['chat_id']).startswith('-100') and row['thread_id'] > 0:
                    node['topic_url'] = f"https://t.me/c/{str(row['chat_id'])[4:]}/{row['thread_id']}"
                try:
                    if tid and self.native and time.monotonic() - started < 12:
                        self.describe(node, tid, native_db)
                except Exception:
                    self.close()
                sessions.append(node)
            native_db.close()
            # A binding changed during native reads must not leak a retired conversation.
            if scope != [(self.identity(row), row['session_title']) for row in self.bindings()]:
                raise ProviderUnavailable('Session bindings changed; refresh again')
            result = {'schema': 'telecodex.sessions-dashboard/v1', 'observed_at': time.time(),
                      'provider': self.config.get('label', 'This receiving bot'), 'sessions': sessions,
                      'host': self.host.sample(), 'stop': self.stop_status(), '_scope': scope}
            self.cache = result
            self.readers = {key: reader for key, reader in self.readers.items()
                            if key in {row['codex_thread_id'] for row in rows}}
            return {key: value for key, value in result.items() if not key.startswith('_')}
        finally:
            self.lock.release()

    def describe(self, node, tid, db):
        node['status'] = self.native.status(tid).get('type', 'unknown')
        response = self.native.rpc('thread/goal/get', {'threadId': tid})
        if response is not None and 'goal' in response:
            goal = response['goal']
            if goal is None:
                node['goal_state'] = 'none'
            elif goal.get('threadId') == tid:
                node['goal_state'] = 'known'
                node['goal'] = {key: goal.get(key) for key in ('status', 'tokensUsed', 'tokenBudget', 'timeUsedSeconds')}
                node['goal']['objective'] = str(goal.get('objective', ''))[:180]
            else:
                raise ProviderUnavailable('Goal binding mismatch')
        row = db.execute('SELECT rollout_path FROM threads WHERE id=? AND archived=0', (tid,)).fetchone()
        if row:
            path = Path(row['rollout_path']).resolve()
            if not path.is_relative_to((self.native_home / 'sessions').resolve()):
                raise ProviderUnavailable('Rollout outside receiving native sessions')
            reader = self.readers.get(tid)
            if reader is None or reader.path != path:
                reader = self.readers[tid] = Rollout(path, tid)
            reader.refresh()
            at = timestamp(reader.started)
            if node['status'] == 'active' and at is not None:
                node['elapsed_seconds'] = max(0, int(time.time() - at))
            node['tools_active'] = len(reader.calls) if node['status'] == 'active' else 0
            # Existing observer validates each referenced child's rooted native ancestry.
            frontier, seen, complete = [tid], {tid}, True
            for _ in range(8):
                if not frontier:
                    break
                next_frontier = []
                for parent in frontier:
                    candidates = db.execute('''SELECT id,agent_nickname FROM threads WHERE archived=0
                        AND json_extract(CASE WHEN json_valid(source) THEN source ELSE NULL END,
                        '$.subagent.thread_spawn.parent_thread_id')=? ORDER BY id LIMIT 33''', (parent,)).fetchall()
                    for candidate in candidates:
                        child = candidate['id']
                        if len(seen) > 32 or time.monotonic() - self.refresh_started >= 12:
                            complete = False
                            break
                        if child in seen or not linked_child(db, child, tid):
                            continue
                        seen.add(child)
                        next_frontier.append(child)
                        node['children'].append({'id': child, 'parent_id': parent,
                            'title': candidate['agent_nickname'] or 'Subagent',
                            'status': self.native.status(child).get('type', 'unknown')})
                frontier = next_frontier
            if frontier:
                complete = False
            node['native_id'] = tid
            node['children_state'] = 'observed_native_index' if complete else 'partial'
            node['children_limit'] = 'Receiving native index, up to 32 descendants and 8 levels; archived children excluded'

    def stop(self, nonce):
        self.assert_binding()
        if not self.authorized(self.human):
            raise ProviderUnavailable('Receiving owner grant no longer authorizes Stop')
        return control(self.receiver['session_control']['socket_path'], nonce)

    def resume(self, nonce, operation):
        self.assert_binding()
        if not self.authorized(self.human):
            raise ProviderUnavailable('Receiving owner grant no longer authorizes Resume')
        observed = self.stop_status()
        if observed.get('operation') != operation or observed.get('state') != 'stopped':
            raise ProviderUnavailable('Selected Stop is no longer the observed stopped operation')
        self.assert_binding()
        # The native coordinator rechecks owned targets, bindings and stopped state.
        return control(self.receiver['session_control']['socket_path'], nonce, 'resume ' + operation)
