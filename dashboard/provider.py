"""Receiving-owned metadata, using the existing observer and Stop protocol.

No bot update consumer, native mutations, model calls or arbitrary process kill.
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


class ProviderUnavailable(Exception):
    pass


def receiving_authority(receiver):
    """Only dependencies that select this dashboard's actor/data/control scope."""
    telegram = receiver.get('telegram', {})
    return json.dumps({
        'db_path': receiver.get('db_path'),
        'session_control': receiver.get('session_control'),
        'interaction': receiver.get('interaction'),
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
            block = stream.recv(min(4096, 65537 - len(data)))
            if not block or len(data) + len(block) > 65536:
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
        """Read the controller's canonical sources; never duplicate its freshness rules."""
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

    def presentation_list(self, delivery, native):
        if type(delivery) is not int or type(native) is not int or min(delivery, native) < 0:
            raise ValueError('Invalid presentation cursor')
        return self.presentation(f'presentation list {delivery} {native}')

    def presentation_part(self, source, revision, offset):
        import re
        if (not isinstance(source, str) or not re.fullmatch(r'(?:delivery:[a-f0-9-]{36}|native:[1-9][0-9]{0,18})', source)
                or not isinstance(revision, str) or not re.fullmatch(r'[a-f0-9]{64}', revision)
                or type(offset) is not int or not 0 <= offset <= 10**9):
            raise ValueError('Invalid presentation selection')
        return self.presentation(f'presentation part {source} {revision} {offset}')

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
