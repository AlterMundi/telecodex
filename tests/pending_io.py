#!/usr/bin/env python3
"""Mobile pending/history and withdrawal across HTTP/native/SQLite boundaries.
All accounts, requests and model responses are synthetic; no live Telegram call.
"""
import json,pathlib,re,sys,sqlite3,threading
from goals_io import Goals

class Pending(Goals):
    def __init__(self,binary,language='en'):
        super().__init__(binary,'complete');self.declaration=None
        self.delay_force_reply=False;self.force_reply_waiting=threading.Event();self.release_force_reply=threading.Event()
        self.config.write_text(self.config.read_text().replace('[telegram]\n',f'[telegram]\nhuman_request_language="{language}"\n'))
    def before_send_message_reply(self,payload):
        if self.delay_force_reply and payload.get('reply_markup',{}).get('force_reply'):
            self.force_reply_waiting.set();self.release_force_reply.wait(timeout=15)
    def finish(self,thread,turn,continuation=False):
        if self.declaration is not None:
            request=next(m for m in reversed(self.rpc) if m.get('method')=='turn/start' and m['params']['threadId']==thread)
            runtime=''.join(v['value'] for k,v in sorted(request['params']['additionalContext'].items()) if k=='telecodex.runtime' or k.startswith('telecodex.runtime.'))
            filename=re.search(r'explicitly declare it in (.*?)\. This optional regular JSON',runtime).group(1)
            pathlib.Path(filename).write_text(json.dumps(self.declaration));self.declaration=None
        super().finish(thread,turn,continuation)
    def completed(self):
        self.wait(lambda:bool(self.row('SELECT id FROM turns')) and self.row("SELECT COUNT(*) FROM turns WHERE status='running'")==[(0,)])
    def start(self):
        super().start()
        def ready():
            try:return bool(self.row("SELECT name FROM sqlite_master WHERE name='incoming_updates'"))
            except sqlite3.OperationalError:return False
        self.wait(ready)
    def settled(self,identifier):
        self.wait(lambda:self.row('SELECT status FROM incoming_updates WHERE update_id=?',(identifier,)) in [[('settled',)],[('failed',)]])
    def declare(self,requests=None,withdrawals=None,topic=7):
        self.declaration={'schema':'telecodex.human-requests/v1','requests':requests or [],'withdrawals':withdrawals or []}
        identifier=self.admit('Explicit manifest operations',topic=topic);self.settled(identifier);self.completed()
    def list(self,history=False,offset=0,topic=7,user=100):
        before=len(self.messages)
        if history or offset:
            self.admit(callback=({'message_id':1000,'message_thread_id':topic,'chat':{'id':100,'type':'private'}},f'{"qhistory" if history else "qpage"}:{offset}'),topic=topic,user=user)
        else:self.admit('/pending',topic=topic,user=user)
        self.wait(lambda:any('reply_markup' in m[1] and ('Human requests ·' in m[1]['text'] or 'History ·' in m[1]['text'] or 'Pendientes ·' in m[1]['text'] or 'Historial ·' in m[1]['text']) for m in self.messages[before:]))
        return next(m for m in reversed(self.messages[before:]) if 'reply_markup' in m[1])
    def open(self,listing,ident):
        before=len(self.messages);self.admit(callback=(listing[0],f'hp:{ident}:open'))
        self.wait(lambda:any(m[1]['text'].startswith('Human request: ') for m in self.messages[before:]))
        return next(m for m in reversed(self.messages[before:]) if m[1]['text'].startswith('Human request: '))

def task(n):return {'id':f'task-{n}','title':f'Request {n} '+('long title 🌿 '*8),'action':'Selected phone-only action','links':[]}
def entries(message):return [b for row in message[1]['reply_markup']['inline_keyboard'] for b in row if b.get('callback_data','').startswith(('hp:','qopen:'))]

