"""Opt-in per-human Telegram menu label, driven by the existing Alerts snapshot."""
import time
from urllib.parse import urlsplit

from .private_state import read_json, write_json


class MenuUnavailable(ValueError):
    pass


SYMBOLS = {'clear':'', 'info':'🔵', 'warning':'🟠', 'error':'🔴', 'unknown':'⚪'}


def attention(view):
    ranks = {'info': 1, 'warning': 2, 'error': 3, 'critical': 3}
    pending = [row['projection'] for row in view.get('incidents', [])
               if row['projection']['processing_state'] == 'open' and not row.get('acknowledgments')]
    rank = max((ranks.get(p['severity'], 0) for p in pending), default=0)
    if rank >= 3:
        return 'error'
    if view.get('state') != 'available' or view.get('observer_state') != 'healthy' or view.get('truncated'):
        return 'unknown'
    return {0:'clear', 1:'info', 2:'warning'}[rank]


def attention_projection(view):
    """One safe, authenticated attention projection for menu, launcher and Sessions."""
    level = attention(view)
    pending = [r['projection'] for r in view.get('incidents', [])
               if r['projection']['processing_state'] == 'open' and not r.get('acknowledgments')]
    ranks = {'critical':4, 'error':3, 'warning':2, 'info':1}
    pending.sort(key=lambda p: ranks.get(p['severity'], 0), reverse=True)
    return {'schema':'telecodex.alert-attention/v1', 'level':level, 'symbol':SYMBOLS[level],
            'count':len(pending), 'observed_at':view.get('observed_at'),
            'summaries':[{'symbol':SYMBOLS['error' if p['severity'] in ('error','critical') else p['severity']],
                          'summary':p['summary']} for p in pending[:3]]}


class MenuAttention:
    def __init__(self, sender, provider, path, binding, label='Apps'):
        if label != 'Apps':
            raise ValueError('Only the explicitly selected Apps menu is supported')
        self.sender, self.provider, self.path, self.binding = sender, provider, path, binding
        self.next_check = 0
        self.last_level = None
        self.label = label

    def call(self, method, value):
        if not self.provider.authorized(self.provider.human):
            raise MenuUnavailable('menu_grant_unavailable')
        self.sender.assert_binding()
        result = self.sender.call(method, value)
        if not isinstance(result, dict) or result.get('ok') is not True:
            raise MenuUnavailable('menu_operation_unconfirmed')
        return result.get('result')

    def tick(self, view):
        level = attention(view)
        if time.monotonic() < self.next_check:
            return 'deferred'
        if level == self.last_level:
            return 'observed'
        # No animated polling and no per-probe Bot API writes. Failed updates are bounded too.
        self.next_check = time.monotonic() + 60
        self.sender.verify()
        payload = {'chat_id': self.provider.human}
        current = self.call('getChatMenuButton', payload)
        if self.path.exists() or self.path.is_symlink():
            record = read_json(self.path)
            if record.get('binding') != self.binding:
                raise MenuUnavailable('menu_binding_changed')
        else:
            if (not isinstance(current, dict) or current.get('type') != 'web_app'
                    or current.get('text') != self.label or set(current) != {'type','text','web_app'}):
                raise MenuUnavailable('selected_apps_menu_unavailable')
            url = current.get('web_app', {}).get('url', '')
            parsed = urlsplit(url)
            if parsed.scheme != 'https' or not parsed.hostname or parsed.username or parsed.password:
                raise MenuUnavailable('selected_apps_menu_unavailable')
            record = {'binding': self.binding, 'original': current, 'selected': current}
            write_json(self.path, record)
        prefix = SYMBOLS[level] + ' ' if SYMBOLS[level] else ''
        selected = {**record['original'], 'text':prefix+self.label}
        # Read before writing and refuse a third party's different label or destination.
        # After an ambiguous setter, the selected desired state is safe to observe/reconcile.
        if current not in (record['original'], record['selected'], record.get('pending'), selected):
            raise MenuUnavailable('menu_changed_externally')
        if current != selected:
            # Retain the desired setter before dispatch, including a lost response.
            record = {**record, 'pending':selected}
            write_json(self.path, record)
            if self.call('setChatMenuButton', {**payload, 'menu_button':selected}) is not True:
                raise MenuUnavailable('menu_operation_unconfirmed')
            current = self.call('getChatMenuButton', payload)
            if current != selected:
                raise MenuUnavailable('menu_operation_unconfirmed')
        write_json(self.path, {**record, 'selected':selected, 'pending':None, 'level':level, 'observed_at':int(time.time())})
        self.last_level = level
        return 'observed'
