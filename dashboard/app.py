"""Private Telegram Mini App shell and explicit receiving-owner controls."""
from contextlib import aclosing, asynccontextmanager, contextmanager
from datetime import datetime, timezone
import hashlib
import html
import http.client
import json
import os
from pathlib import Path
import re
import secrets
import sqlite3
import stat
import time
import tomllib
from urllib.parse import parse_qsl, urlparse, urlencode

from fastapi import FastAPI, HTTPException, Request
from fastapi.responses import FileResponse, JSONResponse, HTMLResponse, Response, StreamingResponse, RedirectResponse
from init_data_py import errors, validate_by_signature

from .provider import Provider, ProviderUnavailable
from .computer_health import ComputerHealth, HealthUnavailable
from .visual import VisualSelectionInvalid
from .alerts import Receiver as AlertsReceiver, AlertsUnavailable
from .traffic import Traffic
from .menu_attention import attention_projection
from .operator import sessions_selection
from .speech import Speech, SpeechUnavailable, speech_part
from .companion import Grants as CompanionGrants, permitted as companion_permitted
from .home_auth import HomeAuth
from scripts.speech_render import RenderError


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


def create_app(config=None, provider=None, validator=validate_launch, alerts=None, traffic=None):
    if config is None:
        config = tomllib.loads(Path(os.environ['TELECODEX_DASHBOARD_CONFIG']).read_text())
    provider = provider or Provider(config)
    companions = CompanionGrants(config, provider)
    home_auth = HomeAuth(config, provider)
    speech = Speech(config['speech']) if config.get('speech') else None
    origin = config['origin']
    prefix = config.get('prefix', '/sessions')
    additional_origins = config.get('additional_origins', [])
    if not isinstance(additional_origins, list) or len(additional_origins) > 8:
        raise ValueError('Bounded exact additional origins required')
    for candidate in additional_origins:
        if not isinstance(candidate, str) or any(c.isspace() or c in "\\\"'<>" for c in candidate):
            raise ValueError('Exact HTTPS additional origin required')
        parsed = urlparse(candidate)
        try:
            parsed.port
        except ValueError:
            raise ValueError('Exact HTTPS additional origin required') from None
        if parsed.scheme != 'https' or not parsed.hostname or parsed.path or parsed.query or parsed.fragment or parsed.username or parsed.password:
            raise ValueError('Exact HTTPS additional origin required')

    launcher_origins = config.get('alerts', {}).get('launcher_origins', [])
    if not isinstance(launcher_origins, list) or len(launcher_origins) > 8:
        raise ValueError('Bounded exact launcher origins required')
    for candidate in launcher_origins:
        parsed = urlparse(candidate)
        if parsed.scheme != 'https' or not parsed.hostname or parsed.path or parsed.query or parsed.fragment or parsed.username or parsed.password:
            raise ValueError('Exact HTTPS launcher origin required')
    daimon_name = config.get('daimon_name', 'Daimon')
    if not isinstance(daimon_name, str) or not 1 <= len(daimon_name) <= 80 or any(ord(c) < 32 for c in daimon_name):
        raise ValueError('daimon_name must be a display name of 1 to 80 characters')
    conversation_appearance = config.get('conversation_appearance', 'inherit')
    if conversation_appearance not in ('inherit', 'authored'):
        raise ValueError('conversation_appearance must be inherit or authored')
    appearance = config.get('appearance_origin', '').rstrip('/')
    if appearance:
        u = urlparse(appearance)
        if (u.scheme != 'https' or not u.netloc or u.path or u.query or u.fragment or
                u.username or u.password or any(c.isspace() or c in "\\\"'<>" for c in appearance)):
            raise ValueError('Exact public Homemaker consumer origin required')
    if (urlparse(origin).scheme != 'https' or urlparse(origin).netloc == ''
            or urlparse(origin).path not in ('', '/') or not re.fullmatch(r'/[a-zA-Z0-9_-]+', prefix)):
        raise ValueError('Exact HTTPS origin and one-segment prefix required')
    state = Path(config['state_dir'])
    state.mkdir(mode=0o700, parents=True, exist_ok=True)
    info = state.lstat()
    if not state.is_dir() or state.is_symlink() or info.st_uid != os.getuid() or info.st_mode & 0o077:
        raise ValueError('Dashboard state must be owner-only')
    computer_health = ComputerHealth(config)
    db_path = state / 'dashboard.sqlite3'
    if db_path.exists() or db_path.is_symlink():
        info = db_path.lstat()
        if not stat.S_ISREG(info.st_mode) or info.st_uid != os.getuid() or info.st_mode & 0o077:
            raise ValueError('Unsafe dashboard state file')
    if alerts is None and config.get('alerts', {}).get('enabled') is True:
        alerts = AlertsReceiver(config, provider)
    if traffic is None and config.get('traffic', {}).get('enabled') is True:
        traffic = Traffic(config, provider)
    if traffic is not None and alerts is not None:
        alerts.dispatch_ready = traffic.notification_ready
    scope_binding = [config['bot_id'],
        getattr(provider, 'binding_bytes', getattr(provider, 'receiver_bytes', b'')).hex()]
    if alerts is not None:
        scope_binding.append(alerts.binding)
    binding = digest(json.dumps(scope_binding))

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
        if traffic is not None:
            traffic.start()
        if alerts is not None:
            alerts.start()
        yield
        if alerts is not None:
            alerts.close()
        if traffic is not None:
            traffic.close()
        computer_health.close()
        provider.close()

    app = FastAPI(lifespan=lifespan, docs_url=None, redoc_url=None, openapi_url=None)

    @app.middleware('http')
    async def boundaries(request, call_next):
        # A device credential never acquires the ambient Telegram session or
        # administrative routes, even when a browser also carries a valid cookie.
        if any(value.lower().startswith('companion') for value in request.headers.getlist('authorization')):
            try:
                await authorize_async(request)
            except HTTPException as exc:
                return JSONResponse({'detail':exc.detail}, status_code=exc.status_code)
        if request.method == 'POST':
            allowed = launcher_origins if request.url.path == '/api/alerts/attention' else [origin, *additional_origins]
            if request.headers.get('origin') not in allowed:
                return JSONResponse({'detail': 'Wrong request origin'}, status_code=403)
            # Bound chunked bodies as well as Content-Length before parsing them.
            if request.url.path in ('/api/interaction/voice','/api/interaction/visual'):
                try:
                    selected = await authorize_async(request)
                    if not secrets.compare_digest(request.headers.get('x-dashboard-csrf', ''), selected['csrf']):
                        raise HTTPException(403, 'Action token missing')
                except HTTPException as exc:
                    return JSONResponse({'detail':exc.detail}, status_code=exc.status_code)
                bound = 2*1024*1024 if request.url.path.endswith('/visual') else 20*1024*1024
            else:
                bound = 262144 if (request.url.path == '/api/interaction/inputs' or re.fullmatch(r'/api/interaction/(?:internal/)?requests/[a-f0-9]{32}',request.url.path)) else 16384
            data = bytearray()
            async for chunk in request.stream():
                data.extend(chunk)
                if len(data) > bound:
                    return JSONResponse({'detail': 'Request too large'}, status_code=413)
            request._body = bytes(data)
        response = await call_next(request)
        response.headers.setdefault('Cache-Control', 'no-store')
        response.headers['X-Content-Type-Options'] = 'nosniff'
        response.headers['Referrer-Policy'] = 'no-referrer'
        response.headers['X-Robots-Tag'] = 'noindex, nofollow'
        response.headers['Content-Security-Policy'] = (
            "default-src 'self'; script-src 'self' https://telegram.org" +
            (' '+appearance+'/home-style.js' if appearance else '') +
            "; style-src 'self'" + (' '+appearance if appearance else '') +
            "; font-src 'self'" + (' '+appearance if appearance else '') +
            "; img-src 'self' data:; media-src 'self' blob:; connect-src 'self'; base-uri 'none'; form-action 'none'; "
            "frame-ancestors 'self' https://web.telegram.org")
        return response

    def authorize(request):
        check_binding()
        token = request.cookies.get('telecodex_dashboard', '')
        headers = request.headers.getlist('authorization')
        if headers:
            if len(headers) == 1 and (device := re.fullmatch(r'Companion ([A-Za-z0-9_-]{43})', headers[0], re.IGNORECASE)):
                grant = companions.authenticate(device.group(1))
                if grant is None:
                    raise HTTPException(401, 'Companion access is unavailable; operator enrollment required')
                if not companion_permitted(request.method, request.url.path, grant['scope'], grant.get('artifact_access', False)):
                    raise HTTPException(403, 'Companion access is limited to the integrating conversation')
                return grant
            # A restricted WebView may omit cookies. Never downgrade an invalid
            # explicit credential to a different ambient cookie session.
            if len(headers) != 1 or not (match := re.fullmatch(
                    r'Bearer ([A-Za-z0-9_-]{43})', headers[0], re.IGNORECASE)):
                raise HTTPException(401, 'Invalid dashboard session credential')
            token = match.group(1)
        if not headers and not token and home_auth.enabled:
            try:
                shared = home_auth.authenticate(request)
            except PermissionError as exc:
                raise HTTPException(403, str(exc)) from None
            except (ValueError, KeyError, TypeError):
                raise HTTPException(401, 'Sign in to your conversation') from None
            except (OSError, http.client.HTTPException):
                raise HTTPException(503, 'Shared authentication unavailable; retry shortly') from None
            if shared is not None:
                return shared
        with database() as db:
            session = db.execute('SELECT * FROM sessions WHERE token_hash=? AND expires>?',
                                 (digest(token + binding), time.time())).fetchone()
        if not session or not provider.authorized(session['human']):
            raise HTTPException(401, 'Open the dashboard from this bot in Telegram')
        return session

    def check_binding():
        if hasattr(provider, 'assert_binding'):
            try:
                provider.assert_binding()
            except (ProviderUnavailable, OSError):
                raise HTTPException(503, 'Receiving control binding changed; operator must requalify Sessions') from None
        if alerts is not None:
            try:
                alerts.assert_binding()
            except (AlertsUnavailable, ProviderUnavailable, OSError):
                raise HTTPException(503, 'Alerts receiving binding changed; operator must requalify Sessions') from None

    async def authorize_async(request):
        if (home_auth.enabled and not request.headers.getlist('authorization')
                and not request.cookies.get('telecodex_dashboard')
                and request.headers.get('host') == home_auth.host
                and request.cookies.get('__Host-home')):
            from starlette.concurrency import run_in_threadpool
            return await run_in_threadpool(authorize, request)
        return authorize(request)

    from .live_transcription import install as install_live_transcription
    can_stream_transcribe = install_live_transcription(app,config,provider,authorize_async,[origin,*additional_origins])

    public = Path(__file__).parent / 'web'

    @app.get('/')
    def shell(request: Request):
        if computer_health.enabled and (request.query_params.get('view') == 'health'
                or computer_health.default_view and not request.query_params.get('view')):
            computer_health.assert_binding()
            return HTMLResponse(computer_health.shell)
        body = (public / 'index.html').read_text()
        body = body.replace('id="conversation-daimon">Daimon</span>', 'id="conversation-daimon">' + html.escape(daimon_name) + '</span>')
        shared_entry = home_auth.enabled and (request.query_params.get('view') == 'conversation'
            or request.headers.get('x-forwarded-prefix') == home_auth.entry.rstrip('/'))
        if shared_entry and (request.headers.get('host') != home_auth.host
                or request.query_params.get('view') != 'conversation'
                or request.query_params.get('client') != 'standalone'):
            query = {**dict(request.query_params), 'view': 'conversation', 'client': 'standalone'}
            return RedirectResponse(home_auth.origin + home_auth.entry + '?' + urlencode(query), status_code=307)
        if request.query_params.get('view') == 'conversation' and request.query_params.get('client') == 'standalone':
            if home_auth.enabled:
                if request.headers.get('host') != home_auth.host:
                    return RedirectResponse(home_auth.origin + home_auth.entry + '?view=conversation&client=standalone', status_code=307)
                body = body.replace('<html lang="en">', '<html lang="en" data-shared-auth="true">')
                body = body.replace('    <script src="assets/app.js" defer></script>',
                    '    <script src="/access-client.js" defer></script>\n    <script src="assets/app.js" defer></script>')
            body = body.replace('    <script src="https://telegram.org/js/telegram-web-app.js" defer></script>\n', '')
        for name in ('app.js', 'live.js', 'requests.js', 'microphone.js', 'visual.js', 'routing_graph.js', 'companion.js', 'sonic.js', 'speech-stream.js', 'live-speech.js', 'markdown-it.min.js', 'markdown.js', 'conversation.js', 'attention.js', 'style.css'):
            version = hashlib.sha256((public / name).read_bytes()).hexdigest()[:16]
            body = body.replace('assets/'+name+'"', 'assets/'+name+'?v='+version+'"')
        standalone_entry = (request.query_params.get('view') == 'conversation'
                            and request.query_params.get('client') == 'standalone')
        if standalone_entry:
            body = body.replace('<title>Sessions · Telecodex</title>', '<title>Daimon Interaction</title>')
            body = body.replace('</head>', '<link rel="manifest" href="manifest.webmanifest">'
                '<meta name="theme-color" content="#0b1018">'
                '<link rel="apple-touch-icon" href="assets/interaction-192.png"></head>')
        if standalone_entry and conversation_appearance == 'authored':
            body = body.replace('<html lang="en"', '<html lang="en" data-style-policy="authored"', 1)
        if appearance and not (standalone_entry and conversation_appearance == 'authored'):
            base = html.escape(appearance, quote=True)
            body = body.replace('<html lang="en">', '<html lang="en" data-style-policy="inherit">')
            body = body.replace('</head>', '<link rel="stylesheet" data-home-style href="'+base+
                '/style.css?surface=sessions"><script src="'+base+'/home-style.js" defer></script></head>')
        return HTMLResponse(body)

    @app.get('/manifest.webmanifest')
    def interaction_manifest():
        entry = home_auth.entry if home_auth.enabled else prefix + '/'
        return JSONResponse({'id': entry, 'name': 'Daimon Interaction', 'short_name': 'Daimon',
            'start_url': entry + '?view=conversation&client=standalone', 'scope': entry,
            'display': 'standalone', 'background_color': '#0b1018', 'theme_color': '#0b1018',
            'icons': [{'src':entry + 'assets/interaction-' + str(size) + '.png',
                       'sizes':str(size)+'x'+str(size), 'type':'image/png', 'purpose':'any'}
                      for size in (192,512)]}, media_type='application/manifest+json')

    @app.get('/api/interaction/events')
    async def interaction_events(request: Request):
        session = await authorize_async(request)
        from .live import changes
        paths = [Path(provider.receiver['db_path'])]
        if config.get('activity_state_dir'):
            paths.append(Path(config['activity_state_dir']) / 'topics.json')
        alert_reads = dict(session).get('scope') is None or dict(session).get('alerts_access') is True
        subscribe = getattr(alerts, 'subscribe', None) if alert_reads else None
        async def stream():
            checked = time.monotonic()
            async for changed in changes(paths, subscribe=subscribe):
                if await request.is_disconnected():
                    break
                try:
                    if time.monotonic() - checked >= 15 or time.time() >= session['expires']:
                        current = await authorize_async(request)
                        checked = time.monotonic()
                        if current['human'] != session['human']:
                            break
                except HTTPException:
                    yield 'event: expired\ndata: {}\n\n'
                    break
                yield 'data: {}\n\n' if changed else ': keepalive\n\n'
        return StreamingResponse(stream(), media_type='text/event-stream',
            headers={'Cache-Control':'no-store', 'X-Accel-Buffering':'no'})

    @app.get('/api/interaction/access')
    def shared_access(request: Request):
        if not home_auth.enabled or request.headers.getlist('authorization'):
            raise HTTPException(401, 'Shared browser sign-in required')
        session = authorize(request)
        if dict(session).get('scope') != 'interaction_only':
            raise HTTPException(403, 'Shared conversation grant required')
        return {'csrf': session['csrf'], 'label': session.get('label', 'Your conversation')}

    @app.get('/assets/{name}')
    def asset(name: str, request: Request):
        if name in ('health.css', 'health-ui.js') and computer_health.enabled:
            computer_health.assert_binding()
            data, media = computer_health.assets[name]
            return Response(data, media_type=media)
        if name not in ('health-live.js', 'live.js', 'interaction-192.png', 'interaction-512.png', 'app.js', 'requests.js', 'microphone.js', 'voice-stream.js', 'voice-pcm.js', 'visual.js', 'routing_graph.js', 'companion.js', 'sonic.js', 'speech-stream.js', 'live-speech.js', 'markdown-it.min.js', 'markdown.js', 'conversation.js', 'attention.js', 'style.css', 'markdown-it.LICENSE', 'markdown-it.SOURCE', 'space-grotesk-latin.woff2', 'OFL.txt'):
            raise HTTPException(404)
        version = hashlib.sha256((public / name).read_bytes()).hexdigest()[:16]
        immutable = list(request.query_params.multi_items()) == [('v',version)]
        headers = {'Cache-Control':'public, max-age=31536000, immutable' if immutable else 'public, max-age=0, must-revalidate',
                   'ETag':'"'+version+'"'}
        if request.headers.get('if-none-match') == headers['ETag']:
            return Response(status_code=304,headers=headers)
        return FileResponse(public / name, headers=headers)

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
        check_binding()
        if not provider.authorized(human):
            raise HTTPException(403, 'This account has no receiving session control grant')
        token, csrf = secrets.token_urlsafe(32), secrets.token_urlsafe(32)
        with database() as db:
            db.execute('DELETE FROM sessions WHERE expires<?', (time.time(),))
            db.execute('INSERT INTO sessions VALUES(?,?,?,?)', (digest(token + binding), human, time.time() + 3600, csrf))
        # Same short-lived, bot/config-bound session, carried explicitly by the
        # authenticated WebView in memory; never persisted in browser storage.
        response = JSONResponse({'ok': True, 'session_token': token})
        response.set_cookie('telecodex_dashboard', token, httponly=True, secure=True,
                            samesite='strict', path=prefix, max_age=3600)
        return response

    @app.get('/api/computer-health')
    def health_snapshot(request: Request):
        authorize(request)
        try:
            result = computer_health.snapshot()
        except HealthUnavailable:
            raise HTTPException(503, 'Computer health binding changed; operator must requalify') from None
        authorize(request)
        return result

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

    @app.get('/api/traffic')
    def bot_traffic(request: Request):
        authorize(request)
        result = traffic.snapshot() if traffic is not None else {'state': 'not_configured'}
        authorize(request)
        return result

    @app.get('/api/interaction')
    async def interaction_sources(request: Request, delivery: int = 0, native: int = 0, direction: str = 'after', matrix: int | None = None):
        session = await authorize_async(request)
        try:
            from starlette.concurrency import run_in_threadpool
            result = await run_in_threadpool(provider.presentation_list, delivery, native, direction) if matrix is None else await run_in_threadpool(provider.presentation_list, delivery, native, direction, matrix)
        except (ProviderUnavailable, OSError, ValueError):
            raise HTTPException(503, 'Integrating conversation unavailable; refresh before continuing') from None
        can_files=bool(getattr(provider,'receiver',{}).get('interaction',{}).get('artifact_store')) and (dict(session).get('scope') is None or dict(session).get('scope')=='interaction_only' and dict(session).get('artifact_access') is True)
        files={}
        if can_files:
            try:
                files=await run_in_threadpool(provider.artifacts)
            except (ProviderUnavailable,OSError,ValueError):
                files={'state':'unavailable'}
        await authorize_async(request)
        return {**result, 'csrf':session['csrf'],
                'presentation_profile':dict(session).get('presentation_profile', companions.default_profile),
                'can_listen':speech is not None,
                'can_stream_speech':bool(speech and speech.streaming),
                'can_speak_live_response':bool(speech and speech.live_response),
                'can_download_files':can_files,'artifacts':files,
                'can_speak':bool(getattr(provider, 'receiver', {}).get('voice')),
                'streaming_transcription_mode':config.get('streaming_transcription',{}).get('mode','prepared'),
                'can_stream_transcribe':can_stream_transcribe and bool(getattr(provider,'receiver',{}).get('voice')) and dict(session).get('scope') != 'monitor_readonly'}

    @app.get('/api/interaction/inputs')
    async def conversation_inputs(request: Request, before: int = 0, direction: str = 'before'):
        session = await authorize_async(request)
        try:
            from starlette.concurrency import run_in_threadpool
            result = await run_in_threadpool(provider.conversation_inputs, before, direction)
        except ValueError:
            raise HTTPException(400, 'Invalid input history cursor') from None
        except (ProviderUnavailable, OSError):
            raise HTTPException(503, 'Message history unavailable; refresh to reconnect') from None
        await authorize_async(request)
        viewer = {}
        if isinstance(session, dict) and session.get('scope') in ('monitor_readonly','interaction_only'):
            label = session.get('label')
            if not isinstance(label, str) or not 1 <= len(label) <= 64 or any(ord(c) < 32 for c in label):
                label = 'Selected receiver'
            viewer = {'viewer': {'label': label, 'expires': session['expires']}}
        return {**result, 'csrf':session['csrf'], **viewer}

    @app.get('/api/interaction/telegram-inputs')
    async def conversation_telegram_inputs(request: Request, before: int = 0, direction: str = 'before'):
        await authorize_async(request)
        try:
            from starlette.concurrency import run_in_threadpool
            result=await run_in_threadpool(provider.conversation_telegram_inputs,before,direction)
        except ValueError:
            raise HTTPException(400,'Invalid Telegram observation cursor') from None
        except (ProviderUnavailable,OSError):
            raise HTTPException(503,'Telegram observation unavailable; no input was resent') from None
        await authorize_async(request)
        return result

    @app.get('/api/interaction/inputs/{ident}')
    async def conversation_input_status(request: Request, ident: str):
        await authorize_async(request)
        try:
            from starlette.concurrency import run_in_threadpool
            result = await run_in_threadpool(provider.conversation_input_status, ident)
        except ValueError:
            raise HTTPException(400, 'Invalid message identity') from None
        except (ProviderUnavailable, OSError):
            raise HTTPException(503, 'Message status unconfirmed; check again without resending') from None
        await authorize_async(request)
        return result

    @app.get('/api/interaction/inputs/{ident}/trace')
    async def conversation_input_trace(request: Request, ident: str):
        await authorize_async(request)
        try:
            from starlette.concurrency import run_in_threadpool
            result = await run_in_threadpool(provider.conversation_input_trace, ident)
        except ValueError:
            raise HTTPException(400, 'Invalid message identity') from None
        except (ProviderUnavailable, OSError):
            raise HTTPException(503, 'Routing observation unavailable; the input was not resent') from None
        await authorize_async(request)
        return result

    @app.get('/api/interaction/inputs/{ident}/trace/{source}')
    async def conversation_trace_text(request: Request, ident: str, source: str, offset: int = 0):
        await authorize_async(request)
        try:
            from starlette.concurrency import run_in_threadpool
            result = await run_in_threadpool(provider.conversation_trace_text, ident, source, offset)
        except ValueError:
            raise HTTPException(400, 'Invalid trace text selection') from None
        except (ProviderUnavailable, OSError):
            raise HTTPException(503, 'Saved text unavailable; the input was not resent') from None
        await authorize_async(request)
        return result

    @app.post('/api/interaction/inputs')
    async def conversation_input_submit(request: Request):
        session = await authorize_async(request)
        if not secrets.compare_digest(request.headers.get('x-dashboard-csrf', ''), session['csrf']):
            raise HTTPException(403, 'Action token missing')
        try:
            body = await request.json()
            from starlette.concurrency import run_in_threadpool
            result = await run_in_threadpool(provider.conversation_input_submit, body)
        except VisualSelectionInvalid:
            return JSONResponse({'detail':'Select the captured image again. This attempt did not submit a message.','code':'visual_reselection','submission_attempted':False},status_code=400)
        except (ValueError, UnicodeError):
            raise HTTPException(400, 'Provide one original message of at most 32 KiB') from None
        except (ProviderUnavailable, OSError):
            raise HTTPException(503, 'Submission unconfirmed. Check this message status before sending it again') from None
        await authorize_async(request)
        return result

    @app.post('/api/interaction/inputs/{ident}/route')
    async def conversation_input_route(request: Request, ident: str):
        session = await authorize_async(request)
        if not secrets.compare_digest(request.headers.get('x-dashboard-csrf', ''), session['csrf']):
            raise HTTPException(403, 'Action token missing')
        try:
            body = await request.json()
            from starlette.concurrency import run_in_threadpool
            result = await run_in_threadpool(provider.conversation_input_route, ident, body)
        except (ValueError, UnicodeError):
            raise HTTPException(400, 'Select one destination for this retained message') from None
        except (ProviderUnavailable, OSError):
            raise HTTPException(409, 'Selection unconfirmed or no longer current. Inspect the retained message; nothing was replayed') from None
        await authorize_async(request)
        return result

    @app.post('/api/interaction/inputs/{ident}/retry')
    async def conversation_input_retry(request: Request, ident: str):
        session = await authorize_async(request)
        if not secrets.compare_digest(request.headers.get('x-dashboard-csrf', ''), session['csrf']):
            raise HTTPException(403, 'Action token missing')
        try:
            body = await request.json()
            from starlette.concurrency import run_in_threadpool
            result = await run_in_threadpool(provider.conversation_input_retry, ident, body)
        except (ValueError, UnicodeError):
            raise HTTPException(400, 'Select one saved route that was not sent') from None
        except (ProviderUnavailable, OSError):
            raise HTTPException(409, 'Recovery unconfirmed or no longer current. Check saved status without creating another message') from None
        await authorize_async(request)
        return result

    @app.post('/api/interaction/visual')
    async def conversation_visual(request: Request):
        session=await authorize_async(request)
        if not secrets.compare_digest(request.headers.get('x-dashboard-csrf',''),session['csrf']):
            raise HTTPException(403,'Action token missing')
        if request.headers.get('content-type')!='image/png':raise HTTPException(400,'Select one PNG image')
        try:
            from starlette.concurrency import run_in_threadpool
            result=await run_in_threadpool(provider.conversation_visual_upload,await request.body(),request.headers.get('x-image-sha256',''))
        except ValueError:raise HTTPException(400,'Image unavailable or invalid. Select a still image of at most 2 MiB and 4 megapixels.') from None
        except (ProviderUnavailable,OSError):raise HTTPException(503,'Image upload unconfirmed. No conversation work was admitted.') from None
        await authorize_async(request)
        return result

    @app.get('/api/interaction/inputs/{ident}/image')
    async def conversation_visual_image(request: Request,ident:str,index:int=0):
        await authorize_async(request)
        try:
            from starlette.concurrency import run_in_threadpool
            data=await run_in_threadpool(provider.conversation_visual_image,ident,index)
        except (ValueError,ProviderUnavailable,OSError):raise HTTPException(409,'Selected image unavailable; do not substitute another view') from None
        await authorize_async(request)
        return Response(content=data,media_type='image/png')

    @app.post('/api/interaction/voice')
    async def conversation_voice(request: Request):
        session = await authorize_async(request)
        if not secrets.compare_digest(request.headers.get('x-dashboard-csrf', ''), session['csrf']):
            raise HTTPException(403, 'Action token missing')
        metadata = {key:request.headers.get(header, '') for key, header in
                    (('binding', 'x-input-binding'), ('id', 'x-input-id'),
                     ('audio_sha256', 'x-audio-sha256'), ('mime', 'content-type'))}
        reply_source = request.headers.get('x-reply-source')
        reply_revision = request.headers.get('x-reply-revision')
        if reply_source is not None or reply_revision is not None:
            metadata['reply'] = {'source':reply_source, 'revision':reply_revision}
            if isinstance(reply_source,str) and reply_source.startswith('context:'):
                # Older receivers reject this source form before ASR rather
                # than silently interpreting context as an explicit answer.
                metadata['reply'].update(source=reply_source[8:], context_only=True)
        playback=request.headers.get('x-playback-context')
        if playback is not None:
            try:
                if len(playback.encode())>2048:raise ValueError('Playback context exceeds bound')
                metadata['playback']=json.loads(playback)
                Provider.check_playback(metadata['playback'])
            except (ValueError,TypeError):raise HTTPException(400,'Invalid device playback context') from None
        capture_questions=request.headers.get('x-capture-questions')
        if capture_questions is not None:
            try:
                if len(capture_questions.encode())>4096:raise ValueError('Capture questions exceed bound')
                metadata['capture_questions']=json.loads(capture_questions)
                Provider.check_capture_questions(metadata['capture_questions'])
            except (ValueError,TypeError):raise HTTPException(400,'Invalid capture question restriction') from None
        visual=request.headers.get('x-visual-reference')
        if visual is not None:
            try:metadata['visual']=json.loads(visual)
            except (ValueError,TypeError):raise HTTPException(400,'Select one captured visual reference') from None
        try:
            data = await request.body()
            from starlette.concurrency import run_in_threadpool
            result = await run_in_threadpool(provider.conversation_voice_submit, metadata, data)
        except VisualSelectionInvalid:
            return JSONResponse({'detail':'Select the captured image again. This attempt did not submit a recording.','code':'visual_reselection','submission_attempted':False},status_code=400)
        except (ValueError, UnicodeError):
            raise HTTPException(400, 'Provide one finalized recording of at most 20 MiB') from None
        except (ProviderUnavailable, OSError):
            raise HTTPException(503, 'Voice capture unconfirmed; check its status before retrying this recording') from None
        await authorize_async(request)
        return result

    @app.get('/api/interaction/sonic')
    async def conversation_sonic_state(request: Request):
        await authorize_async(request)
        try:
            from starlette.concurrency import run_in_threadpool
            result=await run_in_threadpool(provider.sonic_activity)
        except (ValueError,KeyError,ProviderUnavailable,OSError,sqlite3.Error):
            raise HTTPException(503,'Session sound state unavailable; conversation work is unchanged') from None
        await authorize_async(request)
        return result

    @app.get('/api/interaction/artifacts')
    async def conversation_artifacts(request: Request):
        await authorize_async(request)
        try:
            from starlette.concurrency import run_in_threadpool
            result=await run_in_threadpool(provider.artifacts)
        except (ValueError,ProviderUnavailable,OSError):
            raise HTTPException(409,'Files unavailable; conversation work was not repeated') from None
        await authorize_async(request)
        return result

    @app.get('/api/interaction/artifacts/{ident}')
    async def conversation_artifact_download(request: Request,ident: str):
        await authorize_async(request)
        try:
            from starlette.concurrency import run_in_threadpool
            from datetime import datetime
            from urllib.parse import quote
            ref,stream=await run_in_threadpool(provider.artifact_open,ident)
        except (ValueError,ProviderUnavailable,OSError):
            raise HTTPException(409,'File unavailable, expired or changed; the text reply remains available') from None
        try:
            await authorize_async(request)
            deadline=datetime.fromisoformat(ref['expires_at']).timestamp()
            if time.time()>=deadline:
                raise HTTPException(409,'File expired')
        except BaseException:
            stream.close()
            raise
        def chunks():
            checked=0
            try:
                while True:
                    if time.time()>=deadline:
                        raise RuntimeError('Artifact expired during download')
                    if time.monotonic()-checked>=1:
                        authorize(request);checked=time.monotonic()
                    data=stream.read(65536)
                    if not data:break
                    yield data
            finally:
                stream.close()
        return StreamingResponse(chunks(),media_type='application/octet-stream',headers={
            'Content-Disposition':"attachment; filename*=UTF-8''"+quote(ref['name'],safe=''),
            'Content-Length':str(ref['bytes']),'X-Artifact-Sha256':ref['sha256'],
            'X-Content-Type-Options':'nosniff','Cache-Control':'no-store'})

    @app.get('/api/interaction/goals')
    async def conversation_goals(request: Request, before: int = 0):
        await authorize_async(request)
        try:
            from starlette.concurrency import run_in_threadpool
            result = await run_in_threadpool(provider.goal_controls, before)
        except ValueError:
            raise HTTPException(400, 'Invalid control selection') from None
        except (ProviderUnavailable, OSError):
            raise HTTPException(503, 'Goal controls unavailable; nothing was changed') from None
        await authorize_async(request)
        return result

    @app.get('/api/interaction/goals/{ident}')
    async def conversation_goal_detail(request: Request, ident: str):
        await authorize_async(request)
        try:
            from starlette.concurrency import run_in_threadpool
            result = await run_in_threadpool(provider.goal_control_detail, ident)
        except ValueError:
            raise HTTPException(400, 'Invalid control selection') from None
        except (ProviderUnavailable, OSError):
            raise HTTPException(409, 'Control changed or is unavailable; nothing was changed') from None
        await authorize_async(request)
        return result

    @app.get('/api/interaction/tasks')
    async def conversation_tasks(request: Request, after: int = 0):
        session=await authorize_async(request)
        try:
            from starlette.concurrency import run_in_threadpool
            result=await run_in_threadpool(provider.conversation_tasks,after)
        except ValueError:
            raise HTTPException(400,'Invalid task cursor') from None
        except (ProviderUnavailable,OSError):
            raise HTTPException(503,'Human tasks unavailable; refresh to reconnect') from None
        await authorize_async(request)
        return {**result,'csrf':session['csrf']}

    @app.get('/api/interaction/tasks/{ident}')
    async def conversation_task_detail(request: Request, ident: str, offset: int = 0, revision: str | None = None):
        await authorize_async(request)
        try:
            from starlette.concurrency import run_in_threadpool
            result=await run_in_threadpool(provider.conversation_task_detail,ident,offset,revision)
        except ValueError:
            raise HTTPException(400,'Invalid task selection') from None
        except (ProviderUnavailable,OSError):
            raise HTTPException(409,'Task changed or is unavailable; refresh its current state') from None
        await authorize_async(request)
        return result

    @app.post('/api/interaction/tasks/{ident}')
    async def conversation_task_dismiss(request: Request, ident: str):
        session = await authorize_async(request)
        if not secrets.compare_digest(request.headers.get('x-dashboard-csrf',''),session['csrf']):
            raise HTTPException(403,'Action token missing')
        try:
            body = await request.json()
            from starlette.concurrency import run_in_threadpool
            result = await run_in_threadpool(provider.conversation_task_dismiss,ident,body)
        except ValueError:
            raise HTTPException(400,'Select one current action to dismiss') from None
        except (ProviderUnavailable,OSError):
            raise HTTPException(409,'Dismissal unconfirmed or action changed; check its current state') from None
        await authorize_async(request)
        return result

    @app.get('/api/interaction/internal')
    async def internal_pending(request: Request, after: int = 0):
        session=await authorize_async(request)
        try:
            from starlette.concurrency import run_in_threadpool
            result=await run_in_threadpool(provider.internal_pending,after)
        except ValueError:
            raise HTTPException(400,'Invalid internal pending cursor') from None
        except (ProviderUnavailable,OSError):
            raise HTTPException(503,'Session pending requests unavailable') from None
        await authorize_async(request)
        return {**result,'csrf':session['csrf']}

    @app.get('/api/interaction/internal/{kind}/{ident}')
    async def internal_pending_detail(request: Request, kind: str, ident: str, offset: int = 0, revision: str | None = None):
        await authorize_async(request)
        if kind not in ('requests','tasks'):raise HTTPException(404,'Unknown pending kind')
        try:
            from starlette.concurrency import run_in_threadpool
            method=provider.conversation_request_detail if kind=='requests' else provider.conversation_task_detail
            result=await run_in_threadpool(method,ident,offset,revision,True)
        except ValueError:
            raise HTTPException(400,'Invalid session pending selection') from None
        except (ProviderUnavailable,OSError):
            raise HTTPException(409,'Pending item changed; read its current state') from None
        await authorize_async(request)
        return result

    @app.get('/api/interaction/internal/requests/{ident}/receipt')
    async def internal_pending_receipt(request: Request, ident: str, revision: str):
        await authorize_async(request)
        try:
            from starlette.concurrency import run_in_threadpool
            result=await run_in_threadpool(provider.conversation_decision_receipt,ident,revision,True)
        except ValueError:
            raise HTTPException(400,'Invalid session pending receipt') from None
        except (ProviderUnavailable,OSError):
            raise HTTPException(409,'Decision receipt unavailable; nothing was repeated') from None
        await authorize_async(request)
        return result

    @app.post('/api/interaction/internal/{kind}/{ident}')
    async def internal_pending_decide(request: Request, kind: str, ident: str):
        session=await authorize_async(request)
        if kind not in ('requests','tasks'):raise HTTPException(404,'Unknown pending kind')
        if not secrets.compare_digest(request.headers.get('x-dashboard-csrf',''),session['csrf']):
            raise HTTPException(403,'Action token missing')
        try:
            body=await request.json()
            # Conversation-only devices keep their existing approval/cancellation restriction.
            if kind=='requests' and dict(session).get('scope')=='interaction_only' and (
                    not isinstance(body,dict) or not isinstance(body.get('action'),dict)
                    or body['action'].get('kind') not in ('answer','option')):
                raise HTTPException(403,'Use Sessions to review this approval or cancellation')
            from starlette.concurrency import run_in_threadpool
            method=provider.conversation_request_decide if kind=='requests' else provider.conversation_task_dismiss
            result=await run_in_threadpool(method,ident,body,True)
        except ValueError:
            raise HTTPException(400,'Select one exact session pending action') from None
        except (ProviderUnavailable,OSError):
            raise HTTPException(409,'Action unconfirmed or changed; inspect its current state before retrying') from None
        await authorize_async(request)
        return result

    @app.get('/api/interaction/requests')
    async def conversation_requests(request: Request, after: int = 0):
        session = await authorize_async(request)
        try:
            from starlette.concurrency import run_in_threadpool
            result = await run_in_threadpool(provider.conversation_requests, after)
        except ValueError:
            raise HTTPException(400, 'Invalid request cursor') from None
        except (ProviderUnavailable, OSError):
            raise HTTPException(503, 'Pending requests unavailable; refresh to reconnect') from None
        await authorize_async(request)
        return {**result, 'csrf':session['csrf']}

    @app.get('/api/interaction/requests/{ident}')
    async def conversation_request_detail(request: Request, ident: str, offset: int = 0, revision: str | None = None):
        await authorize_async(request)
        try:
            from starlette.concurrency import run_in_threadpool
            result = await run_in_threadpool(provider.conversation_request_detail, ident, offset, revision)
        except ValueError:
            raise HTTPException(400, 'Invalid native request selection') from None
        except (ProviderUnavailable, OSError):
            raise HTTPException(409, 'Request changed or is unavailable; refresh its current state') from None
        await authorize_async(request)
        return result

    @app.get('/api/interaction/requests/{ident}/receipt')
    async def conversation_decision_receipt(request: Request, ident: str, revision: str):
        await authorize_async(request)
        try:
            from starlette.concurrency import run_in_threadpool
            result = await run_in_threadpool(provider.conversation_decision_receipt, ident, revision)
        except ValueError:
            raise HTTPException(400, 'Invalid decision selection') from None
        except (ProviderUnavailable, OSError):
            raise HTTPException(409, 'Decision recording unconfirmed; no repeat was submitted') from None
        await authorize_async(request)
        return result

    @app.post('/api/interaction/requests/{ident}')
    async def conversation_request_decide(request: Request, ident: str):
        session = await authorize_async(request)
        if not secrets.compare_digest(request.headers.get('x-dashboard-csrf', ''), session['csrf']):
            raise HTTPException(403, 'Action token missing')
        try:
            body = await request.json()
            from starlette.concurrency import run_in_threadpool
            result = await run_in_threadpool(provider.conversation_request_decide, ident, body)
        except ValueError:
            raise HTTPException(400, 'Select one exact native request decision') from None
        except (ProviderUnavailable, OSError):
            raise HTTPException(409, 'Decision unconfirmed or request changed. Check its current state before trying again') from None
        await authorize_async(request)
        return result

    @app.get('/api/interaction/part')
    async def interaction_part(request: Request, source: str, revision: str, offset: int = 0):
        await authorize_async(request)
        try:
            from starlette.concurrency import run_in_threadpool
            result = await run_in_threadpool(provider.presentation_part, source, revision, offset)
        except (ProviderUnavailable, OSError, ValueError):
            raise HTTPException(409, 'Selected result changed or is unavailable; refresh the conversation') from None
        await authorize_async(request)
        return result

    @app.post('/api/interaction/audio')
    async def interaction_audio(request: Request):
        session = await authorize_async(request)
        if not secrets.compare_digest(request.headers.get('x-dashboard-csrf', ''), session['csrf']):
            raise HTTPException(403, 'Action token missing')
        if speech is None:
            raise HTTPException(503, 'Speech output is not configured')
        try:
            body = await request.json()
            if (not isinstance(body, dict) or set(body) not in ({'source','revision','offset'},{'source','revision','offset','stream'})
                    or type(body.get('stream',False)) is not bool):
                raise ValueError()
            from starlette.concurrency import run_in_threadpool
            canonical = await run_in_threadpool(provider.presentation_part, body['source'], body['revision'], body['offset'])
            if canonical.get('kind')=='integrator_live' and not speech.live_response:
                raise ValueError('Live speech is not selected')
            selected = speech_part(canonical) if speech.incremental else canonical
        except (ProviderUnavailable, OSError, ValueError, KeyError):
            raise HTTPException(409, 'Selected result changed or is unavailable; refresh the conversation') from None
        await authorize_async(request)
        if body.get('stream'):
            if not speech.streaming or selected.get('kind')=='native_question':
                raise HTTPException(409,'Streaming speech unavailable; use the ordinary reader')
            async def audio_frames():
                try:
                    async with aclosing(speech.stream(selected)) as frames:
                        async for frame in frames:
                            await authorize_async(request)
                            latest=await run_in_threadpool(provider.presentation_part,body['source'],body['revision'],body['offset'])
                            if latest!=canonical:
                                raise SpeechUnavailable('Presentation source changed')
                            yield json.dumps(frame,separators=(',',':'))+'\n'
                except (SpeechUnavailable,RenderError,ProviderUnavailable,OSError,ValueError,KeyError,TimeoutError,HTTPException):
                    yield json.dumps({'type':'error','message':'Speech stopped. Text remains available; no automatic retry.'})+'\n'
            return StreamingResponse(audio_frames(),media_type='application/x-ndjson',headers={
                'Cache-Control':'no-store','X-Accel-Buffering':'no',
                'X-Presentation-Source':body['source'],'X-Presentation-Revision':body['revision'],
                'X-Presentation-Offset':str(body['offset']),
                'X-Presentation-Next-Offset':str(selected['next_offset']) if selected['next_offset'] is not None else '',
                'X-Presentation-End-Offset':str(body['offset']+len(selected['text'])),
                'X-Presentation-Text-Sha256':hashlib.sha256(selected['text'].encode()).hexdigest(),
                'X-Presentation-Prefetch':'true','X-Audio-Encoding':'mp3','X-Playback-Observation':'not_observed'})
        try:
            data, rendered = await speech.render(selected)
            # Retained text, bindings and correction state are rechecked after synthesis.
            latest = await run_in_threadpool(provider.presentation_part, body['source'], body['revision'], body['offset'])
            if latest != canonical:
                raise SpeechUnavailable('Presentation source changed')
        except (SpeechUnavailable, RenderError, ProviderUnavailable, OSError, ValueError, KeyError, TimeoutError):
            raise HTTPException(409, 'Speech outcome retained; refresh before continuing') from None
        await authorize_async(request)
        return Response(data, media_type='audio/wav', headers={
            'X-Presentation-Source':body['source'], 'X-Presentation-Revision':body['revision'],
            'X-Presentation-Offset':str(body['offset']),
            'X-Presentation-Next-Offset':str(selected['next_offset']) if selected['next_offset'] is not None else '',
            'X-Audio-Sha256':rendered['audio_sha256'], 'X-Playback-Observation':'not_observed',
            'X-Presentation-End-Offset':str(body['offset']+len(selected['text'])) if speech.incremental and selected.get('kind')!='native_question' else '',
            'X-Presentation-Text-Sha256':hashlib.sha256(selected['text'].encode()).hexdigest() if speech.incremental and selected.get('kind')!='native_question' else '',
            'X-Presentation-Prefetch':'true' if speech.incremental and selected.get('kind')!='native_question' else 'false'})

    @app.post('/api/alerts/attention')
    async def launcher_attention(request: Request):
        # Read-only signed-launch route. It mints no control/session credential.
        try:
            body = await request.json()
            if not isinstance(body, dict) or set(body) != {'init_data'} or not isinstance(body['init_data'], str):
                raise ValueError()
            human = validator(body['init_data'], config['bot_id'])
        except (ValueError, KeyError, TypeError):
            raise HTTPException(401, 'Invalid or expired Telegram launch') from None
        check_binding()
        if not provider.authorized(human):
            raise HTTPException(403, 'This account has no receiving alert grant')
        view = alerts.snapshot() if alerts is not None else {'state':'not_configured'}
        result = attention_projection(view)
        check_binding()
        if not provider.authorized(human):
            raise HTTPException(403, 'Receiving grant unavailable')
        return {**result, 'app_url':sessions_selection(config)['web_app']['url'],
                'bot_username':config.get('bot_username')}

    @app.get('/api/interaction/alerts')
    def conversation_alerts(request: Request):
        session = authorize(request)
        if dict(session).get('scope') is not None and dict(session).get('alerts_access') is not True:
            raise HTTPException(403, 'This conversation has no alert read grant')
        try:
            result = alerts.snapshot() if alerts is not None else {'state': 'not_configured'}
        except (OSError, sqlite3.Error, ValueError):
            raise HTTPException(503, 'Alerts receiving state unavailable') from None
        authorize(request)
        return {**result, 'attention':attention_projection(result), 'read_only':True}

    @app.get('/api/alerts')
    def incidents(request: Request):
        session = authorize(request)
        try:
            result = alerts.snapshot() if alerts is not None else {'state': 'not_configured'}
        except (OSError, sqlite3.Error, ValueError):
            raise HTTPException(503, 'Alerts receiving state unavailable') from None
        authorize(request)
        return {**result, 'attention':attention_projection(result), 'csrf': session['csrf']}

    @app.post('/api/alerts/acknowledge')
    async def acknowledge(request: Request):
        session = await authorize_async(request)
        if not secrets.compare_digest(request.headers.get('x-dashboard-csrf', ''), session['csrf']):
            raise HTTPException(403, 'Action token missing')
        if alerts is None:
            raise HTTPException(503, 'Alerts are not configured')
        try:
            body = await request.json()
            if not isinstance(body, dict) or set(body) != {'producer_id', 'incident_id', 'request_id'} or any(
                    not isinstance(value, str) or not re.fullmatch(r'[A-Za-z0-9_-]{1,128}', value)
                    for value in body.values()):
                raise ValueError()
        except (ValueError, TypeError):
            raise HTTPException(400, 'Invalid acknowledgment request') from None
        try:
            from starlette.concurrency import run_in_threadpool
            result = await run_in_threadpool(alerts.acknowledge,
                body['producer_id'], body['incident_id'], body['request_id'])
        except (AlertsUnavailable, ProviderUnavailable, OSError, ValueError, sqlite3.Error):
            raise HTTPException(503, 'Acknowledgment unconfirmed; refresh before retrying') from None
        await authorize_async(request)
        return result

    @app.post('/api/stop')
    async def stop(request: Request):
        return await action(request, 'stop')

    @app.post('/api/resume')
    async def resume(request: Request):
        return await action(request, 'resume')

    async def action(request, kind):
        session = await authorize_async(request)
        if not secrets.compare_digest(request.headers.get('x-dashboard-csrf', ''), session['csrf']):
            raise HTTPException(403, 'Action token missing')
        try:
            body = await request.json()
            nonce = body['nonce']
            expected = {'nonce'} if kind == 'stop' else {'nonce', 'operation'}
            if set(body) != expected or not isinstance(nonce, str) or not re.fullmatch(r'[a-zA-Z0-9_-]{16,64}', nonce):
                raise ValueError()
            operation = body.get('operation')
            if kind == 'resume' and (not isinstance(operation, str) or
                    not re.fullmatch(r'[a-zA-Z0-9_-]{1,128}', operation)):
                raise ValueError()
        except (ValueError, KeyError, TypeError):
            raise HTTPException(400, 'Invalid control request') from None
        # Separate action/operation namespaces; never reuse another intent's receipt.
        journal_nonce = nonce if kind == 'stop' else 'resume:' + digest(operation + ':' + nonce)
        with database() as db:
            # Claim before the first external effect; a concurrent retry cannot dispatch it twice.
            fresh = db.execute('INSERT OR IGNORE INTO actions VALUES(?,?,?,NULL)',
                               (session['human'], journal_nonce, 'requested')).rowcount == 1
            prior = db.execute('SELECT state,result FROM actions WHERE human=? AND nonce=?',
                               (session['human'], journal_nonce)).fetchone()
        if not fresh:
            return JSONResponse(json.loads(prior['result']) if prior['result'] else
                                {'state': 'unconfirmed', 'detail': 'Inspect control status before another action'},
                                status_code=200 if prior['result'] else 202)
        try:
            # Sync control runs off the event loop; reads stay available during recovery.
            from starlette.concurrency import run_in_threadpool
            if kind == 'stop':
                result = await run_in_threadpool(provider.stop, nonce)
            else:
                result = await run_in_threadpool(provider.resume, nonce, operation)
        except Exception:
            result = {'state': 'unconfirmed', 'detail': kind.title() + ' acknowledgement unavailable; inspect status before retrying'}
        with database() as db:
            db.execute('UPDATE actions SET state=?,result=? WHERE human=? AND nonce=?',
                       (result.get('state', 'unconfirmed'), json.dumps(result), session['human'], journal_nonce))
        return JSONResponse(result, status_code=202)

    return app
