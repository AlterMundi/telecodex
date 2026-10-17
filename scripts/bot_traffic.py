#!/usr/bin/env python3
"""Private Bot API metadata meter. No message bodies, tokens, URLs or retries."""
import argparse
import asyncio
from contextlib import asynccontextmanager
import hashlib
import json
import logging
import math
import os
from pathlib import Path
import re
import sqlite3
import stat
import time
import tomllib
import uuid
from urllib.parse import parse_qs, urlsplit

CAPTURE_LIMIT = 65536
METHODS = {v.lower(): v for v in '''getMe getUpdates getWebhookInfo setWebhook deleteWebhook getFile fileDownload
sendMessage editMessageText editMessageCaption editMessageReplyMarkup deleteMessage deleteMessages sendMessageDraft
sendChatAction sendPhoto sendDocument sendAudio sendVideo sendVoice sendAnimation sendSticker sendMediaGroup
answerCallbackQuery setMessageReaction setMyCommands getMyCommands getChat getChatMember getChatMenuButton
setChatMenuButton createForumTopic editForumTopic deleteForumTopic closeForumTopic reopenForumTopic
forwardMessage forwardMessages copyMessage copyMessages pinChatMessage unpinChatMessage unpinAllChatMessages
sendLocation sendContact sendPoll sendVenue stopPoll logOut close'''.split()}
HOP = {'connection', 'keep-alive', 'proxy-authenticate', 'proxy-authorization',
       'te', 'trailer', 'transfer-encoding', 'upgrade', 'host', 'x-telecodex-priority'}
MESSAGE_WRITES = {name for name in METHODS.values() if name.startswith(('send', 'editMessage', 'deleteMessage', 'copyMessage', 'forwardMessage'))} - {'sendChatAction'}


def safe_directory(path):
    path = Path(path)
    path.mkdir(mode=0o700, parents=True, exist_ok=True)
    info = path.lstat()
    if not stat.S_ISDIR(info.st_mode) or info.st_uid != os.getuid() or info.st_mode & 0o077:
        raise ValueError('Owner-only state directory required')
    return path


def private_token(path):
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    with os.fdopen(fd) as stream:
        info = os.fstat(stream.fileno())
        if not stat.S_ISREG(info.st_mode) or info.st_uid != os.getuid() or info.st_mode & 0o077:
            raise ValueError('Owner-only token file required')
        token = stream.read(4097).strip()
    if not re.fullmatch(r'[0-9]+:[A-Za-z0-9_-]{20,200}', token):
        raise ValueError('Invalid token file')
    return hashlib.sha256(token.encode()).digest()


def number(value):
    if isinstance(value, bool):
        return None
    if isinstance(value, int) and -(2**63) < value < 2**63:
        return value
    if isinstance(value, str) and re.fullmatch(r'-?[0-9]{1,19}', value):
        return number(int(value))
    return None


def metadata(raw, content_type):
    """Allowlist numeric routing fields; never return arbitrary body fields."""
    try:
        if 'application/json' in content_type:
            value = json.loads(raw)
        elif 'application/x-www-form-urlencoded' in content_type:
            value = {k: v[0] for k, v in parse_qs(raw.decode()).items()}
        elif 'multipart/form-data' in content_type:
            # Inspect only bounded text fields before the first file. Files are opaque.
            from email.parser import BytesParser
            from email.policy import default
            message = BytesParser(policy=default).parsebytes(
                b'Content-Type: ' + content_type.encode() + b'\r\nMIME-Version: 1.0\r\n\r\n' + raw)
            value = {}
            for part in message.iter_parts():
                if part.get_filename() is not None:
                    break
                key = part.get_param('name', header='content-disposition')
                if key in ('chat_id', 'message_thread_id', 'message_id'):
                    value[key] = (part.get_payload(decode=True) or b'')[:32].decode('ascii')
        else:
            return {}
        if not isinstance(value, dict):
            return {}
        return {k: number(value.get(k)) for k in ('chat_id', 'message_thread_id', 'message_id')}
    except (ValueError, UnicodeError, TypeError, AttributeError):
        return {}


