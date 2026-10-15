#!/usr/bin/env python3
"""Optional qualification against the existing Nexus backend (no native model).
Run using Nexus's existing test venv, passing its checkout as the first argument.
All recordings, human accounts and receipts here are synthetic.
"""
import json
from pathlib import Path
import subprocess
import sys
import tempfile

workspace=Path(sys.argv[1]).resolve()
sys.path.insert(0,str(workspace))
from backend.tests.test_service import service,anchor,post
from backend.store import Store

operator=Path(__file__).resolve().parents[1]/'scripts/nexus_operator.py'
with tempfile.TemporaryDirectory(prefix='telecodex-nexus-operator-') as directory:
    app,client,state,bundle,_=service.__wrapped__(Path(directory))
    def call(operation,body,ok=True):
        result=subprocess.run([sys.executable,'-B',str(operator),'--workspace',str(workspace),'--state',str(state),operation],input=json.dumps(body),capture_output=True,text=True)
        assert (result.returncode==0)==ok,(operation,result.stderr)
        return json.loads(result.stdout) if ok else result
    target={'listener_id':'listener','bot_id':123,'human_user_id':100,'chat_id':100,'topic_id':7,'native_thread_id':'existing-native'}
    config={'table_id':'table','author_ref':'human:nico','expected_revision':0,'supported_requests':['contribution','iteration','peer'],'label':'Nexus','responder_ref':'daimon:test','target':target}
    call('configure',config,False)  # No approved Telegram membership.
    selector=app.state.store.register_launch('table')['start_param']
    _,pending=app.state.store.telegram_access(selector,123,100)
    app.state.store.approve_access(pending['request_id'],'table','human:nico','Nico')
    binding=call('configure',config)
    assert call('binding',{'table_id':'table','author_ref':'human:nico'})['revision']==1
    # Actual backend: frozen A survives B; explicit earlier composition stays A.
    from backend.research_projection import project
    from backend.tests.test_service import ORIGIN
    graph=project({'provider':'fixture','context_id':'context','version':'v1','table_id':'table','title':'Research','scope_note':'Two sections','author_ref':'test','documents':[{'observed_at':'2026-10-15T10:00:00Z','document':{'doc_id':'report.md','kind':'source','title':'Report','body':'# A\n\nOne.\n\n# B\n\nTwo.\n'}}]})
    app.state.contexts.install(graph)
    a,b=[n['node_id'] for n in graph['nodes'] if n['title'] in {'A','B'}]
    view={'context_id':'context','version':'v1','focus':a,'selection':{'kind':'node','object_id':a},'path':[],'expansions':{},'renderers':{},'drafts':[]}
    command={'request_id':'visit-a','expected_revision':0,'input_enabled':True,'perspective':view}
    assert client.put('/api/tables/table/visits/phone',json=command,headers={'Origin':ORIGIN}).status_code==200
    incoming={'input_id':'telegram:123:42','native':target}
    frozen=call('capture-input',incoming);assert frozen['status']=='captured' and frozen['perspective']['selection']['object_id']==a
    import time
    ingress_cutoff=time.time()
    moved={**command,'request_id':'visit-b','expected_revision':1,'perspective':{**view,'focus':b,'selection':{'kind':'node','object_id':b}}}
    assert client.put('/api/tables/table/visits/phone',json=moved,headers={'Origin':ORIGIN}).status_code==200
    assert call('capture-input',{**incoming,'input_id':'capture-race','ingress_at':ingress_cutoff})['status']=='view_changed_during_ingress'
    assert call('capture-input',incoming)==frozen
    assert call('current-visits',target)['visits'][0]['perspective']['selection']['object_id']==b
    assert call('capture-input',{**incoming,'input_id':'earlier-composition','visit_id':'phone','view_revision':1})['perspective']['selection']['object_id']==a
    assert call('capture-input',{**incoming,'input_id':'stranger','native':{**target,'human_user_id':999}})['status']=='missing'
    assert 'drafts' not in json.dumps(frozen) and 'native_thread_id' not in json.dumps(frozen)
    note={'contribution_id':'own-question','kind':'question','text':'Selected human question','anchors':[anchor()]}
    assert post(client,'/api/tables/table/contributions',note).status_code==200
    command={'request_id':'request','binding_id':binding['binding_id'],'binding_revision':1,'contribution_id':'own-question'}
    assert post(client,'/api/tables/table/conversation-requests',command).status_code==200
    assert call('pending',{'listener_id':'listener'})==['request']
    selection={'listener_id':'listener','request_id':'request'}
    before=call('inspect',selection);assert before['dispatch'] is False
    dispatch=call('dispatch',selection);assert dispatch['dispatch'] is True and dispatch['target']==target
    assert dispatch['request']['contribution']==before['request']['contribution']
    passage=dispatch['request']['selected_evidence'][0];assert passage['source_id']=='source' and passage['transcript_version']=='asr-v1'
    assert passage['segments'][0]['text']==bundle['sources'][0]['segments'][0]['text']
    assert call('dispatch',selection)['dispatch'] is False
    receipt={'request_id':'request','dispatch_id':dispatch['dispatch_id'],'native_thread_id':'existing-native','native_turn_id':'synthetic-observed-turn','status':'accepted'}
    assert call('receipt',receipt)['status']=='accepted'
    complete={**receipt,'status':'completed','reply':'Synthetic native receipt; not a live model result.'}
    assert call('receipt',complete)['reply']==complete['reply']
    assert call('receipt',receipt)['status']=='completed'
    # The same real backend dispatches a reviewed iteration without requiring or
    # fabricating recording evidence. Native transport receipts never settle work.
    plan={'iteration_id':'research-iteration','title':'Clarify a selected relation','scope':'Selected context only',
          'tasks':[{'task_id':'clarify','title':'Check meaning','instruction':'Clarify the retained source',
                    'intended_outcome':'An evidenced clarification','acceptance':'Observe the source revision and result',
                    'targets':[{'context_id':'context','version':'v1','kind':'node','object_id':a}],
                    'executor':'daimon','operation':'context.iterate','authority_basis':'Fixture work; no external effects'}]}
    app.state.work.create('table','human:nico',plan)
    app.state.work.advance('table','research-iteration','human:nico',{'request_id':'activate-iteration','expected_revision':0})
    selected={'request_id':'iterate','binding_id':binding['binding_id'],'binding_revision':1,
              'iteration_id':'research-iteration','iteration_revision':1}
    assert post(client,'/api/tables/table/iteration-requests',selected).status_code==200
    dispatch=call('dispatch',{'listener_id':'listener','request_id':'iterate'})
    assert dispatch['request']['iteration']['spec']['tasks'][0]['targets']==plan['tasks'][0]['targets']
    assert 'selected_evidence' not in dispatch['request'] and 'contribution' not in dispatch['request']
    receipt={'request_id':'iterate','dispatch_id':dispatch['dispatch_id'],'native_thread_id':'existing-native','native_turn_id':'fixture-iteration-turn','status':'accepted'}
    call('receipt',receipt)
    call('receipt',{**receipt,'status':'completed','reply':'Fixture reply, no effect.'})
    assert app.state.work.read('table','research-iteration')['state']=='active'
    # Same operator, separate native and signed-peer response lanes.
    app.state.peers.configure(dict(route_id='peer',table_id='table',expected_revision=0,being_ref='dm:being:v1:fixture_peer',label='Fixture peer',profile_id='fixture-profile'))
    shared=dict(request_id='peer-review',route_id='peer',route_revision=1,target=dict(context_id='context',version='v1',kind='relation',object_id=graph['relations'][0]['relation_id']),shared_text='Explicit fixture disclosure only.',disclosure='whole-tribe')
    assert post(client,'/api/tables/table/peer-requests',shared).status_code==200
    command=dict(binding_id=binding['binding_id'],binding_revision=1,peer_request_id='peer-review')
    native=post(client,'/api/tables/table/peer-conversation-requests',command).json()
    dispatch=call('dispatch',dict(listener_id='listener',request_id=native['request_id']))
    assert dispatch['request']['peer_request']['target']==shared['target']
    assert 'profile_id' not in json.dumps(dispatch) and 'selected_evidence' not in dispatch['request']
    receipt=dict(request_id=native['request_id'],dispatch_id=dispatch['dispatch_id'],native_thread_id='existing-native',native_turn_id='fixture-peer-turn',status='accepted')
    call('receipt',receipt)
    call('receipt',{**receipt,'status':'completed','reply':'Private fixture response; no peer effect.'})
    assert app.state.peers.all('table')[0]['responses']==[]
    assert app.state.peers.all('table')[0]['status']=='queued'
    call('configure',{**config,'expected_revision':1,'active':False})
    assert call('pending',{'listener_id':'listener'})==[]
    # The API may return selected old versions; permission/symlink checks happen
    # before backend import or a constructor that could create fresh state.
    db=state/'nexus.db';original=state/'original.db';db.rename(original);db.symlink_to(original)
    assert call('binding',{'table_id':'table','author_ref':'human:nico'},False).stdout==''
    db.unlink();original.rename(db);db.chmod(0o644)
    assert call('binding',{'table_id':'table','author_ref':'human:nico'},False).stdout==''
    db.chmod(0o600)
print('PASS actual Nexus backend HTTP/owner-local contract, selected retained evidence, one dispatch, exact receipt retry, private-state refusal; native receipts synthetic')
