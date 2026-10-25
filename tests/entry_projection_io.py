#!/usr/bin/env python3
"""One native integrating conversation, web and Telegram surfaces; synthetic peers."""
from pathlib import Path
import sys
from topic_projection_io import Projected


def qualify(binary):
    j=Projected(str(Path(binary).resolve()))
    try:
        j.start()
        for topic in (7,8,1):
            j.admit('Seed context',topic=topic)
            j.wait(lambda:j.has('Initial goal turn delivered.',topic));j.idle()
        j.stop()
        j.config.write_text(j.config.read_text().replace('[interaction]\n',
            '[interaction]\ntelegram_ingress=true\ntelegram_projection=true\ntelegram_entry_projection=true\n')+j.target_config)
        j.start();j.wait_control_socket(j.socket);j.routing=True
        binding=j.read('presentation','list','0','0')['result']['binding'];j.messages.clear()
        def copies(text,topic=1):
            return sum(m.get('message_thread_id')==topic and text in m['text'] for m,_ in j.messages)
        # App-native entry speech is mirrored, not re-admitted as another input.
        app,_=j.submit(binding,'An app conversation about two topics')
        j.wait(lambda:len(j.packets)==1)
        j.wait(lambda:copies('An app conversation about two topics')==1)
        thread,turn=j.web_thread,j.web_turn
        for _ in range(2):
            j.broadcast(thread,'item/completed',{'turnId':turn,'item':{'id':'entry-progress','type':'agentMessage','phase':'commentary','text':'I am checking both topics.'}})
        j.wait(lambda:copies('I am checking both topics.')==1)
        j.broadcast(thread,'item/completed',{'turnId':turn,'item':{'id':'entry-final','type':'agentMessage','phase':'final_answer','text':'Both topics remain available.'}})
        j.broadcast(thread,'turn/completed',{'turn':{'id':turn,'status':'completed'}})
        j.wait(lambda:copies('Both topics remain available.')==1);j.idle()
        assert j.retained('Both topics remain available.')
        assert len(j.packets)==1
        # An explicit app route shares the same entry echo, worker progress and result.
        routed,_=j.submit(binding,'#Alpha App-routed work')
        j.wait(lambda:j.instructions(7) and j.instructions(7)[0][1]=='accepted')
        j.wait(lambda:copies('#Alpha App-routed work')==1)
        native='native-topic-7';physical=j.workers[native]
        for _ in range(2):
            j.broadcast(native,'item/completed',{'turnId':physical,'item':{'id':'worker-progress','type':'agentMessage','phase':'commentary','text':'Scoped routed progress.'}})
        j.wait(lambda:copies('Scoped routed progress.')==1)
        j.answer(7,'Canonical routed result.',duplicate=True)
        j.wait(lambda:copies('Canonical routed result.')==1)
        j.wait(lambda:j.primary('Canonical routed result.'));j.idle()
        before=len(j.instructions(7));j.submit(binding,routed['text'],ident=routed['id'])
        assert len(j.instructions(7))==before
        assert copies('#Alpha App-routed work')==1 and copies('Canonical routed result.')==1
        assert not any('daimon-result' in m['text'] for m,_ in j.messages)
        # Telegram is another ingress on this entry; work topics stay direct.
        j.admit('#Beta Telegram-routed work',topic=1)
        j.wait(lambda:j.instructions(8) and j.instructions(8)[0][1]=='accepted')
        j.answer(8,'Telegram result in the shared conversation.',duplicate=True)
        j.wait(lambda:copies('Telegram result in the shared conversation.')==1)
        j.wait(lambda:j.primary('Telegram result in the shared conversation.'));j.idle()
        j.admit('Direct work-topic message stays direct.',topic=7)
        j.wait(lambda:len(j.direct_packets)==1)
        j.answer(7,'Direct-only result.',direct=True,duplicate=True)
        j.wait(lambda:j.sent('Direct-only result.',7)==1);j.idle()
        assert copies('Direct-only result.')==0
        assert not j.retained('Direct-only result.')
        # A restart observes prior publications; it does not backfill or replay them.
        count=copies('Canonical routed result.');j.stop();j.start();j.wait_control_socket(j.socket)
        assert copies('Canonical routed result.')==count
        assert len(j.instructions(7))==before
        print('Entry projection: app/Telegram originals, integrator and worker progress/results once, native context, direct-topic isolation and restart without replay; zero models.')
    finally:j.close()


if __name__=='__main__':qualify(sys.argv[1])
