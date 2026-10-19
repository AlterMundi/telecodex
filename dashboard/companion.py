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
from scripts.speech_render import directory, sync_directory, lock, RenderError


def authority(config, provider):
    return hashlib.sha256(json.dumps([config['bot_id'], config['origin'],
        config.get('prefix', '/sessions'), provider.binding_bytes.hex()],
        separators=(',', ':')).encode()).hexdigest()


def permitted(method, path, scope='interaction_only'):
    if scope == 'monitor_readonly':
        return method == 'GET' and re.fullmatch(r'/api/interaction(?:/(?:part|telegram-inputs|inputs(?:/(?:[a-f0-9-]{36}|telegram:[0-9]+)(?:/trace(?:/(?:original|[a-f0-9-]{36}))?|/image)?)?))?', path) is not None
    if scope != 'interaction_only':
        return False
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
                    or grant.get('scope') not in ('interaction_only','monitor_readonly')
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

    def enroll(self, label, output, seconds, *, scope='interaction_only'):
        if (scope not in ('interaction_only','monitor_readonly') or self.root is None or not isinstance(label, str) or not 1 <= len(label) <= 64
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
        grant = {'schema':'telecodex.companion-grant/v1', 'scope':scope,
            'label':label, 'human':human, 'authority':authority(self.config, self.provider),
            'expires':int(time.time()) + seconds, 'csrf':csrf}
        credential = {'schema':'telecodex.companion-device/v1',
            'origin':self.config['origin'], 'prefix':self.config.get('prefix', '/sessions'),
            'authorization':'Companion ' + token, 'csrf':csrf,
            'expires':grant['expires'], 'grant_id':ident}
        if scope == 'monitor_readonly':
            credential['scope'] = scope
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
        return {'grant_id':ident, 'scope':scope, 'expires':grant['expires']}

    def revoke(self, ident):
        if self.root is None or not re.fullmatch(r'[a-f0-9]{64}', ident):
            raise ValueError('Select an enrolled companion grant')
        with lock(self.root / 'owner-mutation.lock'):
            path = self.root / (ident + '.json')
            if not path.exists():
                sync_directory(self.root)
                return {'revoked':True, 'already_absent':True}
            read_json(path)
            path.unlink()
            sync_directory(self.root)
            return {'revoked':True}

    def renew(self, ident, seconds):
        """Extend the same button enrollment without changing captured identity."""
        if (self.root is None or not re.fullmatch(r'[a-f0-9]{64}', ident)
                or type(seconds) is not int or not 60 <= seconds <= 30 * 86400):
            raise ValueError('Select an existing button grant and bounded lifetime')
        with lock(self.root / 'owner-mutation.lock'):
            self.provider.assert_binding()
            path = self.root / (ident + '.json')
            grant = read_json(path)  # Absent/revoked grants are never recreated.
            now = int(time.time())
            if (not isinstance(grant, dict)
                    or grant.get('schema') != 'telecodex.companion-grant/v1'
                    or grant.get('scope') != 'interaction_only'
                    or grant.get('authority') != authority(self.config, self.provider)
                    or type(grant.get('human')) is not int
                    or grant['human'] != self.provider.human
                    or type(grant.get('expires')) is not int
                    or not 0 < grant['expires'] <= now + 30 * 86400
                    or not isinstance(grant.get('csrf'), str)
                    or not re.fullmatch(r'[A-Za-z0-9_-]{43}', grant['csrf'])
                    or not self.provider.authorized(grant['human'])
                    or now + seconds <= grant['expires']):
                raise ValueError('Existing button authority and a later expiry required')
            self.provider.presentation_list(0, 0)
            self.provider.assert_binding()
            if (grant['authority'] != authority(self.config, self.provider)
                    or grant['human'] != self.provider.human
                    or not self.provider.authorized(grant['human'])):
                raise ValueError('Receiving human grant changed during renewal')
            renewed = {**grant, 'expires': now + seconds}
            write_json(path, renewed)
            return {'grant_id':ident, 'scope':grant['scope'], 'expires':renewed['expires'],
                    'previous_expires':grant['expires'], 'renewed':True}

    def reauthorize(self, ident, previous_authority):
        """Explicit owner adoption of a changed receiving profile, same device.

        Ordinary HTTP access and renewal never migrate authority. The owner names
        the retained old authority and validates the live new profile separately.
        No credential, scope, human or lifetime is changed by this operation.
        """
        if (self.root is None or not isinstance(ident,str)
                or not re.fullmatch(r'[a-f0-9]{64}',ident)
                or not isinstance(previous_authority,str)
                or not re.fullmatch(r'[a-f0-9]{64}',previous_authority)):
            raise ValueError('Select an existing grant and its exact previous authority')
        with lock(self.root / 'owner-mutation.lock'):
            self.provider.assert_binding()
            path = self.root / (ident + '.json')
            grant = read_json(path)
            current = authority(self.config,self.provider)
            now = int(time.time())
            if (not isinstance(grant,dict)
                    or grant.get('schema') != 'telecodex.companion-grant/v1'
                    or grant.get('scope') not in ('interaction_only','monitor_readonly')
                    or grant.get('authority') not in (previous_authority,current)
                    or type(grant.get('human')) is not int or grant['human'] != self.provider.human
                    or type(grant.get('expires')) is not int
                    or not now < grant['expires'] <= now + 30*86400
                    or not isinstance(grant.get('csrf'),str)
                    or not re.fullmatch(r'[A-Za-z0-9_-]{43}',grant['csrf'])
                    or not self.provider.authorized(grant['human'])):
                raise ValueError('Existing unexpired same-human grant and exact old authority required')
            self.provider.presentation_list(0,0)
            self.provider.assert_binding()
            if (authority(self.config,self.provider) != current
                    or grant['human'] != self.provider.human
                    or not self.provider.authorized(grant['human'])):
                raise ValueError('Receiving profile changed during explicit adoption')
            changed = grant['authority'] != current
            if changed:
                write_json(path,{**grant,'authority':current})
            return {'grant_id':ident,'scope':grant['scope'],'expires':grant['expires'],
                    'reauthorized':changed,'already_current':not changed}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--config', required=True, type=Path)
    commands = parser.add_subparsers(dest='command', required=True)
    enroll = commands.add_parser('enroll')
    enroll.add_argument('--label', required=True)
    enroll.add_argument('--output', required=True, type=Path)
    enroll.add_argument('--seconds', type=int, default=86400)
    enroll.add_argument('--scope', choices=['interaction_only','monitor_readonly'], default='interaction_only')
    revoke = commands.add_parser('revoke')
    revoke.add_argument('grant_id')
    renew = commands.add_parser('renew')
    renew.add_argument('grant_id')
    renew.add_argument('--seconds', required=True, type=int)
    adopt = commands.add_parser('reauthorize')
    adopt.add_argument('grant_id')
    adopt.add_argument('--previous-authority',required=True)
    args = parser.parse_args()
    from .provider import Provider
    provider = None
    try:
        config = tomllib.loads(args.config.read_text())
        provider = Provider(config)
        grants = Grants(config, provider)
        if args.command == 'enroll':
            result = grants.enroll(args.label, args.output, args.seconds, scope=args.scope)
        elif args.command == 'renew':
            result = grants.renew(args.grant_id, args.seconds)
        elif args.command == 'reauthorize':
            result = grants.reauthorize(args.grant_id,args.previous_authority)
        else:
            result = grants.revoke(args.grant_id)
        print(json.dumps(result))  # Never emit the credential or its private path.
    except (OSError, ValueError, KeyError, RenderError):
        parser.exit(2, 'Companion operation unavailable; inspect owner-local configuration and grant.\n')
    finally:
        if provider is not None:
            provider.close()


if __name__ == '__main__':
    main()
