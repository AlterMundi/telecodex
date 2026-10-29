#!/usr/bin/env python3
"""Creation uses original human input, durable intent and existing worker paths.
All native, HTTP, user and model boundaries are synthetic.
"""
import json
import pathlib
import sys
import subprocess
import uuid
from semantic_routing_io import Semantic
from routing_io import Routing


class Creation(Semantic):
    def __init__(self,binary,unknown=False,retired=False,name="Gamma"):
        self.name=name
        self.retired=retired
        self.unknown=unknown
        self.topic_calls=0
        self.creation_outputs=[]
        super().__init__(binary)

    def created_topic(self,payload):
        self.topic_calls+=1
        assert payload=={'chat_id':100,'name':self.name}
        return {} if self.unknown else {'message_thread_id':77,'name':self.name}

    def create_args(self):
        return {'input_update':self.input_ref,'name':self.name,'reason':'The original input defines an independent new Gamma task.','context_selectors':[]}

    def call(self,ident,operation,arguments):
        if ident=='forged':ident,operation,arguments='create','create',self.create_args()
        super().call(ident,operation,arguments)

    def handle_extra_rpc(self,m,send):
        method=m.get('method');ident=m.get('id');p=m.get('params',{})
        if method=='turn/start' and self.entry and p['threadId']!=self.entry and p['threadId'] not in ('native-topic-7','native-topic-8'):
            return Routing.handle_extra_rpc(self,m,send)
        if method is None and ident in ('create','repeat-create','creation-status','inventory-created'):
            self.creation_outputs.append(m)
            value=m['result']
            if ident=='create':
                assert value['success'] is (not self.unknown and not self.retired)
                if not self.unknown and not self.retired:
                    outcome=json.loads(value['contentItems'][0]['text'])
                    assert outcome['kind']=='created_destination'
                    assert outcome['selector']=='t77'
                self.call('repeat-create','create',self.create_args())
            elif ident=='repeat-create':
                assert value['success'] is (self.unknown or (' ' in self.name and not self.retired))
                if self.unknown:assert json.loads(value['contentItems'][0]['text'])['phase']=='uncertain'
                elif ' ' in self.name and not self.retired:assert json.loads(value['contentItems'][0]['text'])['phase']=='ready'
                self.call('creation-status','creation_status',{'input_update':self.input_ref})
            elif ident=='creation-status':
                assert value['success']
                assert json.loads(value['contentItems'][0]['text'])['phase']==('retained_unsent' if self.retired else 'uncertain' if self.unknown else 'ready')
                self.call('inventory-created','candidates',{'mode':'inventory','selectors':[]})
            else:
                assert value['success']
                assert {c['label'] for c in json.loads(value['contentItems'][0]['text'])['candidates']}==({'Alpha','Beta'} if self.unknown or self.retired else {'Alpha','Beta',('t77' if ' ' in self.name else self.name)})
                send({'method':'turn/completed','params':{'threadId':self.entry,'turn':{'id':self.entry_turn,'status':'completed'}}})
            return True
        return super().handle_extra_rpc(m,send)


class InvalidTitle(Creation):
    def __init__(self,binary):
        self.rejections=[]
        super().__init__(binary,name=" bad title")

    def call(self,ident,operation,arguments):
        if ident=='create' and self.rejections:ident='corrected-create'
        super().call(ident,operation,arguments)

    def handle_extra_rpc(self,m,send):
        ident=m.get('id')
        if m.get('method') is None and ident=='corrected-create':
            return super().handle_extra_rpc({**m,'id':'create'},send)
        if m.get('method') is None and ident=='create' and not self.rejections:
            value=m['result'];assert value['success'] is False
            error=json.loads(value['contentItems'][0]['text'])
            assert error['kind']=='creation_rejected' and error['code']=='invalid_creation_title'
            assert error['stage']=='before_intent'
            assert self.topic_calls==0 and self.row('SELECT count(*) FROM interaction_creations')==[(0,)]
            self.rejections.append(error)
            self.call('rejected-status','creation_status',{'input_update':self.input_ref})
            return True
        if m.get('method') is None and ident=='rejected-status':
            assert m['result']['success']
            assert json.loads(m['result']['contentItems'][0]['text'])['phase']=='not_found'
            self.name='Diamond Badge'
            self.call('create','create',self.create_args())
            return True
        return super().handle_extra_rpc(m,send)


