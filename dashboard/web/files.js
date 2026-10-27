"use strict";
window.FileInput=(()=>{
 function mount({headers,getBinding,unauthorized}){
  const picker=document.querySelector('#conversation-files'),list=document.querySelector('#conversation-files-list'),status=document.querySelector('#conversation-files-state');
  let files=[],generation=0,preparing=false;
  function render(){list.replaceChildren();files.forEach(file=>{const row=document.createElement('div'),name=document.createElement('span'),remove=document.createElement('button');name.textContent=`${file.ref.name} · ${(file.ref.size/1024).toFixed(1)} KiB`;remove.type='button';remove.textContent='×';remove.setAttribute('aria-label',`Remove ${file.ref.name}`);remove.addEventListener('click',()=>{generation++;files=files.filter(f=>f!==file);render();});row.append(name,remove);list.append(row);});list.hidden=!files.length;}
  picker?.addEventListener('change',async()=>{
   const owned=++generation,binding=getBinding();preparing=true;status.textContent='Preparing original files…';
   try{
    const selected=Array.from(picker.files||[]);if(files.length+selected.length>8)throw Error('Attach at most eight files.');
    if(selected.some(f=>f.size>20*1024*1024)||[...files.map(f=>f.ref),...selected].reduce((n,f)=>n+f.size,0)>40*1024*1024)throw Error('At most20MiB per file and40MiB total.');
    const prepared=[];
    for(const file of selected){const blob=await file.arrayBuffer();const sha256=Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256',blob)),x=>x.toString(16).padStart(2,'0')).join('');prepared.push({blob,ref:{file_id:crypto.randomUUID(),sha256,name:file.name,mime:file.type||'application/octet-stream',size:file.size},uploaded:false});}
    if(owned!==generation||binding!==getBinding())return;files.push(...prepared);render();status.textContent='Original files selected. Send with your message or recording.';
   }catch(e){status.textContent=e.message||'File preparation failed. Nothing was sent.';}finally{if(owned===generation)preparing=false;picker.value='';}
  });
  async function capture(){
   if(preparing)throw Error('Wait for file preparation before sending.');
   const owned=generation,binding=getBinding();const selected=files.slice();
   for(const file of selected){
    if(!file.uploaded){status.textContent=`Saving ${file.ref.name}…`;let response;try{response=await fetch('api/interaction/files',{method:'POST',headers:{...headers(),'Content-Type':'application/octet-stream','X-File-Reference':encodeURIComponent(JSON.stringify(file.ref))},body:file.blob});}catch{throw Error('File upload unconfirmed. Keep the selection and check your connection.');}
     if(response.status===401||response.status===403)unauthorized();if(!response.ok)throw Error('Original file upload failed; no message was submitted.');const value=await response.json();for(const key of Object.keys(file.ref))if(value[key]!==file.ref[key])throw Error('File acknowledgement differs from the selected original.');file.uploaded=true;
    }
    if(owned!==generation||binding!==getBinding())throw Error('File selection changed. Review it before sending.');
   }
   status.textContent=selected.length?'Original bytes saved; waiting for message receipt.':'';return selected.map(f=>({...f.ref}));
  }
  function consumed(refs){const ids=new Set((refs||[]).map(r=>r.file_id));files=files.filter(f=>!ids.has(f.ref.file_id));render();status.textContent='';}
  function reset(){generation++;preparing=false;files=[];render();status.textContent='';if(picker)picker.value='';}
  return {capture,consumed,reset,hasFiles:()=>preparing||files.length>0};
 }
 return {mount};
})();
