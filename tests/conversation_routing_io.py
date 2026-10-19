#!/usr/bin/env python3
"""Canonical originals use actual controller/native/store routes; zero models."""
import json
from pathlib import Path
import sys
import threading
import time
from conversation_input_control_io import Inputs


class CanonicalRouting(Inputs):
    def __init__(self, binary):
        self.routing = False
        self.workers = {}
        self.worker_packets = {}
        self.tool_results = {}
        self.topic_calls = 0
        self.hold_new_native = False
        self.new_native_seen = threading.Event()
        self.new_native_release = threading.Event()
        super().__init__(binary)
        self.target_config = '''
[[interaction.targets]]
name="Alpha"
topic_id=7
[[interaction.targets]]
name="Beta"
topic_id=8
'''

    def created_topic(self, payload):
        self.topic_calls += 1
        assert payload == {'chat_id': 100, 'name': 'Gamma'}, payload
        return {'message_thread_id': 77, 'name': 'Gamma'}

    def handle_extra_rpc(self, message, send):
        method = message.get('method'); params = message.get('params', {})
        ident = message.get('id')
        if self.hold_new_native and method == 'thread/start':
            self.new_native_seen.set()
            assert self.new_native_release.wait(timeout=10), 'Synthetic new native binding was not released'

        if method is None and str(ident).startswith('route-'):
            self.tool_results[ident] = message['result']
            return True
        if self.routing and method in ('turn/start', 'turn/steer'):
            text = '\n'.join(part.get('text', '') for part in params['input'])
            if text.startswith('[Interaction controller]'):
                thread = params['threadId']
                self.worker_packets.setdefault(thread, []).append(params)
                provenance = json.loads(next(line.removeprefix('Transport provenance: ')
                                             for line in text.splitlines()
                                             if line.startswith('Transport provenance: ')))
                assert provenance['via'] == 'interaction-controller'
                assert provenance['original_human_author'] == 100
                assert provenance['source_channel'] == 'conversation'
                assert provenance['original_ingress_update'] is None
                assert provenance['source_message']['message_id'] is None
                if method == 'turn/start':
                    turn = 'canonical-worker-'+str(len(self.workers)+1)
                    self.workers[thread] = turn
                    send({'id': ident, 'result': {'turn': {'id': turn}}})
                    self.broadcast(thread, 'turn/started', {'turn': {'id': turn, 'status': 'inProgress'}})
                else:
                    send({'id': ident, 'result': {'turnId': self.workers[thread]}})
                return True
        return super().handle_extra_rpc(message, send)

    def call(self, ident, operation, arguments, *, ok=True):
        send = next(send for selected, send in self.clients.values() if selected == self.web_thread)
        send({'id': ident, 'method': 'item/tool/call', 'params': {
            'threadId': self.web_thread, 'turnId': self.web_turn,
            'callId': ident, 'tool': 'interaction_controller',
            'arguments': {'operation': operation, 'arguments': arguments}}})
        self.wait(lambda: ident in self.tool_results)
        result = self.tool_results[ident]
        assert result['success'] is ok, result
        return json.loads(result['contentItems'][0]['text']) if ok else result

    def instructions(self, topic):
        return self.row('SELECT i.id,i.phase,i.doc FROM interaction_instructions i JOIN sessions s ON s.id=i.session_id WHERE s.thread_id=? ORDER BY i.sequence', (topic,))

    def answer_worker(self, topic, text):
        native = self.row('SELECT codex_thread_id FROM sessions WHERE thread_id=?', (topic,))[0][0]
        rows = self.instructions(topic)
        requests = sorted({request for _, _, doc in rows for request in json.loads(doc)['requests']})
        claim = {'requests': requests, 'covered': [row[0] for row in rows], 'completed': requests}
        self.broadcast(native, 'item/completed', {'turnId': self.workers[native], 'item': {
            'id': 'canonical-answer-'+str(topic), 'type': 'agentMessage', 'phase': 'final_answer',
            'text': text+'\n<daimon-result>'+json.dumps(claim)+'</daimon-result>'}})
        self.broadcast(native, 'turn/completed', {'turn': {'id': self.workers[native], 'status': 'completed'}})


