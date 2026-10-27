"""Reuse a registered home's shared browser session for conversation access."""
import hashlib
import http.client
import json
import re
import time
from urllib.parse import urlsplit

from .companion import permitted


def identity_response(origin, token):
    target = urlsplit(origin)
    connection = http.client.HTTPSConnection(target.hostname, target.port, timeout=3)
    try:
        connection.request('GET', '/access/identity', headers={'Cookie': '__Host-home=' + token,
                                                             'Accept': 'application/json'})
        response = connection.getresponse()
        body = response.read(8193)
        if response.status == 401:
            raise ValueError('Shared sign-in required')
        if response.status != 200 or len(body) > 8192:
            raise OSError('Shared authentication unavailable')
        return json.loads(body)
    finally:
        connection.close()


class HomeAuth:
    def __init__(self, config, provider):
        self.provider = provider
        self.selected = config.get('shared_auth', {})
        self.enabled = self.selected.get('enabled') is True
        if not self.enabled:
            return
        self.origin = self.selected['home_origin']
        target = urlsplit(self.origin)
        self.entry = self.selected.get('entry_path', '/interaction/')
        if (target.scheme != 'https' or not target.hostname or target.username or target.password
                or target.path or target.query or target.fragment
                or not re.fullmatch(r'/[A-Za-z0-9_-]+/', self.entry)
                or any(ord(c) < 33 for c in self.origin)):
            raise ValueError('Exact registered HTTPS home and application path required')
        self.host = target.netloc
        self.artifacts = self.selected.get('artifact_access', False)
        if type(self.artifacts) is not bool:
            raise ValueError('Explicit shared conversation file permission required')
        self.alerts = self.selected.get('alerts_access', False)
        if type(self.alerts) is not bool:
            raise ValueError('Explicit shared conversation alert permission required')
        self.binding = config['origin'] + config.get('prefix', '/sessions')

    def authenticate(self, request):
        if not self.enabled or request.headers.get('host') != self.host:
            return None
        token = request.cookies.get('__Host-home', '')
        if not re.fullmatch(r'[A-Za-z0-9_-]{43}', token):
            return None
        value = identity_response(self.origin, token)
        if not isinstance(value, dict) or not isinstance(value.get('actor'), dict):
            raise ValueError('Shared identity unavailable')
        actor = value.get('actor', {})
        expires = value.get('expires')
        if type(expires) is not int or expires <= time.time():
            raise ValueError('Shared sign-in expired')
        human = self.provider.human
        if (value.get('contract') != 'dm.home-session/v1' or value.get('origin') != self.origin
                or actor.get('kind') != 'human' or actor.get('ref') != 'human:telegram:' + str(human)
                or not self.provider.authorized(human)):
            raise PermissionError('This account has no receiving conversation grant')
        if request.url.path != '/api/interaction/access' and not permitted(
                request.method, request.url.path, 'interaction_only', self.artifacts, self.alerts):
            raise PermissionError('Shared browser access is limited to the conversation')
        return {'human': human, 'scope': 'interaction_only', 'artifact_access': self.artifacts,
                'alerts_access': self.alerts, 'expires': expires,
                'csrf': hashlib.sha256(('shared-conversation:' + self.binding + ':' + token).encode()).hexdigest(),
                'label': actor.get('name', 'Your conversation'), 'presentation_profile': 'simple'}
