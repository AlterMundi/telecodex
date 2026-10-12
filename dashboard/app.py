"""Private Telegram Mini App shell and explicit receiving-owner controls."""
from contextlib import asynccontextmanager, contextmanager
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import re
import secrets
import sqlite3
import stat
import time
import tomllib
from urllib.parse import parse_qsl, urlparse

from fastapi import FastAPI, HTTPException, Request
from fastapi.responses import FileResponse, JSONResponse
from init_data_py import errors, validate_by_signature

from .provider import Provider, ProviderUnavailable


def validate_launch(raw, bot_id):
    # Reuse Nexus's inspected production Ed25519 pattern; a menu launch has no start_param.
    try:
        pairs = parse_qsl(raw, keep_blank_values=True, strict_parsing=True,
                          encoding='utf-8', errors='strict', max_num_fields=40)
        if len({key for key, _ in pairs}) != len(pairs):
            raise ValueError('Duplicate field')
        data = validate_by_signature(raw, bot_id, environment='prod', expires_in=300)
        if data.issued_at.timestamp() > datetime.now(timezone.utc).timestamp() + 30:
            raise ValueError('Future launch')
        if not data.user or data.user.id <= 0:
            raise ValueError('No signed user')
        return data.user.id
    except (errors.InvalidInitDataError, ValueError, UnicodeError, TypeError) as exc:
        raise ValueError('Invalid or expired Telegram launch') from exc


def digest(value):
    return hashlib.sha256(value.encode()).hexdigest()