class Journal:
    def __init__(self, directory, retention_days=7, max_events=250000):
        if not isinstance(retention_days, int) or not 1 <= retention_days <= 365 or not isinstance(max_events, int) or max_events < 1:
            raise ValueError('Positive bounded retention required')
        self.path = safe_directory(directory) / 'traffic.sqlite3'
        if self.path.exists() or self.path.is_symlink():
            info = self.path.lstat()
            if not stat.S_ISREG(info.st_mode) or info.st_uid != os.getuid() or info.st_mode & 0o077:
                raise ValueError('Unsafe traffic journal')
        self.db = sqlite3.connect(self.path, timeout=2)
        self.path.chmod(0o600)
        self.db.execute('PRAGMA journal_mode=WAL')
        self.db.execute('PRAGMA foreign_keys=ON')
        self.db.executescript('''CREATE TABLE IF NOT EXISTS requests(
            id TEXT PRIMARY KEY,started REAL,finished REAL,source TEXT,source_pid INTEGER,
            method TEXT,chat_id INTEGER,topic_id INTEGER,message_id INTEGER,
            request_bytes INTEGER,response_bytes INTEGER,http_status INTEGER,api_error INTEGER,
            retry_after INTEGER,duration_ms REAL,outcome TEXT,metadata_complete INTEGER,request_metadata_complete INTEGER,response_metadata_complete INTEGER);
            CREATE INDEX IF NOT EXISTS requests_time ON requests(started);
            CREATE TABLE IF NOT EXISTS messages(chat_id INTEGER,message_id INTEGER,topic_id INTEGER,
                source TEXT,updated REAL,PRIMARY KEY(chat_id,message_id));
            CREATE INDEX IF NOT EXISTS messages_updated ON messages(updated);
            CREATE TABLE IF NOT EXISTS ingress(request_id TEXT REFERENCES requests(id) ON DELETE CASCADE,topic_id INTEGER,kind TEXT,count INTEGER);
            CREATE INDEX IF NOT EXISTS ingress_request ON ingress(request_id);
            CREATE TABLE IF NOT EXISTS settings(key TEXT PRIMARY KEY,value TEXT);
            CREATE TABLE IF NOT EXISTS outbound_control(key TEXT PRIMARY KEY,until REAL NOT NULL);
        ''')
        self.db.execute("UPDATE requests SET outcome='collector_restarted_unknown',finished=? WHERE outcome='in_flight'", (time.time(),))
        self.db.execute("INSERT OR IGNORE INTO settings VALUES('capture_started',?)", (str(time.time()),))
        self.db.commit()
        self.retention_days, self.max_events = retention_days, max_events
        self.completed = 0
        row = self.db.execute("SELECT value FROM settings WHERE key='dropped_records'").fetchone()
        self.dropped = int(row[0]) if row else 0
        self.prune()
        self.db.commit()

    def loss(self):
        self.dropped += 1
        logging.warning('Traffic journal write failed; coverage incomplete')

    def checkpoint_loss(self):
        self.db.execute("INSERT OR REPLACE INTO settings VALUES('dropped_records',?)", (str(self.dropped),))

    def bind_bot(self, fingerprint):
        prior = self.db.execute("SELECT value FROM settings WHERE key='bot_fingerprint'").fetchone()
        if prior and prior[0] != fingerprint:
            raise ValueError('Traffic journal bot binding changed')
        self.db.execute("INSERT OR IGNORE INTO settings VALUES('bot_fingerprint',?)", (fingerprint,))
        self.db.commit()

    def start(self, source, pid, method):
        ident = uuid.uuid4().hex
        try:
            self.db.execute('INSERT INTO requests(id,started,source,source_pid,method,outcome) VALUES(?,?,?,?,?,?)',
                            (ident, time.time(), source, pid, method, 'in_flight'))
            self.prune()
            self.checkpoint_loss()
            self.db.commit()
            return ident
        except sqlite3.Error:
            self.db.rollback()
            self.loss()
            return None

    def finish(self, ident, route, response, sizes, http_status, elapsed, outcome, complete, request_complete=True):
        if ident is None:
            return
        try:
            if not self.db.execute('SELECT 1 FROM requests WHERE id=?', (ident,)).fetchone():
                return
        except sqlite3.Error:
            self.loss()
            return
        api_error = retry = None
        try:
            value = json.loads(response) if complete else None
        except (ValueError, UnicodeError):
            value = None
        if isinstance(value, dict):
            api_error = number(value.get('error_code'))
            parameters = value.get('parameters')
            retry = number(parameters.get('retry_after')) if isinstance(parameters, dict) else None
            result = value.get('result')
            if isinstance(result, dict) and isinstance(result.get('chat'), dict):
                route['chat_id'] = number(result['chat'].get('id'))
                route['message_thread_id'] = number(result.get('message_thread_id', 0))
                route['message_id'] = number(result.get('message_id'))
        chat, topic, message = (route.get(k) for k in ('chat_id', 'message_thread_id', 'message_id'))
        try:
            if topic is None and chat is not None and message is not None:
                row = self.db.execute('SELECT topic_id FROM messages WHERE chat_id=? AND message_id=?',
                                      (chat, message)).fetchone()
                if row:
                    topic = row[0]
            self.db.execute('''UPDATE requests SET finished=?,chat_id=?,topic_id=?,message_id=?,
                request_bytes=?,response_bytes=?,http_status=?,api_error=?,retry_after=?,duration_ms=?,
                outcome=?,metadata_complete=?,request_metadata_complete=?,response_metadata_complete=? WHERE id=?''',
                (time.time(), chat, topic, message, *sizes, http_status, api_error, retry, elapsed,
                 outcome, int(complete and request_complete), int(request_complete), int(complete), ident))
            if chat is not None and message is not None and topic is not None:
                self.db.execute('INSERT OR REPLACE INTO messages VALUES(?,?,?,?,?)',
                                (chat, message, topic, 'observed', time.time()))
            if isinstance(value, dict) and isinstance(value.get('result'), list):
                counts = {}
                for update in value['result']:
                    if not isinstance(update, dict):
                        continue
                    for kind in ('message', 'edited_message', 'callback_query', 'message_reaction'):
                        msg = update.get(kind)
                        if not isinstance(msg, dict):
                            continue
                        if kind == 'callback_query':
                            msg = msg.get('message') or {}
                        key = (number(msg.get('message_thread_id', 0)), kind)
                        counts[key] = counts.get(key, 0) + 1
                for (topic, kind), count in counts.items():
                    self.db.execute('INSERT INTO ingress VALUES(?,?,?,?)', (ident, topic, kind, count))
            self.completed += 1
            self.prune()
            self.checkpoint_loss()
            self.db.commit()
        except sqlite3.Error:
            self.db.rollback()
            self.loss()

    def seed(self, path):
        """Existing activity message receipts supply routing, never message text."""
        entries = json.loads(Path(path).read_text())
        for v in entries.values():
            if not isinstance(v, dict):
                continue
            chat, message, topic = (number(v.get(k)) for k in ('chat_id', 'message_id', 'topic'))
            if all(x is not None for x in (chat, message, topic)):
                self.db.execute('INSERT OR IGNORE INTO messages VALUES(?,?,?,?,?)',
                                (chat, message, topic, 'activity_receipt', time.time()))
        self.prune()
        self.db.commit()

    def prune(self):
        cutoff = time.time() - self.retention_days * 86400
        self.db.execute('DELETE FROM requests WHERE started<?', (cutoff,))
        self.db.execute('DELETE FROM requests WHERE rowid <= (SELECT MAX(rowid) FROM requests)-?', (self.max_events,))
        self.db.execute('DELETE FROM messages WHERE updated<?', (cutoff,))
        self.db.execute('DELETE FROM messages WHERE rowid <= (SELECT MAX(rowid) FROM messages)-?', (self.max_events,))

    def close(self):
        self.db.close()


