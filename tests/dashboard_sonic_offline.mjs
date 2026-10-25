// Actual Chromium OfflineAudioContext: no physical device, provider or PCM archive.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import {spawn,spawnSync,execFileSync} from 'node:child_process';
const pause=ms=>new Promise(resolve=>setTimeout(resolve,ms));
const binary=process.env.CHROME_BINARY || ['google-chrome','chromium','chromium-browser'].find(name=>spawnSync('which',[name]).status===0);
assert(binary,'Chromium is required for the waveform regression');
assert.equal(typeof WebSocket,'function','Node22 or newer is required');
const root=fs.mkdtempSync(path.join(os.tmpdir(),'telecodex-offline-audio-'));
const log=fs.openSync(path.join(root,'browser.log'),'w');
const browser=spawn(binary,['--headless=new','--no-sandbox','--disable-gpu','--mute-audio','--remote-debugging-port=0','--user-data-dir='+root,'about:blank'],{stdio:['ignore','ignore',log]});
let socket;
try{
 let port;
 for(let n=0;n<300;n++){
  if(browser.exitCode!==null)throw Error('Owned test browser exited: '+fs.readFileSync(path.join(root,'browser.log'),'utf8'));
  try{port=Number(fs.readFileSync(path.join(root,'DevToolsActivePort'),'utf8').split('\n')[0]);}catch{}
  if(port)break;await pause(100);
 }
 assert(port,'Owned test browser did not become ready');
 const pages=await (await fetch(`http://127.0.0.1:${port}/json/list`)).json();
 const page=pages.find(p=>p.type==='page');assert(page);
 socket=new WebSocket(page.webSocketDebuggerUrl);
 await new Promise((resolve,reject)=>{socket.addEventListener('open',resolve,{once:true});socket.addEventListener('error',reject,{once:true});});
 let sequence=0;const pending=new Map();
 socket.addEventListener('message',event=>{
  const value=JSON.parse(event.data);const request=pending.get(value.id);if(!request)return;
  pending.delete(value.id);clearTimeout(request.timer);value.error?request.reject(Error(JSON.stringify(value.error))):request.resolve(value.result);
 });
 const call=(method,params={})=>new Promise((resolve,reject)=>{
  const id=++sequence,timer=setTimeout(()=>{pending.delete(id);reject(Error('CDP deadline: '+method));},45000);
  pending.set(id,{resolve,reject,timer});socket.send(JSON.stringify({id,method,params}));
 });
 await call('Runtime.evaluate',{expression:fs.readFileSync('dashboard/web/sonic.js','utf8')});
 const descriptor=JSON.parse(execFileSync('python3',['-c','import json; from dashboard.sonic import descriptor; print(json.dumps(descriptor({})))'],{encoding:'utf8'}));
 const expression=fs.readFileSync('tests/dashboard_sonic_waveform_fixture.js','utf8').replace('__VOCAB__',descriptor.vocabulary_json);
 const result=await call('Runtime.evaluate',{expression,awaitPromise:true,returnByValue:true});
 assert(!result.exceptionDetails,JSON.stringify(result.exceptionDetails));assert(result.result.value?.offlineAudio);
 console.log(JSON.stringify(result.result.value));
}finally{
 socket?.close();browser.kill('SIGTERM');
 if(browser.exitCode===null)await Promise.race([new Promise(resolve=>browser.once('exit',resolve)),pause(3000)]);
 if(browser.exitCode===null){browser.kill('SIGKILL');await new Promise(resolve=>browser.once('exit',resolve));}
 fs.closeSync(log);fs.rmSync(root,{recursive:true,force:true});
}
