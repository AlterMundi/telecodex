"""Streaming provider contract: revisable captions without dialogue/admission."""
import json
from pathlib import Path
import sys
import tempfile
import threading
import unittest
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from dashboard.transcription import TranscriptionStream, TranscriptionUnavailable


class Socket:
    def __init__(self):
        self.sent = []
        self.events = []
        self.closed = False
    def send(self, message): self.sent.append(json.loads(message))
    def recv(self): return json.dumps(self.events.pop(0))
    def close(self): self.closed = True


class StreamingTranscription(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.key = Path(self.temp.name) / 'key'
        self.key.write_text('synthetic-test-token')
        self.key.chmod(0o600)
        self.socket = Socket()
        self.connections = []
        def connect(*args, **kwargs):
            self.connections.append((args, kwargs))
            return self.socket
        self.stream = TranscriptionStream('wss://provider.test/api-ws/v1/realtime?model=qwen-audio-3.0-realtime-plus', self.key, connect)
    def tearDown(self):
        self.stream.close()
        self.temp.cleanup()
    def ready(self):
        self.socket.events.append({'type':'session.updated','session':{'turn_detection':None}})
        self.assertEqual(self.stream.receive(), {'type':'ready'})
    def transcript(self, text, stash='', final=False, item='utterance'):
        self.socket.events.append({'type':'conversation.item.input_audio_transcription.'+('completed' if final else 'delta'), 'item_id':item, **({'transcript':text} if final else {'text':text,'stash':stash})})
        return self.stream.receive()
    def test_partials_replace_and_finish_only_transcribes(self):
        with self.assertRaises(ValueError): self.stream.append(b'\0\0')
        self.ready();self.stream.append(b'\0\0'*1600)
        a=self.transcript('','diecisiete');b=self.transcript('','diecinueve')
        self.assertEqual(a['text'],'diecisiete');self.assertEqual(b['text'],'diecinueve')
        self.assertGreater(b['revision'],a['revision'])
        with self.assertRaises(ValueError): self.stream.result()
        self.stream.finish()
        self.assertEqual(self.transcript('Diecinueve.',final=True)['type'],'final')
        self.assertEqual(self.stream.result()['audio_millis'],100)
        self.assertEqual(self.stream.result()['text'],'Diecinueve.')
        self.assertEqual([x['type'] for x in self.socket.sent],['session.update','input_audio_buffer.append','input_audio_buffer.commit'])
        self.assertIsNone(self.socket.sent[0]['session']['turn_detection'])
        self.assertFalse(any('response' in x['type'] for x in self.socket.sent))
        with self.assertRaises(ValueError): self.stream.append(b'\0\0')
        with self.assertRaises(ValueError): self.stream.finish()
    def test_no_auto_response_or_nonmanual_readiness(self):
        for event in ({'type':'session.updated','session':{'turn_detection':{'type':'server_vad'}}},{'type':'response.created'},{'type':'error'}):
            self.stream.phase='opening';self.socket.events.append(event)
            with self.assertRaises(TranscriptionUnavailable):self.stream.receive()
            self.assertTrue(self.socket.closed)
        self.assertEqual(len(self.connections),1)
    def test_acknowledgment_omits_manual_configuration(self):
        self.socket.events.append({'type':'session.updated','session':{'model':self.stream.model,'modalities':['text']}})
        self.assertEqual(self.stream.receive(),{'type':'ready'})
        self.stream.append(b'\0\0')
        self.assertEqual(len(self.connections),1)
    def test_changed_item_premature_final_and_bounds(self):
        self.ready();self.stream.append(b'\0\0')
        self.transcript('draft')
        with self.assertRaises(TranscriptionUnavailable):self.transcript('other',item='other')
        self.stream.phase='ready'
        with self.assertRaises(TranscriptionUnavailable):self.transcript('early',final=True)
        self.stream.phase='ready'
        for pcm in (b'',b'x',b'x'*32002):
            with self.assertRaises(ValueError):self.stream.append(pcm)
        self.stream.audio_bytes=self.stream.max_audio_bytes
        with self.assertRaises(ValueError):self.stream.append(b'\0\0')
    def test_disconnect_is_terminal_without_reconnect(self):
        self.ready();self.stream.append(b'\0\0');self.stream.finish()
        with self.assertRaises(TranscriptionUnavailable):self.stream.receive()
        self.assertEqual(self.stream.phase,'failed');self.assertEqual(len(self.connections),1)
        with self.assertRaises(ValueError):self.stream.result()
    def test_concurrent_receivers_keep_wire_order_and_do_not_block_audio(self):
        self.ready();self.stream.append(b'\0\0')
        entered=threading.Event();release=threading.Event();second_read=threading.Event()
        events=[{'type':'conversation.item.input_audio_transcription.delta','item_id':'one','text':'old'},
                {'type':'conversation.item.input_audio_transcription.delta','item_id':'one','text':'new'}]
        def recv():
            event=events.pop(0)
            if event['text']=='old':entered.set();release.wait(2)
            else:second_read.set()
            return json.dumps(event)
        self.socket.recv=recv
        results=[]
        a=threading.Thread(target=lambda:results.append(self.stream.receive()))
        b=threading.Thread(target=lambda:results.append(self.stream.receive()))
        a.start();self.assertTrue(entered.wait(1));b.start()
        try:
            self.assertFalse(second_read.wait(.05))
            self.stream.append(b'\0\0')  # A blocked read must not stop capture.
        finally:release.set();a.join(2);b.join(2)
        self.assertFalse(a.is_alive());self.assertFalse(b.is_alive())
        self.assertEqual([x['text'] for x in results],['old','new'])
        self.assertEqual(self.stream.text,'new')

    def test_silent_receive_interval_preserves_the_same_stream(self):
        self.ready();self.stream.append(b'\0\0')
        original=self.socket.recv
        def timeout():raise TimeoutError('no new words yet')
        self.socket.recv=timeout
        self.assertEqual(self.stream.receive(),{'type':'status'})
        self.assertEqual(self.stream.phase,'ready');self.assertFalse(self.socket.closed)
        self.socket.recv=original
        self.assertEqual(self.transcript('continued')['text'],'continued')
        self.assertEqual(len(self.connections),1)

    def test_discard_does_not_commit(self):
        self.ready();self.stream.append(b'\0\0');self.stream.close()
        self.assertNotIn('input_audio_buffer.commit',[x['type'] for x in self.socket.sent])
        with self.assertRaises(ValueError):self.stream.result()


if __name__=='__main__':unittest.main()
