// Shipped microphone state machine with synthetic devices/network; no model calls.
const assert=require('node:assert/strict'),fs=require('node:fs'),vm=require('node:vm'),crypto=require('node:crypto').webcrypto;
const until=async fn=>{for(let n=0;n<100;n++){if(fn())return;await new Promise(yes=>setTimeout(yes,5));}throw Error('Expected microphone transition did not occur');};
const deferred=()=>{let resolve;return {promise:new Promise(yes=>{resolve=yes;}),resolve:value=>resolve(value)};};
function boot(){
 const nodes=new Map(),events={},timers=[],uploads=[],originals=new Map(),received=[],devices=[],activities=[];
 const document={hidden:false,addEventListener(name,fn){events[name]=fn;},querySelector(name){if(!nodes.has(name))nodes.set(name,{hidden:false,disabled:false,textContent:'',handlers:{},setAttribute(k,v){this[k]=v;},addEventListener(n,f){this.handlers[n]=f;}});return nodes.get(name);}};
 let visual,visualFailure,mediaRequests=0,confirmDiscard=true;const confirmations=[];
 let selection, binding='original-binding',mode,permission,receiptHold,uploadHold,capturePhase='preparing',captureRetired=false,paused=0,unauthorized=0;
 class Recorder{
  static isTypeSupported(type){return type==='audio/webm;codecs=opus';}
  constructor(stream){this.stream=stream;this.events={};this.state='inactive';devices.push(this);}
  addEventListener(n,fn){this.events[n]=fn;}
  start(){this.state='recording';}
  stop(){if(this.state==='inactive')return;this.state='inactive';queueMicrotask(()=>{this.events.dataavailable({data:new Blob(['synthetic finalized audio'])});this.events.stop();});}
 }
 const media=()=>({stopped:0,getTracks(){return [{stop:()=>this.stopped++}];}});
 const window={confirm(message){confirmations.push(message);return confirmDiscard;},isSecureContext:true,MediaRecorder:Recorder,crypto,addEventListener(n,fn){events[n]=fn;},navigator:{mediaDevices:{async getUserMedia(){mediaRequests++;if(permission)return permission.promise;return media();}}}};
 async function fetch(_route,options){
  uploads.push(options);const id=options.headers['X-Input-Id'];
  if(mode==='before'){mode=undefined;throw Error('Offline before capture');}
  if(mode==='visual-refusal'){mode=undefined;return {ok:false,status:400,json:async()=>({code:'visual_reselection',submission_attempted:false})};}
  if(mode==='denied'){mode=undefined;return {ok:false,status:403,json:async()=>({})};}
  const result={id,phase:capturePhase,retired:captureRetired,visual:options.headers['X-Visual-Reference']?JSON.parse(options.headers['X-Visual-Reference']):undefined,voice:{audio_sha256:options.headers['X-Audio-Sha256']}};if(mode==='visual-extra'){mode=undefined;result.visual.push({source_id:'unexpected-third-image'});}
  originals.set(id,result);
  if(uploadHold)await uploadHold.promise;
  if(mode==='after'){mode=undefined;throw Error('Lost acknowledgement');}
  return {ok:true,json:async()=>result};
 }
 const context=vm.createContext({window,document,Blob,Uint8Array,fetch,AbortController,
  setTimeout(fn,ms){const t={fn,ms};timers.push(t);return t;},clearTimeout(t){if(t)t.closed=true;}});
 vm.runInContext(fs.readFileSync('dashboard/web/microphone.js','utf8'),context);
 let ui;ui=window.VoiceInput.mount({headers:()=>({'X-Dashboard-CSRF':'synthetic'}),getBinding:()=>binding,getReply:()=>selection,getVisual:()=>{if(visualFailure)throw Error('Selected image loading or unavailable');return visual;},pauseAudio(){paused++;},onActivity(value){activities.push(value);},onInput(value){received.push(value);},unauthorized(){unauthorized++;ui.reset();},async api(route){if(receiptHold)await receiptHold.promise;const value=originals.get(route.split('/').at(-1));if(!value)throw Error('No recording');return value;}});
 ui.configure(true);
 return {ui,nodes,uploads,originals,received,devices,events,timers,media,activities,
  confirmation:value=>{confirmDiscard=value;},confirmations,
  button:()=>nodes.get('#conversation-talk'),state:()=>nodes.get('#conversation-voice-state').textContent,
  click:()=>nodes.get('#conversation-talk').handlers.click(),cancel:()=>nodes.get('#conversation-voice-discard').handlers.click(),check:()=>nodes.get('#conversation-voice-check').handlers.click(),
  visual:value=>{visual=value;visualFailure=false;},failVisual:()=>{visualFailure=true;},mediaRequests:()=>mediaRequests,select:value=>{selection=value;},captureState:(value,retired=false)=>{capturePhase=value;captureRetired=retired;},mode:value=>{mode=value;},permission:()=>{permission=deferred();return permission;},holdReceipt:()=>{receiptHold=deferred();return receiptHold;},holdUpload:()=>{uploadHold=deferred();return uploadHold;},bind:value=>{binding=value;},paused:()=>paused,unauthorized:()=>unauthorized,
 };
}
(async()=>{
 const hinted=boot();hinted.select({reference:{source:'delivery:11111111-1111-1111-1111-111111111111',revision:'a'.repeat(64),context_only:true},version:1});
 await hinted.click();hinted.click();await until(()=>hinted.received.length===1);
 assert.equal(hinted.uploads[0].headers['X-Reply-Source'],'context:delivery:11111111-1111-1111-1111-111111111111','Device context must not become an explicit destination during voice upload');
 const unified=boot();await unified.click();
 assert.equal(unified.nodes.get('#conversation-voice-state')['data-attention'],'false','Routine recording status belongs to header, without a second visible line');
 assert(unified.button().hidden,'Recording hides the redundant microphone/send control');
 const send=unified.nodes.get('#conversation-send');assert.equal(send.type,'button');assert.equal(send['aria-label'],'Send voice message');assert(!send.disabled);
 assert.equal(unified.nodes.get('#conversation-voice-discard').textContent,'×');assert(!unified.nodes.get('#conversation-voice-discard').hidden);
 send.handlers.click();await until(()=>unified.received.length===1);assert.equal(unified.uploads.length,1);assert.equal(send.type,'submit');assert(!unified.button().hidden);
 const cancelled=boot();await cancelled.click();cancelled.confirmation(false);cancelled.cancel();assert.equal(cancelled.devices[0].state,'recording','Declining confirmation must preserve the recording');assert.equal(cancelled.uploads.length,0);cancelled.confirmation(true);cancelled.cancel();assert.equal(cancelled.devices[0].state,'inactive');assert.equal(cancelled.nodes.get('#conversation-send').type,'submit');assert.equal(cancelled.uploads.length,0);assert.equal(cancelled.confirmations.length,2);assert.match(cancelled.confirmations[0],/deleted without sending/);
 const f=boot();await f.click();assert.equal(f.button().textContent,'↑');assert.equal(f.paused(),1);
 assert.equal(f.uploads.length,0,'Partial recording cannot admit work');f.click();await until(()=>f.received.length===1);
 assert.equal(f.uploads.length,1);assert.equal(f.uploads[0].body.type,'audio/webm;codecs=opus');assert.equal(f.devices[0].stream.stopped,1);
 assert.equal(f.button().textContent,'🎙');assert(f.received[0].voice.audio_sha256);
 assert.equal(f.activities.at(-1),true,'Audio replies wait until the transcript is ready');f.ui.observe({...f.received[0],phase:'accepted'},'Sent');assert.equal(f.activities.at(-1),false);
 const lost=boot();lost.mode('after');await lost.click();lost.click();await until(()=>lost.state().startsWith('Recording unconfirmed'));
 assert.equal(lost.originals.size,1);await lost.check();assert.equal(lost.uploads.length,1,'Status inspection must not upload again');assert.equal(lost.received.length,1);
 const offline=boot();offline.mode('before');await offline.click();offline.click();await until(()=>offline.state().startsWith('Recording unconfirmed'));
 assert.equal(offline.originals.size,0);await offline.click();await until(()=>offline.received.length===1);
 assert.equal(offline.uploads.length,2);assert.deepEqual(offline.uploads[0].headers,offline.uploads[1].headers);assert.equal(offline.uploads[0].body,offline.uploads[1].body,'Explicit retry preserves the exact original bytes');
 const hidden=boot();await hidden.click();hidden.events.pagehide();await until(()=>hidden.devices[0].state==='inactive');assert.equal(hidden.uploads.length,0);assert.equal(hidden.devices[0].stream.stopped,1);
 const delayed=boot(),permission=delayed.permission();const starting=delayed.click();delayed.cancel();const lateMedia=delayed.media();permission.resolve(lateMedia);await starting;assert.equal(lateMedia.stopped,1);assert.equal(delayed.devices.length,0);
 const limit=boot();await limit.click();limit.timers.find(t=>t.ms===120000).fn();await until(()=>limit.button()['aria-label']==='Send recording');assert.equal(limit.uploads.length,0,'Duration limit cannot silently commit speech');limit.click();await until(()=>limit.received.length===1);
 for(const [phase,retired,phrase] of [['answered',false,'answer recorded'],['retained_unsent',false,'no work was submitted'],['preparation_unconfirmed',false,'transcription unconfirmed'],['preparing',true,'no longer eligible']]){
  const saved=boot();saved.captureState(phase,retired);await saved.click();saved.click();await until(()=>saved.received.length===1);assert(saved.state().includes(phrase),'A receipt must show actual preparation eligibility');
 }
 const denied=boot();denied.mode('denied');await denied.click();denied.click();await until(()=>denied.unauthorized()===1);assert(denied.nodes.get('#conversation-voice').hidden);
 // A late status read from an old binding cannot clear a newer upload latch.
 const race=boot();race.mode('before');await race.click();race.click();await until(()=>race.state().startsWith('Recording unconfirmed'));
 const hold=race.holdReceipt(),checking=race.check();race.ui.reset();race.bind('new-binding');race.ui.configure(true);await race.click();const uploading=race.holdUpload();race.click();await until(()=>race.uploads.length===2);hold.resolve();await checking;
 assert(race.button().disabled,'An old receipt read must not unlock a newer upload');assert.equal(race.received.length,0);uploading.resolve();await until(()=>race.received.length===1);
 const progress=boot();await progress.click();progress.click();await until(()=>progress.received.length===1);
 const saved=progress.received[0];progress.ui.observe({...saved,phase:'routed'},'#Alpha: sent to this session. Work may continue.');
 assert.equal(progress.state(),'Recording saved. #Alpha: sent to this session. Work may continue.','Later canonical metadata must replace stale ASR preparation feedback');
 await progress.click();const recordingState=progress.state();
 progress.ui.observe(saved,'Old receipt');assert.equal(progress.state(),recordingState,'An earlier receipt cannot overwrite a new recording');
 progress.cancel();const discardedState=progress.state();progress.ui.observe(saved,'Old receipt');
 assert.equal(progress.state(),discardedState,'A discarded recording must not revive prior capture feedback');
 const focused=boot(),reference={source:'input:original',revision:'a'.repeat(64)};
 focused.select({reference,version:1});const focusPermission=focused.permission(),focusStart=focused.click();
 focused.select({reference:{source:'input:newer',revision:'b'.repeat(64)},version:2});
 focusPermission.resolve(focused.media());await focusStart;focused.timers.find(t=>t.ms===120000).fn();
 await until(()=>focused.button()['aria-label']==='Send recording');focused.mode('before');focused.click();
 await until(()=>focused.state().startsWith('Recording unconfirmed'));await focused.click();await until(()=>focused.received.length===1);
 assert.equal(focused.uploads[0].headers['X-Reply-Source'],reference.source);
 assert.equal(focused.uploads[0].headers['X-Reply-Revision'],reference.revision);
 assert.deepEqual(focused.uploads[0].headers,focused.uploads[1].headers,'Permission delays, duration review and retries must retain the original reply focus');
 const missingVisual=boot();missingVisual.failVisual();await missingVisual.click();
 assert.equal(missingVisual.mediaRequests(),0,'Unready visual context cannot silently acquire a microphone');assert.equal(missingVisual.uploads.length,0);assert.match(missingVisual.state(),/Selected image unavailable/);assert.equal(missingVisual.nodes.get('#conversation-voice-state')['data-attention'],'true','Actionable errors remain readable');
 const oldVisual={source_id:'original-view',point:{x:20,y:30}},newVisual={source_id:'later-view',point:{x:80,y:40}};
 const refused=boot();refused.visual(oldVisual);refused.mode('visual-refusal');await refused.click();refused.click();await until(()=>refused.state().startsWith('This attempt did not submit'));
 refused.visual(newVisual);await refused.click();await until(()=>refused.received.length===1);
 assert.notEqual(refused.uploads[0].headers['X-Input-Id'],refused.uploads[1].headers['X-Input-Id'],'Explicit fresh-selection retry after only definite refusal creates an attributable new original');
 assert.equal(refused.uploads[0].body,refused.uploads[1].body);assert.deepEqual(JSON.parse(refused.uploads[1].headers['X-Visual-Reference']),newVisual);
 const unknownVisual=boot();unknownVisual.visual(oldVisual);unknownVisual.mode('before');await unknownVisual.click();unknownVisual.click();await until(()=>unknownVisual.state().startsWith('Recording unconfirmed'));
 unknownVisual.visual(newVisual);unknownVisual.mode('visual-refusal');await unknownVisual.click();await until(()=>unknownVisual.state().startsWith('An earlier submission is still unconfirmed'));
 await unknownVisual.click();await until(()=>unknownVisual.received.length===1);
 assert.equal(new Set(unknownVisual.uploads.map(v=>v.headers['X-Input-Id'])).size,1,'A later pre-submission refusal cannot settle an earlier unknown attempt or replace its UUID');
 assert(unknownVisual.uploads.every(v=>JSON.stringify(JSON.parse(v.headers['X-Visual-Reference']))===JSON.stringify(oldVisual)),'Uncertain retry cannot adopt a newly selected image');
 const pair=[oldVisual,newVisual],frozenPair=boot();frozenPair.visual(pair);const heldPermission=frozenPair.permission();const startingPair=frozenPair.click();frozenPair.visual([newVisual]);heldPermission.resolve(frozenPair.media());await startingPair;frozenPair.click();await until(()=>frozenPair.received.length===1);
 assert.deepEqual(JSON.parse(frozenPair.uploads[0].headers['X-Visual-Reference']),pair,'Permission delays retain both original images in order');
 const changedPair=boot();changedPair.visual(pair);changedPair.mode('visual-extra');await changedPair.click();changedPair.click();await until(()=>changedPair.state().startsWith('Recording unconfirmed'));
 assert.equal(changedPair.received.length,0,'A receipt with an extra image must not consume the frozen recording');assert.equal(changedPair.uploads.length,1);
 changedPair.originals.values().next().value.visual=pair;await changedPair.check();await until(()=>changedPair.received.length===1);assert.equal(changedPair.uploads.length,1,'Correct receipt observation does not resend audio');
 console.log('Microphone UI: finalized one-button capture, playback pause, source hashing, explicit identical retry, receipt-only recovery, hidden/cancel/late permission cleanup, duration-limit review and binding reset passed; zero models.');
})().catch(error=>{console.error(error);process.exitCode=1;});
