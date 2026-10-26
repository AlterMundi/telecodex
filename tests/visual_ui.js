// Exercise the shipped image selector through user actions, with delayed decoding.
const assert=require('node:assert/strict'),fs=require('node:fs'),vm=require('node:vm'),crypto=require('node:crypto').webcrypto;
const deferred=()=>{let resolve,reject;const promise=new Promise((yes,no)=>{resolve=yes;reject=no;});return {promise,resolve,reject};};
function boot({photo=false}={}){
 const nodes=new Map(),uploads=[],encodings=[];let decode;
 function element(){return {value:'',files:[],hidden:false,disabled:false,handlers:{},textContent:'',width:0,height:0,children:[],attributes:{},
  append(...children){this.children.push(...children);},replaceChildren(...children){this.children=children;},setAttribute(k,v){this.attributes[k]=v;},querySelector(){return undefined;},
  addEventListener(k,f){this.handlers[k]=f;},getContext(){return {drawImage(){},beginPath(){},arc(){},stroke(){}};},getBoundingClientRect(){return {left:0,top:0,width:200,height:100};}};}
 const document={createElement(){return element();},querySelector(name){if(!nodes.has(name))nodes.set(name,element());return nodes.get(name);}};
 const bitmap=()=>({width:photo?4000:200,height:photo?3000:100,closed:false,close(){this.closed=true;}});
 class Canvas{constructor(w,h){this.width=w;this.height=h;}getContext(){return {drawImage(){}};}async convertToBlob(){encodings.push([this.width,this.height]);return new Blob([photo?new Uint8Array(this.width*this.height*3):'synthetic image bytes'],{type:'image/png'});}}
 const context=vm.createContext({window:{},document,crypto,Blob,Uint8Array,AbortController,OffscreenCanvas:Canvas,setTimeout,clearTimeout,
  createImageBitmap(){if(decode){const held=decode;decode=undefined;return held.promise;}return Promise.resolve(bitmap());},
  async fetch(route,options){uploads.push({route,options});return {ok:true,json:async()=>({image_sha256:options.headers['X-Image-Sha256'],width:nodes.get('#conversation-image-view').width,height:nodes.get('#conversation-image-view').height})};}});
 vm.runInContext(fs.readFileSync('dashboard/web/visual.js','utf8'),context);
 const ui=context.window.VisualInput.mount({headers:()=>({}),getBinding:()=> 'entry-binding',unauthorized(){throw Error('Unexpected auth failure');}});
 return {ui,nodes,uploads,encodings,bitmap,
  hold(){decode=deferred();return decode;},
  choose(){nodes.get('#conversation-image').files=[{name:'selected.png',size:100}];nodes.get('#conversation-image').value='selected.png';return nodes.get('#conversation-image').handlers.change();},
  remove(){nodes.get('#conversation-image-clear').handlers.click();},
  point(){nodes.get('#conversation-image-view').handlers.click({clientX:25,clientY:30});}};
}
(async()=>{
 const f=boot(),held=f.hold(),opening=f.choose();
 await assert.rejects(f.ui.capture(),/Wait for the selected image/);assert.equal(f.uploads.length,0);assert(!f.nodes.get('#conversation-image-clear').hidden);
 held.resolve(f.bitmap());await opening;
 const whole=await f.ui.capture();assert.equal(whole.point,null);assert.equal(f.uploads.length,1);assert.equal((await f.ui.captureForVoice()).point,null);
 f.point();const selected=await f.ui.capture();assert.equal(selected.point.x,25);assert.equal(selected.point.y,30);assert.equal(f.uploads.length,1);assert.equal(f.nodes.get('#conversation-image-list').children[0].children[0].children[1].textContent,'selected.png');
 const largePhoto=boot({photo:true});await largePhoto.choose();assert(largePhoto.encodings.length>1,'A large first PNG must be fitted automatically, not rejected');assert.deepEqual(largePhoto.encodings[0],[2048,1536]);largePhoto.point();const fitted=await largePhoto.ui.capture();assert.equal(largePhoto.uploads.length,1);assert(largePhoto.uploads[0].options.body.size<=2*1024*1024);assert(fitted.width<2048 && fitted.height<1536);assert(!largePhoto.nodes.get('#conversation-image-state').textContent.includes('too detailed'));
 const large=boot();large.nodes.get('#conversation-image').files=[{size:21*1024*1024}];await large.nodes.get('#conversation-image').handlers.change();assert.match(large.nodes.get('#conversation-image-state').textContent,/20 MiB/);assert.equal(large.encodings.length,0);
 const camera=boot();camera.nodes.get('#conversation-camera').files=[{name:'camera.jpg',size:100}];await camera.nodes.get('#conversation-camera').handlers.change();const photo=await camera.ui.captureForVoice();assert(photo.image_sha256);assert.equal(photo.point,null);assert.equal(camera.uploads.length,1);const samePhoto=await camera.ui.capture();assert.equal(samePhoto.source_id,photo.source_id);assert.equal(samePhoto.image_sha256,photo.image_sha256);assert.equal(camera.uploads.length,1,'Voice and text share the same selected image upload');camera.ui.consumed(photo);assert.equal(camera.nodes.get('#conversation-camera').value,'');assert.equal(await camera.ui.captureForVoice(),undefined,'Consumed image is not silently reused');assert(!fs.readFileSync('dashboard/web/index.html','utf8').includes('conversation-image-with-voice'),'There is no separate image-for-voice checkbox');
 const failed=boot(),bad=failed.hold(),failure=failed.choose();bad.reject(Error('decode failed'));await failure;
 await assert.rejects(failed.ui.capture(),/selected image is unavailable/);assert(!failed.nodes.get('#conversation-image-clear').hidden);assert.equal(failed.uploads.length,0);
 failed.remove();assert.equal(await failed.ui.capture(),undefined,'Only explicit removal makes an unavailable selection empty');
 const removed=boot(),pending=removed.hold(),loading=removed.choose();removed.remove();const stale=removed.bitmap();pending.resolve(stale);await loading;
 assert(stale.closed,'Late decoding must release its stale bitmap');assert.equal(await removed.ui.capture(),undefined);assert.equal(removed.uploads.length,0);
 const multiple=boot();
 multiple.nodes.get('#conversation-image').files=[{name:'first.png',size:100},{name:'second.png',size:100}];
 await multiple.nodes.get('#conversation-image').handlers.change();
 let refs=await multiple.ui.capture();assert.equal(refs.length,2);assert.notEqual(refs[0].source_id,refs[1].source_id);assert.equal(multiple.uploads.length,2);
 const first=refs[0].source_id,second=refs[1].source_id;
 const list=()=>multiple.nodes.get('#conversation-image-list').children;
 list()[1].children[1].children[0].handlers.click();
 refs=await multiple.ui.captureForVoice();assert.equal(refs[0].source_id,second);assert.equal(refs[1].source_id,first);assert.equal(multiple.uploads.length,2,'Reorder reuses confirmed immutable uploads');
 list()[0].children[0].handlers.click();multiple.point();refs=await multiple.ui.capture();assert.equal(refs[0].point.x,25);assert.equal(refs[1].point,null,'Point selection is per image');
 const frozen=refs;
 multiple.nodes.get('#conversation-camera').files=[{name:'later.jpg',size:100}];await multiple.nodes.get('#conversation-camera').handlers.change();
 multiple.ui.consumed(frozen);const remaining=await multiple.ui.capture();assert(!Array.isArray(remaining));assert.notEqual(remaining.source_id,first);assert.notEqual(remaining.source_id,second,'A later selected camera photo survives consumption of frozen recording images');
 const mixed=boot();await mixed.choose();const waiting=mixed.hold();mixed.nodes.get('#conversation-image').files=[{name:'bad.png',size:100}];const badChoice=mixed.nodes.get('#conversation-image').handlers.change();waiting.reject(Error('bad image'));await badChoice;
 await assert.rejects(mixed.ui.capture(),/selected image is unavailable/);assert.equal(mixed.nodes.get('#conversation-image-list').children.length,2,'Failed intended image stays visible');mixed.remove();assert((await mixed.ui.capture()).source_id,'Explicit removal preserves the good image');
 console.log('Visual UI: ordered multi-image text/voice, reorder/removal, later-selection preservation and held/failed decode never drops an intended image, whole image or optional point, upload only on send, filename retained and explicit removal invalidates late decoding; zero models.');
})().catch(error=>{console.error(error);process.exitCode=1;});
