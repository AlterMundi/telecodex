// Shipped request controls, synthetic DOM/network; no sessions or model calls.
const assert=require('node:assert/strict'), fs=require('node:fs'), vm=require('node:vm');
const clone=value=>JSON.parse(JSON.stringify(value));
const deferred=()=>{let release,enter;return {promise:new Promise(yes=>{release=yes;}),entered:new Promise(yes=>{enter=yes;}),release:()=>release(),enter:()=>enter()};};
function boot(count=1) {
  const nodes=new Map();
  const element=tag=>({tag,children:[],handlers:{},textContent:'',value:'',hidden:false,disabled:false,
    setAttribute(key,value){this[key]=value;},addEventListener(name,fn){this.handlers[name]=fn;},
    append(...values){for(const child of values){child.parent=this;this.children.push(child);}},
    replaceChildren(...values){this.children=[];this.append(...values);},
    remove(){if(this.parent)this.parent.children=this.parent.children.filter(v=>v!==this);},
  });
  const document={createElement:element,querySelector(selector){if(!nodes.has(selector))nodes.set(selector,element('section'));return nodes.get(selector);}};
  let binding='bound', mode, heldList, heldDetail, heldPost;
  const sources=new Map(),receipts=new Map(),calls=[];
  for(let n=1;n<=count;n++) sources.set(`request-${n}`,{id:`request-${n}`,sequence:n,binding,revision:`revision-${n}-0`,
    state:'pending',kind:'question',actionable:true,title:`Choice ${n}`,label:`#Worker${n}`,body:'ABCDEFGHIJ',choices:[{label:'Selected scope',action:{kind:'option',index:0}}],allows_text:true,can_cancel:true,question_index:0,question_total:2,recorded_revisions:[]});
  const error=status=>Object.assign(new Error('Unavailable'),{status});
  async function api(route,body) {
    calls.push({route,body:body&&clone(body)});
    const url=new URL(route,'https://fixture.invalid/'),id=url.pathname.split('/')[3];
    if(body) {
      const current=sources.get(id),key=id+':'+body.revision;
      if(mode==='before'){mode=undefined;throw new TypeError('Offline before capture');}
      if(!receipts.has(key)) {
        if(body.revision!==current.revision)throw error(409);
        receipts.set(key,{id,binding,revision:body.revision,recorded:true,action:clone(body.action)});
        current.recorded_revisions.push(body.revision);
        if(current.kind==='question'&&current.question_index===0&&body.action.kind!=='cancel') {
          current.question_index=1;current.revision=`revision-${current.sequence}-1`;current.body='NEXT';
        }else{current.state=body.action.kind==='cancel'?'cancelled':'answering';current.actionable=false;current.revision+='-done';}
      }else assert.deepEqual(receipts.get(key).action,body.action);
      if(heldPost){const hold=heldPost;heldPost=undefined;hold.enter();await hold.promise;}
      if(mode==='after'){mode=undefined;throw new TypeError('Lost acknowledgment');}
      return clone(receipts.get(key));
    }
    if(url.pathname.endsWith('/receipt')) {
      const value=receipts.get(id+':'+url.searchParams.get('revision'));if(!value)throw error(409);return clone(value);
    }
    if(url.pathname==='/interaction/requests') {
      const after=Number(url.searchParams.get('after'));
      const all=[...sources.values()].filter(v=>v.sequence>after&&['pending','answering','unknown'].includes(v.state)).sort((a,b)=>a.sequence-b.sequence);
      const selected=all.slice(0,8).map(value=>{const v=clone(value);v.preview=v.body.slice(0,3);delete v.body;delete v.choices;delete v.allows_text;delete v.can_cancel;return v;});
      const snapshot={binding,items:selected,cursor:selected.at(-1)?.sequence||after,more:all.length>8,csrf:'synthetic'};
      if(heldList){const hold=heldList;heldList=undefined;hold.enter();await hold.promise;}
      return snapshot;
    }
    const current=sources.get(id);if(!current)throw error(409);
    const offset=Number(url.searchParams.get('offset'));
    if(url.searchParams.has('revision')&&url.searchParams.get('revision')!==current.revision)throw error(409);
    const value={...clone(current),offset,body:current.body.slice(offset,offset+4),next_offset:offset+4<current.body.length?offset+4:null};
    if(heldDetail){const hold=heldDetail;heldDetail=undefined;hold.enter();await hold.promise;}
    return value;
  }
  const context=vm.createContext({window:{},document,TextEncoder,URLSearchParams});
  vm.runInContext(fs.readFileSync('dashboard/web/requests.js','utf8'),context);
  let ui;ui=context.window.RequestControls.mount({api,getBinding:()=>binding,setCsrf(){},unauthorized(){ui.reset();}});
  const all=(el,predicate)=>[...(predicate(el)?[el]:[]),...el.children.flatMap(child=>all(child,predicate))];
  const button=(card,text)=>all(card,el=>el.tag==='button'&&el.textContent===text)[0];
  return {ui,nodes,sources,receipts,calls,button,all,
    cards:()=>nodes.get('#conversation-request-cards').children,
    mode:value=>{mode=value;},bind(value){binding=value;for(const source of sources.values())source.binding=value;},
    holdList(){return heldList=deferred();},holdDetail(){return heldDetail=deferred();},holdPost(){return heldPost=deferred();},
    async read(card){let b=button(card,'Read request')||button(card,'Read current request')||button(card,'Review current request')||button(card,'Refresh request');await b.handlers.click();while((b=button(card,'Continue reading request')))await b.handlers.click();},
  };
}
(async()=>{
  const f=boot();await f.ui.refresh();const card=f.cards()[0];
  await f.button(card,'Read request').handlers.click();
  const option=f.button(card,'Selected scope');assert(option.disabled,'A partial proposal cannot be approved/answered');
  await f.button(card,'Continue reading request').handlers.click();await f.button(card,'Continue reading request').handlers.click();
  assert(!option.disabled||!f.button(card,'Selected scope').disabled);
  f.mode('after');await f.button(card,'Selected scope').handlers.click();
  assert.equal(f.receipts.size,1);assert.equal(f.calls.filter(v=>v.body).length,1);
  assert(!f.button(card,'Retry this same decision').hidden);
  await f.button(card,'Check decision').handlers.click();
  assert.equal(f.calls.filter(v=>v.body).length,1,'Receipt inspection must not repeat a POST');
  assert.equal(f.receipts.size,1);assert(f.button(card,'Selected scope').disabled,'The next question requires its own review');
  await option.handlers.click();assert.equal(f.calls.filter(v=>v.body).length,1,'An old detached option cannot answer a new proposal');
  await f.read(card);
  const text=f.all(card,v=>v.tag==='textarea')[0];text.value='  Exact second answer  ';text.handlers.input();
  await f.all(card,v=>v.tag==='form')[0].handlers.submit({preventDefault(){}});
  assert.equal(f.receipts.size,2);assert.equal([...f.receipts.values()][1].action.text,'  Exact second answer  ');
  assert.equal(text.value,'');

  const offline=boot();await offline.ui.refresh();const off=offline.cards()[0];await offline.read(off);
  offline.mode('before');await offline.button(off,'Selected scope').handlers.click();assert.equal(offline.receipts.size,0);
  await offline.button(off,'Check decision').handlers.click();assert.equal(offline.receipts.size,0);
  await offline.button(off,'Retry this same decision').handlers.click();assert.equal(offline.receipts.size,1);
  const posts=offline.calls.filter(v=>v.body);assert.deepEqual(posts[0].body,posts[1].body,'Explicit retry must freeze source revision and exact action');

  const draft=boot();await draft.ui.refresh();const dc=draft.cards()[0];await draft.read(dc);
  const editor=draft.all(dc,v=>v.tag==='textarea')[0];editor.value='Draft for the original question';editor.handlers.input();
  const source=draft.sources.get('request-1');source.question_index=1;source.revision='external-question-2';source.body='NEW';
  await draft.ui.refresh();assert.equal(editor.value,'');
  assert(draft.all(dc,v=>v.textContent==='Draft for the original question').length,'Changed questions retain the unsent draft');
  await draft.read(dc);assert.equal(draft.button(dc,'Selected scope').disabled,false);

  source.state='expired';source.actionable=false;source.revision='expired-fallback';delete source.question_index;
  await draft.ui.refresh();
  assert(!draft.button(dc,'Dismiss').hidden,'Terminal fallback without a question index closes the second question');
  assert(draft.all(dc,v=>v.textContent==='This request is closed.').length);
  assert(draft.button(dc,'Selected scope')?.disabled ?? true,'Closed source cannot retain actionable choices');

  const page=boot(25);await page.ui.refresh();assert.equal(page.cards().length,8);
  await page.nodes.get('#conversation-requests-more').handlers.click();assert.equal(page.cards().length,16);
  await page.ui.refresh();assert.equal(page.cards().length,24);
  await page.nodes.get('#conversation-requests-more').handlers.click();assert.equal(page.cards().length,25);
  await page.ui.refresh();assert(page.nodes.get('#conversation-requests-more').hidden);
  page.sources.get('request-12').state='expired';page.sources.get('request-12').actionable=false;page.sources.get('request-12').revision+='-expired';
  await page.ui.refresh();const closed=page.cards().find(c=>page.all(c,v=>v.textContent.startsWith('#Worker12 ·')).length);
  assert(!page.button(closed,'Dismiss').hidden,'A request closed elsewhere is refreshed outside the first page');

  const race=boot();const held=race.holdList();const stale=race.ui.refresh();await held.entered;
  race.ui.reset();race.bind('new-binding');await race.ui.refresh();assert.equal(race.cards().length,1);
  held.release();await stale;assert.equal(race.cards().length,1,'Late old-scope reads cannot repopulate reset controls');
  const rc=race.cards()[0];const late=race.holdDetail();const reading=race.button(rc,'Read request').handlers.click();await late.entered;
  race.ui.reset();late.release();await reading;assert.equal(race.cards().length,0);
  console.log('Request UI: full-source gating, source-fenced choices, receipt-only recovery, exact retry, saved drafts, pagination/closure and async scope reset passed; zero models.');
})().catch(error=>{console.error(error);process.exitCode=1;});
