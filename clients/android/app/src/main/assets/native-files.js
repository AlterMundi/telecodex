"use strict";
/* Verified in-page bytes only. Destination authority stays with Android's picker. */
window.NativeFiles=(()=>{
  const bridge=window.DaimonAudio;
  if(!bridge?.postMessage)return;
  let active;
  const send=value=>bridge.postMessage(JSON.stringify(value));
  function receive(value){
    if(!active || value.id!==active.id)return;
    if(value.status==="error"){
      for(const pending of active.waiters.values()){clearTimeout(pending.timer);pending.reject(Error(value.message||"File save failed. The original remains available."));}
      active.waiters.clear();return;
    }
    const key=value.stage+(value.stage==="chunk"?":"+value.sequence:"");
    const pending=active.waiters.get(key);
    if(pending){clearTimeout(pending.timer);active.waiters.delete(key);pending.resolve(value);}
  }
  async function save(blob,name,sha256){
    if(active)throw Error("Finish the current save first.");
    if(!blob || !Number.isSafeInteger(blob.size) || blob.size<0 || blob.size>256*1024*1024)throw Error("File exceeds the256MiB save limit.");
    const owned={id:"export_"+crypto.randomUUID(),waiters:new Map()};active=owned;
    function wait(key,millis=30000){return new Promise((resolve,reject)=>{
      const timer=setTimeout(()=>{owned.waiters.delete(key);reject(Error("Save timed out. The original remains available; the selected destination may be incomplete."));},millis);
      owned.waiters.set(key,{resolve,reject,timer});
    });}
    async function exchange(command,value={},key=command.slice(5)){
      const response=wait(key);response.catch(()=>{});send({command,id:owned.id,...value});return response;
    }
    try{
      const hash=sha256===undefined?[...new Uint8Array(await crypto.subtle.digest("SHA-256",await blob.arrayBuffer()))].map(n=>n.toString(16).padStart(2,"0")).join(""):sha256;
      if(!/^[a-f0-9]{64}$/.test(hash))throw Error("File verification metadata unavailable.");
      const title=String(name||"download").split(/[\\/]/).pop().replace(/[\x00-\x1f\x7f-\x9f]/g,"").slice(0,180)||"download";
      await exchange("file_begin",{name:title,mime:blob.type||"application/octet-stream",bytes:blob.size,sha256:hash});
      let sequence=0;
      for(let offset=0;offset<blob.size;offset+=32768){
        const bytes=new Uint8Array(await blob.slice(offset,offset+32768).arrayBuffer());
        await exchange("file_chunk",{sequence,data:btoa(String.fromCharCode(...bytes))},"chunk:"+sequence);sequence++;
      }
      const saved=wait("result",600000);saved.catch(()=>{});
      await exchange("file_ready");
      const result=await saved;
      return result.status==="saved";
    }finally{
      send({command:"file_cancel",id:owned.id});
      for(const pending of owned.waiters.values()){clearTimeout(pending.timer);pending.reject(Error("Save ended"));}
      owned.waiters.clear();if(active===owned)active=undefined;
    }
  }
  return {save,receive};
})();
