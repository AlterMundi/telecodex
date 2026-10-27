const assert=require('node:assert/strict'),fs=require('node:fs'),vm=require('node:vm'),crypto=require('node:crypto').webcrypto;
class Element{
 constructor(){this.handlers={};this.children=[];this.value='';}
 addEventListener(k,f){this.handlers[k]=f;}replaceChildren(...xs){this.children=xs;}
 append(...xs){this.children.push(...xs);}setAttribute(k,v){this[k]=v;}click(){}remove(){}
}
function boot(){
 const nodes=new Map(),uploads=[],downloads=[],saved=[],access=[],retained=new Map();
 const element=k=>nodes.get(k)||nodes.set(k,new Element()).get(k);
 const context={window:{},document:{querySelector:element,createElement:()=>new Element(),body:new Element()},crypto,Blob,AbortController,setTimeout:()=>1,clearTimeout(){},URL:{createObjectURL(blob){saved.push(blob);return 'blob:fixture';},revokeObjectURL(){}},fetch:async(url,options)=>{
  if(options.method==='POST'){
   uploads.push({url,options});const ref=JSON.parse(decodeURIComponent(options.headers['X-File-Reference']));retained.set(ref.sha256,options.body);
   return{ok:true,status:200,json:async()=>({...ref,media:'retained',work_admitted:false})};
  }
  downloads.push({url,options});return{ok:true,status:200,blob:async()=>new Blob([retained.values().next().value])};
 }};
 vm.runInNewContext(fs.readFileSync('dashboard/web/files.js','utf8'),context);
 return{nodes,uploads,downloads,saved,access,context,ui:context.window.FileInput.mount({headers:()=>({'Authorization':'Companion fixture','X-Dashboard-Csrf':'fixture'}),getBinding:()=> 'binding',unauthorized(error){access.push(error.status);}}),
 async choose(names){element('#conversation-files').files=names.map(([name,data])=>({name,size:Buffer.byteLength(data),type:name.endsWith('.pdf')?'application/pdf':'application/octet-stream',arrayBuffer:async()=>new TextEncoder().encode(data).buffer}));await element('#conversation-files').handlers.change();}};
}
(async()=>{
 const f=boot();await f.choose([['croquis.pdf','%PDF-1.4\nfixture'],['datos.bin','\0\u0001']]);
 assert(f.ui.hasFiles());assert.equal(f.uploads.length,0);const refs=await f.ui.capture();
 assert.equal(refs.length,2);assert.equal(refs[0].name,'croquis.pdf');assert.equal(f.uploads.length,2);
 assert.equal(new TextDecoder().decode(f.uploads[0].options.body),'%PDF-1.4\nfixture');
 await f.ui.capture();assert.equal(f.uploads.length,2,'Confirmed originals are not uploaded again');
 await f.ui.download(refs[0],'11111111-1111-4111-8111-111111111111',0);
 assert.equal(f.downloads[0].options.headers.Authorization,'Companion fixture');assert.equal(f.saved[0].size,refs[0].size);
 f.context.fetch=async()=>({ok:false,status:403});await assert.rejects(f.ui.download(refs[0],'fixture',0));assert.deepEqual(f.access,[403]);
 await f.choose([['later.txt','later']]);f.ui.consumed(refs);
 // Restore upload response for the later selection.
 f.context.fetch=async(url,options)=>({ok:true,status:200,json:async()=>({...JSON.parse(decodeURIComponent(options.headers['X-File-Reference'])),media:'retained',work_admitted:false})});
 const rest=await f.ui.capture();assert.equal(rest.length,1);assert.equal(rest[0].name,'later.txt');
 f.nodes.get('#conversation-files-list').children[0].children[1].handlers.click();assert.equal((await f.ui.capture()).length,0);
 f.ui.consumed([]);assert.equal(f.nodes.get('#conversation-visual').open,false);
 f.ui.reset();assert(!f.ui.hasFiles());
 const race=boot();await race.choose([['existing.txt','existing']]);let release;
 race.nodes.get('#conversation-files').files=[{name:'pending.txt',size:1,type:'text/plain',arrayBuffer:()=>new Promise(resolve=>{release=()=>resolve(new Uint8Array([1]).buffer);})}];
 const preparing=race.nodes.get('#conversation-files').handlers.change();
 race.nodes.get('#conversation-files-list').children[0].children[1].handlers.click();release();await preparing;
 assert.equal((await race.ui.capture()).length,0,'Removing a file cancels obsolete preparation without wedging send');
 const denied=boot();await denied.choose([['file.txt','text']]);denied.context.fetch=async()=>({ok:false,status:401});
 await assert.rejects(denied.ui.capture());assert.deepEqual(denied.access,[401]);
 console.log('PASS opaque original bytes/order, authenticated verified downloads, access revocation, preparation/remove race, frozen consumption and retry; zero models');
})().catch(e=>{console.error(e);process.exitCode=1;});
