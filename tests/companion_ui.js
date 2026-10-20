// Exercise finite, evidence-driven presentation without native sessions/models.
const assert=require("node:assert/strict"),fs=require("node:fs"),vm=require("node:vm");
function boot(){
  let clock=100,animations=0,cancellations=0,next=0;
  const timers=new Map(),handlers={},motion={matches:false,addEventListener(_name,fn){this.change=fn;}};
  const node=()=>({dataset:{},children:[],textContent:"",setAttribute(){},append(...v){this.children.push(...v);},replaceChildren(...v){this.children=v;}});
  const box=node(),face=node(),caption=node(),phases=node();
  face.animate=()=>{animations++;return {cancel(){cancellations++;}};};
  box.querySelector=s=>s==="pre"?face:s===".companion-caption"?caption:phases;
  const document={hidden:false,querySelector:()=>box,createElement:node,createTextNode:text=>({textContent:text}),addEventListener:(name,fn)=>handlers[name]=fn};
  const context=vm.createContext({window:{matchMedia:()=>motion},document,Date,Number,Object,
    setTimeout(fn,ms){const id=++next;timers.set(id,{fn,at:clock+ms});return id;},clearTimeout:id=>timers.delete(id)});
  vm.runInContext(fs.readFileSync("dashboard/web/companion.js","utf8"),context);
  const companion=context.window.ConversationCompanion.mount({now:()=>clock});
  return {companion,box,face,caption,motion,timers,document,handlers,
    phaseNodes:()=>phases.children,
    labels:()=>phases.children.map(chip=>chip.children.at(-1).textContent),
    animations:()=>animations,cancellations:()=>cancellations,
    elapse(ms){clock+=ms;for(const [id,t]of timers)if(t.at<=clock){timers.delete(id);t.fn();}}};
}
const f=boot();
assert.deepEqual(f.labels(),["Connect to begin"]);
f.companion.observe({schema:"telecodex.interaction-activity/v1",integrator_busy:true,sources:[
  {known:true,busy:true,observer_synthesizing:1},{known:true,busy:true,observer_synthesizing:0},{known:false,busy:true,observer_synthesizing:99}]});
const firstChips=f.phaseNodes();
f.companion.observe({schema:"telecodex.interaction-activity/v1",integrator_busy:true,sources:[{known:true,busy:true,observer_synthesizing:1},{known:true,busy:true,observer_synthesizing:0},{known:false,busy:true,observer_synthesizing:99}]});
assert.equal(f.phaseNodes(),firstChips,"unchanged polling must not rebuild the screen reader live region");
f.companion.update({capture:"recording",transcribing:true,routing:true,question:true,playback:"playing"});
assert.deepEqual(f.labels(),["Listening","Transcribing","Conversation active","Observer · 1","2 sessions active","Some session activity unconfirmed","Routing pending","A question for you","Speaking"]);
assert.equal(f.box.dataset.pose,"listening");
f.elapse(20000);
assert(!f.labels().some(x=>x.includes("sessions active")),"stale receiver work cannot keep claiming activity");
assert(f.labels().includes("Session activity unconfirmed"));
assert(!f.labels().includes("Transcribing"),"a retained preparing phase is no longer proof of live transcription once the receiver is stale");
assert(f.labels().includes("Listening"),"local capture does not depend on receiver freshness");
f.companion.reset();
f.companion.observe({schema:"telecodex.interaction-activity/v1",sources:[]});
assert.equal(f.box.dataset.pose,"calm");
const answer={id:"native:1",revision:"rev-1",kind:"integrator_answer",observed_at:"2026-10-20T00:00:00Z",companion_hint:{rest:"delighted"}};
f.companion.reply(answer);
assert.equal(f.box.dataset.pose,"delighted");assert.equal(f.animations(),1);
f.companion.reply(answer);assert.equal(f.animations(),1,"polling the same response cannot replay a gesture");
f.companion.reply({...answer,id:"native:2",observed_at:"2026-10-19T00:00:00Z",companion_hint:{rest:"uncertain"}});
assert.equal(f.box.dataset.pose,"delighted","an earlier response cannot overwrite the newer pose");
f.companion.reply({...answer,id:"native:3",kind:"worker_progress",observed_at:"2026-10-21T00:00:00Z"});
assert.equal(f.animations(),1,"progress is operational evidence, not another reply gesture");
f.motion.matches=true;f.motion.change();
f.companion.reply({...answer,id:"native:4",observed_at:"2026-10-22T00:00:00Z",companion_hint:{rest:"<script>"}});
assert.equal(f.box.dataset.pose,"calm");assert.equal(f.animations(),1,"reduced motion retains static expression without animation");
f.document.hidden=true;f.handlers.visibilitychange();assert.equal(f.timers.size,0);
f.elapse(20001);f.document.hidden=false;f.handlers.visibilitychange();
assert(f.labels().includes("Session activity unconfirmed"),"returning to a hidden view does not invent a fresh observation");
f.companion.observe(undefined);assert(f.labels().includes("Session activity unconfirmed"),"old backends without activity are unknown, not idle");
f.companion.reset();assert.equal(f.timers.size,0);assert.equal(f.box.dataset.pose,"uncertain");
console.log("Companion UI: concurrent evidence, stale/absent backend state, local capture, bounded reply poses, duplicate/old hints, hidden view and reduced motion passed; zero models.");
