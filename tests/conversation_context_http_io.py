"""Reported playback context leaves intent to the integrator; actual controller, zero models."""
from pathlib import Path
import sys
import sqlite3
import json
import threading
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
            'isBlocking':True,'questions':[{'id':'scope','header':'Scope','question':'Which exact scope should Alpha use? '+('á🌱'*2200),'options':None}]}})
        j.wait(lambda:bool(b.get('/api/interaction/requests')['items']))
        question=b.get('/api/interaction/requests')['items'][0]
        question=b.get('/api/interaction/requests/'+question['id'])
        context={'source':'request:'+question['id'],'revision':question['revision'],'context_only':True}
        playback={'source':context['source'],'revision':context['revision'],'segment_offset':0,'position_millis':3700,'duration_millis':12000,'state':'interrupted'}
        assert b.get('/api/interaction')['can_report_playback'] is True
        changed,_=b.text('Start a separate task about the garden in Beta.',context,playback)
        j.wait(lambda:b.status(changed)['phase']=='accepted')
        assert not j.native_answers,'Context alone consumed a native question'
        assert b.status(changed)['playback']==playback
        changed_playback=b.client.post('/api/interaction/inputs',json={'id':changed,'binding':b.binding,'text':'Start a separate task about the garden in Beta.','reply':context,'playback':dict(playback,position_millis=9900)},headers=b.headers)
        assert changed_playback.status_code==503,'Captured media progress changed under the same original identity'
        changed_mode=b.client.post('/api/interaction/inputs',json={'id':changed,'binding':b.binding,
            'text':'Start a separate task about the garden in Beta.','reply':{k:context[k]for k in ('source','revision')}},headers=b.headers)
        assert changed_mode.status_code==503,'An immutable contextual original became an explicit answer'
        assert len(j.packets)==1

        assert '[Device-reported conversational context]'in str(j.packets[0])and changed in str(j.packets[0])
        assert '[Device-reported playback context]'in str(j.packets[0]) and 'not proof of human hearing' in str(j.packets[0])
        view=j.call('route-read-question-context','question_context',{'input_ref':changed})
        assert view['matches_current']and view['question']['body']==question['body']
        caps=j.call('route-playback-capabilities','capabilities',{})
        assert 'playback_context' in caps['operations']
        audio_view=j.call('route-playback-context','playback_context',{'input_ref':changed})
        assert audio_view['report']==playback and audio_view['availability']=='current_source'
        assert audio_view['source_part']['text']==question['body'][:1600]
        assert audio_view['source_part']['offset']==0 and audio_view['source_part']['next_offset']==1600
        assert audio_view['source_part']['id']==playback['source']
        assert audio_view['read_only'] and 'not' in audio_view['interpretation']
        next_audio_view=j.call('route-playback-next','playback_context',{'input_ref':changed,'offset':1600})
        assert next_audio_view['source_part']['text']==question['body'][1600:3200]
        j.call('route-playback-cannot-invent-source','playback_context',{'input_ref':changed,'source':'native:1'},ok=False)
        j.call('route-playback-invalid-offset','playback_context',{'input_ref':changed,'offset':1000001},ok=False)
        j.call('route-playback-unknown-original','playback_context',{'input_ref':'f'*32},ok=False)
        assert b.status(changed)['playback']==playback and not j.native_answers
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
        no_audio=j.call('route-playback-unreported','playback_context',{'input_ref':reserved})
        assert no_audio['availability']=='not_reported' and no_audio['report'] is None
        assert 'source_part' not in no_audio
        j.drop_ack=True
        spoken,_=b.voice(context,playback)
        j.wait(lambda:b.status(spoken)['phase']=='steering'and j.late_ack is not None)
        assert not j.native_answers,'ASR or playback metadata implicitly answered'
        assert b.status(spoken)['playback']==playback
        result=j.call('route-classified-original-answer','answer_question',{'input_ref':spoken})
        assert result['recorded']and result['new_decision']
        j.wait(lambda:any(v['id']=='canonical-context-question'for v in j.native_answers))
        answer=next(v for v in j.native_answers if v['id']=='canonical-context-question')['result']['answers']['scope']['answers'][0]
        assert '[Committed voice input]'in answer and 'Continue requirement17'in answer
        assert '[Device-reported conversational context]'not in answer
        assert '[Device-reported playback context]'not in answer
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
        old_audio=j.call('route-playback-answered','playback_context',{'input_ref':changed})
        assert old_audio['availability'] in ('source_not_current','source_unconfirmed')
        assert 'source_part' not in old_audio and old_audio['report']==playback
        body='Canonical spoken worker result. '+('Español 🌱 mixed English. '*150)
        j.answer_worker(7,body)
        def returned_source():
            return next((item for item in b.get('/api/interaction')['items']
                         if item.get('kind')=='worker_result' and item.get('preview','').startswith('Canonical spoken worker result.')),None)
        j.wait(lambda:returned_source() is not None)
        source=returned_source()
        worker_audio={**playback,'source':source['id'],'revision':source['revision'],'segment_offset':12}
        worker_input,_=b.text('Discuss the interrupted worker explanation.',None,worker_audio)
        j.wait(lambda:b.status(worker_input)['phase']=='accepted')
        worker_view=j.call('route-playback-worker-source','playback_context',{'input_ref':worker_input})
        assert worker_view['availability']=='current_source'
        assert worker_view['source_part']['text']==body[12:1612]
        assert worker_view['source_part']['offset']==12 and worker_view['source_part']['next_offset']==1612
        assert worker_view['source_part']['target']['thread_id']==7
        with sqlite3.connect(j.root/'state.sqlite')as db:
            db.execute('UPDATE sessions SET force_fresh_thread=1 WHERE thread_id=7')
        replaced=j.call('route-playback-replaced-worker','playback_context',{'input_ref':worker_input})
        assert replaced['availability']=='source_not_current' and 'source_part' not in replaced
        missing_audio={**playback,'source':'native:9223372036854775807'}
        missing_input,_=b.text('Unrelated new topic with an unavailable audio source.',None,missing_audio)
        j.wait(lambda:b.status(missing_input)['phase']=='accepted')
        missing_view=j.call('route-playback-missing','playback_context',{'input_ref':missing_input})
        assert missing_view['availability']=='source_unconfirmed' and 'source_part' not in missing_view
        print('PASS actual HTTP/native/SQLite contextual text and committed voice; context does not answer or select target, semantic new-topic dispatch and exact original answer, changed/stale/missing source, immutable receipt retry, no invented text or double consumption; zero models.')
    finally:
        if b:b.client.close()
        j.close()


