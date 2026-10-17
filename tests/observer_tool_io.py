#!/usr/bin/env python3
"""Native integrating dialogue reads observer evidence through dynamic tools.
All model, Telegram, native and user boundaries are synthetic.
"""
import json
import pathlib
import sys
from observer_io import Observer


class Dialogue(Observer):
    def __init__(self, binary):
        self.tool_journey = None
        self.tool_context = None
        self.tool_turns = 0
        self.tool_outputs = []
        self.tool_declarations = []
        super().__init__(binary)

    def tool_request(self, send, ident, mode, selectors, *, namespace=None):
        thread,turn = self.tool_context
        params = {'threadId':thread,'turnId':turn,'callId':ident,'tool':'interaction_candidates',
                  'namespace':namespace,'arguments':{'mode':mode,'selectors':selectors}}
        send({'id':'foreign-'+ident,'method':'item/tool/call','params':{**params,'threadId':'foreign'}})
        send({'id':ident,'method':'item/tool/call','params':params})
        send({'id':ident,'method':'item/tool/call','params':params})

    def handle_extra_rpc(self, request, send):
        method=request.get('method'); p=request.get('params',{}); ident=request.get('id')
        if method=='thread/start' and not p.get('ephemeral'):
            self.tool_declarations.append(p.get('dynamicTools',[]))
        if self.tool_journey and method=='turn/start' and not p['threadId'].startswith('summary-'):
            self.tool_turns += 1
            thread=p['threadId'];turn='observer-tool-'+str(self.tool_turns)
            self.tool_context=(thread,turn)
            send({'id':ident,'result':{'turn':{'id':turn}}})
            send({'method':'turn/started','params':{'threadId':thread,'turn':{'id':turn,'status':'inProgress'}}})
            if self.tool_journey=='denied':self.tool_request(send,'denied','inventory',[])
            else:self.tool_request(send,'inventory','inventory',[])
            return True
        if method is None and ident in ('inventory','refresh','invalid','namespace','denied','after-resolved'):
            self.tool_outputs.append(request)
            result=request['result']
            if ident=='inventory':
                assert result['success'] is True
                inventory=json.loads(result['contentItems'][0]['text'])
                assert {c['label'] for c in inventory['candidates']} == {'Alpha','Beta'}
                self.tool_request(send,'refresh','refresh',['Alpha'])
            elif ident=='refresh':
                assert self.tool_journey!='resolved', 'A resolved native tool request received a late response'
                assert result['success'] is True
                assert 'Derived observer card for #Alpha' in result['contentItems'][0]['text']
                self.tool_request(send,'invalid','refresh',['Unknown'])
            elif ident=='invalid':
                assert result['success'] is False
                self.tool_request(send,'namespace','inventory',[],namespace='unrelated')
            else:
                assert result['success'] is (ident=='after-resolved')
                thread,turn=self.tool_context
                send({'method':'item/completed','params':{'threadId':thread,'turnId':turn,
                    'item':{'id':'tool-answer','type':'agentMessage','phase':'final_answer',
                            'text':'Integrator inspected attributed observer evidence.'}}})
                send({'method':'turn/completed','params':{'threadId':thread,'turn':{'id':turn,'status':'completed'}}})
                self.tool_journey=None
            return True
        return super().handle_extra_rpc(request,send)


def integrating_tool(binary):
    j=Dialogue(binary)
    try:
        j.prepare()
        sources=j.row('SELECT thread_id,codex_thread_id FROM sessions WHERE thread_id IN (7,8) ORDER BY thread_id')
        before=j.row('SELECT count(*) FROM turns')[0][0]
        j.tool_journey='inspect'
        j.admit('What are our other conversations doing?',topic=1)
        j.wait(lambda:len(j.tool_outputs)==4)
        j.wait(lambda:j.has('Integrator inspected attributed observer evidence.',1))
        j.idle()
        assert j.summaries==1
        assert j.tool_declarations[-1][0]['name']=='interaction_candidates'
        assert j.row('SELECT count(*) FROM turns') == [(before+1,)]
        assert j.row('SELECT thread_id,codex_thread_id FROM sessions WHERE thread_id IN (7,8) ORDER BY thread_id')==sources
        assert not [r for r in j.rpc if str(r.get('id','')).startswith('foreign-') and not r.get('method')]
        # A resumed entry keeps native tool metadata; the bridge never replaces
        # the binding or tries unsupported dynamicTools on thread/resume.
        binding=j.row('SELECT codex_thread_id FROM sessions WHERE thread_id=1')
        j.stop();j.start()
        j.tool_outputs.clear();j.tool_journey='inspect'
        j.admit('Inspect the same sources again.',topic=1)
        j.wait(lambda:len(j.tool_outputs)==4)
        j.idle()
        assert j.summaries==1 and j.row('SELECT codex_thread_id FROM sessions WHERE thread_id=1')==binding
        assert all('dynamicTools' not in r['params'] for r in j.rpc if r.get('method')=='thread/resume')
        # A persisted or forged call from a worker cannot gain the entry's view.
        j.tool_outputs.clear();j.tool_journey='denied'
        j.admit('Synthetic worker attempts an integrating tool.',topic=7)
        j.wait(lambda:len(j.tool_outputs)==1)
        j.idle()
        assert j.tool_outputs[0]['result']['success'] is False and j.summaries==1
        print('PASS integrating native tool declaration, async inventory/refresh, duplicate and foreign request isolation, denied arguments/namespace/worker calls, restart binding preservation and no source worker execution')
    finally:
        j.close()


def resolved_tool_is_retired(binary):
    import time
    j=Dialogue(binary)
    try:
        j.prepare()
        j.block_summary=True
        j.tool_journey='resolved'
        j.admit('Inspect an observer source while this dialogue remains active.',topic=1)
        j.wait(j.summary_held.is_set)
        thread,turn=j.tool_context
        j.broadcast(thread,'serverRequest/resolved',{'requestId':'refresh'})
        j.broadcast(thread,'item/completed',{'turnId':turn,'item':{'id':'resolved-marker','type':'agentMessage','phase':'commentary','text':'Native request resolved; dialogue remains active.'}})
        j.wait(lambda:j.row("SELECT count(*) FROM interaction_native_messages WHERE item_id='resolved-marker'") == [(1,)])
        j.release_summary.set()
        j.wait(lambda:j.card() is not None)
        # Give the released deferred result an observation window while the
        # original native turn remains active, then verify a new tool still works.
        time.sleep(.2)
        assert [o['id'] for o in j.tool_outputs] == ['inventory'] and not j.errors
        send=next(send for native,send in list(j.clients.values()) if native==thread)
        j.tool_request(send,'after-resolved','inventory',[])
        j.wait(lambda:len(j.tool_outputs)==2)
        j.idle()
        assert j.summaries==1
        print('PASS native resolution retires held observer result while its native turn remains responsive and subsequent tools work')
    finally:
        j.release_summary.set()
        j.close()


if __name__=='__main__':
    binary=str(pathlib.Path(sys.argv[1]).resolve())
    integrating_tool(binary)
    resolved_tool_is_retired(binary)
