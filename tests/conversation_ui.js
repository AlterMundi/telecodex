// Exercise the shipped conversation state machine with synthetic DOM/network.
// No native sessions, external providers, or models are called.
const assert = require("node:assert/strict");
const fs = require("node:fs");
const vm = require("node:vm");

function boot(readOnly=false,standalone=false,options={}) {
  const nodes = new Map(), requests = [], audioCalls=[], sourceOverrides=new Map();
  const recoveryPosts=[];
  const gaps=new Map(),documentEvents=new Map(),windowEvents=new Map();
  const subscribe=(events,name,fn)=>events.set(name,[...(events.get(name)||[]),fn]);
  const clock={pending:()=>[...gaps.values()].map(t=>t.ms),fire(ms){
    const selected=[...gaps].filter(([,timer])=>timer.ms===ms);
    for(const [id,timer] of selected){gaps.delete(id);timer.fn();}
  }};
  const timeout=(fn,ms)=>{
    if(options.clock && [180,450,1200].includes(ms)){const id={};gaps.set(id,{fn,ms});return id;}
    return setTimeout(fn,ms);
  };
  const cancelTimeout=id=>{if(gaps.has(id))gaps.delete(id);else clearTimeout(id);};
  let capture,intersection;const replyTexts=new Map(),partReads=[],replyHolds=new Map();
  function element() {
    return {
      children: [], dataset: {}, handlers: {}, classList: {toggle() {}},
      textContent: "", value:"", paused: true,
      setAttribute(name, value) { this[name] = value; },
      removeAttribute(name) { delete this[name]; },
      addEventListener(name, fn) { this.handlers[name] = fn; },
      append(...children) { this.children.push(...children); },
      replaceChildren(...children) { this.children = children; },
      insertBefore(child, before) {
        const previous = this.children.indexOf(child);
        if (previous >= 0) this.children.splice(previous,1);
        const at = this.children.indexOf(before);
        this.children.splice(at < 0 ? this.children.length : at, 0, child);
      },
      click() {this.clicked=true;},remove() {this.removed=true;},
      before() {}, pause() { this.paused = true; }, play() {this.paused=false;this.ended=false;return Promise.resolve();}, load() {},
    };
  }
  const document = {
    hidden: false, body: element(), addEventListener(name,fn) {subscribe(documentEvents,name,fn);}, createElement: element,
    querySelector(selector) {
      if (!nodes.has(selector)) nodes.set(selector, element());
      return nodes.get(selector);
    },
  };
  let presentationProfile="simple";
  let count = 8, arriveDuringRecent = false, heldValidation, renderAudio=false, multipart=false,audioFailures=0;
  const source = n => ({id:`native:${n}`, kind:standalone&&n===1?"integrator_progress":"integrator_answer", sequence:n, observed_at:new Date(1704067200000+n*1000).toISOString(),
    revision:`revision-${n}`, text_sha256:`sha-${n}`, preview:`Reply ${n}`,...sourceOverrides.get(n)});
  const originals = new Map(), telegramOriginals = new Map(), nativeRequests = new Map(), humanTasks=new Map(), nativePosts = [];
  let losePost = false, offlinePost = false, heldPost, heldInputs, heldStatus, heldRoute, routePhase="accepted", submitPhase = "accepted";
  const api = async (route, body) => {
    const url = new URL(route, "https://fixture.invalid/");
    if(url.pathname==='/interaction/telegram-inputs'){
      const before=Number(url.searchParams.get('before')),after=url.searchParams.get('direction')==='after';
      const rows=[...telegramOriginals.values()].filter(v=>after?v.sequence>before:!before||v.sequence<before).sort((a,b)=>after?a.sequence-b.sequence:b.sequence-a.sequence);
      const items=rows.slice(0,8);requests.push({telegram:true,body});
      return {schema:'telecodex.telegram-input-list/v1',binding:'fixture',items,cursor:items.at(-1)?.sequence||0,newest:Math.max(0,...items.map(v=>v.sequence)),more:rows.length>8};
    }
    if(url.pathname.endsWith('/trace/original')) {
      const id=decodeURIComponent(url.pathname.split('/').at(-3)),offset=Number(url.searchParams.get('offset'));
      return {id,source:'original',offset,text:(originals.get(id)||telegramOriginals.get(id)).text.slice(offset),next_offset:null};
    }
    if(url.pathname.endsWith('/trace')) {
      const id=decodeURIComponent(url.pathname.split('/').at(-2)),original=originals.get(id)||telegramOriginals.get(id);
      requests.push({trace:id,body});
      return {schema:'telecodex.conversation-trace/v1',id,text:original.longTrace?original.text.slice(0,2):original.text,text_next_offset:original.longTrace?2:null,phase:original.phase,read_only:true,
        routes:[{label:'Alpha',instruction:'retained-instruction',requests:['parent-request'],relation:{kind:'amends'},transport_phase:'accepted',forwarded_original:'Exact correction <script>literal</script>',target_available:true,results:[]}],
        evidence:'Receiver-retained original; native acceptance is not incorporation.'};
    }
    if (url.pathname === "/interaction/tasks") return {binding:"fixture",items:[...humanTasks.values()],cursor:humanTasks.size,more:false};
    if (url.pathname.startsWith("/interaction/tasks/")) return {...humanTasks.get(url.pathname.split("/").at(-1)),offset:0,next_offset:null};
    if (url.pathname === "/interaction/requests") return {binding:"fixture",items:[...nativeRequests.values()],cursor:nativeRequests.size,more:false};
    if (url.pathname.startsWith("/interaction/requests/")) {
      if (body) {nativePosts.push(JSON.parse(JSON.stringify(body)));throw new TypeError("Lost decision acknowledgement");}
      return {...nativeRequests.get(url.pathname.split("/").at(-1)),offset:0,next_offset:null};
    }
    if (url.pathname === "/interaction/inputs" && body) {
      if (offlinePost) {offlinePost=false;throw new TypeError("Offline before HTTP delivery");}
      const value = {...body,sequence:originals.size+1,phase:submitPhase,new_input:!originals.has(body.id)};
      originals.set(body.id,value);
      if (heldPost) { heldPost.started=true; await heldPost.promise; }
      if (losePost) { losePost=false; throw new TypeError("Lost POST acknowledgement"); }
      return value;
    }
    if (url.pathname.endsWith("/retry") && body) {
      recoveryPosts.push(body);const id=url.pathname.split("/").at(-2);
      const old=originals.get(id),value={...old,recoveries:[],routing:{...old.routing,phase:"accepted"}};
      originals.set(id,value);
      if(options.loseRecovery){options.loseRecovery=false;throw new TypeError("Lost recovery acknowledgment");}
      return value;
    }
    if (url.pathname.endsWith("/route") && body) {
      const id=url.pathname.split("/").at(-2);
      const value={...originals.get(id),phase:"routed",routing:{target:{thread_id:7},label:"Alpha",instruction:"instruction-1",phase:routePhase},continuation:{source:`input:${id}`,revision:"a".repeat(64)}};
      if(heldRoute){const held=heldRoute;heldRoute=undefined;held.started();await held.promise;}
      originals.set(id,value);return value;
    }
    if (url.pathname.startsWith("/interaction/inputs/")) {
      if (heldStatus) {const held=heldStatus;heldStatus=undefined;held.started();await held.promise;if(held.fail)throw new TypeError("Delayed status failure");}
      return originals.get(url.pathname.split("/").at(-1));
    }
    if (url.pathname === "/interaction/inputs") {
      const before=Number(url.searchParams.get("before")), direction=url.searchParams.get("direction");
      const rows=[...originals.values()].filter(v=>direction==="after"?v.sequence>before:!before||v.sequence<before);
      rows.sort((a,b)=>direction==="after"?a.sequence-b.sequence:b.sequence-a.sequence);
      const selected=rows.slice(0,8);
      const snapshot={binding:"fixture",csrf:"synthetic",routing_choices:[{selector:"t7",label:"Alpha"},{selector:"t8",label:"Beta"}],items:selected.map(v=>({...v})),cursor:selected.at(-1)?.sequence||0,newest:Math.max(0,...selected.map(v=>v.sequence)),more:rows.length>8};
      if (heldInputs) {const held=heldInputs;heldInputs=undefined;held.started();await held.promise;}
      return snapshot;
    }
    if (url.pathname.endsWith("/part")) {
      if (heldValidation) { heldValidation.started = true; return heldValidation.promise; }
      const sourceId=url.searchParams.get('source'),offset=Number(url.searchParams.get('offset'));
      partReads.push({source:sourceId,offset});
      if(replyHolds.has(sourceId)){const hold=replyHolds.get(sourceId);replyHolds.delete(sourceId);hold.started();await hold.promise;}
      if(replyTexts.has(sourceId)){const text=replyTexts.get(sourceId),end=Math.min(offset+1600,text.length);return {text:text.slice(offset,end),next_offset:end<text.length?end:null};}
      return {text:"Selected reply", next_offset:null};
    }
    const direction = url.searchParams.get("direction");
    const cursor = Number(url.searchParams.get("native"));
    requests.push({direction, cursor});
    if (direction === "before" && arriveDuringRecent) { count += 9; arriveDuringRecent = false; }
    const numbers = Array.from({length:count}, (_, n) => n + 1)
      .filter(n => direction === "after" ? n > cursor : !cursor || n < cursor);
    if (direction === "before") numbers.reverse();
    const selected = numbers.slice(0, 8);
    return {binding:"fixture", csrf:"synthetic", can_listen:true,can_download_files:options.fileAccess===true,artifacts:options.artifacts,presentation_profile:presentationProfile,
      items:selected.map(source), cursor:{delivery:0,native:selected.at(-1) || cursor},
      newest:{delivery:0,native:Math.max(0,...selected)}, more:numbers.length > 8};
  };
  const context = vm.createContext({window:{addEventListener(name,fn) {subscribe(windowEvents,name,fn);}}, document,
    location:{href:"https://fixture.invalid/?view=conversation"}, history:{replaceState() {}},
    URL, URLSearchParams, AbortController, setTimeout:timeout, clearTimeout:cancelTimeout, TextEncoder, Blob, crypto:require("node:crypto"),
    fetch:(_route, options) => {
      if(_route.startsWith('api/interaction/artifacts/'))return Promise.resolve(new Response('Actual fixture file',{headers:{'X-Artifact-Sha256':require('node:crypto').createHash('sha256').update('Actual fixture file').digest('hex')}}));
      audioCalls.push(JSON.parse(options.body));
      if(audioFailures){audioFailures--;return Promise.resolve(new Response(JSON.stringify({detail:'Synthetic TTS outage'}),{status:503}));}
      if(renderAudio){
        const selected=JSON.parse(options.body),data=Buffer.from('synthetic audio'),hash=require('node:crypto').createHash('sha256').update(data).digest('hex');
        return Promise.resolve(new Response(data,{headers:{'x-presentation-source':selected.source,'x-presentation-revision':selected.revision,'x-presentation-offset':String(selected.offset),'x-presentation-next-offset':multipart&&selected.offset===0?'1500':'','x-audio-sha256':hash}}));
      }
      return new Promise((_resolve, reject) => {
      options.signal.addEventListener("abort", () => reject(new Error("Aborted")), {once:true});
    });},
  });
  vm.runInContext(fs.readFileSync("dashboard/web/requests.js", "utf8"), context);
  vm.runInContext(fs.readFileSync("dashboard/web/microphone.js", "utf8"), context);
  if(options.capture)context.window.VoiceInput={mount(value){capture=value;return {configure(){},reset(){},deactivate(){},observe(){},ownsSend:()=>!!options.ownsVoice,submit:()=>options.voiceSubmit?.()};}};
  vm.runInContext(fs.readFileSync("dashboard/web/conversation.js", "utf8"), context);
  if(options.autoText)context.window.IntersectionObserver=class{
    constructor(callback){intersection=callback;}observe(){}unobserve(){}disconnect(){}
  };
  const ui = context.window.Conversation.mount({api,readOnly,standalone,headers:() => ({}),setCsrf() {},modeChanged() {}});
  ui.authenticated();
  return {ui, nodes, requests, originals,replyTexts,partReads,humanTasks,
    holdReplyText(id){let release,started;const entered=new Promise(yes=>{started=yes;}),promise=new Promise(yes=>{release=yes;});replyHolds.set(id,{promise,started});return {entered,release};},
    intersect(id,visible=true){const target=nodes.get('#conversation-timeline').children.find(c=>c.dataset.sourceId===id);assert(target,JSON.stringify({id,keys:nodes.get("#conversation-timeline").children.map(c=>c.dataset),state:nodes.get("#conversation-state")?.textContent}));intersection([{target,isIntersecting:visible}]);}, telegramOriginals, nativePosts,audioCalls,clock,recoveryPosts,
    profile(value){presentationProfile=value;},
    event(name){for(const fn of windowEvents.get(name)||[])fn();},
    hide(){document.hidden=true;for(const fn of documentEvents.get('visibilitychange')||[])fn();},
    show(){document.hidden=false;for(const fn of documentEvents.get('visibilitychange')||[])fn();},
    capture(){capture.pauseAudio();capture.onActivity(true);},
    finishCapture(){capture.onActivity(false);},
    capturedInput(value){capture.onInput(value);},
    failAudio(n=1){audioFailures=n;},
    closeNativeRequest(){Object.assign(nativeRequests.get('native-request'),{state:'answered',actionable:false});},
    overrideSource(n,value){sourceOverrides.set(n,value);},addReplies(n){count+=n;},audio(parts=false){renderAudio=true;multipart=parts;},
    nativeRequest() {
      nativeRequests.set("native-request",{id:"native-request",binding:"fixture",revision:"native-revision",state:"pending",actionable:true,kind:"question",title:"Scope",label:"Conversation",body:"Choose scope",choices:[{label:"Selected scope",action:{kind:"option",index:0}}],allows_text:true,can_cancel:true,question_index:0,question_total:1,recorded_revisions:[]});
    },
    advanceNativeRequest() {Object.assign(nativeRequests.get("native-request"),{revision:"native-revision-2",question_index:1,title:"Next question"});},
    holdPart() {
      let release,started;
      const entered=new Promise(yes=>{started=yes;}),promise=new Promise(yes=>{release=yes;});
      heldValidation={promise,started:false};
      Object.defineProperty(heldValidation,'started',{set(value){if(value)started();}});
      return {entered,release};
    },
    receivingPhase(value) {submitPhase=value;},
    lose() {losePost=true;},
    offline() {offlinePost=true;},
    routePhase(value){routePhase=value;},
    holdRoute() {
      let release, started;
      const entered=new Promise(yes=>{started=yes;}),promise=new Promise(yes=>{release=yes;});
      heldRoute={promise,started};return {entered,release};
    },
    holdStatus(fail=false) {
      let release, started;
      const entered=new Promise(yes=>{started=yes;}),promise=new Promise(yes=>{release=yes;});
      heldStatus={promise,started,fail};return {entered,release};
    },
    holdInputs() {
      let release, started;
      const entered=new Promise(yes=>{started=yes;}), promise=new Promise(yes=>{release=yes;});
      heldInputs={promise,started};return {entered,release};
    },
    posting() {return !!heldPost?.started;},
    holdPost() {let release; const promise = new Promise(yes=>{release=yes;}); heldPost={promise}; return release;},
    arrive() { count = 58; arriveDuringRecent = true; },
    validating() { return !!heldValidation?.started; },
    hold() {
      let reject;
      const promise = new Promise((_, no) => { reject = no; });
      heldValidation = {promise}; return reject;
    },
  };
}

