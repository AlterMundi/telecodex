"""Early native text stays provisional through actual HTTP/SQLite; zero models."""
import json
import hashlib
from urllib.parse import urlencode
import sys
from pathlib import Path
from conversation_cold_entry_http_io import ColdEntry, Browser


def qualify(binary):
    j=ColdEntry(str(Path(binary).resolve()));b=None
    try:
        j.prepare();b=Browser(j)
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

        assert not any(v['preview']=='Useful early answer.'for v in b.get('/api/interaction')['items'])
        assert j.row("SELECT count(*) FROM interaction_native_messages WHERE item_id='blank-completion'")==[(0,)]
        assert j.row('SELECT count(*) FROM interaction_deliveries')==[(0,)]
        # An empty completion retires the exact draft while native work continues.
        emit('item/completed',item={'id':'blank-completion','type':'agentMessage','phase':'final_answer','text':''})
        j.wait(lambda:draft()is None)
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
        print('PASS early native text visible before item completion, no canonical/audio/action authority; final replacement once, worker partial excluded, continued ingress and terminal retirement; actual HTTP/native/SQLite, zero models.')
    finally:
        if b:b.client.close()
        j.close()

if __name__=='__main__':qualify(sys.argv[1])
