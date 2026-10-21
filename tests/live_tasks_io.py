#!/usr/bin/env python3
"""Live task retirement over the real controller and durable ledger; synthetic models."""
import json
import re
import subprocess
from pathlib import Path
import sys
from warm_controller_io import Warm


class NotifiedWarm(Warm):
    def start(self):
        if self.routed:
            text = self.config.read_text()
            if 'telegram_projection=true' not in text:
                self.config.write_text(text.replace('[interaction]\n', '[interaction]\ntelegram_projection=true\n'))
        return super().start()


def qualify(binary):
    j=NotifiedWarm(binary)
    try:
        j.prepare(seed_entry=True)
        j.allow_native_launch=True
        j.admit('Coordinate the selected existing work.',topic=1)
        j.wait(lambda:'native-topic-9' in j.runs)
        entry=j.access(1)
        caps=j.cli(entry,'capabilities')
        assert all(name in caps['operations'] for name in ('human_tasks','human_task','withdraw_task'))
        original=j.cli(entry,'inputs')['inputs'][-1]['input_ref']
        j.declare_task=True
        j.cli(entry,'dispatch',{'input_ref':original,'destinations':[{'selector':'Alpha','relation':'new','requests':[]}]})
        j.wait(lambda:j.instructions(7) and j.instructions(7)[0][1]=='accepted')
        j.answer(7,'The selected work needs a human observation, retained as a task.')
        j.wait(lambda:j.row("SELECT count(*) FROM human_interactions WHERE kind='task' AND state='pending'")==[(1,)])
        j.wait(lambda:j.row("SELECT count(*) FROM interaction_deliveries WHERE phase!='delivered'")==[(0,)])
        selected=j.cli(entry,'human_tasks')['items'][0]
        detail=j.cli(entry,'human_task',{'id':selected['id']})
        assert detail['action']=='Synthetic required observation; no real human is contacted.'
        args={'id':selected['id'],'revision':selected['revision'],'reason':'The later conversation made this observation unnecessary; no human completion is inferred.','links':[]}
        j.cli(entry,'withdraw_task',{**args,'revision':'0'*64},ok=False)
        assert j.row("SELECT state FROM human_interactions WHERE id=?",(selected['id'],))==[('pending',)]
        # The issuer is doing new direct work while the coordinator reassesses its earlier routed task.
        j.admit('Continue ordinary direct work without routing this input.',topic=7)
        j.wait(lambda:len(j.packets['native-topic-7'])>=2)
        worker=j.access(7)
        own=j.cli(worker,'human_tasks')['items']
        assert any(item['id']==selected['id'] for item in own)
        before_packets=len(j.packets['native-topic-7'])
        result=j.cli(entry,'withdraw_task',args)
        assert result['new_withdrawal'] and result['human_completion'] is False
        assert not j.cli(entry,'withdraw_task',args)['new_withdrawal']
        j.wait(lambda:j.row("SELECT phase FROM topic_publications WHERE kind='task_control' AND json_extract(source,'$[0]')=?",(selected['id'],))==[('sent',)])
        notices=[m for m,_ in j.messages if 'Integrator action' in m['text'] and 'request withdrawn' in m['text']]
        assert len(notices)==1 and notices[0].get('message_thread_id')==7
        assert selected['id'] not in notices[0]['text'] and args['reason'] not in notices[0]['text']
        assert len(j.packets['native-topic-7'])==before_packets, 'A visible retirement never becomes more worker input'
        reply=subprocess.run([j.binary,'--control',str(j.config),'topics','notices','native-topic-7'],capture_output=True,text=True,check=True,timeout=20)
        feed=json.loads(reply.stdout)['result']
        assert feed['available'] and 'request withdrawn' in feed['items'][-1]['text']
        assert j.cli(entry,'human_tasks')['items']==[]
        row=j.row("SELECT state,payload_json FROM human_interactions WHERE id=?",(selected['id'],))[0]
        assert row[0]=='cancelled'
        payload=json.loads(row[1]);assert payload['action']==detail['action']
        assert payload['source']['agentWithdrawal']['actor']['session']=={'chat_id':100,'thread_id':1}
        assert j.row("SELECT count(*) FROM audit_log WHERE action='human_task_withdrawn_by_agent'")==[(1,)]
        # Another worker has no authority to read or retire this task.
        j.admit('Ordinary Beta work.',topic=8)
        j.wait(lambda:'native-topic-8' in j.runs)
        other=j.access(8)
        assert j.cli(other,'human_tasks')['items']==[]
        j.cli(other,'human_task',{'id':selected['id']},ok=False)
        j.cli(other,'withdraw_task',args,ok=False)
        # A task declared by a later direct turn is not silently imported into the integrator.
        j.broadcast('native-topic-7','turn/completed',{'turn':{'id':j.runs['native-topic-7'],'status':'completed'}})
        j.wait(lambda:j.row("SELECT count(*) FROM turns t JOIN sessions s ON s.id=t.session_id WHERE s.thread_id=7 AND t.status='running'")==[(0,)])
        before_direct=len(j.packets['native-topic-7'])
        j.admit('Direct work with its own observation.',topic=7)
        j.wait(lambda:len(j.packets['native-topic-7'])>before_direct)
        j.wait(lambda:j.row("SELECT count(*) FROM turns t JOIN sessions s ON s.id=t.session_id WHERE s.thread_id=7 AND t.status='running'")==[(1,)])
        context=j.packets['native-topic-7'][-1]['additionalContext']['telecodex.runtime']['value']
        manifest=Path(re.search(r'explicitly declare it in (\S+human-requests\.json)',context).group(1))
        manifest.write_text(json.dumps({'schema':'telecodex.human-requests/v1','requests':[{'id':'direct-observation','title':'Direct observation','action':'Only this direct topic needs this action.','links':[]}]}))
        j.broadcast('native-topic-7','turn/completed',{'turn':{'id':j.runs['native-topic-7'],'status':'completed'}})
        j.wait(lambda:j.row("SELECT count(*) FROM human_interactions WHERE kind='task' AND state='pending'")==[(1,)])
        assert j.cli(entry,'human_tasks')['items']==[]
        direct=j.row("SELECT id FROM human_interactions WHERE kind='task' AND state='pending'")[0][0]
        j.cli(entry,'human_task',{'id':direct},ok=False)
        for topic in (8,9):
            thread=f'native-topic-{topic}'
            j.broadcast(thread,'turn/completed',{'turn':{'id':j.runs[thread],'status':'completed'}})
        j.idle()
        print('PASS live scoped task retirement, exact revision/retry, agent attribution, active issuer, worker isolation and direct-topic independence; synthetic models')
    finally:
        j.close()

if __name__=='__main__':
    qualify(str(Path(sys.argv[1]).resolve()))
