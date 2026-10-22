// Finite bootstrap checks for asynchronous health reads in initially hidden WebViews.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
const source=fs.readFileSync('dashboard/web/health-live.js','utf8');
const settle=()=>new Promise(resolve=>setImmediate(resolve));
function boot(replies){
 const nodes=new Map(),timers=new Map(),events=new Map(),reports=[];let clock=0,sequence=0,calls=0;
 function element(){return {textContent:'',hidden:false,disabled:false,children:[],handlers:{},cloneNode(){return element()},replaceWith(value){nodes.set('open',value)},remove(){this.removed=true},replaceChildren(...values){this.children=values},append(...values){this.children.push(...values)},addEventListener(name,handler){this.handlers[name]=handler},querySelector(name){return node('notice:'+name)}}}
 function node(name){if(!nodes.has(name))nodes.set(name,element());return nodes.get(name)}
 node('report-date').parentElement=node('notice');
 const document={hidden:true,body:element(),getElementById:node,querySelector:node,createElement:element,addEventListener(name,handler){events.set(name,handler)},dispatchEvent(event){reports.push(event.detail)}};
 const context=vm.createContext({document,window:{Telegram:{WebApp:{initData:'synthetic',ready(){},onEvent(){}}},addEventListener(){}},Date:{now:()=>clock},getComputedStyle:()=>({backgroundColor:'#142a23'}),AbortController,CustomEvent:class{constructor(name,options){this.type=name;this.detail=options.detail}},setTimeout(fn,delay){const id=++sequence;timers.set(id,{fn,delay});return id},clearTimeout(id){timers.delete(id)},async fetch(url){const value=url.endsWith('/session')?{session_token:'x'.repeat(43)}:replies[Math.min(calls++,replies.length-1)];return {ok:true,json:async()=>value}}});
 vm.runInContext(source,context);
 return {nodes,timers,reports,get calls(){return calls},advance(ms){clock+=ms},async poll(){const match=[...timers].find(([,entry])=>entry.delay===2000);assert.ok(match,'An admitted first collection must reconcile even if the WebView initially reports hidden');timers.delete(match[0]);await match[1].fn();await settle()}};
}
module.exports=async()=>{
 const report={schema:'computer-health/v1',mode:'on-demand',generated_at:'2026-01-01T00:00:00Z',targets:[]};
 for(const first of [null,{...report,generated_at:'2025-01-01T00:00:00Z'}]){
  const fixture=boot([{updating:true,report:first},{updating:false,report}]);await settle();assert.equal(fixture.calls,1);await fixture.poll();assert.equal(fixture.calls,2);assert.equal(fixture.reports.at(-1).generated_at,report.generated_at);assert.equal(fixture.timers.size,0,'Completed hidden launch must not install periodic polling');assert.equal(fixture.nodes.get('file').removed,true);
 }
 const bounded=boot([{updating:true,report:null}]);await settle();bounded.advance(61000);await bounded.poll();assert.equal(bounded.calls,1,'Initial hidden reconciliation has a finite deadline');assert.equal(bounded.timers.size,0);
 console.log('Pulso UI: cold/cached hidden launch reconciles once; hidden polling stays bounded');
};
if(require.main===module)module.exports().catch(error=>{console.error(error);process.exitCode=1});
