"use strict";
const assert=require("node:assert/strict"),vm=require("node:vm"),fs=require("node:fs"),{webcrypto,createHash}=require("node:crypto");
const hash=value=>createHash("sha256").update(value).digest("hex");
function fixture(status="saved"){
  const window=new EventTarget(),commands=[],received=[];let picker;
  const emit=value=>window.DaimonAudio.onmessage({data:JSON.stringify({type:"file",...value})});
  window.DaimonAudio={postMessage(text){
    const v=JSON.parse(text);commands.push(v);if(!v.command.startsWith("file_")||v.command==="file_cancel")return;
    const stage=v.command.slice(5);
    if(stage==="chunk")received.push(Buffer.from(v.data,"base64"));
    queueMicrotask(()=>{
      if(stage==="ready"){
        const begin=commands.find(c=>c.command==="file_begin"),bytes=Buffer.concat(received);
        if(begin.bytes!==bytes.length || begin.sha256!==hash(bytes)){emit({id:v.id,status:"error",message:"Digest mismatch"});return;}
        picker=()=>emit({id:v.id,stage:"result",status});
      }
      emit({id:v.id,stage,sequence:v.sequence,status:"ok"});
    });
  }};
  const context={window,navigator:{},Event,EventTarget,crypto:webcrypto,Blob,Uint8Array,Map,Set,DOMException,setTimeout,clearTimeout,
    btoa:s=>Buffer.from(s,"binary").toString("base64"),console};
  for(const name of ["native-files.js","native-audio.js"])vm.runInNewContext(fs.readFileSync(__dirname+"/app/src/main/assets/"+name,"utf8"),context);
  return {window,commands,received,emit,pick:()=>picker?.(),ready:()=>!!picker};
}
async function until(test){for(let n=0;n<200;n++){if(test())return;await new Promise(r=>setTimeout(r,5));}throw Error("Save did not reach picker");}
(async()=>{
  const f=fixture(),bytes=Buffer.alloc(70000,7),blob=new Blob([bytes],{type:"application/pdf"});
  const saved=f.window.NativeFiles.save(blob,"../copia.pdf",hash(bytes));await until(f.ready);
  assert.equal(f.commands.find(c=>c.command==="file_begin").name,"copia.pdf");
  assert.equal(f.commands.filter(c=>c.command==="file_chunk").length,3);
  assert(f.received.every(c=>c.length<=32768));assert.deepEqual(Buffer.concat(f.received),bytes);
  await assert.rejects(f.window.NativeFiles.save(blob,"another.pdf"),/current save/);
  let done=false;saved.then(()=>done=true);
  f.emit({id:"export_stale",stage:"result",status:"saved"});await Promise.resolve();assert.equal(done,false,"Late unrelated result cannot claim saved");
  f.pick();assert.equal(await saved,true);assert.equal(f.commands.at(-1).command,"file_cancel");
  const cancelled=fixture("cancelled"),pending=cancelled.window.NativeFiles.save(new Blob([]),"empty.txt");
  await until(cancelled.ready);cancelled.pick();assert.equal(await pending,false,"Picker cancellation is not a save");
  const bad=fixture();await assert.rejects(bad.window.NativeFiles.save(blob,"bad.pdf","0".repeat(64)),/Digest mismatch/);
  assert.equal(bad.commands.at(-1).command,"file_cancel");
  assert.notEqual(f.commands.find(c=>c.command==="file_begin").id,cancelled.commands.find(c=>c.command==="file_begin").id,"Export identity survives page serial reset");
  await assert.rejects(f.window.NativeFiles.save({size:256*1024*1024+1},"oversized"),/limit/);
  assert(f.commands.every(c=>!c.command.includes("input")&&!c.command.includes("transcri")),"Saving is not message dispatch or transcription");
  console.log("Native file adapter: chunk bounds/order, verified metadata, unique IDs, picker receipt/cancel, stale result, single-save bound and no dispatch passed.");
})().catch(error=>{console.error(error);process.exitCode=1;});
