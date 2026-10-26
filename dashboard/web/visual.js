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
    const rail=document.querySelector('#conversation-image-list'),summary=box.querySelector('summary');
    const MAX_IMAGES=8;
    let generation=0,images=[],active;
    function button(label,text,action){
      const result=document.createElement('button');result.type='button';result.textContent=text;
      result.setAttribute('aria-label',label);result.title=label;result.addEventListener('click',action);return result;
    }
    function draw(){
      if(active?.state!=='ready')return;
      const c=canvas.getContext('2d');c.drawImage(active.bitmap.element||active.bitmap,0,0,canvas.width,canvas.height);
      if(active.point){const r=Math.max(6,canvas.width/80);c.strokeStyle='#fff';c.lineWidth=Math.max(3,canvas.width/200);c.beginPath();c.arc(active.point.x,active.point.y,r,0,2*Math.PI);c.stroke();c.strokeStyle='#111';c.lineWidth=Math.max(1,canvas.width/500);c.stroke();}
    }
    function render(){
      if(!images.includes(active))active=images[0];
      rail.replaceChildren();
      images.forEach((image,index)=>{
        const item=document.createElement('div');item.className='image-attachment';
        const pick=button(`Image ${index+1}: ${image.name}`,image.name,()=>{active=image;render();});
        pick.className='image-thumbnail';pick.setAttribute('aria-pressed',String(active===image));
        if(image.state==='ready'){
          const thumb=document.createElement('canvas');thumb.width=80;thumb.height=60;thumb.setAttribute('aria-hidden','true');
          const scale=Math.min(80/image.width,60/image.height);thumb.getContext('2d').drawImage(image.bitmap.element||image.bitmap,(80-image.width*scale)/2,(60-image.height*scale)/2,image.width*scale,image.height*scale);
          const name=document.createElement('span');name.textContent=image.name;pick.replaceChildren(thumb,name);
        }else pick.textContent=image.state==='loading'?'Opening…':image.name+' — unavailable';
        const controls=document.createElement('div');controls.className='image-attachment-actions';
        const left=button(`Move image ${index+1} earlier`,'←',()=>move(image,-1));left.disabled=index===0;
        const right=button(`Move image ${index+1} later`,'→',()=>move(image,1));right.disabled=index===images.length-1;
        controls.append(left,right,button(`Remove image ${index+1}`,'×',()=>remove(image)));item.append(pick,controls);rail.append(item);
      });
      rail.hidden=!images.length;clear.hidden=!active;
      if(summary){summary.setAttribute('data-count',String(images.length));summary.setAttribute('aria-label',images.length?`Attach images (${images.length} selected)`:'Attach images');}
      const ready=active?.state==='ready';canvas.hidden=!ready;if(coordinates)coordinates.hidden=!ready;x.disabled=y.disabled=!ready;
      x.value=ready&&active.point?String(active.point.x):'';y.value=ready&&active.point?String(active.point.y):'';
      if(ready){canvas.width=active.width;canvas.height=active.height;x.max=String(active.width-1);y.max=String(active.height-1);draw();}
      status.textContent=!active?'':active.state==='failed'?active.error:active.state==='loading'?'Opening image…':`${images.length} image${images.length===1?'':'s'} attached. Marking a point is optional.`;
    }
    function reset(){generation++;for(const image of images)image.bitmap?.close();images=[];active=undefined;file.value='';if(camera)camera.value='';render();}
    function remove(image){if(!image)return;generation++;images=images.filter(item=>item!==image);image.bitmap?.close();if(!images.length){file.value='';if(camera)camera.value='';}render();}
    function move(image,delta){const from=images.indexOf(image),to=from+delta;if(from<0||to<0||to>=images.length)return;generation++;[images[from],images[to]]=[images[to],images[from]];render();}
    function select(px,py){
      if(active?.state!=='ready')return;
      if(!Number.isInteger(px)||!Number.isInteger(py)||px<0||py<0||px>=active.width||py>=active.height){status.textContent='Select one point inside this image.';return;}
      generation++;active.point={x:px,y:py};active.selected_at=new Date().toISOString();x.value=String(px);y.value=String(py);draw();
    }
    async function prepare(image,input){
      if(!images.includes(image))return;
      try{
        if(input.size>20*1024*1024)throw Error('Choose an image smaller than 20 MiB.');
        const bitmap=await decode(input);
        if(!images.includes(image)){bitmap.close();return;}
        const scale=Math.min(1,2048/bitmap.width,2048/bitmap.height),limit=2*1024*1024;
        let width=Math.max(1,Math.round(bitmap.width*scale)),height=Math.max(1,Math.round(bitmap.height*scale)),blob;
        try{
          while(true){
            if(!images.includes(image))return;
            blob=await encode(bitmap,width,height);
            if(!images.includes(image))return;
            if(blob.size<=limit)break;
            const factor=Math.min(.85,Math.sqrt(limit/blob.size)*.9),nextWidth=Math.max(1,Math.floor(width*factor)),nextHeight=Math.max(1,Math.floor(height*factor));
            if(nextWidth===width&&nextHeight===height)throw Error('Image encoding unavailable');
            width=nextWidth;height=nextHeight;
          }
        }finally{bitmap.close();}
        const hash=Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256',await blob.arrayBuffer())),v=>v.toString(16).padStart(2,'0')).join('');
        const preview=await decode(blob);
        if(!images.includes(image)){preview.close();return;}
        const now=new Date().toISOString();Object.assign(image,{state:'ready',blob,bitmap:preview,image_sha256:hash,width,height,captured_at:now,selected_at:now,uploaded:false,point:null});render();
      }catch(error){
        if(!images.includes(image))return;
        image.state='failed';image.error=error.message==='Choose an image smaller than 20 MiB.'?error.message:'Cannot open this image. Choose another or remove it; your text is kept.';active=image;render();
      }
    }
    async function choose(source){
      const selected=Array.from(source.files||[]);if(!selected.length)return;generation++;
      if(images.length+selected.length>MAX_IMAGES){
        const failed=images.find(image=>image.overflow)||{name:'Selection not added',state:'failed',overflow:true,error:'Attach up to eight images. Remove this selection and choose fewer images.'};if(!images.includes(failed))images.push(failed);active=failed;render();return;
      }
      const added=selected.map(input=>({name:input.name||'Photo',source_id:crypto.randomUUID(),state:'loading',input}));images.push(...added);active=added[0];render();
      // Keep admission order even when one image fails; no intended file is dropped.
      for(const image of added){const input=image.input;delete image.input;await prepare(image,input);}
      source.value='';
    }
    file.addEventListener('change',()=>choose(file));camera?.addEventListener('change',()=>choose(camera));
    canvas.addEventListener('click',event=>{const r=canvas.getBoundingClientRect();select(Math.min(canvas.width-1,Math.floor((event.clientX-r.left)*canvas.width/r.width)),Math.min(canvas.height-1,Math.floor((event.clientY-r.top)*canvas.height/r.height)));});
    canvas.addEventListener('keydown',event=>{const delta={ArrowLeft:[-1,0],ArrowRight:[1,0],ArrowUp:[0,-1],ArrowDown:[0,1]}[event.key];if(!delta||active?.state!=='ready')return;event.preventDefault();const p=active.point||{x:Math.floor(active.width/2),y:Math.floor(active.height/2)},step=event.shiftKey?10:1;select(Math.max(0,Math.min(active.width-1,p.x+delta[0]*step)),Math.max(0,Math.min(active.height-1,p.y+delta[1]*step)));});
    for(const field of [x,y])field.addEventListener('change',()=>select(x.value===''?NaN:Number(x.value),y.value===''?NaN:Number(y.value)));
    clear.addEventListener('click',()=>remove(active));
    async function capture(){
      if(images.some(image=>image.state==='loading'))throw Error('Wait for the selected images to open, or remove them before sending.');
      if(images.some(image=>image.state==='failed'))throw Error('A selected image is unavailable. Choose another image or remove it.');
      if(!images.length)return undefined;
      const owned=generation,binding=getBinding(),snapshot=images.map(image=>({image,reference:{source_kind:'selected_image',source_id:image.source_id,image_sha256:image.image_sha256,width:image.width,height:image.height,captured_at:image.captured_at,selected_at:image.selected_at,selected_revision:image.image_sha256,state:'selected',point:image.point?{...image.point}:null}}));
      if(snapshot.some(({reference})=>Date.now()-Date.parse(reference.selected_at)>300000))throw Error('Select the images again to refresh your selection.');
      for(const {image,reference} of snapshot){
        if(!image.uploaded){
          const controller=new AbortController(),timeout=setTimeout(()=>controller.abort(),20000);
          try{
            const response=await fetch('api/interaction/visual',{method:'POST',credentials:'same-origin',signal:controller.signal,headers:{...headers(),'Content-Type':'image/png','X-Image-Sha256':image.image_sha256},body:image.blob});
            const result=await response.json();
            if(!response.ok){if([401,403].includes(response.status))unauthorized({status:response.status});throw Error('Image not confirmed. Retry before sending or remove it.');}
            if(result.image_sha256!==image.image_sha256||result.width!==reference.width||result.height!==reference.height)throw Error('Image changed before sending. Select it again.');
            image.uploaded=true;
          }finally{clearTimeout(timeout);}
        }
        if(owned!==generation||binding!==getBinding())throw Error('Your selected images changed. Review them before sending.');
      }
      const references=snapshot.map(item=>item.reference);return references.length===1?references[0]:references;
    }
    function consumed(reference){
      if(!reference)return;
      const references=Array.isArray(reference)?reference:[reference];
      for(const image of [...images])if(references.some(ref=>ref.source_id===image.source_id&&ref.selected_at===image.selected_at&&ref.image_sha256===image.image_sha256&&JSON.stringify(ref.point)===JSON.stringify(image.point)))remove(image);
    }
    reset();return {capture,captureForVoice:capture,consumed,reset};
  }
  return {mount};
})();
