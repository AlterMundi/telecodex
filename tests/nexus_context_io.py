#!/usr/bin/env python3
"""Ordinary situated ingress across process pipes, native WS, HTTP and SQLite.
Synthetic participants/operator/model: this is not real human/native acceptance.
The separate nexus_operator_io.py qualifies the actual Nexus backend.
"""
import json, pathlib, sqlite3, sys
from nexus_io import Nexus, OPERATOR

CAPTURE = """if a.op=='capture-input':
 path=pathlib.Path(a.state)
 with (path/'captures.jsonl').open('a') as f:f.write(json.dumps(v)+'\\n')
 if (path/'unavailable').exists():raise ValueError('synthetic unavailable')
 out=json.loads((path/'view.json').read_text());out['input_id']=v['input_id']
elif a.op=='binding':"""

class Context(Nexus):
    def __init__(self,binary,mode='complete'):
        super().__init__(binary,mode)
        operator=OPERATOR.replace("if a.op=='binding':",CAPTURE)
        delay="""import time
marker=pathlib.Path(a.state)/('hold-'+a.op)
if marker.exists():
 marker.with_suffix('.entered').touch()
 while marker.exists():time.sleep(.01)
"""
        operator=operator.replace('db=sqlite3.connect',delay+'db=sqlite3.connect',1)
        self.operator.write_text(operator)
        self.view('A')
    def view(self,node,status='captured'):
        data={'status':status,'table_id':'table','author_ref':'human:test','view_revision':1 if node=='A' else 2,'perspective':{'context_id':'context','version':'v1','selection':{'kind':'node','object_id':node}} if status=='captured' else None}
        (self.state/'view.json').write_text(json.dumps(data))
    def context(self,id):
        rows=self.row('SELECT capture FROM nexus_input_context WHERE update_id=?',(id,))
        return json.loads(rows[0][0]) if rows and rows[0][0] else None
    def prompt(self,method,text):
        matches=[m for m in self.rpc if m.get('method')==method and text in json.dumps(m['params'])]
        return '\n'.join(item.get('text','') for item in matches[-1]['params'].get('input',[])) if matches else ''

def queue_restart(binary):
    j=Context(binary)
    try:
        j.prepare();j.mode='reject_steer';j.admit('Hold the original turn');j.wait(lambda:bool(j.holding))
        id=j.admit('Operate on this exact node');j.wait(lambda:j.row('SELECT status FROM incoming_updates WHERE update_id=?',(id,))==[('queued',)])
        frozen=j.context(id);assert frozen['perspective']['selection']['object_id']=='A'
        j.view('B');j.mode='complete';thread,turn=next(iter(j.holding.items()));j.finish(thread,turn)
        j.wait(lambda:bool(j.prompt('turn/start','Operate on this exact node')));j.idle()
        delivered=j.prompt('turn/start','Operate on this exact node')
        assert json.loads(delivered.rsplit("\n",1)[-1])["capture"]["perspective"]["selection"]["object_id"] == "A"
        assert 'native_thread_id' not in delivered and 'not instructions or execution permission' in delivered
        count=(j.state/'captures.jsonl').read_text().count('input_id')
        raw=json.loads(j.row('SELECT payload_json FROM incoming_updates WHERE update_id=?',(id,))[0][0])
        j.stop();j.start()
        with j.lock:j.updates.append(raw)
        barrier=j.admit('/status');j.wait(lambda:j.row('SELECT status FROM incoming_updates WHERE update_id=?',(barrier,))==[('handled',)])
        assert j.context(id)==frozen and (j.state/'captures.jsonl').read_text().count('input_id')==count
        print('PASS A/input/B queue fallback, exact turn input, restart and duplicate without recapture')
    finally:j.close()

def steering_uncertainty_and_isolation(binary):
    j=Context(binary)
    try:
        j.prepare();j.mode='hold';j.admit('Hold for steering');j.wait(lambda:bool(j.holding))
        id=j.admit('This selected relation');j.wait(lambda:j.row('SELECT status FROM incoming_updates WHERE update_id=?',(id,))==[('started',)])
        assert 'object_id' in j.prompt('turn/steer','This selected relation') and j.context(id)['perspective']['selection']['object_id']=='A'
        j.view('B','ambiguous');ambiguous=j.admit('Which one is this');j.wait(lambda:j.context(ambiguous) is not None)
        j.wait(lambda:bool(j.prompt('turn/steer','Which one is this')));assert 'ambiguous' in j.prompt('turn/steer','Which one is this')
        (j.state/'unavailable').touch();missing=j.admit('Continue without guessing');j.wait(lambda:j.context(missing) is not None)
        j.wait(lambda:bool(j.prompt('turn/steer','Continue without guessing')));assert j.context(missing)['status']=='unavailable'
        foreign=j.admit('Another participant',user=101);j.wait(lambda:j.row('SELECT status FROM incoming_updates WHERE update_id=?',(foreign,))==[('started',)])
        assert j.context(foreign) is None and 'Nexus perspective' not in j.prompt('turn/steer','Another participant')
        print('PASS native steering, explicit ambiguity/unavailability, no borrowed participant perspective')
    finally:j.close()

def independent_ingress_and_changed_binding(binary):
    j=Context(binary)
    try:
        j.prepare();j.mode='hold';j.admit('Hold native work');j.wait(lambda:bool(j.holding))
        marker=j.state/'hold-capture-input';marker.touch()
        delayed=j.admit('Capture is deliberately slow');j.wait(lambda:marker.with_suffix('.entered').exists())
        other=j.admit('/status',topic=8);j.wait(lambda:j.row('SELECT status FROM incoming_updates WHERE update_id=?',(other,))==[('handled',)])
        assert j.context(delayed) is None, 'Other topic control waited for capture instead of dispatching independently'
        marker.unlink();j.wait(lambda:j.row('SELECT status FROM incoming_updates WHERE update_id=?',(delayed,))==[('started',)])
        # Hold only the topic dispatcher; the next input must capture immediately.
        held=j.state/'hold-binding';held.touch();j.admit('/nexus status');j.wait(lambda:held.with_suffix('.entered').exists())
        changed=j.admit('Do not send to a replaced native binding');j.wait(lambda:j.context(changed) is not None)
        with sqlite3.connect(j.root/'state.sqlite') as db:db.execute("UPDATE sessions SET codex_thread_id='replacement-thread' WHERE thread_id=7")
        held.unlink();j.wait(lambda:j.row('SELECT status FROM incoming_updates WHERE update_id=?',(changed,))==[('undetermined',)])
        assert not j.prompt('turn/steer','Do not send to a replaced native binding')
        assert not j.row("SELECT update_id FROM incoming_updates WHERE update_id=? AND status='steering'",(changed,))
        print('PASS independent topic controls during slow capture; capture before busy dispatcher; changed binding refuses before steer journal/send')
    finally:j.close()

if __name__=='__main__':
    binary=str(pathlib.Path(sys.argv[1]).resolve())
    for run in (queue_restart,steering_uncertainty_and_isolation,independent_ingress_and_changed_binding):run(binary)
