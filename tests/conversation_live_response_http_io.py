"""Early native text stays provisional through actual HTTP/SQLite; zero models."""
import json
import hashlib
import os
import queue
import re
import uuid
from urllib.parse import urlencode, urlparse, parse_qs
import sys
from pathlib import Path
from conversation_cold_entry_http_io import ColdEntry, Browser
import speech_render as speech_fixture
from unittest.mock import patch


def qualify(binary):
    j=ColdEntry(str(Path(binary).resolve()));b=None;voice=speech_fixture.Render();voice.setUp()
    try:
        voice.qwen_route();voice.content_type='text/event-stream';voice.data=voice.stream_data()
        j.prepare();b=Browser(j,speech={'binding':str(voice.binding),'timeout_seconds':10,'streaming_output':True,'live_response':True})
        original,_=b.text('Discuss the plan while work continues.')
        j.wait(lambda:b.status(original)['phase']=='accepted' and bool(j.packets))
        send=next(send for thread,send in j.clients.values()if thread==j.web_thread)
        def emit(method,**extra):
            send({'method':method,'params':{'threadId':j.web_thread,'turnId':j.web_turn,**extra}})
        def draft():return b.get('/api/interaction').get('response_draft')
        emit('item/started',item={'id':'blank-completion','type':'agentMessage','phase':'final_answer'})
        emit('item/agentMessage/delta',itemId='blank-completion',delta=' \n')
        emit('item/agentMessage/delta',itemId='blank-completion',delta='Useful early answer. ')
        j.wait(lambda:bool(draft()))
        observed=draft();assert observed['text'].strip()=='Useful early answer.'
        assert observed['provisional']is True and observed['can_listen']is False
        def frozen(value):
            text=value['text'];ident=f"live:{value['native_turn']}:{value['native_item']}:0:{len(text)}:{hashlib.sha256(text.encode()).hexdigest()}"
            binding=b.get('/api/interaction')['binding'];revision=hashlib.sha256((binding+':'+ident).encode()).hexdigest()
            return '/api/interaction/part?'+urlencode({'source':ident,'revision':revision,'offset':0})
        frozen_first=frozen(observed)
        assert b.get(frozen_first)['text']==observed['text']
        speech_selection={k:v[0] for k,v in parse_qs(urlparse(frozen_first).query).items()};speech_selection['offset']=int(speech_selection['offset'])
        response=b.client.post('/api/interaction/audio',json={**speech_selection,'stream':True},headers=b.headers)
        assert response.status_code==200,response.text
        assert [json.loads(line)['type'] for line in response.text.splitlines()]==['audio','end']
        assert len(voice.calls)==1,'One prefix owns one actual loopback renderer attempt'
        correction,_=b.text('Continue with this correction.',playback={
            'source':speech_selection['source'],'revision':speech_selection['revision'],'segment_offset':0,'position_millis':5,'duration_millis':None,'state':'interrupted'})
        j.wait(lambda:b.status(correction)['phase']=='accepted')
        report=j.call('route-live-playback-context','playback_context',{'input_ref':correction})
        assert report['availability']=='current_source' and report['source_part']['text']==observed['text'],report


        assert not any(v['preview']=='Useful early answer.'for v in b.get('/api/interaction')['items'])
        assert j.row("SELECT count(*) FROM interaction_native_messages WHERE item_id='blank-completion'")==[(0,)]
        assert j.row('SELECT count(*) FROM interaction_deliveries')==[(0,)]
        # An empty completion retires the exact draft while native work continues.
        emit('item/completed',item={'id':'blank-completion','type':'agentMessage','phase':'final_answer','text':''})
        j.wait(lambda:draft()is None)
        assert b.client.get(frozen_first,headers=b.headers).status_code==409
        assert j.row("SELECT count(*) FROM interaction_native_messages WHERE item_id='blank-completion'")==[(0,)]
        emit('item/started',item={'id':'early','type':'agentMessage','phase':'final_answer'})
        emit('item/agentMessage/delta',itemId='early',delta='Revisable early answer.')
        j.wait(lambda:bool(draft()))
        # A later native completion, even changed from its draft, owns the one answer.
        emit('item/completed',item={'id':'early','type':'agentMessage','phase':'final_answer','text':'Confirmed answer differs from the draft.'})
        j.wait(lambda:draft()is None and any(v['preview']=='Confirmed answer differs from the draft.'for v in b.get('/api/interaction')['items']))
        assert j.row("SELECT count(*) FROM interaction_native_messages WHERE item_id='early'")==[(1,)]
        emit('item/started',item={'id':'matching','type':'agentMessage','phase':'final_answer'})
        emit('item/agentMessage/delta',itemId='matching',delta='Exactly the spoken prefix. Still writing')
        j.wait(lambda:bool(draft()))
        frozen_match=frozen(draft());selected=b.get(frozen_match)
        emit('item/completed',item={'id':'matching','type':'agentMessage','phase':'final_answer','text':'Exactly the spoken prefix. Still writing a confirmed continuation.'})
        j.wait(lambda:draft()is None)
        assert b.get(frozen_match)==selected,'Confirmation retains the exact frozen speech source'
        # Partial worker output has no entry audience and cannot replace this draft.
        worker_original,_=b.text('#Alpha Work independently.')
        j.wait(lambda:j.instructions(7)and j.instructions(7)[0][1]=='accepted')
        worker=next(send for thread,send in j.clients.values()if thread=='native-topic-7')
        worker({'method':'item/started','params':{'threadId':'native-topic-7','turnId':j.workers['native-topic-7'],'item':{'id':'private-partial','type':'agentMessage'}}})
        worker({'method':'item/agentMessage/delta','params':{'threadId':'native-topic-7','turnId':j.workers['native-topic-7'],'itemId':'private-partial','delta':'Not a published worker result.'}})
        # A subsequent acknowledged original proves the collector was not interrupted.
        correction,_=b.text('Continue discussing the plan.')
        j.wait(lambda:b.status(correction)['phase']=='accepted')
        assert draft()is None
        emit('item/started',item={'id':'interrupted','type':'agentMessage','phase':'final_answer'})
        emit('item/agentMessage/delta',itemId='interrupted',delta='A pending draft')
        j.wait(lambda:bool(draft()))
        emit('turn/completed',turn={'id':j.web_turn,'status':'interrupted'})
        j.wait(lambda:draft()is None)
        assert not j.native_answers
        print('PASS source-bound speech before native completion, exact playback context and confirmation handover; no answer/action authority, worker partial excluded, original ingress and terminal retirement; actual HTTP/native/SQLite/loopback renderer, zero models.')
    finally:
        if b:b.client.close()
        j.close();voice.tearDown()

