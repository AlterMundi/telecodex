"use strict";
window.RequestControls = (() => {
  const $ = selector => document.querySelector(selector);
  const node = (tag, text, cls) => {
    const el = document.createElement(tag);
    if (text !== undefined) el.textContent = text;
    if (cls) el.className = cls;
    return el;
  };
  const terminal = value => ["expired","answered","cancelled","source_changed"].includes(value.state);
  const statusText = value => value.state === "pending" && value.actionable
    ? value.kind === "approval" ? "Approval needed."
      : `Question ${(value.question_index || 0) + 1} of ${value.question_total || 1}.`
    : ({answering:"Response recorded. Work may continue.", answered:"Response recorded. Work may continue.",
        unknown:"Earlier response remains unconfirmed.", cancelled:"Cancellation requested.",
        expired:"This request is closed.", source_changed:"This request is no longer current."})[value.state]
      || "This request has no live response channel.";
  function mount({api, getBinding, setCsrf, unauthorized, onReply, onListen, getCanListen=()=>false}) {
    const cards = new Map(), section = $("#conversation-requests"), container = $("#conversation-request-cards");
    const state = $("#conversation-request-state"), more = $("#conversation-requests-more");
    let generation = 0, observation = 0, cursor = 0, reading = false, probeCursor = 0;
    const owns = (card, owned) => generation === owned && cards.get(card.id) === card;
    function reset() {
      generation++; cards.clear(); container.replaceChildren(); section.hidden = true;
      cursor = 0; probeCursor = 0; reading = false; more.hidden = true; state.textContent = "";
    }
    function updateControls(card) {
      const allowed = card.view?.actionable && !terminal(card.view) && card.complete
        && !card.reviewRequired && !card.closed && !card.busy && !card.loading && !card.pending && !card.unavailable && card.view.binding === getBinding();
      for (const button of card.choices.children) button.disabled = !allowed;
      card.reply.disabled = !allowed;
      card.listen.hidden = card.view?.kind!=="question" || typeof onListen!=="function" || !getCanListen();
      card.listen.disabled = !card.view?.actionable || card.closed || card.busy || card.loading || !!card.pending || card.unavailable || card.view?.binding!==getBinding();
      card.text.disabled = !allowed;
      card.submit.disabled = !allowed;
      card.cancel.disabled = !allowed;
      card.read.disabled = card.busy || card.loading;
      card.retry.hidden = !card.pending;
      card.retry.disabled = card.busy;
      card.check.hidden = !card.lastDecision && !card.pending;
      card.check.disabled = card.busy;
      card.dismiss.hidden = !terminal(card.meta) && card.meta?.state !== "unknown";
    }
    function saveDraft(card) {
      if (!card.text.value) return;
      card.saved.hidden = false;
      card.saved.append(node("p", "Unsent draft for the previous question:"), node("div", card.text.value, "reply-text"));
      card.text.value = ""; card.answerRevision = undefined;
    }
    function recorded(card, revision) {
      if (card.pending?.revision === revision) {
        if (card.pending.action.kind === "answer" && card.text.value === card.pending.action.text) card.text.value = "";
        card.pending = undefined;
      }
      card.note.textContent = "Decision recorded. Work may continue.";
      card.lastDecision = revision;
    }
    function metadata(card, value, observed) {
      // Terminal evidence and recorded revisions remain true despite request order.
      for (const revision of value.recorded_revisions || []) {
        if (card.pending?.revision === revision) recorded(card, revision);
      }
      if (terminal(value)) card.closed = true;
      if (card.busy) {
        if (terminal(value)) card.status.textContent = statusText(value);
        updateControls(card); return false;
      }
      if (observed < card.observed && !terminal(value)) return false;
      if (card.closed && !terminal(value)) return false;
      if (value.state === "pending" && (value.question_index ?? 0) < (card.meta?.question_index ?? 0)) return false;
      card.observed = Math.max(card.observed, observed);
      if (card.meta && card.meta.revision !== value.revision) {
        card.reviewRequired = card.reviewRequired || card.expanded;
        card.readVersion++; card.complete = false; card.view = undefined;
        card.body.textContent = ""; card.expanded = false; card.next = undefined;
        card.choices.replaceChildren(); saveDraft(card);
        card.read.textContent = "Read current request";
        if (card.pending) {
          // The old revision cannot be applied to a changed proposal. Keep its
          // identity available for receipt inspection without replaying it.
          card.lastDecision = card.pending.revision;
          card.pending = undefined;
          card.note.textContent = "Earlier choice unconfirmed; this request changed. Review the current request before choosing again.";
        }
      }
      card.meta = value;
      card.title.textContent = `${value.label || "Conversation"} · ${value.title || (value.kind === "approval" ? "Approval" : "Question")}`;
      card.status.textContent = statusText(value);
      updateControls(card);
      return true;
    }
    function create(value) {
      const box = node("article", undefined, "reply request-card");
      const title = node("h3"), status = node("p", "", "reply-status");
      const preview = node("p", value.preview || "", "request-preview");
      const body = node("div", "", "reply-text");
      const read = node("button", "Read request"); read.type = "button";
      const choices = node("div", undefined, "reply-actions");
      const form = node("form"), label = node("label", "Your answer"), text = node("textarea");
      text.rows = 3; text.id = `answer-${value.id}`; label.setAttribute("for", text.id);
      const submit = node("button", "Send answer"); submit.type = "submit";
      form.append(label, text, submit); form.hidden = true;
      const reply = node("button", "Answer in conversation"); reply.type = "button"; reply.hidden = true; form.append(reply);
      const listen = node("button", "Listen to question"); listen.type = "button"; listen.hidden = true; form.append(listen);
      const cancel = node("button", "Cancel this turn"); cancel.type = "button"; cancel.hidden = true;
      const check = node("button", "Check decision"), retry = node("button", "Retry this same decision");
      const dismiss = node("button", "Dismiss");
      for (const button of [check,retry,dismiss]) button.type = "button";
      const actions = node("div", undefined, "reply-actions"); actions.append(check,retry,dismiss);
      const note = node("p", "", "reply-error"); note.setAttribute("role", "status");
      const saved = node("details"); saved.hidden = true; saved.append(node("summary", "Previous drafts"));
      box.append(title,status,preview,read,body,choices,form,cancel,actions,note,saved);
      container.append(box);
      const card = {id:value.id,box,title,status,preview,body,read,choices,form,text,submit,reply,listen,cancel,check,retry,dismiss,note,saved,
        observed:0,readVersion:0,complete:false,expanded:false,busy:false,loading:false,closed:false};
      cards.set(value.id,card);
      read.addEventListener("click", () => readRequest(card,false,true));
      check.addEventListener("click", () => checkDecision(card));
      retry.addEventListener("click", () => card.pending && sendDecision(card,card.pending));
      cancel.addEventListener("click", () => choose(card,{kind:"cancel"},card.reviewedRevision));
      form.addEventListener("submit", event => {event.preventDefault(); return choose(card,{kind:"answer",text:card.text.value},card.answerRevision);});
      reply.addEventListener("click", () => {
        const view=card.view;
        if(typeof onReply!=="function" || view?.kind!=="question" || !view.actionable || !card.complete || card.reviewRequired || card.closed || card.busy || card.loading || card.pending || card.unavailable || view.binding!==getBinding())return;
        onReply({source:`request:${card.id}`,revision:view.revision},`${view.label || "Conversation"} · ${view.title || "Question"}`);
      });
      listen.addEventListener("click", () => {
        const view=card.view;
        if(typeof onListen!=="function" || !getCanListen() || view?.kind!=="question" || !view.actionable || card.closed || card.busy || card.loading || card.pending || card.unavailable || view.binding!==getBinding())return;
        return onListen({id:`request:${card.id}`,revision:view.revision,kind:"native_question",label:`${view.label || "Conversation"} · ${view.title || "Question"}`});
      });
      text.addEventListener("input", () => {card.answerRevision = card.view?.revision;});
      dismiss.addEventListener("click", () => {
        if (card.busy || card.pending || !terminal(card.meta) && card.meta?.state !== "unknown") return;
        cards.delete(card.id); box.remove(); section.hidden = !cards.size;
      });
      return card;
    }
    function selected(value, card, revision) {
      if (value.id !== card.id || value.binding !== getBinding() || revision && value.revision !== revision)
        throw new Error("Request changed. Read its current proposal before responding.");
    }
    function showDetail(card,value,continuation,offset) {
      if (continuation && value.offset !== offset) throw new Error("Request text changed; read it again.");
      const sameExpanded = !continuation && card.expanded && card.view?.revision === value.revision;
      card.body.textContent = continuation ? card.body.textContent + value.body : sameExpanded ? card.body.textContent : value.body;
      // An unchanged refresh preserves both text and the next unread offset.
      card.next = sameExpanded ? card.next : value.next_offset;
      card.complete = card.next === null;
      card.view = value; card.expanded = true; card.unavailable = false;
      card.preview.hidden = true;
      card.read.textContent = card.reviewRequired ? "Review current request" : card.complete ? "Refresh request" : "Continue reading request";
      card.choices.replaceChildren();
      for (const choice of value.choices || []) {
        const button = node("button",choice.label); button.type = "button";
        button.addEventListener("click", () => choose(card,choice.action,value.revision)); card.choices.append(button);
      }
      card.form.hidden = !value.allows_text;
      card.reply.hidden = value.kind!=="question" || !value.allows_text || typeof onReply!=="function";
      card.cancel.hidden = !value.can_cancel;
    }
    async function readRequest(card, force = false, userGesture = false) {
      if (card.loading || card.busy) return;
      const owned = generation, observed = ++observation;
      const continuation = !force && card.expanded && card.next !== null && card.next !== undefined && !card.unavailable;
      const expected = continuation ? card.view.revision : undefined;
      const offset = continuation ? card.next : 0;
      const version = ++card.readVersion;
      card.loading = true; updateControls(card);
      try {
        const query = new URLSearchParams({offset:String(offset)});
        if (expected) query.set("revision",expected);
        const value = await api(`interaction/requests/${card.id}?${query}`);
        if (!owns(card,owned) || card.readVersion !== version) return;
        selected(value,card,expected);
        if (!metadata(card,value,observed)) return;
        if (userGesture) {card.reviewRequired = false; card.reviewedRevision = value.revision;}
        showDetail(card,value,continuation,offset);
      } catch(e) {
        if (!owns(card,owned) || card.readVersion !== version) return;
        card.unavailable = true;
        card.note.textContent = e.message || "Request unavailable. Refresh before choosing.";
        if (e.status === 401 || e.status === 403) unauthorized(e);
      } finally {
        if (owns(card,owned)) {card.loading = false; updateControls(card);}
      }
    }
    function choose(card, action, expectedRevision) {
      if (!expectedRevision || card.view?.revision !== expectedRevision || card.reviewRequired || !card.complete || !card.view?.actionable || card.view.binding !== getBinding() || card.pending || card.busy || card.loading || card.unavailable || card.closed) return;
      if (action.kind === "answer" && (!action.text.trim() || action.text.includes("\0") || new TextEncoder().encode(action.text).length > 32768)) {
        card.note.textContent = "Write an answer of at most 32 KiB."; return;
      }
      const frozen = {binding:card.view.binding,revision:card.view.revision,action:JSON.parse(JSON.stringify(action))};
      card.pending = frozen; card.lastDecision = frozen.revision;
      return sendDecision(card,frozen);
    }
    async function sendDecision(card, frozen) {
      if (card.busy || frozen !== card.pending) return;
      const owned = generation;
      card.busy = true; card.note.textContent = "Sending your decision…"; updateControls(card);
      try {
        const value = await api(`interaction/requests/${card.id}`,frozen);
        if (!owns(card,owned)) return;
        if (!value.recorded || value.id !== card.id || value.revision !== frozen.revision || value.binding !== getBinding())
          throw new Error("Decision recording is unconfirmed.");
        recorded(card,frozen.revision);
      } catch(e) {
        if (!owns(card,owned)) return;
        if (card.pending === frozen) card.note.textContent = "Decision unconfirmed. Check its status before choosing again.";
        if (e.status === 401 || e.status === 403) unauthorized(e);
      } finally {
        if (owns(card,owned)) {card.busy = false; updateControls(card);}
      }
      if (owns(card,owned) && !card.pending) await readRequest(card,true);
    }
    async function checkDecision(card) {
      if (card.busy || !card.lastDecision) return;
      const owned = generation, revision = card.pending?.revision || card.lastDecision;
      card.busy = true; updateControls(card);
      try {
        const value = await api(`interaction/requests/${card.id}/receipt?${new URLSearchParams({revision})}`);
        if (!owns(card,owned)) return;
        if (!value.recorded || value.id !== card.id || value.revision !== revision || value.binding !== getBinding())
          throw new Error("Decision recording is unconfirmed.");
        recorded(card,revision);
      } catch(e) {
        if (!owns(card,owned)) return;
        card.note.textContent = "No decision recording confirmed. Nothing has been resent.";
        if (e.status === 401 || e.status === 403) unauthorized(e);
      } finally {
        if (owns(card,owned)) {card.busy = false; updateControls(card);}
      }
      if (owns(card,owned)) await readRequest(card,true);
    }
    async function refresh(after = 0) {
      if (reading || !getBinding()) return;
      const owned = generation, observed = ++observation;
      reading = true; more.disabled = true;
      try {
        const page = await api(`interaction/requests?after=${after}`);
        if (owned !== generation) return;
        const seen = new Set();
        const apply = value => {
          if (value.binding !== getBinding()) throw new Error("Conversation changed. Refresh before responding.");
          setCsrf(value.csrf);
          for (const item of value.items) {seen.add(item.id); metadata(cards.get(item.id) || create(item),item,observed);}
          cursor = Math.max(cursor,value.cursor);
          return value;
        };
        let last = apply(page);
        if (after === 0 && cursor > page.cursor) {
          const tail = await api(`interaction/requests?after=${cursor}`);
          if (owned !== generation) return;
          last = apply(tail);
        }
        more.hidden = !last.more;
        // Recheck loaded requests absent from these pages, including ones that
        // another surface answered or the native source retired meanwhile.
        const candidates = [...cards.values()].filter(card => !seen.has(card.id) && !terminal(card.meta) && !card.loading && !card.busy);
        const selectedCards = candidates.length ? Array.from({length:Math.min(8,candidates.length)},(_,n)=>candidates[(probeCursor+n)%candidates.length]) : [];
        probeCursor += selectedCards.length;
        await Promise.all(selectedCards.map(async card => {
          const version = ++card.readVersion;
          try {
            const value = await api(`interaction/requests/${card.id}?offset=0`);
            if (!owns(card,owned) || version !== card.readVersion) return;
            selected(value,card);
            if (!metadata(card,value,observed)) return;
            if (card.expanded) {showDetail(card,value,false,0); updateControls(card);}
          } catch(e) {
            if (!owns(card,owned) || version !== card.readVersion || observed < card.observed) return;
            card.unavailable = true; updateControls(card);
            card.note.textContent = "Request unavailable. Check its current state before responding.";
            if (e.status === 401 || e.status === 403) unauthorized(e);
          }
        }));
        if (owned !== generation) return;
        section.hidden = !cards.size;
        state.textContent = last.more ? "More requests are available." : "You can keep writing while sessions wait for a response.";
      } catch(e) {
        if (owned !== generation) return;
        section.hidden = false; state.textContent = "Requests unavailable. Refresh to reconnect.";
        if (e.status === 401 || e.status === 403) unauthorized(e);
      } finally {if (owned === generation) {reading = false; more.disabled = false;}}
    }
    more.addEventListener("click", () => refresh(cursor));
    return {refresh,reset};
  }
  return {mount};
})();