def create_app(config=None, provider=None, validator=validate_launch):
    if config is None:
        config = tomllib.loads(Path(os.environ['TELECODEX_DASHBOARD_CONFIG']).read_text())
    provider = provider or Provider(config)
    origin = config['origin']
    prefix = config.get('prefix', '/sessions')
    if (urlparse(origin).scheme != 'https' or urlparse(origin).netloc == ''
            or urlparse(origin).path not in ('', '/') or not re.fullmatch(r'/[a-zA-Z0-9_-]+', prefix)):
        raise ValueError('Exact HTTPS origin and one-segment prefix required')
    state = Path(config['state_dir'])
    state.mkdir(mode=0o700, parents=True, exist_ok=True)
    info = state.lstat()
    if not state.is_dir() or state.is_symlink() or info.st_uid != os.getuid() or info.st_mode & 0o077:
        raise ValueError('Dashboard state must be owner-only')
    db_path = state / 'dashboard.sqlite3'
    if db_path.exists() or db_path.is_symlink():
        info = db_path.lstat()
        if not stat.S_ISREG(info.st_mode) or info.st_uid != os.getuid() or info.st_mode & 0o077:
            raise ValueError('Unsafe dashboard state file')
    binding = digest(json.dumps([config['bot_id'],
                     getattr(provider, 'receiver_bytes', b'').hex()]))

    @contextmanager
    def database():
        conn = sqlite3.connect(db_path, timeout=3)
        conn.row_factory = sqlite3.Row
        try:
            with conn:
                yield conn
        finally:
            conn.close()

    with database() as db:
        db.executescript('''CREATE TABLE IF NOT EXISTS sessions(token_hash TEXT PRIMARY KEY,
            human INTEGER NOT NULL,expires INTEGER NOT NULL,csrf TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS actions(human INTEGER,nonce TEXT,state TEXT,result TEXT,
            PRIMARY KEY(human,nonce));''')
    db_path.chmod(0o600)

    @asynccontextmanager
    async def lifespan(app):
        yield
        provider.close()

    app = FastAPI(lifespan=lifespan, docs_url=None, redoc_url=None, openapi_url=None)

    @app.middleware('http')
    async def boundaries(request, call_next):
        if request.method == 'POST':
            if request.headers.get('origin') != origin:
                return JSONResponse({'detail': 'Wrong request origin'}, status_code=403)
            # Bound chunked bodies as well as Content-Length before parsing them.
            data = bytearray()
            async for chunk in request.stream():
                data.extend(chunk)
                if len(data) > 16384:
                    return JSONResponse({'detail': 'Request too large'}, status_code=413)
            request._body = bytes(data)
        response = await call_next(request)
        response.headers['Cache-Control'] = 'no-store'
        response.headers['X-Content-Type-Options'] = 'nosniff'
        response.headers['Referrer-Policy'] = 'no-referrer'
        response.headers['X-Robots-Tag'] = 'noindex, nofollow'
        response.headers['Content-Security-Policy'] = (
            "default-src 'self'; script-src 'self' https://telegram.org; style-src 'self'; "
            "img-src 'self' data:; connect-src 'self'; base-uri 'none'; form-action 'none'")
        return response

    def authorize(request):
        token = request.cookies.get('telecodex_dashboard', '')
        with database() as db:
            session = db.execute('SELECT * FROM sessions WHERE token_hash=? AND expires>?',
                                 (digest(token + binding), time.time())).fetchone()
        if not session or not provider.authorized(session['human']):
            raise HTTPException(401, 'Open the dashboard from this bot in Telegram')
        return session

    public = Path(__file__).parent / 'web'

    @app.get('/')
    def shell():
        return FileResponse(public / 'index.html')

    @app.get('/assets/{name}')
    def asset(name: str):
        if name not in ('app.js', 'style.css'):
            raise HTTPException(404)
        return FileResponse(public / name)

    @app.get('/health')
    def health():
        return {'ok': True}

    @app.post('/api/session')
    async def exchange(request: Request):
        try:
            body = await request.json()
            raw = body['init_data']
            if not isinstance(raw, str):
                raise ValueError()
            human = validator(raw, config['bot_id'])
        except (ValueError, KeyError, TypeError):
            raise HTTPException(401, 'Invalid or expired Telegram launch') from None
        if not provider.authorized(human):
            raise HTTPException(403, 'This account has no receiving session control grant')
        token, csrf = secrets.token_urlsafe(32), secrets.token_urlsafe(32)
        with database() as db:
            db.execute('DELETE FROM sessions WHERE expires<?', (time.time(),))
            db.execute('INSERT INTO sessions VALUES(?,?,?,?)', (digest(token + binding), human, time.time() + 3600, csrf))
        response = JSONResponse({'ok': True})
        response.set_cookie('telecodex_dashboard', token, httponly=True, secure=True,
                            samesite='strict', path=prefix, max_age=3600)
        return response

    @app.get('/api/state')
    def snapshot(request: Request):
        session = authorize(request)
        try:
            result = provider.snapshot()
        except (ProviderUnavailable, OSError, sqlite3.Error):
            raise HTTPException(503, 'Receiving metadata unavailable; retry shortly') from None
        # Recheck grants after native I/O, before any private response.
        authorize(request)
        return {**result, 'csrf': session['csrf']}

    @app.post('/api/stop')
    async def stop(request: Request):
        session = authorize(request)
        if not secrets.compare_digest(request.headers.get('x-dashboard-csrf', ''), session['csrf']):
            raise HTTPException(403, 'Action token missing')
        try:
            body = await request.json()
            nonce = body['nonce']
            if set(body) != {'nonce'} or not isinstance(nonce, str) or not re.fullmatch(r'[a-zA-Z0-9_-]{16,64}', nonce):
                raise ValueError()
        except (ValueError, KeyError, TypeError):
            raise HTTPException(400, 'Invalid Stop request') from None
        with database() as db:
            # Claim before the first external effect; a concurrent retry cannot dispatch it twice.
            fresh = db.execute('INSERT OR IGNORE INTO actions VALUES(?,?,?,NULL)',
                               (session['human'], nonce, 'requested')).rowcount == 1
            prior = db.execute('SELECT state,result FROM actions WHERE human=? AND nonce=?',
                               (session['human'], nonce)).fetchone()
        if not fresh:
            return JSONResponse(json.loads(prior['result']) if prior['result'] else
                                {'state': 'unconfirmed', 'detail': 'Inspect Stop status before another action'},
                                status_code=200 if prior['result'] else 202)
        try:
            # Sync control runs off the event loop; dashboard reads stay available during Stop.
            from starlette.concurrency import run_in_threadpool
            result = await run_in_threadpool(provider.stop, nonce)
        except Exception:
            result = {'state': 'unconfirmed', 'detail': 'Stop acknowledgement unavailable; inspect status before retrying'}
        with database() as db:
            db.execute('UPDATE actions SET state=?,result=? WHERE human=? AND nonce=?',
                       (result.get('state', 'unconfirmed'), json.dumps(result), session['human'], nonce))
        return JSONResponse(result, status_code=202)

    return app