def mobile_and_totals(binary,language):
    j=Pending(binary,language)
    try:
        j.start();j.declare([task(n) for n in range(3)]);listing=j.list();assert len(entries(listing))==3
        ids=[r[0] for r in j.row("SELECT id FROM human_interactions WHERE kind='task' ORDER BY client_id")]
        # Actual callback and feedback routes create distinct resolved lifecycles.
        done=j.open(listing,ids[1]);starts=j.native_turns;j.admit(callback=j.button(done,':done'));j.wait(lambda:j.row('SELECT state FROM human_interactions WHERE id=?',(ids[1],))==[('completed',)]);assert j.native_turns==starts
        reply=j.open(listing,ids[2]);j.admit(callback=j.button(reply,':reply'));j.wait(lambda:any(m[1].get('reply_markup',{}).get('force_reply') for m in j.messages));feedback=j.admit('The observed feedback');j.wait(lambda:j.row('SELECT state FROM human_interactions WHERE id=?',(ids[2],))==[('answered',)]);j.settled(feedback);j.completed()
        listing=j.list();assert len(entries(listing))==1 and ('1 para vos' if language=='es' else '1 pending for you') in listing[1]['text']
        assert entries(listing)[0]['text'].startswith('⏳ ') and len(entries(listing)[0]['text'])<80 and entries(listing)[0]['text'].endswith('…')
        history=j.list(history=True);assert len(entries(history))==2
        labels=[b['text'] for b in entries(history)];assert any(s.startswith('✅ ') for s in labels) and any(s.startswith('💬 ') for s in labels)
        for start in range(3,56,10):j.declare([task(n) for n in range(start,min(start+10,56))])
        listing=j.list();assert len(entries(listing))==50 and ('54 para vos' if language=='es' else '54 pending for you') in listing[1]['text']
        assert '1–50' in listing[1]['text'] and '54' in listing[1]['text']
        second=j.list(offset=50);assert len(entries(second))==4 and '51–54' in second[1]['text']
        assert any(b.get('callback_data')=='qhistory:0' for row in second[1]['reply_markup']['inline_keyboard'] for b in row)
        foreign=j.list(user=101);assert not entries(foreign) and '0' in foreign[1]['text'];assert not entries(j.list(history=True,topic=8))
        j.stop();j.start();assert len(entries(j.list(history=True)))==2
        print('PASS',language,'state-first long titles, actionable/history separation, 54 topic-total records across pages, owner/topic isolation and restart')
    finally:j.close()

def withdrawal_and_stale_controls(binary):
    j=Pending(binary)
    try:
        j.start();j.declare([task(0)]);listing=j.list();ident=j.row("SELECT id FROM human_interactions WHERE kind='task'")[0][0];detail=j.open(listing,ident)
        j.mode='hold';j.declaration={'schema':'telecodex.human-requests/v1','withdrawals':[{'id':ident,'reason':'The requested handoff is already observed; this is not phone acceptance.','links':[{'label':'Evidence','url':'https://example.org/receipt'}]}]}
        j.admit('Retire obsolete selected action');j.wait(lambda:bool(j.holding));j.admit(callback=j.button(detail,':reply'));j.wait(lambda:any(m[1].get('reply_markup',{}).get('force_reply') for m in j.messages))
        held_thread,held_turn=next(iter(j.holding.items()));j.finish(held_thread,held_turn);j.completed()
        j.wait(lambda:j.row('SELECT state FROM human_interactions WHERE id=?',(ident,))==[('cancelled',)])
        assert j.row("SELECT COUNT(*) FROM audit_log WHERE action='human_task_withdrawn_by_agent'")==[(1,)]
        assert not entries(j.list());history=j.list(history=True);assert entries(history)[0]['text'].startswith('↩️ Withdrawn')
        reopened=j.open(history,ident);assert 'Withdrawal reason:' in reopened[1]['text'];assert not any(b.get('callback_data','').endswith((':reply',':done',':cancel')) for row in reopened[1].get('reply_markup',{}).get('inline_keyboard',[]) for b in row)
        starts=j.native_turns;j.admit(callback=j.button(detail,':done'));j.wait(lambda:any('cannot accept that action' in m[1]['text'] for m in j.messages));assert j.native_turns==starts
        j.mode='complete';ordinary=j.admit('Ordinary conversation after withdrawal');j.wait(lambda:j.native_turns==starts+1);j.settled(ordinary);j.completed();assert j.row('SELECT status FROM incoming_updates WHERE update_id=?',(ordinary,))==[('settled',)]
        j.stop();j.start();assert not entries(j.list());assert entries(j.list(history=True))[0]['text'].startswith('↩️ Withdrawn')
        print('PASS scoped audited withdrawal, reason inspection, retired text capture, stale controls, ordinary next text and restart; no human completion inferred')
    finally:j.close()

