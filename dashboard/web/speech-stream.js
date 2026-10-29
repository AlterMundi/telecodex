"use strict";
/* Progressive encoded speech through the existing media element; no capture. */
window.SpeechStream=(()=>{
  const nativeAvailable=()=>window.NativeSpeechAudio?.progressive===true && typeof window.NativeSpeechAudio.openStream==="function";
  const available=()=>nativeAvailable() || !!window.MediaSource?.isTypeSupported?.("audio/mpeg");
  const digest=async bytes=>[...new Uint8Array(await crypto.subtle.digest("SHA-256",bytes))].map(n=>n.toString(16).padStart(2,"0")).join("");
  function open(response,signal){
    if(!available())throw Error("Progressive audio unavailable");
    const native=nativeAvailable()?window.NativeSpeechAudio.openStream():undefined;
    const media=native?undefined:new window.MediaSource(),url=native?.url||URL.createObjectURL(media),reader=response.body.getReader();
    let buffer,queue=[],all=[],bytes=0,sequence=0,complete=false,closed=false,failure,handler,decoderError,decoderHandler,verifiedBlob,words,alignmentOffset;
    const timer=setTimeout(()=>fail(Error("Speech stream deadline exceeded")),145000);
    function cancel(){
      if(closed)return;closed=true;clearTimeout(timer);signal?.removeEventListener("abort",cancel);
      Promise.resolve(reader.cancel()).catch(()=>{});queue=[];all=[];
      try{if(buffer?.updating)buffer.abort();}catch{}
      if(native)native.cancel();else URL.revokeObjectURL(url);
    }
    function fail(error){if(closed)return;failure=error;cancel();handler?.(error);}
    function decoderFail(error){
      if(closed || decoderError)return;
      decoderError=error;queue=[];
      try{if(buffer?.updating)buffer.abort();}catch{}
      decoderHandler?.(error);
    }
    function pump(){
      if(native || closed || decoderError || !buffer || buffer.updating || media.readyState!=="open")return;
      try{
        if(queue.length)buffer.appendBuffer(queue.shift());
        else if(complete)media.endOfStream();
      }catch(error){decoderFail(error);}
    }
    media?.addEventListener("sourceopen",()=>{
      if(closed)return;
      try{buffer=media.addSourceBuffer("audio/mpeg");buffer.addEventListener("updateend",pump);buffer.addEventListener("error",()=>decoderFail(Error("Audio decoding failed")));pump();}
      catch(error){decoderFail(error);}
    },{once:true});
    signal?.addEventListener("abort",cancel,{once:true});
    if(signal?.aborted)cancel();
    const done=(async()=>{
      let pending="",wire=0;const decoder=new TextDecoder("utf-8",{fatal:true});
      async function frame(line){
        if(closed)return;
        const value=JSON.parse(line);
        if(complete)throw Error("Audio after terminal frame");
        if(value.type==="audio"){
          if(value.sequence!==sequence || typeof value.data!=="string" || value.data.length>44000)throw Error("Invalid audio sequence");
          const raw=Uint8Array.from(atob(value.data),c=>c.charCodeAt(0));
          if(!raw.length || raw.length>32768 || await digest(raw)!==value.sha256)throw Error("Audio chunk integrity failed");
          if(closed)return;
          bytes+=raw.length;if(bytes>8*1024*1024)throw Error("Audio stream exceeds playback bound");
          sequence++;all.push(raw);
          if(native)native.append(raw,value.sha256);else if(!decoderError)queue.push(raw);
          pump();
        }else if(value.type==="end"){
          const merged=new Uint8Array(bytes);let offset=0;for(const part of all){merged.set(part,offset);offset+=part.length;}
          if(!bytes || value.audio_bytes!==bytes || await digest(merged)!==value.audio_sha256)throw Error("Incomplete audio stream");
          if(closed)return;
          const candidate=value.alignment?.words;
          if(Number.isSafeInteger(value.alignment?.offset) && value.alignment.offset>=0 && value.alignment.offset<=1000000 && Array.isArray(candidate) && candidate.length>0 && candidate.length<=512){
            let previous=value.alignment.offset,lastTime=0,valid=true;
            for(const word of candidate){
              if(!Array.isArray(word)||word.length!==4||word.some(n=>!Number.isSafeInteger(n))
                ||word[0]<previous||word[1]<=word[0]||word[1]>1001600||word[2]<lastTime||word[3]<=word[2]||word[3]>3600000){valid=false;break;}
              previous=word[1];lastTime=word[3];
            }
            if(valid){words=candidate;alignmentOffset=value.alignment.offset;}
          }
          verifiedBlob=new Blob([merged],{type:"audio/mpeg"});
          if(native)native.finish(value);
          complete=true;all=[];clearTimeout(timer);pump();
        }else throw Error("Speech outcome retained; no automatic retry");
      }
      try{
        while(!closed){
          const {value,done}=await reader.read();if(done)break;
          wire+=value.byteLength;if(wire>16*1024*1024)throw Error("Audio transport exceeds bound");
          pending+=decoder.decode(value,{stream:true});let end;
          while((end=pending.indexOf("\n"))>=0){if(end>65536)throw Error("Audio frame exceeds bound");const line=pending.slice(0,end);pending=pending.slice(end+1);if(line)await frame(line);}
          if(pending.length>65536)throw Error("Audio frame exceeds bound");
        }
        if(!closed){pending+=decoder.decode();if(pending.trim() || !complete)throw Error("Speech stream ended without confirmation");}
      }catch(error){fail(error);}
      return {complete:complete&&!failure,error:failure,blob:complete&&!failure&&!closed?verifiedBlob:undefined};
    })();
    function position(seconds){
      if(!words || !Number.isFinite(seconds) || seconds<0)return;
      const millis=seconds*1000;let offset=alignmentOffset;
      for(const word of words){if(word[3]>millis)break;offset=word[1];}
      return offset;
    }
    return {url,cancel,done,position,onError(fn){handler=fn;if(failure)fn(failure);},onDecoderError(fn){decoderHandler=fn;if(decoderError)fn(decoderError);}};
  }
  return {available,open};
})();
