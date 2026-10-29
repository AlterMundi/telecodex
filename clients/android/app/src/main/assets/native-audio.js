"use strict";
/* Loaded only by the Android host, on its exact selected HTTPS main origin. */
window.NativeSpeechAudio=(()=>{
  const bridge=window.DaimonAudio,outputs=new Map(),streams=new Set(),encodedStreams=new Map();
  let serial=0,captureEpoch=0,controls,sessionEnabled=false;
  if(!bridge?.postMessage)return;
  const send=value=>bridge.postMessage(JSON.stringify(value));
  bridge.onmessage=event=>{try{const value=JSON.parse(event.data);if(value.type==="file"){window.NativeFiles?.receive(value);return;}if(value.type==="control"){controls?.[value.action]?.();return;}outputs.get(value.id)?.receive(value);}catch{}};
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
  function openStream(){
    if(encodedStreams.size>=3)throw Error("Native speech preparation bound reached");
    const url="native-speech:"+(++serial);
    let queue=[],bytes=0,sequence=0,owner,terminal,closed=false,decoderFailed=false;
    const chunk=(id,part)=>send({command:"stream_chunk",id,...part});
    const stream={url,
      append(raw,sha256){
        if(closed || terminal || !raw.length || raw.length>32768 || bytes+raw.length>8*1024*1024 || !/^[a-f0-9]{64}$/.test(sha256))throw Error("Invalid native stream chunk");
        const part={sequence:sequence++,data:btoa(String.fromCharCode(...raw)),sha256};bytes+=raw.length;
        if(owner)chunk(owner,part);else if(!decoderFailed)queue.push(part);
      },
      finish(value){
        if(closed || terminal || !bytes || value.audio_bytes!==bytes || !/^[a-f0-9]{64}$/.test(value.audio_sha256))throw Error("Incomplete native speech stream");
        terminal={bytes,sha256:value.audio_sha256};
        if(owner)send({command:"stream_end",id:owner,...terminal});
      },
      attach(id){
        if(closed || owner || decoderFailed)throw Error("Native speech source already selected");
        owner=id;send({command:"stream_begin",id});
        for(const part of queue)chunk(id,part);queue=[];
        if(terminal)send({command:"stream_end",id,...terminal});
      },
      detach(){
        // A decoder can reject an incomplete source while the verified HTTP
        // transport is still useful. Drop native ownership, not that transport.
        decoderFailed=true;if(owner)send({command:"stop",id:owner});owner=undefined;queue=[];
      },
      cancel(){
        if(closed)return;closed=true;
        if(owner)send({command:"stop",id:owner});
        queue=[];owner=undefined;encodedStreams.delete(url);
      }
    };
    encodedStreams.set(url,stream);return stream;
  }
  function mount(element){
    if(element.dataset.nativeAudio)return element;
    element.dataset.nativeAudio="progressive-v2";
    let source="",loadedSource="",id,muted=true,paused=true,ended=false,position=0,duration=NaN,loading,playingAttempt,selectedStream;
    const waiters=new Map();
    function fail(error){for(const waiter of waiters.values()){clearTimeout(waiter.timer);waiter.reject(error);}waiters.clear();}
    const wait=type=>new Promise((resolve,reject)=>{
      const timer=setTimeout(()=>{waiters.delete(type);reject(Error("Native audio deadline exceeded"));},30000);
      waiters.set(type,{resolve,reject,timer});
    });
    function command(command,extra={}){if(id)send({command,id,...extra});}
    function receive(value){
      if(value.id!==id)return;
      if(value.type!=="loaded" && Number.isFinite(value.position))position=value.position;
      if(Number.isFinite(value.duration))duration=value.duration;
      if(value.type==="error"){paused=true;selectedStream?.detach();fail(Error("Android audio unavailable"));element.dispatchEvent(new Event("error"));return;}
      if(value.type==="playing"){paused=false;ended=false;}
      if(value.type==="ended"){paused=true;ended=true;}
      if(["pause","focuslost"].includes(value.type))paused=true;
      const waiter=waiters.get(value.type);
      if(waiter){clearTimeout(waiter.timer);waiters.delete(value.type);waiter.resolve();}
      if(value.type!=="loaded")element.dispatchEvent(new Event(value.type==="focuslost"?"nativefocusloss":value.type));
    }
    function reset(preserveStream=false){
      command("stop");outputs.delete(id);id=undefined;loadedSource="";loading=undefined;playingAttempt=undefined;
      if(preserveStream)selectedStream?.detach();else selectedStream?.cancel();selectedStream=undefined;
      paused=true;ended=false;position=0;duration=NaN;fail(Error("Native audio source changed"));
    }
    async function load(){
      if(loadedSource===source)return;
      if(loading)return loading;
      const selected=source;
      const owned=id="speech_"+(++serial);outputs.set(owned,{receive,reset});
      loading=(async()=>{
        const candidate=encodedStreams.get(selected);
        if(candidate){
          selectedStream=candidate;
          const ready=wait("loaded");candidate.attach(owned);await ready;
          if(id!==owned)throw Error("Native audio source changed");
          loadedSource=selected;return;
        }
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
      try{await loading;}catch(error){if(id===owned)reset(true);throw error;}
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
        const playing=wait("playing");if(position>0)command("seek",{millis:Math.round(position*1000)});command("volume",{value:1});command("play");await playing;
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
  function setSession(enabled){
    enabled=!!enabled;if(enabled===sessionEnabled)return;sessionEnabled=enabled;
    send({command:"session",enabled});
  }
  function serviceLost(){background();controls?.stop?.();}
  function background(keepPlayback=false){
    captureEpoch++;
    window.dispatchEvent(new Event(keepPlayback?"nativecapturebackground":"pagehide"));
    if(!keepPlayback){
      for(const output of [...outputs.values()])output.reset();
      for(const stream of [...encodedStreams.values()])stream.cancel();
    }
    for(const stream of streams)for(const track of stream.getTracks())track.stop();streams.clear();
  }
  return {mount,background,openStream,progressive:true,backgroundPlayback:true,setSession,serviceLost,setControls:value=>{controls=value;}};
})();
