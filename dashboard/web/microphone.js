"use strict";
window.VoiceInput = (() => {
  function mount({api, headers, getBinding, onInput, getReply=()=>undefined, pauseAudio, unauthorized, telegram}) {
    const box=document.querySelector('#conversation-voice'), button=document.querySelector('#conversation-talk');
    const discard=document.querySelector('#conversation-voice-discard'), check=document.querySelector('#conversation-voice-check');
    const state=document.querySelector('#conversation-voice-state');
    let available=false, generation=0, recorder, stream, parts=[], bytes=0, timer, pending, latestSaved, uploading=false, controller;
    let phase='idle', commit=false, readySelection;
    const supported=()=>!!(window.isSecureContext && window.MediaRecorder && typeof window.MediaRecorder.isTypeSupported==='function' && window.navigator?.mediaDevices?.getUserMedia && window.crypto?.subtle);
    function controls() {
      box.hidden=!available;
      button.textContent=phase==='permission'?'Opening microphone…':phase==='recording'?'Send voice':phase==='ready'?'Send recording':pending?'Retry this recording':'Talk';
      button.disabled=!available || !getBinding() || !supported() || uploading || phase==='permission' || phase==='finalizing';
      button.setAttribute('aria-pressed',String(phase==='recording'));
      discard.hidden=!['permission','recording','ready','finalizing'].includes(phase);
      check.hidden=!pending; check.disabled=uploading;
    }
    function stopTracks() {if(stream)for(const track of stream.getTracks())track.stop();stream=undefined;clearTimeout(timer);}
    function cancelRecording(message='Recording discarded. Nothing was sent.') {
      generation++;commit=false;latestSaved=undefined;const old=recorder;recorder=undefined;
      if(old && old.state!=='inactive')old.stop();stopTracks();parts=[];bytes=0;phase='idle';state.textContent=message;controls();
    }
    function reset() {
      controller?.abort();controller=undefined;pending=undefined;uploading=false;
      cancelRecording('');available=false;controls();
    }
    function deactivate() {
      if(['permission','recording','ready','finalizing'].includes(phase))cancelRecording('Recording ended while this view was inactive. Nothing was sent.');
      controller?.abort();
    }
    function configure(value) {
      available=!!value;
      if(!available && ['permission','recording','ready','finalizing'].includes(phase))cancelRecording();
      if(available && !supported())state.textContent='Microphone unavailable in this browser. You can keep writing.';
      controls();
    }
    async function confirmed(value,frozen) {
      if(pending!==frozen || frozen.binding!==getBinding())return;
      if(value.id!==frozen.id || value.voice?.audio_sha256!==frozen.audio_sha256)throw Error('Voice capture does not match this recording.');
      latestSaved={id:frozen.id,binding:frozen.binding,audio_sha256:frozen.audio_sha256};
      onInput(value,frozen.focusVersion);pending=undefined;phase='idle';
      state.textContent=value.retired?'This saved recording is no longer eligible to start work.':
        value.phase==='retained_unsent'?'Recording saved; no work was submitted. You can keep writing.':
        value.phase==='preparation_unconfirmed'?'Recording saved; transcription unconfirmed. Nothing was resubmitted.':
        value.phase==='preparing'?'Recording saved; preparing its automatic transcript.':
        'Recording and automatic transcript saved. Work may continue.';
      controls();
    }
    function observe(value,message) {
      if(!latestSaved || pending || phase!=='idle' || latestSaved.binding!==getBinding()
        || latestSaved.id!==value.id || latestSaved.audio_sha256!==value.voice?.audio_sha256)return;
      state.textContent='Recording saved. '+message;
    }
    async function upload() {
      if(!pending || uploading || pending.binding!==getBinding())return;
      const frozen=pending;uploading=true;controller=new AbortController();const active=controller;
      const deadline=setTimeout(()=>active.abort(),20000);state.textContent='Saving your recording…';controls();
      try {
        const response=await fetch('api/interaction/voice',{method:'POST',credentials:'same-origin',signal:active.signal,
          headers:{...headers(),...(frozen.reply?{'X-Reply-Source':frozen.reply.source,'X-Reply-Revision':frozen.reply.revision}:{}),'Content-Type':frozen.mime,'X-Input-Binding':frozen.binding,'X-Input-Id':frozen.id,'X-Audio-Sha256':frozen.audio_sha256},body:frozen.blob});
        const value=await response.json();
        if(!response.ok){const error=new Error('Recording not confirmed');error.status=response.status;throw error;}
        await confirmed(value,frozen);
      } catch(e) {
        if(pending!==frozen)return;
        state.textContent='Recording unconfirmed. Check its status or retry this same recording. Nothing is resent automatically.';
        if(e.status===401 || e.status===403)unauthorized(e);
      } finally {clearTimeout(deadline);if(controller===active){controller=undefined;uploading=false;controls();}}
    }
    async function finish(owned,binding,mime,selection) {
      if(owned!==generation)return;
      stopTracks();recorder=undefined;
      const blob=new Blob(parts,{type:mime});parts=[];bytes=0;
      if(!commit){phase='ready';parts=[blob];readySelection={binding,selection};state.textContent='Recording limit reached. Send this recording or discard it.';controls();return;}
      phase='finalizing';controls();
      try {
        if(!blob.size || blob.size>20*1024*1024)throw Error('Recording outside limits');
        const digest=await window.crypto.subtle.digest('SHA-256',await blob.arrayBuffer());
        if(owned!==generation || binding!==getBinding())return;
        const audio_sha256=Array.from(new Uint8Array(digest),v=>v.toString(16).padStart(2,'0')).join('');
        pending={id:window.crypto.randomUUID(),binding,audio_sha256,mime,blob,reply:selection?.reference,focusVersion:selection?.version};phase='idle';await upload();
      } catch(_) {if(owned===generation)cancelRecording('Recording could not be finalized. Nothing was sent.');}
    }
    async function start() {
      if(!available || !getBinding() || !supported() || document.hidden)return;
      const owned=++generation,binding=getBinding(),selection=getReply();latestSaved=undefined;phase='permission';commit=false;parts=[];bytes=0;
      state.textContent='Allow microphone access to begin.';pauseAudio();controls();
      try {
        const media=await window.navigator.mediaDevices.getUserMedia({audio:true,video:false});
        if(owned!==generation || binding!==getBinding() || document.hidden){for(const track of media.getTracks())track.stop();return;}
        stream=media;
        const mime=['audio/webm;codecs=opus','audio/ogg;codecs=opus','audio/mp4','audio/webm'].find(value=>window.MediaRecorder.isTypeSupported(value));
        if(!mime)throw Error('No supported recording format');
        const current=new window.MediaRecorder(media,{mimeType:mime,audioBitsPerSecond:64000});recorder=current;
        current.addEventListener('dataavailable',event=>{
          if(owned!==generation || !event.data.size)return;
          bytes+=event.data.size;if(bytes>20*1024*1024){cancelRecording('Recording exceeded its size limit. Nothing was sent.');return;}
          parts.push(event.data);
        });
        current.addEventListener('error',()=>{if(owned===generation)cancelRecording('Recording failed. Nothing was sent.');});
        current.addEventListener('stop',()=>finish(owned,binding,mime,selection));
        current.start(250);phase='recording';state.textContent='Listening. Press Send voice when you finish.';controls();
        timer=setTimeout(()=>{if(owned===generation && current.state==='recording'){phase='finalizing';current.stop();stopTracks();controls();}},120000);
      } catch(_) {if(owned===generation)cancelRecording('Microphone unavailable. You can keep writing.');}
    }
    button.addEventListener('click',()=>{
      if(uploading)return;
      if(phase==='recording'){commit=true;phase='finalizing';recorder.stop();stopTracks();controls();}
      else if(phase==='ready'){commit=true;finish(generation,readySelection.binding,parts[0].type,readySelection.selection);}
      else if(pending)return upload();else return start();
    });
    discard.addEventListener('click',()=>cancelRecording());
    check.addEventListener('click',async()=>{
      if(!pending || uploading)return;const frozen=pending,owned=generation;uploading=true;controls();
      try {await confirmed(await api(`interaction/inputs/${frozen.id}`),frozen);}
      catch(e){if(pending===frozen){state.textContent='Recording capture remains unconfirmed. Nothing has been resent.';if(e.status===401 || e.status===403)unauthorized(e);}}
      finally{if(owned===generation){uploading=false;controls();}}
    });
    document.addEventListener('visibilitychange',()=>{if(document.hidden)deactivate();});
    window.addEventListener('pagehide',deactivate);telegram?.onEvent?.('deactivated',deactivate);
    controls();return {configure,reset,deactivate,observe};
  }
  return {mount};
})();
