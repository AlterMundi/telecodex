// Pure state mapping and actual shipped WebAudio graph ownership with a fake device.
// This does not certify browser activation, acoustic output or human perception.
const assert=require('node:assert/strict'),fs=require('node:fs'),vm=require('node:vm');
const crypto=require('node:crypto');
const descriptor=JSON.parse(require('node:child_process').execFileSync('python3',['-c','import json; from dashboard.sonic import descriptor; print(json.dumps(descriptor({})))'],{encoding:'utf8'}));
const vocabulary=JSON.parse(descriptor.vocabulary_json);
const timers=new Map();let seq=0;
const timeout=(fn,ms)=>{timers.set(++seq,{fn,ms});return seq;};
const cancel=id=>timers.delete(id);
class Param{constructor(value=0){this.value=value;}cancelScheduledValues(){}setValueAtTime(v){this.value=v;}linearRampToValueAtTime(v){this.value=v;}setTargetAtTime(v){this.value=v;}}
class Node{constructor(kind){this.kind=kind;this.gain=new Param();this.frequency=new Param();this.connections=[];this.stopped=false;}connect(n){this.connections.push(n);}disconnect(){this.connections=[];}start(){this.started=true;}stop(){this.stopped=true;}}
class Audio{
  static instances=[];static delay=false;
  constructor(){this.sampleRate=48000;this.currentTime=0;this.state='suspended';this.nodes=[];this.destination={};Audio.instances.push(this);}
  createGain(){const n=new Node('gain');this.nodes.push(n);return n;}
  createOscillator(){const n=new Node('osc');this.nodes.push(n);return n;}
  async resume(){if(Audio.delay)await new Promise(resolve=>{this.release=resolve;});this.state='running';this.onstatechange?.();}
  async close(){this.state='closed';this.onstatechange?.();}
}
const nodes=new Map(),events=new Map();
const element=()=>({value:'0.3',children:[],handlers:{},textContent:'',setAttribute(k,v){this[k]=v;},replaceChildren(...v){this.children=v;},addEventListener(k,f){this.handlers[k]=f;}});
for(const name of ['toggle','status','score','volume','vocabulary','feedback','controls'])nodes.set('#conversation-sound-'+name,element());
const document={hidden:false,querySelector:s=>nodes.get(s),createElement:element,addEventListener(name,fn){events.set(name,fn);}};
const scope=vm.createContext({window:{AudioContext:Audio,addEventListener(name,fn){events.set(name,fn);}},document,crypto,TextEncoder,performance:{now:()=>0},setTimeout:timeout,clearTimeout:cancel});
vm.runInContext(fs.readFileSync('dashboard/web/sonic.js','utf8'),scope);
const sound=scope.window.ConversationSound;
const source=(n,extra={})=>({id:`100:${n}`,session:n,label:`Session ${n}`,fresh:true,observed_at:1000,execution:'working',goal_state:'active',waiting_for_human:false,tools_running:false,error:false,pending:false,...extra});
const frame=(sources)=>({schema:'telecodex.sonic-state/v1',observed_at:1000,configured:true,connected:true,sources,...descriptor});
(async()=>{
  assert(sound.validate(vocabulary));assert(!sound.validate({...vocabulary,max_sessions:33}));
  assert(!sound.validate({...vocabulary,partials:[[1,.8],[2,.8]]}));
  assert(!sound.validate({...vocabulary,partials:[[1,.5],[1,.5]]}));
  assert(!sound.validate({...vocabulary,texture_depth:.5}));
  assert(!sound.validate({...vocabulary,texture_hz:Infinity}));
  assert.equal(sound.state(source(1,{waiting_for_human:true}),1000000),'waiting');
  assert.equal(sound.state(source(1,{execution:'idle',goal_state:'blocked'}),1000000),'blocked');
  assert.equal(sound.state(source(1,{execution:'idle'}),1000000),'goal_idle');
  assert.equal(sound.state(source(1,{execution:'idle',goal_state:'paused'}),1000000),'passive');
  for(const extra of [{fresh:false},{waiting_for_human:null},{goal_state:'unknown'},{observed_at:1030}])assert.equal(sound.state(source(1,extra),1000000),'unknown');
  assert.equal(sound.state(source(1),1000000,21000),'unknown');
  const many=frame(Array.from({length:20},(_,n)=>source(n+1)));
  assert.equal(sound.plan(many,vocabulary).voices.length,20);
  const bounded=sound.plan(many,{...vocabulary,max_sessions:8});assert.equal(bounded.voices.length,8);assert.equal(bounded.omitted,12);
  const priority=sound.plan(frame([...Array.from({length:24},(_,n)=>source(n+1,{execution:'idle',goal_state:'absent'})),source(99,{execution:'idle',goal_state:'absent',pending:true})]),vocabulary);
  assert.equal(priority.voices[0].id,'100:99','A pending idle session was omitted behind passive voices');
  const unknown=sound.plan({...many,connected:false},vocabulary);assert.equal(unknown.unconfirmed,20);assert.equal(unknown.voices.length,0,'Unknown evidence must not create a pitch');
  const layered=source(1,{tools_running:true,pending:true,error:true});
  assert.deepEqual(Array.from(sound.components(layered,1000000)),['idle','active','tools','goal','pending','blocked']);
  assert.deepEqual(Array.from(sound.components(source(1,{execution:'idle',goal_state:'absent'}),1000000)),['idle']);
  const engine=sound.renderer({Context:Audio});await engine.start();const context=Audio.instances.at(-1);
  const audible=sound.plan(frame([source(1,{waiting_for_human:true,pending:true,tools_running:true}),source(2,{execution:'idle',goal_state:'blocked'})]),vocabulary);
  engine.render(audible,vocabulary);
  const pitched=context.nodes.filter(n=>n.kind==='osc' && n.frequency.value>=20);
  assert.deepEqual(pitched.map(n=>n.frequency.value).sort((a,b)=>a-b),[1,2,3,4,7,11].map(n=>40.4*n));
  assert(pitched.every(n=>n.type==='sine'));
  const pulses=context.nodes.filter(n=>n.kind==='osc' && n.frequency.value>0 && n.frequency.value<20).map(n=>n.frequency.value).sort((a,b)=>a-b),expectedPulses=[1,2,3,4,7,11].map(n=>n*.25);
  assert.equal(pulses.length,expectedPulses.length);pulses.forEach((hz,n)=>assert(Math.abs(hz-expectedPulses[n])<1e-9,'Each harmonic must have its proportional four-second-base pulse'));
  assert(context.nodes.length<40,'Identical frequencies must share carriers, not create a large per-session graph');
  const count=context.nodes.length;engine.render(audible,vocabulary);assert.equal(context.nodes.length,count,'Metadata refresh restarted unchanged carriers');
  context.currentTime=.2;
  const modified=sound.plan(frame([source(1,{tools_running:true}),source(2,{execution:'idle',goal_state:'absent'})]),vocabulary);
  engine.render(modified,vocabulary);
  for(const carrier of pitched.filter(n=>[40.4,80.8,121.2,161.6].includes(n.frequency.value)))assert(!carrier.stopped,'Unchanged frequencies must stay phase-continuous');
  const master=context.nodes[0];engine.setQuiet(true);assert.equal(master.gain.value,0);engine.render(modified,vocabulary);assert.equal(master.gain.value,0);
  engine.setVolume(0);engine.setQuiet(false);assert.equal(master.gain.value,0);
  engine.setVolume(1);assert.equal(master.gain.value,vocabulary.peak_gain);
  assert(vocabulary.peak_gain>=.5,'Maximum volume must have useful headroom, rather than the old .04 ceiling');
  const alternate={...vocabulary,id:'another-being',label:'Another sine vocabulary',author:'Independent author',fundamental_hz:50};
  engine.render(audible,alternate);assert(context.nodes.some(n=>n.kind==='osc' && n.frequency.value===350));
  engine.stop();for(const [id,t] of [...timers])if(t.ms===150){timers.delete(id);t.fn();}assert.equal(context.state,'closed');assert(context.nodes.filter(n=>n.kind==='osc').every(n=>n.stopped));assert.equal(timers.size,0);
  // A late resume belongs to its old context and cannot close the newer one.
  Audio.delay=true;const delayed=sound.renderer({Context:Audio});const oldStart=delayed.start(),old=Audio.instances.at(-1);delayed.stop();
  Audio.delay=false;await delayed.start();const newer=Audio.instances.at(-1);old.release();assert.equal(await oldStart,false);assert.equal(newer.state,'running');delayed.stop();for(const [id,t] of [...timers])if(t.ms===150){timers.delete(id);t.fn();}
  let now=0,calls=0,selected=frame([source(7)]);
  const client=sound.mount({api:async route=>{assert.equal(route,'interaction/sonic');calls++;return selected;},now:()=>now});
  assert.equal(calls,0,'Mount requested state before explicit listening');
  await nodes.get('#conversation-sound-toggle').handlers.click();assert.equal(calls,1);
  assert.match(nodes.get('#conversation-sound-feedback').textContent,/Beacon · 1 session voice/);
  assert.equal(nodes.get('#conversation-sound-feedback').hidden,false);
  const live=Audio.instances.at(-1),liveMaster=live.nodes[0];assert(liveMaster.gain.value>0);
  client.update({capture:'recording'});assert.equal(liveMaster.gain.value,0);
  assert.match(nodes.get('#conversation-sound-feedback').textContent,/quiet while/);
  client.update({capture:'idle'});assert(liveMaster.gain.value>0);
  client.update({playback:'playing'});assert.equal(liveMaster.gain.value,0);
  client.update({playback:undefined});assert(liveMaster.gain.value>0);
  now=21000;const expiry=[...timers.entries()].find(([,t])=>t.ms===20001);assert(expiry);timers.delete(expiry[0]);expiry[1].fn();assert.match(nodes.get('#conversation-sound-status').textContent,/1 unconfirmed/);
  assert(!live.nodes.some(n=>n.frequency.value===845),'Stale evidence invented a pitch');
  const retirement=[...timers.entries()].find(([,t])=>t.ms===vocabulary.release_ms+20);assert(retirement);for(const [id,t] of [...timers])if(t.ms===vocabulary.release_ms+20){timers.delete(id);t.fn();}
  assert(!live.nodes.filter(n=>!n.stopped).some(n=>n.kind==='osc' && n.frequency.value>=20),'Stale sources retained notes after release');
  document.hidden=true;events.get('visibilitychange')();for(const [id,t] of [...timers])if(t.ms===150){timers.delete(id);t.fn();}assert.equal(live.state,'closed');assert.equal(nodes.get('#conversation-sound-status').textContent,'Off');assert.equal(timers.size,0);assert.equal(nodes.get('#conversation-sound-feedback').hidden,true);
  await client.refresh();assert.equal(calls,1,'Hidden/reset client restarted metadata or playback');
  document.hidden=false;const beforeMetadata=Audio.instances.length;let observedMetadata=0;
  const metadataView=sound.mount({api:async()=>frame([source(1)]),onState:()=>observedMetadata++});
  await metadataView.refresh({observeOnly:true});assert.equal(observedMetadata,1);assert.equal(Audio.instances.length,beforeMetadata,'Avatar metadata must not open an audio device');
  console.log('Sonic mapping/graph: 40.4Hz additive sine components, 20-session bounded mix, unknown/stale silence, independent vocabulary, no refresh replay, mute/capture/speech ownership, late resume and hidden teardown passed; simulated audio device, zero models.');
})().catch(error=>{console.error(error);process.exitCode=1;});

