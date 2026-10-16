#!/usr/bin/env python3
"""Device-free binding tests and isolated real tmux launch/idempotency tests."""
import json
import os
from pathlib import Path
import shutil
import sqlite3
import sys
import tempfile
import unittest
import uuid

sys.path.insert(0,str(Path(__file__).resolve().parents[1]/'scripts'))
import codex_tg2tmux as tool


class BindingTests(unittest.TestCase):
    def test_explicit_names_ids_and_private_projection(self):
        with tempfile.TemporaryDirectory() as tmp:
            path=Path(tmp)/'store.sqlite'
            with sqlite3.connect(path) as db:
                db.executescript('''CREATE TABLE sessions(chat_id,thread_id,codex_thread_id,force_fresh_thread,cwd,session_title,session_prompt);
                CREATE TABLE bot_state(key,value);
                CREATE TABLE incoming_updates(update_id,payload_json);''')
                db.execute('INSERT INTO sessions VALUES(1,7,?,0,?,?,?)',('native-original',tmp,'Derived summary','PRIVATE PROMPT'))
                db.execute('INSERT INTO bot_state VALUES(?,?)',('session_title_owner:1:7','Canonical title'))
                for uid,chat,name in [(1,1,'Earlier name'),(2,2,'Foreign chat')]:
                    db.execute('INSERT INTO incoming_updates VALUES(?,?)',(uid,json.dumps({'message':{'chat':{'id':chat},'message_thread_id':7,'forum_topic_edited':{'name':name}}})))
            before=path.read_bytes()
            rows=tool.topics({'db_path':str(path)})
            self.assertEqual(rows[0]['title'],'Canonical title')
            self.assertEqual(rows[0]['codex_thread_id'],'native-original')
            self.assertNotIn('PRIVATE PROMPT',json.dumps(rows))
            self.assertEqual(before,path.read_bytes())
            with sqlite3.connect(path) as db:
                db.execute('DELETE FROM bot_state')
            self.assertEqual(tool.topics({'db_path':str(path)})[0]['title'],'Earlier name')

    def test_ambiguous_chat_rejected(self):
        with tempfile.TemporaryDirectory() as tmp:
            path=Path(tmp)/'store.sqlite'
            with sqlite3.connect(path) as db:
                db.executescript('CREATE TABLE sessions(chat_id,thread_id);INSERT INTO sessions VALUES(1,7),(2,7);')
            with self.assertRaisesRegex(ValueError,'--chat'):
                tool.topics({'db_path':str(path)})

    def test_names_collisions_and_forbidden_characters(self):
        items=[{'title':'dmn.homes','thread_id':1},{'title':'dmn:homes','thread_id':2},{'title':'A\nB','thread_id':3}]
        names=tool.session_names(items)
        self.assertEqual(names[1],'dmn_homes [1]')
        self.assertEqual(names[2],'dmn_homes [2]')
        self.assertEqual(names[3],'A_B')

    def test_native_identity_and_freshness(self):
        class Native:
            def rpc(self,*_):return {'thread':{'id':str(uuid.uuid4()),'name':'same name'}}
        item={'codex_thread_id':str(uuid.uuid4()),'force_fresh_thread':0}
        self.assertFalse(tool.native_metadata(Native(),item)['available'])
        item['force_fresh_thread']=1
        self.assertEqual(tool.native_metadata(None,item)['reason'],'no-established-binding')


@unittest.skipUnless(shutil.which('tmux'),'tmux required')
class TmuxTests(unittest.TestCase):
    def test_real_launch_quote_idempotency_rename_and_conflict(self):
        socket='tg2tmux-test-'+uuid.uuid4().hex
        tmux=tool.Tmux(socket)
        with tempfile.TemporaryDirectory() as tmp:
            root=Path(tmp)
            binary=root/'fake codex'
            binary.write_text('#!/usr/bin/python3\nimport json,sys,time\nfrom pathlib import Path\nPath("args.json").write_text(json.dumps(sys.argv[1:]))\ntime.sleep(30)\n')
            binary.chmod(0o700)
            tid=str(uuid.uuid4())
            item={'cwd':tmp,'codex_thread_id':tid,'title':'quoted title'}
            try:
                tmux.run('new-session','-d','-s','unrelated','sleep 30')
                first=tmux.open(item,'Safe name',str(binary),'binding-1')
                self.assertEqual(first['action'],'created')
                self.assertEqual(tmux.open(item,'Safe name',str(binary),'binding-1')['action'],'reused')
                moved=tmux.open(item,'Renamed',str(binary),'binding-1')
                self.assertEqual(first['tmux_id'],moved['tmux_id'])
                with self.assertRaisesRegex(ValueError,'belongs'):
                    tmux.open(item,'unrelated',str(binary),'binding-2')
                import time
                deadline=time.monotonic()+3
                while not (root/'args.json').exists() and time.monotonic()<deadline:
                    time.sleep(.02)
                self.assertEqual(json.loads((root/'args.json').read_text()),['resume',tid])
                self.assertEqual(len(tmux.sessions()),2)
            finally:
                # Dispose only this uniquely named test server, never the user's server.
                tmux.run('kill-server',check=False)


if __name__=='__main__':unittest.main()
