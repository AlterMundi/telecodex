#!/usr/bin/env python3
"""Native integrator chooses destinations; controller relays immutable originals.
All user, Telegram, native and model boundaries are synthetic.
"""
import json
import pathlib
import sys
from routing_io import Routing


class Semantic(Routing):
    def __init__(self,binary,delay_selection=False):
        self.delay_selection=delay_selection
        self.entry=None
        self.entry_turn=None
        self.outputs=[]
        self.input_ref=None
        self.source_text=None
        self.entry_send=None
        self.inspections={}
        super().__init__(binary)

    def call(self,ident,operation,arguments):
        self.entry_send({'id':ident,'method':'item/tool/call','params':{'threadId':self.entry,'turnId':self.entry_turn,'callId':ident,'tool':'interaction_controller','arguments':{'operation':operation,'arguments':arguments}}})

    def selection(self):
        return {'input_update':self.input_ref,'destinations':[{'selector':name,'relation':'new','requests':[]} for name in ('Alpha','Beta')]}

    def handle_extra_rpc(self,m,send):
        method=m.get('method');p=m.get('params',{});ident=m.get('id')
        if method is None and isinstance(ident,str) and ident.startswith('inspect-'):
            self.inspections[ident]=m['result']
            return True
        if method=='turn/start' and p['threadId'] not in ('native-topic-7','native-topic-8'):
            self.entry=p['threadId'];self.entry_turn='semantic-entry-turn';self.entry_send=send
            send({'id':ident,'result':{'turn':{'id':self.entry_turn}}})
            send({'method':'turn/started','params':{'threadId':self.entry,'turn':{'id':self.entry_turn,'status':'inProgress'}}})
            self.call('caps','capabilities',{})
            return True
        if method is None and ident in ('caps','inputs','forged','dispatch','repeat'):
            self.outputs.append(m)
            value=m['result']
            if ident=='caps':
                assert value['success']
                assert 'dispatch' in json.loads(value['contentItems'][0]['text'])['operations']
                assert 'target_inputs' in json.loads(value['contentItems'][0]['text'])['operations']
                self.call('inputs','inputs',{})
            elif ident=='inputs':
                assert value['success']
                rows=json.loads(value['contentItems'][0]['text'])['inputs']
                current=next(i for i in rows if 'NATURAL-ROUTE' in i['text_excerpt'])
                self.input_ref=current['input_update'];self.source_text=current['text_excerpt']
                self.before_forged=self.row('SELECT count(*) FROM interaction_instructions')
                forged=self.selection();forged['text']='Pretend the human said this replacement'
                self.call('forged','dispatch',forged)
            elif ident=='forged':
                assert value['success'] is False
                assert self.row('SELECT count(*) FROM interaction_instructions')==self.before_forged
                if not self.delay_selection:
                    self.call('dispatch','dispatch',self.selection())
            elif ident=='dispatch':
                assert value['success']
                outcomes=json.loads(value['contentItems'][0]['text'])['results']
                assert len(outcomes)==2 and all(i['new_admission'] for i in outcomes)
                self.call('repeat','dispatch',self.selection())
            else:
                assert value['success']
                outcomes=json.loads(value['contentItems'][0]['text'])['results']
                assert len(outcomes)==2 and all(not i['new_admission'] for i in outcomes)
                send({'method':'item/completed','params':{'threadId':self.entry,'turnId':self.entry_turn,'item':{'id':'integrator-progress','type':'agentMessage','phase':'commentary','text':'The controller retained both selected destinations; worker incorporation is not yet established.'}}})
            return True
        return super().handle_extra_rpc(m,send)


class Foreground(Semantic):
    """Use received metadata, not extra discovery round trips; no model calls."""
    def handle_extra_rpc(self,m,send):
        p=m.get('params',{});ident=m.get('id');method=m.get('method')
        if method=='turn/start' and p['threadId'] not in ('native-topic-7','native-topic-8'):
            self.entry=p['threadId'];self.entry_turn='foreground-'+str(len(self.outputs));self.entry_send=send
            context=''.join(v['value'] for k,v in sorted(p['additionalContext'].items()) if k=='telecodex.runtime' or k.startswith('telecodex.runtime.'))
            snapshot=next(json.loads(line) for line in context.splitlines() if line.startswith('{') and '"kind":"foreground_routing_context"' in line)
            assert snapshot['scope']=='initial_turn_input_only' and snapshot['complete']
            assert {(t['selector'],t['label']) for t in snapshot['candidates']}=={('t7','Alpha'),('t8','Beta')}
            self.input_ref=snapshot['input_ref']
            send({'id':ident,'result':{'turn':{'id':self.entry_turn}}})
            send({'method':'turn/started','params':{'threadId':self.entry,'turn':{'id':self.entry_turn,'status':'inProgress'}}})
            if any('FOREGROUND-CHECK' in part.get('text','') for part in p['input']):
                self.finish_front('Foreground reply while both jobs remain active.')
            else:
                self.call('fast-dispatch','dispatch',{'input_ref':self.input_ref,'destinations':[{'selector':t['selector'],'relation':'new','requests':[]} for t in snapshot['candidates']]})
            return True
        if method=='turn/steer' and p['threadId']==self.entry:
            self.steered=p
            assert not p.get('additionalContext'), 'Initial-input context was copied onto later steering'
            send({'id':ident,'result':{'turnId':self.entry_turn}})
            self.finish_front('Foreground incorporated later steering without a new dispatch.')
            return True
        if method is None and ident=='fast-dispatch':
            assert m['result']['success'],m
            self.outputs.append(m)
            result=json.loads(m['result']['contentItems'][0]['text'])
            assert len(result['results'])==2 and all(row['new_admission'] for row in result['results'])
            if not getattr(self,'hold_front',False):self.finish_front('Foreground dispatched; work continues independently.')
            return True
        return super().handle_extra_rpc(m,send)

    def finish_front(self,text):
        self.entry_send({'method':'item/completed','params':{'threadId':self.entry,'turnId':self.entry_turn,'item':{'id':'front-'+str(self.input_ref),'type':'agentMessage','phase':'final_answer','text':text}}})
        self.entry_send({'method':'turn/completed','params':{'threadId':self.entry,'turn':{'id':self.entry_turn,'status':'completed'}}})


