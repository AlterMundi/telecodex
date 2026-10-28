"use strict";
window.VoiceInput = (() => {
  // One unfinished original per origin, bounded by the capture limit. Never
  // evict an uncertain original to make room: confirmation or explicit discard
  // retires it. Reload recovery does not upload or dispatch anything.
  function recoveryStore() {
    if(!window.indexedDB)return;
    let opening;
    async function db(){
      if(!opening)opening=new Promise((resolve,reject)=>{
        let settled=false;const timer=setTimeout(()=>{settled=true;reject(Error('Local recording storage timed out'));},3000);
        const request=window.indexedDB.open('daimon-voice-recovery',1);
        request.onupgradeneeded=()=>{request.result.createObjectStore('capture');};
        request.onsuccess=()=>{clearTimeout(timer);if(settled){request.result.close();return;}settled=true;resolve(request.result);};
        request.onerror=request.onblocked=()=>{clearTimeout(timer);settled=true;reject(Error('Local recording storage unavailable'));};
      });
      return opening;
    }
    async function operation(mode,change){
      const value=await db();return new Promise((resolve,reject)=>{
        const tx=value.transaction('capture',mode),store=tx.objectStore('capture'),request=store.get('pending');let result;
        const timer=setTimeout(()=>{try{tx.abort();}catch{}reject(Error('Local recording recovery timed out'));},3000);
        request.onsuccess=()=>{try{result=change(store,request.result);}catch(error){tx.abort();reject(error);}};
        tx.oncomplete=()=>{clearTimeout(timer);resolve(result);};tx.onerror=tx.onabort=()=>{clearTimeout(timer);reject(Error('Local recording recovery unavailable'));};
      });
    }
    return {
      read:()=>operation('readonly',(_store,old)=>old),
      put:value=>{
        const {blob,...meta}=value;
        if(!blob || blob.size>20*1024*1024 || new Blob([JSON.stringify(meta)]).size>65536)return Promise.reject(Error('Recording recovery limit reached'));
        return operation('readwrite',(store,old)=>{
          if(old && (old.id!==value.id || old.audio_sha256!==value.audio_sha256))throw Error('Another unfinished recording is retained');
          store.put(value,'pending');
        });
      },
      remove:value=>operation('readwrite',(store,old)=>{if(old?.id===value.id && old.audio_sha256===value.audio_sha256)store.delete('pending');})
    };
  }
  function mount({api, headers, getBinding, onInput, getReply=()=>undefined, getPlayback=()=>undefined, getCaptureQuestions=()=>undefined, getVisual=()=>undefined, getFiles=()=>undefined, onFilesSent=()=>{}, onVisualSent=()=>{}, pauseAudio, onActivity=()=>{}, unauthorized, telegram}) {
    const box=document.querySelector('#conversation-voice'), button=document.querySelector('#conversation-talk'),send=document.querySelector('#conversation-send');
    const discard=document.querySelector('#conversation-voice-discard'), check=document.querySelector('#conversation-voice-check');
    const state=document.querySelector('#conversation-voice-state'),save=document.querySelector('#conversation-voice-save'),review=document.querySelector('#conversation-recording-review');
    const recovery=recoveryStore();let restoreBinding,restoring=false;
    function notice(text,attention=false){state.removeAttribute?.('data-live-caption');state.setAttribute('aria-live','polite');state.textContent=text;state.setAttribute('data-attention',String(attention));}
    function caption(text){notice(text);state.setAttribute('data-live-caption','true');state.setAttribute('aria-live','off');state.scrollLeft=state.scrollWidth;}
    let available=false, liveAvailable=false, liveMode='prepared', generation=0, recorder, stream, parts=[], bytes=0, timer, pending, latestSaved, uploading=false, controller;
    let phase='idle', commit=false, readySelection;
    const microphoneIcon=button.querySelector?.('svg')?.cloneNode(true);
    const supported=()=>!!(window.isSecureContext && window.MediaRecorder && typeof window.MediaRecorder.isTypeSupported==='function' && window.navigator?.mediaDevices?.getUserMedia && window.crypto?.subtle);
    const ownsSend=()=>['permission','recording','ready','finalizing'].includes(phase) || uploading || !!pending;
    let ownedSend=false;
    function syncSend(){
      const owned=ownsSend();button.hidden=owned&&!pending;
      if(send){send.type=owned?'button':'submit';send.setAttribute('aria-label',owned?'Send voice message':'Send text message');
        if(owned)send.disabled=button.disabled || !(['recording','ready'].includes(phase)||pending);else if(ownedSend)send.disabled=!getBinding();}
      ownedSend=owned;
    }
    function controls() {
      box.hidden=!available;
      if(phase==='idle' && !pending && microphoneIcon)button.replaceChildren(microphoneIcon.cloneNode(true));
      else button.textContent=phase==='recording'||phase==='ready'?'↑':pending?'↻':['permission','finalizing'].includes(phase)||uploading?'…':'🎙';
      if(pending)button.innerHTML='<svg class="control-icon" viewBox="0 0 24 24" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.8"><path d="M20 7v5h-5M20 12a8 8 0 1 0-2 5"/></svg>';
      button.setAttribute('aria-label',phase==='recording'?'Send voice message':phase==='ready'?'Send recording':pending?'Retry this recording':'Record a voice message');
      button.disabled=!available || !getBinding() || !supported() || restoring || uploading || phase==='permission' || phase==='finalizing';
      button.setAttribute('aria-pressed',String(phase==='recording'));
      syncSend();
      discard.hidden=!pending&&!['permission','recording','ready','finalizing'].includes(phase);discard.disabled=uploading;
      if(save){save.hidden=!pending;save.disabled=uploading;}if(review)review.hidden=!pending;
      discard.textContent='×';discard.setAttribute('aria-label',pending?'Discard local recording':'Discard recording');discard.title=pending?'Discard local recording':'Discard recording';
      check.hidden=!pending; check.disabled=uploading;
      onActivity(uploading || ['permission','recording','ready','finalizing'].includes(phase) || latestSaved?.phase==='preparing', {phase,uploading,transcribing:latestSaved?.phase==='preparing'});
    }
    function stopTracks() {if(stream)for(const track of stream.getTracks())track.stop();stream=undefined;clearTimeout(timer);}
    function cancelRecording(message='Recording discarded. Nothing was sent.') {
      generation++;commit=false;latestSaved=undefined;const old=recorder;recorder=undefined;
      if(old?.discard)old.discard();else if(old && old.state!=='inactive')old.stop();stopTracks();parts=[];bytes=0;phase='idle';notice(message,!!message && message!=='Recording discarded. Nothing was sent.');controls();
    }
    function reset() {
      restoreBinding=undefined;
      controller?.abort();controller=undefined;pending=undefined;uploading=false;
      cancelRecording('');available=false;controls();
    }
    function deactivate() {
      if(['permission','recording','ready','finalizing'].includes(phase))cancelRecording('Recording ended while this view was inactive. Nothing was sent.');
      controller?.abort();
    }
    function configure(value,live=false,mode='prepared') {
      available=!!value;liveAvailable=!!live;liveMode=mode==='captions'?'captions':'prepared';
      if(!available && ['permission','recording','ready','finalizing'].includes(phase))cancelRecording();
      if(available && !supported())notice('Microphone unavailable in this browser. You can keep writing.',true);
      controls();
      if(available && recovery && getBinding() && restoreBinding!==getBinding() && !pending && phase==='idle')void restore();
    }
    async function restore(){
      const binding=getBinding(),owned=generation;restoreBinding=binding;restoring=true;controls();
      try {
        const value=await recovery.read();
        if(owned!==generation || binding!==getBinding() || phase!=='idle' || pending)return;
        if(value?.binding===binding && value.blob?.size>0 && value.blob.size<=20*1024*1024){
          pending={...value,uncertainSubmission:true,localRecovery:true};notice('An unfinished recording was recovered. Check its status, retry the same recording, or save a copy. Nothing was resent.',true);
        }
      }catch(_){if(owned===generation)notice('Local recovery unavailable. Keep this page open until your recording is confirmed.',true);}
      finally{if(owned===generation){restoring=false;controls();}}
    }
    async function confirmed(value,frozen) {
      if(pending!==frozen || frozen.binding!==getBinding())return;
      if(value.id!==frozen.id || value.voice?.audio_sha256!==frozen.audio_sha256)throw Error('Voice capture does not match this recording.');
      if(!!frozen.visual!==!!value.visual||Array.isArray(frozen.visual)!==Array.isArray(value.visual)||Array.isArray(frozen.visual)&&frozen.visual.length!==value.visual.length||frozen.visual&&Object.keys(frozen.visual).some(k=>JSON.stringify(frozen.visual[k])!==JSON.stringify(value.visual[k])))throw Error('Visual capture does not match this recording.');
      const fileValues=refs=>(refs||[]).map(file=>['file_id','sha256','name','mime','size'].map(key=>file[key]));
      if(JSON.stringify(fileValues(frozen.files))!==JSON.stringify(fileValues(value.files)))throw Error('Original files do not match this recording.');
      latestSaved={id:frozen.id,binding:frozen.binding,audio_sha256:frozen.audio_sha256,phase:value.phase,preparedText:frozen.preparedText};
      onInput(value,frozen.focusVersion);onVisualSent(frozen.visual);onFilesSent(frozen.files);pending=undefined;phase='idle';
      try{await recovery?.remove(frozen);}catch{}
      notice(value.phase==='answered'?'Recording and automatic transcript saved; answer recorded for the selected question.':value.retired?'This saved recording is no longer eligible to start work.':
        value.phase==='retained_unsent'?'Recording saved; no work was submitted. You can keep writing.':
        value.phase==='preparation_unconfirmed'?'Recording saved; transcription unconfirmed. Nothing was resubmitted.':
        value.phase==='preparing'?'Confirming transcript…':
        'Recording and automatic transcript saved. Work may continue.',value.retired || ['retained_unsent','preparation_unconfirmed'].includes(value.phase));
      if(value.phase==='preparing'&&frozen.preparedText)caption(frozen.preparedText);
      controls();
    }
    function observe(value,message) {
      if(!latestSaved || pending || phase!=='idle' || latestSaved.binding!==getBinding()
        || latestSaved.id!==value.id || latestSaved.audio_sha256!==value.voice?.audio_sha256)return;
      latestSaved.phase=value.phase;controls();
      if(value.phase==='preparing'&&latestSaved.preparedText)caption(latestSaved.preparedText);
      else notice('');
    }
    async function upload() {
      if(!pending || uploading || pending.binding!==getBinding())return;
      if(pending.needsVisualSelection&&!pending.uncertainSubmission){
        try{const original=pending,visual=await getVisual();if(pending!==original||original.binding!==getBinding())return;pending={...original,id:window.crypto.randomUUID(),visual,needsVisualSelection:false};}
        catch(_){notice('Point again or remove the image before retrying this recording.',true);return;}
      }
      const frozen=pending;uploading=true;controller=new AbortController();const active=controller;
      const deadline=setTimeout(()=>active.abort(),20000);if(frozen.preparedText)caption(frozen.preparedText);else notice('Saving your recording…');controls();
      try {
        const response=await fetch('api/interaction/voice',{method:'POST',credentials:'same-origin',signal:active.signal,
          headers:{...headers(),...(frozen.reply?{'X-Reply-Source':(frozen.reply.context_only?'context:':'')+frozen.reply.source,'X-Reply-Revision':frozen.reply.revision}:{}),...(frozen.visual?{'X-Visual-Reference':JSON.stringify(frozen.visual)}:{}),...(frozen.playback?{'X-Playback-Context':JSON.stringify(frozen.playback)}:{}),...(frozen.captureQuestions!==undefined?{'X-Capture-Questions':JSON.stringify(frozen.captureQuestions)}:{}),...(frozen.files?.length?{'X-File-References':encodeURIComponent(JSON.stringify(frozen.files))}:{}),'Content-Type':frozen.mime,'X-Input-Binding':frozen.binding,'X-Input-Id':frozen.id,'X-Audio-Sha256':frozen.audio_sha256},body:frozen.blob});
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
        notice('Recording unconfirmed. Check its status or retry this same recording. Nothing is resent automatically.'+(!frozen.localRecovery?' Keep this page open or save an audio copy before leaving.':''),true);
        if(e.status===401 || e.status===403)unauthorized(e);
      } finally {clearTimeout(deadline);if(controller===active){controller=undefined;uploading=false;controls();}}
    }
    async function finish(owned,binding,mime,selection) {
      if(owned!==generation)return;
      stopTracks();const preparedText=recorder?.preparedText;recorder=undefined;
      const blob=new Blob(parts,{type:mime});parts=[];bytes=0;
      if(!commit){phase='ready';parts=[blob];readySelection={binding,selection};notice('Recording limit reached. Send this recording or discard it.',true);controls();return;}
      phase='finalizing';controls();
      try {
        if(!blob.size || blob.size>20*1024*1024)throw Error('Recording outside limits');
        const digest=await window.crypto.subtle.digest('SHA-256',await blob.arrayBuffer());
        if(owned!==generation || binding!==getBinding())return;
        const audio_sha256=Array.from(new Uint8Array(digest),v=>v.toString(16).padStart(2,'0')).join('');
        pending={id:selection?.captureId||window.crypto.randomUUID(),binding,audio_sha256,mime,blob,preparedText,reply:selection?.reference,focusVersion:selection?.version,visual:selection?.visual,files:selection?.files,playback:selection?.playback,captureQuestions:selection?.captureQuestions};phase='idle';
        try{if(recovery){await recovery.put(pending);pending.localRecovery=true;}}catch{pending.localRecovery=false;}
        if(owned!==generation || binding!==getBinding())return;
        await upload();
      } catch(_) {if(owned===generation)cancelRecording('Recording could not be finalized. Nothing was sent.');}
    }
    async function start() {
      if(!available || !getBinding() || !supported() || restoring || document.hidden)return;
      const owned=++generation,binding=getBinding(),captureQuestions=getCaptureQuestions(),selection={...getReply(),playback:getPlayback(),...(captureQuestions!==undefined?{captureQuestions:JSON.parse(JSON.stringify(captureQuestions))}:{})};latestSaved=undefined;phase='permission';commit=false;parts=[];bytes=0;
      notice('Allow microphone access to begin.');pauseAudio();controls();
      try {
        let selectedVisual;
        try {const candidate=getVisual();selectedVisual=candidate?.then?await candidate:candidate;}
        catch(_){if(owned===generation)cancelRecording('Selected image unavailable. Choose another image or remove it before recording.');return;}
        if(owned!==generation||binding!==getBinding()||document.hidden)return;
        const fileCandidate=getFiles();const selectedFiles=fileCandidate?.then?await fileCandidate:fileCandidate;
        if(owned!==generation||binding!==getBinding()||document.hidden)return;
        const frozenSelection={...selection,visual:selectedVisual,files:selectedFiles,captureId:window.crypto.randomUUID()};
        const media=await window.navigator.mediaDevices.getUserMedia({audio:true,video:false});
        if(owned!==generation || binding!==getBinding() || document.hidden){for(const track of media.getTracks())track.stop();return;}
        stream=media;
        let current;
        const fallbackMime=['audio/webm;codecs=opus','audio/ogg;codecs=opus','audio/mp4','audio/webm'].find(value=>window.MediaRecorder.isTypeSupported(value));
        const canonical=liveMode==='captions'&&fallbackMime?new window.MediaRecorder(media,{mimeType:fallbackMime,audioBitsPerSecond:64000}):undefined;
        if((liveMode!=='captions'||canonical)&&liveAvailable&&window.StreamingVoice?.supported()){
          try{current=await window.StreamingVoice.create(media,{headers,binding,canonicalRecorder:canonical,id:frozenSelection.captureId,onPartial:text=>{if(owned===generation&&phase==='recording'){caption(text||'Listening…');}}});}catch{}
        }
        if(owned!==generation||binding!==getBinding()||document.hidden){current?.discard?.();for(const track of media.getTracks())track.stop();return;}
        current=current||canonical;
        const mime=current?(current.mimeType||'audio/wav'):fallbackMime;
        if(!mime)throw Error('No supported recording format');
        current=current||new window.MediaRecorder(media,{mimeType:mime,audioBitsPerSecond:64000});recorder=current;
        current.addEventListener('dataavailable',event=>{
          if(owned!==generation || !event.data.size)return;
          bytes+=event.data.size;if(bytes>20*1024*1024){cancelRecording('Recording exceeded its size limit. Nothing was sent.');return;}
          parts.push(event.data);
        });
        current.addEventListener('error',()=>{if(owned===generation)cancelRecording('Recording failed. Nothing was sent.');});
        current.addEventListener('stop',()=>finish(owned,binding,mime,frozenSelection));
        current.start(250);phase='recording';notice('Listening. Press Send when you finish.');controls();
        timer=setTimeout(()=>{if(owned===generation && current.state==='recording'){phase='finalizing';current.stop();stopTracks();controls();}},120000);
      } catch(error) {
        const reason={NotAllowedError:'Microphone permission was denied. Check this browser’s site permissions.',
          NotFoundError:'This browser could not find an accessible microphone.',
          NotReadableError:'The microphone could not be opened. It may be busy or unavailable to this browser.',
          NotSupportedError:'Audio recording is not supported by this browser.',
          SecurityError:'Microphone access is blocked by this browser.'}[error?.name];
        if(owned===generation)cancelRecording(reason || 'Microphone unavailable. You can keep writing.');
      }
    }
    function submit(){
      if(uploading)return;
      if(phase==='recording'){commit=true;phase='finalizing';recorder.stop();stopTracks();controls();}
      else if(phase==='ready'){commit=true;return finish(generation,readySelection.binding,parts[0].type,readySelection.selection);}
      else if(pending)return upload();
    }
    button.addEventListener('click',()=>ownsSend()?submit():start());
    send?.addEventListener('click',()=>{if(ownsSend())return submit();});
    discard.addEventListener('click',()=>{
      if(uploading)return;
      const frozen=pending,message=frozen?'Discard the local recording? An earlier submission may already be saved on the server. This does not cancel it. Save a copy first if you need the audio.':'Discard this voice recording? The recorded audio will be deleted without sending it. Choose Cancel to keep it.';
      if(window.confirm?.(message)){
        pending=undefined;if(frozen)void recovery?.remove(frozen).catch(()=>{});
        cancelRecording(frozen?'Local recording discarded. Existing server work was not cancelled.':'Recording discarded. Nothing was sent.');
      }
    });
    save?.addEventListener('click',()=>{
      if(!pending || uploading || !window.URL?.createObjectURL)return;
      const frozen=pending,url=window.URL.createObjectURL(frozen.blob),link=document.createElement('a');
      link.href=url;link.download=`recording-${frozen.id}.${frozen.mime.includes('wav')?'wav':frozen.mime.includes('mp4')?'m4a':frozen.mime.includes('ogg')?'ogg':'webm'}`;
      document.body.append(link);link.click();link.remove();setTimeout(()=>window.URL.revokeObjectURL(url),10000);
      notice('Recording copy offered for download. Check its saved status or retry the same original; nothing was resent.',true);
    });
    check.addEventListener('click',async()=>{
      if(!pending || uploading)return;const frozen=pending,owned=generation;uploading=true;notice('Checking whether this recording was saved…');controls();
      try {await confirmed(await api(`interaction/inputs/${frozen.id}`),frozen);}
      catch(e){if(pending===frozen){notice([404,409].includes(e.status)?'No saved capture was confirmed. The local recording is kept: retry the same recording, save a copy, or discard the local copy.':'Could not confirm the saved recording. The local audio is kept; check again or retry the same recording. Nothing was resent.',true);if(e.status===401 || e.status===403)unauthorized(e);}}
      finally{if(owned===generation){uploading=false;controls();}}
    });
    document.addEventListener('visibilitychange',()=>{if(document.hidden)deactivate();});
    window.addEventListener('pagehide',deactivate);telegram?.onEvent?.('deactivated',deactivate);
    controls();return {configure,reset,deactivate,observe,submit,ownsSend,syncControls:syncSend};
  }
  return {mount};
})();
