#!/usr/bin/env python3
"""Real HTTP streams and private SQLite, synthetic Bot API only."""
from contextlib import closing
import gzip
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import importlib.util
import json
import os
from pathlib import Path
import socket
import sqlite3
import subprocess
import sys
import tempfile
import threading
import time
import unittest
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location('bot_traffic', ROOT/'scripts/bot_traffic.py')
meter = importlib.util.module_from_spec(spec);spec.loader.exec_module(meter)
TOKEN = '123456:' + 'SYNTHETIC_ONLY_' * 3
OTHER = '999999:' + 'OTHER_SYNTHETIC_' * 3
SECRET = 'MESSAGE_CONTENT_MUST_NEVER_BE_RETAINED'


def free_port():
    with socket.socket() as sock:
        sock.bind(('127.0.0.1', 0));return sock.getsockname()[1]


class Upstream(BaseHTTPRequestHandler):
    protocol_version = 'HTTP/1.1'
    calls = []
    def log_message(self, *args):
        pass
    def do_POST(self):
        self.do_GET()
    def do_GET(self):
        data = bytearray()
        if self.headers.get('Transfer-Encoding', '').lower() == 'chunked':
            while True:
                count = int(self.rfile.readline().strip(), 16)
                if not count:
                    self.rfile.readline();break
                data.extend(self.rfile.read(count));assert self.rfile.read(2)==b'\r\n'
        else:
            data.extend(self.rfile.read(int(self.headers.get('Content-Length', 0))))
        self.calls.append((self.path, bytes(data)))
        method = self.path.split('/')[-1]
        code = 200;headers = {}
        if method == 'editMessageText':
            code = 429;body = json.dumps({'ok':False,'error_code':429,'description':SECRET,'parameters':{'retry_after':90}}).encode()
        elif method == 'deleteMessage':
            body = b'{"ok":true,"result":true}'
        elif method == 'getUpdates':
            body = json.dumps({'ok':True,'result':[{'update_id':1,'message':{'message_thread_id':7,'text':SECRET}}]}).encode()
        elif self.path.startswith('/file/'):
            body = b'\x00' + SECRET.encode()*10000
        elif method == 'gzip':
            body = gzip.compress(json.dumps({'ok':True,'result':SECRET}).encode());headers['Content-Encoding']='gzip'
        elif method == 'unknownFailure':
            self.close_connection = True;return
        else:
            body = json.dumps({'ok':True,'result':{'message_id':42,'message_thread_id':7,'chat':{'id':100},'text':SECRET}}).encode()
        self.send_response(code)
        self.send_header('Content-Length', str(len(body)))
        for k,v in headers.items():self.send_header(k,v)
        self.end_headers();self.wfile.write(body)


