"use strict";
// Explicit foreground owner. Speech analysis is never itself input admission.
window.ContinuousConversation=(()=>{
  function create({headers,getBinding,getQuestions=()=>undefined,getPlayback=()=>undefined,
    retain,commit,onState=()=>{},onCaption=()=>{},onInterrupt=()=>{},onSent=()=>{},onReview=()=>{},onTiming=()=>{}}){
    let active=false,starting=false,stopping=false,epoch=0,binding,source,analysis,media,abort,timer,unsubscribe;
    let current,ring=[],bytes=0,lastEnergy=0,verdict,floor=false,blockedAt=Infinity,order=0,dispatching=false;
    const slots=new Map();
    function state(message){const pending=[...slots.values()].some(s=>s.review);onState({active,starting,floor,pending,working:!active&&slots.size>0&&!pending,message});}
    function stopTracks(){for(const track of media?.getTracks()||[])track.stop();media=undefined;}
    function closeGraph(){unsubscribe?.();unsubscribe=undefined;source?.close();source=undefined;}
    function review(){onReview([...slots.values()].filter(s=>s.original).map(s=>s.original));}
    function hold(message='Voice mode ended. Review your recordings before sending.'){
      if(stopping){state(message);review();return;}stopping=true;active=false;starting=false;floor=false;clearTimeout(timer);abort?.abort();analysis?.close();analysis=undefined;
      const slot=current;
      if(slot?.recorder?.state==='recording'){slot.review=true;onTiming(slot.id,'ended');slot.recorder.stop();}
      else if(!slot)closeGraph();
      stopTracks();state(message);review();
    }
    function prefix(start,end){
      if(end<start||end-start>256000)throw Error('Capture continuity exceeded its bound');
      const parts=[];let next=start;
      for(const part of ring){
        if(part.end<=next)continue;if(part.start>next)throw Error('Capture gap');
        const stop=Math.min(end,part.end);if(stop>next){parts.push(part.pcm.slice(next-part.start,stop-part.start));next=stop;}
        if(next===end)break;
      }
      if(next!==end)throw Error('Capture prefix unavailable');return parts;
    }
    function consider(){
      const slot=current,a=verdict;
      if(!active||stopping||!slot?.recorder||slot.recorder.state!=='recording'||!slot.words||!slot.speech||!a?.endpoint)return;
      const end=a.endpoint;
      if(end.generation!==slot.generation||a.speaking!==false||end.audio_bytes<=slot.start
         ||end.audio_bytes>bytes||lastEnergy>end.audio_bytes)return;
      // Captions can arrive after the analyzer's exact endpoint. Advancing
      // silence is not renewed speech; the acoustic fence above still owns it.
      // Keep the next recorder's prefix within the existing eight-second ring.
      if(bytes-end.audio_bytes>256000){hold('Voice confirmation arrived too late. Review your recording before sending.');return;}
      if(!analysis?.boundary(end)){hold('Voice analysis lost its turn boundary. Your recordings are kept.');return;}
      slot.send=true;floor=false;verdict=undefined;onTiming(slot.id,'ended');slot.recorder.stop();if(!stopping)state('Finishing your phrase');
    }
    function activity(a){
      if(!active||stopping||binding!==getBinding()){if(active)hold('Conversation changed. Your recordings are kept.');return;}
      const slot=current;
      if(slot&&a.onset?.audio_bytes>slot.start){
        slot.generation=a.onset.generation;
        if(!slot.speech){slot.scopeFrozen=true;slot.speech=true;slot.playback=getPlayback();floor=true;onInterrupt();state('Listening');}
      }
      if(a.endpoint)verdict=a;else if(verdict&&a.generation!==verdict.generation)verdict=undefined;
      consider();
    }
    async function finalized(slot){
      if(stopping&&current===slot){current=undefined;closeGraph();}
      if(!slot.blob){slots.delete(slot.id);if(!stopping)hold('No recording was confirmed. Nothing was sent.');return;}
      const r=slot.recorder;onTiming(slot.id,'ready');
      if(r.preparationTiming)onTiming(slot.id,'prepared',r.preparationTiming);
      const original={id:slot.id,binding,capture_started:slot.started,blob:slot.blob,mime:'audio/wav',preparedText:r.preparedText,
        captureStop:r.captureStop,captureQuestions:slot.questions,playback:slot.playback};
      slot.original=original;
      // A failed stream or a view exit does not grant automatic file-ASR/input.
      const automatic=slot.send&&!slot.review&&!original.captureStop&&!!original.preparedText;
      slot.retaining=true;
      try{slot.original=await retain(original);if(slot.original?.id!==original.id||slot.original?.binding!==original.binding||!slot.original?.blob||!/^([a-f0-9]{64})$/.test(slot.original.audio_sha256))throw Error('Original retention changed identity');}
      catch{slot.review=true;blockedAt=Math.min(blockedAt,slot.order);hold('Local recovery unavailable. Keep this page open and save your recordings.');review();return;}
      finally{slot.retaining=false;}
      if(!automatic){slot.review=true;blockedAt=Math.min(blockedAt,slot.order);hold('Your recording needs review. Nothing was resent.');review();return;}
      slot.retained=true;slot.automatic=true;void drain();
    }
    async function drain(){
      if(dispatching)return;dispatching=true;
      try{
        while(slots.size){
          const slot=[...slots.values()].sort((a,b)=>a.order-b.order)[0];
          if(!slot.retained||!slot.automatic||slot.submitted)return;
          if(slot.order>=blockedAt||binding!==getBinding()){slot.review=true;blockedAt=Math.min(blockedAt,slot.order);hold('Earlier recordings need review. Your originals are kept.');review();return;}
          slot.submitted=true;
          try{
            const result=await commit(slot.original);
            if(!result?.confirmed||result.value?.id!==slot.id||result.value?.voice?.audio_sha256!==slot.original.audio_sha256){slot.review=true;blockedAt=Math.min(blockedAt,slot.order);hold('Recording confirmation is missing. Check its status before retrying.');review();return;}
            slots.delete(slot.id);onSent(result.value);state(active?'Listening':slots.size?'Recordings need review':'Voice session ended');
          }catch{slot.review=true;blockedAt=Math.min(blockedAt,slot.order);hold('Recording unconfirmed. Your originals are kept.');review();return;}
        }
      }finally{dispatching=false;}
    }

    async function arm(start,owned){
      if(!active||stopping||owned!==epoch)return;
      if(slots.size>=2){hold('Two recordings are still pending. Review them before continuing.');return;}
      const questions=getQuestions();
      const slot={id:window.crypto.randomUUID(),order:order++,started:Date.now(),start,questions:questions===undefined?undefined:JSON.parse(JSON.stringify(questions)),speech:false,words:false,send:false};
      slots.set(slot.id,slot);current=slot;verdict=undefined;onTiming(slot.id,'requested');onCaption('');
      try{
        const recorder=await window.StreamingVoice.create(undefined,{headers,binding,id:slot.id,captureSource:source,
          initialPCM:end=>prefix(start,end),
          onPartial:text=>{if(current===slot&&active){slot.words=!!text.trim();if(slot.words)onTiming(slot.id,'partial');onCaption(text);consider();}},
          onFlushed:end=>{
            slot.end=end;if(current===slot)current=undefined;
            if(stopping)closeGraph();else void arm(end,owned);
          }});
        slot.recorder=recorder;
        recorder.addEventListener('dataavailable',event=>{slot.blob=event.data;});
        recorder.addEventListener('stop',()=>{void finalized(slot);});
        recorder.addEventListener('error',()=>hold('Voice capture failed. Your recordings are kept.'));
        if(owned!==epoch||stopping){slot.review=true;recorder.discard();slots.delete(slot.id);if(owned===epoch)closeGraph();return;}
        recorder.start();onTiming(slot.id,'started');state('Listening');consider();
      }catch{slots.delete(slot.id);if(owned!==epoch)return;if(current===slot)current=undefined;hold('Voice capture could not continue. Your earlier recordings are kept.');closeGraph();}
    }
    async function start(){
      if(active||starting||slots.size||document.hidden||!getBinding())return false;
      const owned=++epoch;binding=getBinding();starting=true;stopping=false;bytes=lastEnergy=0;ring=[];floor=false;blockedAt=Infinity;order=0;dispatching=false;
      abort=new AbortController();const ownAbort=abort;timer=setTimeout(()=>{if(owned===epoch)hold('Voice session reached five minutes. Your recordings are kept.');},300000);state('Preparing voice');
      try{
        const warmed=await window.StreamingVoice.openActivity({headers,binding,id:window.crypto.randomUUID(),signal:ownAbort.signal,
          onActivity:a=>{if(owned===epoch)activity(a);},onUnavailable:()=>{if(owned===epoch)hold('Voice analysis unavailable. Your recordings are kept.');}});
        if(stopping||owned!==epoch){warmed.close();return false;}analysis=warmed;
        const value=await window.navigator.mediaDevices.getUserMedia({audio:{channelCount:1,echoCancellation:true,noiseSuppression:true,autoGainControl:true},video:false});
        if(stopping||owned!==epoch||document.hidden||binding!==getBinding()){for(const t of value.getTracks())t.stop();if(owned===epoch)hold();return false;}
        const capture=await window.StreamingVoice.openCapture(value);
        if(stopping||owned!==epoch){capture.close();for(const t of value.getTracks())t.stop();return false;}media=value;source=capture;
        unsubscribe=source.subscribe(value=>{
          if(owned!==epoch)return;
          if(value.captureError){hold('Voice capture ended. Your recordings are kept.');return;}
          if(Number.isSafeInteger(value.activityBytes)&&Number.isSafeInteger(value.totalBytes)){
            lastEnergy=value.activityBytes;
            // Arming ASR is preparation, not a human utterance. Snapshot only
            // at its first local acoustic marker; never widen it on later Send.
            if(current&&!current.scopeFrozen&&lastEnergy>current.start){
              const questions=getQuestions();current.questions=questions===undefined?undefined:JSON.parse(JSON.stringify(questions));current.scopeFrozen=true;
            }
          }
          if(!value.pcm||!active)return;
          const end=value.totalBytes;
          if(!Number.isSafeInteger(end)||end!==bytes+value.pcm.byteLength){hold('Capture continuity was interrupted. Your recordings are kept.');return;}
          ring.push({start:bytes,end,pcm:value.pcm});bytes=end;
          while(ring.length&&ring[0].end<=bytes-256000)ring.shift();
          if(!analysis.feed(value.pcm))hold('Voice analysis stopped. Your recordings are kept.');
        });
        source.observe();active=true;starting=false;await arm(0,owned);return active;
      }catch{if(owned!==epoch)return false;hold('Voice could not start. You can keep using the ordinary conversation.');closeGraph();stopTracks();return false;}
    }
    document.addEventListener('visibilitychange',()=>{if(document.hidden)hold();});
    window.addEventListener('pagehide',()=>hold());
    return {start,hold,get active(){return active||starting;},get floor(){return floor;},
      get originals(){return [...slots.values()].filter(s=>s.original).map(s=>s.original);},
      release(id){const slot=slots.get(id);if(slot?.original&&slot.review&&!slot.retaining)slots.delete(id);}
    };
  }
  return {create};
})();