(async () => {
  const automatic=boot(false,true,{autoText:true});
  const bodies=new Map([['native:8','A complete answer. '.repeat(200)],['native:7','A second topic. '.repeat(200)],['native:6','A third topic. '.repeat(200)]]);
  const holds=new Map();
  for(const [id,text] of bodies){const n=Number(id.split(':')[1]);automatic.overrideSource(n,{preview:text.slice(0,20),text_characters:text.length});automatic.replyTexts.set(id,text);holds.set(id,automatic.holdReplyText(id));}
  await automatic.ui.refresh();
  for(const id of bodies.keys())automatic.intersect(id);
  await Promise.all([holds.get('native:8').entered,holds.get('native:7').entered]);
  assert.equal(automatic.partReads.length,2,'visible answers may use only two concurrent readers');
  const autoCompose=automatic.nodes.get('#conversation-compose'),autoDraft=automatic.nodes.get('#conversation-message');
  autoDraft.value='A direction while answer text is still arriving';await autoCompose.handlers.submit({preventDefault(){}});
  assert.equal(automatic.originals.size,1,'answer retrieval must not block the next human input');
  holds.get('native:8').release();holds.get('native:7').release();
  await holds.get('native:6').entered;holds.get('native:6').release();await new Promise(setImmediate);
  for(const [id,text] of bodies){
    const card=automatic.nodes.get('#conversation-timeline').children.find(c=>c.dataset.sourceId===id);
    assert.equal(card.children[2].textContent,text,'normal visible answers should expand without a Read reply click');
    assert.equal(card.children[3].children[0].hidden,true);
  }
  const loaded=automatic.partReads.length;for(const id of bodies.keys())automatic.intersect(id);await automatic.ui.refresh();await new Promise(setImmediate);
  assert.equal(automatic.partReads.length,loaded,'unchanged metadata/visibility must not reread completed text');
  automatic.overrideSource(8,{preview:'🙂🙂',text_characters:8,text_sha256:'new-emoji',revision:'emoji-revision'});automatic.replyTexts.set('native:8','🙂🙂abcdef');
  await automatic.ui.refresh();automatic.intersect('native:8');await new Promise(setImmediate);
  assert.equal(automatic.nodes.get('#conversation-timeline').children.find(c=>c.dataset.sourceId==='native:8').children[2].textContent,'🙂🙂abcdef','truncation uses Unicode code points, not UTF-16 units');
  const scrolledAway=boot(false,true,{autoText:true});const longText='Visible text only. '.repeat(300);
  scrolledAway.overrideSource(8,{preview:'Visible',text_characters:longText.length});scrolledAway.replyTexts.set('native:8',longText);
  await scrolledAway.ui.refresh();const heldText=scrolledAway.holdReplyText('native:8');scrolledAway.intersect('native:8');await heldText.entered;
  scrolledAway.intersect('native:8',false);heldText.release();await new Promise(setImmediate);
  assert.equal(scrolledAway.partReads.length,1,'scrolling away must stop further automatic parts');
  scrolledAway.intersect('native:8');await new Promise(setImmediate);
  assert.equal(scrolledAway.nodes.get('#conversation-timeline').children.find(c=>c.dataset.sourceId==='native:8').children[2].textContent,longText,'returning to the reply resumes its remaining parts');
  const replacement=boot(false,true,{autoText:true});
  replacement.overrideSource(8,{preview:'Old',text_characters:2000});replacement.replyTexts.set('native:8','Old answer '.repeat(200));
  await replacement.ui.refresh();const oldText=replacement.holdReplyText('native:8');replacement.intersect('native:8');await oldText.entered;
  replacement.overrideSource(8,{preview:'New',text_characters:2600,text_sha256:'replacement-hash',revision:'replacement-revision'});replacement.replyTexts.set('native:8','New answer '.repeat(250));
  await replacement.ui.refresh();replacement.intersect('native:8');oldText.release();await new Promise(setImmediate);
  assert.equal(replacement.nodes.get('#conversation-timeline').children.find(c=>c.dataset.sourceId==='native:8').children[2].textContent,'New answer '.repeat(250),'late text from an earlier revision must not overwrite its replacement');
  const profile=boot(false,true);
  profile.profile("explorer");await profile.ui.refresh();
  assert.equal(profile.nodes.get("#conversation-diagnostics").open,true);
  profile.nodes.get("#conversation-diagnostics").open=false;await profile.ui.refresh();
  assert.equal(profile.nodes.get("#conversation-diagnostics").open,false,"polling cannot override a person's read-only panel choice");
  profile.profile("simple");await profile.ui.refresh();
  assert.equal(profile.nodes.get("#conversation-diagnostics").open,false);
  profile.nodes.get("#conversation-diagnostics").open=true;await profile.ui.refresh();
  assert.equal(profile.nodes.get("#conversation-diagnostics").open,true,"simple still permits looking under the hood");
  const single=boot(false,true);
  single.originals.set('between',{id:'between',text:'A direction while work continues',sequence:1,phase:'accepted',received_at:'2024-01-01T00:00:04.500Z'});
  await single.ui.refresh();
  const workers=boot(false,true);
  workers.overrideSource(8,{kind:"worker_progress",label:"Alpha",target:{chat_id:100,thread_id:7},preview:"The newer useful update"});
  workers.overrideSource(7,{kind:"worker_progress",label:"Alpha",target:{chat_id:100,thread_id:7},preview:"An earlier raw update"});
  await workers.ui.refresh();
  const workerTimeline=workers.nodes.get('#conversation-timeline'),workerDetails=workers.nodes.get('#conversation-results');
  assert(!workerTimeline.children.some(c=>c.dataset.timelineKey==='native:8'),'raw worker commentary belongs under inspection');
  assert(workerDetails.children.some(c=>c.dataset.sequence==='8'),'raw update must remain inspectable');
  const compact=workerTimeline.children.filter(c=>c.dataset.timelineKey?.startsWith('session-update:'));
  assert.equal(compact.length,1,'parallel session updates must coalesce by actual target, not title');
  assert.equal(compact[0].children[1].textContent,'The newer useful update');
  workers.addReplies(1);workers.overrideSource(9,{kind:"worker_result",label:"Alpha",target:{chat_id:100,thread_id:7},preview:"The useful final answer"});
  await workers.ui.refresh();
  assert(workerTimeline.children.some(c=>c.dataset.timelineKey==='native:9'),'the only useful final answer must stay in the main chat');
  assert.equal(compact[0].removed,true,'a returned answer settles an earlier progress preview');
  const stream=single.nodes.get('#conversation-timeline');
  assert.deepEqual(stream.children.filter(c=>c.dataset.timelineKey).map(c=>c.dataset.timelineKey),['native:1','native:2','native:3','native:4','input:between','native:5','native:6','native:7','native:8']);
  assert.equal(stream.children.find(c=>c.dataset.timelineKey==='native:1').children[1].textContent,'Progress update.');
  assert.equal(stream.children.find(c=>c.dataset.timelineKey==='input:between').children[0].textContent,'You');
  await single.ui.refresh();
  assert.equal(stream.children.filter(c=>c.dataset.timelineKey).length,9,'Refreshing keeps one card per saved input or native item');
  assert.equal(single.nodes.get('#conversation-replies').hidden,false,'raw session updates remain available inside collapsed inspection');
  // Real browser review exposed lost follow mode when an existing voice card
  // grows from a placeholder into its transcript. Measure before DOM mutation.
  const follow=boot(false,true,{capture:true});await follow.ui.refresh();
  const live=follow.nodes.get('#conversation-timeline');
  const contentSize=node=>String(node.textContent||'').length+node.children.reduce((n,c)=>n+contentSize(c),0);
  Object.defineProperty(live,'scrollHeight',{get:()=>1000+contentSize(live)});
  live.clientHeight=200;live.scrollTop=live.scrollHeight-live.clientHeight;
  follow.overrideSource(8,{revision:'expanded',preview:'A newly expanded response '.repeat(30)});
  await follow.ui.refresh();
  assert.equal(live.scrollTop,live.scrollHeight,'A growing current reply retains live following');
  live.scrollTop=0;
  follow.overrideSource(8,{revision:'expanded-again',preview:'A further update '.repeat(60)});
  await follow.ui.refresh();
  assert.equal(live.scrollTop,0,'Updates never pull a reader away from earlier history');
  follow.capturedInput({id:'own-voice',phase:'preparing',voice:{audio_sha256:'owned'},sequence:1});
  assert.equal(live.scrollTop,live.scrollHeight,'An explicitly submitted voice input returns to the live conversation');
  follow.originals.set('own-voice',{id:'own-voice',phase:'accepted',voice:{audio_sha256:'owned'},text:'The committed automatic transcript '.repeat(20),sequence:1});
  await follow.ui.refresh();
  assert.equal(live.scrollTop,live.scrollHeight,'The committed transcript cannot silently leave following mode');
  const until=async check=>{for(let n=0;n<100;n++){if(check())return;await new Promise(resolve=>setTimeout(resolve,5));}throw Error("Voice transition did not finish");};
  let voiceSubmissions=0;
  const ownedVoice=boot(false,false,{capture:true,ownsVoice:true,voiceSubmit:()=>{voiceSubmissions++;}});await ownedVoice.ui.refresh();
  ownedVoice.nodes.get('#conversation-message').value='Keep this text draft while sending speech';
  await ownedVoice.nodes.get('#conversation-compose').handlers.submit({preventDefault(){}});
  assert.equal(voiceSubmissions,1);assert.equal(ownedVoice.nativePosts.length,0);assert.equal(ownedVoice.nodes.get('#conversation-message').value,'Keep this text draft while sending speech');ownedVoice.ui.stop();
  const voice=boot();voice.audio();await voice.ui.refresh();
  voice.nodes.get('#conversation-voice-replies').handlers.click();
  assert.equal(voice.audioCalls.length,0,'Enabling voice never recites old history');
  voice.addReplies(2);await voice.ui.refresh();
  const voicePlayer=voice.nodes.get('#conversation-audio');
  await until(()=>voice.audioCalls.length===1&&!voicePlayer.paused);
  voicePlayer.ended=true;await voicePlayer.handlers.ended();
  await until(()=>voice.audioCalls.length===2&&!voicePlayer.paused);
  assert.deepEqual(voice.audioCalls.map(r=>r.source),['native:9','native:10']);
  voicePlayer.ended=true;await voicePlayer.handlers.ended();await voice.ui.refresh();
  assert.equal(voice.audioCalls.length,2,'Refresh does not repeat played replies');
  voice.nodes.get('#conversation-voice-replies').handlers.click();
  voice.addReplies(1);await voice.ui.refresh();assert.equal(voice.audioCalls.length,2,'Turning voice off retains text without preparing more audio');
  // Hold only player gaps with a deterministic clock; network/media remain async.
  const paced=boot(false,false,{clock:true});paced.audio();await paced.ui.refresh();
  paced.nodes.get('#conversation-voice-replies').handlers.click();paced.addReplies(3);await paced.ui.refresh();
  const pacedPlayer=paced.nodes.get('#conversation-audio');
  await until(()=>paced.audioCalls.length===1&&!pacedPlayer.paused);
  paced.nativeRequest();await paced.ui.refresh();
  pacedPlayer.ended=true;await pacedPlayer.handlers.ended();
  assert.deepEqual(paced.clock.pending(),[450]);assert.equal(paced.audioCalls.length,1,'The next reply waits for a real silence interval');
  paced.clock.fire(450);await until(()=>paced.audioCalls.length===2&&!pacedPlayer.paused);
  assert.equal(paced.audioCalls[1].source,'request:native-request','A live question takes priority over queued worker output');
  pacedPlayer.ended=true;await pacedPlayer.handlers.ended();
  assert.deepEqual(paced.clock.pending(),[1200],'A question leaves a longer answer opportunity');
  assert.match(paced.nodes.get('#conversation-audio-state').textContent,/Talk to answer or change topic/);
  assert.equal(paced.nodes.get('#conversation-requests').hidden,false,'The actionable question remains visible');
  paced.clock.fire(1200);await until(()=>paced.audioCalls.length===3&&!pacedPlayer.paused);
  assert.equal(paced.audioCalls[2].source,'native:10');paced.ui.stop();

  for(const action of ['pause','stop','hide','disable','capture','offline','pagehide']){
    const gap=boot(false,false,{clock:true,capture:true});gap.audio(true);await gap.ui.refresh();
    gap.nodes.get('#conversation-voice-replies').handlers.click();gap.addReplies(1);await gap.ui.refresh();
    const player=gap.nodes.get('#conversation-audio');await until(()=>gap.audioCalls.length===1&&!player.paused);
    player.ended=true;const ending=player.handlers.ended();
    assert.deepEqual(gap.clock.pending(),[180],'A chunk boundary does not immediately start the next segment');
    if(action==='pause')gap.ui.pause();
    if(action==='stop')gap.ui.stop();
    if(action==='hide')gap.hide();
    if(action==='disable')gap.nodes.get('#conversation-voice-replies').handlers.click();
    if(action==='capture')gap.capture();
    if(['offline','pagehide'].includes(action))gap.event(action);
    await ending;gap.clock.fire(180);
    assert.equal(gap.audioCalls.length,1,`${action} cancels the chunk wait`);
    assert.deepEqual(gap.clock.pending(),[]);
    gap.ui.stop();
  }
  const outage=boot(false,false,{clock:true});outage.audio();await outage.ui.refresh();
  outage.nodes.get('#conversation-voice-replies').handlers.click();outage.failAudio();outage.addReplies(2);await outage.ui.refresh();
  await until(()=>outage.clock.pending().includes(450));
  assert.match(outage.nodes.get('#conversation-state').textContent,/Synthetic TTS outage|replies available/);
  outage.clock.fire(450);await until(()=>outage.audioCalls.length===2&&!outage.nodes.get('#conversation-audio').paused);
  assert.equal(outage.audioCalls[1].source,'native:10','A failed TTS segment cannot strand the next independent reply');outage.ui.stop();

  const noPlayer=boot(false,false,{clock:true});noPlayer.audio();await noPlayer.ui.refresh();
  noPlayer.nodes.get('#conversation-voice-replies').handlers.click();noPlayer.failAudio();noPlayer.addReplies(2);await noPlayer.ui.refresh();
  await until(()=>noPlayer.clock.pending().includes(450));
  assert.equal(noPlayer.nodes.get('#conversation-player').hidden,true);
  noPlayer.ui.pause();noPlayer.clock.fire(450);await noPlayer.ui.refresh();
  assert.equal(noPlayer.audioCalls.length,1,'Pause cancels an interreply gap even without an active audio element');noPlayer.ui.stop();

  const reconnectVoice=boot(false,false,{clock:true});reconnectVoice.audio();await reconnectVoice.ui.refresh();
  reconnectVoice.nodes.get('#conversation-voice-replies').handlers.click();reconnectVoice.addReplies(2);await reconnectVoice.ui.refresh();
  const rp=reconnectVoice.nodes.get('#conversation-audio');await until(()=>reconnectVoice.audioCalls.length===1&&!rp.paused);
  rp.ended=true;await rp.handlers.ended();reconnectVoice.event('offline');reconnectVoice.clock.fire(450);
  reconnectVoice.addReplies(2);reconnectVoice.event('online');
  await until(()=>!reconnectVoice.nodes.get('#conversation-refresh').disabled);await reconnectVoice.ui.refresh();
  assert.equal(reconnectVoice.audioCalls.length,1,'Reconnect must not resume old queued audio or arrivals during suspension');
  assert.match(reconnectVoice.nodes.get('#conversation-voice-label').textContent,/paused/);
  reconnectVoice.nodes.get('#conversation-voice-replies').handlers.click();
  await reconnectVoice.ui.refresh();assert.equal(reconnectVoice.audioCalls.length,1,'Explicit resume starts from the loaded current conversation, not old audio');
  reconnectVoice.addReplies(1);await reconnectVoice.ui.refresh();await until(()=>reconnectVoice.audioCalls.length===2&&!rp.paused);
  assert.equal(reconnectVoice.audioCalls[1].source,'native:13');reconnectVoice.ui.stop();

  const changedGap=boot(false,false,{clock:true});changedGap.audio(true);await changedGap.ui.refresh();
  changedGap.nodes.get('#conversation-voice-replies').handlers.click();changedGap.addReplies(1);await changedGap.ui.refresh();
  const cp=changedGap.nodes.get('#conversation-audio');await until(()=>changedGap.audioCalls.length===1&&!cp.paused);
  cp.ended=true;const changing=cp.handlers.ended();changedGap.overrideSource(9,{revision:'replaced-source'});await changedGap.ui.refresh();
  await changing;changedGap.clock.fire(180);assert.equal(changedGap.audioCalls.length,1,'A changed source cancels its owned continuation gap');changedGap.ui.stop();

  const expiredQuestion=boot(false,false,{clock:true});expiredQuestion.audio();await expiredQuestion.ui.refresh();
  expiredQuestion.nodes.get('#conversation-voice-replies').handlers.click();expiredQuestion.addReplies(1);await expiredQuestion.ui.refresh();
  const eqp=expiredQuestion.nodes.get('#conversation-audio');await until(()=>expiredQuestion.audioCalls.length===1&&!eqp.paused);
  expiredQuestion.nativeRequest();await expiredQuestion.ui.refresh();expiredQuestion.closeNativeRequest();await expiredQuestion.ui.refresh();
  eqp.ended=true;await eqp.handlers.ended();expiredQuestion.clock.fire(450);
  await Promise.resolve();assert.equal(expiredQuestion.audioCalls.length,1,'A question closed before its queue slot must not be synthesized');expiredQuestion.ui.stop();

  const compactVoice=boot();compactVoice.audio();await compactVoice.ui.refresh();
  compactVoice.nodes.get('#conversation-voice-replies').handlers.click();
  compactVoice.overrideSource(9,{kind:'worker_progress',label:'#Alpha',target:{thread_id:7}});
  compactVoice.overrideSource(10,{kind:'worker_result',label:'#Alpha',target:{thread_id:7},current:true});
  compactVoice.addReplies(2);await compactVoice.ui.refresh();
  await until(()=>compactVoice.audioCalls.length===1&&!compactVoice.nodes.get('#conversation-audio').paused);
  assert.equal(compactVoice.audioCalls[0].source,'native:10','A final answer replaces pending voice progress from the same worker');
  const compactPlayer=compactVoice.nodes.get('#conversation-audio');compactPlayer.ended=true;await compactPlayer.handlers.ended();
  compactVoice.overrideSource(11,{kind:'worker_progress',label:'#Alpha',target:{thread_id:7},observed_at:'2024-01-01T00:00:09.500Z'});
  compactVoice.addReplies(1);await compactVoice.ui.refresh();
  assert.equal(compactVoice.audioCalls.length,1,'Late history paging cannot speak old progress after the final answer');
  compactVoice.ui.stop();
  const autoQuestion=boot();autoQuestion.audio();await autoQuestion.ui.refresh();
  autoQuestion.nodes.get('#conversation-voice-replies').handlers.click();autoQuestion.nativeRequest();await autoQuestion.ui.refresh();
  const questionPlayer=autoQuestion.nodes.get('#conversation-audio');
  await until(()=>autoQuestion.audioCalls.length===1&&!questionPlayer.paused);
  assert.equal(autoQuestion.audioCalls[0].source,'request:native-request');
  questionPlayer.ended=true;await questionPlayer.handlers.ended();
  autoQuestion.nodes.get('#conversation-message').value='Start another topic instead';
  await autoQuestion.nodes.get('#conversation-compose').handlers.submit({preventDefault(){}});
  assert.equal([...autoQuestion.originals.values()][0].reply.context_only,true,'An automatically spoken question cannot force the next human message into its session');
  assert.equal(autoQuestion.nativePosts.length,0,'Speaking a question never approves or answers it');
  autoQuestion.advanceNativeRequest();await autoQuestion.ui.refresh();await until(()=>autoQuestion.audioCalls.length===2);assert.equal(autoQuestion.audioCalls[1].revision,'native-revision-2','The next question in one native request must still be spoken');
  autoQuestion.ui.stop();
  const fixture = boot();
  await fixture.ui.refresh();
  fixture.arrive(); // Fifty new replies, then nine more during metadata refresh.
  await fixture.ui.refresh();
  await fixture.ui.refresh();
  await fixture.ui.refresh();
  const cards = fixture.nodes.get("#conversation-answers").children;
  assert.equal(cards.length, 67, "A bounded refresh must not skip the unread backlog");
  assert.equal(new Set(cards.map(card => card.dataset.sequence)).size, 67);
  assert(fixture.requests.some(request => request.direction === "after" && request.cursor === 40),
    "Recent metadata must not move the contiguous forward cursor");

  // A held source check for A must not stop B selected while that check is pending.
  const rejectOld = fixture.hold();
  const first = cards.find(card => card.dataset.sequence === "1");
  const second = cards.find(card => card.dataset.sequence === "2");
  first.children[3].children[1].handlers.click();
  const refresh = fixture.ui.refresh();
  // The refresh awaits its bounded list reads before validating A.
  for (let n = 0; n < 20; n++) await Promise.resolve();
  assert(fixture.validating(), "The regression must actually hold A's validation");
  second.children[3].children[1].handlers.click();
  rejectOld(new Error("Old source changed"));
  await refresh;
  assert.equal(fixture.nodes.get("#conversation-player").hidden, false,
    "Failure of A must not hide B's player");
  assert.equal(fixture.nodes.get("#conversation-audio-state").textContent, "Preparing audio…");
  fixture.ui.stop();
  // Shipped playback completion selects a question only after current-source validation.
  const spoken=boot();spoken.nativeRequest();spoken.audio(true);await spoken.ui.refresh();
  const findButton=(root,text)=>root.children.flatMap(child=>[child,...descendants(child)]).find(el=>el.textContent===text);
  function descendants(root){return root.children.flatMap(child=>[child,...descendants(child)]);}
  const question=spoken.nodes.get('#conversation-request-cards').children[0];
  await findButton(question,'Read request').handlers.click();
  await findButton(question,'Listen to question').handlers.click();
  const player=spoken.nodes.get('#conversation-audio');
  player.ended=true;await player.handlers.ended();
  assert(!spoken.nodes.get('#conversation-focus-label')?.textContent,'A partial spoken question cannot select its answer focus');
  player.ended=true;await player.handlers.ended();
  assert.match(spoken.nodes.get('#conversation-focus-label').textContent,/Reply to Conversation · Scope/);
  assert.equal(spoken.nodes.get('#conversation-audio-state').textContent,'Question finished. Press Talk to answer or change topic.');
  assert.equal(spoken.originals.size,0);assert.equal(spoken.nativePosts.length,0);
  const draftAnswer=spoken.nodes.get('#conversation-message');draftAnswer.value='My original answer';
  await spoken.nodes.get('#conversation-compose').handlers.submit({preventDefault(){}});
  assert.deepEqual(JSON.parse(JSON.stringify([...spoken.originals.values()][0].reply)),{source:'request:native-request',revision:'native-revision',context_only:true});
  spoken.ui.stop();

  const interrupted=boot();interrupted.nativeRequest();interrupted.audio();await interrupted.ui.refresh();
  const iq=interrupted.nodes.get('#conversation-request-cards').children[0];
  await findButton(iq,'Read request').handlers.click();await findButton(iq,'Listen to question').handlers.click();
  const ip=interrupted.nodes.get('#conversation-audio');ip.ended=true;
  const heldEnd=interrupted.holdPart(),end=ip.handlers.ended();await heldEnd.entered;
  interrupted.ui.stop();heldEnd.release();await end;
  assert.doesNotMatch(interrupted.nodes.get('#conversation-focus-label')?.textContent || '',/Reply to/,'Stopped audio cannot replace human reply focus after a delayed completion check');

  const advanced=boot();advanced.nativeRequest();advanced.audio();await advanced.ui.refresh();
  const aq=advanced.nodes.get('#conversation-request-cards').children[0];
  await findButton(aq,'Read request').handlers.click();await findButton(aq,'Listen to question').handlers.click();
  const rejectEnd=advanced.hold(),ap=advanced.nodes.get('#conversation-audio');ap.ended=true;
  const staleEnd=ap.handlers.ended();rejectEnd(new Error('Question advanced'));await staleEnd;
  assert.doesNotMatch(advanced.nodes.get('#conversation-focus-label')?.textContent || '',/Reply to/);
  assert.equal(advanced.nodes.get('#conversation-audio-state').textContent,'This question changed. Refresh before answering.');

  const sending = boot(); await sending.ui.refresh();
  const draft=sending.nodes.get("#conversation-message"), form=sending.nodes.get("#conversation-compose");
  draft.value="  Original message.  ";
  sending.lose(); await form.handlers.submit({preventDefault(){}});
  assert.equal(draft.value,"  Original message.  ","Lost acknowledgment must preserve the draft");
  assert.equal(sending.originals.size,1);
  await form.handlers.submit({preventDefault(){}});
  assert.equal(sending.originals.size,1,"Unknown POST must not be automatically duplicated");
  const retained=sending.nodes.get("#conversation-inputs").children[0];
  await retained.children[2].children[0].handlers.click();
  assert.match(retained.children[1].textContent,/Sent to your conversation/);
  assert.equal(draft.value,"","Status recovery must prevent accidental same-text resubmission");
  const seedRecovery=fixture=>fixture.originals.set('saved-route',{id:'saved-route',text:'The original is still saved.',phase:'routed',binding:'fixture',sequence:1,
    routing:{instruction:'retained-route',phase:'retained_unsent',label:'Alpha'},recoveries:[{instruction:'retained-route',label:'Alpha'}]});
  const recovered=boot();seedRecovery(recovered);await recovered.ui.refresh();
  const retrySaved=findButton(recovered.nodes.get('#conversation-inputs'),'Try again with #Alpha');assert(retrySaved && !retrySaved.hidden);
  recovered.nodes.get('#conversation-message').value='Keep my new draft';
  await retrySaved.handlers.click();assert.equal(recovered.nodes.get('#conversation-message').value,'Keep my new draft');assert.equal(recovered.recoveryPosts.length,1);assert.equal(recovered.originals.size,1);assert(retrySaved.hidden);
  assert.equal(recovered.recoveryPosts[0].instruction,'retained-route');recovered.ui.stop();
  const lostRecovery=boot(false,false,{loseRecovery:true});seedRecovery(lostRecovery);await lostRecovery.ui.refresh();
  const lostRetry=findButton(lostRecovery.nodes.get('#conversation-inputs'),'Try again with #Alpha');await lostRetry.handlers.click();
  assert(lostRetry.disabled,'Unknown recovery response must require status before another action');
  await lostRetry.handlers.click();assert.equal(lostRecovery.recoveryPosts.length,1);
  await findButton(lostRecovery.nodes.get('#conversation-inputs'),'Check status / read message').handlers.click();assert(lostRetry.hidden);assert.equal(lostRecovery.originals.size,1);lostRecovery.ui.stop();
  const readonlyRecovery=boot(true);seedRecovery(readonlyRecovery);await readonlyRecovery.ui.refresh();
  const deniedRetry=findButton(readonlyRecovery.nodes.get('#conversation-inputs'),'Try again with #Alpha');assert(deniedRetry.hidden);await deniedRetry.handlers.click();assert.equal(readonlyRecovery.recoveryPosts.length,0);readonlyRecovery.ui.stop();

  const release=sending.holdPost(); draft.value="Next message";
  const pending=form.handlers.submit({preventDefault(){}});
  await Promise.resolve(); assert(sending.posting());
  draft.value="A correction typed while the POST is pending";
  release(); await pending;
  assert.equal(draft.value,"A correction typed while the POST is pending");
  assert.equal(sending.originals.size,2);
  const reconnect=boot(); await reconnect.ui.refresh();
  const reconnectDraft=reconnect.nodes.get("#conversation-message"), reconnectForm=reconnect.nodes.get("#conversation-compose");
  reconnectDraft.value="Not delivered while offline";reconnect.offline();
  await reconnectForm.handlers.submit({preventDefault(){}});
  assert.equal(reconnect.originals.size,0);
  const unknown=reconnect.nodes.get("#conversation-inputs").children[0];
  await unknown.children[2].children[1].handlers.click();
  assert.equal(reconnect.originals.size,1,"An explicit same-UUID retry must recover a never-delivered request");
  await unknown.children[2].children[1].handlers.click();
  assert.equal(reconnect.originals.size,1);
  const original=[...reconnect.originals.values()][0]; original.phase="queued";
  const hold=reconnect.holdInputs();const refreshing=reconnect.ui.refresh();await hold.entered;
  original.phase="accepted";original.retired=true;
  await unknown.children[2].children[0].handlers.click();hold.release();await refreshing;
  assert.match(unknown.children[1].textContent,/earlier conversation binding/,
    "A stale pending snapshot must not overwrite newer retirement/acceptance evidence");
  for(let n=0;n<30;n++) {
    const id=`original-${n}`;reconnect.originals.set(id,{id,text:`Message ${n}`,phase:"accepted",sequence:n+2});
  }
  await reconnect.ui.refresh();await reconnect.ui.refresh();
  assert.equal(reconnect.nodes.get("#conversation-inputs").children.length,31,"More than eight arrivals must not leave an inaccessible history gap");
  const races=boot(), raceOriginal={id:"race-original",text:"A retained request",sequence:1,phase:"queued"};
  races.originals.set(raceOriginal.id,raceOriginal);await races.ui.refresh();
  const raceCard=races.nodes.get("#conversation-inputs").children[0];
  const late=races.holdStatus();const checking=raceCard.children[2].children[0].handlers.click();await late.entered;
  await races.ui.refresh();raceOriginal.phase="accepted";raceOriginal.retired=true;late.release();await checking;
  assert.match(raceCard.children[1].textContent,/earlier conversation binding/,
    "Older-started requests must retain terminal evidence actually observed later");
  const failure=races.holdStatus(true);const failing=raceCard.children[2].children[0].handlers.click();await failure.entered;
  await races.ui.refresh();const confirmed=raceCard.children[1].textContent;failure.release();await failing;
  assert.equal(raceCard.children[1].textContent,confirmed,"Late errors cannot overwrite a newer confirmed status");
  const answered=boot(), answerOriginal={id:"answered-original",text:"A selected question answer",sequence:1,phase:"queued"};
  answered.originals.set(answerOriginal.id,answerOriginal);await answered.ui.refresh();
  const answeredCard=answered.nodes.get("#conversation-inputs").children[0];
  const oldAnswerStatus=answered.holdStatus();const answerChecking=answeredCard.children[2].children[0].handlers.click();await oldAnswerStatus.entered;
  answerOriginal.phase="answered";await answered.ui.refresh();answerOriginal.phase="preparing";oldAnswerStatus.release();await answerChecking;
  assert.match(answeredCard.children[1].textContent,/Answer recorded/,"A late preparation read cannot undo an already recorded question answer");
  const matrix=boot(), matrixOriginal={id:"matrix-original",text:"Directed external work",sequence:1,phase:"accepted"};
  matrix.originals.set(matrixOriginal.id,matrixOriginal);await matrix.ui.refresh();
  const matrixCard=matrix.nodes.get("#conversation-inputs").children[0];
  matrixOriginal.phase="matrix_requested";await matrix.ui.refresh();
  assert.match(matrixCard.children[1].textContent,/selected daimon/,"Accepted input can advance to Matrix selection");
  matrixOriginal.phase="accepted";await matrix.ui.refresh();
  assert.match(matrixCard.children[1].textContent,/selected daimon/,"A stale native ACK cannot undo Matrix selection");
  const oldMatrixStatus=matrix.holdStatus();const matrixChecking=matrixCard.children[2].children[0].handlers.click();await oldMatrixStatus.entered;
  await matrix.ui.refresh();oldMatrixStatus.release();await matrixChecking;
  assert.match(matrixCard.children[1].textContent,/selected daimon/,"Older-started accepted responses preserve Matrix selection");
  const integrated=boot();integrated.nativeRequest();await integrated.ui.refresh();
  const requestBox=integrated.nodes.get("#conversation-request-cards").children[0];
  await requestBox.children[3].handlers.click();
  const answer=requestBox.children[6].children[1];answer.value="Keep this draft during conversation refresh";answer.handlers.input();
  await requestBox.children[5].children[0].handlers.click();
  const retry=requestBox.children[8].children.find(item=>item.textContent==="Retry this same decision");assert(!retry.hidden);
  await integrated.ui.refresh();
  assert.equal(integrated.nodes.get("#conversation-request-cards").children[0],requestBox,
    "Refreshing input metadata must not reset native request cards under an unchanged binding");
  assert.equal(answer.value,"Keep this draft during conversation refresh");assert(!retry.hidden);
  await retry.handlers.click();
  assert.deepEqual(integrated.nativePosts[0],integrated.nativePosts[1],"Conversation refresh must preserve the frozen unresolved decision");
  const composerState=boot();await composerState.ui.refresh();composerState.receivingPhase("queued");
  composerState.nodes.get("#conversation-message").value="Newest composer message";
  await composerState.nodes.get("#conversation-compose").handlers.submit({preventDefault(){}});
  const sent=[...composerState.originals.values()][0];
  assert.match(composerState.nodes.get("#conversation-send-state").textContent,/waiting to send/);
  sent.phase="accepted";await composerState.ui.refresh();
  assert.match(composerState.nodes.get("#conversation-send-state").textContent,/Sent to your conversation/,"Composer progress follows later native acceptance");
  sent.phase="queued";await composerState.ui.refresh();
  assert.match(composerState.nodes.get("#conversation-send-state").textContent,/Sent to your conversation/,"Older source observations cannot regress acceptance");
  const composerLoss=boot();await composerLoss.ui.refresh();composerLoss.lose();const releaseLoss=composerLoss.holdPost();
  composerLoss.nodes.get("#conversation-message").value="Accepted before the POST acknowledgment was lost";
  const uncertainPost=composerLoss.nodes.get("#conversation-compose").handlers.submit({preventDefault(){}});
  assert(composerLoss.posting());await composerLoss.ui.refresh();releaseLoss();await uncertainPost;
  assert.match(composerLoss.nodes.get("#conversation-send-state").textContent,/Sent to your conversation/,"A lost POST response cannot erase acceptance already observed through history");
  const voiceHistory=boot();
  voiceHistory.originals.set("voice-original",{id:"voice-original",sequence:1,text:"",phase:"preparing",voice:{audio_sha256:"a"}});
  await voiceHistory.ui.refresh();
  voiceHistory.originals.set("voice-original",{id:"voice-original",sequence:1,preview:"A finalized automatic transcript",phase:"accepted",voice:{audio_sha256:"a",transcription:{capture_id:"voice-original"}}});
  await voiceHistory.ui.refresh();
  assert.equal(voiceHistory.nodes.get("#conversation-inputs").children[0].children[0].textContent,"Automatic voice transcript:\nA finalized automatic transcript","Preparing empty text cannot hide the later history transcript preview");
  const routed=boot();
  routed.originals.set("routing-original",{id:"routing-original",text:"#Unknown Work",sequence:1,phase:"awaiting_target"});
  await routed.ui.refresh();
  const routeCard=routed.nodes.get("#conversation-inputs").children[0],routeBox=routeCard.children[3];
  assert.equal(routeBox.hidden,false);assert.equal(routeBox.children[0].value,"");
  assert.equal(routeBox.children[1].disabled,true,"Ambiguous routing requires an explicit choice");
  routeBox.children[0].value="t7";routeBox.children[0].handlers.change();
  const holdRoute=routed.holdRoute(),routePost=routeBox.children[1].handlers.click();await holdRoute.entered;
  // This GET starts after the POST but samples awaiting_target before commit.
  const oldSnapshot=routed.holdInputs(),oldRefresh=routed.ui.refresh();await oldSnapshot.entered;
  holdRoute.release();await routePost;
  assert.match(routeCard.children[1].textContent,/#Alpha: sent to this session/);
  oldSnapshot.release();await oldRefresh;
  assert.match(routeCard.children[1].textContent,/#Alpha: sent to this session/,
    "A later-started GET with null routing cannot erase a committed route");
  assert.equal(routeBox.hidden,true,"A committed selection must not reopen ambiguity");
  routeCard.children[2].children[2].handlers.click();
  routed.nodes.get("#conversation-message").value="Amend the selected work";
  await routed.nodes.get("#conversation-compose").handlers.submit({preventDefault(){}});
  const amendment=[...routed.originals.values()].at(-1);
  assert.deepEqual(JSON.parse(JSON.stringify(amendment.reply)),{source:"input:routing-original",revision:"a".repeat(64)},"Reply must carry the server source identity, never client target IDs");
  assert.equal(routed.nodes.get("#conversation-focus").hidden,true,"Accepted retention consumes one-message focus");
  const earlierPost=boot();earlierPost.routePhase("queued");
  earlierPost.originals.set("early-route",{id:"early-route",text:"#Unknown Held route",sequence:1,phase:"awaiting_target"});
  await earlierPost.ui.refresh();
  const earlyCard=earlierPost.nodes.get("#conversation-inputs").children[0],earlyBox=earlyCard.children[3];
  earlyBox.children[0].value="t7";earlyBox.children[0].handlers.change();
  const heldQueued=earlierPost.holdRoute(),queuedPost=earlyBox.children[1].handlers.click();await heldQueued.entered;
  await earlierPost.ui.refresh(); // newer observation arrives first, before route commit
  heldQueued.release();await queuedPost;
  assert.match(earlyCard.children[1].textContent,/#Alpha: saved; waiting for this session/);
  assert.equal(earlyBox.hidden,true,"An older-started queued route response still proves immutable destination selection");
  assert.equal(earlyCard.children[2].children[2].hidden,false,"The retained route also carries its reply source");
  const monitor=boot();monitor.originals.set('monitor-original',{id:'monitor-original',text:'Correct Alpha',sequence:1,phase:'accepted'});
  await monitor.ui.refresh();
  const monitorCard=monitor.nodes.get('#conversation-inputs').children[0],trace=monitorCard.children[6];
  trace.open=true;await trace.children[3].handlers.click();
  assert.equal(trace.children[2].children[0].children[1].textContent,'Correct Alpha');
  const traceStep=trace.children[2].children[1];
  assert.match(traceStep.children[0].textContent,/#Alpha · Correction/);
  assert.equal(traceStep.children[3].textContent,'Exact correction <script>literal</script>');
  assert.equal(monitor.requests.filter(r=>r.trace).length,1);
  assert.equal(monitor.requests.find(r=>r.trace).body,undefined,'Trace reads cannot submit or retry an input');
  assert.equal(monitor.originals.size,1);
  const paginated=boot();paginated.originals.set('long-trace',{id:'long-trace',text:'Complete immutable original',longTrace:true,sequence:1,phase:'accepted'});
  await paginated.ui.refresh();const longPath=paginated.nodes.get('#conversation-inputs').children[0].children[6];
  longPath.open=true;await longPath.children[3].handlers.click();
  const longOriginal=longPath.children[2].children[0];assert.equal(longOriginal.children[1].textContent,'Co');
  await longOriginal.children[2].handlers.click();assert.equal(longOriginal.children[1].textContent,'Complete immutable original');
  await paginated.ui.refresh();assert.equal(longPath.children[2].children[0].children[1].textContent,'Complete immutable original','Read-only refresh preserves expanded immutable text');
  const mixed=boot();
  mixed.originals.set('app-order',{id:'app-order',text:'App between Telegram captures',phase:'accepted',sequence:1,capture_order:2});
  for(let n=1;n<=10;n++)mixed.telegramOriginals.set(`telegram:${n}`,{id:`telegram:${n}`,text:`Telegram original ${n}`,preview:`Telegram original ${n}`,phase:'completed',sequence:n,capture_order:n===1?1:n+1,voice:n===2?{phase:'committed'}:null,origin_kind:'telegram',read_only:true});
  mixed.nodes.get('#conversation-sent').open=true;await mixed.ui.refresh();
  assert.equal(mixed.nodes.get('#conversation-inputs').children.length,9);
  await mixed.nodes.get('#conversation-telegram-older').handlers.click();
  const mixedCards=mixed.nodes.get('#conversation-inputs').children;
  assert.equal(mixedCards.length,11);
  assert.deepEqual(mixedCards.map(card=>Number(card.dataset.sequence)),[11,10,9,8,7,6,5,4,3,2,1]);
  const telegramCard=mixedCards.find(card=>card.children[0].textContent.includes('Telegram original 2'));
  assert.match(telegramCard.children[0].textContent,/Automatic voice transcript/);
  assert.equal(telegramCard.children[2].children[0].hidden,true,'Telegram originals expose no UUID status action');
  assert.equal(telegramCard.children[3].hidden,true,'Inspection cannot choose another destination for a Telegram original');
  const telegramPath=telegramCard.children[6];telegramPath.open=true;await telegramPath.children[3].handlers.click();
  assert.equal(telegramPath.children[2].children[0].children[1].textContent,'Telegram original 2');
  mixed.telegramOriginals.set('telegram:11',{id:'telegram:11',text:'New Telegram ingress',phase:'completed',sequence:11,capture_order:12,origin_kind:'telegram',read_only:true});
  await mixed.ui.refresh();assert.equal(mixed.nodes.get('#conversation-inputs').children[0].children[0].textContent,'New Telegram ingress');
  assert(mixed.requests.filter(r=>r.telegram||r.trace).every(r=>r.body===undefined),'All observation requests remain GET only');
  const fileOptions={fileAccess:true},fileClient=boot(false,false,fileOptions);
  fileClient.overrideSource(8,{artifact_status:'running',artifacts:[]});
  await fileClient.ui.refresh();
  const fileCard=fileClient.nodes.get('#conversation-answers').children.find(card=>card.dataset.sequence==='8');
  assert.match(fileCard.children[5].children[1].textContent,/Preparing files/);
  const file={id:'12345678-1234-5678-9876-123456789abc',name:'native-output.txt',sha256:require('node:crypto').createHash('sha256').update('Actual fixture file').digest('hex'),bytes:19,available:true};
  fileClient.overrideSource(8,{artifact_status:'done',artifacts:[file]});await fileClient.ui.refresh();
  await fileCard.children[5].children[1].children[0].handlers.click();
  assert.equal(fileCard.children[5].children[1].children[1].textContent,'Download ready');
  fileClient.overrideSource(8,{artifact_status:'partial',artifacts:[{...file,available:false}]});await fileClient.ui.refresh();
  assert.match(fileCard.children[5].children[1].textContent,/could not be prepared/);
  assert.equal(fileCard.children[5].children[2].children[0].disabled,true);
  fileOptions.fileAccess=false;await fileClient.ui.refresh();assert.equal(fileCard.children[5].hidden,true);
  fileOptions.fileAccess=true;fileClient.overrideSource(8,{artifact_status:'running',artifacts:[]});
  fileOptions.artifacts={items:[{...file,native_source:'native:8'}],jobs:[{native_source:'native:8',phase:'done'}]};
  await fileClient.ui.refresh();
  assert.equal(fileCard.children[5].children[1].children[0].disabled,false,'Late global file metadata must update an already-seen reply');
  assert.equal(fileCard.children[5].children[1].children[0].textContent,'↓ native-output.txt');
  {
    const f=boot(false,true),task={id:'a'.repeat(32),binding:'fixture',revision:'b'.repeat(64),state:'pending',kind:'task',title:'Observe the result',label:'#Alpha',body:'Tell us what you observed.',preview:'Tell us what you observed.',read_only:true};
    f.humanTasks.set(task.id,task);await f.ui.refresh();
    const card=f.nodes.get('#conversation-task-cards').children[0];
    await card.children.find(n=>n.textContent==='Read action').handlers.click();
    const reply=card.children.find(n=>n.textContent==='Reply in conversation');
    f.nodes.get('#conversation-message').value='Keep this draft';reply.handlers.click();
    assert.equal(f.nodes.get('#conversation-message').value,'Keep this draft');
    f.nodes.get('#conversation-message').value='';reply.handlers.click();
    f.nodes.get('#conversation-message').value='Here is what I observed';
    await f.nodes.get('#conversation-compose').handlers.submit({preventDefault(){}});
    const original=[...f.originals.values()].at(-1);
    assert.equal(original.text,'Here is what I observed');
    assert.equal(original.reply.source,'task:'+task.id);assert.equal(original.reply.revision,task.revision);assert.equal(original.reply.context_only,true);
  }
  const viewer=boot(true);
  viewer.originals.set('view-only',{id:'view-only',text:'#Missing Inspect only',sequence:1,phase:'awaiting_target',binding:'fixture'});
  await viewer.ui.refresh();
  const viewCard=viewer.nodes.get('#conversation-inputs').children[0];
  assert.equal(viewCard.children[3].hidden,true,'Read-only viewer cannot resolve a pending route');
  assert.equal(viewCard.children[2].children[0].hidden,true);
  assert.equal(viewer.nodes.get('#conversation-message').disabled,true);
  viewer.nodes.get('#conversation-message').value='Do not send';
  await viewer.nodes.get('#conversation-compose').handlers.submit({preventDefault(){}});
  assert.equal(viewer.originals.size,1);
  console.log("Conversation UI: paced/cancelable speech, question priority/expiry, TTS recovery, reconnect silence, history/routing, stale evidence, original-input recovery and native requests passed (zero model calls).");
})().catch(error => { console.error(error); process.exitCode = 1; });