class Governor:
    """One bot's durable admission budget. Never retry a transmitted request."""
    def __init__(self, journal):
        self.journal = journal
        # Adopt an already observed cooldown when upgrading the observer. Local
        # deferrals must never extend Telegram's deadline or create incidents.
        rows = journal.db.execute('''SELECT method,MAX(finished+retry_after) FROM requests
            WHERE outcome='response' AND (api_error=429 OR http_status=429)
            AND retry_after>0 GROUP BY method''').fetchall()
        for method, until in rows:
            self.extend(self.cooldown_key(method), until + 1)

    @staticmethod
    def cooldown_key(method):
        # Telegram does not specify the exact scope of a 429. Conservatively
        # share message-write cooldowns across this bot, including all topics.
        return 'cooldown:writes' if method in MESSAGE_WRITES else 'cooldown:' + method

    def extend(self, key, until):
        with self.journal.db:
            self.journal.db.execute('''INSERT INTO outbound_control VALUES(?,?)
                ON CONFLICT(key) DO UPDATE SET until=MAX(until,excluded.until)''', (key, until))

    def until(self, key):
        row = self.journal.db.execute('SELECT until FROM outbound_control WHERE key=?', (key,)).fetchone()
        return row[0] if row else 0

    def observe(self, method, response, complete):
        if not complete:
            return
        try:
            value = json.loads(response)
            retry = number(value.get('parameters', {}).get('retry_after'))
            if value.get('error_code') == 429 and retry and retry > 0:
                self.extend(self.cooldown_key(method), time.time() + retry + 1)
        except (ValueError, TypeError, AttributeError):
            pass

    async def admit(self, method, route, background):
        if method not in MESSAGE_WRITES and method != 'sendChatAction':
            # Method-specific cooldowns also apply to controls, never ingress.
            if method in ('getUpdates', 'fileDownload'):
                return 0
            return max(0, math.ceil(self.until(self.cooldown_key(method)) - time.time()))
        now = time.time()
        cooldown = self.until(self.cooldown_key(method))
        if cooldown > now:
            return math.ceil(cooldown - now)
        chat, topic = route.get('chat_id'), route.get('message_thread_id', 0)
        if method == 'sendChatAction':
            # Coalesce duplicate typing producers within Telegram's 5s display
            # lifetime, per topic. Typing never consumes the reply budget.
            keys = [(f'typing:{chat}:{topic}', 4)]
            background = True
        else:
            keys = [('budget:global', .04), (f'budget:chat:{chat}', 3.5 if chat is not None and chat < 0 else 1)]
        due = max(self.until(key) for key, _ in keys)
        if background and due > now:
            return max(1, math.ceil(due - now))
        # Foreground clients already pace their own requests. Do not introduce
        # a second queue timeout: native final delivery cannot yet durably replay
        # a locally rejected completed answer. Reserve the quiet period for
        # optional work, without rejecting a human reply for ordinary contention.
        with self.journal.db:
            self.journal.db.execute('DELETE FROM outbound_control WHERE until<?', (now,))
            for key, spacing in keys:
                self.journal.db.execute('INSERT OR REPLACE INTO outbound_control VALUES(?,?)', (key, now + spacing))
        return 0


