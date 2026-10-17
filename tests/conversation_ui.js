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
      textContent: "", paused: true,
      setAttribute(name, value) { this[name] = value; },
      removeAttribute(name) { delete this[name]; },
      addEventListener(name, fn) { this.handlers[name] = fn; },
      append(...children) { this.children.push(...children); },
      replaceChildren(...children) { this.children = children; },
      insertBefore(child, before) {
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
  const api = async route => {
    const url = new URL(route, "https://fixture.invalid/");
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
    URL, URLSearchParams, AbortController, setTimeout, clearTimeout,
    fetch:(_route, options) => new Promise((_resolve, reject) => {
      options.signal.addEventListener("abort", () => reject(new Error("Aborted")), {once:true});
    }),
  });
  vm.runInContext(fs.readFileSync("dashboard/web/conversation.js", "utf8"), context);
  const ui = context.window.Conversation.mount({api,headers:() => ({}),setCsrf() {},modeChanged() {}});
  ui.authenticated();
  return {ui, nodes, requests,
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
  console.log("Conversation backlog and stale validation regressions passed (zero model calls).");
})().catch(error => { console.error(error); process.exitCode = 1; });
