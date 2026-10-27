"""Real filesystem notifications: commits, WAL, replacement, heartbeat and cleanup."""
import asyncio
import os
from pathlib import Path
import sqlite3
import sys
import tempfile
import unittest
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from dashboard.live import changes
from dashboard.alerts import Receiver

class Live(unittest.IsolatedAsyncioTestCase):
    async def test_real_changes_and_unrelated_files(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);db=root/'conversation.db';observer=root/'topics.json'
            conn=sqlite3.connect(db);conn.execute('pragma journal_mode=wal');conn.execute('create table events(n)');conn.commit()
            before=len(os.listdir('/proc/self/fd'))
            source=changes([db,observer],heartbeat=.3)
            self.assertTrue(await anext(source))
            conn.execute('insert into events values(1)');conn.commit()
            self.assertTrue(await asyncio.wait_for(anext(source),1))
            temporary=root/'topics.new';temporary.write_text('{}');temporary.replace(observer)
            self.assertTrue(await asyncio.wait_for(anext(source),1))
            (root/'unrelated').write_text('ignore')
            self.assertFalse(await asyncio.wait_for(anext(source),1))
            await source.aclose();conn.close()
            self.assertLessEqual(len(os.listdir('/proc/self/fd')),before)

    async def test_existing_alert_tick_invalidates_only_subscribed_views(self):
        class Provider:
            human=100
            binding_bytes=b'fixture'
            def assert_binding(self): pass
            def authorized(self,human): return human==100
        class Client:
            base='https://fixture.invalid'
            fingerprint='fixture'
            calls=0
            def assert_binding(self): pass
            def call(self,*args): self.calls+=1;return {'observers':[]}
            def collection(self,*args,**kwargs): self.calls+=1;return [],False
        class Sender:
            token='fixture'
            def assert_binding(self): pass
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);root.chmod(0o700)
            client=Client()
            receiver=Receiver({'state_dir':str(root),'bot_id':123,'alerts':{
                'receiving_scope':'human','adapter_id':'telegram','deliver':False}},Provider(),client,Sender())
            source=changes([root/'conversation.db'],heartbeat=.3,subscribe=receiver.subscribe)
            self.assertTrue(await anext(source))
            self.assertEqual(len(receiver.listeners),1)
            self.assertEqual(client.calls,0,'Subscribing cannot start source observation or model work')
            await asyncio.to_thread(receiver.tick)
            self.assertTrue(await asyncio.wait_for(anext(source),1),'In-memory alert observations must wake the existing SSE')
            self.assertEqual(receiver.snapshot()['state'],'available')
            await source.aclose()
            self.assertEqual(len(receiver.listeners),0,'A disconnected browser must release its receiving subscription')
            await asyncio.to_thread(receiver.tick)
            receiver.close()

if __name__=='__main__':unittest.main()