def source_for(port, listen_port, sources):
    """Linux socket ownership; unknown remains unknown. Never record command lines."""
    try:
        inode = None
        for family in ('tcp', 'tcp6'):
            for line in Path('/proc/net/' + family).read_text().splitlines()[1:]:
                parts = line.split()
                if int(parts[1].rsplit(':', 1)[1], 16) == port and int(parts[2].rsplit(':', 1)[1], 16) == listen_port:
                    inode = parts[9]
                    break
        if inode:
            target = 'socket:[' + inode + ']'
            for process in Path('/proc').iterdir():
                if not process.name.isdecimal() or process.stat().st_uid != os.getuid():
                    continue
                try:
                    cgroup = (process / 'cgroup').read_text()
                    label = next((name for name, unit in sources.items() if '/' + unit + '\n' in cgroup), None)
                    if label and any(os.readlink(fd) == target for fd in (process / 'fd').iterdir()):
                        return label, int(process.name)
                except (OSError, PermissionError):
                    continue
    except (OSError, ValueError, IndexError):
        pass
    return 'unknown', None


def headers_for(headers):
    blocked = HOP | {v.strip().lower() for v in headers.get('connection', '').split(',')}
    return [(k, v) for k, v in headers.items() if k.lower() not in blocked]


def create_app(config):
    import httpx
    from starlette.applications import Starlette
    from starlette.responses import JSONResponse, StreamingResponse
    from starlette.routing import Route
    upstream = urlsplit(config['upstream'])
    if upstream.scheme != 'http' or upstream.hostname not in ('127.0.0.1', '::1') or upstream.path not in ('', '/') or upstream.query or upstream.username:
        raise ValueError('Fixed loopback upstream required')
    fingerprint = private_token(config['token_file'])
    journal = Journal(config['state_dir'], config.get('retention_days', 7), config.get('max_events', 250000))
    journal.bind_bot(fingerprint.hex())
    if type(config.get('rate_control', True)) is not bool:
        raise ValueError('rate_control must be a boolean')
    governor = Governor(journal) if config.get('rate_control', True) else None
    if config.get('activity_receipts'):
        journal.seed(config['activity_receipts'])

    @asynccontextmanager
    async def lifespan(app):
        async with httpx.AsyncClient(timeout=httpx.Timeout(180, connect=5), trust_env=False,
                                    follow_redirects=False, transport=httpx.AsyncHTTPTransport(retries=0)) as client:
            app.state.client = client
            yield
        journal.close()

    async def health(request):
        return JSONResponse({'ok': True, 'dropped_records': journal.dropped})

    async def forward(request):
        path = request.url.path
        match = re.fullmatch(r'/(?:file/)?bot([^/]+)/([^/]+)(?:/.*)?', path)
        tracked = bool(match and hashlib.sha256(match[1].encode()).digest() == fingerprint)
        method = 'fileDownload' if path.startswith('/file/') else METHODS.get(match[2].lower(), 'unknown') if match else 'unknown'
        source, pid = ('unknown', None)
        if tracked and request.client:
            source, pid = await asyncio.to_thread(source_for, request.client.port, config['port'], config.get('sources', {}))
        ident = journal.start(source, pid, method) if tracked else None
        began = time.monotonic()
        incoming, outgoing = [0], [0]
        request_prefix, response_prefix = bytearray(), bytearray()
        route = metadata(request.url.query.encode(), 'application/x-www-form-urlencoded')
        finalized = False

        def finish(status, outcome):
            nonlocal finalized
            if finalized:
                return
            finalized = True
            if incoming[0] <= CAPTURE_LIMIT or 'multipart/form-data' in request.headers.get('content-type', ''):
                route.update({k: v for k, v in metadata(bytes(request_prefix), request.headers.get('content-type', '')).items() if v is not None})
            complete = outgoing[0] <= CAPTURE_LIMIT and len(response_prefix) == outgoing[0]
            if tracked and governor and outcome == 'response':
                try:
                    governor.observe(method, bytes(response_prefix), complete)
                except sqlite3.Error:
                    journal.loss()
            journal.finish(ident, route, bytes(response_prefix), (incoming[0], outgoing[0]), status,
                           (time.monotonic() - began) * 1000, outcome, complete,
                           incoming[0] <= CAPTURE_LIMIT and 'multipart/form-data' not in request.headers.get('content-type', '') and outcome == 'response')

        stream = request.stream().__aiter__()
        buffered = []
        async def body():
            for chunk in buffered:
                yield chunk
            async for chunk in stream:
                incoming[0] += len(chunk)
                if tracked and len(request_prefix) < CAPTURE_LIMIT:
                    request_prefix.extend(chunk[:CAPTURE_LIMIT - len(request_prefix)])
                yield chunk
        if tracked and governor:
            try:
                # Read only a bounded prefix for routing. Uploads remain streamed;
                # no message/file bytes enter persistent admission state.
                async for chunk in stream:
                    incoming[0] += len(chunk)
                    request_prefix.extend(chunk[:max(0, CAPTURE_LIMIT - len(request_prefix))])
                    buffered.append(chunk)
                    if incoming[0] >= CAPTURE_LIMIT:
                        break
                route.update({k: v for k, v in metadata(bytes(request_prefix), request.headers.get('content-type', '')).items() if v is not None})
            except asyncio.CancelledError:
                finish(None, 'cancelled_not_forwarded')
                raise
            except Exception:
                finish(None, 'not_forwarded')
                return JSONResponse({'ok': False, 'description': 'Input interrupted before forwarding. No retry was made.'}, status_code=503)
            try:
                retry = await governor.admit(method, route, request.headers.get('x-telecodex-priority') == 'background')
            except sqlite3.Error:
                journal.loss()
                retry = None
            if retry is None or retry:
                value = {'ok': False, 'error_code': 503 if retry is None else 429,
                         'description': 'Local outbound admission unavailable; request was not forwarded.' if retry is None else 'Local outbound deferral; request was not forwarded.',
                         'telecodex_local': True}
                if retry:
                    value['parameters'] = {'retry_after': retry}
                encoded = json.dumps(value).encode()
                response_prefix.extend(encoded);outgoing[0] = len(encoded)
                finish(value['error_code'], 'admission_unavailable' if retry is None else 'deferred_local')
                return JSONResponse(value, status_code=value['error_code'])
        try:
            target = config['upstream'].rstrip('/') + request.scope.get('raw_path', path.encode()).decode('ascii')
            if request.url.query:
                target += '?' + request.url.query
            outgoing_request = request.app.state.client.build_request(request.method, target,
                headers=headers_for(request.headers), content=body())
            response = await request.app.state.client.send(outgoing_request, stream=True)
        except asyncio.CancelledError:
            finish(None, 'cancelled_unknown')
            raise
        except Exception:
            finish(None, 'upstream_unknown')
            return JSONResponse({'ok': False, 'description': 'Upstream unavailable; delivery is uncertain. No retry was made.'}, status_code=502)

        async def chunks():
            outcome = 'response'
            try:
                async for chunk in response.aiter_raw():
                    outgoing[0] += len(chunk)
                    if tracked and not response.headers.get('content-encoding') and len(response_prefix) < CAPTURE_LIMIT:
                        response_prefix.extend(chunk[:CAPTURE_LIMIT - len(response_prefix)])
                    yield chunk
            except asyncio.CancelledError:
                outcome = 'cancelled_unknown'
                raise
            except Exception:
                outcome = 'upstream_unknown'
                raise RuntimeError('Upstream stream interrupted; delivery uncertain') from None
            finally:
                await response.aclose()
                finish(response.status_code, outcome)
        return StreamingResponse(chunks(), status_code=response.status_code,
                                 headers=dict(headers_for(response.headers)))

    app = Starlette(routes=[Route('/healthz', health), Route('/{path:path}', forward, methods=['GET', 'POST', 'PUT', 'DELETE', 'HEAD'])], lifespan=lifespan)
    app.state.journal = journal
    return app


