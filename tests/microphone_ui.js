// Shipped microphone state machine with synthetic devices/network; no model calls.
const assert=require('node:assert/strict'),fs=require('node:fs'),vm=require('node:vm'),crypto=require('node:crypto').webcrypto;
const until=async fn=>{for(let n=0;n<100;n++){if(fn())return;await new Promise(yes=>setTimeout(yes,5));}throw Error('Expected microphone transition did not occur');};
const deferred=()=>{let resolve;return {promise:new Promise(yes=>{resolve=yes;}),resolve:value=>resolve(value)};};
function memoryRecovery(){
 const values=new Map();
 const db={createObjectStore(){},close(){},transaction(_name,mode){
   const tx={aborted:false,abort(){this.aborted=true;this.onabort?.();},objectStore(){return {
     get(key){const request={};queueMicrotask(()=>{request.result=values.get(key);request.onsuccess?.();queueMicrotask(()=>{if(!tx.aborted)tx.oncomplete?.();});});return request;},
     put(value,key){assert.equal(mode,'readwrite');values.set(key,value);},delete(key){assert.equal(mode,'readwrite');values.delete(key);}
   };}};return tx;
 }};
 return {values,open(){const request={result:db};queueMicrotask(()=>request.onsuccess?.());return request;}};
}
function boot(live=false,recovery){
 let now=1000;
 const nodes=new Map(),events={},timers=[],uploads=[],originals=new Map(),received=[],devices=[],activities=[];
 const document={hidden:false,addEventListener(name,fn){events[name]=fn;},querySelector(name){if(!nodes.has(name))nodes.set(name,{hidden:false,disabled:false,textContent:'',handlers:{},setAttribute(k,v){this[k]=v;},addEventListener(n,f){this.handlers[n]=f;}});return nodes.get(name);}};
 let fileReferences,mediaFailure,visual,visualFailure,mediaRequests=0,confirmDiscard=true;const confirmations=[];
 let captureQuestions,playback,selection, binding='original-binding',mode,permission,receiptHold,uploadHold,capturePhase='preparing',captureRetired=false,paused=0,unauthorized=0;
 class Recorder{
  static isTypeSupported(type){return type==='audio/webm;codecs=opus';}
  constructor(stream){this.stream=stream;this.events={};this.state='inactive';devices.push(this);}
  addEventListener(n,fn){this.events[n]=fn;}
  start(){this.state='recording';}
  stop(){if(this.state==='inactive')return;this.state='inactive';queueMicrotask(()=>{this.events.dataavailable({data:new Blob(['synthetic finalized audio'])});this.events.stop();});}
 }
 const media=()=>({stopped:0,getTracks(){return [{stop:()=>this.stopped++}];}});
 const window={indexedDB:recovery,confirm(message){confirmations.push(message);return confirmDiscard;},isSecureContext:true,MediaRecorder:Recorder,crypto,addEventListener(n,fn){events[n]=fn;},navigator:{mediaDevices:{async getUserMedia(){mediaRequests++;if(mediaFailure)throw mediaFailure;if(permission)return permission.promise;return media();}}}};
 async function fetch(_route,options){
  options.route=_route;uploads.push(options);const id=options.headers['X-Input-Id'];
  if(mode==='before'){mode=undefined;throw Error('Offline before capture');}
  if(mode==='visual-refusal'){mode=undefined;return {ok:false,status:400,json:async()=>({code:'visual_reselection',submission_attempted:false})};}
  if(mode==='denied'){mode=undefined;return {ok:false,status:403,json:async()=>({})};}
  const result={id,phase:capturePhase,retired:captureRetired,visual:options.headers['X-Visual-Reference']?JSON.parse(options.headers['X-Visual-Reference']):undefined,voice:{audio_sha256:options.headers['X-Audio-Sha256'],...(options.headers['X-Recording-Stop']?{capture_stop:options.headers['X-Recording-Stop']}:{})}};if(mode==='visual-extra'){mode=undefined;result.visual.push({source_id:'unexpected-third-image'});}
  if(options.headers['X-File-References']){result.files=JSON.parse(decodeURIComponent(options.headers['X-File-References'])).map(f=>({size:f.size,name:f.name,mime:f.mime,sha256:f.sha256,file_id:f.file_id}));if(mode==='files-changed'){mode=undefined;result.files[0].size++;}}
  originals.set(id,result);
  if(uploadHold)await uploadHold.promise;
  if(mode==='after'){mode=undefined;throw Error('Lost acknowledgement');}
  return {ok:true,json:async()=>result};
 }
 const timing=[];
 let preparation;
 if(live)window.StreamingVoice={supported:()=>true,async create(stream,meta){preparation=meta;const recorder=new Recorder(stream);recorder.discard=()=>recorder.stop();return recorder;}};
 const context=vm.createContext({Date:{now:()=>now},window,document,Blob,Uint8Array,fetch,AbortController,
  setTimeout(fn,ms){const t={fn,ms};timers.push(t);return t;},clearTimeout(t){if(t)t.closed=true;}});
 vm.runInContext(fs.readFileSync('dashboard/web/microphone.js','utf8'),context);
 let ui;ui=window.VoiceInput.mount({headers:()=>({'X-Dashboard-CSRF':'synthetic'}),getBinding:()=>binding,onTiming:(id,stage)=>timing.push({id,stage}),getReply:()=>selection,getPlayback:()=>playback,getCaptureQuestions:()=>captureQuestions,getFiles:()=>fileReferences,getVisual:()=>{if(visualFailure)throw Error('Selected image loading or unavailable');return visual;},pauseAudio(){paused++;},onActivity(value){activities.push(value);},onInput(value){received.push(value);},unauthorized(){unauthorized++;ui.reset();},async api(route){if(receiptHold)await receiptHold.promise;const value=originals.get(route.split('/').at(-1));if(!value)throw Error('No recording');return value;}});
 ui.configure(true,live);
 return {timing,advance:ms=>{now+=ms;},files:value=>{fileReferences=value;},ui,nodes,uploads,originals,received,devices,events,timers,media,activities,questions:value=>{captureQuestions=value;},playback:value=>{playback=value;},preparation:()=>preparation,
  confirmation:value=>{confirmDiscard=value;},confirmations,
  button:()=>nodes.get('#conversation-talk'),state:()=>nodes.get('#conversation-voice-state').textContent,
  click:()=>nodes.get('#conversation-talk').handlers.click(),cancel:()=>nodes.get('#conversation-voice-discard').handlers.click(),check:()=>nodes.get('#conversation-voice-check').handlers.click(),
  visual:value=>{visual=value;visualFailure=false;},failVisual:()=>{visualFailure=true;},mediaRequests:()=>mediaRequests,microphoneFailure:name=>{mediaFailure={name,message:"private platform details"};},select:value=>{selection=value;},captureState:(value,retired=false)=>{capturePhase=value;captureRetired=retired;},mode:value=>{mode=value;},permission:()=>{permission=deferred();return permission;},holdReceipt:()=>{receiptHold=deferred();return receiptHold;},holdUpload:()=>{uploadHold=deferred();return uploadHold;},bind:value=>{binding=value;},paused:()=>paused,unauthorized:()=>unauthorized,
 };
}
(async()=>{
 const prepared=boot(true);await prepared.click();prepared.preparation().onPartial('Live caption without a prior error');
 assert.equal(prepared.nodes.get('#conversation-voice-state')['data-live-caption'],'true');
 prepared.devices[0].preparedText='Final recognized sentence';const savedHold=prepared.holdUpload();
 prepared.nodes.get('#conversation-send').handlers.click();await until(()=>prepared.uploads.length===1);
 assert.equal(prepared.state(),'Final recognized sentence','Keep completed recognition visible during durable saving');
 savedHold.resolve();await until(()=>prepared.received.length===1);
 assert.equal(prepared.state(),'Final recognized sentence','Do not replace prepared text with a second-transcription claim');
 prepared.ui.observe({...prepared.received[0],phase:'preparing'},'Confirming transcript…');assert.equal(prepared.state(),'Final recognized sentence');
 prepared.ui.observe({...prepared.received[0],phase:'accepted'},'Sent');assert.equal(prepared.state(),'');
 assert.equal(prepared.uploads.length,1);
 const mediaHint={source:'native:7',revision:'a'.repeat(64),segment_offset:240,position_millis:3700,duration_millis:12000,state:'interrupted'};
 const playCapture=boot();playCapture.playback(mediaHint);const heldPlayPermission=playCapture.permission();
 const playingStart=playCapture.click();playCapture.playback({...mediaHint,position_millis:9900});heldPlayPermission.resolve(playCapture.media());await playingStart;
 playCapture.mode('after');playCapture.click();await until(()=>playCapture.uploads.length===1&&playCapture.state().includes('unconfirmed'));
 assert.deepEqual(JSON.parse(playCapture.uploads[0].headers['X-Playback-Context']),mediaHint,'Playback progress is frozen before microphone permission');
 playCapture.playback(undefined);await playCapture.click();await until(()=>playCapture.received.length===1);
 assert.equal(playCapture.uploads[0].headers['X-Playback-Context'],playCapture.uploads[1].headers['X-Playback-Context'],'An identical recording retry keeps its captured playback metadata');

 const scoped=boot(true),known={['a'.repeat(32)]:'b'.repeat(64)};
 scoped.questions(known);const scopePermission=scoped.permission(),scopeStarting=scoped.click();
 known['c'.repeat(32)]='d'.repeat(64);scoped.questions({});scopePermission.resolve(scoped.media());await scopeStarting;
 scoped.mode('after');scoped.click();await until(()=>scoped.uploads.length===1&&scoped.state().includes('unconfirmed'));
 assert.deepEqual(JSON.parse(scoped.uploads[0].headers['X-Capture-Questions']),{['a'.repeat(32)]:'b'.repeat(64)},'Question limits are cloned before permission and live ASR');
 scoped.questions({['c'.repeat(32)]:'d'.repeat(64)});await scoped.click();await until(()=>scoped.received.length===1);
 assert.equal(scoped.uploads[1].headers['X-Capture-Questions'],scoped.uploads[0].headers['X-Capture-Questions'],'A recording retry cannot acquire later question versions');
 const none=boot(true);none.questions({});await none.click();none.click();await until(()=>none.received.length===1);
 assert.equal(none.uploads[0].headers['X-Capture-Questions'],'{}','An empty capture-time question set is an explicit restriction');
 const live=boot(true);await live.click();const preparation=live.preparation();
 assert(preparation.id&&preparation.binding==='original-binding');preparation.onPartial('unfinished words');
 assert.equal(live.state(),'unfinished words');assert.equal(live.uploads.length,0,'Live recognition cannot dispatch the original');
 live.nodes.get('#conversation-send').handlers.click();await until(()=>live.received.length===1);
 assert.deepEqual(live.timing.map(event=>event.stage),['requested','started','partial','send','ended','ready','send','retained']);
 assert(live.timing.every(event=>event.id===preparation.id),'Capture timing owns the same immutable original across finalization/upload');
 assert.equal(live.uploads.length,1);assert.equal(live.uploads[0].headers['X-Input-Id'],preparation.id,'Streaming preparation and original admission must own the same capture UUID');assert.equal(live.uploads[0].body.type,'audio/wav');
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
 const originalFiles=[{file_id:'11111111-1111-4111-8111-111111111111',sha256:'a'.repeat(64),name:'scan.pdf',mime:'application/pdf',size:17}];
 const withFiles=boot();withFiles.files(originalFiles);await withFiles.click();withFiles.click();await until(()=>withFiles.received.length===1);
 assert.equal(withFiles.uploads.length,1,'Reordered server object keys cannot invalidate a receipt');
 const changedFiles=boot();changedFiles.files(originalFiles);changedFiles.mode('files-changed');await changedFiles.click();changedFiles.click();await until(()=>changedFiles.state().startsWith('Recording unconfirmed'));assert.equal(changedFiles.received.length,0,'Changed original metadata must stay unconfirmed');
 const lost=boot();lost.mode('after');await lost.click();lost.click();await until(()=>lost.state().startsWith('Recording unconfirmed'));
 assert.equal(lost.originals.size,1);await lost.check();assert.equal(lost.uploads.length,1,'Status inspection must not upload again');assert.equal(lost.received.length,1);
 const storage=memoryRecovery(),beforeReload=boot(false,storage);
 await until(()=>!beforeReload.button().disabled);beforeReload.mode('before');await beforeReload.click();beforeReload.click();
 await until(()=>beforeReload.state().startsWith('Recording unconfirmed'));
 assert.equal(storage.values.size,1);assert.equal(beforeReload.button().hidden,false,'Unconfirmed original keeps explicit retry reachable');
 assert.equal(beforeReload.nodes.get('#conversation-voice-discard').hidden,false,'Unconfirmed capture must have an exit without reloading');
 const retained=storage.values.get('pending'),afterReload=boot(false,storage);
 await until(()=>afterReload.state().startsWith('An unfinished recording was recovered'));
 assert.equal(afterReload.uploads.length,0);assert.equal(afterReload.mediaRequests(),0,'Recovery does not capture, dispatch or retry');
 await afterReload.check();assert.equal(afterReload.uploads.length,0);assert.match(afterReload.state(),/local audio is kept/);
 await afterReload.click();await until(()=>afterReload.received.length===1);
 assert.equal(afterReload.uploads[0].headers['X-Input-Id'],retained.id);assert.equal(afterReload.uploads[0].body,retained.blob);
 assert.equal(storage.values.size,0,'Only a matching receipt retires local recovery');
 const discarded=boot();discarded.mode('before');await discarded.click();discarded.click();await until(()=>discarded.state().startsWith('Recording unconfirmed'));
 discarded.confirmation(false);discarded.cancel();assert.equal(discarded.nodes.get('#conversation-voice-check').hidden,false);
 discarded.confirmation(true);discarded.cancel();assert.equal(discarded.ui.ownsSend(),false);assert.equal(discarded.button()['aria-label'],'Record a voice message');
 assert.equal(discarded.uploads.length,1,'Discard of an uncertain local copy cannot resend or cancel server work');
 const offline=boot();offline.mode('before');await offline.click();offline.click();await until(()=>offline.state().startsWith('Recording unconfirmed'));
 assert.equal(offline.originals.size,0);await offline.click();await until(()=>offline.received.length===1);
 assert.equal(offline.uploads.length,2);assert.deepEqual(offline.uploads[0].headers,offline.uploads[1].headers);assert.equal(offline.uploads[0].body,offline.uploads[1].body,'Explicit retry preserves the exact original bytes');
 const hidden=boot();await hidden.click();hidden.events.pagehide();await until(()=>hidden.devices[0].state==='inactive');assert.equal(hidden.uploads.length,0);assert.equal(hidden.devices[0].stream.stopped,1);
 const delayed=boot(),permission=delayed.permission();const starting=delayed.click();delayed.cancel();const lateMedia=delayed.media();permission.resolve(lateMedia);await starting;assert.equal(lateMedia.stopped,1);assert.equal(delayed.devices.length,0);
 const counter=boot(true);await counter.click();const remaining=counter.nodes.get('#conversation-voice-remaining');
 assert.equal(remaining.textContent,'2:00 left');assert.equal(remaining.hidden,false);
 counter.preparation().onPartial('Revisable words');counter.advance(105000);counter.timers.find(t=>t.ms===1000&&!t.closed).fn();
 assert.equal(remaining.textContent,'0:15 left');assert.equal(remaining['data-near-limit'],'true');assert.equal(counter.state(),'Revisable words','Counter never replaces live captions');
 counter.cancel();assert.equal(remaining.hidden,true);assert(counter.timers.filter(t=>t.ms===1000).every(t=>t.closed),'Discard retires the countdown timer');
 const limit=boot();await limit.click();limit.timers.find(t=>t.ms===120000).fn();await until(()=>limit.button()['aria-label']==='Send recording');assert.equal(limit.uploads.length,0,'Duration limit cannot silently commit speech');limit.click();await until(()=>limit.received.length===1);assert.equal(limit.uploads[0].route,'api/interaction/voice/limited');assert.equal(limit.received[0].voice.capture_stop,'duration_limit');
 const flushReview=boot(true);await flushReview.click();flushReview.devices[0].captureStop='audio_flush_unconfirmed';flushReview.click();
 await until(()=>flushReview.button()['aria-label']==='Send recording');assert.equal(flushReview.uploads.length,0,'A flush timeout requires explicit review even after the first Send');
 assert(flushReview.state().includes('final audio buffer'));assert(!flushReview.state().includes('2:00'),'A flush failure must not be mislabeled as a duration limit');
 flushReview.click();await until(()=>flushReview.received.length===1);assert.equal(flushReview.received[0].voice.capture_stop,'audio_flush_unconfirmed');assert.equal(flushReview.uploads[0].route,'api/interaction/voice/limited');
 const reviewStorage=memoryRecovery(),storedReview=boot(true,reviewStorage);await until(()=>!storedReview.button().disabled);await storedReview.click();storedReview.devices[0].captureStop='audio_flush_unconfirmed';storedReview.click();
 await until(()=>storedReview.button()['aria-label']==='Send recording');assert.equal(storedReview.uploads.length,0);assert.equal(reviewStorage.values.size,1,'A fragment awaiting explicit review survives reload');
 const loadedReview=boot(false,reviewStorage);await until(()=>loadedReview.state().startsWith('An unfinished recording was recovered'));assert(loadedReview.state().includes('ending may be missing'));assert.equal(loadedReview.uploads.length,0,'Reload never sends the fragment');
 await loadedReview.click();await until(()=>loadedReview.received.length===1);assert.equal(loadedReview.received[0].voice.capture_stop,'audio_flush_unconfirmed');assert.equal(reviewStorage.values.size,0);
 const racedLimit=boot(true);await racedLimit.click();racedLimit.devices[0].captureStop='duration_limit';racedLimit.click();await until(()=>racedLimit.received.length===1);assert.equal(racedLimit.received[0].voice.capture_stop,'duration_limit','Send racing a streaming limit cannot erase the stop reason');
 const limitStorage=memoryRecovery(),limitedRecovery=boot(true,limitStorage);await until(()=>!limitedRecovery.button().disabled);await limitedRecovery.click();limitedRecovery.devices[0].preparedText='The complete captured fragment';limitedRecovery.timers.find(t=>t.ms===120000).fn();await until(()=>limitedRecovery.button()['aria-label']==='Send recording');limitedRecovery.mode('before');limitedRecovery.click();await until(()=>limitedRecovery.uploads.length===1&&limitedRecovery.state().includes('unconfirmed'));const restoredLimited=boot(false,limitStorage);await until(()=>restoredLimited.state().startsWith('An unfinished recording was recovered'));await restoredLimited.click();await until(()=>restoredLimited.received.length===1);assert.equal(restoredLimited.received[0].voice.capture_stop,'duration_limit');assert.equal(restoredLimited.uploads[0].headers['X-Input-Id'],limitedRecovery.uploads[0].headers['X-Input-Id']);assert.equal(limitStorage.values.size,0);
 for(const [phase,retired,phrase] of [['answered',false,'answer recorded'],['retained_unsent',false,'no work was submitted'],['preparation_unconfirmed',false,'transcription unconfirmed'],['preparing',true,'no longer eligible']]){
  const saved=boot();saved.captureState(phase,retired);await saved.click();saved.click();await until(()=>saved.received.length===1);assert(saved.state().includes(phrase),'A receipt must show actual preparation eligibility');
 }
 for(const [name,message] of [['NotAllowedError','permission was denied'],['NotFoundError','could not find an accessible microphone'],['NotReadableError','may be busy'],['NotSupportedError','not supported']]){
  const failed=boot();failed.microphoneFailure(name);await failed.click();
  assert(failed.state().includes(message));assert(!failed.state().includes('private platform details'));
  assert.equal(failed.uploads.length,0);assert.equal(failed.devices.length,0);assert(!failed.ui.ownsSend());
 }
 const denied=boot();denied.mode('denied');await denied.click();denied.click();await until(()=>denied.unauthorized()===1);assert(denied.nodes.get('#conversation-voice').hidden);
 // A late status read from an old binding cannot clear a newer upload latch.
 const race=boot();race.mode('before');await race.click();race.click();await until(()=>race.state().startsWith('Recording unconfirmed'));
 const hold=race.holdReceipt(),checking=race.check();race.ui.reset();race.bind('new-binding');race.ui.configure(true);await race.click();const uploading=race.holdUpload();race.click();await until(()=>race.uploads.length===2);hold.resolve();await checking;
 assert(race.button().disabled,'An old receipt read must not unlock a newer upload');assert.equal(race.received.length,0);uploading.resolve();await until(()=>race.received.length===1);
 const progress=boot();await progress.click();progress.click();await until(()=>progress.received.length===1);
 const saved=progress.received[0];progress.ui.observe({...saved,phase:'routed'},'#Alpha: sent to this session. Work may continue.');
 assert.equal(progress.state(),'','The input card owns saved/delivery status; do not duplicate it above the composer');
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
 const streamingPair=boot(true);streamingPair.visual(pair);const streamingPermission=streamingPair.permission(),streamingStart=streamingPair.click();
 streamingPair.visual([newVisual]);streamingPermission.resolve(streamingPair.media());await streamingStart;
 const streamingCapture=streamingPair.preparation();streamingCapture.onPartial('two images, still speaking');assert.equal(streamingPair.uploads.length,0);
 streamingPair.mode('after');streamingPair.nodes.get('#conversation-send').handlers.click();await until(()=>streamingPair.state().startsWith('Recording unconfirmed'));
 streamingPair.visual([oldVisual]);await streamingPair.check();await until(()=>streamingPair.received.length===1);
 assert.equal(streamingPair.uploads.length,1,'Lost streaming-upload acknowledgment is recovered without another admission');
 assert.equal(streamingPair.uploads[0].headers['X-Input-Id'],streamingCapture.id);
 assert.deepEqual(JSON.parse(streamingPair.uploads[0].headers['X-Visual-Reference']),pair,'Streaming capture retains both frozen images across permission delay, caption updates and lost acknowledgment');
 const changedPair=boot();changedPair.visual(pair);changedPair.mode('visual-extra');await changedPair.click();changedPair.click();await until(()=>changedPair.state().startsWith('Recording unconfirmed'));
 assert.equal(changedPair.received.length,0,'A receipt with an extra image must not consume the frozen recording');assert.equal(changedPair.uploads.length,1);
 changedPair.originals.values().next().value.visual=pair;await changedPair.check();await until(()=>changedPair.received.length===1);assert.equal(changedPair.uploads.length,1,'Correct receipt observation does not resend audio');
 console.log('Microphone UI: finalized one-button capture, playback pause, source hashing, explicit identical retry, receipt-only recovery, hidden/cancel/late permission cleanup, duration-limit review and binding reset passed; zero models.');
})().catch(error=>{console.error(error);process.exitCode=1;});
