"""Explicit bot menu selection; never starts or replaces the update consumer."""
import argparse
import hashlib
import json
from pathlib import Path
import tomllib
import urllib.request

from scripts.activity_indicator import Telegram
from .private_state import private_directory, read_json, write_json


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--config', required=True)
    parser.add_argument('operation', choices=['inspect-menu', 'publish-menu', 'restore-menu'])
    args = parser.parse_args()
    config = tomllib.loads(Path(args.config).read_text())
    receiver_bytes = Path(config['receiver_config']).read_bytes()
    receiver = tomllib.loads(receiver_bytes.decode())
    telegram = Telegram(receiver['telegram'])  # Existing safe token-file reader, operator only.
    human = receiver['session_control']['human_id']
    state = private_directory(config['state_dir'])
    binding = {'bot_id':config['bot_id'],'human':human,
               'receiver_hash':hashlib.sha256(receiver_bytes).hexdigest()}

    def api(method, payload):
        request = urllib.request.Request(f'{telegram.base}/bot{telegram.token}/{method}',
                    data=json.dumps(payload).encode(), headers={'Content-Type':'application/json'})
        try:
            with urllib.request.urlopen(request, timeout=10) as response:
                result = json.load(response)
            if result.get('ok') is not True:
                raise ValueError()
            return result['result']
        except Exception:
            # Credential-bearing URLs/exceptions never enter logs or model output.
            raise SystemExit('Telegram menu operation unconfirmed') from None

    bot = api('getMe', {})
    if bot.get('id') != config['bot_id'] or bot.get('username') != config.get('bot_username'):
        raise SystemExit('Receiving bot binding mismatch')
    previous = api('getChatMenuButton', {'chat_id':human})
    if args.operation == 'inspect-menu':
        print(json.dumps({'menu_type':previous['type'], 'text':previous.get('text'),
                          'has_main_web_app':bot.get('has_main_web_app', False)}))
        return
    rollback = state / 'menu-before.json'
    selected = {'type':'web_app','text':'Sessions','web_app':{'url':config['origin'].rstrip('/')+config.get('prefix','/sessions')+'/'}}
    if args.operation == 'restore-menu':
        record = read_json(rollback)
        if record.get('binding') != binding:
            raise SystemExit('Menu rollback belongs to another receiving scope')
        selected = record['menu']
    elif rollback.exists() or rollback.is_symlink():
        if read_json(rollback).get('binding') != binding:
            raise SystemExit('Existing menu rollback belongs to another receiving scope')
    else:
        write_json(rollback, {'binding':binding,'menu':previous})
    if Path(config['receiver_config']).read_bytes() != receiver_bytes:
        raise SystemExit('Receiving configuration changed; no menu mutation requested')
    api('setChatMenuButton', {'chat_id':human, 'menu_button':selected})
    observed = api('getChatMenuButton', {'chat_id':human})
    if observed != selected:
        raise SystemExit('Selected menu was not observed')
    write_json(state / 'menu-receipt.json', {'binding':binding,'operation':args.operation,'menu':observed})
    print(json.dumps({'state':'observed','menu_type':observed['type'],'text':observed.get('text')}))


if __name__ == '__main__':
    main()
