// Execute the shipped bootstrap against WebView visibility and HTTP failures.
// Synthetic DOM/network only: this does not establish real Telegram acceptance.
const assert = require("node:assert/strict");
const fs = require("node:fs");
const vm = require("node:vm");
const source = fs.readFileSync("dashboard/web/app.js", "utf8");
const settle = () => new Promise((resolve) => setImmediate(resolve));

function boot(fetchImpl, alertsImpl = () => Promise.resolve({ok:true, json:async () => ({state:"not_configured", csrf:"fixture-csrf"})}), mode=false, shared=false) {
  const monitor=mode===true, standalone=mode==='conversation', device=monitor||standalone;
  const nodes = new Map(), requests = [], timers = new Map();
  let nextTimer = 0, monitorHeaders, signIns=0, unauthorized;
  const saved = new Map();
  const element = () => ({
    style: {}, setAttribute(name, value) { this[name] = value; }, textContent: "", disabled: false, handlers: {}, children: [], dataset: {},
    addEventListener(name, handler) { this.handlers[name] = handler; },
    append(...children) { this.children.push(...children); },
    replaceChildren(...children) { this.children = children; },
    close() {}, showModal() {},
  });
  const document = {
    hidden: true,
    documentElement: { dataset: shared?{sharedAuth:'true'}:{} },
    querySelector(selector) {
      if (!nodes.has(selector)) nodes.set(selector, element());
      return nodes.get(selector);
    },
    createElement() { return element(); },
    querySelectorAll() { return []; },
    addEventListener() {},
  };
  const context = vm.createContext({
    document,
    window: { Telegram: { WebApp: { initData: standalone ? undefined : "synthetic", ready() {} } },
      DaimonHomeAccess:{createClient(){return {async signIn(){signIns++;return {status:'redirecting'};}};}},
      ...(device?{Conversation:{mount(options){monitorHeaders=options.headers;unauthorized=options.onUnauthorized;options.modeChanged(true,true);return {authenticated(){},refresh:async()=>{}};}}}:{}) },
    sessionStorage:{getItem:k=>saved.get(k),setItem:(k,v)=>saved.set(k,v),removeItem:k=>saved.delete(k)},
    URL, location:{href:"https://fixture.invalid/sessions/"+(device?"?view="+(monitor?"monitor":"conversation"):""),origin:"https://fixture.invalid",pathname:"/sessions/"},
    AbortController,
    AbortSignal,
    crypto: { randomUUID: () => "synthetic-action-nonce" },
    fetch(route, options) {
      requests.push({ route, options });
      if (route === "api/traffic") return Promise.resolve({ok:true,json:async () => ({state:"not_configured"})});
      if (route === "api/alerts") return alertsImpl(route, options);
      return fetchImpl(route, options);
    },
    setTimeout(callback, milliseconds) {
      const id = ++nextTimer;
      timers.set(id, { callback, milliseconds });
      return id;
    },
    clearTimeout(id) { timers.delete(id); },
    setInterval() { throw new Error("Hidden WebView must not poll"); },
    clearInterval() {},
  });
  vm.runInContext(source, context);
  return { context, nodes, requests, timers, headers:()=>monitorHeaders(), saved, signIns:()=>signIns, expire:()=>unauthorized({status:401}) };
}

const fixtureToken = "f".repeat(43);
const ok = () => Promise.resolve({ ok: true, json: async () => ({ ok: true, session_token: fixtureToken }) });
const unavailable = () => Promise.resolve({
  ok: false, json: async () => ({ detail: "Receiving metadata unavailable" }),
});
const hung = (options) => new Promise((resolve, reject) => {
  options.signal.addEventListener("abort", () => reject(new Error("Aborted")));
});

