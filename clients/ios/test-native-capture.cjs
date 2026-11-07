'use strict';
const assert=require('node:assert/strict'),fs=require('node:fs'),vm=require('node:vm'),crypto=require('node:crypto');
const source=fs.readFileSync(__dirname+'/Resources/native-capture.js','utf8');
function fixture({begin,permission}={}){
  const messages=[],listeners=new Map();let requests=0,stops=0;
  const track={readyState:'live',stop(){stops++;this.readyState='ended';},addEventListener(type,fn){this.ended=fn;}};
  const stream={getTracks:()=>[track]},document=crypto.randomUUID();
  const devices={getUserMedia:async()=>{requests++;return permission?permission():stream;}};
  const window={__DaimonAudioDocument:document,webkit:{messageHandlers:{capture:{postMessage:raw=>{
    const value=JSON.parse(raw);messages.push(value);
    return value.command==='capture_begin'&&begin?begin():Promise.resolve(true);
  }}}},addEventListener:(name,fn)=>listeners.set(name,fn)};
  vm.runInNewContext(source,{window,navigator:{mediaDevices:devices},crypto,DOMException,Promise,Object});
  return {devices,window,track,stream,messages,requests:()=>requests,stops:()=>stops,event:name=>listeners.get(name)?.()};
}
(async()=>{
  const f=fixture();const stream=await f.devices.getUserMedia({audio:true});
  assert.equal(f.requests(),1);assert.equal(f.messages.length,1);
  await assert.rejects(f.devices.getUserMedia({audio:true}),{name:'InvalidStateError'});
  stream.getTracks()[0].stop();f.track.stop();
  assert.equal(f.messages.filter(m=>m.command==='capture_end').length,1,'Explicit stop releases once despite missing ended event');
  await f.devices.getUserMedia({audio:true});assert.equal(f.messages.length,4);
  f.track.ended();f.track.ended();assert.equal(f.messages.length,4,'Late ended callback cannot duplicate a release');
  let nativeGrant;const pending=fixture({begin:()=>new Promise(r=>nativeGrant=r)});
  const request=pending.devices.getUserMedia({audio:true});pending.event('pagehide');nativeGrant(true);
  await assert.rejects(request,{name:'AbortError'});assert.equal(pending.requests(),0);
  assert.equal(pending.messages.filter(m=>m.command==='capture_end').length,1);
  let grant,requested;const permissionReady=new Promise(r=>requested=r);
  const late=fixture({permission:()=>{requested();return new Promise(r=>grant=r);}});
  const audio=late.devices.getUserMedia({audio:true}), refused=assert.rejects(audio,{name:'AbortError'});
  await permissionReady;late.window.__DaimonCaptureStop();grant(late.stream);
  await refused;assert.equal(late.stops(),1);
  assert.equal(late.messages.filter(m=>m.command==='capture_end').length,1,'Late OS grant closes tracks without another native mutation');
  const rejected=fixture({permission:()=>Promise.reject(new DOMException('No microphone','NotAllowedError'))});
  await assert.rejects(rejected.devices.getUserMedia({audio:true}),{name:'NotAllowedError'});
  assert.equal(rejected.messages.at(-1).command,'capture_end');
  const inactive=fixture();await assert.rejects(inactive.devices.getUserMedia({audio:true,video:true}),{name:'NotSupportedError'});
  assert.equal(inactive.messages.length,0);assert.equal(inactive.requests(),0);
  const revoked=fixture();await revoked.devices.getUserMedia({audio:true});revoked.event('nativecapturebackground');
  assert.equal(revoked.stops(),1);assert.equal(revoked.messages.length,2);
  console.log('iOS capture ownership: stop/release, concurrency, late native/OS grants, refusal and lifecycle passed; no real microphone/provider');
})().catch(error=>{console.error(error);process.exitCode=1;});
