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
  async function create(media,{headers,binding,id,onPartial=()=>{},canonicalRecorder}){
    const Context=window.AudioContext||window.webkitAudioContext,context=new Context();
    let worklet;
    try{await context.audioWorklet.addModule('assets/voice-pcm.js');worklet=new AudioWorkletNode(context,'interaction-pcm');}
    catch(error){await context.close();throw error;}
    let source,filter,mute,socket;
    try{
      source=context.createMediaStreamSource(media);filter=context.createBiquadFilter();mute=context.createGain();
      filter.type='lowpass';filter.frequency.value=7400;mute.gain.value=0;
      const target=new URL('api/interaction/voice/live',window.location.href);target.protocol=target.protocol==='https:'?'wss:':'ws:';
      socket=new WebSocket(target);
    }catch(error){
      for(const node of [source,filter,mute,worklet])if(node)node.disconnect();
      await context.close();throw error;
    }
    let ready=false,parts=[],sent=0,bytes=0,ending=false,discarded=false,done=false,timer,finishSent=false,disposed=false;
    const events=new EventTarget(),recorder={state:'inactive',mimeType:canonicalRecorder?.mimeType||'audio/wav',addEventListener:(...args)=>events.addEventListener(...args)};
    function event(type,data){const e=new Event(type);if(data!==undefined)Object.defineProperty(e,'data',{value:data});events.dispatchEvent(e);}
    function dispose(){if(disposed)return;disposed=true;clearTimeout(timer);source.disconnect();filter.disconnect();worklet.disconnect();mute.disconnect();context.close().catch(()=>{});socket.close();}
    function complete(){
      if(done)return;done=true;
      const blob=discarded||canonicalRecorder?undefined:wav(parts);parts=[];dispose();recorder.state='inactive';
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
        socket.send(JSON.stringify({id,binding,...(canonicalRecorder?{mode:'captions'}:{}),csrf:values['X-Dashboard-CSRF']||values['x-dashboard-csrf'],...(authorization?{authorization}:{})}));
      }catch{ready=false;socket.close();}
    };
    socket.onmessage=event=>{
      if(discarded||done)return;
      try{
        if(typeof event.data!=='string'||event.data.length>65536)throw Error('Invalid caption event');
        const value=JSON.parse(event.data);
        if(value.type==='ready'){ready=true;pump();}
        else if(value.type==='partial'&&typeof value.text==='string')onPartial(value.text);
        else if(value.type==='prepared'){
          if(canonicalRecorder)throw Error('Live captions cannot confirm the original');
          if(value.id!==id||!ending||!finishSent)throw Error('Unexpected prepared recording');
          complete();
        }else if(value.type==='unavailable'){ready=false;socket.close();if(ending&&!canonicalRecorder)complete();}
      }catch{ready=false;socket.close();if(ending&&!canonicalRecorder)complete();}
    };
    const unavailable=()=>{ready=false;if(canonicalRecorder){parts=[];dispose();}else if(ending&&!canonicalRecorder)complete();};
    socket.onclose=unavailable;socket.onerror=unavailable;
    worklet.port.onmessage=message=>{
      if(done||discarded)return;
      if(message.data.pcm){
        if(canonicalRecorder&&disposed)return;
        const input=message.data.pcm,remaining=120*32000-bytes;
        const pcm=input.byteLength>remaining?input.slice(0,remaining):input;
        if(pcm.byteLength){bytes+=pcm.byteLength;parts.push(pcm);pump();}
        if(canonicalRecorder&&parts.reduce((n,p)=>n+p.byteLength,0)>256*1024){ready=false;dispose();parts=[];}
        if(bytes===120*32000)recorder.stop();
      }
      if(message.data.finished){
        if(canonicalRecorder)return;
        clearTimeout(timer);ending=true;pump();
        // Keep the complete local WAV as ordinary file-ASR fallback. Never
        // create an original instruction on this preparation socket.
        timer=setTimeout(complete,5000);
        if(socket.readyState===WebSocket.CLOSED)complete();
      }
    };
    function previewFailed(){
      if(canonicalRecorder){ready=false;parts=[];dispose();}
      else{event('error');recorder.discard();}
    }
    recorder.start=(timeslice)=>{
      recorder.state='recording';canonicalRecorder?.start(timeslice);if(disposed)return;
      try{
        source.connect(filter);filter.connect(worklet);worklet.connect(mute);mute.connect(context.destination);
        context.resume().catch(previewFailed);
      }catch{previewFailed();}
    };
    recorder.stop=()=>{if(recorder.state==='inactive'||ending||discarded)return;recorder.state='inactive';if(canonicalRecorder){ending=true;ready=false;dispose();canonicalRecorder.stop();return;}worklet.port.postMessage('finish');timer=setTimeout(complete,6000);};
    recorder.discard=()=>{if(done)return;discarded=true;if(canonicalRecorder&&canonicalRecorder.state!=='inactive'){canonicalRecorder.stop();dispose();return;}if(socket.readyState===WebSocket.OPEN)socket.send(JSON.stringify({type:'discard'}));complete();};
    return recorder;
  }
  return {supported,create,wav};
})();
