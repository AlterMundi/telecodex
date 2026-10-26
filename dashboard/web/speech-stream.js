"use strict";
/* Progressive encoded speech through the existing media element; no capture. */
window.SpeechStream=(()=>{
  const available=()=>!!window.MediaSource?.isTypeSupported?.("audio/mpeg");
  const digest=async bytes=>[...new Uint8Array(await crypto.subtle.digest("SHA-256",bytes))].map(n=>n.toString(16).padStart(2,"0")).join("");
  function open(response,signal){
    if(!available())throw Error("Progressive audio unavailable");
    const media=new window.MediaSource(),url=URL.createObjectURL(media),reader=response.body.getReader();
    let buffer,queue=[],all=[],bytes=0,sequence=0,complete=false,closed=false,failure,handler;
    const timer=setTimeout(()=>fail(Error("Speech stream deadline exceeded")),145000);
    function cancel(){
      if(closed)return;closed=true;clearTimeout(timer);signal?.removeEventListener("abort",cancel);
      Promise.resolve(reader.cancel()).catch(()=>{});queue=[];all=[];
      try{if(buffer?.updating)buffer.abort();}catch{}
      URL.revokeObjectURL(url);
    }
    function fail(error){if(closed)return;failure=error;cancel();handler?.(error);}
    function pump(){
      if(closed || !buffer || buffer.updating || media.readyState!=="open")return;
      try{
        if(queue.length)buffer.appendBuffer(queue.shift());
        else if(complete)media.endOfStream();
      }catch(error){fail(error);}
    }
    media.addEventListener("sourceopen",()=>{
      if(closed)return;
      try{buffer=media.addSourceBuffer("audio/mpeg");buffer.addEventListener("updateend",pump);buffer.addEventListener("error",()=>fail(Error("Audio decoding failed")));pump();}
      catch(error){fail(error);}
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
          sequence++;all.push(raw);queue.push(raw);pump();
        }else if(value.type==="end"){
          const merged=new Uint8Array(bytes);let offset=0;for(const part of all){merged.set(part,offset);offset+=part.length;}
          if(!bytes || value.audio_bytes!==bytes || await digest(merged)!==value.audio_sha256)throw Error("Incomplete audio stream");
          if(closed)return;
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
      return {complete:complete&&!failure,error:failure};
    })();
    return {url,cancel,done,onError(fn){handler=fn;if(failure)fn(failure);}};
  }
  return {available,open};
})();
