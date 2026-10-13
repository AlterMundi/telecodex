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
    config={'table_id':'table','author_ref':'human:nico','expected_revision':0,'label':'Nexus','responder_ref':'daimon:test','target':target}
    call('configure',config,False)  # No approved Telegram membership.
    selector=app.state.store.register_launch('table')['start_param']
    _,pending=app.state.store.telegram_access(selector,123,100)
    app.state.store.approve_access(pending['request_id'],'table','human:nico','Nico')
    binding=call('configure',config)
    assert call('binding',{'table_id':'table','author_ref':'human:nico'})['revision']==1
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
