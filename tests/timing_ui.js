// Correlated timing must not attribute a worker/history/steered answer to one recording.
const assert=require('node:assert/strict'),fs=require('node:fs'),vm=require('node:vm');
let clock=0;
class Element{
 constructor(){this.children=[];this.handlers={};this.textContent='';}
 append(...children){this.children.push(...children);}
 replaceChildren(...children){this.children=children;}
 addEventListener(name,callback){this.handlers[name]=callback;}
 text(){return this.textContent+this.children.map(child=>child.text()).join('\n');}
}
const nodes=new Map(),document={createElement:()=>new Element(),querySelector:name=>{if(!nodes.has(name))nodes.set(name,new Element());return nodes.get(name);}};
const context=vm.createContext({window:{},document,performance:{now:()=>clock}});vm.runInContext(fs.readFileSync('dashboard/web/timing.js','utf8'),context);
const timing=context.window.ConversationTiming.mount(),list=nodes.get('#conversation-timing-records'),text=()=>list.text();
timing.bind('one');timing.mark('clip','requested');clock=100;timing.mark('clip','started');clock=600;timing.mark('clip','partial');clock=1200;timing.mark('clip','send');timing.mark('clip','ended');clock=1300;timing.mark('clip','retained');
timing.input({id:'clip',local_turn:7,phase:'queued'});
clock=1500;timing.source({id:'native:worker',kind:'worker_result',local_turn:7});
assert(text().includes('First caption: 0.50 s'));assert(text().includes('Stop → saved: 0.10 s'));assert(text().includes('Send → same-turn text: not observed'));
// Metadata can precede accepted-input observation, retaining its original browser time.
clock=1800;timing.source({id:'draft',kind:'integrator_live',native_turn:'physical',native_item:'item',local_turn:7});
clock=1900;timing.input({id:'clip',local_turn:7,phase:'accepted'});
assert(text().includes('Send → same-turn text: 0.60 s'));
clock=2400;timing.source({kind:'integrator_live',native_turn:'physical',native_item:'item'},true);
assert(text().includes('Send → playback: 1.20 s'));
timing.mark('clip','prepared',{finish_to_prepared_ms:4567,finish_authorization_ms:31,provider_final_ms:4200,final_authorization_ms:39,receipt_write_ms:297,unexpected:'private data'});
assert(text().includes('Server finish → prepared: 4.57 s'));assert(text().includes('Provider final: 4.20 s'));assert(text().includes('Receipt write: 0.30 s'));assert(!text().includes('private data'));
timing.mark('clip','prepared',{finish_to_prepared_ms:999});assert(text().includes('Server finish → prepared: 4.57 s'),'Later duplicate events cannot replace the exact first preparation');
clock=2500;timing.mark('clip','send');timing.mark('clip','partial');assert(text().includes('First caption: 0.50 s'),'Repeated callbacks/retries cannot reset the first mark');
clock=2600;timing.mark('correction','requested');timing.mark('correction','send');timing.input({id:'correction',local_turn:7,phase:'accepted'});
assert.equal(text().split('Shared native turn; response timing unassigned.').length-1,2,'A steered shared turn cannot masquerade as exact correction-response latency');
clock=2700;timing.pauseApplied(2698);assert(nodes.get('#conversation-timing-pause').text().includes('2.0 ms'));
nodes.get('#conversation-timing-clear').handlers.click();assert(text().includes('No recordings measured'));assert.equal(nodes.get('#conversation-timing-pause').text(),'');
clock=3000;timing.source({kind:'integrator_answer',id:'history',local_turn:8});clock=4000;timing.mark('later','requested');timing.mark('later','send');timing.input({id:'later',local_turn:8,phase:'accepted'});
assert(text().includes('Send → same-turn text: not observed'),'Earlier same-turn history is not a response to a later input');
for(let n=0;n<12;n++){timing.mark('bounded-'+n,'requested');timing.mark('bounded-'+n,'send');timing.input({id:'bounded-'+n,local_turn:100+n,phase:'accepted'});timing.source({kind:'integrator_answer',id:'new-'+n,local_turn:100+n});}
timing.mark('bounded-11','prepared',{finish_to_prepared_ms:-1,provider_final_ms:NaN,receipt_write_ms:140001,unexpected:2});
assert(!text().includes('Server finish → prepared:'),'Invalid durations and evicted recordings add no timing');
timing.mark('bounded-11','prepared',{provider_final_ms:12});assert(text().includes('Server finish → prepared: not observed'),'An incomplete measurement never invents a total');
assert.equal(list.children.length,8,'Only eight recording histories retained');timing.bind('two');assert(text().includes('No recordings measured'),'Conversation change clears earlier identities/marks');
assert(!fs.readFileSync('dashboard/web/timing.js','utf8').match(/localStorage|fetch\(|setInterval|setTimeout/),'The timing reader installs no persistence, requests or poller');
console.log('Browser timing: exact local-turn correlation, ambiguity/history exclusion, first marks, bounded retention and reset passed.');
