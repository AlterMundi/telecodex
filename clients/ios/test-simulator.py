"""One disposable simulator and loopback HTTPS fixture; no capture/provider/login."""
import argparse
import http.server
import json
import os
from pathlib import Path
import ssl
import subprocess
import tempfile
import time
import uuid

ROOT = Path(__file__).resolve().parent


def serve(state):
    class Handler(http.server.BaseHTTPRequestHandler):
        def do_GET(self):
            print(json.dumps({'fixture_get': self.path[:256]}), flush=True)
            path = self.path.split('?',1)[0]
            children = {'/interaction/audio-iframe/':'audio-iframe.html',
                        '/interaction/capture-iframe/':'capture-iframe.html'}
            name = children.get(path) or ('conversation.html' if path.startswith('/interaction/') else 'outside.html')
            body = (ROOT / 'Tests/Fixtures' / name).read_bytes()
            self.send_response(200); self.send_header('Content-Type', 'text/html; charset=utf-8')
            self.send_header('Content-Length', str(len(body))); self.end_headers(); self.wfile.write(body)

        def do_POST(self):
            length = int(self.headers.get('Content-Length', '-1'))
            if self.path != '/result' or not 0 < length <= 8192:
                self.send_error(400); return
            value = json.loads(self.rfile.read(length))
            with (state / 'result.json').open('x') as result:
                json.dump(value, result)
            self.send_response(204); self.end_headers()

        def log_message(self, *args):
            pass

    server = http.server.ThreadingHTTPServer(('127.0.0.1', 443), Handler)
    context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
    context.load_cert_chain(str(state / 'server.crt'), str(state / 'server.key'))
    server.socket = context.wrap_socket(server.socket, server_side=True)
    (state / 'server.pid').write_text(str(os.getpid()))
    server.serve_forever(poll_interval=.1)


def run(argv, *, timeout=30):
    started = time.monotonic()
    try:
        result = subprocess.run(argv, capture_output=True, text=True, timeout=timeout)
    except subprocess.TimeoutExpired as error:
        details = (error.stdout or b'') + (error.stderr or b'')
        print(json.dumps({'command':argv[:4],'state':'observation_timeout','seconds':timeout,
                          'partial_output':details[-12000:].decode(errors='replace')}),flush=True)
        raise
    print(json.dumps({'command':argv[:4],'exit':result.returncode,'elapsed':round(time.monotonic()-started,2)}),flush=True)
    if result.returncode:
        raise RuntimeError(f'{argv[0]} exit {result.returncode}: {(result.stdout + result.stderr)[-12000:]}')
    return result.stdout


