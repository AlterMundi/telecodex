"""Optional receiving-scoped Alerts client and deterministic delivery companion.

No native control, bot polling consumer, model call or producer recovery.
Durable send intents and confirmed receipts survive sidecar restarts.
"""
from contextlib import contextmanager
import hashlib
import json
import os
from pathlib import Path
import re
import secrets
import sqlite3
import stat
import threading
import time
import tomllib
import urllib.error
import urllib.parse
import urllib.request

from .alerts_contract import validate
from .private_state import private_directory


class AlertsUnavailable(Exception):
    """Safe codes only; never propagate credential-bearing network exceptions."""


def identifier(value):
    if not isinstance(value, str) or not re.fullmatch(r'[A-Za-z0-9_-]{1,128}', value):
        raise AlertsUnavailable('invalid_identifier')
    return value


def private_token(path):
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    with os.fdopen(fd) as stream:
        info = os.fstat(stream.fileno())
        if not stat.S_ISREG(info.st_mode) or info.st_uid != os.getuid() or info.st_mode & 0o077:
            raise AlertsUnavailable('unsafe_credential_reference')
        token = stream.read(4097).strip()
    if not token or len(token) > 4096 or not re.fullmatch(r'[!-~]+', token):
        raise AlertsUnavailable('invalid_credential_reference')
    return token


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, *args, **kwargs):
        return None


def opener():
    return urllib.request.build_opener(urllib.request.ProxyHandler({}), NoRedirect())


class Client:
    def __init__(self, config):
        self.base = config['base_url'].rstrip('/')
        parts = urllib.parse.urlsplit(self.base)
        if (parts.scheme != 'http' or parts.hostname not in ('127.0.0.1', '::1')
                or parts.username or parts.password or parts.query or parts.fragment
                or parts.path or not parts.port):
            raise ValueError('Alerts v1 requires a configured loopback HTTP endpoint')
        self.token_file = config['token_file']
        self.fingerprint = hashlib.sha256(private_token(self.token_file).encode()).hexdigest()
        self.opener = opener()

    def assert_binding(self):
        if hashlib.sha256(private_token(self.token_file).encode()).hexdigest() != self.fingerprint:
            raise AlertsUnavailable('credential_binding_changed')

    def call(self, method, path, shape, value=None):
        self.assert_binding()
        request = urllib.request.Request(self.base + path, method=method,
            data=json.dumps(value).encode() if value is not None else None,
            headers={'Authorization': 'Bearer ' + private_token(self.token_file),
                     'Content-Type': 'application/json'})
        try:
            with self.opener.open(request, timeout=5) as response:
                if response.status not in (200, 201):
                    raise AlertsUnavailable('operation_unconfirmed')
                raw = response.read(4 * 1024 * 1024 + 1)
                if len(raw) > 4 * 1024 * 1024:
                    raise AlertsUnavailable('response_too_large')
                result = validate(json.loads(raw), shape)
            return result
        except urllib.error.HTTPError as error:
            try:
                code = validate(json.loads(error.read(16384)), 'Error')['error']
            except (ValueError, KeyError, RecursionError, OSError):
                code = 'invalid_response'
            raise AlertsUnavailable(code) from None
        except (OSError, ValueError, RecursionError):
            raise AlertsUnavailable('service_unavailable') from None

    def collection(self, route, shape, **filters):
        items, seen, cursor = [], set(), None
        # Bounded full pagination; a truncated view is explicitly labelled.
        for _ in range(2):
            query = {**filters, 'limit': 100}
            if cursor is not None:
                query['cursor'] = cursor
            page = self.call('GET', route + '?' + urllib.parse.urlencode(query), shape)
            if len(page['items']) > 100:
                raise AlertsUnavailable('invalid_response')
            items.extend(page['items'])
            cursor = page['next_cursor']
            if cursor is None:
                return items, False
            if not re.fullmatch(r'[1-9][0-9]{0,19}', cursor) or cursor in seen:
                raise AlertsUnavailable('invalid_cursor')
            seen.add(cursor)
        return items, True


