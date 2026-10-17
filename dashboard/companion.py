"""Explicit owner enrollment for interaction-only devices; no native owner."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import secrets
import time
import tomllib

from .private_state import private_directory, read_json, write_json
from scripts.speech_render import directory, sync_directory, RenderError


def authority(config, provider):
    return hashlib.sha256(json.dumps([config['bot_id'], config['origin'],
        config.get('prefix', '/sessions'), provider.binding_bytes.hex()],
        separators=(',', ':')).encode()).hexdigest()


def permitted(method, path):
    if method == 'GET':
        return re.fullmatch(r'/api/interaction(?:/(?:part|inputs(?:/[a-f0-9-]{36}(?:/image)?)?|requests(?:/[a-f0-9]{32}(?:/receipt)?)?))?', path) is not None
    if method == 'POST':
        return path in ('/api/interaction/inputs', '/api/interaction/voice', '/api/interaction/visual', '/api/interaction/audio') or re.fullmatch(r'/api/interaction/inputs/[a-f0-9-]{36}/route', path) is not None
    return False


class Grants:
    def __init__(self, config, provider):
        selected = config.get('companion', {})
        self.root = None
        self.provider = provider
        self.config = config
        if selected.get('enabled') is True:
            path = Path(selected['grants_dir'])
            if not path.is_absolute():
                raise ValueError('Absolute private companion grant directory required')
            self.root = directory(path)

    def authenticate(self, token):
        if self.root is None or not re.fullmatch(r'[A-Za-z0-9_-]{43}', token):
            return None
        try:
            private_directory(self.root)
            grant = read_json(self.root / (hashlib.sha256(token.encode()).hexdigest() + '.json'))
            if not isinstance(grant, dict):
                return None
            now = time.time()
            if (grant.get('schema') != 'telecodex.companion-grant/v1'
                    or grant.get('scope') != 'interaction_only'
                    or grant.get('authority') != authority(self.config, self.provider)
                    or type(grant.get('human')) is not int
                    or type(grant.get('expires')) is not int
                    or not now < grant['expires'] <= now + 30 * 86400
                    or not isinstance(grant.get('csrf'), str)
                    or not re.fullmatch(r'[A-Za-z0-9_-]{43}', grant['csrf'])
                    or not self.provider.authorized(grant['human'])):
                return None
            return grant
        except (OSError, ValueError, KeyError, TypeError):
            return None

    def enroll(self, label, output, seconds):
        if (self.root is None or not isinstance(label, str) or not 1 <= len(label) <= 64
                or any(ord(c) < 32 for c in label)
                or type(seconds) is not int or not 60 <= seconds <= 30 * 86400):
            raise ValueError('Enabled companion route, bounded label and lifetime required')
        output = Path(output)
        if not output.is_absolute() or output.exists() or output.is_symlink():
            raise ValueError('Select a new absolute private device credential file')
        self.provider.assert_binding()
        human = self.provider.human
        if not self.provider.authorized(human):
            raise ValueError('Receiving human grant is unavailable')
        # Use the real owned entry before minting anything. The existing native
        # controller continues to enforce its owner, enrolled targets and actions.
        self.provider.presentation_list(0, 0)
        token, csrf = secrets.token_urlsafe(32), secrets.token_urlsafe(32)
        ident = hashlib.sha256(token.encode()).hexdigest()
        grant = {'schema':'telecodex.companion-grant/v1', 'scope':'interaction_only',
            'label':label, 'human':human, 'authority':authority(self.config, self.provider),
            'expires':int(time.time()) + seconds, 'csrf':csrf}
        credential = {'schema':'telecodex.companion-device/v1',
            'origin':self.config['origin'], 'prefix':self.config.get('prefix', '/sessions'),
            'authorization':'Companion ' + token, 'csrf':csrf,
            'expires':grant['expires'], 'grant_id':ident}
        self.provider.assert_binding()
        if not self.provider.authorized(human):
            raise ValueError('Receiving human grant changed during enrollment')
        record = self.root / (ident + '.json')
        write_json(record, grant)
        try:
            # Publish the device credential only after its receiving grant exists.
            directory(output.parent)
            fd = os.open(output, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
            try:
                with os.fdopen(fd, 'w') as stream:
                    json.dump(credential, stream)
                    stream.flush()
                    os.fsync(stream.fileno())
                sync_directory(output.parent)
            except BaseException:
                output.unlink()
                raise
        except BaseException:
            record.unlink()
            sync_directory(self.root)
            raise
        return {'grant_id':ident, 'scope':'interaction_only', 'expires':grant['expires']}

    def revoke(self, ident):
        if self.root is None or not re.fullmatch(r'[a-f0-9]{64}', ident):
            raise ValueError('Select an enrolled companion grant')
        path = self.root / (ident + '.json')
        if not path.exists():
            sync_directory(self.root)
            return {'revoked':True, 'already_absent':True}
        read_json(path)
        path.unlink()
        sync_directory(self.root)
        return {'revoked':True}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--config', required=True, type=Path)
    commands = parser.add_subparsers(dest='command', required=True)
    enroll = commands.add_parser('enroll')
    enroll.add_argument('--label', required=True)
    enroll.add_argument('--output', required=True, type=Path)
    enroll.add_argument('--seconds', type=int, default=86400)
    revoke = commands.add_parser('revoke')
    revoke.add_argument('grant_id')
    args = parser.parse_args()
    from .provider import Provider
    provider = None
    try:
        config = tomllib.loads(args.config.read_text())
        provider = Provider(config)
        grants = Grants(config, provider)
        result = grants.enroll(args.label, args.output, args.seconds) if args.command == 'enroll' else grants.revoke(args.grant_id)
        print(json.dumps(result))  # Never emit the credential or its private path.
    except (OSError, ValueError, KeyError, RenderError):
        parser.exit(2, 'Companion operation unavailable; inspect owner-local configuration and grant.\n')
    finally:
        if provider is not None:
            provider.close()


if __name__ == '__main__':
    main()