def report(directory, hours=24):
    path = Path(directory) / 'traffic.sqlite3'
    db = sqlite3.connect(path.as_uri() + '?mode=ro', uri=True)
    since = time.time() - hours * 3600
    def rows(sql):
        return db.execute(sql, (since,)).fetchall()
    result = {
        'capture_started': db.execute("SELECT value FROM settings WHERE key='capture_started'").fetchone()[0],
        'hours': hours,
        'columns': {
            'totals': ['requests', 'request_body_bytes', 'response_body_bytes', 'rate_limited', 'in_flight'],
            'by_source_method': ['source', 'method', 'requests', 'rate_limited', 'max_retry_after_seconds'],
            'by_topic': ['chat_id', 'topic_id', 'requests', 'rate_limited'],
            'unattributed_requests': ['unknown_source', 'unknown_topic', 'incomplete_metadata'],
            'per_minute': ['utc_epoch_minute', 'source', 'method', 'requests', 'rate_limited'],
            'peak_requests_per_minute': ['utc_epoch_minute', 'requests'],
            'ingress': ['topic_id', 'kind', 'updates'],
            'failures': ['source', 'method', 'http_status', 'api_error', 'outcome', 'requests']},
        'dropped_records_checkpoint': db.execute("SELECT value FROM settings WHERE key='dropped_records'").fetchone(),
        'loss_accounting': 'Checkpointed on successful writes; an unwritable database followed by process loss can lose the final count. Check service warnings and health.',
        'totals': rows('SELECT COUNT(*),SUM(request_bytes),SUM(response_bytes),SUM(CASE WHEN outcome="response" AND (api_error=429 OR http_status=429) THEN 1 ELSE 0 END),SUM(outcome="in_flight") FROM requests WHERE started>=?')[0],
        'by_source_method': rows('SELECT source,method,COUNT(*),SUM(CASE WHEN outcome="response" AND (api_error=429 OR http_status=429) THEN 1 ELSE 0 END),MAX(CASE WHEN outcome="response" THEN retry_after END) FROM requests WHERE started>=? GROUP BY source,method ORDER BY COUNT(*) DESC'),
        'by_topic': rows('SELECT chat_id,topic_id,COUNT(*),SUM(CASE WHEN outcome="response" AND (api_error=429 OR http_status=429) THEN 1 ELSE 0 END) FROM requests WHERE started>=? GROUP BY chat_id,topic_id ORDER BY COUNT(*) DESC'),
        'unattributed_requests': rows('SELECT SUM(source="unknown"),SUM(topic_id IS NULL),SUM(metadata_complete=0) FROM requests WHERE started>=?')[0],
        'known_backoff_until': rows('SELECT MAX(finished+retry_after) FROM requests WHERE started>=? AND outcome="response" AND retry_after>0')[0][0],
        'deferred_local': rows('SELECT COUNT(*) FROM requests WHERE started>=? AND outcome="deferred_local"')[0][0],
        'per_minute': rows('SELECT CAST(started/60 AS INTEGER)*60,source,method,COUNT(*),SUM(CASE WHEN outcome="response" AND (api_error=429 OR http_status=429) THEN 1 ELSE 0 END) FROM requests WHERE started>=? GROUP BY CAST(started/60 AS INTEGER),source,method ORDER BY 1'),
        'peak_requests_per_minute': rows('SELECT CAST(started/60 AS INTEGER)*60,COUNT(*) FROM requests WHERE started>=? GROUP BY CAST(started/60 AS INTEGER) ORDER BY COUNT(*) DESC LIMIT 10'),
        'ingress': rows('SELECT i.topic_id,i.kind,SUM(i.count) FROM ingress i JOIN requests r ON r.id=i.request_id WHERE r.started>=? GROUP BY i.topic_id,i.kind'),
        'failures': rows('SELECT source,method,http_status,api_error,outcome,COUNT(*) FROM requests WHERE started>=? AND (api_error IS NOT NULL OR outcome NOT IN ("response","in_flight")) GROUP BY source,method,http_status,api_error,outcome'),
        'coverage': 'Only this configured bot through this endpoint. Unknown topics/origins and oversized metadata remain explicit; no historical traffic is reconstructed.'}
    db.close()
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest='command', required=True)
    serve = sub.add_parser('serve');serve.add_argument('--config', required=True)
    read = sub.add_parser('report');read.add_argument('--state-dir', required=True);read.add_argument('--hours', type=float, default=24)
    args = parser.parse_args()
    if args.command == 'report':
        print(json.dumps(report(Path(args.state_dir).resolve(), args.hours), indent=2));return
    config = tomllib.loads(Path(args.config).read_text())
    if config.get('host', '127.0.0.1') not in ('127.0.0.1', '::1'):
        raise ValueError('Loopback listener required')
    for label in config.get('sources', {}):
        if not re.fullmatch(r'[a-zA-Z0-9_-]{1,48}', label):
            raise ValueError('Invalid source label')
    import uvicorn
    logging.getLogger('httpx').disabled = True
    logging.getLogger('httpcore').disabled = True
    uvicorn.run(create_app(config), host=config.get('host', '127.0.0.1'), port=config['port'], access_log=False, proxy_headers=False)


if __name__ == '__main__':
    main()