class Traffic(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix='telecodex-traffic-')
        self.root = Path(self.tmp.name);self.root.chmod(0o700)
        self.token = self.root/'token';self.token.write_text(TOKEN);self.token.chmod(0o600)
        self.upstream = ThreadingHTTPServer(('127.0.0.1',0),Upstream);Upstream.calls=[]
        threading.Thread(target=self.upstream.serve_forever,daemon=True).start()
        self.port = free_port();self.config = self.root/'meter.toml'
        self.config.write_text(f'host="127.0.0.1"\nport={self.port}\nupstream="http://127.0.0.1:{self.upstream.server_port}"\ntoken_file="{self.token}"\nstate_dir="{self.root}/state"\n')
        self.config.chmod(0o600)
        self.log = (self.root/'process.log').open('wb')
        self.start()
    def start(self):
        self.process = subprocess.Popen([sys.executable,'-B',str(ROOT/'scripts/bot_traffic.py'),'serve','--config',str(self.config)],stdout=self.log,stderr=self.log)
        for _ in range(150):
            if self.process.poll() is not None:raise AssertionError((self.root/'process.log').read_text())
            try:
                with urllib.request.urlopen(f'http://127.0.0.1:{self.port}/healthz',timeout=.2) as response:
                    assert json.load(response)['ok'];return
            except OSError:time.sleep(.02)
        raise AssertionError('meter did not become ready')
    def tearDown(self):
        self.process.terminate();self.process.wait(timeout=10);self.log.close();self.upstream.shutdown();self.upstream.server_close();self.tmp.cleanup()
    def request(self, method, body=None, token=TOKEN, path=None):
        import httpx
        with httpx.Client(trust_env=False) as client:
            return client.post(f'http://127.0.0.1:{self.port}'+(path or f'/bot{token}/{method}'),json=body or {})
    def rows(self):
        with closing(sqlite3.connect(self.root/'state/traffic.sqlite3')) as db:
            db.row_factory=sqlite3.Row
            return [dict(r) for r in db.execute('SELECT * FROM requests ORDER BY started')]
    def wait_rows(self,n):
        for _ in range(100):
            rows=self.rows()
            if len(rows)>=n and all(r['finished'] is not None for r in rows):return rows
            time.sleep(.02)
        raise AssertionError(self.rows())
    def test_routing_errors_ingress_privacy_and_other_bot(self):
        self.assertEqual(self.request('sendMessage',{'chat_id':100,'message_thread_id':7,'text':SECRET}).status_code,200)
        failed=self.request('editMessageText',{'chat_id':100,'message_id':42,'text':SECRET})
        self.assertEqual(failed.status_code,429);self.assertEqual(failed.json()['parameters']['retry_after'],90)
        self.request('getUpdates',{'timeout':0});self.request('getMe',token=OTHER)
        rows=self.wait_rows(3);self.assertEqual(len(rows),3);self.assertEqual([r['topic_id'] for r in rows[:2]],[7,7]);self.assertEqual(rows[1]['api_error'],429)
        self.assertEqual(rows[1]['retry_after'],90);self.assertEqual(len(Upstream.calls),4)
        with closing(sqlite3.connect(self.root/'state/traffic.sqlite3')) as db:self.assertEqual(db.execute('SELECT topic_id,kind,count FROM ingress').fetchall(),[(7,'message',1)])
        report=meter.report(self.root/'state');self.assertEqual(report['totals'][0],3)
        self.process.terminate();self.process.wait(timeout=10);self.log.flush()
        for path in (self.root/'state').iterdir():
            raw=path.read_bytes();self.assertNotIn(TOKEN.encode(),raw);self.assertNotIn(SECRET.encode(),raw);self.assertNotIn(OTHER.encode(),raw)
        self.assertNotIn(TOKEN,(self.root/'process.log').read_text());self.assertNotIn(SECRET,(self.root/'process.log').read_text())
        self.start();self.assertEqual(len(self.rows()),3)
    def test_stream_upload_download_encoded_path_and_compression(self):
        import httpx
        data=(SECRET*10000).encode()
        with httpx.Client(trust_env=False) as client:
            response=client.post(f'http://127.0.0.1:{self.port}/bot{TOKEN}/sendDocument',data={'chat_id':'100','message_thread_id':'7'},files={'document':('fixture.bin',data)})
            self.assertEqual(response.status_code,200);self.assertIn(data,Upstream.calls[-1][1])
            raw=[b'prefix-',data,b'-suffix']
            client.post(f'http://127.0.0.1:{self.port}/bot{TOKEN}/sendDocument',content=iter(raw))
            self.assertEqual(Upstream.calls[-1][1],b''.join(raw))
            path=f'/file/bot{TOKEN}/dir/file%20name%2B.bin'
            response=client.get(f'http://127.0.0.1:{self.port}'+path)
            self.assertEqual(response.content,b'\x00'+data);self.assertEqual(Upstream.calls[-1][0],path)
            response=client.get(f'http://127.0.0.1:{self.port}/bot{TOKEN}/gzip');self.assertEqual(response.json()['result'],SECRET)
        rows=self.wait_rows(4);self.assertEqual(rows[0]['request_bytes'],len(Upstream.calls[0][1]));self.assertEqual(rows[2]['method'],'fileDownload');self.assertEqual(rows[2]['response_bytes'],len(data)+1)
        self.assertEqual(rows[3]['metadata_complete'],0)
    def test_request_truncation_is_explicit(self):
        self.request('deleteMessage', {'text': SECRET * 10000, 'chat_id': 101, 'message_id': 99})
        row = self.wait_rows(1)[0]
        self.assertEqual(row['request_metadata_complete'], 0)
        self.assertEqual(row['response_metadata_complete'], 1)
        self.assertEqual(row['metadata_complete'], 0)
        self.assertIsNone(row['topic_id'])

    def test_unknown_delivery_is_not_retried(self):
        self.assertEqual(self.request('unknownFailure').status_code,502)
        rows=self.wait_rows(1);self.assertEqual(rows[0]['outcome'],'upstream_unknown');self.assertEqual(len(Upstream.calls),1)
        self.assertNotIn(TOKEN,(self.root/'process.log').read_text())


class Storage(unittest.TestCase):
    def test_retention_restart_and_strict_private_fields(self):
        with tempfile.TemporaryDirectory() as directory:
            j=meter.Journal(directory,max_events=2)
            unfinished=j.start('activity',123,'editMessageText')
            j.close();j=meter.Journal(directory,max_events=2)
            self.assertEqual(j.db.execute('SELECT outcome FROM requests WHERE id=?',(unfinished,)).fetchone()[0],'collector_restarted_unknown')
            for _ in range(5):
                ident=j.start('bridge',123,'sendMessage');j.finish(ident,{'chat_id':100,'message_thread_id':7},b'{}',(5,2),200,1,'response',True)
            self.assertEqual(j.db.execute('SELECT count(*) FROM requests').fetchone()[0],2)
            for n in range(5):
                j.start('bridge', 123, 'getUpdates')
                self.assertLessEqual(j.db.execute('SELECT count(*) FROM requests').fetchone()[0], 2)
                j.db.execute('INSERT OR REPLACE INTO messages VALUES(?,?,?,?,?)', (100, n, 7, 'observed', time.time()))
            j.prune();j.db.commit()
            self.assertLessEqual(j.db.execute('SELECT count(*) FROM messages').fetchone()[0], 2)
            j.loss();j.start('bridge', 123, 'getMe');j.close()
            j=meter.Journal(directory,max_events=2)
            self.assertEqual(j.dropped, 1)
            j.db.execute('UPDATE requests SET started=?', (time.time()-8*86400,));j.db.commit();j.close()
            j=meter.Journal(directory,max_events=2)
            self.assertEqual(j.db.execute('SELECT count(*) FROM requests').fetchone()[0], 0)
            self.assertIsNone(meter.number(True));self.assertIsNone(meter.number(SECRET));self.assertEqual(meter.metadata(json.dumps({'chat_id':100,'text':SECRET}).encode(),'application/json')['chat_id'],100);j.close()


if __name__=='__main__':unittest.main()
