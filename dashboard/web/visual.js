"use strict";
window.VisualInput = (() => {
  function mount({headers,getBinding,unauthorized}) {
    const file=document.querySelector('#conversation-image'),canvas=document.querySelector('#conversation-image-view');
    const camera=document.querySelector('#conversation-camera');
    const box=document.querySelector('#conversation-visual'),clear=document.querySelector('#conversation-image-clear');
    const status=document.querySelector('#conversation-image-state'),x=document.querySelector('#conversation-image-x'),y=document.querySelector('#conversation-image-y');
    const coordinates=document.querySelector('#conversation-image-coordinates');
    async function decode(input){
      try {if(typeof createImageBitmap==='function')return await createImageBitmap(input,{imageOrientation:'from-image'});}catch{}
      if(!window.Image)throw Error('Image decoder unavailable');
      const data=await new Promise((resolve,reject)=>{const reader=new FileReader();reader.onload=()=>resolve(reader.result);reader.onerror=reject;reader.readAsDataURL(input);});
      const image=new window.Image();
      await new Promise((resolve,reject)=>{image.onload=resolve;image.onerror=reject;image.src=data;});
      return {width:image.naturalWidth,height:image.naturalHeight,element:image,close(){}};
    }
    async function encode(bitmap,width,height){
      const element=bitmap.element||bitmap;
      if(typeof OffscreenCanvas==='function'){
        try {const target=new OffscreenCanvas(width,height);target.getContext('2d').drawImage(element,0,0,width,height);return await target.convertToBlob({type:'image/png'});}catch{}
      }
      const target=document.createElement('canvas');target.width=width;target.height=height;target.getContext('2d').drawImage(element,0,0,width,height);
      return await new Promise((resolve,reject)=>target.toBlob(blob=>blob?resolve(blob):reject(Error('Image encoding unavailable')),'image/png'));
    }
    let generation=0,chosen,point,selectedAt,selectionState="empty";
    function reset(clearFile=true,nextState="empty"){generation++;chosen?.bitmap.close();chosen=point=selectedAt=undefined;selectionState=nextState;if(clearFile){file.value='';if(camera)camera.value='';}canvas.hidden=true;if(coordinates)coordinates.hidden=true;clear.hidden=nextState==='empty';x.value=y.value='';x.disabled=y.disabled=true;status.textContent='Choose an image. Marking a point is optional.';}
    function draw(){if(!chosen)return;const c=canvas.getContext('2d');c.drawImage(chosen.bitmap.element||chosen.bitmap,0,0,canvas.width,canvas.height);if(point){const r=Math.max(6,canvas.width/80);c.strokeStyle='#ffffff';c.lineWidth=Math.max(3,canvas.width/200);c.beginPath();c.arc(point.x,point.y,r,0,2*Math.PI);c.stroke();c.strokeStyle='#111111';c.lineWidth=Math.max(1,canvas.width/500);c.stroke();}}
    function select(px,py){if(!chosen||!Number.isInteger(px)||!Number.isInteger(py)||px<0||py<0||px>=canvas.width||py>=canvas.height){point=undefined;status.textContent='Select one point inside this image.';return;}point={x:px,y:py};selectedAt=new Date().toISOString();x.value=String(px);y.value=String(py);draw();status.textContent='Point selected. Your next message will include this captured image.';}
    async function choose(source){
      const input=source.files?.[0];if(!input)return;reset(false);if(source!==file)file.value='';if(camera && source!==camera)camera.value='';const owned=generation;selectionState='loading';clear.hidden=false;status.textContent='Opening image…';
      try {
        if(input.size>20*1024*1024)throw Error('Choose an image smaller than 20 MiB.');
        const bitmap=await decode(input);
        if(owned!==generation){bitmap.close();return;}
        // The human chooses an ordinary photo; only our internal PNG has the 2 MiB bound.
        const scale=Math.min(1,2048/bitmap.width,2048/bitmap.height),limit=2*1024*1024;
        let width=Math.max(1,Math.round(bitmap.width*scale)),height=Math.max(1,Math.round(bitmap.height*scale)),blob;
        try {
          while(true){
            if(owned!==generation)return;
            blob=await encode(bitmap,width,height);
            if(owned!==generation)return;
            if(blob.size<=limit)break;
            // PNG complexity varies by photo. Fit encoded bytes as well as dimensions.
            const factor=Math.min(0.85,Math.sqrt(limit/blob.size)*0.9);
            const nextWidth=Math.max(1,Math.floor(width*factor)),nextHeight=Math.max(1,Math.floor(height*factor));
            if(nextWidth===width && nextHeight===height)throw Error('Image encoding unavailable');
            width=nextWidth;height=nextHeight;status.textContent='Preparing your photo…';
          }
        } finally {bitmap.close();}
        if(owned!==generation)return;
        canvas.width=width;canvas.height=height;
        const image_sha256=Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256',await blob.arrayBuffer())),v=>v.toString(16).padStart(2,'0')).join('');
        const preview=await decode(blob);
        if(owned!==generation){preview.close();return;}
        chosen={blob,bitmap:preview,image_sha256,source_id:crypto.randomUUID(),captured_at:new Date().toISOString(),uploaded:false};
        selectedAt=chosen.captured_at;selectionState='ready';canvas.hidden=false;if(coordinates)coordinates.hidden=false;clear.hidden=false;x.disabled=y.disabled=false;x.max=String(canvas.width-1);y.max=String(canvas.height-1);draw();status.textContent='Image attached. You can optionally mark a point.';
      }catch(e){if(owned===generation){reset(false,'failed');status.textContent=e.message==='Choose an image smaller than 20 MiB.'?e.message:'Cannot open this image. Try a JPEG, PNG or WebP, or remove it; your text is kept.';}}
    }
    file.addEventListener('change',()=>choose(file));
    camera?.addEventListener('change',()=>choose(camera));
    canvas.addEventListener('click',e=>{const r=canvas.getBoundingClientRect();select(Math.min(canvas.width-1,Math.floor((e.clientX-r.left)*canvas.width/r.width)),Math.min(canvas.height-1,Math.floor((e.clientY-r.top)*canvas.height/r.height)));});
    canvas.addEventListener('keydown',e=>{const delta={ArrowLeft:[-1,0],ArrowRight:[1,0],ArrowUp:[0,-1],ArrowDown:[0,1]}[e.key];if(!delta||!chosen)return;e.preventDefault();const p=point||{x:Math.floor(canvas.width/2),y:Math.floor(canvas.height/2)},step=e.shiftKey?10:1;select(Math.max(0,Math.min(canvas.width-1,p.x+delta[0]*step)),Math.max(0,Math.min(canvas.height-1,p.y+delta[1]*step)));});
    for(const field of [x,y])field.addEventListener('change',()=>select(x.value===''?NaN:Number(x.value),y.value===''?NaN:Number(y.value)));
    clear.addEventListener('click',reset);
    async function capture(){
      if(selectionState==='loading')throw Error('Wait for the selected image to open, or remove it before sending.');
      if(selectionState==='failed')throw Error('The selected image is unavailable. Choose another image or remove it.');
      if(!chosen)return undefined;
      if(Date.now()-Date.parse(selectedAt)>300000)throw Error('Select the image again to refresh your selection.');
      const owned=generation,image=chosen,binding=getBinding();
      const reference={source_kind:'selected_image',source_id:image.source_id,image_sha256:image.image_sha256,width:canvas.width,height:canvas.height,captured_at:image.captured_at,selected_at:selectedAt,selected_revision:image.image_sha256,state:'selected',point:point?{...point}:null};
      if(!image.uploaded){
        const controller=new AbortController(),timeout=setTimeout(()=>controller.abort(),20000);
        try {
          const response=await fetch('api/interaction/visual',{method:'POST',credentials:'same-origin',signal:controller.signal,headers:{...headers(),'Content-Type':'image/png','X-Image-Sha256':image.image_sha256},body:image.blob});
          const result=await response.json();
          if(!response.ok){if([401,403].includes(response.status))unauthorized({status:response.status});throw Error('Image not confirmed. Select it again or retry before sending.');}
          if(result.image_sha256!==image.image_sha256||result.width!==reference.width||result.height!==reference.height)throw Error('Image changed before sending. Select it again.');
          image.uploaded=true;
        }finally{clearTimeout(timeout);}
      }
      if(owned!==generation||binding!==getBinding()||selectedAt!==reference.selected_at)throw Error('Your selected view changed. Review it before sending.');
      return reference;
    }
    function consumed(reference){if(reference&&chosen?.source_id===reference.source_id&&selectedAt===reference.selected_at)reset();}
    reset();return {capture,captureForVoice:capture,consumed,reset};
  }
  return {mount};
})();
