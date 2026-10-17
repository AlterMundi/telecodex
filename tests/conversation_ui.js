// Exercise the shipped conversation state machine with synthetic DOM/network.
// No native sessions, external providers, or models are called.
const assert = require("node:assert/strict");
const fs = require("node:fs");
const vm = require("node:vm");

function boot() {
  const nodes = new Map(), requests = [];
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
      before() {}, pause() { this.paused = true; }, load() {},
    };
  }
  const document = {
    hidden: false, body: element(), addEventListener() {}, createElement: element,
    querySelector(selector) {
      if (!nodes.has(selector)) nodes.set(selector, element());
      return nodes.get(selector);
    },
  };
  let count = 8, arriveDuringRecent = false, heldValidation;
  const source = n => ({id:`native:${n}`, kind:"integrator_answer", sequence:n,
    revision:`revision-${n}`, text_sha256:`sha-${n}`, preview:`Reply ${n}`});
  const originals = new Map(), nativeRequests = new Map(), nativePosts = [];
  let losePost = false, offlinePost = false, heldPost, heldInputs, heldStatus, submitPhase = "accepted";
  const api = async (route, body) => {
    const url = new URL(route, "https://fixture.invalid/");
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
    if (url.pathname.startsWith("/interaction/inputs/")) {
      if (heldStatus) {const held=heldStatus;heldStatus=undefined;held.started();await held.promise;if(held.fail)throw new TypeError("Delayed status failure");}
      return originals.get(url.pathname.split("/").at(-1));
    }
    if (url.pathname === "/interaction/inputs") {
      const before=Number(url.searchParams.get("before")), direction=url.searchParams.get("direction");
      const rows=[...originals.values()].filter(v=>direction==="after"?v.sequence>before:!before||v.sequence<before);
      rows.sort((a,b)=>direction==="after"?a.sequence-b.sequence:b.sequence-a.sequence);
      const selected=rows.slice(0,8);
      const snapshot={binding:"fixture",csrf:"synthetic",items:selected.map(v=>({...v})),cursor:selected.at(-1)?.sequence||0,newest:Math.max(0,...selected.map(v=>v.sequence)),more:rows.length>8};
      if (heldInputs) {const held=heldInputs;heldInputs=undefined;held.started();await held.promise;}
      return snapshot;
    }
    if (url.pathname.endsWith("/part")) {
      if (heldValidation) { heldValidation.started = true; return heldValidation.promise; }
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
    return {binding:"fixture", csrf:"synthetic", can_listen:true,
      items:selected.map(source), cursor:{delivery:0,native:selected.at(-1) || cursor},
      newest:{delivery:0,native:Math.max(0,...selected)}, more:numbers.length > 8};
  };
  const context = vm.createContext({window:{addEventListener() {}}, document,
    location:{href:"https://fixture.invalid/?view=conversation"}, history:{replaceState() {}},
    URL, URLSearchParams, AbortController, setTimeout, clearTimeout, TextEncoder, crypto:require("node:crypto"),
    fetch:(_route, options) => new Promise((_resolve, reject) => {
      options.signal.addEventListener("abort", () => reject(new Error("Aborted")), {once:true});
    }),
  });
  vm.runInContext(fs.readFileSync("dashboard/web/requests.js", "utf8"), context);
  vm.runInContext(fs.readFileSync("dashboard/web/microphone.js", "utf8"), context);
  vm.runInContext(fs.readFileSync("dashboard/web/conversation.js", "utf8"), context);
  const ui = context.window.Conversation.mount({api,headers:() => ({}),setCsrf() {},modeChanged() {}});
  ui.authenticated();
  return {ui, nodes, requests, originals, nativePosts,
    nativeRequest() {
      nativeRequests.set("native-request",{id:"native-request",binding:"fixture",revision:"native-revision",state:"pending",actionable:true,kind:"question",title:"Scope",label:"Conversation",body:"Choose scope",choices:[{label:"Selected scope",action:{kind:"option",index:0}}],allows_text:true,can_cancel:true,question_index:0,question_total:1,recorded_revisions:[]});
    },
    receivingPhase(value) {submitPhase=value;},
    lose() {losePost=true;},
    offline() {offlinePost=true;},
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
  const integrated=boot();integrated.nativeRequest();await integrated.ui.refresh();
  const requestBox=integrated.nodes.get("#conversation-request-cards").children[0];
  await requestBox.children[3].handlers.click();
  const answer=requestBox.children[6].children[1];answer.value="Keep this draft during conversation refresh";answer.handlers.input();
  await requestBox.children[5].children[0].handlers.click();
  const retry=requestBox.children[8].children[1];assert(!retry.hidden);
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
  console.log("Conversation history, terminal evidence ordering, stale playback, lost/offline POST recovery, concurrent draft and integrated native request retention regressions passed (zero model calls).");
})().catch(error => { console.error(error); process.exitCode = 1; });
