// Exercise the shipped image selector through user actions, with delayed decoding.
const assert=require('node:assert/strict'),fs=require('node:fs'),vm=require('node:vm'),crypto=require('node:crypto').webcrypto;
const deferred=()=>{let resolve,reject;const promise=new Promise((yes,no)=>{resolve=yes;reject=no;});return {promise,resolve,reject};};
function boot(){
 const nodes=new Map(),uploads=[];let decode;
 function element(){return {value:'',files:[],hidden:false,disabled:false,handlers:{},textContent:'',width:0,height:0,
  addEventListener(k,f){this.handlers[k]=f;},getContext(){return {drawImage(){},beginPath(){},arc(){},stroke(){}};},getBoundingClientRect(){return {left:0,top:0,width:200,height:100};}};}
 const document={querySelector(name){if(!nodes.has(name))nodes.set(name,element());return nodes.get(name);}};
 const bitmap=()=>({width:200,height:100,closed:false,close(){this.closed=true;}});
 class Canvas{constructor(w,h){this.width=w;this.height=h;}getContext(){return {drawImage(){}};}async convertToBlob(){return new Blob(['synthetic image bytes'],{type:'image/png'});}}
 const context=vm.createContext({window:{},document,crypto,Blob,Uint8Array,AbortController,OffscreenCanvas:Canvas,setTimeout,clearTimeout,
  createImageBitmap(){if(decode){const held=decode;decode=undefined;return held.promise;}return Promise.resolve(bitmap());},
  async fetch(route,options){uploads.push({route,options});return {ok:true,json:async()=>({image_sha256:options.headers['X-Image-Sha256'],width:200,height:100})};}});
 vm.runInContext(fs.readFileSync('dashboard/web/visual.js','utf8'),context);
 const ui=context.window.VisualInput.mount({headers:()=>({}),getBinding:()=> 'entry-binding',unauthorized(){throw Error('Unexpected auth failure');}});
 return {ui,nodes,uploads,bitmap,
  hold(){decode=deferred();return decode;},
  choose(){nodes.get('#conversation-image').files=[{name:'selected.png',size:100}];nodes.get('#conversation-image').value='selected.png';return nodes.get('#conversation-image').handlers.change();},
  remove(){nodes.get('#conversation-image-clear').handlers.click();},
  point(){nodes.get('#conversation-image-view').handlers.click({clientX:25,clientY:30});}};
}
(async()=>{
 const f=boot(),held=f.hold(),opening=f.choose();
 await assert.rejects(f.ui.capture(),/Wait for the selected image/);assert.equal(f.uploads.length,0);assert(!f.nodes.get('#conversation-image-clear').hidden);
 held.resolve(f.bitmap());await opening;
 await assert.rejects(f.ui.capture(),/Point to what you mean/);assert.equal(f.uploads.length,0);
 f.point();const selected=await f.ui.capture();assert.equal(selected.point.x,25);assert.equal(selected.point.y,30);assert.equal(f.uploads.length,1);assert.equal(f.nodes.get('#conversation-image').value,'selected.png');
 const camera=boot();camera.nodes.get('#conversation-camera').files=[{name:'camera.jpg',size:100}];await camera.nodes.get('#conversation-camera').handlers.change();camera.point();const photo=await camera.ui.capture();assert(photo.image_sha256);assert.equal(camera.uploads.length,1);camera.ui.consumed(photo);assert.equal(camera.nodes.get('#conversation-camera').value,'');
 const failed=boot(),bad=failed.hold(),failure=failed.choose();bad.reject(Error('decode failed'));await failure;
 await assert.rejects(failed.ui.capture(),/selected image is unavailable/);assert(!failed.nodes.get('#conversation-image-clear').hidden);assert.equal(failed.uploads.length,0);
 failed.remove();assert.equal(await failed.ui.capture(),undefined,'Only explicit removal makes an unavailable selection empty');
 const removed=boot(),pending=removed.hold(),loading=removed.choose();removed.remove();const stale=removed.bitmap();pending.resolve(stale);await loading;
 assert(stale.closed,'Late decoding must release its stale bitmap');assert.equal(await removed.ui.capture(),undefined);assert.equal(removed.uploads.length,0);
 console.log('Visual UI: held/failed decode never drops an intended image, point required, upload only on send, filename retained and explicit removal invalidates late decoding; zero models.');
})().catch(error=>{console.error(error);process.exitCode=1;});