def direct_after_canonical_work(binary):
    class Switching(CanonicalRouting):
        def __init__(self, binary):
            self.direct_packets=[]
            super().__init__(binary)
        def handle_extra_rpc(self, message, send):
            method=message.get('method');params=message.get('params',{})
            if self.routing and method in ('turn/start','turn/steer') and params.get('clientUserMessageId','').startswith('telegram:'):
                self.direct_packets.append(params)
                native=params['threadId']
                if method=='turn/start':
                    self.workers[native]='direct-surface-'+str(len(self.direct_packets))
                    send({'id':message['id'],'result':{'turn':{'id':self.workers[native]}}})
                    self.broadcast(native,'turn/started',{'turn':{'id':self.workers[native],'status':'inProgress'}})
                else:
                    send({'id':message['id'],'result':{'turnId':self.workers[native]}})
                return True
            return super().handle_extra_rpc(message,send)
        def direct_answer(self, text, end=True):
            native='native-topic-7';turn=self.workers[native]
            for suffix,phase,body in [('progress','commentary',text+' progress'),('final','final_answer',text)]:
                self.broadcast(native,'item/completed',{'turnId':turn,'item':{'id':text+suffix,'type':'agentMessage','phase':phase,'text':body}})
            if end:self.broadcast(native,'turn/completed',{'turn':{'id':turn,'status':'completed'}})
    j=Switching(binary)
    try:
        j.start()
        for topic in (7,8,1):
            j.admit('Seed owned context',topic=topic);j.wait(lambda:j.has('Initial goal turn delivered.',topic));j.idle()
        j.stop();j.config.write_text(j.config.read_text()+'\nmax_active_workers=1\n'+j.target_config);j.start();j.wait_control_socket(j.socket);j.routing=True
        binding=j.read('presentation','list','0','0')['result']['binding']
        j.submit(binding,'#Alpha Start from the web')
        j.wait(lambda:j.instructions(7) and j.instructions(7)[0][1]=='accepted')
        j.broadcast('native-topic-7','item/completed',{'turnId':j.workers['native-topic-7'],'item':{'id':'web-progress-before-direct','type':'agentMessage','phase':'commentary','text':'Web progress before the direct switch.'}})
        j.wait(lambda:any(i['kind']=='worker_progress' and i['preview']=='Web progress before the direct switch.' for i in j.read('presentation','list','0','0')['result']['items']))
        j.answer_worker(7,'First web answer.');j.idle();j.messages.clear()
        j.admit('Now I am writing directly in this work topic.',topic=7)
        j.wait(lambda:len(j.direct_packets)==1)
        assert j.direct_packets[-1]['input'][0]['text']=='Now I am writing directly in this work topic.'
        assert len(j.instructions(7))==1
        j.direct_answer('Idle direct answer.')
        j.wait(lambda:j.has('Idle direct answer.',7) and j.has('Idle direct answer. progress',7));j.idle()
        assert not any('Idle direct answer.' in i['preview'] for i in j.read('presentation','list','0','0')['result']['items']),'Direct output leaked into the web conversation'
        j.submit(binding,'#Alpha Return to the web for new work')
        j.wait(lambda:len(j.instructions(7))==2 and j.instructions(7)[1][1]=='accepted')
        j.admit('A raw direct correction while the web worker is still running.',topic=7)
        j.wait(lambda:len(j.direct_packets)==2)
        assert j.direct_packets[-1]['input'][0]['text']=='A raw direct correction while the web worker is still running.'
        assert len(j.instructions(7))==2,'Direct steer entered the router journal'
        j.direct_answer('Busy direct answer.')
        j.wait(lambda:j.has('Busy direct answer.',7) and j.has('Busy direct answer. progress',7));j.idle()
        assert sum(m['text']=='Busy direct answer.' for m,_ in j.messages)==1
        # An older web input can wait behind another worker while a later raw
        # direct correction reaches this session. Dispatch order is not capture
        # order; the late relay must identify the newer native-history context.
        j.submit(binding,'#Beta Occupy the single routed worker slot.')
        j.wait(lambda:j.instructions(8) and j.instructions(8)[0][1]=='accepted')
        earlier,_=j.submit(binding,'#Alpha Earlier retained requirement13.')
        j.wait(lambda:len(j.instructions(7))==3 and j.instructions(7)[2][1]=='queued')
        direct_text='Later raw direct correction: use requirement17 instead.'
        direct=j.admit(direct_text,topic=7)
        j.wait(lambda:len(j.direct_packets)==3)
        assert j.direct_packets[-1]['input'][0]['text']==direct_text
        assert len(j.instructions(7))==3,'Direct correction entered router admission'
        j.wait(lambda:j.instructions(7)[2][1]=='accepted')
        # Native preparation may include the queued relay as another input
        # part of the direct turn/start, rather than a separate turn/steer.
        def routed_packet(instruction):
            return next(part['text'] for packet in j.worker_packets['native-topic-7']+j.direct_packets
                for part in packet['input'] if 'Current instruction: '+instruction in part.get('text',''))
        packet=routed_packet(j.instructions(7)[2][0])
        assert direct_text in packet,'Late routed input lost the newer raw direct correction chronology'
        assert 'Later direct context already in native history' in packet
        assert packet.count(direct_text)==1
        assert j.row('SELECT count(*) FROM interaction_sources WHERE update_id=?',(direct,))==[(0,)]
        assert j.row('SELECT busy FROM sessions WHERE thread_id=8')==[(1,)]
        j.answer_worker(7,'Late original preserves requirement17.')
        j.wait(lambda:j.has('Late original preserves requirement17.',7))
        j.answer_worker(8,'Other routed work complete.');j.idle()
        held,_=j.submit(binding,'#Missing Earlier original still awaiting its destination.')
        later_text='Later direct requirement23, before the older original is clarified.'
        later_direct=j.admit(later_text,topic=7)
        j.wait(lambda:len(j.direct_packets)==4)
        j.direct_answer('Direct requirement23 acknowledged.')
        j.wait(lambda:j.has('Direct requirement23 acknowledged.',7))
        j.wait(lambda:j.row('SELECT busy FROM sessions WHERE thread_id=7')==[(0,)])
        j.read('input','route',json.dumps({'binding':binding,'id':held['id'],'selector':'t7'}))
        j.wait(lambda:len(j.instructions(7))==4 and j.instructions(7)[3][1]=='accepted')
        packet=routed_packet(j.instructions(7)[3][0])
        assert later_text in packet and direct_text not in packet
        assert '"input_ref":'+str(later_direct) in packet
        assert len(j.direct_packets)==4 and j.row('SELECT count(*) FROM interaction_sources WHERE update_id=?',(later_direct,))==[(0,)]
        j.answer_worker(7,'Clarified old original preserves requirement23.');j.idle()
        print('PASS web to same-session direct idle and busy steer: raw input, own commentary/final exactly once, no new router admission; zero models')
    finally:j.close()