def withdrawal_during_force_reply(binary):
    j=Pending(binary)
    try:
        j.start();j.declare([task(0)]);ident=j.row("SELECT id FROM human_interactions WHERE kind='task'")[0][0];detail=j.open(j.list(),ident)
        j.mode='hold';j.declaration={'schema':'telecodex.human-requests/v1','withdrawals':[{'id':ident,'reason':'Obsolete action retired while the reply control is in flight.'}]}
        original=j.admit('Retire action during delayed HTTP reply');j.wait(lambda:bool(j.holding))
        j.delay_force_reply=True;callback=j.admit(callback=j.button(detail,':reply'));j.wait(j.force_reply_waiting.is_set)
        thread,turn=next(iter(j.holding.items()));j.finish(thread,turn)
        j.wait(lambda:j.row('SELECT state FROM human_interactions WHERE id=?',(ident,))==[('cancelled',)]);j.settled(original)
        j.release_force_reply.set();j.wait(lambda:j.row('SELECT status FROM incoming_updates WHERE update_id=?',(callback,))==[('handled',)])
        j.wait(lambda:any('Text capture was not armed' in m[1]['text'] for m in j.messages))
        starts=j.native_turns;j.mode='complete';ordinary=j.admit('Ordinary input after late ForceReply');j.settled(ordinary);assert j.native_turns==starts+1
        assert j.row('SELECT state FROM human_interactions WHERE id=?',(ident,))==[('cancelled',)]
        prompt=next(m for m in reversed(j.messages) if m[1].get('reply_markup',{}).get('force_reply'))
        stale=j.admit('Late explicit task quote',reply=prompt[0]['message_id']);j.wait(lambda:j.row('SELECT status FROM incoming_updates WHERE update_id=?',(stale,))==[('handled',)])
        assert j.native_turns==starts+1
        print('PASS withdrawal during delayed ForceReply cannot re-arm retired text capture; ordinary input continues and stale quote remains refused')
    finally:j.release_force_reply.set();j.close()

def refused_mixed_declaration(binary):
    j=Pending(binary)
    try:
        j.start();j.declare([task(0)]);ident=j.row("SELECT id FROM human_interactions WHERE kind='task'")[0][0]
        j.declaration={'schema':'telecodex.human-requests/v1','requests':[task(1)],'withdrawals':[{'id':ident,'reason':'Wrong-topic attempt'}]}
        original=j.admit('Refuse foreign mixed declaration',topic=8)
        j.wait(lambda:j.row("SELECT status FROM turns ORDER BY id DESC LIMIT 1")==[('failed',)])
        j.wait(lambda:j.row('SELECT status FROM incoming_updates WHERE update_id=?',(original,))==[('undetermined',)])
        assert j.row("SELECT COUNT(*) FROM human_interactions WHERE kind='task'")==[(1,)] and j.row('SELECT state FROM human_interactions WHERE id=?',(ident,))==[('pending',)]
        assert j.row("SELECT COUNT(*) FROM audit_log WHERE action='human_task_withdrawn_by_agent'")==[(0,)]
        assert list((j.root/'.telecodex/turns').glob('*/human-requests.json'))
        print('PASS wrong-topic mixed manifest refused atomically, original task and audit preserved, declaration retained without replay')
    finally:j.close()

if __name__=='__main__':
    binary=str(pathlib.Path(sys.argv[1]).resolve())
    mobile_and_totals(binary,'en');mobile_and_totals(binary,'es');withdrawal_and_stale_controls(binary);withdrawal_during_force_reply(binary);refused_mixed_declaration(binary)
