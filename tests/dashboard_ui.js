// Execute the shipped bootstrap against WebView visibility and HTTP failures.
// Synthetic DOM/network only: this does not establish real Telegram acceptance.
const assert = require("node:assert/strict");
const fs = require("node:fs");
const vm = require("node:vm");
const source = fs.readFileSync("dashboard/web/app.js", "utf8");
const settle = () => new Promise((resolve) => setImmediate(resolve));

function boot(fetchImpl, alertsImpl = () => Promise.resolve({ok:true, json:async () => ({state:"not_configured", csrf:"fixture-csrf"})})) {
  const nodes = new Map(), requests = [], timers = new Map();
  let nextTimer = 0;
  const element = () => ({
    textContent: "", disabled: false, handlers: {}, children: [], dataset: {},
    addEventListener(name, handler) { this.handlers[name] = handler; },
    append(...children) { this.children.push(...children); },
    replaceChildren(...children) { this.children = children; },
    close() {}, showModal() {},
  });
  const document = {
    hidden: true,
    documentElement: { dataset: {} },
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
    window: { Telegram: { WebApp: { initData: "synthetic", ready() {} } } },
    AbortController,
    crypto: { randomUUID: () => "synthetic-action-nonce" },
    fetch(route, options) {
      requests.push({ route, options });
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
  return { context, nodes, requests, timers };
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
  // Regression: previously this bootstrap skipped state and stayed on Reading….
  const launch = boot((route) => route === "api/session" ? ok() : unavailable());
  await settle();
  assert.deepEqual(launch.requests.map((x) => x.route), ["api/session", "api/alerts", "api/state"]);
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
  assert.equal(article.children[2].textContent,incident.projection.summary);
  assert.equal(article.dataset.acknowledgment,"unacknowledged");
  const acknowledge = article.children.at(-1);
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
  // Resolution does not acknowledge a notice the human has not yet seen.
  const resolved = {...incident, projection:{...incident.projection,processing_state:"resolved"}};
  vm.runInContext('alerts('+JSON.stringify({...operations,incidents:[],resolved_incidents:[resolved]})+')',independent.context);
  assert.match(independent.nodes.get("#operations-summary").textContent,/0 open.*1 unacknowledged/);
  const history = independent.nodes.get("#incidents").children.find((n) => n.className === "incident-history");
  assert.equal(history.open,false);
  assert.equal(history.children[1].dataset.processingState,"resolved");
  assert.equal(history.children[1].dataset.acknowledgment,"unacknowledged");
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
  console.log("dashboard UI: hidden launch, retry, HTTP deadlines and uncertain Stop passed");
})().catch((error) => { console.error(error); process.exitCode = 1; });
