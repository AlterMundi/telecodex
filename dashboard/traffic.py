"""Private bot traffic projection and deterministic, durable Alerts producer."""
from contextlib import closing
import hashlib
import json
import os
from pathlib import Path
import secrets
import sqlite3
import stat
import threading
import time
import tomllib

from scripts.bot_traffic import private_token as bot_fingerprint
from .alerts import Client, AlertsUnavailable, identifier, opener
from .private_state import private_directory, read_json, write_json
from .provider import ProviderUnavailable


def private_file(path):
    info = Path(path).lstat()
    if not stat.S_ISREG(info.st_mode) or info.st_uid != os.getuid() or info.st_mode & 0o077:
        raise ValueError('Owner-only traffic file required')


class Traffic:
    def __init__(self, config, provider, client=None, health=None):
        self.provider = provider
        self.settings = config['traffic']
        self.config_path = Path(self.settings['meter_config'])
        private_file(self.config_path)
        self.config_bytes = self.config_path.read_bytes()
        self.meter = tomllib.loads(self.config_bytes.decode())
        self.fingerprint = bot_fingerprint(provider.receiver['telegram']['bot_token_file']).hex()
        self.endpoint = provider.receiver['telegram']['api_base'].rstrip('/')
        if self.endpoint != 'http://127.0.0.1:' + str(self.meter['port']):
            raise ValueError('Traffic endpoint must match receiving bot endpoint')
        if bot_fingerprint(self.meter['token_file']).hex() != self.fingerprint:
            raise ValueError('Traffic bot must match receiving bot')
        self.path = Path(self.meter['state_dir']) / 'traffic.sqlite3'
        self.interval = self.settings.get('interval_seconds', 10)
        if type(self.interval) is not int or not 5 <= self.interval <= 60:
            raise ValueError('Traffic observation interval must be 5-60 seconds')
        self.client = client
        self.producer = self.scope = self.observer = None
        publish = self.settings.get('alerts', {})
        if publish.get('enabled') is True:
            self.client = client or Client(publish)
            self.producer = identifier(publish['producer_id'])
            self.scope = identifier(publish['receiving_scope'])
            self.observer = identifier(publish['observer_id'])
            if self.scope != config.get('alerts', {}).get('receiving_scope'):
                raise ValueError('Traffic Alerts scope must match Sessions receiving scope')
        root = private_directory(private_directory(config['state_dir']) / 'traffic')
        self.state_path = root / 'producer.json'
        binding = hashlib.sha256(json.dumps([str(self.config_path), self.config_bytes.hex(),
            self.fingerprint, provider.binding_bytes.hex(), self.producer, self.scope,
            self.observer, getattr(self.client, 'base', None), getattr(self.client, 'fingerprint', None)],
            sort_keys=True).encode()).hexdigest()
        if self.state_path.exists():
            self.state = read_json(self.state_path)
            self.state.setdefault('transport_failed', {})
            if self.state['binding'] != binding:
                raise ValueError('Traffic producer binding changed; retain state and requalify')
        else:
            self.state = {'binding': binding, 'blocked': {}, 'transport_failed': {}, 'incidents': {},
                          'dropped': 0, 'loss_at': None, 'failures': 0, 'outbox': []}
            self.save()
        self.health = health or self.read_health
        self.lock = threading.Lock()
        self.closed = threading.Event()
        self.thread = None
        self.view = {'state': 'checking', 'observed_at': None}

    def save(self):
        write_json(self.state_path, self.state)

    def assert_binding(self):
        self.provider.assert_binding()
        if (self.config_path.read_bytes() != self.config_bytes or
                bot_fingerprint(self.meter['token_file']).hex() != self.fingerprint):
            raise ValueError('Traffic receiving binding changed')
        if not self.provider.authorized(self.provider.human):
            raise ProviderUnavailable('Traffic receiving grant unavailable')

    def read_health(self):
        try:
            with opener().open(self.endpoint + '/healthz', timeout=2) as response:
                value = json.loads(response.read(1024))
                if (response.status == 200 and value.get('ok') is True
                        and type(value.get('dropped_records')) is int and value['dropped_records'] >= 0):
                    return value
        except (OSError, ValueError):
            pass
        return None

    def capture(self):
        now = time.time()
        self.assert_binding()
        private_file(self.path)
        with closing(sqlite3.connect(self.path.resolve().as_uri()+'?mode=ro', uri=True, timeout=1)) as db:
            db.row_factory = sqlite3.Row
            db.execute('PRAGMA query_only=ON')
            if db.execute("SELECT value FROM settings WHERE key='bot_fingerprint'").fetchone()[0] != self.fingerprint:
                raise ValueError('Traffic journal bot mismatch')
            db.execute('BEGIN')
            settings = dict(db.execute('SELECT key,value FROM settings'))
            def query(sql, parameters=()):
                return [dict(r) for r in db.execute(sql, parameters)]
            total = query('''SELECT COUNT(*) AS requests,COALESCE(SUM(request_bytes),0) AS request_bytes,
                COALESCE(SUM(response_bytes),0) AS response_bytes,
                COALESCE(SUM(CASE WHEN http_status=429 OR api_error=429 THEN 1 ELSE 0 END),0) AS rate_limited,
                COALESCE(SUM(outcome='in_flight'),0) AS in_flight,
                COALESCE(SUM(CASE WHEN http_status>=500 OR outcome='upstream_unknown' THEN 1 ELSE 0 END),0) AS failures,
                COALESCE(SUM(source='unknown'),0) AS unknown_source,
                COALESCE(SUM(topic_id IS NULL),0) AS unknown_topic,
                COALESCE(SUM(metadata_complete=0),0) AS incomplete,
                MAX(finished) AS last_completed FROM requests WHERE started>=?''', (now-86400,))[0]
            sources = query('''SELECT source,method,COUNT(*) AS requests,
                SUM(CASE WHEN http_status=429 OR api_error=429 THEN 1 ELSE 0 END) AS rate_limited
                FROM requests WHERE started>=? GROUP BY source,method ORDER BY COUNT(*) DESC LIMIT 20''', (now-86400,))
            minute = query('''SELECT CAST(started/60 AS INTEGER)*60 AS at,COUNT(*) AS requests,
                SUM(CASE WHEN http_status=429 OR api_error=429 THEN 1 ELSE 0 END) AS rate_limited
                FROM requests WHERE started>=? GROUP BY CAST(started/60 AS INTEGER) ORDER BY 1''', (now-3600,))
            recent = db.execute('SELECT COUNT(*) FROM requests WHERE started>=?', (now-300,)).fetchone()[0]
            topic_counts = query('''SELECT chat_id,topic_id,COUNT(*) AS requests,
                SUM(CASE WHEN http_status=429 OR api_error=429 THEN 1 ELSE 0 END) AS rate_limited
                FROM requests WHERE started>=? GROUP BY chat_id,topic_id ORDER BY COUNT(*) DESC''', (now-86400,))
            limited = query('''SELECT method,MAX(finished) AS failed_at,MAX(finished+retry_after) AS until
                FROM requests WHERE (http_status=429 OR api_error=429) AND finished IS NOT NULL GROUP BY method''')
            failed = query("""SELECT method,MAX(finished) AS failed_at FROM requests
                WHERE finished>=? AND (http_status>=500 OR outcome='upstream_unknown')
                GROUP BY method HAVING COUNT(*)>=3""", (now-300,))
            successes = {r['method']: r['succeeded_at'] for r in query('''SELECT method,MAX(finished) AS succeeded_at
                FROM requests WHERE http_status BETWEEN 200 AND 299 AND api_error IS NULL
                AND outcome='response' AND response_metadata_complete=1 GROUP BY method''')}
        # Receiving-owned labels only. No raw chat/topic IDs or unrelated titles leave this projection.
        names = {(r['chat_id'], r['thread_id']): r['session_title'] or 'Untitled conversation'
                 for r in self.provider.bindings()}
        topics = [{'name': names[(r['chat_id'], r['topic_id'])], 'requests': r['requests'],
                   'rate_limited': r['rate_limited']} for r in topic_counts if (r['chat_id'], r['topic_id']) in names][:12]
        health = self.health()
        dropped = max(int(settings.get('dropped_records', 0)), health['dropped_records'] if health else 0)
        return {'state': 'available' if health else 'degraded', 'observed_at': now,
                'capture_started': float(settings['capture_started']), 'totals': total,
                'requests_last_5m': recent, 'sources': sources, 'topics': topics, 'minutes': minute,
                'dropped_records': dropped, 'meter_healthy': bool(health),
                '_limited': limited, '_successes': successes, '_failed': failed}

    def cooldown(self, value):
        blocked = self.state['blocked']
        for row in value['_limited']:
            previous = blocked.get(row['method'])
            if not previous or row['failed_at'] > previous['failed_at']:
                blocked[row['method']] = {'failed_at': row['failed_at'], 'until': row['until']}
        for method, row in list(blocked.items()):
            if value['_successes'].get(method, 0) > row['failed_at']:
                del blocked[method]
        until = max((r['until'] or 0 for r in blocked.values()), default=0)
        return {'state': 'active' if until > time.time() else 'awaiting_recovery' if blocked else 'none',
                'until': until or None, 'methods': sorted(blocked)}

    def transition(self, key, active, code, summary, action):
        if not self.producer:
            return
        prior = self.state['incidents'].get(key)
        if not active and (not prior or prior['processing_state'] == 'resolved'):
            return
        state = 'open' if active else 'resolved'
        if prior and prior['processing_state'] == state and prior['summary'] == summary:
            return
        # Freeze pending same-state revisions: never change an uncertain request's content.
        if prior and prior['processing_state'] == state and any(
                p['incident_id'] == prior['incident_id'] for p in self.state['outbox']):
            return
        if len(self.state['outbox']) >= 16:
            self.state['outbox_full'] = True
            self.state['alert_history_incomplete'] = True
            return
        now = int(time.time())
        if prior and prior['processing_state'] == 'open':
            value = {**prior, 'revision': prior['revision']+1, 'updated_at': max(now, prior['updated_at']),
                     'processing_state': state, 'summary': summary}
        else:
            value = dict(schema='daimon-alerts/v1', producer_id=self.producer,
                incident_id=key+'-'+secrets.token_hex(12), operation_id='bot-traffic-'+key,
                revision=1, receiving_scope=self.scope, phase='bot-traffic',
                resource_ref='service-telegram-bot-api', severity='error', problem_code=code,
                summary=summary, action=action, source_at=now, updated_at=now,
                processing_state=state, work_preserved=True)
        self.state['incidents'][key] = value
        self.state['outbox'].append(value)

    def publish(self):
        if not self.producer:
            return
        # Exact projections persist before HTTP. Replays after lost acceptance are identical.
        while self.state['outbox']:
            value = self.state['outbox'][0]
            result = self.client.call('PUT', '/v1/incidents/'+self.producer+'/'+value['incident_id'], 'Acceptance', value)
            if any(result[k] != value[k] for k in ('producer_id', 'incident_id', 'revision')):
                raise AlertsUnavailable('traffic_acceptance_mismatch')
            self.state['outbox'].pop(0)
            self.save()
        self.state['outbox_full'] = False
        self.client.call('PUT', '/v1/observers/'+self.observer, 'Status',
            {'receiving_scope': self.scope, 'source_at': int(time.time()), 'source_state': 'healthy'})

    def tick(self):
        now = time.time()
        try:
            value = self.capture()
            transport = self.state['transport_failed']
            for row in value['_failed']:
                transport[row['method']] = max(transport.get(row['method'], 0), row['failed_at'])
            for method, failed_at in list(transport.items()):
                if value['_successes'].get(method, 0) > failed_at:
                    del transport[method]
            value['failed_methods'] = sorted(transport)
            self.transition('transport', bool(transport), 'transport_error',
                'Repeated Bot API failures in '+', '.join(sorted(transport))+'.' if transport else 'Previously failing Bot API methods succeeded.',
                'Inspect the local Bot API and collector. Do not replay calls whose delivery is uncertain.')
            cooldown = self.cooldown(value)
            value['cooldown'] = cooldown
            if value['meter_healthy']:
                self.state['failures'] = 0
            else:
                self.state['failures'] += 1
            if value['dropped_records'] > self.state['dropped']:
                self.state['dropped'] = value['dropped_records']
                self.state['loss_at'] = now
            self.transition('cooldown', bool(cooldown['methods']), 'rate_limited',
                ('Telegram rejected '+', '.join(cooldown['methods'])+'. '
                 + ('Reported cooldown ends '+time.strftime('%Y-%m-%d %H:%M:%S UTC', time.gmtime(cooldown['until']))+'.' if cooldown['until'] else 'No retry deadline reported.'))
                if cooldown['methods'] else 'Previously rate-limited methods succeeded after their last rejection.',
                'Inspect bot traffic in Sessions. Respect Telegram retry deadlines; do not replay uncertain replies.')
            coverage_open = self.state['incidents'].get('coverage', {}).get('processing_state') == 'open'
            loss = self.state['loss_at'] is not None and (now-self.state['loss_at'] < 60 or (coverage_open and not value['meter_healthy']))
            self.transition('coverage', loss, 'observation_unreadable',
                'Traffic capture lost journal records; statistics have a coverage gap.' if loss else 'Capture is writing again; the retained historical coverage gap remains visible.',
                'Check collector health and storage. Preserve the journal; missing traffic cannot be reconstructed.')
        except (OSError, ValueError, KeyError, TypeError, sqlite3.Error, ProviderUnavailable):
            self.state['failures'] += 1
            value = {'state': 'unavailable', 'observed_at': now, 'error': 'capture_unavailable'}
        self.transition('collector', self.state['failures'] >= 2, 'observation_unreadable',
            'Bot traffic capture is unavailable; traffic and cooldown recovery cannot be confirmed.'
            if self.state['failures'] >= 2 else 'Bot traffic capture is observable again.',
            'Inspect the traffic collector and its private journal. Sessions/native work must not be reset.')
        try:
            self.save()
            self.assert_binding()
            self.publish()
            value['alerts_state'] = ('backlog' if self.state.get('outbox_full') else 'available') if self.producer else 'not_configured'
        except (OSError, ValueError, sqlite3.Error, AlertsUnavailable, ProviderUnavailable):
            value['alerts_state'] = 'unavailable'
        value = {k: v for k, v in value.items() if not k.startswith('_')}
        value['alert_history_incomplete'] = self.state.get('alert_history_incomplete', False)
        with self.lock:
            self.view = value

    def notification_ready(self):
        # Deferral happens BEFORE a delivery claim; it never retries a claimed send.
        with self.lock:
            if self.view.get('state') != 'available' or not self.view.get('meter_healthy'):
                return False
            if time.time()-self.view['observed_at'] > max(30, self.interval*2):
                return False
            blocked = dict(self.state['blocked'])
        return not any((row['until'] or 0) > time.time() for method, row in blocked.items()
                       if method in ('sendMessage', 'unknown'))

    def snapshot(self):
        with self.lock:
            value = json.loads(json.dumps(self.view))
        if value['observed_at'] and time.time()-value['observed_at'] > max(30, self.interval*2):
            value['state'] = 'stale'
        return value

    def run(self):
        while not self.closed.is_set():
            self.tick()
            self.closed.wait(self.interval)

    def start(self):
        if self.thread is None:
            self.thread = threading.Thread(target=self.run, name='telecodex-traffic', daemon=True)
            self.thread.start()

    def close(self):
        self.closed.set()
        if self.thread:
            self.thread.join(timeout=2)
