"""Scoped health HTTP, single-flight, stale reading and renderer integration."""
import importlib.util
import json
import os
from pathlib import Path
import shutil
import sys
import tempfile
import threading
import time
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from fastapi.testclient import TestClient
from dashboard.app import create_app
from dashboard.computer_health import ComputerHealth, HealthUnavailable

PACKAGE = os.environ.get('PULSO_TEST_PACKAGE')


class Receiver:
    human = 100
    allowed = True
    snapshots = 0
    def authorized(self, human): return self.allowed and human == self.human
    def snapshot(self):
        self.snapshots += 1
        raise AssertionError('Health must not read conversations or the native provider')
    def close(self): pass


class HealthTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.package = self.root / 'package'
        if PACKAGE:
            shutil.copytree(PACKAGE, self.package)
        else:
            # A small contract fixture keeps authorization and single-flight
            # tests offline. PULSO_TEST_PACKAGE additionally qualifies the real
            # maintained collector/renderer, not an implementation copy.
            (self.package / 'scripts').mkdir(parents=True)
            (self.package / 'assets').mkdir()
            (self.package / 'package.json').write_text(json.dumps({'name': 'computer-health', 'version': '1.1.0'}))
            source = '''import json,os,tempfile
from pathlib import Path
def validate_config(profile):
    if not profile.get('targets'):raise ValueError()
def collect(profile,previous):
    return {'schema':'computer-health/v1','mode':'on-demand','generated_at':'2026-01-01T00:00:00Z','targets':[{'id':'local','state':'available'}]}
def atomic_private(path,content):
    fd,tmp=tempfile.mkstemp(dir=Path(path).parent)
    with os.fdopen(fd,'w') as stream:stream.write(content)
    os.replace(tmp,path)
def render(appearance_origin=None):
    return (Path(__file__).parent.parent/'assets/pulso.html').read_text()
'''
            (self.package / 'scripts/pulso.py').write_text(source)
            (self.package / 'assets/pulso.html').write_text('<html><head><style>\nbody{color:black}</style></head><body><script>\ndocument.addEventListener("pulso:report",()=>{});</script></body></html>')
        self.profile = self.root / 'profile.json'
        self.profile.write_text(json.dumps({'targets': [{'id': 'local', 'label': 'Container',
                                                       'scope': 'Selected container'}]}))
        self.profile.chmod(0o600)
        self.config = {'state_dir': str(self.root), 'origin': 'https://example.test', 'prefix': '/sessions', 'bot_id': 123,
                       'computer_health': {'enabled': True, 'default_view': True,
                                          'package_dir': str(self.package), 'config_file': str(self.profile)}}
        self.receiver = Receiver()

    def client(self):
        return TestClient(create_app(self.config, self.receiver, lambda raw, bot: int(raw)), base_url='https://example.test')

    def auth(self, client):
        response = client.post('/api/session', json={'init_data': '100'}, headers={'origin': 'https://example.test'})
        self.assertEqual(response.status_code, 200)
        return {'authorization': 'Bearer ' + response.json()['session_token']}

    def test_owner_authentication_no_native_read_and_live_shell(self):
        with self.client() as client:
            self.assertEqual(client.get('/api/computer-health').status_code, 401)
            self.assertEqual(client.post('/api/session', json={'init_data': '101'}, headers={'origin': 'https://example.test'}).status_code, 403)
            shell = client.get('/')
            self.assertEqual(shell.status_code, 200)
            self.assertIn('assets/health-live.js', shell.text)
            self.assertLess(shell.text.index('assets/health-ui.js'), shell.text.index('assets/health-live.js'))
            self.assertNotIn('<style>', shell.text)
            self.assertNotIn('<script>\n', shell.text)
            self.assertNotIn("'unsafe-inline'", shell.headers['content-security-policy'])
            self.assertIn('Sessions', client.get('/?view=sessions').text)
            for name in ['health-ui.js', 'health.css', 'health-live.js']:
                self.assertEqual(client.get('/assets/' + name).status_code, 200)
            auth = self.auth(client)
            first = client.get('/api/computer-health', headers=auth)
            self.assertEqual(first.status_code, 200)
            self.assertIn(first.json()['state'], ['collecting', 'available'])
            deadline = time.monotonic() + 8
            while time.monotonic() < deadline:
                result = client.get('/api/computer-health', headers=auth).json()
                if result['report']:
                    break
                time.sleep(.05)
            self.assertEqual(result['report']['targets'][0]['state'], 'available')
            self.assertNotIn('/sys/fs/cgroup', json.dumps(result))
            self.receiver.allowed = False
            denied = client.get('/api/computer-health', headers=auth)
            self.assertEqual(denied.status_code, 401)
            self.assertNotIn('current_bytes', denied.text)
            self.assertEqual(self.receiver.snapshots, 0)

    def test_single_flight_and_profile_drift_deny_old_metrics(self):
        health = ComputerHealth(self.config)
        self.addCleanup(health.close)
        entered, release = threading.Event(), threading.Event()
        calls = []
        original = health.collector.collect
        def slow(profile, previous):
            calls.append(profile)
            entered.set()
            self.assertTrue(release.wait(5))
            return original(profile, previous)
        with patch.object(health.collector, 'collect', side_effect=slow):
            self.assertTrue(health.snapshot()['updating'])
            self.assertTrue(entered.wait(2))
            for _ in range(5):
                self.assertTrue(health.snapshot()['updating'])
            self.assertEqual(len(calls), 1)
            release.set()
            deadline = time.monotonic() + 5
            while health.running and time.monotonic() < deadline:
                time.sleep(.05)
        self.assertIsNotNone(health.snapshot()['report'])
        self.profile.write_text('{"targets":[]}')
        with self.assertRaises(HealthUnavailable):
            health.snapshot()

    def test_revocation_between_read_and_response_denies_payload(self):
        app = create_app(self.config, self.receiver, lambda raw, bot: int(raw))
        with TestClient(app, base_url='https://example.test') as client:
            auth = self.auth(client)
            def snapshot(_):
                self.receiver.allowed = False
                return {'report': {'private': 'not-for-a-revoked-human'}}
            with patch.object(ComputerHealth, 'snapshot', snapshot):
                result = client.get('/api/computer-health', headers=auth)
            self.assertEqual(result.status_code, 401)
            self.assertNotIn('not-for-a-revoked-human', result.text)

    def test_restart_reads_only_matching_profile_and_renderer_drift_denies(self):
        health = ComputerHealth(self.config)
        report = {'schema': 'computer-health/v1', 'mode': 'on-demand', 'generated_at': '2026-01-01T00:00:00Z', 'targets': []}
        health.collector.atomic_private(health.state_file, json.dumps({'profile_sha256': health.profile_hash, 'report': report}))
        resumed = ComputerHealth(self.config)
        self.assertEqual(resumed.report, report)
        (self.package / 'assets/pulso.html').write_text('changed')
        with self.assertRaises(HealthUnavailable):
            resumed.snapshot()
        health.close();resumed.close()


if __name__ == '__main__':
    unittest.main()