class Standalone(Creation):
    def handle_extra_rpc(self, message, send):
        if not self.routed:
            return Routing.handle_extra_rpc(self, message, send)
        if message.get('method') is None and message.get('id') == 'inputs':
            value = message['result']
            assert value['success'], value
            rows = json.loads(value['contentItems'][0]['text'])['inputs']
            current = next(row for row in rows if 'NATURAL-ROUTE' in row['text_excerpt'])
            self.input_ref = current.get('input_ref') or current['input_update']
            self.source_text = current['text_excerpt']
            self.call('create', 'create', self.create_args())
            return True
        if message.get('method') is None and message.get('id') == 'create':
            value = message['result']
            assert value['success'], {'reply':value,
                'creation':self.row('SELECT phase,topic,doc FROM interaction_creations'),
                'entry':self.row('SELECT thread_id,codex_thread_id,busy FROM sessions'),
                'input':self.row('SELECT phase,doc FROM conversation_inputs')}
            result = json.loads(value['contentItems'][0]['text'])
            assert result['selector'] == 't77' and result['dispatch_error'] is None
            self.creation_outputs.append(result)
            send({'method':'turn/completed','params':{'threadId':self.entry,'turn':{'id':self.entry_turn,'status':'completed'}}})
            return True
        return super().handle_extra_rpc(message, send)


def standalone_destination(binary):
    j = Standalone(binary)
    try:
        j.prepare(seed_entry=True)
        j.stop()
        j.socket = j.root / 'owner.sock'
        text = j.config.read_text().replace('[telegram]\n', '[telegram]\nenabled=false\n')
        text = '\n'.join(line for line in text.splitlines() if not line.startswith('bot_token_file='))
        j.config.write_text(text + '\n[[interaction.targets]]\nname="Existing"\ntopic_id=76\n'
                            + '\n[session_control]\nhuman_id=100\nsocket_path=' + json.dumps(str(j.socket)) + '\n')
        before = j.row('SELECT count(*) FROM incoming_updates')
        j.start()
        j.wait_control_socket(j.socket)
        def owner(*args):
            result = subprocess.run([j.binary, '--control', str(j.config), *args], capture_output=True, text=True, timeout=15)
            assert result.returncode == 0, result.stderr
            return json.loads(result.stdout)['result']
        binding = owner('presentation', 'list', '0', '0')['binding']
        original = str(uuid.uuid4())
        owner('input', 'submit', json.dumps({'id':original,'binding':binding,'text':'NATURAL-ROUTE Start one standalone task from this exact browser original.'}))
        j.wait(lambda: len(j.creation_outputs) == 1)
        j.wait(lambda: len(j.instructions(77)) == 1 and j.instructions(77)[0][1] == 'accepted')
        assert j.topic_calls == 0
        assert j.row('SELECT count(*) FROM incoming_updates') == before
        assert j.row('SELECT count(*) FROM sessions WHERE thread_id=76') == [(0,)]
        assert j.row('SELECT count(*) FROM interaction_enrollments') == [(1,)]
        assert json.loads(j.instructions(77)[0][2])['text'] == j.source_text
        j.stop(); j.start()
        assert j.topic_calls == 0 and j.row('SELECT count(*) FROM interaction_creations') == [(1,)]
        print('PASS standalone original creates one local worker without Telegram API/token, reserves uninitialized targets and retains enrollment across restart')
    finally:
        j.close()


def invalid_title_then_corrected(binary):
    j=InvalidTitle(binary)
    try:
        j.prepare()
        original=j.admit('NATURAL-ROUTE Start an independent named task.',topic=1)
        j.wait(lambda:len(j.creation_outputs)==4)
        j.wait(lambda:len(j.instructions(77))==1 and j.instructions(77)[0][1]=='accepted')
        assert len(j.rejections)==1 and j.topic_calls==1
        assert j.row('SELECT count(*) FROM interaction_creations')==[(1,)]
        assert json.loads(j.instructions(77)[0][2])['origin']['message_id']==original
        print('PASS safe preflight rejection has zero effects; inspected correction uses the same original and creates exactly one topic')
    finally:j.close()


