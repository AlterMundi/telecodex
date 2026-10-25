"use strict";
window.VoiceInput = (() => {
  function mount({api, headers, getBinding, onInput, getReply=()=>undefined, getVisual=()=>undefined, onVisualSent=()=>{}, pauseAudio, onActivity=()=>{}, unauthorized, telegram}) {
    const box=document.querySelector('#conversation-voice'), button=document.querySelector('#conversation-talk'),send=document.querySelector('#conversation-send');
    const discard=document.querySelector('#conversation-voice-discard'), check=document.querySelector('#conversation-voice-check');
    const state=document.querySelector('#conversation-voice-state');
    function notice(text,attention=false){state.textContent=text;state.setAttribute('data-attention',String(attention));}
    let available=false, generation=0, recorder, stream, parts=[], bytes=0, timer, pending, latestSaved, uploading=false, controller;
    let phase='idle', commit=false, readySelection;
    const supported=()=>!!(window.isSecureContext && window.MediaRecorder && typeof window.MediaRecorder.isTypeSupported==='function' && window.navigator?.mediaDevices?.getUserMedia && window.crypto?.subtle);
    const ownsSend=()=>['permission','recording','ready','finalizing'].includes(phase) || uploading || !!pending;
    let ownedSend=false;
    function syncSend(){
      const owned=ownsSend();button.hidden=owned;
      if(send){send.type=owned?'button':'submit';send.setAttribute('aria-label',owned?'Send voice message':'Send text message');
        if(owned)send.disabled=button.disabled || !(['recording','ready'].includes(phase)||pending);else if(ownedSend)send.disabled=!getBinding();}
      ownedSend=owned;
    }
    function controls() {
      box.hidden=!available;
      button.textContent=phase==='recording'||phase==='ready'?'↑':pending?'↻':['permission','finalizing'].includes(phase)||uploading?'…':'🎙';
      button.setAttribute('aria-label',phase==='recording'?'Send voice message':phase==='ready'?'Send recording':pending?'Retry this recording':'Record a voice message');
      button.disabled=!available || !getBinding() || !supported() || uploading || phase==='permission' || phase==='finalizing';
      button.setAttribute('aria-pressed',String(phase==='recording'));
      syncSend();
      discard.hidden=!['permission','recording','ready','finalizing'].includes(phase);
      discard.textContent='×';discard.setAttribute('aria-label','Discard recording');discard.title='Discard recording';
      check.hidden=!pending; check.disabled=uploading;
      onActivity(uploading || ['permission','recording','ready','finalizing'].includes(phase) || latestSaved?.phase==='preparing', {phase,uploading,transcribing:latestSaved?.phase==='preparing'});
    }
    function stopTracks() {if(stream)for(const track of stream.getTracks())track.stop();stream=undefined;clearTimeout(timer);}
    function cancelRecording(message='Recording discarded. Nothing was sent.') {
      generation++;commit=false;latestSaved=undefined;const old=recorder;recorder=undefined;
      if(old && old.state!=='inactive')old.stop();stopTracks();parts=[];bytes=0;phase='idle';notice(message,!!message && message!=='Recording discarded. Nothing was sent.');controls();
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
      if(available && !supported())notice('Microphone unavailable in this browser. You can keep writing.',true);
      controls();
    }
    async function confirmed(value,frozen) {
      if(pending!==frozen || frozen.binding!==getBinding())return;
      if(value.id!==frozen.id || value.voice?.audio_sha256!==frozen.audio_sha256)throw Error('Voice capture does not match this recording.');
      if(!!frozen.visual!==!!value.visual||frozen.visual&&Object.keys(frozen.visual).some(k=>JSON.stringify(frozen.visual[k])!==JSON.stringify(value.visual[k])))throw Error('Visual capture does not match this recording.');
      latestSaved={id:frozen.id,binding:frozen.binding,audio_sha256:frozen.audio_sha256,phase:value.phase};
      onInput(value,frozen.focusVersion);onVisualSent(frozen.visual);pending=undefined;phase='idle';
      notice(value.phase==='answered'?'Recording and automatic transcript saved; answer recorded for the selected question.':value.retired?'This saved recording is no longer eligible to start work.':
        value.phase==='retained_unsent'?'Recording saved; no work was submitted. You can keep writing.':
        value.phase==='preparation_unconfirmed'?'Recording saved; transcription unconfirmed. Nothing was resubmitted.':
        value.phase==='preparing'?'Recording saved; preparing its automatic transcript.':
        'Recording and automatic transcript saved. Work may continue.',value.retired || ['retained_unsent','preparation_unconfirmed'].includes(value.phase));
      controls();
    }
    function observe(value,message) {
      if(!latestSaved || pending || phase!=='idle' || latestSaved.binding!==getBinding()
        || latestSaved.id!==value.id || latestSaved.audio_sha256!==value.voice?.audio_sha256)return;
      latestSaved.phase=value.phase;controls();
      notice('Recording saved. '+message);
    }
    async function upload() {
      if(!pending || uploading || pending.binding!==getBinding())return;
      if(pending.needsVisualSelection&&!pending.uncertainSubmission){
        try{const original=pending,visual=await getVisual();if(pending!==original||original.binding!==getBinding())return;pending={...original,id:window.crypto.randomUUID(),visual,needsVisualSelection:false};}
        catch(_){notice('Point again or remove the image before retrying this recording.',true);return;}
      }
      const frozen=pending;uploading=true;controller=new AbortController();const active=controller;
      const deadline=setTimeout(()=>active.abort(),20000);notice('Saving your recording…');controls();
      try {
        const response=await fetch('api/interaction/voice',{method:'POST',credentials:'same-origin',signal:active.signal,
          headers:{...headers(),...(frozen.reply?{'X-Reply-Source':(frozen.reply.context_only?'context:':'')+frozen.reply.source,'X-Reply-Revision':frozen.reply.revision}:{}),...(frozen.visual?{'X-Visual-Reference':JSON.stringify(frozen.visual)}:{}),'Content-Type':frozen.mime,'X-Input-Binding':frozen.binding,'X-Input-Id':frozen.id,'X-Audio-Sha256':frozen.audio_sha256},body:frozen.blob});
        const value=await response.json();
        if(!response.ok){const error=new Error('Recording not confirmed');error.status=response.status;if(value.code==='visual_reselection'&&value.submission_attempted===false)error.visualReselection=true;throw error;}
        await confirmed(value,frozen);
      } catch(e) {
        if(pending!==frozen)return;
        if(e.visualReselection){
          if(frozen.uncertainSubmission){pending={...frozen,needsVisualSelection:false};notice('An earlier submission is still unconfirmed. Keep this recording identity and check its status before starting another attempt.',true);return;}
          pending={...frozen,needsVisualSelection:true};notice('This attempt did not submit the recording. Point again or remove the image, then retry this recording.',true);return;
        }
        pending={...frozen,uncertainSubmission:true};
        notice('Recording unconfirmed. Check its status or retry this same recording. Nothing is resent automatically.',true);
        if(e.status===401 || e.status===403)unauthorized(e);
      } finally {clearTimeout(deadline);if(controller===active){controller=undefined;uploading=false;controls();}}
    }
    async function finish(owned,binding,mime,selection) {
      if(owned!==generation)return;
      stopTracks();recorder=undefined;
      const blob=new Blob(parts,{type:mime});parts=[];bytes=0;
      if(!commit){phase='ready';parts=[blob];readySelection={binding,selection};notice('Recording limit reached. Send this recording or discard it.',true);controls();return;}
      phase='finalizing';controls();
      try {
        if(!blob.size || blob.size>20*1024*1024)throw Error('Recording outside limits');
        const digest=await window.crypto.subtle.digest('SHA-256',await blob.arrayBuffer());
        if(owned!==generation || binding!==getBinding())return;
        const audio_sha256=Array.from(new Uint8Array(digest),v=>v.toString(16).padStart(2,'0')).join('');
        pending={id:window.crypto.randomUUID(),binding,audio_sha256,mime,blob,reply:selection?.reference,focusVersion:selection?.version,visual:selection?.visual};phase='idle';await upload();
      } catch(_) {if(owned===generation)cancelRecording('Recording could not be finalized. Nothing was sent.');}
    }
    async function start() {
      if(!available || !getBinding() || !supported() || document.hidden)return;
      const owned=++generation,binding=getBinding(),selection=getReply();latestSaved=undefined;phase='permission';commit=false;parts=[];bytes=0;
      notice('Allow microphone access to begin.');pauseAudio();controls();
      try {
        let selectedVisual;
        try {const candidate=getVisual();selectedVisual=candidate?.then?await candidate:candidate;}
        catch(_){if(owned===generation)cancelRecording('Selected image unavailable. Choose another image or remove it before recording.');return;}
        if(owned!==generation||binding!==getBinding()||document.hidden)return;
        const frozenSelection={...selection,visual:selectedVisual};
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
        current.addEventListener('stop',()=>finish(owned,binding,mime,frozenSelection));
        current.start(250);phase='recording';notice('Listening. Press Send when you finish.');controls();
        timer=setTimeout(()=>{if(owned===generation && current.state==='recording'){phase='finalizing';current.stop();stopTracks();controls();}},120000);
      } catch(_) {if(owned===generation)cancelRecording('Microphone unavailable. You can keep writing.');}
    }
    function submit(){
      if(uploading)return;
      if(phase==='recording'){commit=true;phase='finalizing';recorder.stop();stopTracks();controls();}
      else if(phase==='ready'){commit=true;return finish(generation,readySelection.binding,parts[0].type,readySelection.selection);}
      else if(pending)return upload();
    }
    button.addEventListener('click',()=>ownsSend()?submit():start());
    send?.addEventListener('click',()=>{if(ownsSend())return submit();});
    discard.addEventListener('click',()=>{if(window.confirm?.('Discard this voice recording? The recorded audio will be deleted without sending it. Choose Cancel to keep it.'))cancelRecording();});
    check.addEventListener('click',async()=>{
      if(!pending || uploading)return;const frozen=pending,owned=generation;uploading=true;controls();
      try {await confirmed(await api(`interaction/inputs/${frozen.id}`),frozen);}
      catch(e){if(pending===frozen){notice('Recording capture remains unconfirmed. Nothing has been resent.',true);if(e.status===401 || e.status===403)unauthorized(e);}}
      finally{if(owned===generation){uploading=false;controls();}}
    });
    document.addEventListener('visibilitychange',()=>{if(document.hidden)deactivate();});
    window.addEventListener('pagehide',deactivate);telegram?.onEvent?.('deactivated',deactivate);
    controls();return {configure,reset,deactivate,observe,submit,ownsSend,syncControls:syncSend};
  }
  return {mount};
})();
