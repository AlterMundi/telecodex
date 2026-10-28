"use strict";
window.StreamingVoice=(()=>{
  function wav(parts){
    const size=parts.reduce((sum,part)=>sum+part.byteLength,0),header=new ArrayBuffer(44),v=new DataView(header);
    function text(offset,value){for(let i=0;i<value.length;i++)v.setUint8(offset+i,value.charCodeAt(i));}
    text(0,'RIFF');v.setUint32(4,size+36,true);text(8,'WAVEfmt ');v.setUint32(16,16,true);
    v.setUint16(20,1,true);v.setUint16(22,1,true);v.setUint32(24,16000,true);v.setUint32(28,32000,true);
    v.setUint16(32,2,true);v.setUint16(34,16,true);text(36,'data');v.setUint32(40,size,true);
    return new Blob([header,...parts],{type:'audio/wav'});
  }
  const supported=()=>!!(window.AudioWorkletNode&&(window.AudioContext||window.webkitAudioContext)&&window.WebSocket);
  // One explicitly owned foreground graph can outlive an utterance. It never
  // retains PCM: its bounded consumers own originals and any pre-speech ring.
  async function openCapture(media){
    const Context=window.AudioContext||window.webkitAudioContext,context=new Context();
    let worklet,source,filter,mute,started=false,closed=false,expired=false,timer;
    const listeners=new Set();
    function emit(value){for(const listener of [...listeners])listener(value);}
    function close(reason='capture_closed'){
      if(closed)return;closed=true;clearTimeout(timer);emit({captureError:reason});
      for(const node of [source,filter,worklet,mute])node?.disconnect();
      context.close().catch(()=>{});listeners.clear();
    }
    try{
      await context.audioWorklet.addModule('assets/voice-pcm.js');worklet=new AudioWorkletNode(context,'interaction-pcm');
      source=context.createMediaStreamSource(media);filter=context.createBiquadFilter();mute=context.createGain();
      filter.type='lowpass';filter.frequency.value=7400;mute.gain.value=0;
    }catch(error){close();throw error;}
    worklet.port.onmessage=message=>{
      if(closed)return;
      if(message.data.captureError){close(message.data.captureError);return;}
      if(message.data.pcm&&message.data.totalBytes>300*32000){
        if(!expired){expired=true;close('session_limit');}
        return;
      }
      emit(message.data);
    };
    return {
      subscribe(listener){
        if(closed||listeners.size>=3||typeof listener!=='function')throw Error('Capture consumer unavailable');
        listeners.add(listener);return ()=>listeners.delete(listener);
      },
      start(){
        if(closed)throw Error('Capture closed');if(started)return;started=true;
        try{source.connect(filter);filter.connect(worklet);worklet.connect(mute);mute.connect(context.destination);}
        catch(error){close('capture_unavailable');throw error;}
        timer=setTimeout(()=>close('session_limit'),300000);
        context.resume().catch(()=>close('capture_unavailable'));
      },
      observe(){if(!closed)worklet.port.postMessage('observe');},
      boundary(token){if(closed)throw Error('Capture closed');worklet.port.postMessage({boundary:token});},
      finish(){if(!closed)worklet.port.postMessage('finish');},
      close,
      get closed(){return closed;}
    };
  }
  // One explicitly selected foreground analysis socket. It neither transcribes
  // nor admits originals; the coordinator owns PCM/original retention and Send.
  async function openActivity({headers,binding,id,onActivity=()=>{},onUnavailable=()=>{},signal}){
    if(signal?.aborted)throw Error('Foreground voice closed');
    const target=new URL('api/interaction/voice/activity',window.location.href);target.protocol=target.protocol==='https:'?'wss:':'ws:';
    const socket=new WebSocket(target);
    let ready=false,closed=false,total=0,offset=0,lastEndpoint,pendingBoundary,timer,deadline,resolve,reject;
    const opened=new Promise((yes,no)=>{resolve=yes;reject=no;});
    function close(){
      if(closed)return;closed=true;clearTimeout(timer);clearTimeout(deadline);signal?.removeEventListener('abort',abort);
      try{if(ready&&socket.readyState===WebSocket.OPEN)socket.send(JSON.stringify({type:'close'}));}catch{}
      socket.close();if(!ready)reject(Error('Foreground voice closed'));
    }
    function fail(){if(closed)return;const wasReady=ready;close();if(wasReady)onUnavailable();}
    function abort(){close();}
    signal?.addEventListener('abort',abort,{once:true});
    timer=setTimeout(fail,35000);deadline=setTimeout(fail,300000);
    socket.onopen=()=>{
      if(closed)return;
      try{
        const values=headers(),authorization=values.Authorization||values.authorization;
        socket.send(JSON.stringify({id,binding,csrf:values['X-Dashboard-CSRF']||values['x-dashboard-csrf'],...(authorization?{authorization}:{})}));
      }catch{fail();}
    };
    socket.onmessage=event=>{
      if(closed)return;
      try{
        if(typeof event.data!=='string'||event.data.length>8192)throw Error('Invalid activity event');
        const value=JSON.parse(event.data);
        if(value.type==='ready'){
          if(ready||value.id!==id||value.analysis_only!==true)throw Error('Unexpected activity readiness');
          ready=true;clearTimeout(timer);resolve();return;
        }
        if(!ready||value.id!==id||!['turn_activity','boundary'].includes(value.type))throw Error('Activity unavailable');
        const a=value.activity;
        if(value.type==='turn_activity'){
          if(!a||a.schema!=='telecodex.turn-activity/v1'||a.type!=='activity'
             ||!Number.isSafeInteger(a.audio_bytes)||a.audio_bytes<offset||a.audio_bytes>total
             ||!Number.isSafeInteger(a.generation)||a.generation<0||typeof a.speaking!=='boolean'||typeof a.speech_seen!=='boolean')throw Error('Invalid activity snapshot');
          offset=a.audio_bytes;
          for(const v of [a.onset,a.endpoint].filter(Boolean))if(!Number.isSafeInteger(v.generation)||v.generation<=0||!Number.isSafeInteger(v.audio_bytes)||v.audio_bytes<=0||v.audio_bytes>a.audio_bytes||v.generation>a.generation)throw Error('Invalid speech boundary');
          if(a.endpoint){
            if(a.endpoint.generation!==a.generation||a.endpoint.authority!=='analysis_only'||!Number.isFinite(a.endpoint.probability)||a.endpoint.probability<.5||a.endpoint.probability>1)throw Error('Invalid endpoint verdict');
            lastEndpoint={generation:a.endpoint.generation,audio_bytes:a.endpoint.audio_bytes};
          }
          onActivity(a);
        }else{
          if(!pendingBoundary||!a||a.schema!=='telecodex.turn-activity/v1'||a.type!=='boundary'||a.generation!==pendingBoundary.generation||a.audio_bytes!==pendingBoundary.audio_bytes)throw Error('Unexpected analysis boundary acknowledgment');
          pendingBoundary=undefined;
        }
      }catch{fail();}
    };
    socket.onerror=socket.onclose=fail;
    await opened;
    return {
      feed(pcm){
        if(closed)return false;
        if(!(pcm instanceof ArrayBuffer)||!pcm.byteLength||pcm.byteLength%2||pcm.byteLength>32000||total+pcm.byteLength>300*32000||socket.bufferedAmount>256*1024){fail();return false;}
        try{socket.send(pcm);total+=pcm.byteLength;return true;}catch{fail();return false;}
      },
      boundary(value){
        if(closed||pendingBoundary||!lastEndpoint||value.generation!==lastEndpoint.generation||value.audio_bytes!==lastEndpoint.audio_bytes)return false;
        try{pendingBoundary=lastEndpoint;socket.send(JSON.stringify({boundary:lastEndpoint}));lastEndpoint=undefined;return true;}catch{fail();return false;}
      },
      close,
      get closed(){return closed;}
    };
  }
  async function create(media,{headers,binding,id,onPartial=()=>{},canonicalRecorder,turnDetection=false,onEndpoint=()=>{},onDetectionUnavailable=()=>{},captureSource,initialPCM=()=>[],onFlushed=()=>{}}){
    if(captureSource&&canonicalRecorder)throw Error('Shared PCM cannot split a compressed original');
    const shared=!!captureSource,capture=captureSource||await openCapture(media);
    let socket,unsubscribe;
    try{
      const target=new URL('api/interaction/voice/live',window.location.href);target.protocol=target.protocol==='https:'?'wss:':'ws:';
      socket=new WebSocket(target);
    }catch(error){if(!shared)capture.close();throw error;}
    let origin=0,collecting=!shared;
    let ready=false,parts=[],sent=0,bytes=0,ending=false,discarded=false,done=false,timer,finishSent=false,disposed=false,flushed=false;
    let lastEnergy=0,totalSamplesBytes=0,lastEndpoint=0,detectorFailed=false,recognized=false,verdict;
    const events=new EventTarget(),recorder={state:'inactive',mimeType:canonicalRecorder?.mimeType||'audio/wav',addEventListener:(...args)=>events.addEventListener(...args)};
    function event(type,data){const e=new Event(type);if(data!==undefined)Object.defineProperty(e,'data',{value:data});events.dispatchEvent(e);}
    function dispose(){if(disposed)return;disposed=true;clearTimeout(timer);unsubscribe?.();if(!shared)capture.close();socket.close();}
    function complete(){
      if(done)return;done=true;
      if(!discarded && !canonicalRecorder && !flushed)recorder.captureStop ||= 'audio_flush_unconfirmed';
      const blob=discarded||canonicalRecorder||!parts.length?undefined:wav(parts);parts=[];dispose();recorder.state='inactive';
      if(blob)event('dataavailable',blob);event('stop');
    }
    if(canonicalRecorder){
      canonicalRecorder.addEventListener('dataavailable',e=>{if(!discarded&&!done)event('dataavailable',e.data);});
      canonicalRecorder.addEventListener('error',()=>event('error'));
      canonicalRecorder.addEventListener('stop',complete);
    }
    function pump(){
      if(!ready||socket.readyState!==WebSocket.OPEN||discarded)return;
      try{
        while(sent<parts.length){
          if(socket.bufferedAmount>256*1024){ready=false;socket.close();return;}
          socket.send(parts[sent++]);
        }
        if(canonicalRecorder&&sent){parts.splice(0,sent);sent=0;}
        if(ending&&!finishSent){finishSent=true;socket.send(JSON.stringify({type:'finish'}));}
      }catch{ready=false;socket.close();if(ending&&!canonicalRecorder)complete();}
    }
    socket.onopen=()=>{
      if(done)return;
      try{
        const values=headers(),authorization=values.Authorization||values.authorization;
        socket.send(JSON.stringify({id,binding,...(canonicalRecorder?{mode:'captions'}:{}),...(turnDetection?{turn_detection:true}:{}),csrf:values['X-Dashboard-CSRF']||values['x-dashboard-csrf'],...(authorization?{authorization}:{})}));
      }catch{ready=false;socket.close();}
    };
    function considerEndpoint(){
      if(!recognized||!verdict||detectorFailed||ending||discarded||done||recorder.state!=='recording')return;
      const activity=verdict,endpoint=activity.endpoint;
          if(endpoint && activity.speech_seen===true && activity.speaking===false
            && endpoint.authority==='analysis_only' && Number.isSafeInteger(endpoint.generation) && endpoint.generation>lastEndpoint
            && endpoint.generation===activity.generation && Number.isSafeInteger(endpoint.audio_bytes) && endpoint.audio_bytes>0
            && endpoint.audio_bytes===activity.audio_bytes && endpoint.audio_bytes<=bytes && endpoint.audio_bytes<=totalSamplesBytes
            && lastEnergy<=endpoint.audio_bytes && totalSamplesBytes-endpoint.audio_bytes<32000
            && Number.isFinite(endpoint.probability)&&endpoint.probability>=.5&&endpoint.probability<=1){
            lastEndpoint=endpoint.generation;onEndpoint();
          }
    }
    socket.onmessage=event=>{
      if(discarded||done)return;
      try{
        if(typeof event.data!=='string'||event.data.length>65536)throw Error('Invalid caption event');
        const value=JSON.parse(event.data);
        if(value.type==='ready'){ready=true;pump();}
        else if(value.type==='partial'&&typeof value.text==='string'){recognized=!!value.text.trim();onPartial(value.text);considerEndpoint();}
        else if(value.type==='turn_detection_unavailable'){detectorFailed=true;onDetectionUnavailable();}
        else if(value.type==='turn_activity'&&turnDetection&&value.id===id){verdict=value.activity;considerEndpoint();}
        else if(value.type==='prepared'){
          if(canonicalRecorder)throw Error('Live captions cannot confirm the original');
          if(value.id!==id||!ending||!finishSent||typeof value.text!=='string')throw Error('Unexpected prepared recording');
          recorder.preparedText=value.text;
          complete();
        }else if(value.type==='unavailable'){ready=false;socket.close();if(ending&&!canonicalRecorder)complete();}
      }catch{ready=false;socket.close();if(ending&&!canonicalRecorder)complete();}
    };
    const unavailable=()=>{
      ready=false;
      if(turnDetection&&!detectorFailed&&!ending&&!discarded&&!done){detectorFailed=true;onDetectionUnavailable();}
      if(canonicalRecorder){parts=[];dispose();}else if(ending&&!canonicalRecorder)complete();
    };
    socket.onclose=unavailable;socket.onerror=unavailable;
    try{unsubscribe=capture.subscribe(data=>{
      const message={data:{...data}};
      if(done||discarded)return;
      if(data.captureError){if(shared&&flushed)return;if(canonicalRecorder){previewFailed();return;}recorder.captureError=data.captureError;recorder.captureStop='audio_flush_unconfirmed';ending=true;complete();return;}
      if(shared&&data.boundary===id+':begin'){
        try{
          const prefix=initialPCM(data.totalBytes);
          if(!Array.isArray(prefix)||prefix.some(p=>!(p instanceof ArrayBuffer)||p.byteLength%2))throw Error('Invalid capture prefix');
          const size=prefix.reduce((n,p)=>n+p.byteLength,0);
          if(size>256000||size>data.totalBytes)throw Error('Capture prefix outside foreground bounds');
          origin=data.totalBytes-size;collecting=true;
          for(const p of prefix){if(p.byteLength){parts.push(p);bytes+=p.byteLength;}}pump();
        }catch{recorder.captureStop='audio_flush_unconfirmed';complete();}
        return;
      }
      if(shared&&data.boundary===id+':end'){collecting=false;message.data.finished=true;}
      if(shared&&!collecting&&!message.data.finished)return;
      if(Number.isSafeInteger(message.data.activityBytes)&&Number.isSafeInteger(message.data.totalBytes)){
        lastEnergy=Math.max(0,message.data.activityBytes-origin);totalSamplesBytes=Math.max(0,message.data.totalBytes-origin);
      }
      if(message.data.pcm){
        if(canonicalRecorder&&disposed)return;
        const input=message.data.pcm,remaining=120*32000-bytes;
        const pcm=input.byteLength>remaining?input.slice(0,remaining):input;
        if(pcm.byteLength){bytes+=pcm.byteLength;parts.push(pcm);pump();}
        if(canonicalRecorder&&parts.reduce((n,p)=>n+p.byteLength,0)>256*1024){ready=false;dispose();parts=[];}
        if(bytes===120*32000){recorder.captureStop='duration_limit';recorder.stop();}
      }
      if(message.data.finished){
        if(canonicalRecorder)return;
        flushed=true;clearTimeout(timer);ending=true;pump();
        // Keep the complete local WAV as ordinary file-ASR fallback. Never
        // create an original instruction on this preparation socket.
        // Actual Gemini confirmation can approach five seconds. Keep the exact
        // flushed original for a bounded ten-second wait before review.
        timer=setTimeout(complete,10000);
        if(shared)onFlushed(data.totalBytes);
        if(socket.readyState===WebSocket.CLOSED)complete();
      }
    });}catch(error){dispose();throw error;}
    function previewFailed(){
      if(canonicalRecorder){ready=false;parts=[];dispose();}
      else{event('error');recorder.discard();}
    }
    recorder.start=(timeslice)=>{
      recorder.state='recording';canonicalRecorder?.start(timeslice);if(disposed)return;
      try{
        if(turnDetection)capture.observe();
        if(shared)capture.boundary(id+':begin');
        capture.start();
      }catch{previewFailed();}
    };
    recorder.stop=()=>{if(recorder.state==='inactive'||ending||discarded)return;recorder.state='inactive';if(canonicalRecorder){ending=true;ready=false;dispose();canonicalRecorder.stop();return;}if(shared)capture.boundary(id+':end');else capture.finish();timer=setTimeout(complete,6000);};
    recorder.discard=()=>{if(done)return;discarded=true;if(canonicalRecorder&&canonicalRecorder.state!=='inactive'){canonicalRecorder.stop();dispose();return;}if(socket.readyState===WebSocket.OPEN)socket.send(JSON.stringify({type:'discard'}));complete();};
    return recorder;
  }
  return {supported,create,wav,openCapture,openActivity};
})();