def created_destination(binary,name="Gamma"):
    j=Creation(binary,name=name)
    try:
        j.prepare()
        original=j.admit('NATURAL-ROUTE Start the independent Gamma task from this exact original text.',topic=1)
        j.wait(lambda:len(j.creation_outputs)==4)
        j.wait(lambda:len(j.instructions(77))==1 and j.instructions(77)[0][1]=='accepted')
        assert j.topic_calls==1
        doc=json.loads(j.instructions(77)[0][2])
        assert doc['text']==j.source_text and doc['origin']['message_id']==original
        j.admit('#t77 Change the new task requirement to 17.',topic=1)
        j.wait(lambda:len(j.instructions(77))==2 and j.instructions(77)[1][1]=='accepted')
        assert j.row('SELECT session_title FROM sessions WHERE thread_id=77')==[(name,)]
        native=j.row('SELECT codex_thread_id FROM sessions WHERE thread_id=77')[0][0]
        j.wait(lambda:any(r.get('method')=='thread/name/set' and r.get('params')=={'threadId':native,'name':name} for r in j.rpc))
        rows=j.instructions(77)
        requests=sorted({r for _,_,doc in rows for r in json.loads(doc)['requests']})
        claim={'requests':requests,'covered':[r[0] for r in rows],'completed':requests}
        j.broadcast(native,'item/completed',{'turnId':j.runs[native],'item':{'id':'created-answer','type':'agentMessage','phase':'final_answer','text':'Gamma incorporated 17.\n<daimon-result>'+json.dumps(claim)+'</daimon-result>'}})
        j.broadcast(native,'turn/completed',{'turn':{'id':j.runs[native],'status':'completed'}})
        j.wait(lambda:j.has('Gamma incorporated 17.',1))
        j.idle()
        j.command('/new','Fresh Codex session armed',topic=77)
        assert j.row("SELECT value FROM bot_state WHERE key='session_title_owner:100:77'")==[(name,)]
        assert j.row('SELECT codex_thread_id,force_fresh_thread FROM sessions WHERE thread_id=77')==[(None,1)]
        j.stop();j.start()
        j.admit('#t77 Continue the retained created task.',topic=1)
        j.wait(lambda:len(j.instructions(77))==3 and j.instructions(77)[2][1]=='accepted')
        assert j.topic_calls==1 and j.row('SELECT count(*) FROM interaction_creations')==[(1,)]
        fresh=j.row('SELECT codex_thread_id FROM sessions WHERE thread_id=77')[0][0]
        j.wait(lambda:any(r.get('method')=='thread/name/set' and r.get('params')=={'threadId':fresh,'name':name} for r in j.rpc))
        assert j.row('SELECT count(*) FROM interaction_enrollments')==[(1,)]
        print('PASS one durable creation, original input preservation, repeat refusal, status inspection, observer enrollment, live explicit correction, correlated result and restart routing without another topic RPC')
    finally:j.close()


def uncertain_topic(binary):
    j=Creation(binary,unknown=True)
    try:
        j.prepare()
        original=j.admit('NATURAL-ROUTE Start an independent Gamma task.',topic=1)
        j.wait(lambda:len(j.creation_outputs)==4)
        j.idle()
        assert j.topic_calls==1 and j.row('SELECT count(*) FROM interaction_enrollments')==[(0,)]
        assert j.row('SELECT count(*) FROM interaction_sources WHERE update_id=?',(original,))==[(0,)]
        assert j.row("SELECT phase FROM interaction_creations")==[('uncertain',)]
        j.stop();j.start()
        j.command('/interaction status','Interaction status',topic=1)
        j.idle()
        assert j.topic_calls==1 and j.row("SELECT phase FROM interaction_creations")==[('uncertain',)]
        print('PASS unconfirmed topic receipt remains inspectable uncertainty; repeated selection and restart do not create another topic or run a worker')
    finally:j.close()


def retired_before_send(binary):
    import sqlite3
    j=Creation(binary,retired=True)
    try:
        j.prepare()
        j.cooldown_once=True
        j.admit('/interaction status',topic=1)
        j.wait(j.cooldown_seen.is_set)
        j.admit('NATURAL-ROUTE Create Gamma after the held outbound slot.',topic=1)
        j.wait(lambda:j.row("SELECT phase FROM interaction_creations")==[('prepared',)])
        with sqlite3.connect(j.root/'state.sqlite') as db:
            db.execute("INSERT OR REPLACE INTO bot_state(key,value) VALUES('stop_admission_epoch:100','synthetic-retired-input')")
        j.wait(lambda:len(j.creation_outputs)==4)
        j.idle()
        assert j.topic_calls==0 and j.row("SELECT phase FROM interaction_creations")==[('retained_unsent',)]
        assert j.row('SELECT count(*) FROM interaction_enrollments')==[(0,)]
        print('PASS original input retired during outbound cooldown causes zero topic mutations and no worker admission; no Stop invoked')
    finally:j.close()


if __name__=='__main__':
    binary=str(pathlib.Path(sys.argv[1]).resolve())
    standalone_destination(binary)
    created_destination(binary)
    created_destination(binary,"Diamond Badge")
    invalid_title_then_corrected(binary)
    uncertain_topic(binary)
    retired_before_send(binary)