class Sender:
    def __init__(self, provider, config):
        self.provider, self.config = provider, config
        telegram = provider.receiver['telegram']
        self.token_file = telegram['bot_token_file']
        self.token = private_token(self.token_file)
        self.base = telegram.get('api_base', 'https://api.telegram.org').rstrip('/')
        parts = urllib.parse.urlsplit(self.base)
        if (parts.scheme not in ('http', 'https') or not parts.hostname
                or parts.username or parts.password or parts.query or parts.fragment
                or (parts.scheme == 'http' and parts.hostname not in ('127.0.0.1', '::1'))):
            raise AlertsUnavailable('unsafe_telegram_endpoint')
        self.opener = opener()
        self.verified = False

    def call(self, method, payload):
        self.provider.assert_binding()
        if private_token(self.token_file) != self.token:
            raise AlertsUnavailable('bot_binding_changed')
        request = urllib.request.Request(f'{self.base}/bot{self.token}/{method}',
            data=json.dumps(payload).encode(), headers={'Content-Type': 'application/json'})
        try:
            try:
                response = self.opener.open(request, timeout=8)
            except urllib.error.HTTPError as error:
                response = error
            with response:
                raw = response.read(65537)
                if len(raw) > 65536:
                    return None
                return json.loads(raw)
        except (OSError, ValueError, RecursionError):
            return None

    def verify(self):
        if not self.verified:
            response = self.call('getMe', {})
            bot = response.get('result', {}) if isinstance(response, dict) else {}
            if (response is None or response.get('ok') is not True
                    or type(bot.get('id')) is not int or bot['id'] != self.config['bot_id']
                    or bot.get('username') != self.config.get('bot_username')):
                raise AlertsUnavailable('receiving_bot_unconfirmed')
            self.verified = True

    def send(self, projection):
        resolved = projection['processing_state'] == 'resolved'
        text = (f"{'✅' if resolved else '⚠️'} {projection['producer_id']} · {projection['phase']}\n"
                f"{projection['summary']}\n{projection['action']}\n"
                f"Processing: {projection['processing_state']} · {projection['problem_code']}\n"
                + ('Work preserved' if projection['work_preserved'] else 'Work preservation unconfirmed'))
        response = self.call('sendMessage', {'chat_id': self.provider.human, 'text': text})
        if isinstance(response, dict):
            result = response.get('result', {})
            if (response.get('ok') is True and isinstance(result, dict)
                    and type(result.get('message_id')) is int and result['message_id'] > 0
                    and result.get('chat', {}).get('id') == self.provider.human):
                return {'outcome': 'delivered', 'message_ref': f"telegram-{result['message_id']}",
                        'destination_ref': f'telegram-{self.provider.human}'}
            if response.get('ok') is False and type(response.get('error_code')) is int and 400 <= response['error_code'] < 500:
                return {'outcome': 'failed'}
        return {'outcome': 'uncertain'}


