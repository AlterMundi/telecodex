"""Optional foreground, owner-authorized computer readings for Pulso.

Reuses computer-health's collector/viewer, Sessions identity and one bounded
current report. No daemon, publisher, native observer or model is started.
"""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import stat
import threading
import time


class HealthUnavailable(Exception):
    pass


def owned_file(path, private=False, limit=262144):
    path = Path(path)
    info = path.lstat()
    if (not stat.S_ISREG(info.st_mode) or info.st_uid != os.getuid()
            or info.st_mode & (0o077 if private else 0o022) or info.st_size > limit):
        raise ValueError('Owner-controlled bounded regular file required')
    return path.read_bytes()


class ComputerHealth:
    def __init__(self, config):
        selected = config.get('computer_health', {})
        self.enabled = selected.get('enabled') is True
        self.default_view = self.enabled and selected.get('default_view', False) is True
        if not self.enabled:
            return
        if set(selected) - {'enabled', 'default_view', 'package_dir', 'config_file'}:
            raise ValueError('Unknown computer_health setting')
        self.package = Path(selected['package_dir'])
        self.config_file = Path(selected['config_file'])
        for path in (self.package, self.config_file):
            if not path.is_absolute() or any(parent.is_symlink() for parent in (path, *path.parents)):
                raise ValueError('Absolute non-symlink health binding required')
        self.profile_bytes = owned_file(self.config_file, private=True, limit=131072)
        self.profile = json.loads(self.profile_bytes)
        self.profile_hash = hashlib.sha256(self.profile_bytes).hexdigest()
        metadata = json.loads(owned_file(self.package / 'package.json'))
        if metadata.get('name') != 'computer-health' or tuple(map(int, metadata['version'].split('.'))) < (1, 1, 0):
            raise ValueError('Qualified computer-health 1.1.0 or newer required')
        source = self.package / 'scripts' / 'pulso.py'
        template = self.package / 'assets' / 'pulso.html'
        self.pins = {path: hashlib.sha256(owned_file(path)).digest() for path in (source, template)}
        spec = importlib.util.spec_from_file_location('receiving_computer_health', source)
        self.collector = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(self.collector)
        self.collector.validate_config(self.profile)
        # Use the maintained template and externalize executable content for
        # Sessions' existing strict CSP. This is a trusted backend shell, not
        # an artifact whose sandbox or access policy is being weakened.
        shell = self.collector.render(appearance_origin=config.get('appearance_origin'))
        styles = re.findall(r'<style>\n(.*?)</style>', shell, re.S)
        scripts = re.findall(r'<script>\n(.*?)</script>', shell, re.S)
        if len(styles) != 1 or len(scripts) != 1 or "pulso:report" not in scripts[0]:
            raise ValueError('Compatible Pulso renderer required')
        shell = re.sub(r'<meta http-equiv="Content-Security-Policy"[^>]*>\n', '', shell)
        shell = re.sub(r'<style>\n.*?</style>', '<link rel="stylesheet" href="assets/health.css">', shell, flags=re.S)
        shell = re.sub(r'<script>\n.*?</script>', '', shell, flags=re.S)
        shell = shell.replace('</head>', '<script src="https://telegram.org/js/telegram-web-app.js" defer></script>\n<script src="assets/health-ui.js" defer></script>\n<script src="assets/health-live.js" defer></script></head>')
        self.shell = shell
        self.assets = {'health.css': (styles[0].encode(), 'text/css'),
                       'health-ui.js': (scripts[0].encode(), 'text/javascript')}
        self.state_file = Path(config['state_dir']) / 'computer-health.json'
        self.lock = threading.Lock()
        self.running = False
        self.report = None
        self.failure = False
        self.last_attempt = 0.0
        self.closed = False
        try:
            saved = json.loads(owned_file(self.state_file, private=True, limit=131072))
            if saved.get('profile_sha256') == self.profile_hash:
                self.report = saved['report']
        except (OSError, ValueError, KeyError):
            pass

    def assert_binding(self):
        try:
            if owned_file(self.config_file, private=True, limit=131072) != self.profile_bytes:
                raise ValueError()
            for path, pinned in self.pins.items():
                if hashlib.sha256(owned_file(path)).digest() != pinned:
                    raise ValueError()
        except (OSError, ValueError):
            raise HealthUnavailable('Computer health binding changed; operator must requalify') from None

    def snapshot(self):
        if not self.enabled:
            return {'state': 'not_configured'}
        self.assert_binding()
        with self.lock:
            if not self.closed and not self.running and time.monotonic() - self.last_attempt >= 30:
                self.running = True
                self.last_attempt = time.monotonic()
                threading.Thread(target=self._collect, name='pulso-owner-read', daemon=True).start()
            return {'state': 'available' if self.report else 'collecting' if self.running else 'unavailable',
                    'updating': self.running, 'last_attempt_failed': self.failure,
                    'report': self.report, 'refresh_seconds': 30}

    def _collect(self):
        try:
            report = self.collector.collect(self.profile, self.report)
            self.assert_binding()
            content = json.dumps({'profile_sha256': self.profile_hash, 'report': report}, allow_nan=False)
            if len(content.encode()) > 131072:
                raise ValueError('Report exceeds supported bound')
            self.collector.atomic_private(self.state_file, content)
            with self.lock:
                self.report = report
                self.failure = False
        except (OSError, ValueError, HealthUnavailable):
            with self.lock:
                self.failure = True
        finally:
            with self.lock:
                self.running = False

    def close(self):
        self.closed = True