def unhinted_question_selection(binary):
    j=ColdEntry(str(Path(binary).resolve()));b=None;recording=None
    try:
        j.prepare();b=Browser(j)
        for name,topic in (('Alpha',7),('Beta',8)):
            b.text('#'+name+' Work on this independent requirement.')
            j.wait(lambda topic=topic:j.instructions(topic)and j.instructions(topic)[0][1]=='accepted')
        early,_=b.text('Discuss this before any question exists.')
        j.wait(lambda:b.status(early)['phase']=='accepted')
        workers={topic:next(send for thread,send in j.clients.values()if thread=='native-topic-'+str(topic))for topic in(7,8)}
        def ask(topic,rpc,questions):
            workers[topic]({'id':rpc,'method':'item/tool/requestUserInput','params':{
                'threadId':'native-topic-'+str(topic),'turnId':j.workers['native-topic-'+str(topic)],'itemId':rpc,'isBlocking':True,'questions':questions}})
        def question(ident,body):
            return {'id':ident,'header':ident.title(),'question':body,'isOther':True,'options':None}
        ask(7,'canonical-selection-alpha',[question('colour','Which colour should Alpha use?'),question('scope','Which final scope should Alpha use?')])
        ask(8,'canonical-selection-beta',[question('scope','Which scope should Beta use?')])
        workers[8]({'id':'canonical-selection-approval','method':'item/commandExecution/requestApproval','params':{
            'threadId':'native-topic-8','turnId':j.workers['native-topic-8'],'itemId':'selection-approval','command':'echo synthetic-only','reason':'Never approve from an interpreted original','availableDecisions':['accept','decline']}})
        j.wait(lambda:j.row("SELECT count(*)FROM human_interactions WHERE state='pending'")==[(3,)])
        items=b.get('/api/interaction/requests')['items']
        alpha=next(v for v in items if v['kind']=='question'and v['label']=='#Alpha')
        beta=next(v for v in items if v['kind']=='question'and v['label']=='#Beta')
        approval=next(v for v in items if v['kind']=='approval')
        def selection(view):return {k:view[k]for k in('id','revision')}
        def call(tag,operation,ident,view=None,ok=True,**extra):
            args={'input_ref':ident,**extra}
            if view:args['question']=selection(view)
            return j.call('route-select-'+tag,operation,args,ok=ok)
        page=call('early-list','questions',early)
        assert len(page['items'])==2 and all(not v['available_at_capture']for v in page['items'])
        assert all(v['kind']=='question'for v in page['items'])
        call('not-yet-asked','answer_question',early,alpha,ok=False)
        plain,_=b.text('Use blue for Alpha colour.')
        before_step,_=b.text('This original was captured before Alpha advanced to its next question.')
        j.wait(lambda:all(b.status(ident)['phase']=='accepted'for ident in(plain,before_step)))
        assert not j.native_answers,'Plain text implicitly answered a question'
        catalog=call('plain-list','questions',plain)
        assert all(v['available_at_capture']for v in catalog['items'])and not catalog['more']
        assert not call('next-page','questions',plain,after=catalog['cursor'])['items']
        call('bad-page','questions',plain,ok=False,after=-1)
        full=call('plain-read','question_context',plain,alpha)
        assert full['matches_current']and full['available_at_capture']and 'Which colour' in full['question']['body']
        call('approval-refused','answer_question',plain,approval,ok=False)
        call('invented-answer','answer_question',plain,alpha,ok=False,text='Invented human speech')
        call('bad-revision','answer_question',plain,{**alpha,'revision':'f'*64},ok=False)
        prior_update=j.row('SELECT updated_at FROM human_interactions WHERE id=?',(alpha['id'],))[0][0]
        callback_message={'message_id':4242,'chat':{'id':100,'type':'private'}}
        j.admit(callback=(callback_message,'qopen:'+alpha['id']),topic=1)
        j.wait(lambda:j.row('SELECT updated_at FROM human_interactions WHERE id=?',(alpha['id'],))[0][0]!=prior_update)
        reopened=call('unchanged-reopen','question_context',plain,alpha)
        assert reopened['matches_current']and reopened['available_at_capture'],'Reopening unchanged controls invalidated the original'
        answered=call('plain-answer','answer_question',plain,alpha)
        assert answered['recorded']and answered['new_decision']
        assert b.status(plain)['phase']=='answered'and b.status(plain)['decision']['source_input']==plain
        assert not j.native_answers,'A multi-part native question closed before its final answer'
        alpha2=b.get('/api/interaction/requests/'+alpha['id'])
        assert alpha2['revision']!=alpha['revision']and 'final scope' in alpha2['body']
        call('earlier-step','answer_question',before_step,alpha2,ok=False)
        versions=call('independent-versions','questions',before_step)['items']
        assert not next(v for v in versions if v['id']==alpha['id'])['available_at_capture']
        assert next(v for v in versions if v['id']==beta['id'])['available_at_capture'],'An unrelated question change invalidated Beta'
        assert not call('plain-identical','answer_question',plain,alpha)['new_decision']
        call('plain-no-dispatch','dispatch',plain,ok=False,destinations=[{'selector':'Beta','relation':'new','requests':[]}])
        context={'source':'request:'+alpha2['id'],'revision':alpha2['revision'],'context_only':True}
        other,_=b.text('For Beta, keep only the selected requirement.',context)
        j.wait(lambda:b.status(other)['phase']=='accepted')
        call('explicit-other-question','answer_question',other,beta)
        j.wait(lambda:any(v['id']=='canonical-selection-beta'for v in j.native_answers))
        actual=next(v for v in j.native_answers if v['id']=='canonical-selection-beta')['result']['answers']['scope']['answers'][0]
        assert actual=='For Beta, keep only the selected requirement.'
        status=b.status(other);assert status['decision']['id']==beta['id']and status['decision']['source_input']==other
        with sqlite3.connect(j.root/'state.sqlite')as db:
            retained=json.loads(db.execute('SELECT doc FROM conversation_inputs WHERE id=?',(other,)).fetchone()[0])
        assert retained['reply']==context,'Semantic selection rewrote the device context'
        call('consumed-no-second-question','answer_question',other,alpha2,ok=False)

        (j.root/'hold-asr').touch();voice_result=[];voice_errors=[]
        def voice():
            try:voice_result.append(b.voice())
            except BaseException as error:voice_errors.append(error)
        recording=threading.Thread(target=voice);recording.start()
        j.wait(lambda:j.row("SELECT count(*) FROM conversation_inputs WHERE phase='preparing'")==[(1,)])
        ask(8,'canonical-selection-later',[{**question('first','Which identical scope should Beta use?'),'header':'Scope'},{**question('second','Which identical scope should Beta use?'),'header':'Scope'}])
        pending_later="SELECT id FROM human_interactions WHERE state='pending' AND json_extract(payload_json,'$.source.itemId')='canonical-selection-later'"
        j.wait(lambda:len(j.row(pending_later))==1)
        later=b.get('/api/interaction/requests/'+j.row(pending_later)[0][0])
        assert later['kind']=='question'and later['label']=='#Beta'and later['actionable']
        (j.root/'release-asr').touch();recording.join(timeout=15)
        assert not recording.is_alive()and not voice_errors,voice_errors
        spoken=voice_result[0][0];j.wait(lambda:b.status(spoken)['phase']=='accepted')
        late_view=call('late-asr-read','question_context',spoken,later)
        assert late_view['matches_current']and not late_view['available_at_capture']
        call('late-asr-refused','answer_question',spoken,later,ok=False)
        assert b.status(spoken)['phase']=='accepted'
        legacy,_=b.text('A retained legacy original has no version snapshot.')
        j.wait(lambda:b.status(legacy)['phase']=='accepted')
        with sqlite3.connect(j.root/'state.sqlite')as db:db.execute("UPDATE conversation_inputs SET doc=json_remove(doc,'$.question_versions')WHERE id=?",(legacy,))
        call('no-legacy-snapshot','answer_question',legacy,later,ok=False)
        reserved,_=b.text('An original reserved for creation must not answer as well.')
        j.wait(lambda:b.status(reserved)['phase']=='accepted')
        with sqlite3.connect(j.root/'state.sqlite')as db:
            db.execute("INSERT INTO interaction_creations(id,human_id,entry_chat,entry_topic,input_update,phase,doc,instance_id)SELECT 'question-selection-reserved',100,100,1,?,'prepared','{}',instance_id FROM conversation_inputs WHERE id=?",(reserved,reserved))
        call('creation-exclusive','answer_question',reserved,later,ok=False)
        routed,_=b.text('Use this original as a new Beta task, never also an answer.')
        j.wait(lambda:b.status(routed)['phase']=='accepted')
        call('route-exclusive','dispatch',routed,destinations=[{'selector':'Beta','relation':'new','requests':[]}])
        call('routed-no-answer','answer_question',routed,later,ok=False)
        final,_=b.text('Use only the final Alpha requirement.')
        j.wait(lambda:b.status(final)['phase']=='accepted')
        call('final-alpha','answer_question',final,alpha2)
        j.wait(lambda:any(v['id']=='canonical-selection-alpha'for v in j.native_answers))
        answers=next(v for v in j.native_answers if v['id']=='canonical-selection-alpha')['result']['answers']
        assert answers['colour']['answers']==['Use blue for Alpha colour.']and answers['scope']['answers']==['Use only the final Alpha requirement.']
        old_same,_=b.text('This was captured before the identical-looking next question.')
        first_same,_=b.text('Use the first selected Beta scope.')
        j.wait(lambda:all(b.status(ident)['phase']=='accepted'for ident in(old_same,first_same)))
        call('same-proposal-first','answer_question',first_same,later)
        later2=b.get('/api/interaction/requests/'+later['id'])
        assert later2['revision']!=later['revision']and later2['question_index']==1
        assert 'Which identical scope' in later2['body']
        call('same-visible-proposal-next','answer_question',old_same,later2,ok=False)
        second_same,_=b.text('Use the second selected Beta scope.')
        j.wait(lambda:b.status(second_same)['phase']=='accepted')
        call('same-proposal-second','answer_question',second_same,later2)
        j.wait(lambda:any(v['id']=='canonical-selection-later'for v in j.native_answers))
        same_answers=next(v for v in j.native_answers if v['id']=='canonical-selection-later')['result']['answers']
        assert same_answers['first']['answers']==['Use the first selected Beta scope.']and same_answers['second']['answers']==['Use the second selected Beta scope.']
        journal=j.row('SELECT count(*)FROM conversation_decisions')
        j.stop();j.start();j.wait_control_socket(j.socket)
        assert b.status(plain)['decision']['source_input']==plain and b.status(other)['decision']['id']==beta['id']
        assert j.row('SELECT count(*)FROM conversation_decisions')==journal
        print('PASS actual HTTP/native/SQLite unhinted question selection: exact original answers, independent concurrent questions, unchanged conflicting playback hint, multi-question capture versions, late ASR/legacy refusal, approvals excluded, no invented text/double-use, immutable receipts and restart; zero models.')
    finally:
        (j.root/'release-asr').touch()
        if recording is not None:recording.join(timeout=30)
        if b:b.client.close()
        j.close()


if __name__=='__main__':
    qualify(sys.argv[1])
    unhinted_question_selection(sys.argv[1])
