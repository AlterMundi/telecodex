"use strict";
window.VisualInput = (() => {
  function mount({headers,getBinding,unauthorized}) {
    const file=document.querySelector('#conversation-image'),canvas=document.querySelector('#conversation-image-view');
    const box=document.querySelector('#conversation-visual'),clear=document.querySelector('#conversation-image-clear');
    const status=document.querySelector('#conversation-image-state'),x=document.querySelector('#conversation-image-x'),y=document.querySelector('#conversation-image-y');
    let generation=0,chosen,point,selectedAt,selectionState="empty";
    function reset(clearFile=true,nextState="empty"){generation++;chosen?.bitmap.close();chosen=point=selectedAt=undefined;selectionState=nextState;if(clearFile)file.value='';canvas.hidden=true;clear.hidden=nextState==='empty';x.value=y.value='';x.disabled=y.disabled=true;status.textContent='Choose an image, then point to what you mean.';}
    function draw(){if(!chosen)return;const c=canvas.getContext('2d');c.drawImage(chosen.bitmap,0,0,canvas.width,canvas.height);if(point){const r=Math.max(6,canvas.width/80);c.strokeStyle='#ffffff';c.lineWidth=Math.max(3,canvas.width/200);c.beginPath();c.arc(point.x,point.y,r,0,2*Math.PI);c.stroke();c.strokeStyle='#111111';c.lineWidth=Math.max(1,canvas.width/500);c.stroke();}}
    function select(px,py){if(!chosen||!Number.isInteger(px)||!Number.isInteger(py)||px<0||py<0||px>=canvas.width||py>=canvas.height){point=undefined;status.textContent='Select one point inside this image.';return;}point={x:px,y:py};selectedAt=new Date().toISOString();x.value=String(px);y.value=String(py);draw();status.textContent='Point selected. Your next message will include this captured image.';}
    file.addEventListener('change',async()=>{
      const input=file.files?.[0];reset(false);if(!input)return;const owned=generation;selectionState='loading';clear.hidden=false;status.textContent='Opening image…';
      try {
        if(input.size>20*1024*1024)throw Error('Choose an image smaller than 20 MiB.');
        const bitmap=await createImageBitmap(input,{imageOrientation:'from-image'});
        if(owned!==generation){bitmap.close();return;}
        const scale=Math.min(1,2048/bitmap.width,2048/bitmap.height);
        canvas.width=Math.max(1,Math.round(bitmap.width*scale));canvas.height=Math.max(1,Math.round(bitmap.height*scale));
        const original=new OffscreenCanvas(canvas.width,canvas.height);original.getContext('2d').drawImage(bitmap,0,0,canvas.width,canvas.height);bitmap.close();
        const blob=await original.convertToBlob({type:'image/png'});
        if(blob.size>2*1024*1024)throw Error('This image is too detailed. Choose a smaller view.');
        const image_sha256=Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256',await blob.arrayBuffer())),v=>v.toString(16).padStart(2,'0')).join('');
        const preview=await createImageBitmap(blob);
        if(owned!==generation){preview.close();return;}
        chosen={blob,bitmap:preview,image_sha256,source_id:crypto.randomUUID(),captured_at:new Date().toISOString(),uploaded:false};
        selectionState='ready';canvas.hidden=false;clear.hidden=false;x.disabled=y.disabled=false;x.max=String(canvas.width-1);y.max=String(canvas.height-1);draw();status.textContent='Point to what you mean in this image. You can also enter X and Y.';
      }catch(e){if(owned===generation){reset(false,'failed');status.textContent=['Choose an image smaller than 20 MiB.','This image is too detailed. Choose a smaller view.'].includes(e.message)?e.message:'Image unavailable. Choose another view.';}}
    });
    canvas.addEventListener('click',e=>{const r=canvas.getBoundingClientRect();select(Math.min(canvas.width-1,Math.floor((e.clientX-r.left)*canvas.width/r.width)),Math.min(canvas.height-1,Math.floor((e.clientY-r.top)*canvas.height/r.height)));});
    canvas.addEventListener('keydown',e=>{const delta={ArrowLeft:[-1,0],ArrowRight:[1,0],ArrowUp:[0,-1],ArrowDown:[0,1]}[e.key];if(!delta||!chosen)return;e.preventDefault();const p=point||{x:Math.floor(canvas.width/2),y:Math.floor(canvas.height/2)},step=e.shiftKey?10:1;select(Math.max(0,Math.min(canvas.width-1,p.x+delta[0]*step)),Math.max(0,Math.min(canvas.height-1,p.y+delta[1]*step)));});
    for(const field of [x,y])field.addEventListener('change',()=>select(x.value===''?NaN:Number(x.value),y.value===''?NaN:Number(y.value)));
    clear.addEventListener('click',reset);
    async function capture(){
      if(selectionState==='loading')throw Error('Wait for the selected image to open, or remove it before sending.');
      if(selectionState==='failed')throw Error('The selected image is unavailable. Choose another image or remove it.');
      if(!chosen)return undefined;
      if(!point)throw Error('Point to what you mean in the image before sending.');
      if(Date.now()-Date.parse(selectedAt)>300000)throw Error('Select the image again to refresh your point.');
      const owned=generation,image=chosen,binding=getBinding();
      const reference={source_kind:'selected_image',source_id:image.source_id,image_sha256:image.image_sha256,width:canvas.width,height:canvas.height,captured_at:image.captured_at,selected_at:selectedAt,selected_revision:image.image_sha256,state:'selected',point:{...point}};
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
    reset();return {capture,consumed,reset};
  }
  return {mount};
})();