def canonical_routes(binary):
    journey = CanonicalRouting(binary)
    try:
        journey.start()
        for topic in (7, 8, 1):
            journey.admit('Seed this owned native context', topic=topic)
            journey.wait(lambda: journey.has('Initial goal turn delivered.', topic))
            journey.idle()
        journey.stop()
        journey.config.write_text(journey.config.read_text()+journey.target_config)
        journey.start(); journey.wait_control_socket(journey.socket)
        binding = journey.read('presentation', 'list', '0', '0')['result']['binding']
        journal = journey.row('SELECT count(*) FROM incoming_updates')
        journey.routing = True
        journey.messages.clear(); journey.telegram_methods.clear()
        text = '  Canonical original: progress Alpha and Beta independently.  '
        original, _ = journey.submit(binding, text)
        journey.wait(lambda: journey.row('SELECT phase FROM conversation_inputs WHERE id=?', (original['id'],)) == [('accepted',)])
        inputs = journey.call('route-inputs', 'inputs', {})['inputs']
        selected = next(row for row in inputs if row['input_ref'] == original['id'])
        assert selected['input_update'] is None and selected['origin']['message_id'] is None
        assert selected['text_excerpt'] == text
        selection = {'input_ref': original['id'], 'destinations': [
            {'selector': name, 'relation': 'new', 'requests': []} for name in ('Alpha', 'Beta')]}
        journey.call('route-forged', 'dispatch', {**selection, 'text': 'Forged human speech'}, ok=False)
        routed = journey.call('route-dispatch', 'dispatch', selection)
        assert len(routed['results']) == 2 and all(row['new_admission'] for row in routed['results'])
        journey.wait(lambda: all(len(journey.instructions(topic)) == 1 and journey.instructions(topic)[0][1] == 'accepted' for topic in (7, 8)))
        worker_send = next(send for selected, send in journey.clients.values() if selected == 'native-topic-7')
        worker_send({'id': 'canonical-question', 'method': 'item/tool/requestUserInput', 'params': {
            'threadId': 'native-topic-7', 'turnId': journey.workers['native-topic-7'], 'itemId': 'worker-choice',
            'isBlocking': True, 'questions': [{'id': 'scope', 'header': 'Scope', 'question': 'Which scope?',
                                             'isOther': True, 'isSecret': False, 'options': None}]}})
        worker_send({'id': 'canonical-approval', 'method': 'item/commandExecution/requestApproval', 'params': {
            'threadId': 'native-topic-7', 'turnId': journey.workers['native-topic-7'], 'itemId': 'worker-command',
            'command': 'synthetic proposal awaiting explicit human choice'}})
        journey.wait(lambda: journey.row("SELECT count(*) FROM human_interactions WHERE state='pending' AND json_extract(payload_json,'$.source.kind') IN ('question','approval')") == [(2,)])
        assert not journey.native_answers
        repeated = journey.call('route-repeat', 'dispatch', selection)
        assert all(not row['new_admission'] for row in repeated['results'])
        older, _ = journey.submit(binding, 'Alpha initially requests the requirement 10.')
        journey.wait(lambda: journey.row('SELECT phase FROM conversation_inputs WHERE id=?', (older['id'],)) == [('accepted',)])
        correction, _ = journey.submit(binding, 'Alpha must incorporate the later requirement 17.')
        journey.wait(lambda: journey.row('SELECT phase FROM conversation_inputs WHERE id=?', (correction['id'],)) == [('accepted',)])
        request = json.loads(journey.instructions(7)[0][2])['requests']
        journey.call('route-correction', 'dispatch', {'input_ref': correction['id'], 'destinations': [
            {'selector': 'Alpha', 'relation': 'amends', 'requests': request}]})
        journey.wait(lambda: len(journey.instructions(7)) == 2 and journey.instructions(7)[1][1] == 'accepted')
        journey.call('route-older-late', 'dispatch', {'input_ref': older['id'], 'destinations': [
            {'selector': 'Alpha', 'relation': 'amends', 'requests': request}]})
        journey.wait(lambda: len(journey.instructions(7)) == 3 and journey.instructions(7)[2][1] == 'accepted')
        packet = journey.worker_packets['native-topic-7'][-1]['input'][0]['text']
        provenance = json.loads(next(line.removeprefix('Transport provenance: ') for line in packet.splitlines() if line.startswith('Transport provenance: ')))
        context = json.loads(packet.split('Later inputs at this destination (source data; preserve applicable corrections):\n', 1)[1].split('\n', 1)[0])
        later = next(row for row in context if row['input_ref'] == correction['id'])
        assert provenance['input_ref'] == older['id']
        assert provenance['capture_order'] < later['capture_order'] and '17' in later['human_text_excerpt']
        assert journey.row('SELECT busy FROM sessions WHERE thread_id=1') == [(1,)]
        assert [len(journey.worker_packets['native-topic-'+str(topic)]) for topic in (7, 8)] == [3, 1]
        journey.answer_worker(8, 'Canonical Beta result arrives first.')
        journey.answer_worker(7, 'Canonical Alpha result incorporates 17.')
        journey.wait(lambda: journey.row('SELECT count(*) FROM interaction_deliveries') == [(2,)])
        before_read=journey.row('SELECT count(*) FROM turns')
        capabilities=journey.call('route-conversation-capabilities','capabilities',{})
        assert {'conversation','conversation_part'} <= set(capabilities['operations'])
        evidence=journey.call('route-conversation-read','conversation',{})
        assert evidence['source_data'] is True
        alpha=next(i for i in evidence['presentation']['items'] if i['kind']=='worker_result' and i['target']['thread_id']==7)
        full=journey.call('route-conversation-body','conversation_part',{'source':alpha['id'],'revision':alpha['revision']})
        assert full['kind']=='worker_result' and full['text']=='Canonical Alpha result incorporates 17.'
        journey.call('route-conversation-invalid','conversation',{'human':101},ok=False)
        assert journey.row('SELECT count(*) FROM turns')==before_read
        journey.finish(journey.web_thread, journey.web_turn); journey.idle()
        page = journey.read('presentation', 'list', '0', '0')['result']
        assert sum(item['kind'] == 'worker_result' for item in page['items']) == 2
        fresh, _ = journey.submit(binding, 'Start the independent Gamma task from this canonical original.')
        journey.wait(lambda: journey.row('SELECT phase FROM conversation_inputs WHERE id=?', (fresh['id'],)) == [('accepted',)])
        journey.hold_new_native = True
        created = journey.call('route-create', 'create', {'input_ref': fresh['id'], 'name': 'Gamma',
            'reason': 'This original defines an independent new Gamma task.', 'context_selectors': []})
        assert created['selector'] == 't77'
        assert journey.new_native_seen.wait(timeout=8)
        time.sleep(.2)
        assert 'sendChatAction' not in journey.telegram_methods, journey.telegram_methods
        preparing, _ = journey.submit(binding, 'Gamma must incorporate requirement 44 before its first native start.')
        journey.wait(lambda: journey.row('SELECT phase FROM conversation_inputs WHERE id=?', (preparing['id'],)) == [('accepted',)])
        parent = json.loads(journey.instructions(77)[0][2])['requests']
        journey.call('route-gamma-preparing', 'dispatch', {'input_ref': preparing['id'], 'destinations': [
            {'selector': 'Gamma', 'relation': 'amends', 'requests': parent}]})
        journey.wait(lambda: len(journey.instructions(77)) == 2 and journey.instructions(77)[1][1] == 'steering')
        journey.new_native_release.set()
        journey.wait(lambda: len(journey.instructions(77)) == 2 and all(row[1] == 'accepted' for row in journey.instructions(77)))
        status = journey.call('route-status', 'creation_status', {'input_ref': fresh['id']})
        assert status['phase'] == 'ready' and status['intent']['input_update'] == fresh['id']
        assert journey.topic_calls == 1
        native = journey.row('SELECT codex_thread_id FROM sessions WHERE thread_id=77')[0][0]
        assert len(journey.worker_packets[native]) == 1
        assert any('requirement 44' in part.get('text', '') for part in journey.worker_packets[native][0]['input'])
        journey.answer_worker(77, 'Canonical Gamma result from the newly created session.')
        journey.wait(lambda: journey.row('SELECT count(*) FROM interaction_deliveries') == [(3,)])
        journey.finish(journey.web_thread, journey.web_turn); journey.idle()
        assert journey.row('SELECT count(*) FROM incoming_updates') == journal
        assert journey.row("SELECT count(*) FROM interaction_sources WHERE channel='conversation' AND update_id IS NULL AND origin_message IS NULL") == [(5,)]
        forbidden = {'sendMessage', 'editMessageText', 'sendMessageDraft', 'sendPhoto', 'sendDocument', 'sendAudio', 'sendVoice', 'sendVideo', 'sendChatAction'}
        assert not forbidden.intersection(journey.telegram_methods), journey.telegram_methods
        journey.stop(); journey.start(); journey.wait_control_socket(journey.socket)
        page = journey.read('presentation', 'list', '0', '0')['result']
        assert sum(item['kind'] == 'worker_result' for item in page['items']) == 3
        assert journey.topic_calls == 1 and journey.row('SELECT count(*) FROM interaction_instructions') == [(6,)]
        print('PASS canonical UUID originals → actual controller → parallel workers, immediate amendments with reversed dispatch retaining receiver capture precedence, reverse-order canonical results, new-session creation/status, restart without replay; no fabricated Telegram identity or publication, zero models')
    finally:
        journey.close()


