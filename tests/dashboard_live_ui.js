const assert=require('node:assert/strict'),fs=require('node:fs'),vm=require('node:vm');
function boot(){
  const handlers=new Map(),timers=new Map(),connections=[];let refreshes=0,expired=0;
  const window={addEventListener(name,fn){handlers.set(name,fn);}};
  const fetch=async(route,options)=>{
    assert.equal(route,'api/interaction/events');let resolveRead,rejectRead;
    const connection={options,send(text){resolveRead({value:new TextEncoder().encode(text),done:false});},end(){resolveRead({done:true});}};
    const reader={read:()=>new Promise((resolve,reject)=>{resolveRead=resolve;rejectRead=reject;}),cancel:async()=>{resolveRead?.({done:true});}};
    options.signal.addEventListener('abort',()=>rejectRead?.(new Error('Connection aborted')));
    connections.push(connection);return {ok:true,status:200,body:{getReader:()=>reader}};
  };
  const context={window,AbortController,TextDecoder,fetch,setTimeout:(fn,ms)=>{const id={};timers.set(id,{fn,ms});return id;},clearTimeout:id=>timers.delete(id)};
  vm.runInNewContext(fs.readFileSync('dashboard/web/live.js','utf8'),context);
  const live=window.InteractionLive.mount({headers:()=>({Authorization:'Bearer fixture'}),refresh:async()=>{refreshes++;},expired:()=>{expired++;}});
  const settle=()=>new Promise(resolve=>setImmediate(resolve));
  return {live,mount:args=>window.InteractionLive.mount(args),connections,timers,settle,event:(name,value={})=>handlers.get(name)?.(value),refreshes:()=>refreshes,expired:()=>expired,
    async send(text){connections.at(-1).send(text);await settle();},
    async fire(ms){const selected=[...timers].filter(([,t])=>t.ms===ms);for(const [id,t]of selected){timers.delete(id);t.fn();}await settle();}};
}
(async()=>{
  const f=boot();f.live.start();await f.settle();const first=f.connections[0];
  assert.equal(first.options.credentials,'same-origin');assert.equal(first.options.headers.Authorization,'Bearer fixture');
  await f.send(': keepalive\n\n');assert.equal(f.refreshes(),0);
  await f.send('da');await f.send('ta: {}\n\n');assert.equal(f.refreshes(),1);
  await f.send('data: {}\n\ndata: {}\n\n');assert(f.refreshes()>=2);
  await f.send('event: expired\ndata: {}\n\n');assert.equal(f.expired(),1);assert(first.options.signal.aborted);assert.equal(f.timers.size,0);

  const resume=boot();resume.live.start();await resume.settle();
  const old=resume.connections[0];resume.event('pagehide',{persisted:false});await resume.settle();assert(old.options.signal.aborted);
  resume.event('pageshow',{persisted:false});await resume.settle();
  assert.equal(resume.connections.length,2,'A non-BFCache return must restore notifications after pagehide aborted them');
  await resume.send('data: {}\n\n');assert(resume.refreshes()>0,'A later reply refreshes without another visibility transition');
  resume.event('online');await resume.settle();assert.equal(resume.connections.length,3,'Network recovery replaces the old stalled subscription once');
  resume.live.stop();await resume.settle();resume.event('pageshow',{persisted:true});resume.event('online');await resume.settle();
  assert.equal(resume.connections.length,3,'Explicit stop cannot reconnect from lifecycle events');assert.equal(resume.timers.size,0);

  const stalled=boot();stalled.live.start();await stalled.settle();await stalled.fire(45000);
  assert(stalled.connections[0].options.signal.aborted,'A stream missing three server heartbeats must not wait forever');
  await stalled.fire(1000);assert.equal(stalled.connections.length,2);
  assert(stalled.timers.size<=1,'Only the selected connection owns a heartbeat deadline');
  stalled.live.stop();await stalled.settle();assert.equal(stalled.timers.size,0);
  const stale=boot(),pending=[];let staleExpiry=0;
  const selected=stale.mount({headers:()=>({}),refresh:async()=>{},expired:()=>staleExpiry++,fetchStream:()=>new Promise(resolve=>pending.push(resolve))});
  selected.start();await stale.settle();stale.event('online');await stale.settle();assert.equal(pending.length,2);
  pending[0]({status:401});await stale.settle();assert.equal(staleExpiry,0,'An obsolete connection cannot expire current access');
  selected.stop();pending[1]({status:403});await stale.settle();assert.equal(staleExpiry,0);assert.equal(stale.timers.size,0);
  console.log('Live conversation: authenticated push, fragmented events, expiry, lifecycle/network recovery and bounded heartbeat reconnect passed; zero work submissions.');
})().catch(error=>{console.error(error);process.exitCode=1;});
