// Shipped fullscreen controller with synthetic callbacks; no capture or models.
const assert=require('node:assert/strict'),fs=require('node:fs'),vm=require('node:vm');
function boot(){
 const timers=[],listeners={},nodes=[];
 class Node{
  constructor(tag){this.tag=tag;this.children=[];this.attributes={};this.events={};this.hidden=false;this.dataset={};this.classList={add(){},remove(){}};nodes.push(this);}
  append(...children){for(const child of children){child.remove();child.parent=this;this.children.push(child);}}
  remove(){if(this.parent)this.parent.children.splice(this.parent.children.indexOf(this),1);this.parent=undefined;}
  before(node){node.remove();const parent=this.parent;parent.children.splice(parent.children.indexOf(this),0,node);node.parent=parent;}
  replaceWith(node){this.before(node);this.remove();}
  setAttribute(key,value){this.attributes[key]=value;}
  getAttribute(key){return this.attributes[key]??null;}removeAttribute(key){delete this.attributes[key];}
  addEventListener(name,fn){this.events[name]=fn;}
  showModal(){this.open=true;}close(){this.open=false;}focus(){this.focused=true;}
 }
 const body=new Node('body'),header=new Node('header'),portrait=new Node('pre');body.append(header);header.append(portrait);
 const document={body,hidden:false,createElement:tag=>new Node(tag),createComment:()=>new Node('comment'),addEventListener:(name,fn)=>listeners[name]=fn};
 const window={addEventListener:(name,fn)=>listeners[name]=fn};
 vm.runInNewContext(fs.readFileSync('dashboard/web/voice-mode.js','utf8'),{window,document,setTimeout:(fn,ms)=>{const timer={fn,ms};timers.push(timer);return timer;},clearTimeout:t=>{if(t)t.cancelled=true;}});
 let starts=0,enters=0,leaves=0,canStart=true,canEnter=true;
 const mode=window.ConversationVoiceMode.mount({portrait,onEnter:()=>{enters++;},onLeave:()=>{leaves++;},onSpeak:()=>starts++,canStart:()=>canStart,canEnter:()=>canEnter});
 return {mode,portrait,header,nodes,timers,document,listeners,counts:()=>({starts,enters,leaves}),canStart:value=>canStart=value,canEnter:value=>canEnter=value};
}
const f=boot();f.mode.configure(false);f.portrait.events.click();assert.equal(f.counts().starts,0);
f.mode.configure(true);f.canEnter(false);f.portrait.events.click();assert.equal(f.counts().starts,0,'An unfinished original prevents opening');
f.canEnter(true);f.portrait.events.click();assert(f.mode.active);assert.equal(f.counts().starts,1);assert.notEqual(f.portrait.parent,f.header);
f.mode.update({capture:'recording',message:'Live words'});assert.equal(f.nodes.find(n=>n.className==='voice-mode-state').textContent,'Listening');
f.mode.update({capture:'idle',pending:false,sent:true});assert.equal(f.counts().starts,1,'Receipt alone does not reopen the microphone over the response');
f.mode.update({playback:'playing'});f.mode.update({playback:undefined});
let timer=f.timers.findLast(t=>t.ms===900&&!t.cancelled);assert(timer);f.canStart(false);timer.fn();assert.equal(f.counts().starts,1,'A queued chunk or capture fences automatic rearming');
f.canStart(true);f.mode.update({playback:'playing'});f.mode.update({playback:undefined});timer=f.timers.findLast(t=>t.ms===900&&!t.cancelled);timer.fn();assert.equal(f.counts().starts,2);
f.mode.update({capture:'ready',pending:true});assert.equal(f.nodes.find(n=>n.className==='voice-mode-state').textContent,'Recording needs review');
f.nodes.find(n=>n.tag==='dialog'&&n.open).events.cancel({preventDefault(){}});
assert(!f.mode.active);assert.equal(f.portrait.parent,f.header);assert.deepEqual(f.counts(),{starts:2,enters:1,leaves:1});
f.portrait.events.keydown({key:'Enter',preventDefault(){}});assert(f.mode.active);
f.document.hidden=true;f.listeners.visibilitychange();assert(!f.mode.active);assert.equal(f.portrait.parent,f.header);
f.document.hidden=false;f.portrait.events.click();f.timers.findLast(t=>t.ms===300000&&!t.cancelled).fn();assert(!f.mode.active,'Foreground opt-in has a five-minute stop path');
f.mode.close();assert.equal(f.counts().leaves,3,'Repeated teardown is inert');
const direct=boot();direct.mode.configure(true,true);direct.portrait.events.click();
assert.equal(direct.counts().starts,1);
direct.mode.update({continuousState:{active:true,starting:false,pending:false,working:false}});
direct.mode.update({playback:'playing'});assert.equal(direct.nodes.find(n=>n.className==='voice-mode-state').textContent,'Speaking · listening');
direct.mode.update({playback:undefined});assert(!direct.timers.some(t=>t.ms===900&&!t.cancelled),'Continuous mode never uses the alternating rearm timer');
direct.mode.update({continuousState:{active:false,starting:false,pending:true,working:false}});direct.mode.update({pending:false,uploading:true,capture:'idle'});
assert.equal(direct.nodes.find(n=>n.className==='voice-mode-state').textContent,'Recording needs review','An old ordinary upload cannot clear continuous recovery state');
direct.mode.configure(true,false);assert(!direct.mode.active);assert.equal(direct.portrait.parent,direct.header);
console.log('Fullscreen voice: avatar identity, explicit opt-in, response/rearm fences, review, Escape, hidden and bounded exit passed.');