def original_references_beyond_recent_window(binary):
    """Each native input must keep its identity when the recent list rolls over."""
    journey = CanonicalRouting(binary)
    try:
        journey.start()
        for topic in (7, 8, 1):
            journey.admit('Seed synthetic owned native context', topic=topic)
            journey.wait(lambda: journey.has('Initial goal turn delivered.', topic)); journey.idle()
        journey.stop(); journey.config.write_text(journey.config.read_text() + journey.target_config)
        journey.start(); journey.wait_control_socket(journey.socket); journey.routing = True
        binding = journey.read('presentation', 'list', '0', '0')['result']['binding']
        originals = [journey.submit(binding, f'Independent retained requirement {number}.')[0] for number in range(20)]
        journey.wait(lambda: journey.row("SELECT count(*) FROM conversation_inputs WHERE phase='accepted'") == [(20,)])
        assert journey.packets and all(method in ('turn/start', 'turn/steer') for method, _ in journey.packets)
        rendered = '\n'.join(part.get('text', '') for _, packet in journey.packets for part in packet['input'])
        for original in originals:
            assert original['id'] in rendered, 'Native input omitted its retained original UUID'
            assert original['text'] in rendered
        recent = [item for item in journey.call('route-window', 'inputs', {})['inputs'] if item['channel'] == 'conversation']
        assert len(recent) == 12 and originals[0]['id'] not in [item['input_ref'] for item in recent]
        journey.call('route-oldest', 'dispatch', {'input_ref': originals[0]['id'], 'destinations': [
            {'selector': 'Alpha', 'relation': 'new', 'requests': []}]})
        journey.wait(lambda: len(journey.instructions(7)) == 1 and journey.instructions(7)[0][1] == 'accepted')
        instruction = json.loads(journey.instructions(7)[0][2])
        assert instruction['text'] == originals[0]['text'], 'Controller metadata became human task text'
        assert journey.read('input', 'status', originals[0]['id'])['result']['text'] == originals[0]['text']
        journey.answer_worker(7, 'Synthetic result for the oldest original outside the recent window.')
        journey.wait(lambda: journey.row('SELECT count(*) FROM interaction_deliveries') == [(1,)])
        journey.finish(journey.web_thread, journey.web_turn); journey.idle()
        print('PASS twenty original references survive native start/steer and recent-twelve rollover; oldest exact original still routes without fabricated task text; zero models.')
    finally:
        journey.close()


