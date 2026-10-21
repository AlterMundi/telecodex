// Exercise finite, evidence-driven presentation without native sessions/models.
const assert=require("node:assert/strict"),fs=require("node:fs"),vm=require("node:vm");
function boot(){
  let clock=100,animations=0,cancellations=0,next=0,lastFrames;
  const timers=new Map(),handlers={},motion={matches:false,addEventListener(_name,fn){this.change=fn;}};
  const node=()=>({dataset:{},children:[],textContent:"",setAttribute(){},append(...v){this.children.push(...v);},replaceChildren(...v){this.children=v;}});
  const box=node(),face=node(),caption=node(),phases=node();
  face.animate=frames=>{lastFrames=frames;animations++;return {cancel(){cancellations++;}};};
  box.querySelector=s=>s==="pre"?face:s===".companion-caption"?caption:phases;
  const document={hidden:false,querySelector:()=>box,createElement:node,createTextNode:text=>({textContent:text}),addEventListener:(name,fn)=>handlers[name]=fn};
  const context=vm.createContext({window:{matchMedia:()=>motion},document,Date,Number,Object,
    setTimeout(fn,ms){const id=++next;timers.set(id,{fn,at:clock+ms});return id;},clearTimeout:id=>timers.delete(id)});
  vm.runInContext(fs.readFileSync("dashboard/web/companion.js","utf8"),context);
  const companion=context.window.ConversationCompanion.mount({now:()=>clock});
  return {companion,box,face,caption,motion,timers,document,handlers,
    phaseNodes:()=>phases.children,
    labels:()=>phases.children.map(chip=>chip.children.at(-1).textContent),
    animations:()=>animations,cancellations:()=>cancellations,lastFrames:()=>lastFrames,
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
f.companion.reply({...answer,id:"native:nod",observed_at:"2026-10-21T01:00:00Z",companion_hint:{gesture:"nod",rest:"curious"}});
assert.equal(f.animations(),2);assert(f.lastFrames().some(frame=>frame.transform.includes("translateY")));
f.companion.reply({...answer,id:"native:quiet",observed_at:"2026-10-21T02:00:00Z",companion_hint:{gesture:"none",rest:"calm"}});
assert.equal(f.animations(),2,"none cancels the old gesture and creates no animation");
f.motion.matches=true;f.motion.change();
f.companion.reply({...answer,id:"native:4",observed_at:"2026-10-22T00:00:00Z",companion_hint:{rest:"<script>"}});
assert.equal(f.box.dataset.pose,"calm");assert.equal(f.animations(),2,"reduced motion retains static expression without animation");
f.document.hidden=true;f.handlers.visibilitychange();assert.equal(f.timers.size,0);
f.elapse(20001);f.document.hidden=false;f.handlers.visibilitychange();
assert(f.labels().includes("Session activity unconfirmed"),"returning to a hidden view does not invent a fresh observation");
f.companion.observe(undefined);assert(f.labels().includes("Session activity unconfirmed"),"old backends without activity are unknown, not idle");
f.companion.reset();assert.equal(f.timers.size,0);assert.equal(f.box.dataset.pose,"uncertain");
console.log("Companion UI: concurrent evidence, stale/absent backend state, local capture, bounded reply poses, duplicate/old hints, hidden view and reduced motion passed; zero models.");
// Two unrelated per-being vocabularies share rendering, not identity or moods.
const pack=JSON.parse(fs.readFileSync('tests/fixtures/source-intervals.json','utf8'));
const g=boot();g.companion.configure(pack,'a'.repeat(64));
g.companion.observe({schema:'telecodex.interaction-activity/v1',sources:[]});
assert.equal(g.box.dataset.pose,'weave');
assert.equal(g.face.textContent,pack.expressions.weave.variants.field.frames[0]);
const selected={...answer,id:'pack:1',companion_hint:{gesture:'play',catalog:'a'.repeat(64),pack:pack.id,revision:pack.revision,rest:'inquiry'}};
g.companion.reply(selected);
assert.equal(g.box.dataset.pose,'inquiry');
g.elapse(300);
assert.equal(g.face.textContent,pack.expressions.inquiry.variants.field.frames[1]);
g.companion.reply(selected);g.elapse(300);
assert.equal(g.face.textContent,pack.expressions.inquiry.variants.field.frames[0], 'one finite cycle settles; duplicate does not restart');
g.motion.matches=true;g.motion.change();
g.companion.reply({...selected,id:'pack:2',observed_at:'2026-10-23T00:00:00Z'});
g.elapse(300);assert.equal(g.face.textContent,pack.expressions.inquiry.variants.field.frames[0]);
const other=JSON.parse(JSON.stringify(pack));other.id='other-being';other.revision='2';
other.expressions={ripples:{alt:'A field of ripples',variants:{small:{frames:['~ : ~',' :~: '],frame_ms:100}}}};
for(const d of Object.values(other.defaults)){d.expression='ripples';d.variant='small';}
g.companion.configure(other,'b'.repeat(64));assert.equal(g.box.dataset.pose,'ripples');
g.companion.reply({...selected,id:'old-pack',observed_at:'2026-10-24T00:00:00Z'});
assert.equal(g.box.dataset.pose,'ripples','a hint from another catalog cannot select this being’s expression');
const narrow=JSON.parse(JSON.stringify(other));narrow.revision='3';narrow.defaults.app.columns=2;
g.companion.configure(narrow,'c'.repeat(64));assert.equal(g.face.textContent,'A field of ripples','whole composition falls back instead of clipping');
const unsafe=JSON.parse(JSON.stringify(other));unsafe.expressions.ripples.variants.small.frames=['\x1b[2J'];
g.companion.configure(unsafe,'d'.repeat(64));assert.equal(g.box.dataset.pose,'calm','invalid packs cannot install control characters');
console.log('Per-being packs: independent vocabularies, exact selection, finite frames, reduced motion, stale pack and narrow fallback passed.');

const h=boot();h.companion.configure(pack,'a'.repeat(64));
h.companion.reply({...selected,companion_hint:{...selected.companion_hint,gesture:'none'}});
h.elapse(300);assert.equal(h.face.textContent,pack.expressions.inquiry.variants.field.frames[0],'none stays static');
h.companion.reply({...selected,id:'changed-bytes',observed_at:'2026-10-25T00:00:00Z',companion_hint:{...selected.companion_hint,catalog:'b'.repeat(64)}});
assert.equal(h.box.dataset.pose,'weave','reused pack labels with different bytes do not select a new expression');

const busyExpression=boot();
busyExpression.companion.observe({schema:"telecodex.interaction-activity/v1",integrator_busy:true,sources:[{known:true,busy:true}]});
busyExpression.companion.reply({...answer,id:"busy-answer",companion_hint:{rest:"delighted",gesture:"wave"}});
assert.equal(busyExpression.box.dataset.pose,"delighted","parallel work must not suppress the authored reply expression");
assert(busyExpression.labels().includes("1 session active"),"artistic expression must preserve independent work evidence");
const expressionArt=busyExpression.face.textContent;
busyExpression.companion.update({capture:"recording"});
assert.notEqual(busyExpression.face.textContent,expressionArt,"local capture must produce a perceptibly different field");
busyExpression.companion.update({capture:undefined});
assert.equal(busyExpression.box.dataset.pose,"delighted","capture settles back into the authored response expression");