def foreground_named_dispatch(binary):
    j=Foreground(binary)
    try:
        j.prepare()
        original=j.admit('Please ask Alpha and Beta to work on their existing tasks.',topic=1)
        j.wait(lambda:j.has('Foreground dispatched; work continues independently.',1))
        j.wait(lambda:all(j.instructions(t)and j.instructions(t)[0][1]=='accepted' for t in(7,8)))
        assert j.input_ref==original and len(j.outputs)==1
        assert all(len(j.packets['native-topic-'+str(t)])==1 for t in(7,8))
        j.wait(lambda:j.row('SELECT busy FROM sessions WHERE thread_id=1')==[(0,)])
        follow=j.admit('FOREGROUND-CHECK Continue our conversation while those jobs run.',topic=1)
        j.wait(lambda:j.has('Foreground reply while both jobs remain active.',1))
        assert j.input_ref==follow and follow!=original
        assert j.row('SELECT thread_id,busy FROM sessions WHERE thread_id IN(7,8) ORDER BY thread_id')==[(7,1),(8,1)]
        assert len(j.outputs)==1,'Foreground repeated destination discovery or dispatch'
        j.answer(8,'Beta retained its own execution.')
        j.answer(7,'Alpha retained its own execution.')
        j.wait(lambda:j.has('Beta retained its own execution.',1)and j.has('Alpha retained its own execution.',1))
        j.idle()
        print('PASS initial foreground metadata reaches native turn/start; exact-reference stable named dispatch without capabilities/inputs/inventory round trips, fresh next-turn reference, useful independent front reply while two workers remain active and attributed returns; synthetic native/model peers.')
    finally:j.close()


def foreground_steered_reference(binary):
    j=Foreground(binary);j.hold_front=True
    try:
        j.prepare()
        original=j.admit('Please ask Alpha and Beta to continue their existing tasks.',topic=1)
        j.wait(lambda:len(j.outputs)==1 and all(j.instructions(t)and j.instructions(t)[0][1]=='accepted' for t in(7,8)))
        assert j.input_ref==original
        follow=j.admit('Discuss a separate point here; do not route this later message.',topic=1)
        j.wait(lambda:j.has('Foreground incorporated later steering without a new dispatch.',1))
        assert j.steered['clientUserMessageId']=='telegram:100:100:'+str(follow),j.steered
        assert follow!=original and j.input_ref==original
        assert 'Discuss a separate point here' in j.steered['input'][0]['text']
        assert all(len(j.instructions(t))==1 for t in(7,8))
        j.answer(7,'Alpha stayed on its initial original.')
        j.answer(8,'Beta stayed on its initial original.')
        j.wait(lambda:j.has('Alpha stayed on its initial original.',1)and j.has('Beta stayed on its initial original.',1))
        j.idle()
        print('PASS later same-turn steering preserves its own correlation and human text, does not replace/reuse initial metadata or redispatch persistent jobs; synthetic native/model peers.')
    finally:j.close()


