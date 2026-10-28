// Foreground coordinator with synthetic devices, analysis and retained originals.
const assert=require('node:assert/strict'),fs=require('node:fs'),vm=require('node:vm'),crypto=require('node:crypto').webcrypto;
const tick=()=>new Promise(yes=>setImmediate(yes));
const until=async f=>{for(let i=0;i<100;i++){if(f())return;await new Promise(yes=>setTimeout(yes,5));}throw Error('Expected continuous voice transition');};
const deferred=()=>{let resolve;return {promise:new Promise(yes=>resolve=yes),resolve};};
function boot(){
 const events={},timers=[],analyses=[],sources=[],recorders=[],medias=[],retained=[],commits=[],sent=[],reviews=[],states=[],captions=[];
 let binding='a'.repeat(64),interrupts=0,permission,commitHold,questions={old:'v1'};
 const document={hidden:false,addEventListener:(n,f)=>events[n]=f};
 const window={crypto,addEventListener:(n,f)=>events[n]=f,navigator:{mediaDevices:{async getUserMedia(options){
  assert.equal(options.audio.echoCancellation,true);assert.equal(options.video,false);
  if(permission){const p=permission;permission=undefined;return p.promise;}
  const value={stops:0,getTracks(){return [{stop:()=>value.stops++}];}};medias.push(value);return value;
 }}}};
 window.StreamingVoice={
  async openActivity(options){
   const value={options,pcm:[],closed:false,feed(pcm){this.pcm.push(pcm);return !this.closed;},boundary(v){this.boundaries.push(v);return true;},boundaries:[],close(){this.closed=true;},emit(v){options.onActivity(v);}};analyses.push(value);return value;
  },
  async openCapture(media){
   const listeners=new Set(),s={media,bytes:0,acoustic(activityBytes,totalBytes){for(const f of [...listeners])f({activityBytes,totalBytes});},closed:false,started:false,subscribe(fn){listeners.add(fn);return ()=>listeners.delete(fn);},observe(){},start(){this.started=true;},close(){this.closed=true;for(const f of [...listeners])f({captureError:'capture_closed'});listeners.clear();},emit(n,value=1){const pcm=new ArrayBuffer(n);new Uint8Array(pcm).fill(value);this.bytes+=n;for(const f of [...listeners])f({pcm,totalBytes:this.bytes});}};sources.push(s);return s;
  },
  async create(_media,options){
   const events={},s=options.captureSource;
   const r={options,parts:[],state:'inactive',addEventListener:(n,f)=>events[n]=f,
    start(){this.state='recording';this.parts.push(...options.initialPCM(s.bytes));this.unsubscribe=s.subscribe(v=>{if(v.pcm)this.parts.push(v.pcm);});s.start();},
    stop(){if(this.state==='inactive')return;this.state='inactive';this.unsubscribe?.();options.onFlushed(s.bytes);},
    discard(){this.state='inactive';this.unsubscribe?.();},
    partial(text){options.onPartial(text);},
    complete(text){if(text)this.preparedText=text;events.dataavailable({data:new Blob(this.parts,{type:'audio/wav'})});events.stop();}
   };recorders.push(r);return r;
  }
 };
 vm.runInNewContext(fs.readFileSync('dashboard/web/voice-session.js','utf8'),{window,document,AbortController,ArrayBuffer,JSON,Map,Promise,Infinity,setTimeout:(fn,ms)=>{const t={fn,ms};timers.push(t);return t;},clearTimeout:t=>{if(t)t.closed=true;}});
 const session=window.ContinuousConversation.create({headers:()=>({}),getBinding:()=>binding,getQuestions:()=>questions,getPlayback:()=>({source:'synthetic-reply',position_millis:123}),
  async retain(original){const v={...original,audio_sha256:Buffer.from(await crypto.subtle.digest('SHA-256',await original.blob.arrayBuffer())).toString('hex')};retained.push(v);return v;},
  async commit(original){commits.push(original);if(commitHold)return commitHold.promise;return {confirmed:true,value:{id:original.id,voice:{audio_sha256:original.audio_sha256}}};},
  onState:v=>states.push(v),onCaption:v=>captions.push(v),onInterrupt:()=>interrupts++,onSent:v=>sent.push(v),onReview:v=>reviews.push(v)
 });
 const activity=(generation,onset,endpoint)=>analyses.at(-1).emit({schema:'telecodex.turn-activity/v1',type:'activity',generation,audio_bytes:sources.at(-1).bytes,speech_seen:true,speaking:!endpoint,
  ...(onset?{onset:{generation,audio_bytes:sources.at(-1).bytes}}:{}),...(endpoint?{endpoint:{generation,audio_bytes:sources.at(-1).bytes,probability:.9,authority:'analysis_only'}}:{})});
 return {session,analyses,sources,recorders,medias,retained,commits,sent,reviews,states,captions,document,events,timers,activity,interrupts:()=>interrupts,
  questions:v=>questions=v,bind:v=>binding=v,permission:()=>{permission=deferred();return permission;},holdCommit:()=>{commitHold=deferred();return commitHold;}};
}
(async()=>{
 const f=boot();assert(await f.session.start());assert(f.session.active);assert.equal(f.medias.length,1);assert.equal(f.analyses.length,1);
 f.sources[0].emit(3200);f.recorders[0].partial('Una frase');assert.equal(f.interrupts(),0,'Captions cannot interrupt or admit work');
 f.activity(1,true,false);assert(f.session.floor);assert.equal(f.interrupts(),1);assert.equal(f.commits.length,0);
 f.activity(1,true,false);assert.equal(f.interrupts(),1,'A repeated onset does not interrupt twice');
 f.activity(1,false,true);assert(!f.session.floor);
 f.sources[0].emit(32,2); // Next PCM arrives before the next ASR recorder is armed.
 await until(()=>f.recorders.length===2&&f.recorders[1].state==='recording');
 assert.equal(f.sources.length,1);assert.equal(f.medias.length,1);assert.equal(f.analyses.length,1);
 assert.equal(f.recorders[1].parts.reduce((n,p)=>n+p.byteLength,0),32,'The boundary gap belongs to the next exact original');
 f.recorders[0].complete('Una frase');await until(()=>f.sent.length===1);
 assert.equal(f.retained[0].blob.size,3200);assert.equal(f.commits.length,1);assert.equal(f.session.active,true,'Finalizing an old original does not stop the next microphone capture');
 f.sources[0].emit(3200,3);f.recorders[1].partial('Otra frase');f.activity(2,true,false);f.activity(2,false,true);
 await until(()=>f.recorders.length===3);f.recorders[1].complete('Otra frase');await until(()=>f.sent.length===2);
 assert.equal(f.retained[1].blob.size,3232);assert.equal(f.retained[1].playback.position_millis,123);
 assert.notEqual(f.retained[0].id,f.retained[1].id);assert.equal(f.interrupts(),2);
 f.sources[0].emit(64);f.session.hold();f.recorders[2].complete();await until(()=>f.session.originals.length===1);
 assert.equal(f.commits.length,2,'Leaving retains the unsent fragment without automatic file transcription or dispatch');
 assert.equal(f.medias[0].stops,1);assert(f.sources[0].closed);assert(f.analyses[0].closed);assert(!f.session.active);

 const lost=boot(),held=lost.holdCommit();await lost.session.start();
 lost.sources[0].emit(3200);lost.recorders[0].partial('Primera');lost.activity(1,true,false);lost.activity(1,false,true);
 await until(()=>lost.recorders.length===2);lost.recorders[0].complete('Primera');await until(()=>lost.commits.length===1);
 lost.sources[0].emit(3200);lost.recorders[1].partial('Segunda');lost.activity(2,true,false);lost.activity(2,false,true);
 await tick();assert.equal(lost.recorders.length,2,'Two unresolved originals stop admission of a third');
 lost.recorders[1].complete('Segunda');await until(()=>lost.retained.length===2);
 held.resolve({confirmed:false});await tick();await tick();
 assert.equal(lost.commits.length,1,'A later original cannot pass an uncertain predecessor');
 assert.equal(lost.session.originals.length,2);assert.equal(lost.medias[0].stops,1);assert(!lost.session.active);
 assert.equal(lost.sources.length,1);assert.equal(lost.analyses.length,1);

 const reversed=boot();await reversed.session.start();
 reversed.sources[0].emit(3200);reversed.recorders[0].partial('Primera');reversed.activity(1,true,false);reversed.activity(1,false,true);await until(()=>reversed.recorders.length===2);
 reversed.sources[0].emit(3200);reversed.recorders[1].partial('Segunda');reversed.activity(2,true,false);reversed.activity(2,false,true);await tick();
 reversed.recorders[1].complete('Segunda');await until(()=>reversed.retained.length===1);assert.equal(reversed.commits.length,0,'A later ASR final cannot pass the earlier unfinished original');
 reversed.recorders[0].complete('Primera');await until(()=>reversed.sent.length===2);
 assert.deepEqual(reversed.commits.map(v=>v.id),reversed.recorders.map(r=>r.options.id));

 const scoped=boot();await scoped.session.start();scoped.questions({visibleBeforeSpeech:'v2'});scoped.sources[0].acoustic(640,640);
 scoped.questions({arrivedLater:'v3'});scoped.sources[0].emit(3200);scoped.recorders[0].partial('Respuesta');scoped.activity(1,true,false);scoped.activity(1,false,true);
 await until(()=>scoped.recorders.length===2);scoped.recorders[0].complete('Respuesta');await until(()=>scoped.sent.length===1);
 assert.equal(JSON.stringify(scoped.retained[0].captureQuestions),JSON.stringify({visibleBeforeSpeech:'v2'}),'Passive ASR arming cannot predate a question heard before speech; later questions remain excluded');
 scoped.sources[0].emit(10);scoped.session.hold();scoped.recorders[1].complete();await tick();

 const late=boot(),permission=late.permission(),starting=late.session.start();
 await until(()=>late.analyses.length===1);await tick();late.session.hold();assert(await late.session.start());
 const old={stops:0,getTracks(){return [{stop:()=>old.stops++}];}};permission.resolve(old);assert.equal(await starting,false);
 assert.equal(old.stops,1);assert(late.session.active,'Late permission from an old view cannot close the newer voice session');
 late.sources[0].emit(20);late.session.hold();late.recorders[0].complete();await tick();

 const hidden=boot();await hidden.session.start();hidden.sources[0].emit(10);hidden.document.hidden=true;hidden.events.visibilitychange();hidden.recorders[0].complete('draft');
 await until(()=>hidden.retained.length===1);assert.equal(hidden.commits.length,0);assert.equal(hidden.medias[0].stops,1);
 const moved=boot();await moved.session.start();moved.sources[0].emit(12);moved.bind('b'.repeat(64));moved.activity(1,true,false);moved.recorders[0].complete('draft');
 await until(()=>moved.retained.length===1);assert.equal(moved.commits.length,0);assert.equal(moved.retained[0].binding,'a'.repeat(64));
 console.log('Continuous foreground: one mic/analyzer, floor/barge-in, gap preservation, exact ordered originals, uncertainty/capacity, exit/hidden/binding and late permission passed; no devices or models.');
})().catch(error=>{console.error(error);process.exitCode=1;});
