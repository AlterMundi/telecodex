"use strict";
/* Loaded only by the Android host, on its exact selected HTTPS main origin. */
window.NativeSpeechAudio=(()=>{
  const bridge=window.DaimonAudio,outputs=new Map(),streams=new Set();
  let serial=0,captureEpoch=0;
  if(!bridge?.postMessage)return;
  const send=value=>bridge.postMessage(JSON.stringify(value));
  bridge.onmessage=event=>{try{const value=JSON.parse(event.data);outputs.get(value.id)?.receive(value);}catch{}};
  send({command:"hello"});
  const devices=navigator.mediaDevices,original=devices?.getUserMedia?.bind(devices);
  if(original)devices.getUserMedia=async constraints=>{
    // Explicit track.stop() does not dispatch ended. Drop finished captures
    // before another recording so the foreground registry cannot accumulate.
    for(const previous of streams)if(previous.getTracks().every(track=>track.readyState==="ended"))streams.delete(previous);
    const epoch=captureEpoch,stream=await original(constraints);
    if(epoch!==captureEpoch){for(const track of stream.getTracks())track.stop();throw new DOMException("Capture interrupted by app lifecycle","AbortError");}
    streams.add(stream);
    stream.getTracks().forEach(track=>track.addEventListener("ended",()=>{if(stream.getTracks().every(t=>t.readyState==="ended"))streams.delete(stream);},{once:true}));
    return stream;
  };
  function mount(element){
    if(element.dataset.nativeAudio)return element;
    element.dataset.nativeAudio="buffered-v1";
    let source="",loadedSource="",id,muted=true,paused=true,ended=false,position=0,duration=NaN,loading,playingAttempt;
    const waiters=new Map();
    function fail(error){for(const waiter of waiters.values()){clearTimeout(waiter.timer);waiter.reject(error);}waiters.clear();}
    const wait=type=>new Promise((resolve,reject)=>{
      const timer=setTimeout(()=>{waiters.delete(type);reject(Error("Native audio deadline exceeded"));},30000);
      waiters.set(type,{resolve,reject,timer});
    });
    function command(command,extra={}){if(id)send({command,id,...extra});}
    function receive(value){
      if(value.id!==id)return;
      if(Number.isFinite(value.position))position=value.position;
      if(Number.isFinite(value.duration))duration=value.duration;
      if(value.type==="error"){paused=true;fail(Error("Android audio unavailable"));element.dispatchEvent(new Event("error"));return;}
      if(value.type==="playing"){paused=false;ended=false;}
      if(value.type==="ended"){paused=true;ended=true;}
      if(["pause","focuslost"].includes(value.type))paused=true;
      const waiter=waiters.get(value.type);
      if(waiter){clearTimeout(waiter.timer);waiters.delete(value.type);waiter.resolve();}
      if(value.type!=="loaded")element.dispatchEvent(new Event(value.type==="focuslost"?"nativefocusloss":value.type));
    }
    function reset(){
      command("stop");outputs.delete(id);id=undefined;loadedSource="";loading=undefined;playingAttempt=undefined;
      paused=true;ended=false;position=0;duration=NaN;fail(Error("Native audio source changed"));
    }
    async function load(){
      if(loadedSource===source)return;
      if(loading)return loading;
      const selected=source;
      const owned=id="speech_"+(++serial);outputs.set(owned,{receive,reset});
      loading=(async()=>{
        if(!selected.startsWith("blob:"))throw Error("Native audio requires verified in-page bytes");
        const response=await fetch(selected),bytes=new Uint8Array(await response.arrayBuffer());
        if(id!==owned || source!==selected)throw Error("Native audio source changed");
        if(!bytes.length || bytes.length>8*1024*1024)throw Error("Native audio exceeds playback bound");
        const digest=[...new Uint8Array(await crypto.subtle.digest("SHA-256",bytes))].map(n=>n.toString(16).padStart(2,"0")).join("");
        if(id!==owned)throw Error("Native audio source changed");
        send({command:"begin",id:owned,bytes:bytes.length,sha256:digest});
        let sequence=0;
        for(let offset=0;offset<bytes.length;offset+=32768){
          const part=bytes.subarray(offset,offset+32768);
          send({command:"chunk",id:owned,sequence:sequence++,data:btoa(String.fromCharCode(...part))});
        }
        const ready=wait("loaded");send({command:"ready",id:owned});await ready;
        if(id!==owned)throw Error("Native audio source changed");
        loadedSource=selected;
      })();
      try{await loading;}catch(error){if(id===owned)reset();throw error;}
      finally{if(id===owned)loading=undefined;}
    }
    element.play=()=>{
      // Existing canonical source validation invokes muted play first. No focus
      // or sound is acquired until that validation unmutes this exact source.
      if(muted)return Promise.resolve();
      if(playingAttempt)return playingAttempt;
      const selected=source;
      const attempt=(async()=>{
        await load();
        if(source!==selected || muted)throw Error("Native playback interrupted");
        const playing=wait("playing");command("seek",{millis:Math.round(position*1000)});command("volume",{value:1});command("play");await playing;
      })();
      playingAttempt=attempt;
      return attempt.finally(()=>{if(playingAttempt===attempt)playingAttempt=undefined;});
    };
    element.pause=()=>{paused=true;command("pause");};
    element.load=()=>reset();
    const remove=element.removeAttribute.bind(element);
    element.removeAttribute=name=>{if(name==="src"){reset();source="";}else remove(name);};
    Object.defineProperties(element,{
      src:{get:()=>source,set:value=>{reset();source=String(value);}},
      muted:{get:()=>muted,set:value=>{muted=!!value;command("volume",{value:muted?0:1});}},
      paused:{get:()=>paused},ended:{get:()=>ended},duration:{get:()=>duration},
      currentTime:{get:()=>position,set:value=>{if(Number.isFinite(value)&&value>=0){position=value;ended=false;command("seek",{millis:Math.round(value*1000)});}}}
    });
    return element;
  }
  function background(){
    captureEpoch++;
    window.dispatchEvent(new Event("pagehide"));
    for(const output of [...outputs.values()])output.reset();
    for(const stream of streams)for(const track of stream.getTracks())track.stop();streams.clear();
  }
  return {mount,background,progressive:false};
})();