def qualify(app):
    device = None
    server = None
    server_log = None
    launch_attempted = False
    passed = False
    with tempfile.TemporaryDirectory(prefix='daimon-ios-', dir=os.environ.get('RUNNER_TEMP')) as temporary:
        state = Path(temporary)
        try:
            # A one-day fixture CA is trusted only by this newly created simulator.
            # Production app trust and every existing device/keychain stay unchanged.
            (state/'ca.cnf').write_text('[req]\ndistinguished_name=dn\n[dn]\n[v3_ca]\nbasicConstraints=critical,CA:TRUE\nkeyUsage=critical,keyCertSign,cRLSign\n')
            run(['openssl','req','-x509','-newkey','rsa:2048','-nodes','-sha256','-days','1',
                 '-subj','/CN=Daimon Simulator Fixture CA','-config',str(state/'ca.cnf'),'-extensions','v3_ca',
                 '-keyout',str(state/'ca.key'),'-out',str(state/'ca.crt')])
            run(['openssl','req','-new','-newkey','rsa:2048','-nodes','-subj','/CN=localhost',
                 '-keyout',str(state/'server.key'),'-out',str(state/'server.csr')])
            (state/'server.ext').write_text('subjectAltName=DNS:localhost,IP:127.0.0.1\nextendedKeyUsage=serverAuth\nbasicConstraints=CA:FALSE\n')
            run(['openssl','x509','-req','-in',str(state/'server.csr'),'-CA',str(state/'ca.crt'),
                 '-CAkey',str(state/'ca.key'),'-CAcreateserial','-days','1','-sha256',
                 '-extfile',str(state/'server.ext'),'-out',str(state/'server.crt')])
            server_log = (state/'server.log').open('wb')
            server = subprocess.Popen(['sudo','-n','python3',str(Path(__file__).resolve()),'--serve',str(state)],
                                      stdout=server_log, stderr=server_log)
            until = time.monotonic()+10
            while not (state/'server.pid').exists() and time.monotonic()<until:
                if server.poll() is not None: raise RuntimeError('Loopback HTTPS fixture exited before readiness: '+(state/'server.log').read_text()[-8192:])
                time.sleep(.1)
            assert (state/'server.pid').exists(), 'Loopback HTTPS fixture did not become ready'
            print('Reading installed simulator runtimes; bounded120seconds, no downloads',flush=True)
            runtimes = json.loads(run(['xcrun','simctl','list','runtimes','--json'],timeout=120))['runtimes']
            runtimes = [r for r in runtimes if r.get('isAvailable') and '.iOS-' in r['identifier']]
            assert runtimes, 'No installed available iOS runtime; none was downloaded'
            runtime = max(runtimes,key=lambda r:tuple(map(int,r['version'].split('.'))))
            types = json.loads(run(['xcrun','simctl','list','devicetypes','--json']))['devicetypes']
            phones = [t for t in types if t['name'].startswith('iPhone')]
            phone = next((p for p in phones if p['name']=='iPhone 16'),phones[-1])
            device = run(['xcrun','simctl','create','Daimon-'+uuid.uuid4().hex[:12],phone['identifier'],runtime['identifier']]).strip()
            print(json.dumps({'device':device,'runtime':runtime['identifier'],'phone':phone['name']}),flush=True)
            run(['xcrun','simctl','boot',device]); run(['xcrun','simctl','bootstatus',device,'-b'],timeout=120)
            run(['xcrun','simctl','keychain',device,'add-root-cert',str(state/'ca.crt')],timeout=90)
            try:
                run(['xcrun','simctl','install',device,str(app)],timeout=90)
            except subprocess.TimeoutExpired:
                # The native installation may have committed after its CLI lost
                # observation. Read this same unique device; never install again.
                installed = Path(run(['xcrun','simctl','get_app_container',device,
                                      'homes.dmn.interaction.qualification','app'],timeout=15).strip())
                assert (installed/'native-audio.js').read_bytes()==(app/'native-audio.js').read_bytes()
                print('Exact installed adapter observed after lost install receipt; no reinstall',flush=True)
            launch_attempted = True
            try:
                run(['xcrun','simctl','launch',device,'homes.dmn.interaction.qualification','--qualification-home','https://localhost'],timeout=90)
            except subprocess.TimeoutExpired:
                print('Launch receipt unconfirmed; observing this same fixture result without relaunch',flush=True)
            until = time.monotonic()+35
            while not (state/'result.json').exists() and time.monotonic()<until:
                if server.poll() is not None: raise RuntimeError('HTTPS fixture stopped during the journey: '+(state/'server.log').read_text()[-8192:])
                time.sleep(.2)
            if not (state/'result.json').exists():
                print('Loopback fixture observations: '+(state/'server.log').read_text()[-8192:],flush=True)
                try:
                    container = Path(run(['xcrun','simctl','get_app_container',device,
                                          'homes.dmn.interaction.qualification','data'],timeout=15).strip())
                    stage = container/'Documents/qualification-stage.txt'
                    print(json.dumps({'qualification_stage':stage.read_text()[:512] if stage.exists() else 'not_observed'}),flush=True)
                except (subprocess.TimeoutExpired,RuntimeError,OSError) as error:
                    print(json.dumps({'stage_read':'unconfirmed','error':str(error)[-512:]}),flush=True)
                try:
                    print(run(['xcrun','simctl','spawn',device,'log','show','--last','3m','--style','compact',
                               '--predicate','process == "Daimon"'],timeout=10)[-16384:])
                except (subprocess.TimeoutExpired,RuntimeError) as error:
                    print(json.dumps({'native_log':'unconfirmed','error':str(error)[-512:]}),flush=True)
                raise RuntimeError('No terminal WebView/native receipt within35seconds; no relaunch')
            result = json.loads((state/'result.json').read_text()); print(json.dumps(result),flush=True)
            assert (result.get('state')=='passed' and result.get('played_bytes')==28800
                    and result.get('capture_iframe')=='refused'
                    and result.get('duplex_reservation')=='held_across_pcm_idle'
                    and result.get('microphone_requested') is False), result
            passed = True
        finally:
            cleanup = []
            try:
                if device:
                    commands = []
                    if launch_attempted: commands.append(['xcrun','simctl','terminate',device,'homes.dmn.interaction.qualification'])
                    commands += [['xcrun','simctl','shutdown',device],['xcrun','simctl','delete',device]]
                    for command in commands:
                        try:
                            run(command,timeout=30); cleanup.append({'action':command[2],'state':'acknowledged'})
                        except (subprocess.TimeoutExpired,RuntimeError) as error:
                            cleanup.append({'action':command[2],'state':'unconfirmed','error':str(error)[-1000:]})
            finally:
                if server:
                    if server.poll() is None and (state/'server.pid').exists():
                        subprocess.run(['sudo','-n','kill','-TERM',(state/'server.pid').read_text().strip()],capture_output=True,timeout=10)
                    server.wait(timeout=10)
                if server_log: server_log.close()
            print(json.dumps({'cleanup':cleanup,'fixture_server_stopped':server is None or server.poll() is not None}),flush=True)
            if passed:
                assert any(c['action']=='delete' and c['state']=='acknowledged' for c in cleanup),'Simulator deletion not confirmed'
    print('Owned simulator, HTTPS fixture, one-day CA and temporary state removed',flush=True)


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--serve',type=Path)
    parser.add_argument('--app',type=Path)
    args = parser.parse_args()
    if args.serve: serve(args.serve)
    elif args.app: qualify(args.app.resolve())
    else: parser.error('Select --app or the fixture server mode')
