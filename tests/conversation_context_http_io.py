"""Reported playback context leaves intent to the integrator; actual controller, zero models."""
from pathlib import Path
import sys
import sqlite3
from conversation_cold_entry_http_io import ColdEntry, Browser


def qualify(binary):
    j=ColdEntry(str(Path(binary).resolve()));b=None
    try:
        j.prepare();b=Browser(j)
        b.text('#Alpha Work on the selected requirement.')
        j.wait(lambda:j.instructions(7)and j.instructions(7)[0][1]=='accepted')
        worker=next(send for thread,send in j.clients.values()if thread=='native-topic-7')
        worker({'id':'canonical-context-question','method':'item/tool/requestUserInput','params':{
            'threadId':'native-topic-7','turnId':j.workers['native-topic-7'],'itemId':'context-question',
            'isBlocking':True,'questions':[{'id':'scope','header':'Scope','question':'Which exact scope should Alpha use?','options':None}]}})
        j.wait(lambda:bool(b.get('/api/interaction/requests')['items']))
        question=b.get('/api/interaction/requests')['items'][0]
        question=b.get('/api/interaction/requests/'+question['id'])
        context={'source':'request:'+question['id'],'revision':question['revision'],'context_only':True}
        changed,_=b.text('Start a separate task about the garden in Beta.',context)
        j.wait(lambda:b.status(changed)['phase']=='accepted')
        assert not j.native_answers,'Context alone consumed a native question'
        changed_mode=b.client.post('/api/interaction/inputs',json={'id':changed,'binding':b.binding,
            'text':'Start a separate task about the garden in Beta.','reply':{k:context[k]for k in ('source','revision')}},headers=b.headers)
        assert changed_mode.status_code==503,'An immutable contextual original became an explicit answer'
        assert len(j.packets)==1

        assert '[Device-reported conversational context]'in str(j.packets[0])
        view=j.call('route-read-question-context','question_context',{'input_ref':changed})
        assert view['matches_current']and view['question']['body']==question['body']
        j.call('route-context-new-topic','dispatch',{'input_ref':changed,'destinations':[{'selector':'Beta','relation':'new','requests':[]}]})
        j.wait(lambda:j.instructions(8)and j.instructions(8)[0][1]=='accepted')
        j.call('route-no-double-use','answer_question',{'input_ref':changed},ok=False)
        assert not j.native_answers and b.get('/api/interaction/requests/'+question['id'])['actionable']

        reserved,_=b.text('Use the selected scope while another creation intent is reserved.',context)
        j.wait(lambda:b.status(reserved)['phase']=='accepted')
        with sqlite3.connect(j.root/'state.sqlite')as db:
            db.execute("INSERT INTO interaction_creations(id,human_id,entry_chat,entry_topic,input_update,phase,doc,instance_id) SELECT 'synthetic-reserved-creation',100,100,1,?,'prepared','{}',instance_id FROM conversation_inputs WHERE id=?",(reserved,reserved))
        j.call('route-no-answer-after-creation-reservation','answer_question',{'input_ref':reserved},ok=False)
        assert not j.native_answers
        j.drop_ack=True
        spoken,_=b.voice(context)
        j.wait(lambda:b.status(spoken)['phase']=='steering'and j.late_ack is not None)
        assert not j.native_answers,'ASR or playback metadata implicitly answered'
        result=j.call('route-classified-original-answer','answer_question',{'input_ref':spoken})
        assert result['recorded']and result['new_decision']
        j.wait(lambda:any(v['id']=='canonical-context-question'for v in j.native_answers))
        answer=next(v for v in j.native_answers if v['id']=='canonical-context-question')['result']['answers']['scope']['answers'][0]
        assert '[Committed voice input]'in answer and 'Continue requirement17'in answer
        assert '[Device-reported conversational context]'not in answer
        assert b.status(spoken)['decision']['source_input']==spoken and b.status(spoken)['phase']=='answered'
        send,rpc,turn=j.late_ack;send({'id':rpc,'result':{'turnId':turn}})
        j.drop_ack=False
        assert b.status(spoken)['phase']=='answered','Late native acknowledgment reopened a consumed original'
        repeated=j.call('route-identical-answer-receipt','answer_question',{'input_ref':spoken})
        assert repeated['recorded']and not repeated['new_decision']
        j.call('route-no-dispatch-after-answer','dispatch',{'input_ref':spoken,'destinations':[{'selector':'Beta','relation':'new','requests':[]}]},ok=False)
        j.call('route-no-invented-answer','answer_question',{'input_ref':spoken,'text':'Model invented text'},ok=False)
        assert len(j.native_answers)==1

        stale,_=b.text('Discuss a new project despite the old playback context.',context)
        j.wait(lambda:b.status(stale)['phase']=='accepted')
        j.call('route-old-context-cannot-answer','answer_question',{'input_ref':stale},ok=False)
        assert b.status(stale)['phase']=='accepted'and len(j.native_answers)==1
        unknown={**context,'source':'request:'+'f'*32}
        missing,_=b.text('Start a new topic even though the reported source is unavailable.',unknown)
        j.wait(lambda:b.status(missing)['phase']=='accepted')
        j.call('route-missing-context-cannot-answer','answer_question',{'input_ref':missing},ok=False)
        assert len(j.native_answers)==1
        print('PASS actual HTTP/native/SQLite contextual text and committed voice; context does not answer or select target, semantic new-topic dispatch and exact original answer, changed/stale/missing source, immutable receipt retry, no invented text or double consumption; zero models.')
    finally:
        if b:b.client.close()
        j.close()


if __name__=='__main__':qualify(sys.argv[1])