def cumulative_results_are_not_old_request_fanout(binary):
    j=CanonicalRouting(binary)
    try:
        j.start()
        for topic in (7,8,1):
            j.admit('Seed the owned source context',topic=topic)
            j.wait(lambda:j.has('Initial goal turn delivered.',topic));j.idle()
        j.stop();j.config.write_text(j.config.read_text()+j.target_config)
        j.start();j.wait_control_socket(j.socket);j.routing=True
        binding=j.read('presentation','list','0','0')['result']['binding']
        j.messages.clear();j.telegram_methods.clear()
        counts=[]
        for number in range(2):
            original,_=j.submit(binding,'Continue independent work iteration '+str(number))
            j.wait(lambda:j.row('SELECT phase FROM conversation_inputs WHERE id=?',(original['id'],))==[('accepted',)])
            j.call('route-cumulative-'+str(number),'dispatch',{'input_ref':original['id'],'destinations':[{'selector':'Alpha','relation':'new','requests':[]}]})
            j.wait(lambda:len(j.instructions(7))==number+1 and j.instructions(7)[-1][1]=='accepted')
            rows=j.instructions(7)
            native=j.row('SELECT codex_thread_id FROM sessions WHERE thread_id=7')[0][0]
            claim={'requests':sorted({r for _,_,doc in rows for r in json.loads(doc)['requests']}),'covered':[row[0]for row in rows],'completed':[],'deferred':[]}
            j.broadcast(native,'item/completed',{'turnId':j.workers[native],'item':{'id':'progress-'+str(number),'type':'agentMessage','phase':'final_answer','text':'Progress '+str(number)+'\n<daimon-result>'+json.dumps(claim)+'</daimon-result>'}})
            j.broadcast(native,'turn/completed',{'turn':{'id':j.workers[native],'status':'completed'}})
            j.wait(lambda:j.row('SELECT busy FROM sessions WHERE thread_id=7')==[(0,)])
            page=j.read('presentation','list','0','0')['result']
            results=[item for item in page['items']if item['kind']=='worker_result']
            counts.append(len(results))
            assert len(results)==number+1, 'Historical unfinished requests duplicated the current native item'
            assert all(not item['completed'] for item in results)
            current=max(results,key=lambda item:item['sequence'])
            assert current['request_ids']==json.loads(rows[-1][2])['requests']
        assert counts==[1,2]
        assert j.row('SELECT count(*) FROM interaction_results')==[(2,)]
        assert len(json.loads(j.row('SELECT claim FROM interaction_results ORDER BY rowid DESC LIMIT 1')[0][0])['requests'])==2
        forbidden={'sendMessage','editMessageText','sendMessageDraft','sendPhoto','sendDocument','sendAudio','sendVoice','sendVideo','sendChatAction'}
        assert not forbidden.intersection(j.telegram_methods) and not j.messages,(j.telegram_methods,j.messages)
        j.finish(j.web_thread,j.web_turn);j.idle()
        print('PASS canonical cumulative coverage preserves historical unfinished work without repeating its old returns; two native items yield two visible replies, zero models.')
    finally:j.close()


if __name__ == '__main__':
    cumulative_results_are_not_old_request_fanout(str(Path(sys.argv[1]).resolve()))
    direct_after_canonical_work(str(Path(sys.argv[1]).resolve()))
    canonical_routes(str(Path(sys.argv[1]).resolve()))
    original_references_beyond_recent_window(str(Path(sys.argv[1]).resolve()))
