const assert=require('node:assert/strict'),fs=require('node:fs'),vm=require('node:vm');
(async()=>{
 let options,resolveRead,reads=[],refreshes=0,expired=0;const retries=[];
 const reader={read:()=>new Promise(resolve=>resolveRead=resolve),cancel:async()=>{}};
 const context={window:{addEventListener(){}},AbortController,TextDecoder,fetch:async(route,value)=>{assert.equal(route,'api/interaction/events');options=value;return {ok:true,status:200,body:{getReader:()=>reader}};},setTimeout:(f,ms)=>{retries.push({f,ms});return retries.length;},clearTimeout(){}};
 vm.runInNewContext(fs.readFileSync('dashboard/web/live.js','utf8'),context);
 const live=context.window.InteractionLive.mount({headers:()=>({Authorization:'Bearer fixture'}),refresh:async()=>{refreshes++;},expired:()=>{expired++;}});
 const settle=()=>new Promise(resolve=>setImmediate(resolve));
 live.start();await settle();assert.equal(options.credentials,'same-origin');assert.equal(options.headers.Authorization,'Bearer fixture');
 const send=async text=>{resolveRead({value:new TextEncoder().encode(text),done:false});await settle();};
 await send(': keepalive\n\n');assert.equal(refreshes,0);
 await send('da');await send('ta: {}\n\n');assert.equal(refreshes,1);
 await send('data: {}\n\ndata: {}\n\n');assert(refreshes>=2);
 await send('event: expired\ndata: {}\n\n');assert.equal(expired,1);assert(options.signal.aborted);assert.equal(retries.length,0);
 console.log('Live conversation: authenticated push, fragmented events, coalescing and expiry passed');
})().catch(error=>{console.error(error);process.exitCode=1;});