def continuous_overlap(binary):
    """One actual ingress/ASR/controller/store journey; synthetic speech/native peers."""
    from dashboard.live_transcription import wav_bytes
    from conversation_voice_http_io import audio

    transcript='Para Alpha elegí el alcance pequeño. Conservá su trabajo.'
    class Recognition:
        max_audio_bytes=120*32000
        opened=[]
        def __init__(self,*args,**kwargs):
            self.events=queue.Queue();self.events.put({'type':'ready'})
            self.pcm=b'';self.finished=False;self.opened.append(self)
        def append(self,pcm):
            self.pcm+=pcm
            self.events.put({'type':'partial','text':'Para Alpha, el alcance grande.' if len(self.pcm)<6400 else transcript,'revision':len(self.pcm)})
        def finish(self):
            self.finished=True;self.events.put({'type':'final','text':transcript,'revision':3})
        def receive(self):return self.events.get(timeout=15)
        def result(self):return {'text':transcript,'final':True,'model':'gemini-3.5-transcribe-live',
                                 'pcm_sha256':hashlib.sha256(self.pcm).hexdigest(),'audio_millis':len(self.pcm)*1000//32000}
        def close(self):self.events.put(None)

    previous=os.umask(0o077);j=ColdEntry(str(Path(binary).resolve()));b=None
    voice=speech_fixture.Render();voice.setUp()
    try:
        voice.qwen_route();voice.content_type='text/event-stream';voice.data=voice.stream_data()
        j.prepare();j.stop()
        prepared=j.root/'prepared';helper=j.root/'unused-file-asr.py';helper.write_text("def process(*args):\n raise AssertionError('Prepared voice must not call file ASR')\n")
        route=j.root/'file-asr-route.json';route.write_text(json.dumps({'schema':'audio-transcription.route/v1','provider':'synthetic'}))
        binding=j.root/'asr-binding.json';binding.write_text(json.dumps({'schema':'telecodex.interaction-asr/v1',
            'helper':str(helper),'route_binding':str(route),'cache_root':str(j.root/'asr-cache'),'streaming_cache_root':str(prepared)}))
        script=Path(__file__).resolve().parents[1]/'scripts/interaction_asr.py'
        command=[sys.executable,str(script),'--binding',str(binding),'--retention-receipt']
        j.config.write_text(re.sub(r'(?m)^transcription_command=.*$',lambda _: 'transcription_command='+json.dumps(command),j.config.read_text())+
                            'completed_transcription_command='+json.dumps(command+['--completed-only'])+'\n')
        j.start();j.wait_control_socket(j.socket)
        streaming={'enabled':True,'provider':'gemini','endpoint':'wss://generativelanguage.googleapis.com/ws/google.ai.generativelanguage.v1beta.GenerativeService.BidiGenerateContent',
                   'credential_file':str(j.root/'synthetic-stream-key'),'cache_root':str(prepared)}
        b=Browser(j,speech={'binding':str(voice.binding),'timeout_seconds':10,'streaming_output':True,'live_response':True},streaming=streaming)
        original,_=b.text('Keep our conversation available while Alpha and Beta work.')
        j.wait(lambda:b.status(original)['phase']=='accepted')
        j.call('route-continuous-workers','dispatch',{'input_ref':original,'destinations':[
            {'selector':name,'relation':'new','requests':[]}for name in('Alpha','Beta')]})
        j.wait(lambda:all(j.instructions(t)and j.instructions(t)[0][1]=='accepted'for t in(7,8)))
        workers={t:next(send for thread,send in j.clients.values()if thread=='native-topic-'+str(t))for t in(7,8)}
        def ask(topic):
            workers[topic]({'id':'canonical-overlap-'+str(topic),'method':'item/tool/requestUserInput','params':{
                'threadId':'native-topic-'+str(topic),'turnId':j.workers['native-topic-'+str(topic)],'itemId':'overlap-'+str(topic),'isBlocking':True,
                'questions':[{'id':'scope','header':'Scope','question':'Which scope should '+('Alpha'if topic==7 else 'Beta')+' use?','options':None}]}})
        ask(7);j.wait(lambda:bool(b.get('/api/interaction/requests')['items']))
        alpha=b.get('/api/interaction/requests')['items'][0]
        capture_questions={alpha['id']:alpha['capture_version']}
        send=next(send for thread,send in j.clients.values()if thread==j.web_thread)
        send({'method':'item/started','params':{'threadId':j.web_thread,'turnId':j.web_turn,'item':{'id':'overlap-live','type':'agentMessage','phase':'final_answer'}}})
        send({'method':'item/agentMessage/delta','params':{'threadId':j.web_thread,'turnId':j.web_turn,'itemId':'overlap-live','delta':'A useful early spoken reply. '}})
        j.wait(lambda:b.get('/api/interaction').get('response_draft'))
        draft=b.get('/api/interaction')['response_draft'];text=draft['text']
        ident=f"live:{draft['native_turn']}:{draft['native_item']}:0:{len(text)}:{hashlib.sha256(text.encode()).hexdigest()}"
        revision=hashlib.sha256((b.binding+':'+ident).encode()).hexdigest()
        rendered=b.client.post('/api/interaction/audio',json={'source':ident,'revision':revision,'offset':0,'stream':True},headers=b.headers)
        assert rendered.status_code==200,rendered.text
        assert [json.loads(line)['type']for line in rendered.text.splitlines()]==['audio','end']
        capture=str(uuid.uuid4());pcm=audio()[44:]
        before_packets=len(j.packets)
        with patch('dashboard.live_transcription.TranscriptionStream',Recognition):
            with b.client.websocket_connect('/api/interaction/voice/live',headers={'origin':'https://example.test'})as ws:
                ws.send_json({'id':capture,'binding':b.binding,'csrf':b.headers['x-dashboard-csrf'],'authorization':b.headers['authorization']})
                assert ws.receive_json()['type']=='ready'
                ws.send_bytes(pcm[:3200]);assert 'grande'in ws.receive_json()['text']
                assert not j.row('SELECT id FROM conversation_inputs WHERE id=?',(capture,))and len(j.packets)==before_packets
                # This question did not exist when recording started.
                ask(8);j.wait(lambda:len(b.get('/api/interaction/requests')['items'])==2)
                beta=next(v for v in b.get('/api/interaction/requests')['items']if v['id']!=alpha['id'])
                ws.send_bytes(pcm[3200:]);assert 'pequeño'in ws.receive_json()['text']
                assert len(j.packets)==before_packets,'Revised captions became an instruction'
                ws.send_json({'type':'finish'});assert ws.receive_json()['type']=='prepared'
        wav=wav_bytes(pcm);playback={'source':ident,'revision':revision,'segment_offset':0,'position_millis':100,'duration_millis':None,'state':'interrupted'}
        h={**b.headers,'Content-Type':'audio/wav','X-Input-Binding':b.binding,'X-Input-Id':capture,'X-Audio-Sha256':hashlib.sha256(wav).hexdigest(),
           'X-Playback-Context':json.dumps(playback),'X-Capture-Questions':json.dumps(capture_questions)}
        for invalid in ([],{'bad':'b'*64},{str(i).zfill(32):'b'*64 for i in range(33)}):
            bad=b.client.post('/api/interaction/voice',content=wav,headers={**h,'X-Capture-Questions':json.dumps(invalid)})
            assert bad.status_code==400 and not j.row('SELECT id FROM conversation_inputs WHERE id=?',(capture,))
        response=b.client.post('/api/interaction/voice',content=wav,headers=h);assert response.status_code==200,response.text
        j.wait(lambda:b.status(capture)['phase']=='accepted')
        assert b.status(capture)['text']==transcript and len(j.packets)==before_packets+1
        assert b.status(capture)['voice']['transcription']['actual_transcriber']=='gemini-3.5-transcribe-live'
        context=j.call('route-continuous-playback','playback_context',{'input_ref':capture})
        assert context['availability']=='current_source'and context['report']==playback
        beta_context=j.call('route-continuous-late-question','question_context',{'input_ref':capture,'question':{k:beta[k]for k in('id','revision')}})
        assert beta_context['available_at_capture']is False,'A question arriving during streaming capture became eligible for its earlier speech'
        j.call('route-continuous-late-answer','answer_question',{'input_ref':capture,'question':{k:beta[k]for k in('id','revision')}},ok=False)
        alpha_context=j.call('route-continuous-original-question','question_context',{'input_ref':capture,'question':{k:alpha[k]for k in('id','revision')}})
        assert alpha_context['available_at_capture']is True
        assert not j.native_answers
        j.call('route-continuous-alpha-answer','answer_question',{'input_ref':capture,'question':{k:alpha[k]for k in('id','revision')}})
        j.wait(lambda:len(j.native_answers)==1)
        spoken=j.native_answers[0]['result']['answers']['scope']['answers']
        assert len(spoken)==1 and spoken[0].endswith('\n\n'+transcript),spoken
        assert '[Committed voice input]'in spoken[0] and capture in spoken[0]
        assert b.status(capture)['phase']=='answered'
        j.answer_worker(7,'Alpha preserved its work and used the small scope.')
        j.wait(lambda:any('Alpha preserved'in item['preview']for item in b.get('/api/interaction')['items']))
        assert j.row('SELECT busy FROM sessions WHERE thread_id=8')==[(1,)]
        # Reopen the authenticated client after a lost view. Inspect, never resend.
        b.client.close();packets=len(j.packets)
        b=Browser(j,speech={'binding':str(voice.binding),'timeout_seconds':10,'streaming_output':True,'live_response':True},streaming=streaming)
        assert b.status(capture)['phase']=='answered'and b.status(capture)['text']==transcript
        assert len(j.packets)==packets and len(Recognition.opened)==1
        assert len(list(prepared.glob('asr-*.json')))==1,'Prepared text was not committed once before native admission'
        # The later question can be answered by a distinct, later human original.
        later,_=b.text('Para Beta, usá el alcance grande.')
        j.wait(lambda:b.status(later)['phase']=='accepted')
        j.call('route-continuous-beta-answer','answer_question',{'input_ref':later,'question':{k:beta[k]for k in('id','revision')}})
        j.wait(lambda:len(j.native_answers)==2)
        j.answer_worker(8,'Beta finished independently after its later answer.')
        j.wait(lambda:any('Beta finished'in item['preview']for item in b.get('/api/interaction')['items']))
        send({'method':'item/completed','params':{'threadId':j.web_thread,'turnId':j.web_turn,'item':{'id':'overlap-live','type':'agentMessage','phase':'final_answer','text':text+'Both useful results remain available.'}}})
        send({'method':'turn/completed','params':{'threadId':j.web_thread,'turn':{'id':j.web_turn,'status':'completed'}}})
        j.wait(lambda:j.row('SELECT busy FROM sessions WHERE thread_id IN(1,7,8) ORDER BY thread_id')==[(0,),(0,),(0,)])
        items=b.get('/api/interaction')['items']
        for label in ('Alpha preserved','Beta finished'):
            assert sum(label in item['preview']for item in items)==1,'Reconnect duplicated a useful worker result'
        assert len(voice.calls)==1 and len(Recognition.opened)==1
        assert not any(row.get('method')=='turn/interrupt'for row in j.rpc),'Audio interruption cancelled native work'
        print('PASS continuous overlap: revised streaming ASR reused exactly once, two active workers, early speech/cursor, late-question fence, exact original answer, reconnect without replay, independent worker returns and foreground completion; actual HTTP/WebSocket/controller/SQLite/native/loopback TTS, zero models or hardware.')
    finally:
        if b:b.client.close()
        j.close();voice.tearDown();os.umask(previous)

if __name__=='__main__':
    qualify(sys.argv[1]);continuous_overlap(sys.argv[1])
