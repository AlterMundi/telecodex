// Exercise the shipped conversation state machine with synthetic DOM/network.
// No native sessions, external providers, or models are called.
const assert = require("node:assert/strict");
const fs = require("node:fs");
const vm = require("node:vm");

function boot(readOnly=false,standalone=false,options={}) {
  const optionsForFixture=options;const streamedTransports=[];
  const nodes = new Map(), requests = [], audioCalls=[], sourceOverrides=new Map();
  const copied=[],recoveryPosts=[],statusReads=[],mediaActions=new Map();const mediaSession={setActionHandler:(name,fn)=>mediaActions.set(name,fn)};
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
  let capture,intersection,pageFailure,continuousHooks,continuousMode,continuousSession;const continuousCalls=[];const replyTexts=new Map(),partReads=[],replyHolds=new Map();
  function element() {
    return {
      children: [], dataset: {}, handlers: {}, classList: {toggle() {}},
      textContent: "", value:"", paused: true,
      setAttribute(name, value) { this[name] = value; },
      removeAttribute(name) { delete this[name]; },
      addEventListener(name, fn) { this.handlers[name] = fn; },
      append(...children) { this.appendCalls=(this.appendCalls||0)+1;for(const child of children){if(child.parentNode&&child.parentNode!==this)child.remove();child.parentNode=this;const at=this.children.indexOf(child);if(at>=0)this.children.splice(at,1);this.children.push(child);} },
      replaceChildren(...children) { this.children = children; },
      insertBefore(child, before) {
        if(child.parentNode&&child.parentNode!==this)child.remove();child.parentNode=this;
        const previous = this.children.indexOf(child);
        if (previous >= 0) this.children.splice(previous,1);
        const at = this.children.indexOf(before);
        this.children.splice(at < 0 ? this.children.length : at, 0, child);
      },
      requestSubmit() {return this.handlers.submit?.({preventDefault(){}});},
      click() {this.clicked=true;},remove() {this.removed=true;if(this.parentNode){const at=this.parentNode.children.indexOf(this);if(at>=0)this.parentNode.children.splice(at,1);this.parentNode=undefined;}},
      before(...nodes) {this.beforeNodes=nodes;}, pause() { this.paused = true; }, play() {this.paused=false;this.ended=false;return Promise.resolve();}, load() {},
    };
  }
  const document = {
    documentElement:{lang:options.language||"en"}, hidden: false, body: element(), addEventListener(name,fn) {subscribe(documentEvents,name,fn);}, createElement: element,
    querySelector(selector) {
      if (!nodes.has(selector)) nodes.set(selector, element());
      return nodes.get(selector);
    },
  };
  let presentationProfile="simple";
  let count = 8, arriveDuringRecent = false, heldValidation, validationPending=false, renderAudio=false, multipart=false,audioFailures=0;
  const source = n => ({id:`native:${n}`, kind:standalone&&n===1?"integrator_progress":"integrator_answer", sequence:n, observed_at:new Date(1704067200000+n*1000).toISOString(),
    revision:`revision-${n}`, text_sha256:`sha-${n}`, preview:`Reply ${n}`,...sourceOverrides.get(n)});
  const originals = new Map(), telegramOriginals = new Map(), nativeRequests = new Map(), humanTasks=new Map(), nativePosts = [],taskPosts=[];
  let losePost = false, offlinePost = false, heldPost, heldInputs, heldRequests, heldStatus, heldRoute, routePhase="accepted", submitPhase = "accepted";
  const api = async (route, body) => {
    const url = new URL(route, "https://fixture.invalid/");
    if(pageFailure && url.pathname==="/interaction"){const name=pageFailure;pageFailure=false;throw Object.assign(Error("Synthetic network suspension"),{name});}
    if(url.pathname==='/interaction/telegram-inputs'){
      if(options.telegramDenied)throw Object.assign(Error('Telegram observation denied'),{status:403});
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
    if (url.pathname.startsWith("/interaction/tasks/")) {
      const task=humanTasks.get(url.pathname.split("/").at(-1));
      if(body){taskPosts.push({...body});task.state='cancelled';task.can_dismiss=false;return {id:task.id,binding:'fixture',revision:body.revision,dismissed:true};}
      return {...task,offset:0,next_offset:null};
    }
    if (url.pathname === "/interaction/requests") {
      if(options.requestFailure)throw new TypeError("Synthetic question registry outage");
      if(heldRequests){const held=heldRequests;heldRequests=undefined;held.started();await held.promise;}
      return {binding:"fixture",items:options.omitRequests?[]:[...nativeRequests.values()],cursor:nativeRequests.size,more:false};
    }
    if (url.pathname.startsWith("/interaction/requests/")) {
      if(options.questionDetailFailure)throw Object.assign(Error("Request no longer available"),{status:404});
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
      statusReads.push(url.pathname);
      if (heldStatus) {const held=heldStatus;heldStatus=undefined;held.started();await held.promise;if(held.fail)throw new TypeError("Delayed status failure");}
      return originals.get(url.pathname.split("/").at(-1));
    }
    if (url.pathname === "/interaction/inputs") {
      const before=Number(url.searchParams.get("before")), direction=url.searchParams.get("direction");
      const rows=[...originals.values()].filter(v=>direction==="after"?v.sequence>before:!before||v.sequence<before);
      rows.sort((a,b)=>direction==="after"?a.sequence-b.sequence:b.sequence-a.sequence);
      const selected=rows.slice(0,8);
      const snapshot={binding:"fixture",csrf:"synthetic",routing_choices:[{selector:"t7",label:"Alpha"},{selector:"t8",label:"Beta"}],items:selected.map(v=>({...v})),cursor:selected.at(-1)?.sequence||0,newest:Math.max(0,...selected.map(v=>v.sequence)),more:rows.length>8};
      if (heldInputs && (!heldInputs.direction || heldInputs.direction===direction)) {const held=heldInputs;heldInputs=undefined;held.started();await held.promise;}
      return snapshot;
    }
    if (url.pathname.endsWith("/part")) {
      if (heldValidation) { const held=heldValidation;heldValidation=undefined;held.started = true;validationPending=true; return held.promise.finally(()=>{validationPending=false;}); }
      const sourceId=url.searchParams.get('source'),offset=Number(url.searchParams.get('offset'));
      partReads.push({source:sourceId,offset});
      if(sourceId.startsWith('live:')){
        const fields=sourceId.split(':'),end=Number(fields[4]),text=Array.from(options.liveText||options.responseDraft?.text||'').slice(0,end).join('');
        if(require('node:crypto').createHash('sha256').update(text).digest('hex')!==fields[5])throw Object.assign(Error('Changed live prefix'),{status:409});
        return {text:Array.from(text).slice(offset).join(''),offset,next_offset:null};
      }
      if(replyHolds.has(sourceId)){const hold=replyHolds.get(sourceId);replyHolds.delete(sourceId);hold.started();await hold.promise;}
      if(replyTexts.has(sourceId)){const text=Array.from(replyTexts.get(sourceId)),end=Math.min(offset+1600,text.length);return {text:text.slice(offset,end).join(''),next_offset:end<text.length?end:null};}
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
    return {binding:"fixture", csrf:"synthetic",can_converse:options.continuous===true,can_converse_continuous:options.continuous===true, response_draft:options.responseDraft, can_listen:true,can_reply_updates:true,can_speak_live_response:options.liveSpeech===true,can_report_playback:options.playback===true,can_report_word_position:options.wordTiming===true,can_stream_speech:options.progressive===true,can_download_files:options.fileAccess===true,artifacts:options.artifacts,presentation_profile:presentationProfile,
      items:selected.map(source), cursor:{delivery:0,native:selected.at(-1) || cursor},
      newest:{delivery:0,native:Math.max(0,...selected)}, more:numbers.length > 8};
  };
  const context = vm.createContext({Date:optionsForFixture.confirmNow===undefined?Date:class extends Date{static now(){return optionsForFixture.confirmNow;}},window:{navigator:{mediaSession,clipboard:options.copyRich?{write:async items=>{copied.push(await (await items[0].data['text/plain']).text());}}:{writeText:async text=>{if(options.copyDenied)throw Error("Clipboard permission denied");copied.push(text);}}},ClipboardItem:options.copyRich?class{constructor(data){this.data=data;}}:undefined,localStorage:options.followStorage?{getItem:key=>options.followStorage.get(key)??null,setItem:(key,value)=>options.followStorage.set(key,value)}:undefined,addEventListener(name,fn) {subscribe(windowEvents,name,fn);}}, document,
    location:{href:"https://fixture.invalid/?view=conversation"}, history:{replaceState() {}},
    URL, URLSearchParams, AbortController, setTimeout:timeout, clearTimeout:cancelTimeout, TextEncoder, Blob, crypto:options.immediateHash?{...require("node:crypto"),subtle:{digest:(_algorithm,bytes)=>Promise.resolve(require("node:crypto").createHash('sha256').update(new Uint8Array(bytes)).digest())}}:options.hashHold?{...require("node:crypto"),subtle:{digest:(algorithm,bytes)=>options.hashHold.active?new Promise(resolve=>options.hashHold.releases.push(()=>resolve(require("node:crypto").createHash('sha256').update(bytes).digest()))):require("node:crypto").subtle.digest(algorithm,bytes)}}:require("node:crypto"),
    fetch:(_route, options) => {
      if(_route.startsWith('api/interaction/artifacts/'))return Promise.resolve(new Response('Actual fixture file',{headers:{'X-Artifact-Sha256':require('node:crypto').createHash('sha256').update('Actual fixture file').digest('hex')}}));
      audioCalls.push(JSON.parse(options.body));
      if(audioFailures){audioFailures--;return Promise.resolve(new Response(JSON.stringify({detail:'Synthetic TTS outage'}),{status:503}));}
      if(renderAudio && JSON.parse(options.body).source.startsWith('live:')){
        const selected=JSON.parse(options.body),end=Number(selected.source.split(':')[4]),text=Array.from(optionsForFixture.liveText||optionsForFixture.responseDraft.text).slice(selected.offset,end).join('');
        const data=Buffer.from('synthetic live audio'),hash=require('node:crypto').createHash('sha256');
        return Promise.resolve(new Response(data,{headers:{...(optionsForFixture.progressive?{'content-type':'application/x-ndjson','x-audio-encoding':'mp3'}:{}),'x-presentation-source':selected.source,'x-presentation-revision':selected.revision,'x-presentation-offset':String(selected.offset),'x-presentation-next-offset':'','x-presentation-end-offset':String(end),'x-presentation-text-sha256':hash.update(text).digest('hex'),'x-presentation-prefetch':'true','x-audio-sha256':require('node:crypto').createHash('sha256').update(data).digest('hex')}}));
      }
      if(renderAudio){
        const selected=JSON.parse(options.body);if(optionsForFixture.failOffset===selected.offset)return Promise.resolve(new Response(JSON.stringify({detail:'Synthetic future preparation outage'}),{status:503}));const data=Buffer.from('synthetic audio'),hash=require('node:crypto').createHash('sha256').update(data).digest('hex');
        if(optionsForFixture.incremental){
          const chars=Array.from(replyTexts.get(selected.source)||'Selected reply'),end=Math.min(chars.length,selected.offset+(selected.offset===0?120:300));
          const text=chars.slice(selected.offset,end).join(''),sha=require('node:crypto').createHash('sha256').update(text).digest('hex');
          return Promise.resolve(new Response(data,{headers:{...(optionsForFixture.progressive?{'content-type':'application/x-ndjson','x-audio-encoding':'mp3'}:{}),'x-presentation-source':selected.source,'x-presentation-revision':selected.revision,'x-presentation-offset':String(selected.offset),'x-presentation-next-offset':end<chars.length?String(end):'','x-presentation-end-offset':String(end),'x-presentation-text-sha256':sha,'x-presentation-prefetch':'true','x-audio-sha256':hash}}));
        }
        return Promise.resolve(new Response(data,{headers:{'x-presentation-source':selected.source,'x-presentation-revision':selected.revision,'x-presentation-offset':String(selected.offset),'x-presentation-next-offset':multipart&&selected.offset===0?'1600':'','x-audio-sha256':hash}}));
      }
      return new Promise((_resolve, reject) => {
      options.signal.addEventListener("abort", () => reject(new Error("Aborted")), {once:true});
    });},
  });
  vm.runInContext(fs.readFileSync("dashboard/web/requests.js", "utf8"), context);
  vm.runInContext(fs.readFileSync("dashboard/web/microphone.js", "utf8"), context);
  if(options.capture)context.window.VoiceInput={mount(value){capture=value;return {configure(){},reset(){},deactivate(){},observe(){},ownsSend:()=>!!options.ownsVoice,submit:()=>options.voiceSubmit?.(),canStartContinuous:()=>true,continuousOwner:v=>continuousCalls.push(['owner',v]),retainContinuous:v=>{continuousCalls.push(['retain',v]);return v;},commitContinuous:v=>{continuousCalls.push(['commit',v]);return v;},reviewContinuous:v=>continuousCalls.push(['review',v]),hold:()=>continuousCalls.push(['ordinary-hold']),startConversation:()=>continuousCalls.push(['ordinary-start'])};}};
  if(options.continuous){
    context.window.ContinuousConversation={create(hooks){continuousHooks=hooks;return continuousSession={active:false,floor:false,originals:[],start(){this.active=true;continuousCalls.push(['start']);hooks.onState({active:true,starting:false,floor:false,pending:false});},hold(){this.active=false;continuousCalls.push(['hold']);},release:id=>continuousCalls.push(['release',id])};}};
    if(options.continuousDriver){
      const driver=options.continuousDriver;
      context.window.crypto=context.crypto;
      context.window.navigator.mediaDevices={getUserMedia:async()=>({getTracks:()=>[{stop(){}}]})};
      const source={bytes:0,subscribe(fn){this.listener=fn;return ()=>{this.listener=undefined;};},observe(){},close(){},
        emit(n){this.bytes+=n;this.listener?.({pcm:new ArrayBuffer(n),totalBytes:this.bytes});}};
      context.window.StreamingVoice={
        openActivity:async hooks=>{driver.activity=hooks.onActivity;return {feed:()=>true,boundary:()=>true,close(){}};},
        openCapture:async()=>{driver.source=source;return source;},
        create:async()=>({state:'inactive',addEventListener(){},start(){this.state='recording';driver.ready=true;},stop(){this.state='inactive';}})
      };
      vm.runInContext(fs.readFileSync('dashboard/web/voice-session.js','utf8'),context);
      const create=context.window.ContinuousConversation.create;
      context.window.ContinuousConversation={create(hooks){continuousHooks=hooks;return continuousSession=create(hooks);}};
    }
    context.window.ConversationVoiceMode={mount(hooks){let enabled=false,direct=false;return continuousMode={active:false,updates:[],configure(value,next){if(this.active&&(!value||direct!==next))this.close();enabled=value;direct=next;},update:v=>continuousMode.updates.push(v),open(){if(!enabled||!hooks.canEnter())return;hooks.onEnter();this.active=true;hooks.onSpeak();},suspend(){if(this.active)hooks.onSuspend();},close(){if(!this.active)return;this.active=false;hooks.onLeave();}};}};
  }
  if(options.progressive)context.window.SpeechStream={available:()=>true,open(_response,signal){
    const value={url:'blob:fixture-stream',signal,done:options.streamDone || Promise.resolve({complete:true,blob:new Blob(['verified encoded sample'],{type:'audio/mpeg'})}),onDecoderError(fn){this.decoderError=fn;},position:seconds=>options.wordPosition?.(seconds,Number(_response.headers.get("x-presentation-offset"))),cancelled:false,cancel(){this.cancelled=true;},onError(){}};streamedTransports.push(value);return value;
  }};
  if(options.liveSpeech)vm.runInContext(fs.readFileSync("dashboard/web/live-speech.js", "utf8"), context);
  if(options.timingEvents)context.window.ConversationTiming={mount:()=>Object.fromEntries(["bind","mark","input","source","pauseApplied"].map(name=>[name,(...args)=>options.timingEvents.push([name,...args])]))};
  vm.runInContext(fs.readFileSync("dashboard/web/conversation.js", "utf8"), context);
  if(options.autoText)context.window.IntersectionObserver=class{
    constructor(callback){intersection=callback;}observe(){}unobserve(){}disconnect(){}
  };
  document.querySelector("#conversation-message").rows=1;
  const ui = context.window.Conversation.mount({api,readOnly,standalone,headers:() => ({}),setCsrf() {},modeChanged() {}});
  if(standalone){
    assert.equal(document.querySelector("#conversation-visual summary").textContent,"","Mount must preserve the authored paperclip, not overwrite it with text");
    assert.equal(document.querySelector("#conversation-message").rows,1);
    assert(document.querySelector("#conversation-presence").children.some(track=>track.children?.includes(document.querySelector("#conversation-companion .companion-caption"))));
    assert(document.querySelector("header .header-tools").children.includes(document.querySelector("#conversation-sound-toggle")));
    assert(document.querySelector("#conversation-sound").children.includes(document.querySelector("#conversation-sound-controls")));
  }
  ui.authenticated();
  return {ui, copied,statusReads, nodes, requests, originals,replyTexts,partReads,humanTasks,streamedTransports,
    continuous:()=>({hooks:continuousHooks,mode:continuousMode,session:continuousSession,calls:continuousCalls}),microphoneState:(value,detail)=>capture.onActivity(value,detail),
    rangeGeometry(fn){context.document.createRange=fn;},
    reducedMotion(value){context.window.matchMedia=()=>({matches:value});},
    holdReplyText(id){let release,started;const entered=new Promise(yes=>{started=yes;}),promise=new Promise(yes=>{release=yes;});replyHolds.set(id,{promise,started});return {entered,release};},
    intersect(id,visible=true){const target=nodes.get('#conversation-timeline').children.find(c=>c.dataset.sourceId===id);assert(target,JSON.stringify({id,keys:nodes.get("#conversation-timeline").children.map(c=>c.dataset),state:nodes.get("#conversation-state")?.textContent}));intersection([{target,isIntersecting:visible}]);}, telegramOriginals, nativePosts,taskPosts,audioCalls,clock,recoveryPosts,
    profile(value){presentationProfile=value;},
    mediaSession,mediaActions,event(name,value){for(const fn of windowEvents.get(name)||[])fn(value);},
    hide(){document.hidden=true;for(const fn of documentEvents.get('visibilitychange')||[])fn();},
    show(){document.hidden=false;for(const fn of documentEvents.get('visibilitychange')||[])fn();},
    playbackContext(){return capture.getPlayback();},
    capture(){capture.pauseAudio();capture.onActivity(true);},
    finishCapture(){capture.onActivity(false);},
    capturedInput(value){capture.onInput(value);},
    timingCapture(id,stage){capture.onTiming(id,stage);},
    failPage(name="TypeError"){pageFailure=name;},failAudio(n=1){audioFailures=n;},
    closeNativeRequest(){Object.assign(nativeRequests.get('native-request'),{state:'answered',actionable:false});},
    overrideSource(n,value){sourceOverrides.set(n,value);},addReplies(n){count+=n;},audio(parts=false){renderAudio=true;multipart=parts;},
    nativeRequest(audioTrack="conversation",target={chat_id:100,thread_id:1}) {
      nativeRequests.set("native-request",{id:"native-request",binding:"fixture",revision:"native-revision",audio_track:audioTrack,target,state:"pending",actionable:true,kind:"question",title:"Scope",label:"Conversation",body:"Choose scope",choices:[{label:"Selected scope",action:{kind:"option",index:0}}],allows_text:true,can_cancel:true,question_index:0,question_total:1,recorded_revisions:[]});
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
    holdRequests() {
      let release,started;
      const entered=new Promise(yes=>{started=yes;}),promise=new Promise(yes=>{release=yes;});
      heldRequests={promise,started};return {entered,release};
    },
    holdInputs(direction) {
      let release, started;
      const entered=new Promise(yes=>{started=yes;}), promise=new Promise(yes=>{release=yes;});
      heldInputs={promise,started,direction};return {entered,release};
    },
    posting() {return !!heldPost?.started;},
    holdPost() {let release; const promise = new Promise(yes=>{release=yes;}); heldPost={promise}; return release;},
    arrive() { count = 58; arriveDuringRecent = true; },
    validating() { return validationPending; },
    hold() {
      let reject;
      const promise = new Promise((_, no) => { reject = no; });
      heldValidation = {promise}; return reject;
    },
  };
}

(async () => {
  const liveOptions={responseDraft:{provisional:true,text:"Useful early answer.",observed_at:"2100-01-01T00:00:00Z"}};
  const earlyDraft=boot(false,true,liveOptions);await earlyDraft.ui.refresh();
  const draftCard=earlyDraft.nodes.get('#conversation-timeline').children.find(c=>c.id==='conversation-response-draft');
  assert(draftCard && !draftCard.hidden);
  assert.equal(draftCard.children[1].textContent,"Useful early answer.");
  assert.equal(draftCard.children.length,2,"No Listen, reply or action controls on a provisional draft");
  assert.equal(earlyDraft.audioCalls.length,0);
  const draftTimeline=earlyDraft.nodes.get('#conversation-timeline');
  assert.equal(draftCard.dataset.timelineKey,"response-draft");
  assert.equal(draftTimeline.children.at(-1),draftCard,"Older canonical history stays before the draft");
  draftTimeline.scrollHeight=5000;draftTimeline.clientHeight=1000;draftTimeline.scrollTop=4000;
  liveOptions.responseDraft.text+=" More useful text.";await earlyDraft.ui.refresh();
  assert.equal(draftTimeline.scrollTop,5000,"A reader at the bottom follows growing text");
  draftTimeline.scrollTop=1000;liveOptions.responseDraft.text+=" Further text.";await earlyDraft.ui.refresh();
  assert.equal(draftTimeline.scrollTop,1000,"Reading older text preserves the viewport");
  liveOptions.responseDraft=null;await earlyDraft.ui.refresh();assert(draftCard.hidden);
  liveOptions.responseDraft={provisional:true,text:"Next draft"};await earlyDraft.ui.refresh();assert(!draftCard.hidden);
  earlyDraft.failPage("TypeError");await earlyDraft.ui.refresh();assert(draftCard.hidden,"Connection failure hides unconfirmed live display");
  for(const [language,day] of [["en","mon"]]) {
    const localized=boot(false,true,{language});
    const stamp=new Date(2026,9,26,23,40).toISOString();
    localized.overrideSource(8,{observed_at:stamp});
    await localized.ui.refresh();
    const message=localized.nodes.get('#conversation-timeline').children.find(c=>c.dataset.sourceId==='native:8');
    const time=message.children.find(c=>c.className==='message-time');
    assert.equal(time.textContent,`${day} 23:40`);
    assert.equal(time.dateTime,stamp);
    localized.overrideSource(8,{observed_at:'unavailable'});
    await localized.ui.refresh();assert.equal(time.hidden,true,'Unknown dates do not invent timestamps');
    localized.ui.stop();
  }
  const quoted=boot(false,true);await quoted.ui.refresh();
  const quotedCard=quoted.nodes.get('#conversation-timeline').children.find(card=>card.dataset.sourceId==='native:8');
  const replyHere=quotedCard.children[3].children.find(button=>button.textContent==='Reply to this message');
  assert(replyHere&&!replyHere.hidden);replyHere.handlers.click();
  quoted.nodes.get('#conversation-message').value='About that earlier answer';
  await quoted.nodes.get('#conversation-compose').handlers.submit({preventDefault(){}});
  const referenceOriginal=[...quoted.originals.values()].at(-1);
  assert.deepEqual(JSON.parse(JSON.stringify(referenceOriginal.reply)),{source:'native:8',revision:'revision-8',context_only:true});
  assert.equal(referenceOriginal.text,'About that earlier answer','Message replies carry a compact reference rather than copying the earlier answer');
  assert.equal(quoted.nativePosts.length,0,'An integrator message reference does not become a worker destination');quoted.ui.stop();
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
  const localProgress=boot(false,true,{autoText:true});
  const completeLocalProgress='Queda establecido: seguimos con el trabajo. '.repeat(80)+'La última palabra permanece completa.';
  localProgress.overrideSource(8,{kind:'integrator_progress',preview:completeLocalProgress.slice(0,240),text_characters:Array.from(completeLocalProgress).length});
  localProgress.replyTexts.set('native:8',completeLocalProgress);
  await localProgress.ui.refresh();localProgress.intersect('native:8');await new Promise(setImmediate);
  const localProgressCard=localProgress.nodes.get('#conversation-timeline').children.find(c=>c.dataset.sourceId==='native:8');
  assert.equal(localProgressCard.children[2].textContent,completeLocalProgress,
               'Conversation progress must expand beyond its preview without a Read reply click');
  assert.equal(localProgressCard.children[3].children[0].hidden,true);
  const localReads=localProgress.partReads.length;assert(localReads>1);
  await localProgress.ui.refresh();localProgress.intersect('native:8');await new Promise(setImmediate);
  assert.equal(localProgress.partReads.length,localReads,'Completed local progress does not fetch its text again');
  localProgress.ui.stop();
  const progressText=boot(false,true,{autoText:true});
  const completeProgress='This update must remain complete on phones as well as TVs. '.repeat(60);
  progressText.overrideSource(8,{kind:'worker_progress',target:{chat_id:100,thread_id:7},label:'Realtime',preview:completeProgress.slice(0,160),text_characters:completeProgress.length});
  progressText.replyTexts.set('native:8',completeProgress);
  await progressText.ui.refresh();progressText.intersect('native:8');await new Promise(setImmediate);
  const visibleProgress=progressText.nodes.get('#conversation-timeline').children.find(c=>c.dataset.sourceId==='native:8');
  assert.equal(visibleProgress.children[1].textContent,completeProgress,'visible Update from cards must load full canonical text, without opening the inspector');
  progressText.ui.stop();
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
  const replyUpdate=compact[0].children.find(button=>button.textContent==='Reply to this update');
  assert(replyUpdate&&!replyUpdate.hidden);replyUpdate.handlers.click();
  workers.addReplies(1);workers.overrideSource(9,{kind:'worker_progress',label:'Alpha',target:{chat_id:100,thread_id:7},preview:'A later update while the human is replying'});
  await workers.ui.refresh();
  workers.nodes.get('#conversation-message').value='About this exact progress update';
  await workers.nodes.get('#conversation-compose').handlers.submit({preventDefault(){}});
  const progressReply=[...workers.originals.values()].at(-1);
  assert.deepEqual(JSON.parse(JSON.stringify(progressReply.reply)),{source:'native:8',revision:'revision-8'});
  assert.equal(progressReply.text,'About this exact progress update');

  workers.addReplies(1);workers.overrideSource(10,{kind:"worker_result",label:"Alpha",target:{chat_id:100,thread_id:7},preview:"The useful final answer"});
  await workers.ui.refresh();
  assert(workerTimeline.children.some(c=>c.dataset.timelineKey==='native:10'),'the only useful final answer must stay in the main chat');
  assert.equal(compact[0].removed,true,'a returned answer settles an earlier progress preview');
  const anchoredProgress=workerTimeline.children.find(c=>c.dataset.timelineKey==='input:'+progressReply.id);
  const exactProgress=workerTimeline.children[workerTimeline.children.indexOf(anchoredProgress)-1];
  assert.equal(exactProgress.dataset.sourceId,'native:8','A reply stays below its exact older update after coalescing and a final return');
  assert.equal(exactProgress.dataset.sourceRevision,'revision-8');
  assert.match(anchoredProgress.children[0].textContent,/reply to Alpha/);
  const threaded=boot(false,true);threaded.overrideSource(3,{kind:'worker_result',label:'Alpha',target:{chat_id:100,thread_id:7}});
  threaded.originals.set('reply-later',{id:'reply-later',text:'Second answer to Alpha',reply:{source:'native:3',revision:'revision-3'},sequence:2,phase:'accepted',received_at:'2024-01-01T00:00:12Z'});
  threaded.originals.set('reply-earlier',{id:'reply-earlier',text:'First answer to Alpha',reply:{source:'native:3',revision:'revision-3'},sequence:1,phase:'accepted',received_at:'2024-01-01T00:00:10Z'});
  await threaded.ui.refresh();const threadLane=threaded.nodes.get('#conversation-timeline');
  const order=()=>threadLane.children.map(c=>c.dataset.timelineKey);
  const target=order().indexOf('native:3');assert.deepEqual(order().slice(target,target+4),['native:3','input:reply-earlier','input:reply-later','native:4']);
  const retainedThreadReply=threadLane.children[target+1];assert.equal(retainedThreadReply.dataset.timelineTime,String(Date.parse('2024-01-01T00:00:10Z')),'Grouping never rewrites capture time');
  await threaded.ui.refresh();assert.equal(threadLane.children[target+1],retainedThreadReply,'Polling preserves the same reply node and adjacency');
  threaded.overrideSource(3,{revision:'new-revision-3'});await threaded.ui.refresh();
  assert.equal(order().at(-2),'input:reply-earlier','A different source revision is not silently used as the reply parent');
  threaded.ui.stop();
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
  const directOptions={capture:true,continuous:true,playback:true,immediateHash:true};
  const direct=boot(false,true,directOptions);direct.audio();await direct.ui.refresh();
  const bridge=direct.continuous();direct.nodes.get('#conversation-message').value='Keep this ordinary draft';bridge.mode.open();
  assert(bridge.session.active);assert(bridge.calls.some(v=>v[0]==='owner'&&v[1]===true));assert(!bridge.calls.some(v=>v[0]==='ordinary-start'));
  direct.microphoneState(true,{phase:'idle',uploading:true,pending:false});
  direct.addReplies(1);await direct.ui.refresh();const directPlayer=direct.nodes.get('#conversation-audio');await until(()=>!directPlayer.paused);
  assert(direct.audioCalls.length>0,'Armed continuous capture and an old upload cannot block ordinary Conversation speech');
  bridge.hooks.onInterrupt();bridge.session.floor=true;bridge.hooks.onState({active:true,starting:false,floor:true,pending:false});
  assert(directPlayer.paused,'Actual playback pauses on the confirmed speech-onset callback');assert.equal(direct.nativePosts.length,0,'Barge-in cannot cancel or answer native work');
  const bridgeOriginal={id:'source-bound-original'};assert.equal(bridge.hooks.retain(bridgeOriginal),bridgeOriginal);assert.equal(bridge.hooks.commit(bridgeOriginal),bridgeOriginal);
  const interruptedCalls=direct.audioCalls.length;
  direct.overrideSource(10,{kind:'worker_result',target:{chat_id:100,thread_id:7},label:'Alpha',preview:'Internal report ready'});
  direct.overrideSource(11,{preview:'Alpha reports that the plan is ready. Beta is still working.',native_returns:[{id:'exact-alpha-return',revision:'current-alpha-revision'}]});
  direct.addReplies(2);await direct.ui.refresh();
  assert.equal(direct.audioCalls.length,interruptedCalls,'Late result speech waits while the human owns the floor');
  assert(directPlayer.paused);
  bridge.session.floor=false;bridge.hooks.onState({active:true,starting:false,floor:false,pending:false});
  await until(()=>direct.audioCalls.length===interruptedCalls+1&&!directPlayer.paused);
  assert.equal(direct.audioCalls.at(-1).source,'native:11','Conversation speaks the integrated result, with internal worker audio still muted');
  assert.equal(direct.nativePosts.length,0,'Result playback cannot cancel or answer native work');
  await direct.ui.refresh();
  assert.equal(direct.audioCalls.length,interruptedCalls+1,'Repeated result observation cannot repeat speech or resume the interrupted old answer');
  const floorDriver={},floorUI=boot(false,true,{capture:true,continuous:true,continuousDriver:floorDriver,immediateHash:true});
  floorUI.audio();await floorUI.ui.refresh();const floorBridge=floorUI.continuous();floorBridge.mode.open();await until(()=>floorDriver.ready);
  floorUI.addReplies(1);await floorUI.ui.refresh();const floorPlayer=floorUI.nodes.get('#conversation-audio');await until(()=>!floorPlayer.paused);
  floorDriver.source.emit(3200);floorDriver.activity({generation:1,onset:{generation:1,audio_bytes:3200},speaking:true});
  assert(floorPlayer.paused,'The real continuous coordinator interrupts the mounted player before captions');
  const beforeFloorRelease=floorUI.audioCalls.length;
  floorUI.overrideSource(10,{kind:'worker_result',target:{chat_id:100,thread_id:7},label:'Alpha',preview:'Internal report ready'});
  floorUI.addReplies(2);await floorUI.ui.refresh();assert.equal(floorUI.audioCalls.length,beforeFloorRelease);
  floorDriver.activity({generation:1,endpoint:{generation:1,audio_bytes:3200},speaking:false});
  await until(()=>floorUI.audioCalls.length>beforeFloorRelease&&!floorPlayer.paused);
  assert.equal(floorUI.audioCalls.at(-1).source,'native:11','A queued Conversation result can speak after the acoustic endpoint without waiting for an ASR caption');
  floorDriver.source.emit(3200);floorDriver.activity({generation:2,onset:{generation:2,audio_bytes:6400},speaking:true});
  assert(floorPlayer.paused,'Renewed speech reacquires the floor through the actual coordinator/player connection');
  assert.equal(floorUI.nativePosts.length,0);assert(!floorBridge.calls.some(v=>v[0]==='commit'),'Acoustic floor release cannot commit an original');
  floorBridge.mode.close();
  const hiddenDirect=boot(false,true,{capture:true,continuous:true,playback:true,immediateHash:true,clock:true});hiddenDirect.audio();await hiddenDirect.ui.refresh();
  const hiddenBridge=hiddenDirect.continuous();hiddenBridge.mode.open();hiddenDirect.addReplies(1);await hiddenDirect.ui.refresh();
  const hiddenPlayer=hiddenDirect.nodes.get('#conversation-audio');await until(()=>!hiddenPlayer.paused);
  hiddenBridge.mode.suspend();hiddenDirect.hide();
  assert(hiddenBridge.mode.active);assert(!hiddenBridge.session.active);assert(hiddenBridge.calls.some(v=>v[0]==='hold'));
  assert.equal(hiddenDirect.nodes.get('#conversation-voice-replies').dataset.voiceState,'on','Hidden capture cannot restore Voice off while the avatar reply is playing');
  assert.equal(hiddenPlayer.paused,false);assert(!hiddenBridge.calls.some(v=>v[0]==='owner'&&v[1]===false),'The Conversation-only mix stays owned until explicit exit');
  hiddenDirect.overrideSource(10,{kind:'worker_result',target:{chat_id:100,thread_id:7},label:'Alpha'});hiddenDirect.addReplies(2);await hiddenDirect.ui.refresh();
  hiddenPlayer.ended=true;await hiddenPlayer.handlers.ended();hiddenDirect.clock.fire(450);
  await until(()=>hiddenDirect.audioCalls.length===2&&!hiddenPlayer.paused);assert.equal(hiddenDirect.audioCalls.at(-1).source,'native:11','A later Conversation reply can play hidden while direct internal output stays muted');
  hiddenDirect.show();assert.equal(hiddenBridge.calls.filter(v=>v[0]==='start').length,1,'Returning cannot automatically reopen capture');
  hiddenBridge.mode.close();assert.equal(hiddenDirect.nodes.get('#conversation-voice-replies').dataset.voiceState,'off','Explicit exit still restores the previous Voice choice');hiddenDirect.ui.stop();
  directOptions.continuous=false;await direct.ui.refresh();assert(!bridge.mode.active);assert(bridge.calls.some(v=>v[0]==='hold'));
  assert(!bridge.calls.some(v=>v[0]==='ordinary-hold'),'A configuration change closes the previously selected continuous owner');
  assert.equal(direct.nodes.get('#conversation-message').value,'Keep this ordinary draft');direct.ui.stop();

  for(const pendingQuestion of [false,true]){
    const options={liveSpeech:true,immediateHash:true};
    const ready=boot(false,true,options);ready.addReplies(-8);ready.audio();await ready.ui.refresh();
    ready.nodes.get('#conversation-voice-replies').handlers.click();await ready.ui.refresh();
    await until(()=>!ready.nodes.get('#conversation-refresh').disabled);await new Promise(resolve=>setImmediate(resolve));
    assert.equal(ready.audioCalls.length,0);
    options.responseDraft={provisional:true,native_turn:'ready-physical',native_item:'ready-item',text:'Useful sentence. More',observed_at:'2100-01-01T00:00:00Z'};
    if(pendingQuestion)ready.nativeRequest();
    const questions=ready.holdRequests(),inputs=ready.holdInputs(),refresh=ready.ui.refresh();await questions.entered;
    await new Promise(resolve=>setImmediate(resolve));assert.equal(ready.audioCalls.length,0,'New speech waits for current question priority');
    questions.release();await inputs.entered;
    try{
      await until(()=>ready.audioCalls.length===1);
      assert(pendingQuestion?ready.audioCalls[0].source==='request:native-request':ready.audioCalls[0].source.startsWith('live:'),
        `Useful live speech must not wait for input history, and a current question retains priority: ${JSON.stringify({pendingQuestion,calls:ready.audioCalls})}`);
    }finally{inputs.release();await refresh;ready.ui.stop();}
  }
  {
    const options={liveSpeech:true,capture:true,immediateHash:true};
    const offline=boot(false,true,options);offline.addReplies(-8);offline.audio();await offline.ui.refresh();
    offline.nodes.get('#conversation-voice-replies').handlers.click();await offline.ui.refresh();
    await until(()=>!offline.nodes.get('#conversation-refresh').disabled);await new Promise(resolve=>setImmediate(resolve));
    options.responseDraft={provisional:true,native_turn:'offline-physical',native_item:'offline-item',text:'Wait for current questions. More',observed_at:'2100-01-01T00:00:00Z'};
    options.requestFailure=true;await offline.ui.refresh();offline.capture();offline.finishCapture();
    await new Promise(resolve=>setImmediate(resolve));assert.equal(offline.audioCalls.length,0,'Failed question observation cannot unlock new automatic speech');
    options.requestFailure=false;await offline.ui.refresh();await until(()=>offline.audioCalls.length===1);offline.ui.stop();
  }
  for(const finishHashWhileReading of [false,true]){
    const hold={active:true,releases:[]},options={liveSpeech:true,hashHold:hold,responseDraft:{provisional:true,native_turn:'priority-physical',native_item:'priority-item',text:'An early sentence. More',observed_at:'2100-01-01T00:00:00Z'}};
    const race=boot(false,true,options);race.addReplies(-8);race.audio();await race.ui.refresh();race.nodes.get('#conversation-voice-replies').handlers.click();
    await until(()=>hold.releases.length===1);race.nativeRequest();const questions=race.holdRequests(),refresh=race.ui.refresh();await questions.entered;
    if(!finishHashWhileReading){questions.release();await refresh;}
    hold.releases.shift()();await until(()=>hold.releases.length===1);hold.releases.shift()();hold.active=false;
    await new Promise(resolve=>setImmediate(resolve));
    if(finishHashWhileReading){assert.equal(race.audioCalls.length,0,'An in-flight hash cannot bypass the question observation gate');questions.release();await refresh;}
    await until(()=>race.audioCalls.length===1);assert.equal(race.audioCalls[0].source,'request:native-request','A newly observed question precedes an asynchronously selected prefix');race.ui.stop();
  }
  {
    const options={liveSpeech:true,immediateHash:true,clock:true};
    const stale=boot(false,true,options);stale.addReplies(-8);stale.audio();await stale.ui.refresh();
    stale.nodes.get('#conversation-voice-replies').handlers.click();await stale.ui.refresh();await until(()=>!stale.nodes.get('#conversation-refresh').disabled);await new Promise(resolve=>setImmediate(resolve));stale.nativeRequest();await stale.ui.refresh();const player=stale.nodes.get('#conversation-audio');
    await until(()=>stale.audioCalls.length===1&&!player.paused&&!player.muted);player.ended=true;await player.handlers.ended();stale.clock.fire(1200);await new Promise(resolve=>setImmediate(resolve));
    options.omitRequests=true;options.questionDetailFailure=true;
    options.responseDraft={provisional:true,native_turn:'available-physical',native_item:'available-item',text:'A new useful answer. More',observed_at:'2100-01-01T00:00:00Z'};
    await stale.ui.refresh();
    try{
      assert.equal(stale.nodes.get('#conversation-voice-replies').dataset.voiceState,'on','Voice intent remains enabled during an absent old question');
      await until(()=>stale.audioCalls.length===2&&!player.paused&&!player.muted);assert(stale.audioCalls[1].source.startsWith('live:'),'One unavailable old question cannot silence the whole conversation');
      player.ended=true;await player.handlers.ended();stale.clock.fire(180);await new Promise(resolve=>setImmediate(resolve));options.omitRequests=false;options.questionDetailFailure=false;stale.advanceNativeRequest();await stale.ui.refresh();
      await until(()=>stale.audioCalls.length===3);assert.equal(stale.audioCalls[2].source,'request:native-request');assert.equal(stale.audioCalls[2].revision,'native-revision-2','Fresh registry metadata restores an available question without reviving an absent source');
    }finally{stale.ui.stop();}
  }
  {
    const text='Puedo conservar el texto exacto y comenzar a explicar cada paso mientras seguimos trabajando juntos sin esperar a que termine toda la respuesta porque las palabras ya disponibles describen una acción concreta y útil para nuestra conversación y después continuamos con más detalles';
    const options={liveSpeech:true,clock:true,playback:true,capture:true,responseDraft:{provisional:true,native_turn:'long-physical',native_item:'long-item',text,observed_at:'2100-01-01T00:00:00Z'}};
    const longVoice=boot(false,true,options);longVoice.audio();await longVoice.ui.refresh();
    longVoice.nodes.get('#conversation-voice-replies').handlers.click();await longVoice.ui.refresh();
    const player=longVoice.nodes.get('#conversation-audio');await until(()=>longVoice.audioCalls.length===1&&!player.paused&&!player.muted);
    const end=Number(longVoice.audioCalls[0].source.split(':')[4]);assert(end>60&&end<=240);
    assert(options.responseDraft,'Useful speech must begin before native completion');
    player.currentTime=.5;player.duration=2;assert.equal(longVoice.playbackContext().segment_offset,0);
    const confirmed=text+'.';options.liveText=confirmed;options.responseDraft=null;longVoice.addReplies(1);
    longVoice.overrideSource(9,{native_turn:'long-physical',native_item:'long-item',text_characters:Array.from(confirmed).length,preview:confirmed});longVoice.replyTexts.set('native:9',confirmed);
    const prefixMedia=player.src;await longVoice.ui.refresh();await until(()=>longVoice.audioCalls.length===2);
    assert.equal(player.src,prefixMedia,'Canonical arrival cannot replace the active long-sentence prefix');
    player.ended=true;await player.handlers.ended();longVoice.clock.fire(180);
    await until(()=>longVoice.audioCalls.length===2&&!player.paused);
    assert.equal(longVoice.audioCalls[1].offset,end,'Canonical speech continues after the exact whole-word prefix');
    assert.equal(longVoice.nativePosts.length,0,'Early speech cannot answer questions or affect native jobs');longVoice.ui.stop();
  }
  const liveOptionsAudio={liveSpeech:true,clock:true,playback:true,capture:true,responseDraft:{provisional:true,native_turn:'physical',native_item:'early',text:'First useful sentence. Remaining text',observed_at:'2100-01-01T00:00:00Z'}};
  const liveVoice=boot(false,true,liveOptionsAudio);liveVoice.audio();await liveVoice.ui.refresh();
  liveVoice.nodes.get('#conversation-voice-replies').handlers.click();await liveVoice.ui.refresh();
  const livePlayer=liveVoice.nodes.get('#conversation-audio');await until(()=>liveVoice.audioCalls.length===1&&!livePlayer.paused&&!livePlayer.muted);
  const liveEnd=Number(liveVoice.audioCalls[0].source.split(':')[4]);assert.equal(liveVoice.audioCalls[0].offset,0);
  livePlayer.currentTime=.5;livePlayer.duration=2;
  assert(liveVoice.playbackContext().source.startsWith('live:'),'Capture retains a source-linked device cursor during early speech');
  const confirmedText='First useful sentence. Remaining text is now confirmed.';
  liveOptionsAudio.liveText=confirmedText;liveOptionsAudio.responseDraft=null;liveVoice.addReplies(1);
  liveVoice.overrideSource(9,{native_turn:'physical',native_item:'early',text_characters:Array.from(confirmedText).length,preview:confirmedText});liveVoice.replyTexts.set('native:9',confirmedText);
  const liveMedia=livePlayer.src;await liveVoice.ui.refresh();await until(()=>liveVoice.audioCalls.length===2);assert.equal(livePlayer.src,liveMedia,'Preparing the canonical tail cannot replace the active prefix');
  livePlayer.ended=true;await livePlayer.handlers.ended();liveVoice.clock.fire(180);
  await until(()=>liveVoice.audioCalls.length===2&&!livePlayer.paused);
  assert.equal(liveVoice.audioCalls[1].source,'native:9');assert.equal(liveVoice.audioCalls[1].offset,liveEnd,'Canonical speech starts after the reserved prefix');
  liveVoice.ui.stop();
  const hashHold={active:true,releases:[]},selectionOptions={liveSpeech:true,capture:true,hashHold,responseDraft:{provisional:true,native_turn:'held-physical',native_item:'held-item',text:'Useful interrupted sentence. More',observed_at:'2100-01-01T00:00:00Z'}};
  const selectionRace=boot(false,true,selectionOptions);selectionRace.audio();await selectionRace.ui.refresh();
  selectionRace.nodes.get('#conversation-voice-replies').handlers.click();await selectionRace.ui.refresh();await until(()=>hashHold.releases.length===1);
  selectionRace.capture();selectionRace.finishCapture();
  hashHold.releases.shift()();await until(()=>hashHold.releases.length===1);hashHold.releases.shift()();await new Promise(resolve=>setImmediate(resolve));
  assert.equal(selectionRace.audioCalls.length,0,'Capture invalidates an awaited prefix even after recording finishes');
  hashHold.active=false;await selectionRace.ui.refresh();await until(()=>selectionRace.audioCalls.length===1);
  selectionRace.ui.stop();
  const aheadOptions={liveSpeech:true,clock:true,immediateHash:true,responseDraft:{provisional:true,native_turn:'ahead-physical',native_item:'ahead-item',text:'First sentence. Waiting',observed_at:'2100-01-01T00:00:00Z'}};
  const ahead=boot(false,true,aheadOptions);ahead.audio();await ahead.ui.refresh();ahead.nodes.get('#conversation-voice-replies').handlers.click();await ahead.ui.refresh();
  const aheadPlayer=ahead.nodes.get('#conversation-audio');await until(()=>ahead.audioCalls.length===1&&!aheadPlayer.paused);
  const firstMedia=aheadPlayer.src;
  aheadOptions.responseDraft.text='First sentence. Second useful sentence. Waiting';await ahead.ui.refresh();await new Promise(resolve=>setImmediate(resolve));
  assert.equal(ahead.audioCalls.length,2,'The next live prefix is prepared while the first media still plays');
  assert.equal(aheadPlayer.src,firstMedia,'Preparation cannot replace the audible element');
  ahead.nativeRequest();await ahead.ui.refresh();
  aheadPlayer.ended=true;await aheadPlayer.handlers.ended();ahead.clock.fire(180);
  await until(()=>ahead.audioCalls.length===3&&!aheadPlayer.paused);
  assert.equal(ahead.audioCalls[2].source,'request:native-request','A pending question precedes the prepared live prefix');
  const questionMedia=aheadPlayer.src;
  aheadPlayer.ended=true;await aheadPlayer.handlers.ended();ahead.clock.fire(1200);
  await until(()=>aheadPlayer.src!==questionMedia&&!aheadPlayer.paused);
  assert.equal(ahead.audioCalls.length,3,'Returning from a question reuses the existing preparation');
  ahead.ui.stop();
  // A failed paid preparation is also retained across question priority. It is
  // consumed once, without silently retrying the same provider intent.
  const failedAheadOptions={liveSpeech:true,clock:true,immediateHash:true,responseDraft:{provisional:true,native_turn:'failed-ahead',native_item:'item',text:'First sentence. Waiting',observed_at:'2100-01-01T00:00:00Z'}};
  const failedAhead=boot(false,true,failedAheadOptions);failedAhead.audio();await failedAhead.ui.refresh();failedAhead.nodes.get('#conversation-voice-replies').handlers.click();await failedAhead.ui.refresh();
  const failedPlayer=failedAhead.nodes.get('#conversation-audio');await until(()=>failedAhead.audioCalls.length===1&&!failedPlayer.paused);
  failedAhead.failAudio();failedAheadOptions.responseDraft.text='First sentence. Second sentence. Waiting';await failedAhead.ui.refresh();await until(()=>failedAhead.audioCalls.length===2);
  failedAhead.nativeRequest();await failedAhead.ui.refresh();failedPlayer.ended=true;await failedPlayer.handlers.ended();failedAhead.clock.fire(180);
  await until(()=>failedAhead.audioCalls.length===3&&!failedPlayer.paused);
  failedPlayer.ended=true;await failedPlayer.handlers.ended();failedAhead.clock.fire(1200);await new Promise(resolve=>setImmediate(resolve));
  assert.equal(failedAhead.audioCalls.length,3,'An unsuccessful prepared prefix is not synthesized again after a question');
  assert.equal(failedPlayer.paused,true);failedAhead.ui.stop();
  for(const invalidate of ['capture','changed','stop']){
    const pendingOptions={liveSpeech:true,clock:true,capture:true,immediateHash:true,responseDraft:{provisional:true,native_turn:'parked-'+invalidate,native_item:'item',text:'First sentence. Waiting',observed_at:'2100-01-01T00:00:00Z'}};
    const pending=boot(false,true,pendingOptions);pending.audio();await pending.ui.refresh();pending.nodes.get('#conversation-voice-replies').handlers.click();await pending.ui.refresh();
    const player=pending.nodes.get('#conversation-audio');await until(()=>pending.audioCalls.length===1&&!player.paused);
    pendingOptions.responseDraft.text='First sentence. Second sentence. Waiting';await pending.ui.refresh();await until(()=>pending.audioCalls.length===2);
    pending.nativeRequest();await pending.ui.refresh();player.ended=true;await player.handlers.ended();pending.clock.fire(180);
    await until(()=>pending.audioCalls.length===3&&!player.paused);
    if(invalidate==='capture'){
      pending.capture();pending.finishCapture();await new Promise(resolve=>setImmediate(resolve));
      assert.equal(player.paused,true,'Capture pauses the question and holds the parked live item');
    }else if(invalidate==='stop'){
      pending.nodes.get('#conversation-voice-replies').handlers.click();
    }else {
      pendingOptions.responseDraft=null;pendingOptions.liveText='A changed canonical response.';pending.addReplies(1);
      pending.overrideSource(9,{native_turn:'parked-'+invalidate,native_item:'item',text_characters:pendingOptions.liveText.length,preview:pendingOptions.liveText});pending.replyTexts.set('native:9',pendingOptions.liveText);
      await pending.ui.refresh();const question=player.src;player.ended=true;await player.handlers.ended();pending.clock.fire(1200);
      await until(()=>pending.audioCalls.length===4&&player.src!==question&&!player.paused);
      assert.equal(pending.audioCalls[3].source,'native:9','Changed confirmation discards the parked live prefix');
    }
    pending.clock.fire(180);pending.clock.fire(450);pending.clock.fire(1200);await new Promise(resolve=>setImmediate(resolve));
    assert.equal(pending.audioCalls.length,invalidate==='changed'?4:3,'Invalidated parked preparations cannot restart or retry automatically');pending.ui.stop();
  }
  const interruptionOptions={liveSpeech:true,clock:true,capture:true,immediateHash:true,responseDraft:{provisional:true,native_turn:'interruption-physical',native_item:'interruption-item',text:'First sentence. Unfinished',observed_at:'2100-01-01T00:00:00Z'}};
  const interruptedLive=boot(false,true,interruptionOptions);interruptedLive.audio();await interruptedLive.ui.refresh();
  interruptedLive.nodes.get('#conversation-voice-replies').handlers.click();await interruptedLive.ui.refresh();
  const interruptedPlayer=interruptedLive.nodes.get('#conversation-audio');await until(()=>interruptedLive.audioCalls.length===1&&!interruptedPlayer.paused);
  interruptionOptions.responseDraft.text='First sentence. Second sentence. Unfinished';await interruptedLive.ui.refresh();
  interruptedLive.capture();interruptedLive.finishCapture();await new Promise(resolve=>setImmediate(resolve));
  assert.equal(interruptedLive.audioCalls.length,2,'Capture retains the one already prepared prefix without another synthesis');
  const interruptedText='First sentence. Second sentence. Confirmed tail.';interruptionOptions.liveText=interruptedText;interruptionOptions.responseDraft=null;interruptedLive.addReplies(1);
  interruptedLive.overrideSource(9,{native_turn:'interruption-physical',native_item:'interruption-item',text_characters:interruptedText.length,preview:interruptedText});interruptedLive.replyTexts.set('native:9',interruptedText);
  await interruptedLive.ui.refresh();assert.equal(interruptedLive.audioCalls.length,2,'Unchanged canonical confirmation does not synthesize interrupted narration');
  assert.equal(interruptedPlayer.paused,true);
  await interruptedPlayer.beforeNodes[0].handlers.click();await until(()=>!interruptedPlayer.paused&&!interruptedPlayer.muted);
  assert.equal(interruptedLive.audioCalls.length,2,'Explicit Play reuses the interrupted media');
  const interruptedMedia=interruptedPlayer.src;interruptedPlayer.ended=true;await interruptedPlayer.handlers.ended();interruptedLive.clock.fire(180);
  await until(()=>interruptedPlayer.src!==interruptedMedia&&!interruptedPlayer.paused);
  assert(interruptedLive.audioCalls[1].source.startsWith('live:'),'Explicit resume consumes the already prepared prefix');
  interruptedPlayer.ended=true;await interruptedPlayer.handlers.ended();interruptedLive.clock.fire(180);
  await until(()=>interruptedLive.audioCalls.length===3&&!interruptedPlayer.paused);
  assert.equal(interruptedLive.audioCalls[2].source,'native:9');
  assert.equal(interruptedLive.audioCalls[2].offset,Number(interruptedLive.audioCalls[1].source.split(':')[4]),'Explicit resume continues the matching canonical tail once');

  interruptedLive.ui.stop();

  // A captured provisional prefix can become a confirmed reply while paused.
  // Its validated word cursor must survive that handover and a view reload.
  for(const alignment of [true,false,'invalid']){
    const storage=new Map(),words={liveSpeech:true,clock:true,capture:true,playback:true,progressive:true,incremental:true,wordTiming:true,followStorage:storage,
      wordPosition:(seconds,offset)=>alignment==='invalid'?offset+9000:alignment?offset+(seconds>=4?20:0):undefined,
      responseDraft:{provisional:true,native_turn:'partial-live',native_item:'partial-item',text:'A useful spoken sentence with words. Unfinished',observed_at:'2100-01-01T00:00:00Z'}};
    const partial=boot(false,true,words);partial.audio();await partial.ui.refresh();partial.nodes.get('#conversation-voice-replies').handlers.click();await partial.ui.refresh();
    const player=partial.nodes.get('#conversation-audio');await until(()=>partial.audioCalls.length===1&&!player.paused&&!player.muted);
    player.currentTime=5;partial.capture();partial.finishCapture();
    const text='A useful spoken sentence with words. The confirmed remainder.';words.liveText=text;words.responseDraft=null;partial.addReplies(1);
    partial.overrideSource(9,{native_turn:'partial-live',native_item:'partial-item',text_characters:text.length,preview:text});partial.replyTexts.set('native:9',text);
    await partial.ui.refresh();
    const saved=JSON.parse(storage.get('interaction-speech-position:fixture')).find(row=>row.source==='native:9');
    assert.equal(saved.offset,alignment===true?20:0,'Provisional capture preserves the available exact word cursor through canonical handover');
    assert.equal(partial.audioCalls.length,1,'A paused prefix is not synthesized again on confirmation');
    if(alignment===true){player.currentTime=1;partial.ui.pause();assert.equal(JSON.parse(storage.get('interaction-speech-position:fixture')).find(row=>row.source==='native:9').offset,0,'A backward seek in the retained prefix replaces its newer cursor');player.currentTime=5;partial.ui.pause();}
    const reloaded=boot(false,true,{...words,followStorage:storage});reloaded.audio();reloaded.addReplies(1);
    reloaded.overrideSource(9,{native_turn:'partial-live',native_item:'partial-item',text_characters:text.length,preview:text});reloaded.replyTexts.set('native:9',text);await reloaded.ui.refresh();
    const card=reloaded.nodes.get('#conversation-timeline').children.find(c=>c.dataset.sourceId==='native:9');await card.children[3].children[1].handlers.click();
    assert.equal(reloaded.audioCalls.at(-1).offset,alignment===true?20:0);reloaded.ui.stop();
    {
      await player.beforeNodes[0].handlers.click();await until(()=>!player.paused&&!player.muted);
      player.ended=true;await player.handlers.ended();
      assert.equal(JSON.parse(storage.get('interaction-speech-position:fixture')).find(row=>row.source==='native:9').offset,Number(partial.audioCalls[0].source.split(':')[4]),'A completed prefix keeps its full boundary with missing or shorter word timing');
    }
    partial.ui.stop();
  }

  for(const failedCanonicalWrite of [false,true]){
    const storage=new Map();let rejectPositionWrite=false;
    const set=storage.set.bind(storage);storage.set=(key,value)=>{
      if(rejectPositionWrite && key==='interaction-speech-position:fixture')throw new Error('Synthetic storage quota');
      return set(key,value);
    };
    const options={liveSpeech:true,clock:true,capture:true,playback:true,progressive:true,incremental:true,wordTiming:true,followStorage:storage,
      wordPosition:(seconds,offset)=>offset+(seconds>=4?20:0),responseDraft:{provisional:true,native_turn:'reload-live',native_item:'reload-item',text:'A useful spoken sentence with words. Unfinished',observed_at:'2100-01-01T00:00:00Z'}};
    const original=boot(false,true,options);original.audio();await original.ui.refresh();original.nodes.get('#conversation-voice-replies').handlers.click();await original.ui.refresh();
    const player=original.nodes.get('#conversation-audio');await until(()=>original.audioCalls.length===1&&!player.paused&&!player.muted);
    player.currentTime=5;original.capture();original.finishCapture();original.ui.stop();
    assert.equal(JSON.parse(storage.get('interaction-live-speech:fixture'))[0].partial.end,20);
    const text='A useful spoken sentence with words. The confirmed remainder.';
    rejectPositionWrite=failedCanonicalWrite;
    const reloaded=boot(false,true,{...options,responseDraft:null,liveText:text});reloaded.audio();reloaded.addReplies(1);
    reloaded.overrideSource(9,{native_turn:'reload-live',native_item:'reload-item',text_characters:text.length,preview:text});reloaded.replyTexts.set('native:9',text);
    const validation=failedCanonicalWrite?undefined:reloaded.holdPart();
    const refresh=reloaded.ui.refresh();
    if(validation){
      await validation.entered;
      const pendingCard=reloaded.nodes.get('#conversation-timeline').children.find(c=>c.dataset.sourceId==='native:9');
      const listen=pendingCard.children[3].children[1];assert.equal(listen.disabled,true);
      await listen.handlers.click();assert.equal(reloaded.audioCalls.length,0,'Explicit Listen waits for exact restored-source validation');
      validation.release({text:'A useful spoken sentence with words. ',offset:0,next_offset:null});
    }
    await refresh;
    assert.equal(reloaded.audioCalls.length,0,'Restoring a cursor on the initial history page must not speak history');
    assert(validation || reloaded.partReads.some(row=>row.source.startsWith('live:')),'Initial canonical page revalidates the retained prefix');
    assert.equal(JSON.parse(storage.get('interaction-live-speech:fixture')).length,failedCanonicalWrite?1:0,'Retirement requires a successful canonical write');
    const card=reloaded.nodes.get('#conversation-timeline').children.find(c=>c.dataset.sourceId==='native:9');await card.children[3].children[1].handlers.click();
    assert.equal(reloaded.audioCalls.at(-1).offset,20,'Reload before confirmation preserves the unsaid boundary when canonical text arrives on the first page');
    {
      rejectPositionWrite=false;
      const resumedPlayer=reloaded.nodes.get('#conversation-audio');await until(()=>!resumedPlayer.paused&&!resumedPlayer.muted);
      resumedPlayer.currentTime=5;reloaded.ui.pause();
      assert.equal(JSON.parse(storage.get('interaction-speech-position:fixture')).find(row=>row.source==='native:9').offset,40);
      assert.equal(JSON.parse(storage.get('interaction-live-speech:fixture')).length,0,'A successful later canonical save retires the older backup');
      await reloaded.ui.refresh();
      assert.equal(JSON.parse(storage.get('interaction-speech-position:fixture')).find(row=>row.source==='native:9').offset,40,'Repeated history observation cannot rewind later canonical progress');
      reloaded.ui.stop();await card.children[3].children[1].handlers.click();
      assert.equal(reloaded.audioCalls.at(-1).offset,40,'Explicit Resume keeps progress after history refresh');
      resumedPlayer.currentTime=0;
    }
    reloaded.ui.stop();
    if(failedCanonicalWrite){
      rejectPositionWrite=false;
      const recovered=boot(false,true,{...options,responseDraft:null,liveText:text});recovered.audio();recovered.addReplies(1);
      recovered.overrideSource(9,{native_turn:'reload-live',native_item:'reload-item',text_characters:text.length,preview:text});recovered.replyTexts.set('native:9',text);
      await recovered.ui.refresh();
      assert.equal(JSON.parse(storage.get('interaction-speech-position:fixture')).find(row=>row.source==='native:9').offset,40,'Reload retains newer playback after storage recovers in the same view');
      assert.equal(recovered.audioCalls.length,0);recovered.ui.stop();
    }
  }

  const gapOptions={liveSpeech:true,clock:true,progressive:true,incremental:true,wordTiming:true,wordPosition:(_seconds,offset)=>offset+10,responseDraft:{provisional:true,native_turn:'gap-physical',native_item:'gap-item',text:'First sentence. Unfinished',observed_at:'2100-01-01T00:00:00Z'}};
  const gapVoice=boot(false,true,gapOptions);gapVoice.audio();await gapVoice.ui.refresh();gapVoice.failAudio();
  gapVoice.nodes.get('#conversation-voice-replies').handlers.click();await gapVoice.ui.refresh();await until(()=>gapVoice.clock.pending().includes(450));
  gapOptions.responseDraft.text='First sentence. Second sentence. Unfinished';await gapVoice.ui.refresh();gapVoice.clock.fire(450);
  const gapPlayer=gapVoice.nodes.get('#conversation-audio');await until(()=>gapVoice.audioCalls.length===2&&!gapPlayer.paused);
  gapPlayer.ended=true;await gapPlayer.handlers.ended();
  const gapText='First sentence. Second sentence. The confirmed tail.';gapOptions.liveText=gapText;gapOptions.responseDraft=null;gapVoice.addReplies(1);
  gapVoice.overrideSource(9,{native_turn:'gap-physical',native_item:'gap-item',text_characters:gapText.length,preview:gapText});gapVoice.replyTexts.set('native:9',gapText);
  await gapVoice.ui.refresh();gapVoice.clock.fire(180);await until(()=>gapVoice.audioCalls.length===3&&!gapPlayer.paused);
  gapPlayer.currentTime=5;gapVoice.ui.pause();
  const gapCard=gapVoice.nodes.get('#conversation-timeline').children.find(card=>card.dataset.sourceId==='native:9');
  await gapCard.children[3].children[1].handlers.click();
  await until(()=>gapVoice.audioCalls.length===4);
  assert.equal(gapVoice.audioCalls.at(-1).offset,0,'Pause during the canonical tail cannot overwrite an earlier unsaid prefix');
  gapVoice.ui.stop();



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

  // Skip drops every remaining segment of one message, preserving queue order.
  const skipping=boot(false,true,{clock:true,incremental:true});skipping.audio();
  skipping.replyTexts.set('native:9','Long sentence. '.repeat(150));await skipping.ui.refresh();
  skipping.nodes.get('#conversation-voice-replies').handlers.click();skipping.addReplies(3);await skipping.ui.refresh();
  const skipPlayer=skipping.nodes.get('#conversation-audio');
  await until(()=>skipping.audioCalls.length===2&&!skipPlayer.paused&&!skipPlayer.muted);
  const skipCard=skipping.nodes.get('#conversation-timeline').children.find(card=>card.dataset.sourceId==='native:9');
  const skipButton=skipCard.children[3].children[1];assert.equal(skipButton.textContent,'Skip');
  await skipButton.handlers.click();await until(()=>skipping.audioCalls.some(call=>call.source==='native:10')&&!skipPlayer.paused);
  assert.equal(skipping.audioCalls.filter(call=>call.source==='native:9').length,2,'Already prepared continuation is cancelled rather than played or retried');
  assert.equal(skipButton.textContent,'Resume audio','Skip preserves the actual heard cursor for a later explicit resume');
  await skipping.ui.refresh();skipPlayer.ended=true;await skipPlayer.handlers.ended();skipping.clock.fire(450);
  await until(()=>skipping.audioCalls.some(call=>call.source==='native:11'));
  assert.deepEqual([...new Set(skipping.audioCalls.map(call=>call.source))],['native:9','native:10','native:11']);
  assert.equal(skipping.nativePosts.length,0,'Skipping audio does not answer or change native work');skipping.ui.stop();

  const incremental=boot(false,true,{clock:true,incremental:true});incremental.audio();incremental.replyTexts.set('native:9','A sentence with Unicode 🙂. '.repeat(40));await incremental.ui.refresh();
  incremental.nodes.get('#conversation-voice-replies').handlers.click();incremental.addReplies(1);await incremental.ui.refresh();
  const early=incremental.nodes.get('#conversation-audio');await until(()=>incremental.audioCalls.length===2&&!early.paused).catch(error=>{throw Error(JSON.stringify({stage:'initial_incremental_playback',calls:incremental.audioCalls,state:incremental.nodes.get('#conversation-audio-state').textContent,paused:early.paused,muted:early.muted}));});
  assert.deepEqual(incremental.audioCalls.map(x=>x.offset),[0,120],'Exactly one future segment prepares during current playback');
  incremental.ui.pause();early.beforeNodes[0].handlers.click();await until(()=>!early.paused&&!early.muted);assert.equal(incremental.audioCalls.length,2,'Resume reuses the owned future attempt');
  early.ended=true;const continuing=early.handlers.ended();incremental.clock.fire(180);await continuing;await until(()=>incremental.audioCalls.length===3&&!early.paused).catch(e=>{throw Error(JSON.stringify({calls:incremental.audioCalls,state:incremental.nodes.get('#conversation-state').textContent,audioState:incremental.nodes.get('#conversation-audio-state').textContent,paused:early.paused,muted:early.muted}));});
  assert.deepEqual(incremental.audioCalls.map(x=>x.offset),[0,120,420],'Continuation consumes its exact cached attempt and prepares only the next segment');
  incremental.ui.stop();const stoppedCalls=incremental.audioCalls.length;incremental.clock.fire(180);await new Promise(r=>setImmediate(r));assert.equal(incremental.audioCalls.length,stoppedCalls,'Stop cannot revive prefetched speech');
  // Barge-in pauses the existing media immediately without canceling its
  // stream or any native work. Reload can restart the interrupted segment,
  // never claim word-level hearing from an estimated position.
  const positionStorage=new Map();
  const barge=boot(false,true,{clock:true,incremental:true,progressive:true,capture:true,followStorage:positionStorage});
  barge.audio();barge.replyTexts.set('native:9','A retained mixed-language reply 🙂. '.repeat(40));await barge.ui.refresh();
  barge.nodes.get('#conversation-voice-replies').handlers.click();barge.addReplies(1);await barge.ui.refresh();
  const bargePlayer=barge.nodes.get('#conversation-audio');await until(()=>barge.audioCalls.length===2&&!bargePlayer.paused);
  bargePlayer.ended=true;const bargeNext=bargePlayer.handlers.ended();barge.clock.fire(180);await bargeNext;
  await until(()=>barge.audioCalls.length===3&&!bargePlayer.paused);bargePlayer.currentTime=7.5;
  const activeTransport=barge.streamedTransports[1],beforeBargeCalls=barge.audioCalls.length;
  barge.capture();assert.equal(bargePlayer.paused,true);assert.equal(bargePlayer.muted,true);
  assert.equal(bargePlayer.currentTime,7.5);assert.equal(activeTransport.cancelled,false,'Barge-in retains the bounded owned audio stream');
  await bargePlayer.beforeNodes[0].handlers.click();assert.equal(bargePlayer.paused,true,'Manual Play cannot leak audio into capture');
  barge.finishCapture();await bargePlayer.beforeNodes[0].handlers.click();
  assert.equal(bargePlayer.paused,false);assert.equal(bargePlayer.currentTime,7.5);
  assert.equal(barge.audioCalls.length,beforeBargeCalls,'Resume reuses the same media without synthesis');
  barge.capture();barge.finishCapture();
  const stored=JSON.parse(positionStorage.get('interaction-speech-position:fixture'));
  assert.deepEqual(stored.find(v=>v.source==='native:9'),{source:'native:9',revision:'revision-9',offset:120});
  assert(!JSON.stringify(stored).includes('mixed-language'),'Playback storage contains no transcript');
  barge.capture();const beforeWorker=barge.audioCalls.length;
  barge.addReplies(1);barge.overrideSource(10,{kind:'worker_result',label:'Worker',target:{chat_id:100,thread_id:77},current:true,native_binding_current:true});await barge.ui.refresh();
  assert.equal(barge.audioCalls.length,beforeWorker,'Worker returns wait while the microphone owns audio');
  barge.finishCapture();await until(()=>barge.audioCalls.length>beforeWorker&&!bargePlayer.paused);
  assert.equal(barge.audioCalls[beforeWorker].source,'native:10','Capture does not discard useful queued worker returns');

  const restored=boot(false,true,{incremental:true,followStorage:positionStorage});restored.audio();restored.addReplies(1);
  restored.replyTexts.set('native:9','A retained mixed-language reply 🙂. '.repeat(40));await restored.ui.refresh();
  const restoredCard=restored.nodes.get('#conversation-timeline').children.find(c=>c.dataset.sourceId==='native:9');
  const resume=restoredCard.children.flatMap(c=>c.children||[]).find(c=>c.textContent==='Resume audio');assert(resume);
  await resume.handlers.click();assert.equal(restored.audioCalls[0].offset,120,'Reload resumes at the last segment boundary, conservatively repeating the interrupted segment');
  assert(restored.partReads.some(v=>v.source==='native:9'&&v.offset===120),'Resume revalidates canonical source text');
  restored.overrideSource(9,{revision:'new-revision',text_sha256:'new-sha'});await restored.ui.refresh();
  assert(!JSON.parse(positionStorage.get('interaction-speech-position:fixture')).some(v=>v.source==='native:9'),'A source revision change retires the old playback cursor');
  assert.equal(barge.nativePosts.length,0);assert.equal(restored.nativePosts.length,0);barge.ui.stop();restored.ui.stop();
  const futureFailure=boot(false,true,{clock:true,incremental:true,failOffset:120});futureFailure.audio();futureFailure.replyTexts.set('native:9','Actual canonical text. '.repeat(40));await futureFailure.ui.refresh();
  futureFailure.nodes.get('#conversation-voice-replies').handlers.click();futureFailure.addReplies(1);await futureFailure.ui.refresh();
  const futurePlayer=futureFailure.nodes.get('#conversation-audio');await until(()=>futureFailure.audioCalls.length===2&&!futurePlayer.paused);
  futurePlayer.ended=true;const futureEnd=futurePlayer.handlers.ended();futureFailure.clock.fire(180);await futureEnd;
  assert.equal(futureFailure.audioCalls.filter(x=>x.offset===120).length,1,'Consuming a failed/uncertain future preparation never repeats its provider attempt');futureFailure.ui.stop();

  const nextBackground=boot(false,true,{clock:true});nextBackground.audio();await nextBackground.ui.refresh();
  nextBackground.nodes.get('#conversation-voice-replies').handlers.click();nextBackground.addReplies(1);await nextBackground.ui.refresh();
  const nextPlayer=nextBackground.nodes.get('#conversation-audio');await until(()=>nextBackground.audioCalls.length===1&&!nextPlayer.paused);
  nextBackground.hide();nextPlayer.ended=true;nextPlayer.paused=true;await nextPlayer.handlers.ended();nextBackground.clock.fire(450);
  nextBackground.addReplies(1);await nextBackground.ui.refresh();await until(()=>nextBackground.audioCalls.length===2&&!nextPlayer.paused);
  assert.equal(nextBackground.mediaSession.playbackState,'playing','A new reply after end-of-media starts while still hidden');
  assert.equal(nextBackground.nodes.get('#conversation-voice-replies').dataset.voiceState,'on');nextBackground.ui.stop();

  const background=boot(false,true,{clock:true});background.audio(true);await background.ui.refresh();
  background.nodes.get('#conversation-voice-replies').handlers.click();background.addReplies(1);await background.ui.refresh();
  const bgPlayer=background.nodes.get('#conversation-audio');await until(()=>background.audioCalls.length===1&&!bgPlayer.paused);
  background.hide();assert.equal(bgPlayer.paused,false,'Standalone Voice on keeps playing when another app is foreground');
  bgPlayer.ended=true;const bgEnding=bgPlayer.handlers.ended();background.clock.fire(180);await bgEnding;
  await until(()=>background.audioCalls.length===2&&!bgPlayer.paused);
  bgPlayer.paused=true;background.show();await until(()=>!bgPlayer.paused);
  assert.equal(background.audioCalls.length,2,'Returning resumes the same verified segment without replaying its input');
  assert.equal(background.mediaSession.playbackState,'playing','Opted-in playback is registered with the OS media session');
  background.hide();background.event('offline');assert.equal(bgPlayer.paused,false,'A network transition does not silence an already loaded reply');
  assert.equal(background.nodes.get('#conversation-voice-replies').dataset.voiceState,'on');
  bgPlayer.paused=true;background.event('online');await until(()=>!bgPlayer.paused);
  background.show();
  await until(()=>!background.nodes.get('#conversation-refresh').disabled);background.failPage();await background.ui.refresh();
  assert.equal(background.nodes.get('#conversation-voice-replies').dataset.voiceState,'on','A transient refresh failure cannot become a human pause');
  background.failPage('AbortError');await background.ui.refresh();
  assert.equal(background.nodes.get('#conversation-voice-replies').dataset.voiceState,'on','The API request deadline cannot become a human pause');
  background.mediaActions.get('pause')();assert.equal(bgPlayer.paused,true);assert.equal(background.mediaSession.playbackState,'paused');
  assert.equal(background.nodes.get('#conversation-voice-replies').dataset.lastPauseReason,'media-session','Retain the actual pause entry point locally for device diagnosis');
  background.show();assert.equal(bgPlayer.paused,true,'OS Pause is explicit human intent');
  background.mediaActions.get('play')();await until(()=>!bgPlayer.paused);
  background.ui.pause();background.hide();background.show();
  assert.equal(bgPlayer.paused,true,'Returning never overrides an explicit human pause');background.ui.stop();

  const speakerResume=boot(false,true,{clock:true});speakerResume.audio();await speakerResume.ui.refresh();
  speakerResume.nodes.get('#conversation-voice-replies').handlers.click();speakerResume.addReplies(1);await speakerResume.ui.refresh();
  const srPlayer=speakerResume.nodes.get('#conversation-audio');await until(()=>speakerResume.audioCalls.length===1&&!srPlayer.paused);
  assert.equal(srPlayer.beforeNodes[0].hidden,false,'Playing reply exposes Pause in the persistent panel');
  speakerResume.mediaActions.get('pause')();assert.equal(srPlayer.paused,true);assert.equal(srPlayer.beforeNodes[0].hidden,false,'Pause keeps the same item resumable');
  srPlayer.beforeNodes[0].handlers.click();
  await until(()=>!srPlayer.paused);
  assert.equal(speakerResume.audioCalls.length,1,'Speaker resume reuses the loaded reply, without another synthesis');
  speakerResume.addReplies(1);await speakerResume.ui.refresh();srPlayer.ended=true;await srPlayer.handlers.ended();speakerResume.clock.fire(450);
  await until(()=>speakerResume.audioCalls.length===2&&!srPlayer.paused);
  assert.equal(speakerResume.audioCalls[1].source,'native:10','New replies remain audible after speaker resume, without a Stop cycle');speakerResume.ui.stop();
  const emptyResume=boot(false,true,{clock:true});await emptyResume.ui.refresh();emptyResume.nodes.get('#conversation-voice-replies').handlers.click();emptyResume.addReplies(1);await emptyResume.ui.refresh();
  await until(()=>emptyResume.audioCalls.length===1);emptyResume.ui.pause();
  emptyResume.nodes.get('#conversation-voice-replies').handlers.click();emptyResume.nodes.get('#conversation-voice-replies').handlers.click();emptyResume.audio();emptyResume.addReplies(1);await emptyResume.ui.refresh();
  await until(()=>emptyResume.audioCalls.length===2&&!emptyResume.nodes.get('#conversation-audio').paused);
  assert.equal(emptyResume.audioCalls[1].source,'native:10','An aborted preparation cannot hold the resumed voice queue');
  assert.equal(emptyResume.audioCalls.filter(call=>call.source==='native:9').length,1,'Resuming never automatically retries the cancelled/uncertain synthesis');emptyResume.ui.stop();
  const captureResume=boot(false,true,{clock:true,capture:true});captureResume.audio();await captureResume.ui.refresh();captureResume.nodes.get('#conversation-voice-replies').handlers.click();captureResume.addReplies(1);await captureResume.ui.refresh();
  const crPlayer=captureResume.nodes.get('#conversation-audio');await until(()=>captureResume.audioCalls.length===1&&!crPlayer.paused);
  captureResume.capture();captureResume.mediaActions.get('pause')();captureResume.nodes.get('#conversation-voice-replies').handlers.click();captureResume.nodes.get('#conversation-voice-replies').handlers.click();
  captureResume.addReplies(1);await captureResume.ui.refresh();assert.equal(crPlayer.paused,true,'Resuming Voice cannot play output during recording');
  captureResume.finishCapture();await until(()=>captureResume.audioCalls.length===2&&!crPlayer.paused);
  assert.equal(captureResume.audioCalls[1].source,'native:10','Recording release drains the resumed queue without a Stop cycle');captureResume.ui.stop();

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

  const submittedClips=[],recordingId='same-recording';
  const dismissWhileRecording=boot(false,true,{capture:true,ownsVoice:true,voiceSubmit:()=>submittedClips.push(recordingId)});
  dismissWhileRecording.humanTasks.set('e'.repeat(32),{id:'e'.repeat(32),binding:'fixture',revision:'task-r1',state:'pending',can_dismiss:true,title:'Observation',label:'#Worker',preview:'Observe',body:'Observe this',next_offset:null});
  await dismissWhileRecording.ui.refresh();dismissWhileRecording.capture();dismissWhileRecording.nodes.get('#conversation-message').value='Keep my text too';
  const attention=dismissWhileRecording.nodes.get('#conversation-task-cards').children[0];
  await attention.children.find(child=>child.textContent==='Dismiss').handlers.click();
  assert.equal(dismissWhileRecording.taskPosts.length,1);assert.equal(dismissWhileRecording.nodes.get('#conversation-message').value,'Keep my text too','Dismiss must preserve the occupied draft');
  assert.equal(submittedClips.length,0,'Dismiss must not send/discard the recording');
  await dismissWhileRecording.nodes.get('#conversation-compose').handlers.submit({preventDefault(){}});
  assert.deepEqual(submittedClips,[recordingId],'The same recording still owns Send after dismissal');
  assert.equal(dismissWhileRecording.nativePosts.length,0,'Dismissing attention is not a native question answer');dismissWhileRecording.ui.stop();

  const followed=boot(false,true);followed.audio();await followed.ui.refresh();
  const lane=followed.nodes.get('#conversation-timeline'),followSwitch=followed.nodes.get('#conversation-follow');
  const scrolls=[];lane.contains=card=>lane.children.includes(card);lane.getBoundingClientRect=()=>({top:100});lane.scrollTop=0;
  lane.scrollTo=value=>{scrolls.push(value);lane.scrollTop=value.top;};
  const spokenCard=n=>lane.children.find(card=>card.dataset.sourceId===`native:${n}`);
  const listenButton=card=>card.children.flatMap(child=>child.children||[]).find(child=>['Listen','Resume audio','Skip'].includes(child.textContent));
  for(const cancel of [false,true]){
    let resolveStream;const streamDone=new Promise(resolve=>{resolveStream=resolve;});
    const fallback=boot(false,true,{incremental:true,progressive:true,streamDone});fallback.audio();await fallback.ui.refresh();
    const card=fallback.nodes.get('#conversation-timeline').children.find(c=>c.dataset.sourceId==='native:8');
    await listenButton(card).handlers.click();const player=fallback.nodes.get('#conversation-audio');player.currentTime=3;
    const calls=fallback.audioCalls.length;player.paused=true;player.handlers.error();
    if(cancel)fallback.ui.stop();
    resolveStream({complete:true,blob:new Blob(['verified encoded sample'],{type:'audio/mpeg'})});
    await new Promise(setImmediate);await new Promise(setImmediate);
    assert.equal(fallback.audioCalls.length,calls,'Decoder fallback never requests synthesis again');
    if(cancel)assert.equal(player.paused,true,'Stop owns a late verified fallback');
    else {await until(()=>!player.paused&&!player.muted);assert.equal(player.currentTime,3,'Decoder fallback preserves the media cursor');assert.equal(fallback.streamedTransports[0].cancelled,true);}
    fallback.ui.stop();
  }
  const buffering=boot(false,true,{incremental:true,progressive:true});buffering.audio();await buffering.ui.refresh();
  const heldBuffer=buffering.holdReplyText('native:8');
  const bufferingCard=buffering.nodes.get('#conversation-timeline').children.find(card=>card.dataset.sourceId==='native:8');
  const preparing=listenButton(bufferingCard).handlers.click();await heldBuffer.entered;
  assert.equal(buffering.streamedTransports.length,1);buffering.ui.pause();
  assert.equal(buffering.streamedTransports[0].signal.aborted,false,'Pause while validating/buffering preserves the one streaming attempt');
  heldBuffer.release();await preparing;
  await buffering.nodes.get('#conversation-audio').beforeNodes[0].handlers.click();
  assert.equal(buffering.audioCalls.length,1,'Play resumes the same buffered stream without another preparation');
  buffering.ui.stop();assert.equal(buffering.streamedTransports[0].cancelled,true,'Stop still cancels the owned stream');
  spokenCard(8).getBoundingClientRect=()=>({top:300});spokenCard(7).getBoundingClientRect=()=>({top:450});
  assert.equal(followSwitch["aria-pressed"],"true",'Follow is enabled by default');
  await listenButton(spokenCard(8)).handlers.click();
  assert.equal(scrolls.length,0,'Voice off preserves the reader position even with Follow enabled');
  followed.nodes.get('#conversation-voice-replies').handlers.click();followed.nodes.get('#conversation-voice-replies').handlers.click();
  followed.ui.pause();followSwitch.handlers.click();await listenButton(spokenCard(8)).handlers.click();assert.equal(scrolls.length,0,'Follow off never moves the chat');
  followSwitch.handlers.click();assert.equal(scrolls.length,1);assert.equal(scrolls[0].top,200,'Follow targets the verified spoken answer within the timeline');
  await listenButton(spokenCard(7)).handlers.click();assert.equal(scrolls.length,2,'A new answer follows its own source');
  followSwitch.handlers.click();await listenButton(spokenCard(8)).handlers.click();assert.equal(scrolls.length,2,'Disabling Follow preserves the reader position');
  followed.nodes.get('#conversation-voice-replies').handlers.click();followSwitch.handlers.click();
  await listenButton(spokenCard(7)).handlers.click();
  assert.equal(scrolls.length,2,'Turning Voice off also prevents subsequent Follow movement');followed.ui.stop();

  const playbackHint=boot(false,true,{capture:true,playback:true});playbackHint.audio();
  playbackHint.overrideSource(8,{revision:'a'.repeat(64)});await playbackHint.ui.refresh();
  const hintedCard=playbackHint.nodes.get('#conversation-timeline').children.find(c=>c.dataset.sourceId==='native:8');
  await listenButton(hintedCard).handlers.click();const hintedAudio=playbackHint.nodes.get('#conversation-audio');hintedAudio.currentTime=4.567;hintedAudio.duration=100;
  assert.deepEqual(JSON.parse(JSON.stringify(playbackHint.playbackContext())),{source:'native:8',revision:'a'.repeat(64),segment_offset:0,position_millis:4567,duration_millis:100000,state:'interrupted'});
  playbackHint.nodes.get('#conversation-message').value='Start a different topic';
  await playbackHint.nodes.get('#conversation-compose').handlers.submit({preventDefault(){}});
  const capturedHint=[...playbackHint.originals.values()][0];assert.equal(capturedHint.playback.state,'playing');
  assert.equal(capturedHint.playback.position_millis,4567);assert.equal(capturedHint.reply,undefined,'Media context cannot choose a reply destination');
  playbackHint.capture();assert.equal(hintedAudio.paused,true);playbackHint.finishCapture();playbackHint.ui.stop();
  const oldReceiver=boot(false,true,{capture:true});oldReceiver.audio();await oldReceiver.ui.refresh();
  const oldCard=oldReceiver.nodes.get('#conversation-timeline').children.find(c=>c.dataset.sourceId==='native:8');await listenButton(oldCard).handlers.click();
  assert.equal(oldReceiver.playbackContext(),undefined,'Older receivers never get an unsupported metadata field');oldReceiver.ui.stop();
  for(const capable of [false,true]){
    const timed=boot(false,true,{capture:true,playback:true,progressive:true,incremental:true,wordTiming:capable,wordPosition:seconds=>seconds>=4?20:0});
    timed.audio();timed.replyTexts.set('native:8','A longer canonical answer for timed speech. '.repeat(5));await timed.ui.refresh();
    const card=timed.nodes.get('#conversation-timeline').children.find(c=>c.dataset.sourceId==='native:8');
    await listenButton(card).handlers.click();const audio=timed.nodes.get('#conversation-audio');audio.currentTime=5;
    assert.equal(timed.playbackContext().word_offset,capable?20:undefined,'New cursor requires actual native capability');
    audio.currentTime=1;assert.equal(timed.playbackContext().word_offset,capable?0:undefined,'Backward seeking must not retain the maximum reported word');
    timed.capture();assert.equal(audio.paused,true);timed.finishCapture();timed.ui.stop();
  }
  // Explicit Resume uses the provider word cursor, never a duration estimate.
  for(const aligned of [true,false,'invalid']){
    const storage=new Map(),options={capture:true,playback:true,progressive:true,incremental:true,wordTiming:true,followStorage:storage,wordPosition:(seconds,offset)=>aligned==='invalid'?offset+9000:aligned?offset+(seconds>=4?20:0):undefined};
    const resumed=boot(false,true,options);resumed.audio();const text='A retained canonical response. '.repeat(20);resumed.replyTexts.set('native:8',text);resumed.overrideSource(8,{text_characters:text.length});await resumed.ui.refresh();
    const card=resumed.nodes.get('#conversation-timeline').children.find(c=>c.dataset.sourceId==='native:8');await listenButton(card).handlers.click();
    const player=resumed.nodes.get('#conversation-audio');player.currentTime=5;player.duration=10;resumed.ui.stop();
    const checkpoint=JSON.parse(storage.get('interaction-speech-position:fixture'))[0];
    assert.equal(checkpoint.offset,aligned===true?20:0,'Use only an available bounded provider word-end position');
    const reloaded=boot(false,true,{...options,followStorage:storage});reloaded.audio();reloaded.replyTexts.set('native:8',text);reloaded.overrideSource(8,{text_characters:text.length});await reloaded.ui.refresh();
    const nextCard=reloaded.nodes.get('#conversation-timeline').children.find(c=>c.dataset.sourceId==='native:8');await listenButton(nextCard).handlers.click();
    assert.equal(reloaded.audioCalls[0].offset,checkpoint.offset,'Reload and explicit Resume retain the exact selected source boundary');
    reloaded.ui.stop();
  }
  {
    const storage=new Map(),seek=boot(false,true,{progressive:true,incremental:true,wordTiming:true,followStorage:storage,wordPosition:seconds=>seconds>=4?20:0});seek.audio();seek.replyTexts.set('native:8','A source with aligned words. '.repeat(10));await seek.ui.refresh();
    const card=seek.nodes.get('#conversation-timeline').children.find(c=>c.dataset.sourceId==='native:8');await listenButton(card).handlers.click();const player=seek.nodes.get('#conversation-audio');
    player.currentTime=5;seek.ui.pause();assert.equal(JSON.parse(storage.get('interaction-speech-position:fixture'))[0].offset,20);
    player.currentTime=1;seek.ui.stop();assert.equal(JSON.parse(storage.get('interaction-speech-position:fixture'))[0].offset,0,'A backward seek keeps the current word cursor rather than its old high-water mark');
  }
  const updatesFollow=boot(false,true);updatesFollow.audio();
  updatesFollow.overrideSource(8,{kind:'worker_progress',target:{chat_id:100,thread_id:77},label:'Alpha'});
  await updatesFollow.ui.refresh();updatesFollow.nodes.get('#conversation-voice-replies').handlers.click();
  const updateLane=updatesFollow.nodes.get('#conversation-timeline');let updateScrolls=0;
  updateLane.contains=card=>updateLane.children.includes(card);updateLane.getBoundingClientRect=()=>({top:100});updateLane.scrollTop=0;
  updateLane.scrollTo=()=>{updateScrolls++;};
  const updateCard=updateLane.children.find(card=>card.children[0]?.children[0]?.textContent==='Update from Alpha');assert(updateCard);
  updateCard.getBoundingClientRect=()=>({top:350});
  const updateListen=updateCard.children.find(child=>child.textContent==='Listen');assert(updateListen&&!updateListen.hidden,'Visible progress updates expose Listen');
  await updateListen.handlers.click();
  assert.equal(updatesFollow.audioCalls.at(-1).source,'native:8','Progress Listen selects the exact retained update');
  assert.equal(updateScrolls,1,'Follow targets the visible Update from card for the exact spoken source');
  const updateAttempts=updatesFollow.audioCalls.length;await updateListen.handlers.click();
  assert.equal(updatesFollow.audioCalls.length,updateAttempts,'Skip cancels this update without repeating synthesis');assert.equal(updateListen.textContent,'Resume audio');updatesFollow.ui.stop();
  const readonlyUpdate=boot(true,true);readonlyUpdate.overrideSource(8,{kind:'worker_progress',target:{chat_id:100,thread_id:77},label:'Alpha'});await readonlyUpdate.ui.refresh();
  const readonlyProgress=readonlyUpdate.nodes.get('#conversation-timeline').children.find(card=>card.dataset.sourceId==='native:8');
  const readonlyListen=readonlyProgress.children.find(child=>child.textContent==='Listen');assert(readonlyListen.hidden);await readonlyListen.handlers.click();assert.equal(readonlyUpdate.audioCalls.length,0);

  {
  const rememberedOff=new Map([['interaction-follow-audio','false']]);
  const restoredFollow=boot(false,true,{followStorage:rememberedOff});
  assert.equal(restoredFollow.nodes.get('#conversation-follow')["aria-pressed"],"false",'The toggle restores the existing disabled preference');
  restoredFollow.nodes.get('#conversation-follow').handlers.click();
  assert.equal(rememberedOff.get('interaction-follow-audio'),'true','The button persists its enabled preference');
  assert.equal(restoredFollow.nodes.get('#conversation-follow')["aria-pressed"],"true");
  const followIcon=restoredFollow.nodes.get('#conversation-follow');let helpPrevented=false;
  followIcon.handlers.contextmenu({preventDefault(){helpPrevented=true;}});
  assert(helpPrevented);assert.match(restoredFollow.nodes.get('#conversation-audio-hint').textContent,/^Follow:/);
  assert.equal(followIcon["aria-expanded"],"true");
  followIcon.handlers.click();
  assert.equal(followIcon["aria-pressed"],"true",'The click following long press cannot toggle Follow');
  assert.equal(rememberedOff.get('interaction-follow-audio'),'true','Help preserves the saved Follow preference');
  assert.equal(restoredFollow.audioCalls.length,0,'Follow help never synthesizes or plays audio');restoredFollow.ui.stop();
  const followPreference=new Map(),tracked=boot(false,true,{clock:true,followStorage:followPreference});tracked.audio(true);
  const longText='🙂'.repeat(3200);
  tracked.replyTexts.set('native:8',longText);
  tracked.overrideSource(8,{preview:longText.slice(0,40),text_characters:3200});
  await tracked.ui.refresh();tracked.nodes.get('#conversation-voice-replies').handlers.click();
  const trackLane=tracked.nodes.get('#conversation-timeline'),trackPlayer=tracked.nodes.get('#conversation-audio');
  const trackCard=trackLane.children.find(card=>card.dataset.sourceId==='native:8');
  const trackBody=trackCard.children.find(child=>child.className==='reply-text');
  trackBody.firstChild={};
  trackLane.contains=card=>trackLane.children.includes(card);trackLane.scrollTop=0;
  trackLane.getBoundingClientRect=()=>({top:100,bottom:300,height:200,right:390});
  trackCard.getBoundingClientRect=()=>({top:300-trackLane.scrollTop});
  const trackingScrolls=[],ranges=[];
  trackLane.scrollTo=value=>{trackingScrolls.push(value);trackLane.scrollTop=value.top;};
  tracked.rangeGeometry(()=>{
    let offset;
    return {setStart(_node,value){offset=value;},setEnd(){},getBoundingClientRect(){
      ranges.push(offset);const top=400+Math.floor(offset/100)*20-trackLane.scrollTop;
      return {top,bottom:top+20,height:20};
    }};
  });
  await listenButton(trackCard).handlers.click();
  trackPlayer.duration=100;trackPlayer.currentTime=80;trackPlayer.handlers.timeupdate();
  await until(()=>ranges.length>0);
  assert.equal(tracked.partReads.filter(value=>value.source==='native:8').length,1,'Follow reuses the segment already validated for playback');
  assert.equal(ranges.at(-1),Math.floor(1599*.8)*2,'Canonical Unicode offsets become actual DOM UTF-16 offsets');
  assert(trackingScrolls.at(-1).top>200,'Long spoken segments advance the reading window beyond the answer beginning');
  const stablePosition=trackingScrolls.length;trackPlayer.duration=Infinity;trackPlayer.handlers.timeupdate();
  assert.equal(trackingScrolls.length,stablePosition,'Unknown duration never invents progress or resets the reader');trackPlayer.duration=100;
  const readCount=tracked.partReads.length;
  trackPlayer.handlers.timeupdate();assert.equal(tracked.partReads.length,readCount,'Media ticks reuse loaded immutable text');
  tracked.ui.pause();const pausedScrolls=trackingScrolls.length;trackPlayer.currentTime=90;trackPlayer.handlers.timeupdate();
  assert.equal(trackingScrolls.length,pausedScrolls,'Pause stops following as well as audio');
  await trackPlayer.beforeNodes[0].handlers.click();
  assert(trackingScrolls.slice(pausedScrolls).every(value=>value.top!==200),'Resume does not jump back to the answer beginning');
  const beforeHidden=trackingScrolls.length;tracked.hide();trackPlayer.currentTime=95;trackPlayer.handlers.timeupdate();
  assert.equal(trackingScrolls.length,beforeHidden,'Background playback does not move a hidden conversation');
  tracked.show();await until(()=>!tracked.nodes.get('#conversation-refresh').disabled);
  trackPlayer.ended=true;const trackEnded=trackPlayer.handlers.ended();tracked.clock.fire(180);await trackEnded;
  assert.equal(tracked.audioCalls.at(-1).offset,1600,'Multipart continuation retains its exact canonical offset');
  assert(trackingScrolls.slice(1).every(value=>value.top!==200),'Later chunks never restart following at the answer beginning');
  trackPlayer.duration=100;trackPlayer.currentTime=80;
  tracked.reducedMotion(true);trackPlayer.handlers.timeupdate();
  await until(()=>ranges.at(-1)>3200);
  assert.equal(trackingScrolls.at(-1).behavior,'instant','Reduced motion never introduces animated following');
  const preferenceBefore=followPreference.get('interaction-follow-audio');
  trackLane.handlers.wheel();assert.equal(followPreference.get('interaction-follow-audio'),preferenceBefore,'Manual reading does not overwrite the explicit Follow preference');assert.equal(tracked.nodes.get('#conversation-follow')["aria-pressed"],"true",'A gesture never switches Follow off');
  const manualScrolls=trackingScrolls.length;trackPlayer.currentTime=95;trackPlayer.handlers.timeupdate();
  assert.equal(trackingScrolls.length,manualScrolls,'Following waits while the person scrolls');
  tracked.clock.fire(180);assert(trackingScrolls.length>manualScrolls,'Following returns to spoken progress after scrolling settles');
  trackLane.handlers.pointerdown({pointerId:7});const heldScrolls=trackingScrolls.length;trackPlayer.currentTime=99;trackPlayer.handlers.timeupdate();
  assert.equal(trackingScrolls.length,heldScrolls,'A held finger owns the viewport');
  tracked.event('pointerup',{pointerId:7});assert(trackingScrolls.length>heldScrolls,'Releasing the finger resumes Follow');
  tracked.nodes.get('#conversation-follow').handlers.click();
  const explicitOff=trackingScrolls.length;trackLane.handlers.pointerdown({pointerId:8});tracked.event('pointercancel',{pointerId:8});trackPlayer.handlers.timeupdate();
  assert.equal(trackingScrolls.length,explicitOff,'Explicitly disabled Follow stays disabled after a gesture');
  tracked.nodes.get('#conversation-follow').handlers.click();
  const beforeRevision=trackingScrolls.length;
  tracked.overrideSource(8,{revision:'new-revision',text_sha256:'new-content'});await tracked.ui.refresh();
  trackPlayer.handlers.timeupdate();assert.equal(trackingScrolls.length,beforeRevision,'A changed source cannot keep driving Follow');tracked.ui.stop();
  }

  const telegramChat=boot(false,true);
  telegramChat.telegramOriginals.set('telegram:42',{id:'telegram:42',preview:'A human Telegram message',phase:'completed',sequence:42,capture_order:500,received_at:'2024-01-01T00:00:05.500Z',origin_kind:'telegram',read_only:true});
  await telegramChat.ui.refresh();
  const telegramLane=telegramChat.nodes.get('#conversation-timeline'),telegramBubble=telegramLane.children.find(card=>card.dataset.timelineKey==='input:telegram:42');
  assert(telegramBubble,'Standalone chat includes Telegram input without opening diagnostics');
  assert.equal(telegramBubble.children[0].textContent,'You');
  assert.equal(telegramBubble.dataset.timelineTime,String(Date.parse('2024-01-01T00:00:05.500Z')));
  const telegramCount=telegramLane.children.length;await telegramChat.ui.refresh();assert.equal(telegramLane.children.length,telegramCount,'Refresh does not duplicate Telegram originals');
  assert(telegramChat.requests.filter(r=>r.telegram).every(r=>r.body===undefined),'Cross-surface observation never resubmits an original');telegramChat.ui.stop();
  const telegramDenied=boot(false,true,{telegramDenied:true});
  await telegramDenied.ui.refresh();
  const retainedReplies=telegramDenied.nodes.get('#conversation-timeline').children.length;
  assert(retainedReplies>0,'Denied optional Telegram observation preserves visible replies');
  telegramDenied.nodes.get('#conversation-message').value='Unsent draft';
  await telegramDenied.ui.refresh();
  assert.equal(telegramDenied.nodes.get('#conversation-message').value,'Unsent draft');
  assert.equal(telegramDenied.nodes.get('#conversation-timeline').children.length,retainedReplies);
  telegramDenied.ui.stop();

  const copyFixture=boot(false,true,{capture:true});await copyFixture.ui.refresh();
  const copyLane=copyFixture.nodes.get('#conversation-timeline');
  const copyCard=copyLane.children.find(c=>c.dataset.sourceId==='native:8');
  const copyAction=box=>{const visit=n=>n.className==='message-copy'?n:(n.children||[]).map(visit).find(Boolean);return visit(box);};
  const fullCopy='🙂 **Whole message**\n'+('A paragraph with code `git status`.\n'.repeat(110));
  copyFixture.replyTexts.set('native:8',fullCopy);copyFixture.overrideSource(8,{preview:'🙂 Preview',text_characters:Array.from(fullCopy).length});await copyFixture.ui.refresh();
  await copyAction(copyCard).handlers.click();assert.equal(copyFixture.copied.at(-1),fullCopy,'Copy resolves all exact Unicode pages, preserving source text rather than its preview');
  assert.equal(copyAction(copyCard).textContent,'Copied');assert(copyFixture.partReads.filter(v=>v.source==='native:8').length>1);
  copyFixture.capturedInput({id:'11111111-1111-4111-8111-111111111111',text:'My full message 🙂',phase:'accepted'});
  const ownCopy=copyLane.children.find(c=>c.dataset.timelineKey==='input:11111111-1111-4111-8111-111111111111');await copyAction(ownCopy).handlers.click();assert.equal(copyFixture.copied.at(-1),'My full message 🙂');
  copyFixture.overrideSource(9,{kind:'worker_progress',target:{chat_id:100,thread_id:7},preview:'Update preview',text_characters:30});copyFixture.addReplies(1);copyFixture.replyTexts.set('native:9','The complete update, not preview');await copyFixture.ui.refresh();
  const copiedUpdate=copyLane.children.find(c=>c.dataset.sourceId==='native:9');await copyAction(copiedUpdate).handlers.click();assert.equal(copyFixture.copied.at(-1),'The complete update, not preview');
  const historicalId='22222222-2222-4222-8222-222222222222';copyFixture.originals.set(historicalId,{id:historicalId,text:'Historical complete input 🙂',phase:'accepted'});copyFixture.capturedInput({id:historicalId,preview:'Historical preview',phase:'accepted'});
  const historicalCard=copyLane.children.find(c=>c.dataset.timelineKey==='input:'+historicalId);await copyAction(historicalCard).handlers.click();assert.equal(copyFixture.copied.at(-1),'Historical complete input 🙂','History preview resolves the original text endpoint');
  copyFixture.nativeRequest();await copyFixture.ui.refresh();const questionCopy=copyFixture.nodes.get('#conversation-request-cards').children[0];await copyAction(questionCopy).handlers.click();assert.equal(copyFixture.copied.at(-1),'Choose scope');assert.equal(copyFixture.nativePosts.length,0,'Copying a question cannot answer or approve it');
  const copiedBeforeChange=copyFixture.copied.length,heldCopy=copyFixture.holdReplyText('native:8'),copying=copyAction(copyCard).handlers.click();await heldCopy.entered;copyFixture.overrideSource(8,{revision:'new-copy-revision'});await copyFixture.ui.refresh();heldCopy.release();await copying;assert.equal(copyFixture.copied.length,copiedBeforeChange,'Revision change during paging cannot copy a partial or mixed message');assert.equal(copyAction(copyCard).textContent,'Copy failed');
  const copyTask={id:'copy-task',binding:'fixture',revision:'task-copy-rev',label:'Conversation',title:'A human action',body:'Complete human action body',preview:'Action preview',state:'pending',can_dismiss:true};copyFixture.humanTasks.set(copyTask.id,copyTask);await copyFixture.ui.refresh();await copyAction(copyFixture.nodes.get('#conversation-task-cards').children[0]).handlers.click();assert.equal(copyFixture.copied.at(-1),copyTask.body);assert.equal(copyFixture.taskPosts.length,0,'Copy cannot dismiss, reply to or complete a human action');
  assert.equal(copyFixture.audioCalls.length,0,'Copy never synthesizes speech');assert.equal(copyFixture.recoveryPosts.length,0,'Copy never resends input');copyFixture.ui.stop();
  const richCopy=boot(false,true,{copyRich:true});await richCopy.ui.refresh();await copyAction(richCopy.nodes.get('#conversation-timeline').children.find(c=>c.dataset.sourceId==='native:8')).handlers.click();assert.equal(richCopy.copied[0],'Selected reply','ClipboardItem accepts a promised full-text Blob');richCopy.ui.stop();
  const deniedCopy=boot(false,true,{copyDenied:true});await deniedCopy.ui.refresh();const deniedCard=deniedCopy.nodes.get('#conversation-timeline').children.find(c=>c.dataset.sourceId==='native:8');await copyAction(deniedCard).handlers.click();assert.equal(copyAction(deniedCard).textContent,'Copy failed');assert.equal(deniedCopy.copied.length,0);deniedCopy.ui.stop();

  const noPlayer=boot(false,false,{clock:true});noPlayer.audio();await noPlayer.ui.refresh();
  noPlayer.nodes.get('#conversation-voice-replies').handlers.click();noPlayer.failAudio();noPlayer.addReplies(2);await noPlayer.ui.refresh();
  await until(()=>noPlayer.clock.pending().includes(450));
  assert.equal(noPlayer.nodes.get('#conversation-player').hidden,false,'Audio controls remain visible after failure');
  noPlayer.ui.pause();noPlayer.clock.fire(450);await noPlayer.ui.refresh();
  assert.equal(noPlayer.audioCalls.length,1,'Pause cancels an interreply gap even without an active audio element');noPlayer.ui.stop();

  const reconnectVoice=boot(false,false,{clock:true});reconnectVoice.audio();await reconnectVoice.ui.refresh();
  reconnectVoice.nodes.get('#conversation-voice-replies').handlers.click();reconnectVoice.addReplies(2);await reconnectVoice.ui.refresh();
  const rp=reconnectVoice.nodes.get('#conversation-audio');await until(()=>reconnectVoice.audioCalls.length===1&&!rp.paused);
  rp.ended=true;await rp.handlers.ended();reconnectVoice.event('offline');reconnectVoice.clock.fire(450);
  reconnectVoice.addReplies(2);reconnectVoice.event('online');
  await until(()=>!reconnectVoice.nodes.get('#conversation-refresh').disabled);await reconnectVoice.ui.refresh();
  assert.equal(reconnectVoice.audioCalls.length,1,'Reconnect must not resume old queued audio or arrivals during suspension');
  assert.equal(reconnectVoice.nodes.get('#conversation-voice-replies').dataset.voiceState,'on');
  assert.equal(reconnectVoice.nodes.get('#conversation-voice-replies')['aria-pressed'],'true');
  assert.equal(reconnectVoice.nodes.get('#conversation-voice-replies')['aria-label'],'Stop audio and disable spoken replies');
  reconnectVoice.nodes.get('#conversation-voice-replies').handlers.click();reconnectVoice.nodes.get('#conversation-voice-replies').handlers.click();
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
  const muteStorage=new Map(),muted=boot(false,true,{clock:true,followStorage:muteStorage});muted.audio();
  const worker=(thread,label)=>({kind:'worker_result',label,target:{chat_id:100,thread_id:thread},current:true,native_binding_current:true});
  muted.overrideSource(8,{...worker(7,'Alpha'),kind:'worker_progress'});await muted.ui.refresh();
  const muteControl=(fixture,id)=>fixture.nodes.get('#conversation-timeline').children.find(c=>c.dataset.sourceId===id&&!c.removed).children[0].children.flatMap(c=>[c,...c.children]).find(c=>c.className==='session-audio-mute');
  const alphaMute=muteControl(muted,'native:8');assert(alphaMute);alphaMute.handlers.click();
  assert.equal(alphaMute['aria-pressed'],'true');assert.match(alphaMute['aria-label'],/Unmute.*Alpha/);
  const muteHeading=muted.nodes.get('#conversation-timeline').children.find(c=>c.dataset.sourceId==='native:8').children[0],attached=muteHeading.appendCalls;
  await muted.ui.refresh();assert.equal(muteHeading.appendCalls,attached,'Live refresh never detaches/re-appends the focused mute button');
  muted.nodes.get('#conversation-voice-replies').handlers.click();
  muted.overrideSource(9,worker(7,'Alpha'));muted.overrideSource(10,worker(8,'Beta'));muted.addReplies(2);await muted.ui.refresh();
  const mutedPlayer=muted.nodes.get('#conversation-audio');await until(()=>muted.audioCalls.length===1&&!mutedPlayer.paused);
  assert.equal(muted.audioCalls[0].source,'native:10','Muted Alpha stays visible while Beta is spoken');
  assert(muted.nodes.get('#conversation-timeline').children.some(c=>c.dataset.sourceId==='native:9'));
  muteControl(muted,'native:9').handlers.click();mutedPlayer.ended=true;await mutedPlayer.handlers.ended();muted.clock.fire(450);
  await Promise.resolve();assert.equal(muted.audioCalls.length,1,'Unmuting does not replay the silent backlog');
  muted.overrideSource(11,worker(7,'Renamed Alpha'));muted.addReplies(1);await muted.ui.refresh();
  await until(()=>muted.audioCalls.length===2&&!mutedPlayer.paused);
  muteControl(muted,'native:11').handlers.click();assert.equal(mutedPlayer.paused,true,'Mute stops the selected automatic playback');
  assert.equal(muted.nodes.get('#conversation-voice-replies').dataset.voiceState,'on','Mute does not pause global Voice or native work');
  muted.overrideSource(12,worker(7,'Renamed Alpha'));muted.overrideSource(13,worker(8,'Beta'));muted.addReplies(2);await muted.ui.refresh();
  await until(()=>muted.audioCalls.length===3&&!mutedPlayer.paused);assert.equal(muted.audioCalls[2].source,'native:13');
  const manualCard=muted.nodes.get('#conversation-timeline').children.find(c=>c.dataset.sourceId==='native:12');
  manualCard.children[3].children[1].handlers.click();await until(()=>muted.audioCalls.length===4&&!mutedPlayer.paused);
  assert.equal(muted.audioCalls[3].source,'native:12','Explicit Listen remains available for a muted session');muted.ui.stop();
  const restoredMute=boot(false,true,{followStorage:muteStorage});restoredMute.overrideSource(8,{...worker(7,'New label'),kind:'worker_progress'});await restoredMute.ui.refresh();
  assert.equal(muteControl(restoredMute,'native:8')['aria-pressed'],'true','Stable session identity and binding-scoped preference survive reload');restoredMute.ui.stop();
  const otherBinding=boot(false,true,{followStorage:new Map([['interaction-session-mutes:another-binding',JSON.stringify(['[100,7]'])]])});
  otherBinding.overrideSource(8,{...worker(7,'Alpha'),kind:'worker_progress'});await otherBinding.ui.refresh();
  assert.equal(muteControl(otherBinding,'native:8')['aria-pressed'],'false','Another conversation binding cannot mute this one');otherBinding.ui.stop();
  const mixStorage=new Map([['interaction-session-mutes:fixture',JSON.stringify(['[100,7]'])]]);
  const mix=boot(false,true,{clock:true,followStorage:mixStorage});mix.audio();
  mix.overrideSource(8,{...worker(7,'Alpha'),kind:'worker_progress'});await mix.ui.refresh();
  const filterControl=(fixture,id,name)=>fixture.nodes.get('#conversation-timeline').children.find(c=>c.dataset.sourceId===id&&!c.removed).children[0].children.flatMap(c=>[c,...c.children]).find(c=>c.className===`session-audio-${name}`);
  assert.equal(muteControl(mix,'native:8')['aria-pressed'],'true','Existing mute-only preferences migrate on first filter change');
  filterControl(mix,'native:8','solo').handlers.click();
  assert.equal(muteControl(mix,'native:8')['aria-pressed'],'false','Solo clears previous explicit mutes');
  assert.equal(filterControl(mix,'native:8','solo')['aria-pressed'],'true');
  assert.equal(filterControl(mix,'native:8','reset').hidden,false);
  mix.nodes.get('#conversation-voice-replies').handlers.click();
  mix.overrideSource(9,worker(7,'Alpha'));mix.overrideSource(10,worker(8,'Beta'));mix.addReplies(2);await mix.ui.refresh();
  const mixPlayer=mix.nodes.get('#conversation-audio');await until(()=>mix.audioCalls.length===1&&!mixPlayer.paused);
  assert.equal(mix.audioCalls[0].source,'native:9','Solo speaks Alpha and leaves Beta visible and silent');
  assert(mix.nodes.get('#conversation-timeline').children.some(c=>c.dataset.sourceId==='native:10'));
  filterControl(mix,'native:10','solo').handlers.click();
  assert.equal(filterControl(mix,'native:9','solo')['aria-pressed'],'true','Multiple solos form the audible selection');
  assert.equal(filterControl(mix,'native:10','solo')['aria-pressed'],'true');
  filterControl(mix,'native:9','solo').handlers.click();
  assert.equal(mixPlayer.paused,true,'Removing the active track from Solo stops its automatic audio');
  assert.equal(mix.nodes.get('#conversation-voice-replies').dataset.voiceState,'on');
  const persistedMix=boot(false,true,{followStorage:mixStorage});
  persistedMix.overrideSource(8,{...worker(8,'Renamed Beta'),kind:'worker_progress'});await persistedMix.ui.refresh();
  assert.equal(filterControl(persistedMix,'native:8','solo')['aria-pressed'],'true','Solo survives reload under stable session identity');persistedMix.ui.stop();
  filterControl(mix,'native:10','reset').handlers.click();
  assert.equal(filterControl(mix,'native:10','solo')['aria-pressed'],'false');
  assert.equal(filterControl(mix,'native:9','reset').hidden,true,'Reset restores the default mix everywhere');
  await Promise.resolve();assert.equal(mix.audioCalls.length,1,'Reset does not replay the previously silent backlog');
  mix.overrideSource(11,worker(7,'Alpha'));mix.addReplies(1);await mix.ui.refresh();
  await until(()=>mix.audioCalls.length===2&&!mixPlayer.paused);
  assert.equal(mix.audioCalls[1].source,'native:11','Reset restores future audio from previously filtered sessions');mix.ui.stop();
  const resetMix=boot(false,true,{followStorage:mixStorage});resetMix.overrideSource(8,{...worker(8,'Beta'),kind:'worker_progress'});await resetMix.ui.refresh();
  assert.equal(filterControl(resetMix,'native:8','reset').hidden,true,'Reset persists across reload');resetMix.ui.stop();
  const conversationStorage=new Map();
  const conversationMix=boot(false,true,{clock:true,followStorage:conversationStorage});conversationMix.audio();await conversationMix.ui.refresh();
  filterControl(conversationMix,'native:8','solo').handlers.click();
  assert.equal(filterControl(conversationMix,'native:8','solo')['aria-label'],'Remove solo from automatic audio updates from Conversation');
  conversationMix.nodes.get('#conversation-voice-replies').handlers.click();
  conversationMix.overrideSource(9,worker(7,'Alpha'));conversationMix.addReplies(2);await conversationMix.ui.refresh();
  const conversationPlayer=conversationMix.nodes.get('#conversation-audio');
  await until(()=>conversationMix.audioCalls.length===1&&!conversationPlayer.paused);
  assert.equal(conversationMix.audioCalls[0].source,'native:10','Solo Conversation excludes internal sessions');
  filterControl(conversationMix,'native:10','reset').handlers.click();
  filterControl(conversationMix,'native:10','mute').handlers.click();
  assert.equal(conversationPlayer.paused,true,'Mute Conversation stops its current automatic audio');
  conversationMix.overrideSource(11,worker(7,'Alpha'));conversationMix.addReplies(2);await conversationMix.ui.refresh();
  await until(()=>conversationMix.audioCalls.length===2&&!conversationPlayer.paused);
  assert.equal(conversationMix.audioCalls[1].source,'native:11','Mute Conversation leaves internal-session audio available');
  assert.equal(filterControl(conversationMix,'native:12','mute')['aria-pressed'],'true','Conversation cards share one mute identity');
  conversationMix.ui.stop();
  const restoredConversation=boot(false,true,{followStorage:conversationStorage});await restoredConversation.ui.refresh();
  assert.equal(filterControl(restoredConversation,'native:8','mute')['aria-pressed'],'true','Conversation filter survives reload');
  filterControl(restoredConversation,'native:8','reset').handlers.click();
  assert.equal(filterControl(restoredConversation,'native:8','mute')['aria-pressed'],'false');restoredConversation.ui.stop();
  for(const audioMix of [{muted:['conversation'],solo:[]},{muted:[],solo:['[100,7]']}]){
    const filteredOptions={liveSpeech:true,clock:true,responseDraft:{provisional:true,native_turn:'filtered-physical',native_item:'filtered-item',text:'A useful live sentence. Remaining',observed_at:'2100-01-01T00:00:00Z'},followStorage:new Map([['interaction-session-audio:fixture',JSON.stringify({version:1,...audioMix})]])};
    const filteredLive=boot(false,true,filteredOptions);filteredLive.audio();await filteredLive.ui.refresh();
    filteredLive.nodes.get('#conversation-voice-replies').handlers.click();await filteredLive.ui.refresh();
    await new Promise(resolve=>setTimeout(resolve,20));
    assert.equal(filteredLive.audioCalls.length,0,'The conversation mix also filters direct live-speech selection');filteredLive.ui.stop();
  }
  const questionMix=boot(false,true,{clock:true});questionMix.audio();await questionMix.ui.refresh();
  filterControl(questionMix,'native:8','solo').handlers.click();
  questionMix.nodes.get('#conversation-voice-replies').handlers.click();
  questionMix.nativeRequest("session",{chat_id:100,thread_id:7});await questionMix.ui.refresh();
  await new Promise(resolve=>setTimeout(resolve,20));
  assert.equal(questionMix.audioCalls.length,0,'Solo Conversation also excludes internal pending questions');
  questionMix.ui.stop();
  const localQuestionMix=boot(false,true,{clock:true});localQuestionMix.audio();await localQuestionMix.ui.refresh();
  filterControl(localQuestionMix,'native:8','solo').handlers.click();
  localQuestionMix.nodes.get('#conversation-voice-replies').handlers.click();
  localQuestionMix.nativeRequest();await localQuestionMix.ui.refresh();
  await until(()=>localQuestionMix.audioCalls.length===1);
  assert.equal(localQuestionMix.audioCalls[0].source,'request:native-request','Conversation questions remain audible under Solo Conversation');localQuestionMix.ui.stop();
  const timingEvents=[],timed=boot(false,true,{clock:true,capture:true,timingEvents});timed.audio();await timed.ui.refresh();
  timed.timingCapture('owned-recording','requested');timed.timingCapture('owned-recording','started');
  assert(timingEvents.some(event=>event[0]==='mark'&&event[1]==='owned-recording'&&event[2]==='started'));
  timed.nodes.get('#conversation-voice-replies').handlers.click();timed.addReplies(1);await timed.ui.refresh();
  const timedPlayer=timed.nodes.get('#conversation-audio');await until(()=>!timedPlayer.paused);
  const beforePlaying=timingEvents.filter(event=>event[0]==='source'&&event[2]===true).length;
  timedPlayer.muted=true;timedPlayer.handlers.playing();assert.equal(timingEvents.filter(event=>event[0]==='source'&&event[2]===true).length,beforePlaying,'Muted source validation is not playback timing');
  timedPlayer.muted=false;timedPlayer.handlers.playing();assert(timingEvents.some(event=>event[0]==='source'&&event[2]===true&&event[1].id==='native:9'));timed.ui.stop();
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
  // Wait for the actual held validation, rather than a microtask count tied to
  // the number of bounded source/cursor observations in refresh.
  await until(()=>fixture.validating());
  assert(fixture.validating(), "The regression must actually hold A's validation");
  second.children[3].children[1].handlers.click();
  rejectOld(new Error("Old source changed"));
  await refresh;
  assert.equal(fixture.nodes.get("#conversation-player").hidden, false,
    "Failure of A must not hide B's player");
  assert.equal(fixture.nodes.get("#conversation-audio-state").textContent, "Preparing audio…");
  const statusButton=fixture.nodes.get("#conversation-audio-state"),statusHint=fixture.nodes.get("#conversation-audio-hint");
  assert.equal(statusButton.dataset.audioState,"preparing");
  const beforeExplain=fixture.audioCalls.length;
  statusButton.handlers.click();assert.equal(statusHint.hidden,false);assert.equal(statusHint.textContent,"Preparing audio…");
  assert.equal(fixture.audioCalls.length,beforeExplain,"Explaining a status must not request or play audio");
  statusButton.handlers.click();assert.equal(statusHint.hidden,true);
  let prevented=false;statusButton.handlers.contextmenu({preventDefault(){prevented=true;}});
  assert(prevented);assert.equal(statusHint.hidden,false);
  assert.equal(fixture.audioCalls.length,beforeExplain,"Long-press explanation must not request audio");
  fixture.event('keydown',{key:'Escape'});assert.equal(statusHint.hidden,true);
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
  for(const guard of [{shiftKey:true},{isComposing:true},{keyCode:229},{ctrlKey:true},{altKey:true},{metaKey:true}]){
    let prevented=false;await draft.handlers.keydown({key:"Enter",...guard,preventDefault(){prevented=true;}});
    assert.equal(sending.originals.size,0,"Newlines and composition must not submit an original");assert.equal(prevented,false);
  }
  sending.lose(); await draft.handlers.keydown({key:"Enter",preventDefault(){}});
  assert.equal(draft.value,"  Original message.  ","Lost acknowledgment must preserve the draft");
  assert.equal(sending.originals.size,1);
  await draft.handlers.keydown({key:"Enter",preventDefault(){}});
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
  // A slow response/question refresh must not keep a saved voice transcript in preparing.
  const confirming=boot();
  confirming.originals.set('confirmed-voice',{id:'confirmed-voice',sequence:1,text:'',phase:'preparing',voice:{audio_sha256:'owned'}});
  await confirming.ui.refresh();
  const slowQuestions=confirming.holdRequests(),slowRefresh=confirming.ui.refresh();await slowQuestions.entered;
  confirming.originals.set('confirmed-voice',{id:'confirmed-voice',sequence:1,text:'Already committed transcript',phase:'accepted',voice:{audio_sha256:'owned',transcription:{capture_id:'confirmed-voice'}}});
  try {
    const statusHold=confirming.holdStatus();
    await confirming.ui.refresh();await statusHold.entered;
    await confirming.ui.refresh();assert.equal(confirming.statusReads.length,1,'One confirmation read at a time');
    statusHold.release();
    await until(()=>confirming.nodes.get('#conversation-inputs').children[0].children[0].textContent.includes('Already committed transcript'));
    assert.equal(confirming.nativePosts.length,0,'Confirmation refresh must not submit native work');
  } finally {slowQuestions.release();await slowRefresh;}
  const lateConfirm=boot();
  lateConfirm.originals.set('late-confirm',{id:'late-confirm',sequence:1,text:'',phase:'preparing',voice:{audio_sha256:'late'}});
  await lateConfirm.ui.refresh();
  const lateQuestions=lateConfirm.holdRequests(),lateRefresh=lateConfirm.ui.refresh();await lateQuestions.entered;
  const lateStatus=lateConfirm.holdStatus();await lateConfirm.ui.refresh();await lateStatus.entered;
  const oldInput=lateConfirm.holdInputs("before");lateQuestions.release();await oldInput.entered;
  lateConfirm.originals.set('late-confirm',{id:'late-confirm',sequence:1,text:'Confirmed while a newer snapshot was stale',phase:'accepted',voice:{audio_sha256:'late',transcription:{capture_id:'late-confirm'}}});
  oldInput.release();await lateRefresh;lateStatus.release();
  await until(()=>lateConfirm.nodes.get('#conversation-inputs').children[0].children[0].textContent.includes('Confirmed while a newer snapshot was stale'));
  const boundedOptions={confirmNow:Date.now()},boundedConfirm=boot(false,false,boundedOptions);
  boundedConfirm.originals.set('still-preparing',{id:'still-preparing',sequence:1,text:'',phase:'preparing',voice:{audio_sha256:'bounded'}});
  await boundedConfirm.ui.refresh();
  const heldConfirm=boundedConfirm.holdRequests(),heldRefresh=boundedConfirm.ui.refresh();await heldConfirm.entered;
  try {
    for(let n=0;n<8;n++){await boundedConfirm.ui.refresh();await new Promise(setImmediate);if(n===0){await boundedConfirm.ui.refresh();assert.equal(boundedConfirm.statusReads.length,1,'Refresh bursts must not bypass confirmation pacing');}boundedOptions.confirmNow+=2000;}
    assert.equal(boundedConfirm.statusReads.length,4,'Saved confirmation checks have a finite per-original budget');
  } finally {heldConfirm.release();await heldRefresh;}
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

// Progressive media transport: prove first append precedes completion, and that
// corruption/Stop cannot append later bytes. No browser/audio device is opened.
(async()=>{
  const crypto=require('node:crypto').webcrypto;
  const until=async fn=>{for(let n=0;n<200;n++){if(fn())return;await new Promise(r=>setTimeout(r,5));}throw Error('Stream fixture state not ready');};
  const hash=async raw=>Buffer.from(await crypto.subtle.digest('SHA-256',raw)).toString('hex');
  async function setup(){
    let latest,revoked=0;
    class Events{constructor(){this.events=new Map();}addEventListener(name,fn){this.events.set(name,fn);}fire(name){this.events.get(name)?.();}}
    class Media extends Events{
      static isTypeSupported(value){return value==='audio/mpeg';}
      constructor(){super();this.readyState='closed';this.appended=[];latest=this;}
      addSourceBuffer(){const buffer=new Events();buffer.updating=false;buffer.appendBuffer=raw=>{buffer.updating=true;this.appended.push(raw);queueMicrotask(()=>{buffer.updating=false;buffer.fire('updateend');});};buffer.abort=()=>{};return buffer;}
      endOfStream(){this.readyState='ended';}
      open(){this.readyState='open';this.fire('sourceopen');}
    }
    const window={MediaSource:Media},context={window,URL:{createObjectURL:()=> 'blob:synthetic-stream',revokeObjectURL(){revoked++;}},crypto,Uint8Array,TextDecoder,Blob,setTimeout,clearTimeout,atob,Error,Promise};
    vm.runInNewContext(fs.readFileSync('dashboard/web/speech-stream.js','utf8'),context);
    let controller;const response=new Response(new ReadableStream({start(value){controller=value;}}));
    const abort=new AbortController(),stream=window.SpeechStream.open(response,abort.signal);latest.open();
    const send=value=>controller.enqueue(new TextEncoder().encode(JSON.stringify(value)+'\n'));
    return {stream,media:latest,send,abort,close:()=>controller.close(),revoked:()=>revoked};
  }
  const raw=new TextEncoder().encode('synthetic encoded audio');
  const audio={type:'audio',sequence:0,data:Buffer.from(raw).toString('base64'),sha256:await hash(raw)};
  const end={type:'end',audio_bytes:raw.length,audio_sha256:await hash(raw)};
  const first=await setup();first.send(audio);await until(()=>first.media.appended.length===1);
  assert.equal(first.media.readyState,'open','First audio appended before terminal result');
  first.send({...end,alignment:{offset:240,words:[[240,244,0,200],[245,246,400,600]]}});first.close();assert.equal((await first.stream.done).complete,true);await until(()=>first.media.readyState==='ended');
  assert.equal(first.stream.position(.3),244);assert.equal(first.stream.position(.7),246);assert.equal(first.stream.position(.1),240,'Backward seek follows actual current media time');
  const decoder=await setup();let decoderErrors=0;decoder.stream.onDecoderError(()=>decoderErrors++);
  decoder.media.readyState='open';decoder.media.addSourceBuffer=()=>{throw Error('Synthetic decoder unavailable');};decoder.media.fire('sourceopen');
  decoder.send(audio);decoder.send(end);decoder.close();const retained=await decoder.stream.done;
  assert.equal(decoderErrors,1);assert(retained.complete&&retained.blob.size===raw.length,'Decoder failure retains only fully verified bytes for local fallback');decoder.stream.cancel();
  const unavailable=await setup();unavailable.send(audio);unavailable.send({...end,alignment:{offset:0,words:[[0,1,200,100]]}});unavailable.close();assert.equal((await unavailable.stream.done).complete,true);assert.equal(unavailable.stream.position(1),undefined,'Bad optional timing does not break successful audio');unavailable.stream.cancel();
  const bad=await setup();bad.send({...audio,sha256:'0'.repeat(64)});const failure=await bad.stream.done;
  assert.equal(failure.blob,undefined,'Incomplete or corrupt transport never supplies local fallback media');
  assert.equal(failure.complete,false);assert.equal(bad.media.appended.length,0,'Corrupt audio is never appended');
  const stopped=await setup();stopped.send(audio);stopped.abort.abort();await stopped.stream.done;
  assert.equal(stopped.media.appended.length,0,'Stop owns late hash/decode completion');assert.equal(stopped.revoked(),1);
  const broken=await setup();broken.send(audio);broken.close();assert.equal((await broken.stream.done).complete,false,'EOF without terminal result stays incomplete');
  first.stream.cancel();assert.equal(first.revoked(),1);
  console.log('Progressive speech transport: first append, exact framing/integrity, terminal completion, cancellation and incomplete EOF passed.');
})().catch(error=>{console.error(error);process.exitCode=1;});