class Receiver:
    def __init__(self, config, provider, client=None, sender=None):
        self.config, self.provider = config, provider
        settings = config['alerts']
        self.scope = identifier(settings['receiving_scope'])
        self.adapter = identifier(settings['adapter_id'])
        self.interval = settings.get('interval_seconds', 15)
        self.deliver = settings.get('deliver', False)
        if type(self.interval) is not int or not 5 <= self.interval <= 300 or type(self.deliver) is not bool:
            raise ValueError('Invalid Alerts receiving settings')
        self.client = client or Client(settings)
        self.sender = sender or (Sender(provider, config) if self.deliver else None)
        self.binding = hashlib.sha256(json.dumps([self.client.base, self.client.fingerprint,
            self.scope, self.adapter, config['bot_id'], provider.human,
            provider.binding_bytes.hex(), getattr(self.sender, 'token', '')], sort_keys=True).encode()).hexdigest()
        root = private_directory(Path(config['state_dir']) / 'alerts')
        self.db_path = root / 'delivery.sqlite3'
        if self.db_path.exists() or self.db_path.is_symlink():
            info = self.db_path.lstat()
            if not stat.S_ISREG(info.st_mode) or info.st_uid != os.getuid() or info.st_mode & 0o077:
                raise ValueError('Unsafe Alerts delivery journal')
        with self.database() as db:
            db.executescript('''CREATE TABLE IF NOT EXISTS binding(value TEXT PRIMARY KEY);
                CREATE TABLE IF NOT EXISTS attempts(delivery_id TEXT PRIMARY KEY,
                    request_id TEXT NOT NULL,state TEXT NOT NULL,receipt TEXT,updated INTEGER NOT NULL);''')
            previous = db.execute('SELECT value FROM binding').fetchone()
            if previous and previous[0] != self.binding:
                raise ValueError('Alerts receiving binding changed; preserve journal and requalify')
            db.execute('INSERT OR IGNORE INTO binding VALUES(?)', (self.binding,))
        self.db_path.chmod(0o600)
        self.closed = threading.Event()
        self.lock = threading.Lock()
        self.tick_lock = threading.Lock()
        self.thread = None
        self.view = {'state': 'checking', 'observed_at': None, 'incidents': [], 'observers': [],
                     'observer_state': 'unobserved', 'truncated': False, 'delivery_enabled': self.deliver}

    @contextmanager
    def database(self):
        db = sqlite3.connect(self.db_path, timeout=3)
        db.row_factory = sqlite3.Row
        db.execute('PRAGMA synchronous=FULL')
        try:
            with db:
                yield db
        finally:
            db.close()

    def authorized(self):
        self.provider.assert_binding()
        self.client.assert_binding()
        if not self.provider.authorized(self.provider.human):
            raise AlertsUnavailable('receiving_grant_unavailable')

    def snapshot(self):
        with self.lock:
            result = json.loads(json.dumps(self.view))
        if result['observed_at'] is not None and time.time() - result['observed_at'] > max(30, self.interval * 2):
            result['state'] = 'stale'
        with self.database() as db:
            result['local_deliveries'] = [dict(r) for r in db.execute(
                'SELECT delivery_id,state,updated FROM attempts ORDER BY updated DESC LIMIT 20')]
        return result

    def start(self):
        if self.thread is None:
            self.thread = threading.Thread(target=self.run, name='telecodex-alerts', daemon=True)
            self.thread.start()

    def close(self):
        self.closed.set()
        if self.thread:
            self.thread.join(timeout=2)

    def run(self):
        while not self.closed.is_set():
            self.tick()
            self.closed.wait(self.interval)

    def tick(self):
        if not self.tick_lock.acquire(blocking=False):
            return
        try:
            self.authorized()
            status = self.client.call('GET', '/v1/status', 'Status')
            records, truncated = self.client.collection('/v1/incidents', 'IncidentPage', processing_state='open')
            incidents = [r for r in records if r['projection']['receiving_scope'] == self.scope]
            observers = [r for r in status['observers'] if r['receiving_scope'] == self.scope]
            self.authorized()
            view = {'state': 'available', 'observed_at': time.time(), 'incidents': incidents,
                    'observers': observers, 'truncated': truncated, 'delivery_enabled': self.deliver,
                    'observer_state': ('unobserved' if not observers else 'degraded'
                        if any(r['freshness'] != 'fresh' or r['source_state'] != 'healthy' for r in observers)
                        else 'healthy')}
            with self.lock:
                self.view = view
            if self.deliver:
                self.sender.verify()
                self.reconcile()
                deliveries, _ = self.client.collection('/v1/deliveries', 'DeliveryPage',
                    adapter_id=self.adapter, state='pending')
                for delivery in deliveries[:10]:
                    if self.closed.is_set():
                        break
                    if delivery['receiving_scope'] == self.scope and delivery['adapter_id'] == self.adapter:
                        self.dispatch(delivery)
        except (AlertsUnavailable, OSError, ValueError, KeyError, sqlite3.Error) as error:
            code = str(error) if isinstance(error, AlertsUnavailable) else 'receiving_unavailable'
            with self.lock:
                self.view = {**self.view, 'state': 'unavailable', 'error': code}
        finally:
            self.tick_lock.release()

    def receipt(self, delivery_id, value):
        result = self.client.call('PUT', f'/v1/deliveries/{identifier(delivery_id)}/receipt', 'Delivery', value)
        if (result['delivery_id'] != delivery_id or result['receiving_scope'] != self.scope
                or result['adapter_id'] != self.adapter or result['receipt'] != value):
            raise AlertsUnavailable('receipt_binding_mismatch')
        with self.database() as db:
            db.execute('UPDATE attempts SET state=?,updated=? WHERE delivery_id=?',
                       ('receipted', int(time.time()), delivery_id))

    def reconcile(self):
        with self.database() as db:
            rows = db.execute("SELECT * FROM attempts WHERE receipt IS NOT NULL AND state!='receipted' LIMIT 10").fetchall()
        for row in rows:
            self.authorized()
            self.receipt(row['delivery_id'], json.loads(row['receipt']))

    def dispatch(self, delivery):
        self.authorized()
        value = delivery['projection']
        if (value['receiving_scope'] != self.scope or value['producer_id'] != delivery['producer_id']
                or value['incident_id'] != delivery['incident_id']):
            raise AlertsUnavailable('delivery_binding_mismatch')
        delivery_id, request_id = identifier(delivery['delivery_id']), secrets.token_hex(16)
        with self.database() as db:
            db.execute("DELETE FROM attempts WHERE state='receipted' AND updated<?", (int(time.time()) - 30 * 86400,))
            if db.execute('SELECT count(*) FROM attempts').fetchone()[0] >= 10000:
                raise AlertsUnavailable('local_capacity_exceeded')
            fresh = db.execute('INSERT OR IGNORE INTO attempts VALUES(?,?,?,?,?)',
                (delivery_id, request_id, 'claim_unconfirmed', None, int(time.time()))).rowcount == 1
        if not fresh:
            return  # Restart or a parallel receiver can never repeat the send.
        claim = self.client.call('POST', f'/v1/deliveries/{delivery_id}/claim', 'Claim', {'request_id': request_id})
        if claim['delivery_id'] != delivery_id or claim['request_id'] != request_id:
            raise AlertsUnavailable('claim_binding_mismatch')
        if not claim['dispatch_allowed']:
            with self.database() as db:
                db.execute('UPDATE attempts SET state=? WHERE delivery_id=?', ('not_dispatched', delivery_id))
            return
        if claim['state'] != 'uncertain':
            raise AlertsUnavailable('claim_binding_mismatch')
        # Both service and local intent are durable before touching Telegram.
        self.authorized()
        if self.closed.is_set():
            return
        result = {'request_id': request_id, **self.sender.send(value)}
        validate(result, 'ReceiptInput')
        with self.database() as db:
            db.execute('UPDATE attempts SET state=?,receipt=?,updated=? WHERE delivery_id=?',
                (result['outcome'], json.dumps(result), int(time.time()), delivery_id))
        self.receipt(delivery_id, result)

    def acknowledge(self, producer, incident, request_id):
        self.authorized()
        route = f'/v1/incidents/{identifier(producer)}/{identifier(incident)}'
        prior = self.client.call('GET', route, 'Incident')
        value = prior['projection']
        if value['receiving_scope'] != self.scope or value['producer_id'] != producer or value['incident_id'] != incident:
            raise AlertsUnavailable('incident_binding_mismatch')
        self.authorized()
        return self.client.call('PUT', route + '/acknowledgments/' + identifier(request_id), 'Acknowledgment', {})


def main():
    import argparse
    from .provider import Provider
    parser = argparse.ArgumentParser(description='Read-only receiving Alerts qualification; no Telegram send')
    parser.add_argument('--config', required=True)
    args = parser.parse_args()
    try:
        config = tomllib.loads(Path(args.config).read_text())
        config['alerts']['deliver'] = False
        receiver = Receiver(config, Provider(config))
        receiver.tick()
        result = receiver.snapshot()
        print(json.dumps({'state': result['state'], 'observer_state': result['observer_state'],
                          'open_incidents': len(result['incidents']), 'error': result.get('error')}))
    except (ValueError, KeyError, OSError, AlertsUnavailable, sqlite3.Error):
        raise SystemExit('Alerts receiving probe unavailable') from None


if __name__ == '__main__':
    main()