(async () => {
  const shared=boot(()=>Promise.resolve({ok:true,json:async()=>({csrf:'shared-csrf',label:'Human'})}),undefined,'conversation',true);
  await settle();
  assert.equal(shared.requests[0].route,'api/interaction/access');
  assert.equal(shared.requests[0].options.credentials,'same-origin');
  assert.equal(shared.nodes.get('#feedback').children[0].textContent,'Connected · Human');
  assert.equal(shared.saved.size,0,'Shared auth must not copy its session into JS storage');
  assert.equal(shared.signIns(),0,'A remembered session reconnects without another login');
  assert.equal(shared.headers().Authorization,undefined);
  shared.expire();
  assert.equal(shared.nodes.get('#feedback').children[0].textContent,'Sign in again to reconnect.');
  assert.equal(shared.nodes.get('#feedback').children[1].textContent,'Sign in');
  assert.equal(shared.signIns(),0,'Expiry exposes login without replaying work or silently starting another ceremony');
  for(const status of [401,403,503]){
    const denied=boot(()=>Promise.resolve({ok:false,status,json:async()=>({})}),undefined,'conversation',true);
    await settle();
    assert.equal(denied.signIns(),status===401?1:0,'Only a missing session starts common sign-in');
    assert.equal(denied.nodes.get('#feedback').children[1].textContent,'Sign in');
  }
  const monitor=boot(()=>Promise.resolve({ok:true,json:async()=>({binding:"fixture",viewer:{label:"TEST <fixture>",expires:1900000000}})}),undefined,true);
  await settle();
  assert.equal(monitor.requests.length,0,"An unopened monitor must not use ambient owner cookies");
  assert.throws(()=>monitor.headers(),/monitor credential/);
  const upload=monitor.nodes.get("#feedback").children[0].children[0];
  const credential={scope:"monitor_readonly",origin:"https://fixture.invalid",prefix:"/sessions",expires:Date.now()/1000+3600,authorization:"Companion "+"m".repeat(43)};
  upload.files=[{size:400,text:async()=>JSON.stringify(credential)}];await upload.handlers.change();
  assert.equal(monitor.context.document.title,"Routing monitor · TEST <fixture>");
  assert.equal(monitor.nodes.get("#feedback").children[2].children[0].textContent,"Change monitor access");
  assert.equal(monitor.nodes.get("#feedback").children[0].textContent,"Source: TEST <fixture>");
  assert.equal(monitor.headers().Authorization,credential.authorization,"Direct image reads use the same restricted grant");
  assert.equal(monitor.requests[0].options.headers.Authorization,credential.authorization);
  assert(monitor.requests.every(r=>r.options.method==="GET"));
  upload.files=[{size:400,text:async()=>JSON.stringify({...credential,scope:"interaction_only"})}];await upload.handlers.change();
  assert.equal(monitor.context.document.title,"Routing monitor");
  assert.throws(()=>monitor.headers(),/monitor credential/,"A failed selection cannot restore ambient cookie authority");
  const before=monitor.requests.length;
  await assert.rejects(vm.runInContext('api("interaction/inputs")',monitor.context),/monitor credential/);
  assert.equal(monitor.requests.length,before);
  const client=boot(()=>Promise.resolve({ok:true,json:async()=>({binding:"fixture",csrf:"server-csrf",viewer:{label:"Phone",expires:1900000000}})}),undefined,'conversation');
  await settle();
  assert.equal(client.requests.length,0,"Standalone chat must not open a Telegram session or use ambient owner cookies");
  const accessFile=client.nodes.get("#feedback").children[0].children[0];
  const access={...credential,schema:"telecodex.companion-device/v1",scope:undefined,csrf:"c".repeat(43),expires:1};
  accessFile.files=[{size:400,text:async()=>JSON.stringify(access)}];await accessFile.handlers.change();
  assert.equal(client.context.document.title,"Daimon Interaction · Phone");
  assert.equal(client.headers().Authorization,access.authorization);
  await vm.runInContext('api("interaction/inputs",{id:"synthetic",text:"Continue while a worker is active"})',client.context);
  const submitted=client.requests.at(-1);
  assert.equal(submitted.options.method,"POST");
  assert.equal(submitted.options.headers["X-Dashboard-CSRF"],"server-csrf");
  assert.equal(submitted.options.headers.Authorization,access.authorization);
  const requestCount=client.requests.length;
  await assert.rejects(vm.runInContext('api("stop",{nonce:"forbidden"})',client.context),/limited to your conversation/);
  assert.equal(client.requests.length,requestCount);
  accessFile.files=[{size:400,text:async()=>JSON.stringify({...access,scope:"monitor_readonly"})}];await accessFile.handlers.change();
  assert.throws(()=>client.headers(),/conversation access file/);
  // Regression: previously this bootstrap skipped state and stayed on Reading….
  const launch = boot((route) => route === "api/session" ? ok() : unavailable());
  await settle();
  assert.deepEqual(launch.requests.map((x) => x.route), ["api/session", "api/alerts", "api/traffic", "api/state"]);
  assert.equal(launch.requests[0].options.headers.Authorization, undefined);
  assert.equal(launch.requests[1].options.headers.Authorization, `Bearer ${fixtureToken}`);
  assert.equal(launch.nodes.get("#overview").textContent, "State unavailable");
  assert.equal(launch.nodes.get("#refresh").disabled, false);
  assert.equal(launch.nodes.get("#stop").disabled, true);
  await launch.nodes.get("#refresh").handlers.click();
  assert.equal(launch.requests.filter((x) => x.route === "api/state").length, 2);

  // A hanging state fetch releases the in-flight guard and makes retry usable.
  const stalled = boot((route, options) => route === "api/session" ? ok() : hung(options));
  await settle();
  assert.equal(stalled.timers.size, 1);
  const deadline = [...stalled.timers.values()][0];
  assert.equal(deadline.milliseconds, 20000);
  deadline.callback();
  await settle();
  assert.match(stalled.nodes.get("#feedback").textContent, /timed out/);
  assert.equal(stalled.nodes.get("#refresh").disabled, false);
  assert.equal(vm.runInContext("refreshing", stalled.context), false);
  assert.equal(stalled.timers.size, 0);

  // Authentication timeout is an explicit connection failure, not native work.
  const auth = boot((route, options) => hung(options));
  [...auth.timers.values()][0].callback();
  await settle();
  assert.equal(auth.nodes.get("#overview").textContent, "Connection unavailable");
  assert.match(auth.nodes.get("#feedback").textContent, /reopen Sessions/);
  assert.equal(auth.nodes.get("#refresh").disabled, true);
  assert.deepEqual(auth.requests.map((x) => x.route), ["api/session"]);

  // A timed-out Stop remains uncertain and must never be auto-submitted again.
  const stop = boot((route, options) => route === "api/session" ? ok()
    : route === "api/stop" ? hung(options) : unavailable());
  await settle();
  vm.runInContext('controlIntent = {kind:"stop"}', stop.context);
  const approval = stop.nodes.get("#approve").handlers.click();
  const stopDeadline = [...stop.timers.values()][0];
  assert.equal(stopDeadline.milliseconds, 30000);
  stopDeadline.callback();
  await approval;
  assert.equal(stop.requests.filter((x) => x.route === "api/stop").length, 1);
  assert.equal(stop.requests.find((x) => x.route === "api/stop").options.headers.Authorization, `Bearer ${fixtureToken}`);
  assert.equal(vm.runInContext("pendingNonce", stop.context), "synthetic-action-nonce");
  assert.match(stop.nodes.get("#feedback").textContent, /Check Stop status/);
  assert.equal(stop.nodes.get("#stop").disabled, true);
  // Observed Stop releases the pending Stop guard and offers the real Resume.
  vm.runInContext('controlButton({state:"stopped",operation:"op-A"}, false, 2)', stop.context);
  assert.equal(stop.nodes.get("#stop").textContent, "Resume owned sessions");
  assert.equal(stop.nodes.get("#stop").disabled, false);
  vm.runInContext('snapshot={stop:{state:"stopped",operation:"op-A"}}', stop.context);
  stop.nodes.get("#stop").handlers.click();
  assert.equal(stop.nodes.get("#approve").textContent, "Resume");
  stop.nodes.get("#cancel").handlers.click();
  assert.equal(stop.requests.filter((x) => x.route === "api/resume").length, 0);
  // A real confirmation pins the observed operation, and uncertainty isn't replayed.
  stop.nodes.get("#stop").handlers.click();
  const continuation = stop.nodes.get("#approve").handlers.click();
  await continuation;
  const resumeRequest = stop.requests.find((x) => x.route === "api/resume");
  assert.equal(JSON.parse(resumeRequest.options.body).operation, "op-A");
  vm.runInContext('controlButton({state:"stopped",operation:"op-A"}, false, 2)', stop.context);
  assert.equal(stop.nodes.get("#stop").disabled, true);
  vm.runInContext('controlButton({state:"resumed",operation:"op-A"}, false, 2)', stop.context);
  assert.equal(stop.nodes.get("#stop").disabled, false);
  assert.equal(stop.nodes.get("#stop").textContent, "Stop owned sessions");
  for (const phase of ["stopping", "resuming", "partial", "unknown"])
    assert.equal(vm.runInContext(`controlButton({state:"${phase}"}, false, 2); $("#stop").disabled`, stop.context), true);

  // An unavailable native probe cannot suppress operational incidents/controls.
  // All producer text stays literal, and acknowledgment IDs survive rerender.
  const incident = {projection:{producer_id:"fixture",incident_id:"one",phase:"index",
    summary:"<img src=x onerror=attack()>",action:"Restore credit",processing_state:"open"},
    freshness:"stale",deliveries:[{state:"uncertain"}],acknowledgments:[]};
  const operations = {state:"available",observer_state:"unobserved",incidents:[incident],csrf:"alerts-csrf"};
  const independent = boot((route) => route === "api/session" ? ok()
    : route === "api/alerts/acknowledge" ? Promise.resolve({ok:true,json:async () => ({request_id:"one"})})
    : unavailable(), () => Promise.resolve({ok:true,json:async () => operations}));
  await settle();
  assert.equal(independent.nodes.get("#overview").textContent,"State unavailable");
  assert.equal(independent.nodes.get("#operations").hidden,false);
  assert.equal(independent.nodes.get("#operations").dataset.attention,"true");
  assert.match(independent.nodes.get("#operations-summary").textContent,/⚠.*1 unacknowledged/);
  assert.match(independent.nodes.get("#alerts-state").textContent,/not been established/);
  const activeArticle = () => independent.nodes.get("#incidents").children.find((n) => n.dataset.processingState === "open");
  const article = activeArticle();
  assert.equal(article.children[1].textContent,"fixture · index");
  assert.equal(article.children[2].textContent,"Reported by: fixture · Resource: Unspecified");
  assert.equal(article.children[3].textContent,incident.projection.summary);
  // Current and legacy memory producers identify DMK without hiding exact provenance.
  for (const producer of ["hmk-compaii", "dmk-oliva", "other-reporter"]) {
    const attributed = {...incident, projection:{...incident.projection,producer_id:producer,
      resource_ref:"memory-worker",phase:"capture"}};
    vm.runInContext('alerts('+JSON.stringify({...operations,incidents:[attributed]})+')',independent.context);
    assert.equal(activeArticle().children[1].textContent,
      `${producer === "other-reporter" ? producer : "DMK (Memory)"} · capture`);
    assert.equal(activeArticle().children[2].textContent,`Reported by: ${producer} · Resource: memory-worker`);
  }
  vm.runInContext('alerts('+JSON.stringify(operations)+')',independent.context);
  assert.equal(article.dataset.acknowledgment,"unacknowledged");
  const acknowledge = activeArticle().children.at(-1);
  await acknowledge.handlers.click();
  assert.equal(acknowledge.textContent,"Acknowledged");
  assert.equal(independent.nodes.get("#operations").dataset.attention,"false");
  assert.doesNotMatch(independent.nodes.get("#operations-summary").textContent,/unacknowledged/);
  const sent = independent.requests.find((r) => r.route === "api/alerts/acknowledge");
  assert.equal(sent.options.headers["X-Dashboard-CSRF"],"alerts-csrf");
  const original = JSON.parse(sent.options.body).request_id;
  vm.runInContext('alerts('+JSON.stringify(operations)+')', independent.context);
  await activeArticle().children.at(-1).handlers.click();
  assert.equal(JSON.parse(independent.requests.filter((r) => r.route === "api/alerts/acknowledge").at(-1).options.body).request_id,original);
  assert.equal(independent.requests.some((r) => r.route === "api/stop" || r.route === "api/resume"),false);
  // Resolved history retains acknowledgment state without active attention.
  const resolved = {...incident, projection:{...incident.projection,processing_state:"resolved"}};
  vm.runInContext('alerts('+JSON.stringify({...operations,incidents:[],resolved_incidents:[resolved]})+')',independent.context);
  assert.equal(independent.nodes.get("#operations").dataset.attention,"false");
  assert.match(independent.nodes.get("#operations-summary").textContent,/0 open/);
  assert.doesNotMatch(independent.nodes.get("#operations-summary").textContent,/⚠|unacknowledged/);
  const history = independent.nodes.get("#incidents").children.find((n) => n.className === "incident-history");
  assert.equal(history.open,false);
  assert.equal(history.children[1].dataset.processingState,"resolved");
  assert.equal(history.children[1].dataset.acknowledgment,"unacknowledged");
  assert.match(history.children[0].textContent,/1 unacknowledged/);
  history.open=true;history.handlers.toggle();
  resolved.acknowledgments=[{request_id:"ack-confirmed"}];
  vm.runInContext('alerts('+JSON.stringify({...operations,incidents:[],resolved_incidents:[resolved]})+')',independent.context);
  assert.equal(independent.nodes.get("#operations").dataset.attention,"false");
  assert.equal(independent.nodes.get("#incidents").children.find((n) => n.className === "incident-history").open,true);
  // Acknowledgment changes attention; it never moves an open failure into history.
  const acknowledgedOpen = {...incident,acknowledgments:[{request_id:"ack-confirmed"}]};
  vm.runInContext('alerts('+JSON.stringify({...operations,incidents:[acknowledgedOpen],resolved_incidents:[resolved]})+')',independent.context);
  assert.equal(activeArticle().dataset.processingState,"open");
  assert.equal(activeArticle().dataset.acknowledgment,"acknowledged");
  assert.match(independent.nodes.get("#operations-summary").textContent,/1 open/);
  // Traffic renders independently and keeps labels literal. Missing capture never claims health.
  const observed = {state:"available",observed_at:Date.now()/1000,capture_started:Date.now()/1000-300,
    meter_healthy:true,alerts_state:"available",requests_last_5m:150,dropped_records:2,
    cooldown:{state:"active",until:Date.now()/1000+120,methods:["sendMessage"]},
    totals:{requests:150,rate_limited:1,request_bytes:100,response_bytes:200,unknown_source:2,unknown_topic:3,incomplete:1},
    minutes:[],sources:[{source:"bridge",method:"sendChatAction",requests:149,rate_limited:0}],
    topics:[{name:"<img onerror=attack()>",requests:149,rate_limited:0}]};
  vm.runInContext('traffic('+JSON.stringify(observed)+')', independent.context);
  assert.match(independent.nodes.get("#traffic-summary").textContent,/30\/min.*Telegram limited/);
  assert.match(independent.nodes.get("#traffic-status").textContent,/Retry deadline/);
  assert.equal(independent.nodes.get("#traffic-topics").children[0].children[0].textContent, observed.topics[0].name);
  assert.equal(independent.nodes.get("#traffic-chart").children.length,60);
  observed.cooldown.state="awaiting_recovery";
  vm.runInContext('traffic('+JSON.stringify(observed)+')', independent.context);
  assert.match(independent.nodes.get("#traffic-status").textContent,/deadline alone does not confirm recovery/);
  vm.runInContext('traffic({state:"unavailable"})', independent.context);
  assert.equal(independent.nodes.get("#traffic-topics").children.length,0);
  assert.match(independent.nodes.get("#traffic-status").textContent,/cannot be confirmed/);
  await require("./dashboard_health_ui.js")();
  console.log("dashboard UI: hidden launch, retry, HTTP deadlines and uncertain Stop passed");
})().catch((error) => { console.error(error); process.exitCode = 1; });
