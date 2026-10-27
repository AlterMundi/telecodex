"use strict";
const assert=require('node:assert/strict'),fs=require('node:fs'),vm=require('node:vm'),crypto=require('node:crypto');
const context={window:{},crypto,TextEncoder};vm.runInNewContext(fs.readFileSync('dashboard/web/live-speech.js','utf8'),context);
const deadline=setTimeout(()=>{throw Error("Live speech test did not settle");},10000);
(async()=>{
 const empty=context.window.LiveSpeech.mount();empty.unsent(undefined);
 const state=context.window.LiveSpeech.mount(),turn='native-turn',item='native-item',key={native_turn:turn,native_item:item};
 state.observe({...key,text:'Hola Nicolás. Incomplete',provisional:true},'binding');
 const first=await state.next();assert(state.admit(first));assert.equal(first.speech_start,0);assert.equal(first.preview,'Hola Nicolás. ');
 assert.equal(await state.next(),undefined,'Incomplete text cannot become audible');
 state.ended(first);
 state.observe({...key,text:'Hola Nicolás. Second useful sentence. Next',provisional:true},'binding');
 const second=await state.next();assert(state.admit(second));assert.equal(second.speech_start,Array.from(first.preview).length);
 assert.equal(second.preview,'Second useful sentence. ');
 assert.equal(crypto.createHash('sha256').update('Hola Nicolás. Second useful sentence. ').digest('hex'),second.id.split(':').at(-1));
 assert.equal(crypto.createHash('sha256').update('binding:'+second.id).digest('hex'),second.revision);
 const canonical={...key,id:'native:1',revision:'canonical',kind:'integrator_answer',text_characters:100};
 const result=await state.reconcile(canonical,async selected=>assert.equal(selected.id,second.id));
 assert.equal(result.source.speech_start,second.speech_end,'Canonical automatic reading never repeats an attempted prefix');
 assert.equal(result.resume,first.speech_end,'Explicit resume retains the last completed media boundary, not a hearing claim');
 assert.equal(await state.next(),undefined,'Canonical item retires future provisional attempts');
 const corrected=await state.reconcile(canonical,async()=>{throw Object.assign(Error('Changed'),{status:409});});
 assert.equal(corrected.changed,true);assert.equal(corrected.source.speech_start,0);
 await assert.rejects(state.reconcile(canonical,async()=>{throw new TypeError('Offline');}),/Offline/,'Observation failure cannot silently restart canonical narration');
 state.observe({...key,text:'A code `not yet closed. Still',provisional:true},'other-binding');assert.equal(await state.next(),undefined);
 state.observe({...key,text:'A complete sentence. Next',provisional:true},'third-binding');assert.equal((await state.next()).speech_start,0,'Changed entry binding clears previous cursors');
 state.observe(null,'third-binding');assert.equal(await state.next(),undefined,'Terminal/missing snapshot does not replay old speech');
 // Canonical arrival while hashing must revoke the unadmitted selection.
 const releases=[];const delayed={window:{},TextEncoder,crypto:{subtle:{digest:(_algorithm,bytes)=>new Promise(resolve=>{releases.push(()=>resolve(crypto.createHash('sha256').update(bytes).digest()));})}}};
 vm.runInNewContext(fs.readFileSync('dashboard/web/live-speech.js','utf8'),delayed);
 const raced=delayed.window.LiveSpeech.mount();raced.observe({...key,text:'A complete sentence. Next'},'binding');
 const pending=raced.next();await raced.reconcile(canonical,()=>assert.fail('No paid prefix existed'));
 releases.shift()();await new Promise(resolve=>setImmediate(resolve));assert.equal(releases.length,1);releases.shift()();assert.equal(await pending,undefined);
 assert.equal(await raced.next(),undefined,'Canonical answer cannot race a late prefix');
 const interruptedState=context.window.LiveSpeech.mount();interruptedState.observe({...key,text:'One useful sentence. Next'},'binding');
 const owned=await interruptedState.next();assert(interruptedState.admit(owned));interruptedState.interrupt(owned);
 interruptedState.observe({...key,text:'One useful sentence. Another sentence. More'},'binding');assert.equal(await interruptedState.next(),undefined);
 const held=await interruptedState.reconcile(canonical,async()=>{});assert.equal(held.skip,true);
 const continuation=interruptedState.continueSource(owned);assert.equal(continuation.speech_start,owned.speech_end);
 const gap=context.window.LiveSpeech.mount();gap.observe({...key,text:'First sentence. Second sentence. Still'},'binding');
 const interrupted=await gap.next();assert(gap.admit(interrupted));
 gap.observe({...key,text:'First sentence. Second sentence. Still another sentence. More'},'binding');
 const later=await gap.next();assert(gap.admit(later));gap.ended(later);
 const unplayed=await gap.reconcile(canonical,async()=>{});assert.equal(unplayed.resume,0,'Later completion cannot hide the first unsaid gap');
 const completed=gap.ended(interrupted);assert.equal(completed.resume,later.speech_end,'Finishing a live prefix after confirmation updates the canonical cursor');
 assert.equal(gap.resume(canonical),later.speech_end);
 const partial=context.window.LiveSpeech.mount();partial.observe({...key,text:'A useful spoken sentence. Unfinished'},'binding');
 const playing=await partial.next();assert(partial.admit(playing));
 partial.checkpoint(playing,0,12);partial.interrupt(playing);
 assert.equal((await partial.reconcile(canonical,async()=>{})).resume,12,'Confirmation retains partial aligned media position');
 assert.equal(partial.checkpoint(playing,0,4).resume,4,'Backward media seek replaces a partial high-water mark');
 assert.equal(partial.checkpoint(playing,0,playing.speech_end+1),undefined);assert.equal(partial.resume(canonical),4);
 assert.equal(partial.ended(canonical,4,100).resume,100,'Explicit canonical remainder joins the retained partial prefix');
 const changedPartial=context.window.LiveSpeech.mount();changedPartial.observe({...key,text:'A useful spoken sentence. Unfinished'},'binding');
 const obsolete=await changedPartial.next();assert(changedPartial.admit(obsolete));changedPartial.checkpoint(obsolete,0,12);
 assert.equal((await changedPartial.reconcile(canonical,async()=>{throw Object.assign(Error('Changed'),{status:409});})).resume,0);
 assert.equal(changedPartial.checkpoint(obsolete,0,20),undefined,'Old timing cannot refine a changed canonical reply');
 const partialGap=context.window.LiveSpeech.mount();partialGap.observe({...key,text:'First sentence. Unfinished'},'binding');
 const missing=await partialGap.next();assert(partialGap.admit(missing));
 partialGap.observe({...key,text:'First sentence. Second useful sentence. Unfinished'},'binding');
 const ahead=await partialGap.next();assert(partialGap.admit(ahead));partialGap.checkpoint(ahead,ahead.speech_start,ahead.speech_start+5);
 assert.equal((await partialGap.reconcile(canonical,async()=>{})).resume,0,'A later aligned prefix cannot hide the earlier unplayed range');
 let validateAgain;const rechecking=partialGap.reconcile(canonical,()=>new Promise(resolve=>{validateAgain=resolve;}));
 assert.equal(partialGap.resume(canonical),0,'Revalidating the same source cannot remove its known earlier-gap fence');
 assert.equal(partialGap.resume({...canonical,revision:'unverified-new-revision'}),undefined,'The previous gap fence is scoped to its validated revision');
 assert.equal(partialGap.checkpoint(ahead,ahead.speech_start,ahead.speech_start+6),undefined);
 validateAgain();assert.equal((await rechecking).resume,0);
 assert.equal(partialGap.ended(missing).resume,ahead.speech_start+6,'An earlier completed gap joins the later partial cursor');
 partialGap.reset();assert.equal(partialGap.checkpoint(ahead,ahead.speech_start,ahead.speech_start+10),undefined,'Entry reset retires timing state');
 for(const outcome of ['match','changed','offline','reset']){
   const pendingState=context.window.LiveSpeech.mount();pendingState.observe({...key,text:'A useful spoken sentence. Unfinished'},'binding');
   const selected=await pendingState.next();assert(pendingState.admit(selected));pendingState.checkpoint(selected,0,12);
   let release,reject;const validation=new Promise((yes,no)=>{release=yes;reject=no;});
   const mapping=pendingState.reconcile(canonical,()=>validation);
   assert.equal(pendingState.checkpoint(selected,0,20),undefined,'Pending validation cannot publish a canonical cursor');
   assert.equal(pendingState.resume(canonical),undefined);
   assert.equal(pendingState.continuation(selected),undefined);
   if(outcome==='reset'){pendingState.reset();release();assert.equal((await mapping).skip,true);assert.equal(pendingState.resume(canonical),undefined);}
   else if(outcome==='offline'){
     reject(new TypeError('Offline'));await assert.rejects(mapping,/Offline/);
     assert.equal(pendingState.checkpoint(selected,0,22),undefined,'A failed observation cannot expose an unverified canonical mapping');
   }else{
     if(outcome==='changed')reject(Object.assign(Error('Changed'),{status:409}));else release();
     assert.equal((await mapping).resume,outcome==='match'?20:0);
   }
 }
 clearTimeout(deadline);console.log('PASS live prefixes: useful boundary, Unicode hash, one owned attempt, canonical handover, manual cursor, changed source, network uncertainty, code and entry fences; zero media/providers.');
})().catch(error=>{console.error(error);process.exitCode=1;});
