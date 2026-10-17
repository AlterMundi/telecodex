"""Per-human menu projection, bounded writes and preserved external selection."""
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import Mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from dashboard.menu_attention import MenuAttention, MenuUnavailable, attention
from dashboard.private_state import read_json


def view(severity=None):
    return dict(state='available', observer_state='healthy', incidents=[] if severity is None else [
        {'projection':{'severity':severity,'processing_state':'open'},'acknowledgments':[]}])


class MenuTests(unittest.TestCase):
    def test_severity_ack_history_and_unknown(self):
        self.assertEqual(attention(view()), 'clear')
        self.assertEqual(attention(view('critical')), 'error')
        self.assertEqual(attention(view('warning')), 'warning')
        v = view('critical'); v['incidents'][0]['acknowledgments'] = [{'actor':'fictional'}]
        self.assertEqual(attention(v), 'clear')
        v['incidents'][0]['projection']['processing_state'] = 'resolved'
        v['incidents'][0]['acknowledgments'] = []
        self.assertEqual(attention(v), 'clear')
        self.assertEqual(attention(dict(view(), state='unavailable')), 'unknown')
        self.assertEqual(attention(dict(view(), truncated=True)), 'unknown')

    def test_menu_target_preservation_throttle_rollback_and_external_change(self):
        with tempfile.TemporaryDirectory() as tmp:
            provider = Mock(human=100); provider.authorized.return_value = True
            sender = Mock(spec=['call', 'verify', 'assert_binding'])
            current = {'type':'web_app','text':'Apps','web_app':{'url':'https://example.test/apps/'}}
            original = dict(current)
            calls = []
            def call(method, data):
                nonlocal current
                calls.append((method, data))
                if method == 'getChatMenuButton': return {'ok':True, 'result':current}
                current = data['menu_button']; return {'ok':True, 'result':True}
            sender.call.side_effect = call
            path = Path(tmp)/'menu.json'
            menu = MenuAttention(sender, provider, path, 'fictional-binding')
            self.assertEqual(menu.tick(view('critical')), 'observed')
            self.assertEqual(current['text'], '🔴 Apps')
            self.assertEqual(current['web_app'], original['web_app'])
            self.assertTrue(all(data['chat_id'] == 100 for _,data in calls))
            self.assertEqual(read_json(path)['original'], original)
            count = len(calls)
            self.assertEqual(menu.tick(view('warning')), 'deferred')
            self.assertEqual(len(calls), count)
            menu.next_check = 0; menu.tick(view())
            self.assertEqual(current, original)
            current = {**original, 'web_app':{'url':'https://example.test/different/'}}
            menu.next_check = 0
            with self.assertRaises(MenuUnavailable): menu.tick(view('critical'))
            self.assertEqual(current['web_app']['url'], 'https://example.test/different/')
            provider.authorized.return_value = False
            menu.next_check = 0
            with self.assertRaises(MenuUnavailable): menu.tick(view('critical'))

    def test_ambiguous_setter_reconciles_without_extra_write(self):
        with tempfile.TemporaryDirectory() as tmp:
            provider = Mock(human=100); provider.authorized.return_value = True
            sender = Mock(spec=['call', 'verify', 'assert_binding'])
            current = {'type':'web_app','text':'Apps','web_app':{'url':'https://example.test/apps/'}}
            writes = []
            def call(method, data):
                nonlocal current
                if method == 'getChatMenuButton': return {'ok':True, 'result':current}
                current = data['menu_button']; writes.append(current)
                return None  # Accepted setter with lost reply.
            sender.call.side_effect = call
            menu = MenuAttention(sender, provider, Path(tmp)/'menu.json', 'fictional')
            with self.assertRaises(MenuUnavailable): menu.tick(view('critical'))
            menu.next_check = 0
            self.assertEqual(menu.tick(view('critical')), 'observed')
            self.assertEqual(len(writes), 1)


if __name__ == '__main__': unittest.main()