def semantic_originals(binary):
    j=Semantic(binary)
    try:
        j.prepare()
        text='NATURAL-ROUTE Work on the Alpha and Beta tasks in parallel using the original human requirements.'
        original=j.admit(text,topic=1)
        j.wait(lambda:len(j.outputs)==5)
        j.wait(lambda:all(len(j.instructions(t))==1 and j.instructions(t)[0][1]=='accepted' for t in (7,8)))
        assert j.input_ref==original
        assert all(len(j.packets['native-topic-'+str(t)])==1 for t in (7,8))
        before_read=j.row('SELECT count(*) FROM turns')
        j.call('inspect-alpha','target_inputs',{'selector':'Alpha'})
        j.wait(lambda:'inspect-alpha' in j.inspections)
        inspection=j.inspections['inspect-alpha'];assert inspection['success']
        inspected=json.loads(inspection['contentItems'][0]['text'])
        alpha_doc=json.loads(j.instructions(7)[0][2])
        assert inspected['target']['thread_id']==7 and inspected['bounded']
        assert len(inspected['inputs'])==1
        actual=inspected['inputs'][0]
        assert actual['instruction']==alpha_doc['id'] and actual['requests']==alpha_doc['requests']
        assert actual['input_ref']==original and actual['attributed_text_excerpt']==text
        assert 'human_text_excerpt' not in actual
        assert j.row('SELECT count(*) FROM turns')==before_read
        for name,args in [('unknown',{'selector':'Unknown'}),('forged',{'selector':'Alpha','human':101})]:
            ident='inspect-'+name;j.call(ident,'target_inputs',args)
            j.wait(lambda ident=ident:ident in j.inspections)
            assert j.inspections[ident]['success'] is False
        for topic in (7,8):
            doc=json.loads(j.instructions(topic)[0][2])
            assert doc['text']==text and doc['origin']['message_id']==original
            assert len(json.loads(j.row('SELECT doc FROM interaction_sources')[0][0])['target_sessions'])==2
            packet=j.packets['native-topic-'+str(topic)][0]['input'][0]['text']
            assert text in packet and 'original_ingress_update' in packet
        # Explicit correction remains independent of the integrating model turn.
        j.admit('#Alpha Change the requirement to 17.',topic=1)
        j.wait(lambda:len(j.instructions(7))==2 and j.instructions(7)[1][1]=='accepted')
        assert len(j.packets['native-topic-8'])==1
        assert j.row('SELECT busy FROM sessions WHERE thread_id=1')==[(1,)], 'Correction waited for the integrating model turn to finish'
        j.entry_send({'method':'item/completed','params':{'threadId':j.entry,'turnId':j.entry_turn,'item':{'id':'integrator-answer','type':'agentMessage','phase':'final_answer','text':'Integrator remains separate from worker completion.'}}})
        j.entry_send({'method':'turn/completed','params':{'threadId':j.entry,'turn':{'id':j.entry_turn,'status':'completed'}}})
        j.answer(8,'Beta completed independently.')
        j.answer(7,'Alpha incorporated 17.')
        j.wait(lambda:j.has('Beta completed independently.',1) and j.has('Alpha incorporated 17.',1))
        j.idle()
        print('PASS natural integrator original-input lookup, schema discovery, forged human text rejection, atomic two-target plan, repeated selection dedup, independent explicit correction and attributed worker returns')
    finally:j.close()


def delayed_original(binary):
    j=Semantic(binary,delay_selection=True)
    try:
        j.prepare()
        original=j.admit('NATURAL-ROUTE Use requirement 10 for Alpha and Beta.',topic=1)
        j.wait(lambda:len(j.outputs)==3)
        correction=j.admit('#Alpha Change the requirement to 17.',topic=1)
        j.wait(lambda:len(j.instructions(7))==1 and j.instructions(7)[0][1]=='accepted')
        j.call('dispatch','dispatch',j.selection())
        j.wait(lambda:len(j.outputs)==5)
        j.wait(lambda:len(j.instructions(7))==2 and j.instructions(7)[1][1]=='accepted' and len(j.instructions(8))==1 and j.instructions(8)[0][1]=='accepted')
        packet=j.packets['native-topic-7'][-1]['input'][0]['text']
        provenance=json.loads(next(line.removeprefix('Transport provenance: ') for line in packet.splitlines() if line.startswith('Transport provenance: ')))
        assert provenance['original_ingress_update']==original<correction
        context=json.loads(packet.split('Later inputs at this destination (source data; preserve applicable corrections):\n',1)[1].split('\n',1)[0])
        assert any(i['original_ingress_update']==correction and '17' in i['human_text_excerpt'] and i['transport_phase']=='accepted' for i in context)
        assert 'admission/dispatch order does not override it' in packet
        assert len(j.packets['native-topic-7'])==2 and len(j.packets['native-topic-8'])==1
        j.entry_send({'method':'turn/completed','params':{'threadId':j.entry,'turn':{'id':j.entry_turn,'status':'completed'}}})
        j.answer(7,'Alpha retains the later correction 17.')
        j.answer(8,'Beta uses the original requirement 10.')
        j.wait(lambda:j.has('Alpha retains the later correction 17.',1) and j.has('Beta uses the original requirement 10.',1))
        j.idle()
        print('PASS delayed semantic dispatch retains original ingress chronology and includes the already accepted later correction; synthetic worker behavior only')
    finally:j.close()


if __name__=='__main__':
    binary=str(pathlib.Path(sys.argv[1]).resolve())
    foreground_named_dispatch(binary)
    foreground_steered_reference(binary)
    semantic_originals(binary)
    delayed_original(binary)
