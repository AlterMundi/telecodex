// Actual PCM worklet and streaming recorder with synthetic devices/sockets.
const assert=require('node:assert/strict'),fs=require('node:fs'),vm=require('node:vm');
const {execFileSync}=require('node:child_process');
const uuid='10000000-0000-4000-8000-000000000001',binding='a'.repeat(64);
const pcm=(...samples)=>{const bytes=new ArrayBuffer(samples.length*2),view=new DataView(bytes);samples.forEach((v,i)=>view.setInt16(i*2,v,true));return bytes;};
function worklet(rate){
 let Processor;const messages=[];
 class Base{constructor(){this.port={postMessage:value=>messages.push(value)};}}
 vm.runInNewContext(fs.readFileSync('dashboard/web/voice-pcm.js','utf8'),{AudioWorkletProcessor:Base,sampleRate:rate,registerProcessor:(name,type)=>{assert.equal(name,'interaction-pcm');Processor=type;},ArrayBuffer,DataView});
 return {processor:new Processor(),messages};
}
function boot(fail){
 const contexts=[],sockets=[],worklets=[],timers=[];
 class Node{constructor(){this.links=[];this.disconnected=false;}connect(to){if(fail==='connect')throw Error('Preview connect unavailable');this.links.push(to);}disconnect(){this.disconnected=true;}}
 class Context{
  constructor(){this.destination={};this.closed=false;this.audioWorklet={addModule:async path=>{assert.equal(path,'assets/voice-pcm.js');if(fail==='module')throw Error('Module unavailable');}};contexts.push(this);}
  createMediaStreamSource(media){assert.equal(media.kind,'synthetic-media');this.source=new Node();return this.source;}
  createBiquadFilter(){if(fail==='filter')throw Error('Graph unavailable');this.filter=new Node();this.filter.frequency={value:0};return this.filter;}
  createGain(){this.mute=new Node();this.mute.gain={value:1};return this.mute;}
  async close(){this.closed=true;}async resume(){if(fail==='resume')throw Error('Preview resume unavailable');this.resumed=true;}
 }
 class Worklet extends Node{
  constructor(context,name){super();assert.equal(name,'interaction-pcm');this.controls=[];this.port={postMessage:value=>this.controls.push(value)};worklets.push(this);}
  emit(value){this.port.onmessage({data:value});}
 }
 class Socket{
  static OPEN=1;static CLOSED=3;
  constructor(url){if(fail==='socket')throw Error('Socket unavailable');this.url=url.href;this.readyState=0;this.bufferedAmount=0;this.sent=[];sockets.push(this);}
  send(data){if(this.failSend)throw Error('Connection lost');assert.equal(this.readyState,1);this.sent.push(data);}
  open(){this.readyState=1;this.onopen();}
  message(value){this.onmessage({data:JSON.stringify(value)});}
  close(){if(this.readyState===3)return;this.readyState=3;queueMicrotask(()=>this.onclose?.());}
 }
 const window={AudioWorkletNode:Worklet,AudioContext:Context,WebSocket:Socket,location:{href:'https://example.invalid/interaction/'}};
 vm.runInNewContext(fs.readFileSync('dashboard/web/voice-stream.js','utf8'),{window,AudioWorkletNode:Worklet,WebSocket:Socket,URL,Blob,EventTarget,Event,ArrayBuffer,DataView,setTimeout:(fn,ms)=>{const t={fn,ms};timers.push(t);return t;},clearTimeout:t=>{if(t)t.cleared=true;}});
 const captions=[],events=[];
 return {window,contexts,sockets,worklets,timers,captions,events,async create(canonicalRecorder,options={}){const recorder=await window.StreamingVoice.create({kind:'synthetic-media'},{id:uuid,binding,canonicalRecorder,headers:()=>({'X-Dashboard-CSRF':'synthetic-csrf',Authorization:'Bearer synthetic-session'}),onPartial:value=>captions.push(value),...options});recorder.addEventListener('dataavailable',e=>events.push({type:'data',data:e.data}));recorder.addEventListener('stop',()=>events.push({type:'stop'}));return recorder;}};
}
(async()=>{
 // Resampling counts and cross-block continuity at common mobile capture rates.
 for(const rate of [16000,44100,48000]){
  const run=step=>{const w=worklet(rate);for(let i=0;i<rate;i+=step){const size=Math.min(step,rate-i),block=Float32Array.from({length:size},(_,j)=>Math.sin((i+j)*2*Math.PI*230/rate));assert.equal(w.processor.process([[block]]),true);}w.processor.port.onmessage({data:'finish'});assert.equal(w.processor.process([[new Float32Array(128)]]),false);assert(w.messages.at(-1).finished);return Buffer.concat(w.messages.filter(m=>m.pcm).map(m=>Buffer.from(m.pcm)));};
  const small=run(128),large=run(1177);assert.equal(small.length,32000);assert(small.equals(large),`Resampling must not jump at ${rate}Hz chunk boundaries`);
 }
 // A segment boundary flushes the resampler without stopping or resetting it.
 for(const rate of [16000,44100,48000]){
  const w=worklet(rate),reference=worklet(rate);let before=0;
  for(let i=0;i<rate;i+=128){
   const block=Float32Array.from({length:Math.min(128,rate-i)},(_,j)=>Math.sin((i+j)*2*Math.PI*230/rate));
   w.processor.process([[block]]);reference.processor.process([[block]]);
   if(i>rate/3&&!before){
    w.processor.port.onmessage({data:{boundary:'first:end'}});
    const mark=w.messages.at(-1);assert.equal(mark.boundary,'first:end');before=mark.totalBytes;
    assert.equal(before,Buffer.concat(w.messages.filter(m=>m.pcm).map(m=>Buffer.from(m.pcm))).length);
   }
  }
  w.processor.port.onmessage({data:{boundary:'second:end'}});reference.processor.port.onmessage({data:'finish'});
  assert(w.processor.active,'A boundary must leave the microphone graph running');
  assert.equal(w.messages.at(-1).totalBytes,32000);assert(before>0&&before<32000);
  assert(Buffer.concat(w.messages.filter(m=>m.pcm).map(m=>Buffer.from(m.pcm))).equals(Buffer.concat(reference.messages.filter(m=>m.pcm).map(m=>Buffer.from(m.pcm)))));
  const count=w.messages.length;w.processor.port.onmessage({data:{boundary:'bad token'}});assert.equal(w.messages.length,count);
 }
 const clamp=worklet(16000);clamp.processor.process([[Float32Array.of(-2,-1,0,1,2)]]);clamp.processor.port.onmessage({data:'finish'});assert.deepEqual([...new Int16Array(clamp.messages[0].pcm)],[-32767,-32767,0,32767,32767]);
 const canonical=boot(),samples=pcm(0,-32767,15000,32767);
 const wav=Buffer.from(await canonical.window.StreamingVoice.wav([samples]).arrayBuffer());
 const expected=execFileSync('python3',['-c',"import sys,io,wave; o=io.BytesIO(); w=wave.open(o,'wb'); w.setnchannels(1); w.setsampwidth(2); w.setframerate(16000); w.writeframes(sys.stdin.buffer.read()); w.close(); sys.stdout.buffer.write(o.getvalue())"],{input:Buffer.from(samples)});
 assert(wav.equals(expected),'Browser WAV bytes must equal the server/native preparation identity');
 // Foreground activity stays separate from ASR, media capture and admission.
 const activity=boot(),activityValues=[];let activityFailures=0;
 const opening=activity.window.StreamingVoice.openActivity({headers:()=>({'X-Dashboard-CSRF':'csrf'}),binding,id:uuid,onActivity:v=>activityValues.push(v),onUnavailable:()=>activityFailures++});
 const as=activity.sockets[0];as.open();assert.equal(as.url,'wss://example.invalid/interaction/api/interaction/voice/activity');
 assert.deepEqual(JSON.parse(as.sent[0]),{id:uuid,binding,csrf:'csrf'});
 as.message({type:'ready',id:uuid,analysis_only:true});const analysis=await opening;
 assert.equal(activity.contexts.length,0,'Analysis transport cannot request or open a microphone');
 const snapshot=(offset,generation=1)=>({schema:'telecodex.turn-activity/v1',type:'activity',generation,audio_bytes:offset,speech_seen:true,speaking:false,
   onset:{generation,audio_bytes:offset},endpoint:{generation,audio_bytes:offset,probability:.9,authority:'analysis_only'}});
 assert(analysis.feed(new ArrayBuffer(3200)));as.message({type:'turn_activity',id:uuid,activity:snapshot(3200)});
 assert.equal(activityValues.length,1);assert(analysis.boundary({generation:1,audio_bytes:3200}));assert(!analysis.boundary({generation:1,audio_bytes:3200}));
 assert.deepEqual(JSON.parse(as.sent.at(-1)),{boundary:{generation:1,audio_bytes:3200}});
 as.message({type:'boundary',id:uuid,activity:{schema:'telecodex.turn-activity/v1',type:'boundary',generation:1,audio_bytes:3200}});
 assert(analysis.feed(new ArrayBuffer(3200)));as.message({type:'turn_activity',id:uuid,activity:snapshot(6400,2)});assert.equal(activityValues.length,2);
 assert.equal(activity.sockets.length,1);assert.equal(activity.events.length,0,'Activity never admits a recording');
 analysis.close();analysis.close();as.message({type:'turn_activity',id:uuid,activity:snapshot(6400,2)});
 assert.equal(activityValues.length,2);assert.equal(activityFailures,0);
 for(const failure of ['backpressure','send','bad-event','unavailable','limit']){
  const unit=boot();let failed=0;const opening=unit.window.StreamingVoice.openActivity({headers:()=>({}),binding,id:uuid,onUnavailable:()=>failed++});
  const s=unit.sockets[0];s.open();s.message({type:'ready',id:uuid,analysis_only:true});const a=await opening;
  if(failure==='backpressure'){s.bufferedAmount=300000;assert(!a.feed(samples));}
  if(failure==='send'){s.failSend=true;assert(!a.feed(samples));}
  if(failure==='bad-event')s.message({type:'turn_activity',id:uuid,activity:snapshot(9999)});
  if(failure==='unavailable')s.message({type:'unavailable'});
  if(failure==='limit')unit.timers.find(t=>t.ms===300000&&!t.cleared).fn();
  assert(a.closed);assert.equal(failed,1);assert.equal(unit.sockets.length,1,'No automatic analyzer reconnect');
 }
 for(const failure of ['abort','timeout','wrong-id']){
  const unit=boot(),abort=new AbortController();
  const opening=unit.window.StreamingVoice.openActivity({headers:()=>({}),binding,id:uuid,signal:abort.signal});
  const s=unit.sockets[0];s.open();
  if(failure==='abort')abort.abort();
  if(failure==='timeout')unit.timers.find(t=>t.ms===35000&&!t.cleared).fn();
  if(failure==='wrong-id')s.message({type:'ready',id:'another',analysis_only:true});
  await assert.rejects(opening);assert.equal(s.readyState,3);assert.equal(unit.contexts.length,0);
 }
 const f=boot(),recorder=await f.create();assert(f.window.StreamingVoice.supported());recorder.start();const socket=f.sockets[0],node=f.worklets[0];socket.open();
 assert.equal(socket.url,'wss://example.invalid/interaction/api/interaction/voice/live');assert(!socket.url.includes('synthetic-session'));
 assert.deepEqual(JSON.parse(socket.sent[0]),{id:uuid,binding,csrf:'synthetic-csrf',authorization:'Bearer synthetic-session'});
 node.emit({pcm:samples});assert.equal(socket.sent.length,1,'Wait for server/provider readiness before sending PCM');
 socket.message({type:'ready'});assert.equal(socket.sent[1],samples);
 socket.message({type:'partial',text:'provisional'});socket.message({type:'partial',text:'corrected'});assert.deepEqual(f.captions,['provisional','corrected']);assert.equal(f.events.length,0,'Captions never submit an original');
 recorder.stop();assert.deepEqual(node.controls,['finish']);assert.equal(f.events.length,0,'Wait for final worklet PCM before admission');node.emit({pcm:pcm(100)});node.emit({finished:true});assert.deepEqual(JSON.parse(socket.sent.at(-1)),{type:'finish'});assert.equal(f.events.length,0,'Preparation final is separate from original submission');
 socket.message({type:'prepared',id:uuid,text:'Final recognized words'});assert.equal(recorder.preparedText,'Final recognized words');assert.deepEqual(f.events.map(e=>e.type),['data','stop']);const captured=Buffer.from(await f.events[0].data.arrayBuffer());assert.equal(captured.length,44+samples.byteLength+2);assert.equal(f.contexts[0].mute.gain.value,0);assert(f.contexts[0].closed);assert.equal(socket.readyState,3);
 node.emit({finished:true});socket.message({type:'prepared',id:uuid});assert.equal(f.events.length,2,'A late event must not create a second recording');
 const direct=boot();let endpoints=0,detectorFailures=0;
 const continuous=await direct.create(undefined,{turnDetection:true,onEndpoint:()=>endpoints++,onDetectionUnavailable:()=>detectorFailures++});
 continuous.start();const ds=direct.sockets[0],dw=direct.worklets[0];ds.open();ds.message({type:'ready'});
 assert.equal(JSON.parse(ds.sent[0]).turn_detection,true);assert.deepEqual(dw.controls,['observe']);
 dw.emit({activityBytes:640,totalBytes:3200});dw.emit({pcm:new ArrayBuffer(3200)});
 const endpoint=(generation,offset=3200)=>({type:'turn_activity',id:uuid,activity:{generation,speaking:false,speech_seen:true,audio_bytes:offset,endpoint:{generation,audio_bytes:offset,probability:.9,authority:'analysis_only'}}});
 ds.message(endpoint(1));assert.equal(endpoints,0,'Wait for recognition without losing a still-current verdict');
 ds.message({type:'partial',text:'Una frase terminada'});assert.equal(endpoints,1);ds.message(endpoint(1));assert.equal(endpoints,1,'Duplicate verdict is inert');
 dw.emit({activityBytes:3400,totalBytes:3840});ds.message(endpoint(2));assert.equal(endpoints,1,'Speech after the scored PCM fences a late verdict');
 ds.message({...endpoint(3),id:'other-capture'});assert.equal(endpoints,1);
 ds.message({type:'turn_detection_unavailable'});assert.equal(detectorFailures,1);
 dw.emit({pcm:new ArrayBuffer(3200)});dw.emit({activityBytes:3400,totalBytes:6400});ds.message(endpoint(4,6400));assert.equal(endpoints,1,'Failed analysis never resumes automatically');
 continuous.discard();ds.message(endpoint(5,6400));assert.equal(endpoints,1);
 // Two independent ASR finalizations share one still-running PCM graph.
 // A permanent coordinator tap sees the between-turn samples too; it owns
 // bounded pre-speech retention rather than either individual utterance.
 const shared=boot(),source=await shared.window.StreamingVoice.openCapture({kind:'synthetic-media'}),observed=[];
 const untap=source.subscribe(value=>observed.push(value));
 const first=await shared.create(undefined,{captureSource:source}),secondId='20000000-0000-4000-8000-000000000002';
 const second=await shared.create(undefined,{captureSource:source,id:secondId}),clips=[];
 first.addEventListener('dataavailable',e=>clips.push(['first',e.data]));second.addEventListener('dataavailable',e=>clips.push(['second',e.data]));
 const sw=shared.worklets[0];assert.equal(shared.contexts.length,1);assert.equal(shared.sockets.length,2);
 await assert.rejects(shared.create(undefined,{captureSource:source}));assert.equal(shared.sockets.pop().readyState,3,'A rejected excess consumer closes only its own socket');
 assert(!source.closed);
 for(const socket of shared.sockets){socket.open();socket.message({type:'ready'});}
 first.start();sw.emit({boundary:uuid+':begin',totalBytes:100});sw.emit({pcm:samples,totalBytes:108});
 first.stop();sw.emit({pcm:pcm(200),totalBytes:110});sw.emit({boundary:uuid+':end',totalBytes:110});
 assert.deepEqual(JSON.parse(shared.sockets[0].sent.at(-1)),{type:'finish'});
 assert.equal(shared.sockets[1].sent.length,1,'Another utterance boundary cannot finish an unstarted provider');
 assert(!observed.at(-1).finished,'Consumers cannot mutate the shared source event');
 sw.emit({pcm:pcm(300),totalBytes:112}); // Retained by the coordinator, not appended to the stopped original.
 second.start();sw.emit({boundary:secondId+':begin',totalBytes:112});sw.emit({pcm:pcm(400),totalBytes:114});
 shared.sockets[0].message({type:'prepared',id:uuid,text:'first'});assert(!shared.contexts[0].closed);
 second.stop();sw.emit({boundary:secondId+':end',totalBytes:114});shared.sockets[1].message({type:'prepared',id:secondId,text:'second'});
 assert.equal(clips.length,2);assert.equal(clips[0][1].size,44+10);assert.equal(clips[1][1].size,44+2);
 assert.deepEqual([...new Int16Array((await clips[1][1].arrayBuffer()).slice(44))],[400]);
 assert.equal(observed.filter(v=>v.pcm).length,4);assert(!shared.contexts[0].closed);assert.equal(shared.contexts[0].resumed,true);
 assert(sw.controls.some(v=>v.boundary===uuid+':end'));assert(!sw.controls.includes('finish'));
 untap();source.close();source.close();assert(shared.contexts[0].closed);assert(source.closed);
 await assert.rejects(shared.create({},{captureSource:source}));
 for(const why of ['explicit-close','wall-limit','sample-limit']){
  const bounded=boot(),graph=await bounded.window.StreamingVoice.openCapture({kind:'synthetic-media'});
  const r=await bounded.create(undefined,{captureSource:graph});r.start();const w=bounded.worklets[0];
  w.emit({boundary:uuid+':begin',totalBytes:0});w.emit({pcm:samples,totalBytes:samples.byteLength});
  if(why==='explicit-close')graph.close();
  if(why==='wall-limit')bounded.timers.find(t=>t.ms===300000&&!t.cleared).fn();
  if(why==='sample-limit')w.emit({pcm:pcm(999),totalBytes:300*32000+2});
  assert(graph.closed);assert.equal(bounded.events.length,2);assert.equal(r.captureStop,'audio_flush_unconfirmed','Reuse the existing native limited-original contract');assert.equal(r.captureError,why==='explicit-close'?'capture_closed':'session_limit');
  assert.equal(bounded.events[0].data.size,44+samples.byteLength,'Session exit retains the accumulated fragment, without claiming a final flush');
  assert.equal(bounded.sockets.length,1);assert(!bounded.sockets[0].sent.some(v=>typeof v==='string'&&JSON.parse(v).type==='finish'));
  w.emit({pcm:pcm(900)});assert.equal(bounded.events.length,2);
 }
 const cancel=boot(),cancelRecorder=await cancel.create();cancelRecorder.start();cancel.sockets[0].open();cancel.sockets[0].message({type:'ready'});cancel.worklets[0].emit({pcm:samples});cancelRecorder.discard();assert.deepEqual(cancel.events.map(e=>e.type),['stop']);assert.deepEqual(JSON.parse(cancel.sockets[0].sent.at(-1)),{type:'discard'});assert(!cancel.sockets[0].sent.some(v=>typeof v==='string'&&JSON.parse(v).type==='finish'));
 for(const mode of ['disconnect','send-error','backpressure','unavailable','wrong-prepared-id']){
  const lost=boot(),device=await lost.create(),s=lost.sockets[0],w=lost.worklets[0];device.start();s.open();s.message({type:'ready'});
  if(mode==='send-error')s.failSend=true;if(mode==='backpressure')s.bufferedAmount=300000;
  w.emit({pcm:samples});if(mode==='disconnect')s.close();if(mode==='unavailable')s.message({type:'unavailable'});
  device.stop();w.emit({finished:true});if(mode==='wrong-prepared-id')s.message({type:'prepared',id:'other'});await new Promise(yes=>setImmediate(yes));
  assert.deepEqual(lost.events.map(e=>e.type),['data','stop'],`${mode} must retain one complete local fallback`);assert(Buffer.from(await lost.events[0].data.arrayBuffer()).equals(wav));assert.equal(lost.sockets.length,1,'Never reconnect/replay provider audio');
 }
 const stalled=boot(),stalledDevice=await stalled.create();stalledDevice.start();stalled.sockets[0].open();stalled.sockets[0].message({type:'ready'});stalled.worklets[0].emit({pcm:samples});stalledDevice.stop();
 stalled.timers.find(t=>t.ms===6000&&!t.cleared).fn();
 assert.equal(stalledDevice.captureStop,'audio_flush_unconfirmed','A missing final worklet flush must not present its partial WAV as a complete recording');
 assert.deepEqual(stalled.events.map(e=>e.type),['data','stop']);
 assert(Buffer.from(await stalled.events[0].data.arrayBuffer()).equals(wav),'Preserve the actual accumulated fragment');
 stalled.worklets[0].emit({pcm:pcm(900)});stalled.worklets[0].emit({finished:true});
 assert.equal(stalled.events.length,2,'A late flush must not change an offered recording identity');
 const delayed=boot(),delayedDevice=await delayed.create();delayedDevice.start();delayed.worklets[0].emit({pcm:samples});delayedDevice.stop();delayed.worklets[0].emit({finished:true});delayed.timers.find(t=>t.ms===5000&&!t.cleared).fn();assert.equal(delayed.events[0].data.size,wav.length,'A provider that never opens cannot lose finalized audio');
 for(const failure of ['module','filter','socket']){const setup=boot(failure);await assert.rejects(setup.create());assert(setup.contexts[0].closed,`${failure} must release its owned audio graph`);}
 class Canonical extends EventTarget{
  constructor(){super();this.mimeType='audio/webm;codecs=opus';this.state='inactive';}
  start(slice){this.state='recording';this.slice=slice;}
  stop(){this.state='inactive';queueMicrotask(()=>{const e=new Event('dataavailable');Object.defineProperty(e,'data',{value:new Blob(['compressed-original'],{type:this.mimeType})});this.dispatchEvent(e);this.dispatchEvent(new Event('stop'));});}
 }
 for(const network of ['connected','disconnected','not-ready']){
  const captions=boot(),canonical=new Canonical(),capture=await captions.create(canonical);capture.start(250);const socket=captions.sockets[0];socket.open();
  assert.equal(JSON.parse(socket.sent[0]).mode,'captions');
  if(network!=='not-ready')socket.message({type:'ready'});
  captions.worklets[0].emit({pcm:samples});socket.message({type:'partial',text:'provisional, never canonical'});
  if(network==='disconnected'){socket.close();await new Promise(yes=>setImmediate(yes));}
  capture.stop();await new Promise(yes=>setImmediate(yes));
  assert.deepEqual(captions.events.map(e=>e.type),['data','stop']);assert.equal(await captions.events[0].data.text(),'compressed-original');
  assert.equal(captions.events[0].data.type,'audio/webm;codecs=opus');assert.equal(canonical.slice,250);
  assert(!socket.sent.some(value=>typeof value==='string'&&JSON.parse(value).type==='finish'),'Captions never finalize or replace the original');
  assert(!captions.timers.some(t=>!t.cleared&&t.ms===5000),'Send never waits for a provisional final');assert(captions.contexts[0].closed);
 }
 for(const failure of ['resume','connect']){
  const preview=boot(failure),original=new Canonical(),capture=await preview.create(original);
  let errors=0;capture.addEventListener('error',()=>errors++);capture.start(250);
  await new Promise(yes=>setImmediate(yes));
  assert.equal(original.state,'recording',`${failure} must preserve the original recording`);
  assert.equal(capture.state,'recording');assert.equal(errors,0);assert.equal(preview.events.length,0);
  assert(preview.contexts[0].closed);capture.stop();await new Promise(yes=>setImmediate(yes));
  assert.deepEqual(preview.events.map(e=>e.type),['data','stop']);
  assert.equal(await preview.events[0].data.text(),'compressed-original');assert.equal(preview.sockets.length,1);
 }
 const discarded=boot(),original=new Canonical(),capture=await discarded.create(original);capture.start(250);capture.discard();await new Promise(yes=>setImmediate(yes));
 assert.deepEqual(discarded.events.map(e=>e.type),['stop'],'Discard never releases a canonical clip for upload');
 console.log('voice stream UI: resampling, exact WAV identity, captions/finish/discard, fallback and cleanup passed');
})().catch(error=>{console.error(error);process.exitCode=1;});
