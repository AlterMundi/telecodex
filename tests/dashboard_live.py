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

if __name__=='__main__':unittest.main()
