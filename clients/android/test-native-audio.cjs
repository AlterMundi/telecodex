"use strict";
const assert=require("node:assert/strict"),vm=require("node:vm"),fs=require("node:fs"),{webcrypto}=require("node:crypto");
const script=fs.readFileSync(__dirname+"/app/src/main/assets/native-audio.js","utf8");
const flush=()=>new Promise(resolve=>setImmediate(resolve));
async function fixture(capture){
  const commands=[],window=new EventTarget(),tracks=[{stopped:false,readyState:"live",addEventListener(){},stop(){this.stopped=true;}}];
  const bridge={postMessage(text){
    const value=JSON.parse(text);commands.push(value);
    if(value.command==="ready")queueMicrotask(()=>bridge.onmessage({data:JSON.stringify({id:value.id,type:"loaded",duration:2})}));
    if(value.command==="play")queueMicrotask(()=>bridge.onmessage({data:JSON.stringify({id:value.id,type:"playing",position:0,duration:2})}));
  }};
  window.DaimonAudio=bridge;
  const context={window,navigator:{mediaDevices:{getUserMedia:capture || (async()=>({getTracks:()=>tracks}))}},Event,EventTarget,DOMException,Uint8Array,Blob,crypto:webcrypto,fetch:async()=>({arrayBuffer:async()=>new Uint8Array([1,2,3,4]).buffer}),btoa:s=>Buffer.from(s,"binary").toString("base64"),setTimeout,clearTimeout,Map,Set,console};
  vm.runInNewContext(script,context);
  const element=new EventTarget();element.dataset={};element.removeAttribute=()=>{};
  const audio=window.NativeSpeechAudio.mount(element);
  return {audio,commands,window,bridge,context,tracks,event(type,id=commands.find(v=>v.command==="begin")?.id){bridge.onmessage({data:JSON.stringify({id,type,position:1,duration:2})});}};
}
(async()=>{
  const f=await fixture();f.audio.src="blob:verified";
  await f.audio.play();assert(!f.commands.some(v=>v.command==="begin"),"Muted canonical validation never acquires native output");
  f.audio.muted=false;await Promise.all([f.audio.play(),f.audio.play()]);
  assert.equal(f.commands.filter(v=>v.command==="begin").length,1);
  assert.equal(f.commands.filter(v=>v.command==="play").length,1);
  assert.equal(f.audio.paused,false);assert.equal(f.audio.duration,2);
  const begin=f.commands.find(v=>v.command==="begin");
  assert.equal(begin.bytes,4);assert.match(begin.sha256,/^[a-f0-9]{64}$/);
  assert.equal(f.commands.find(v=>v.command==="chunk").data,"AQIDBA==");
  f.audio.pause();assert.equal(f.audio.paused,true);
  f.event("ended");assert.equal(f.audio.ended,true);
  f.audio.currentTime=0;assert.equal(f.audio.ended,false);
  let lost=0;f.audio.addEventListener("nativefocusloss",()=>lost++);f.event("focuslost");assert.equal(lost,1);
  f.audio.src="blob:next";f.event("ended",begin.id);assert.equal(f.audio.ended,false,"Late native completion cannot end replacement source");
  await f.context.navigator.mediaDevices.getUserMedia({audio:true});f.window.NativeSpeechAudio.background();assert(f.tracks[0].stopped);
  const invalid=await fixture();invalid.audio.src="https://foreign.example/audio";invalid.audio.muted=false;
  await assert.rejects(invalid.audio.play(),/verified in-page/);assert(!invalid.commands.some(v=>v.command==="begin"));
  const stale=await fixture();let resolve;
  stale.context.fetch=()=>new Promise(r=>resolve=r);stale.audio.src="blob:old";stale.audio.muted=false;
  const pending=stale.audio.play();stale.audio.src="blob:new";
  resolve({arrayBuffer:async()=>new Uint8Array([1]).buffer});await assert.rejects(pending,/source changed/);
  assert(!stale.commands.some(v=>v.command==="begin"));
  let grant;const delayed=await fixture(()=>new Promise(resolve=>grant=resolve));
  const capture=delayed.context.navigator.mediaDevices.getUserMedia({audio:true});
  delayed.window.NativeSpeechAudio.background();grant({getTracks:()=>delayed.tracks});
  await assert.rejects(capture,{name:"AbortError"});assert(delayed.tracks[0].stopped,"Late capture grant must close tracks after backgrounding");
  console.log("Native audio adapter: verified bytes, muted validation, dedup, focus loss, stale source and capture cleanup passed.");
})().catch(error=>{console.error(error);process.exitCode=1;});
