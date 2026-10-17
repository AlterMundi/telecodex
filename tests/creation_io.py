#!/usr/bin/env python3
"""Creation uses original human input, durable intent and existing worker paths.
All native, HTTP, user and model boundaries are synthetic.
"""
import json
import pathlib
import sys
from semantic_routing_io import Semantic
from routing_io import Routing


class Creation(Semantic):
    def __init__(self,binary,unknown=False,retired=False):
        self.retired=retired
        self.unknown=unknown
        self.topic_calls=0
        self.creation_outputs=[]
        super().__init__(binary)

    def created_topic(self,payload):
        self.topic_calls+=1
        assert payload=={'chat_id':100,'name':'Gamma'}
        return {} if self.unknown else {'message_thread_id':77,'name':'Gamma'}

    def create_args(self):
        return {'input_update':self.input_ref,'name':'Gamma','reason':'The original input defines an independent new Gamma task.','context_selectors':[]}

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
                assert value['success'] is self.unknown
                if self.unknown:assert json.loads(value['contentItems'][0]['text'])['phase']=='uncertain'
                self.call('creation-status','creation_status',{'input_update':self.input_ref})
            elif ident=='creation-status':
                assert value['success']
                assert json.loads(value['contentItems'][0]['text'])['phase']==('retained_unsent' if self.retired else 'uncertain' if self.unknown else 'ready')
                self.call('inventory-created','candidates',{'mode':'inventory','selectors':[]})
            else:
                assert value['success']
                assert {c['label'] for c in json.loads(value['contentItems'][0]['text'])['candidates']}==({'Alpha','Beta'} if self.unknown or self.retired else {'Alpha','Beta','Gamma'})
                send({'method':'turn/completed','params':{'threadId':self.entry,'turn':{'id':self.entry_turn,'status':'completed'}}})
            return True
        return super().handle_extra_rpc(m,send)


def created_destination(binary):
    j=Creation(binary)
    try:
        j.prepare()
        original=j.admit('NATURAL-ROUTE Start the independent Gamma task from this exact original text.',topic=1)
        j.wait(lambda:len(j.creation_outputs)==4)
        j.wait(lambda:len(j.instructions(77))==1 and j.instructions(77)[0][1]=='accepted')
        assert j.topic_calls==1
        doc=json.loads(j.instructions(77)[0][2])
        assert doc['text']==j.source_text and doc['origin']['message_id']==original
        j.admit('#Gamma Change the new task requirement to 17.',topic=1)
        j.wait(lambda:len(j.instructions(77))==2 and j.instructions(77)[1][1]=='accepted')
        native=j.row('SELECT codex_thread_id FROM sessions WHERE thread_id=77')[0][0]
        rows=j.instructions(77)
        requests=sorted({r for _,_,doc in rows for r in json.loads(doc)['requests']})
        claim={'requests':requests,'covered':[r[0] for r in rows],'completed':requests}
        j.broadcast(native,'item/completed',{'turnId':j.runs[native],'item':{'id':'created-answer','type':'agentMessage','phase':'final_answer','text':'Gamma incorporated 17.\n<daimon-result>'+json.dumps(claim)+'</daimon-result>'}})
        j.broadcast(native,'turn/completed',{'turn':{'id':j.runs[native],'status':'completed'}})
        j.wait(lambda:j.has('Gamma incorporated 17.',1))
        j.idle()
        j.stop();j.start()
        j.admit('#Gamma Continue the retained created task.',topic=1)
        j.wait(lambda:len(j.instructions(77))==3 and j.instructions(77)[2][1]=='accepted')
        assert j.topic_calls==1 and j.row('SELECT count(*) FROM interaction_creations')==[(1,)]
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
    created_destination(binary)
    uncertain_topic(binary)
    retired_before_send(binary)
