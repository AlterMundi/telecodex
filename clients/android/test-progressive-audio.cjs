"use strict";
const assert=require("node:assert/strict"),vm=require("node:vm"),fs=require("node:fs"),{webcrypto,createHash}=require("node:crypto");
const hash=bytes=>createHash("sha256").update(bytes).digest("hex");
const turn=()=>new Promise(resolve=>setTimeout(resolve,10));
async function until(test){for(let n=0;n<100;n++){if(test())return;await turn();}throw Error("Synthetic bridge did not advance");}
function fixture(){
  const window=new EventTarget(),commands=[];
  const emit=(id,type,position=0)=>window.DaimonAudio.onmessage({data:JSON.stringify({id,type,position})});
  const loaded=new Set();
  window.DaimonAudio={postMessage(text){const value=JSON.parse(text);commands.push(value);
    if(value.command==="stream_chunk" && !loaded.has(value.id)){loaded.add(value.id);queueMicrotask(()=>emit(value.id,"loaded"));}
    if(value.command==="ready")queueMicrotask(()=>emit(value.id,"loaded"));
    if(value.command==="play")queueMicrotask(()=>emit(value.id,"playing"));
  }};
  const context={window,navigator:{},Event,EventTarget,DOMException,Uint8Array,Blob,URL,TextDecoder,crypto:webcrypto,fetch,btoa:s=>Buffer.from(s,"binary").toString("base64"),atob:s=>Buffer.from(s,"base64").toString("binary"),setTimeout,clearTimeout,console};
  vm.runInNewContext(fs.readFileSync(__dirname+"/app/src/main/assets/native-audio.js","utf8"),context);
  vm.runInNewContext(fs.readFileSync(__dirname+"/../../dashboard/web/speech-stream.js","utf8"),context);
  const element=new EventTarget();element.dataset={};element.removeAttribute=()=>{};
  const audio=window.NativeSpeechAudio.mount(element);
  const streams=[];
  function stream(){
    let reader,cancelled=false;
    const response={body:new ReadableStream({start(controller){reader=controller;},cancel(){cancelled=true;}})};
    const output=window.SpeechStream.open(response,new AbortController().signal);streams.push(output);
    return {output,write:value=>reader.enqueue(new TextEncoder().encode(JSON.stringify(value)+"\n")),close:()=>reader.close(),cancelled:()=>cancelled};
  }
  return {window,audio,commands,emit,stream,close:()=>streams.forEach(s=>s.cancel())};
}
const frame=(sequence,bytes)=>({type:"audio",sequence,data:Buffer.from(bytes).toString("base64"),sha256:hash(bytes)});
(async()=>{
  const f=fixture();
  try{
    assert(f.window.SpeechStream.available(),"Native progressive path does not require MediaSource");
    const first=f.stream(),prefix=Buffer.from([1,2,3]),tail=Buffer.from([4,5]);
    first.write(frame(0,prefix));await turn();
    f.audio.src=first.output.url;await f.audio.play();
    assert(!f.commands.some(v=>v.command==="stream_begin"),"Prefetch/muted validation cannot replace audible output");
    f.audio.muted=false;await f.audio.play();
    assert.equal(f.commands.filter(v=>v.command==="stream_begin").length,1);
    assert.equal(f.commands.find(v=>v.command==="stream_chunk").sha256,hash(prefix));
    assert.equal(f.audio.paused,false);assert(!f.commands.some(v=>v.command==="stream_end"),"Playback starts before terminal bytes");
    const next=f.stream();next.write(frame(0,Buffer.from([8])));await turn();
    assert.equal(f.commands.filter(v=>v.command==="stream_begin").length,1,"Preparing next source must not steal player/focus");
    first.write(frame(1,tail));const all=Buffer.concat([prefix,tail]);
    first.write({type:"end",audio_bytes:all.length,audio_sha256:hash(all)});first.close();
    const result=await first.output.done;assert(result.complete);assert.equal(result.blob.size,5);
    assert.equal(f.commands.find(v=>v.command==="stream_end").sha256,hash(all));
    const current=f.commands.find(v=>v.command==="stream_begin").id;
    f.emit(current,"ended",2);assert.equal(f.audio.ended,true);
    next.output.cancel();
    // Existing verified finite bytes remain usable after a decoder failure;
    // this is a blob read, never another provider request.
    const fallback=URL.createObjectURL(result.blob);f.audio.src=fallback;f.audio.currentTime=1;
    await f.audio.play();assert.equal(f.commands.find(v=>v.command==="begin").bytes,5);
    f.emit(current,"ended",2);assert.equal(f.audio.ended,false,"Late old stream cannot end finite recovery");URL.revokeObjectURL(fallback);
    const corrupted=f.stream();corrupted.write({...frame(0,prefix),sha256:"0".repeat(64)});corrupted.close();
    assert.equal((await corrupted.output.done).complete,false);
    assert.equal(f.commands.filter(v=>v.command==="stream_begin").length,1,"Corrupt unselected stream never reaches native output");
    const partial=f.stream();partial.write(frame(0,prefix));f.audio.src=partial.output.url;
    await f.audio.play();partial.output.cancel();await until(partial.cancelled);
    assert(f.commands.some(v=>v.command==="stop" && v.id===f.commands.filter(v=>v.command==="stream_begin").at(-1).id));
    console.log("Native progressive integration: verified prefix playback before EOF, quiet prefetch, finite recovery, stale output, integrity and cancellation passed.");
  }finally{f.close();f.audio.removeAttribute("src");}
})().catch(error=>{console.error(error);process.exitCode=1;});
