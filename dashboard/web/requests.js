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
  function mount({api, getBinding, setCsrf, unauthorized, onReply, onListen, getCanListen=()=>false, getCanDecide=()=>true,onSource=()=>{}}) {
    const cards = new Map(), section = $("#conversation-requests"), container = $("#conversation-request-cards");
    const state = $("#conversation-request-state"), more = $("#conversation-requests-more");
    let generation = 0, observation = 0, cursor = 0, reading = false, probeCursor = 0, refreshTask;
    const owns = (card, owned) => generation === owned && cards.get(card.id) === card;
    function reset() {
      generation++; cards.clear(); container.replaceChildren(); section.hidden = true;
      cursor = 0; probeCursor = 0; reading = false; refreshTask=undefined; more.hidden = true; state.textContent = "";
    }
    function updateControls(card) {
      const allowed = card.view?.actionable && !terminal(card.view) && card.complete
        && !card.reviewRequired && !card.closed && !card.busy && !card.loading && !card.pending && !card.unavailable && card.view.binding === getBinding();
      const mayDecide=allowed && getCanDecide();
      for (const button of card.choices.children) button.disabled = !mayDecide;
      card.reply.disabled = !allowed;
      card.listen.hidden = card.view?.kind!=="question" || typeof onListen!=="function" || !getCanListen();
      card.listen.disabled = !card.view?.actionable || card.closed || card.busy || card.loading || !!card.pending || card.unavailable || card.view?.binding!==getBinding();
      card.text.disabled = !mayDecide;
      card.submit.disabled = !mayDecide;
      card.cancel.disabled = !mayDecide;
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
      card.unavailable=false;card.meta = value;
      card.title.textContent = `${value.label || "Conversation"} · ${value.title || (value.kind === "approval" ? "Approval" : "Question")}`;
      card.status.textContent = statusText(value) + (!getCanDecide() && value.state==='pending' ? value.kind==='approval' ? ' Open Sessions to review this approval.' : ' Reply through the conversation to answer.' : '');
      updateControls(card);
      const source=audioSource(value);if(source)onSource(source);
      return true;
    }
    function audioSource(value) {
      if(value?.kind!=="question" || value.state!=="pending" || !value.actionable)return;
      return {id:`request:${value.id}`,revision:value.revision,kind:"native_question",label:`${value.label || "Conversation"} · ${value.title || "Question"}`};
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
      const reply = node("button", "Answer in conversation"); reply.type = "button"; reply.hidden = true;
      const listen = node("button", "Listen to question"); listen.type = "button"; listen.hidden = true;
      const cancel = node("button", "Cancel this turn"); cancel.type = "button"; cancel.hidden = true;
      const check = node("button", "Check decision"), retry = node("button", "Retry this same decision");
      const dismiss = node("button", "Dismiss");
      for (const button of [check,retry,dismiss]) button.type = "button";
      const actions = node("div", undefined, "reply-actions"); actions.append(reply,listen,check,retry,dismiss);
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
      card.form.hidden = !value.allows_text || !getCanDecide();
      card.reply.hidden = value.kind!=="question" || !value.allows_text || typeof onReply!=="function";
      card.cancel.hidden = !value.can_cancel || !getCanDecide();
      card.choices.hidden = !getCanDecide();
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
      if (!getCanDecide() || !expectedRevision || card.view?.revision !== expectedRevision || card.reviewRequired || !card.complete || !card.view?.actionable || card.view.binding !== getBinding() || card.pending || card.busy || card.loading || card.unavailable || card.closed) return;
      if (action.kind === "answer" && (!action.text.trim() || action.text.includes("\0") || new TextEncoder().encode(action.text).length > 32768)) {
        card.note.textContent = "Write an answer of at most 32 KiB."; return;
      }
      const frozen = {binding:card.view.binding,revision:card.view.revision,action:JSON.parse(JSON.stringify(action))};
      card.pending = frozen; card.lastDecision = frozen.revision;
      return sendDecision(card,frozen);
    }
    async function sendDecision(card, frozen) {
      if (!getCanDecide() || card.busy || frozen !== card.pending) return;
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
    function refresh(after = 0) {
      if(refreshTask)return refreshTask;
      const task=refreshCurrent(after).finally(()=>{if(refreshTask===task)refreshTask=undefined;});
      refreshTask=task;return task;
    }
    async function refreshCurrent(after = 0) {
      if (reading || !getBinding()) return false;
      const owned = generation, observed = ++observation;let current=true;
      reading = true; more.disabled = true;
      try {
        const page = await api(`interaction/requests?after=${after}`);
        if (owned !== generation) return false;
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
          if (owned !== generation) return false;
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
            // An absent old request is not a failure of the current registry.
            // Keep its unknown card, but never offer its stale audio/context.
            if(e.status!==404)current=false;card.unavailable = true; updateControls(card);
            card.note.textContent = "Request unavailable. Check its current state before responding.";
            if (e.status === 401 || e.status === 403) unauthorized(e);
          }
        }));
        if (owned !== generation) return false;
        section.hidden = !cards.size;
        state.textContent = last.more ? "More requests are available." : "You can keep writing while sessions wait for a response.";
        return current;
      } catch(e) {
        if (owned !== generation) return false;
        section.hidden = false; state.textContent = "Requests unavailable. Refresh to reconnect.";
        if (e.status === 401 || e.status === 403) unauthorized(e);
        return false;
      } finally {if (owned === generation) {reading = false; more.disabled = false;}}
    }
    more.addEventListener("click", () => refresh(cursor));
    return {attentionItems:()=>[...cards.values()].filter(c=>c.meta&&!terminal(c.meta)&&!c.closed).map(c=>({id:`request:${c.id}`,label:c.meta.label,title:c.meta.title,box:c.box,open:()=>readRequest(c,true,true)})),refresh,reset,captureQuestions:()=>Object.fromEntries([...cards.values()].filter(card=>!card.unavailable).map(card=>card.meta).filter(value=>audioSource(value) && value.binding===getBinding() && /^[a-f0-9]{32}$/.test(value.id) && /^[a-f0-9]{64}$/.test(value.capture_version)).slice(0,32).map(value=>[value.id,value.capture_version])),audioSources:()=>[...cards.values()].filter(c=>!c.unavailable).map(c=>audioSource(c.meta)).filter(Boolean)};
  }
  return {mount};
})();

// Goal decisions remain ordinary human conversation inputs. Reviewing a card
// only drafts a precisely attributed decision; Send creates the actual original.
window.GoalControls = (() => {
  const closed = phase => ["acknowledged","cancelled","superseded","rejected"].includes(phase);
  const text = (tag,value) => {const el=document.createElement(tag);el.textContent=value;return el;};
  const status = view => !view.current && !closed(view.phase) && !["uncertain","partial","submitted"].includes(view.phase) ? "This selection changed or expired. Ask for a fresh proposal."
    : ({awaiting_confirmation:"Your choice is needed.",prepared:"Authorized; not submitted yet.",claimed:"Waiting for the selected session.",submitted:"Applying your instruction; confirmation pending.",
      acknowledged:"Instruction acknowledged. Work and reply may continue.",cancelled:"Proposal cancelled.",superseded:"Replaced by a later instruction.",rejected:"Not applied; review the current goal.",
      known_unsent:"Saved; not sent.",retained_unsent:"Saved; not sent after restart.",uncertain:"Outcome unconfirmed. Nothing will be repeated.",partial:"Part of this instruction applied. Check the retained outcome before continuing."})[view.phase] || "Control outcome unavailable.";
  function mount({api,getBinding,onChoose,readOnly=false}) {
    const section=document.querySelector("#conversation-goals"),list=document.querySelector("#conversation-goal-cards"),more=document.querySelector("#conversation-goals-more");
    const message=document.querySelector("#conversation-goal-state");
    const cards=new Map();let generation=0,cursor=0,reading=false,probe=0;
    const owns=(card,owned)=>generation===owned && cards.get(card.id)===card;
    function reset(){generation++;cards.clear();list.replaceChildren();section.hidden=true;cursor=0;reading=false;more.hidden=true;message.textContent="";}
    function render(card,view){
      card.view=view;card.title.textContent=`${({start:"Start",resume:"Resume",pause:"Pause",clear:"Clear",edit:"Edit",replace:"Replace"})[view.action]||"Review"} · ${view.label}`;
      card.state.textContent=status(view);card.dismiss.hidden=!closed(view.phase);
      card.confirm.disabled=card.cancel.disabled=readOnly || !view.can_confirm || !card.reviewed;
      if(readOnly || !view.can_confirm){card.confirm.hidden=card.cancel.hidden=true;}
      else {card.confirm.hidden=card.cancel.hidden=false;}
    }
    function choose(card,accept){
      const v=card.view;if(readOnly || !v.can_confirm || !card.reviewed)return;
      let message=accept?`Confirm goal proposal ${v.id}: ${v.action} ${v.label}.`:`Cancel goal proposal ${v.id} for ${v.label}.`;
      if(accept && v.plan.objective)message+=` Objective: ${v.plan.objective}`;
      if(accept && v.plan.token_budget)message+=` Token budget: ${v.plan.token_budget}.`;
      if(accept && v.plan.followup)message+=` Then ${v.plan.followup}`;
      card.note.textContent=onChoose(message)?"Review your message, then Send. Nothing changed yet.":"Keep or clear your current draft before choosing here.";
    }
    function create(view){
      const box=text("article","");box.className="reply request-card";
      const title=text("h3",""),state=text("p",""),read=text("button","Review proposal and outcome");read.type="button";
      state.setAttribute("role","status");
      const body=text("div","");body.className="reply-text";body.hidden=true;
      const confirm=text("button","Confirm in conversation"),cancel=text("button","Decline in conversation"),dismiss=text("button","Dismiss");
      const note=text("p","");note.setAttribute("role","status");const actions=text("div","");actions.className="reply-actions";
      for(const b of [confirm,cancel,dismiss])b.type="button";
      actions.append(confirm,cancel,dismiss);box.append(title,state,read,body,actions,note);list.append(box);
      const card={id:view.id,box,title,state,read,body,confirm,cancel,dismiss,note,reviewed:false,version:0};cards.set(view.id,card);
      read.addEventListener("click",()=>detail(card,true));confirm.addEventListener("click",()=>choose(card,true));cancel.addEventListener("click",()=>choose(card,false));
      dismiss.addEventListener("click",()=>{if(!closed(card.view.phase))return;cards.delete(card.id);box.remove();section.hidden=!cards.size && more.hidden;});
      render(card,view);return card;
    }
    async function detail(card,review=false){
      const owned=generation,version=++card.version;card.read.disabled=true;
      try {
        const v=await api(`interaction/goals/${encodeURIComponent(card.id)}`);
        if(!owns(card,owned)||version!==card.version)return;
        if(v.binding!==getBinding() || v.id!==card.id)throw new Error("Conversation changed; refresh this proposal.");
        if(review){
          card.body.replaceChildren(text("p",v.reason||""),text("p",`Action: ${v.action} · ${v.label}`));
          if(v.plan.expected.goal)card.body.append(text("p",`Selected goal: ${v.plan.expected.goal.objective}`));
          if(v.plan.objective)card.body.append(text("p",`New objective: ${v.plan.objective}`));
          if(v.plan.token_budget)card.body.append(text("p",`Token budget: ${v.plan.token_budget}`));
          if(v.plan.followup)card.body.append(text("p",`Then: ${v.plan.followup}`));
          if(v.result)card.body.append(text("p",v.result.followup_outcome?`Follow-up: ${v.result.followup_outcome}. ${v.result.error||""}`:v.result.error||v.result.meaning||"Outcome retained."));
          card.body.hidden=false;card.reviewed=true;
        }
        render(card,{...card.view,...v});
      }catch(e){if(owns(card,owned)){card.reviewed=false;card.confirm.disabled=card.cancel.disabled=true;card.note.textContent="Current proposal unavailable. Nothing changed; refresh before choosing.";}}
      finally{if(owns(card,owned))card.read.disabled=false;}
    }
    async function observe(page){
      if(!page || page.binding!==getBinding())return;
      message.textContent="";
      const seen=new Set();
      for(const v of page.items){seen.add(v.id);const card=cards.get(v.id)||create(v);render(card,{...card.view,...v});}
      cursor=cursor?Math.min(cursor,page.cursor||cursor):page.cursor;more.hidden=!page.more;section.hidden=!cards.size && more.hidden;
      const absent=[...cards.values()].filter(c=>!seen.has(c.id)&&!closed(c.view.phase));
      const selected=absent.length?Array.from({length:Math.min(8,absent.length)},(_,n)=>absent[(probe+n)%absent.length]):[];probe+=selected.length;
      await Promise.all(selected.map(c=>detail(c)));
    }
    more.addEventListener("click",async()=>{if(reading)return;reading=true;more.disabled=true;const owned=generation;
      try{const page=await api(`interaction/goals?before=${cursor}`);if(owned===generation)await observe(page);}
      catch(_){if(owned===generation)message.textContent="Older controls unavailable. Nothing changed; refresh to reconnect.";}
      finally{if(owned===generation){reading=false;more.disabled=false;}}});
    return {observe,reset,attentionItems:()=>[...cards.values()].filter(c=>c.view&&!closed(c.view.phase)).map(c=>({id:`goal:${c.id}`,label:c.view.label,title:status(c.view),box:c.box,open:()=>detail(c,true)}))};
  }
  return {mount};
})();

// Durable actions share the request ledger, not native question responders.
// Reply only drafts ordinary human text; Send remains the admission boundary.
window.HumanTaskControls = (() => {
  function mount({api,getBinding,setCsrf=()=>{},unauthorized=()=>{},onReply,canReply=()=>true}) {
    const section=document.querySelector('#conversation-tasks'),container=document.querySelector('#conversation-task-cards');
    const state=document.querySelector('#conversation-task-state'),more=document.querySelector('#conversation-tasks-more');
    if(!section||!container||!state||!more)return;
    const cards=new Map(),feedback=new Map();let generation=0,reading=false,cursor=0,probe=0;
    const terminal=v=>['answered','completed','cancelled','withdrawn','expired'].includes(v?.state);
    const el=(tag,text)=>{const n=document.createElement(tag);if(text!==undefined)n.textContent=text;return n;};
    const owns=(card,token)=>token===generation&&cards.get(card.id)===card;
    function reset(){generation++;cards.clear();feedback.clear();container.replaceChildren();section.hidden=true;cursor=probe=0;reading=false;more.hidden=true;state.textContent='';}
    function controls(card){card.dismiss.hidden=!terminal(card.view)&&!(canReply()&&card.view?.can_dismiss===true);card.dismiss.disabled=card.loading||card.unavailable;card.read.disabled=card.loading;card.reply.hidden=!canReply();card.reply.disabled=card.unavailable||card.loading||!card.complete||card.view?.state!=='pending'||card.view.binding!==getBinding();}
    const feedbackText=card=>{
      if(card.view?.state!=='pending')return;
      const input=feedback.get(`${card.id}:${card.view.revision}`);
      if(!input)return;
      return ({accepted:'Your reply reached the conversation; this action awaits reassessment.',routed:'Your reply is routed; this action awaits reassessment.',queued:'Your reply is saved and waiting to send.',retained:'Your reply is saved and waiting to send.',retained_unsent:'Your reply is saved; it has not been sent.',uncertain:'Your reply is saved; delivery is unconfirmed. Check its routing path.',preparing:'Your voice reply is being transcribed.',preparation_unconfirmed:'Your voice reply has no confirmed transcript yet.'})[input.phase];
    };
    function remove(card){cards.delete(card.id);card.box.remove();section.hidden=!cards.size;}
    function observeInput(input){
      const match=/^task:([a-f0-9]{32})$/.exec(input.reply?.source||'');
      if(!match||input.binding!==getBinding()||input.reply.context_only!==true||!input.phase)return;
      feedback.set(`${match[1]}:${input.reply.revision}`,input);
      if(feedback.size>128)feedback.delete(feedback.keys().next().value);
      const card=cards.get(match[1]);if(card&&feedbackText(card))card.status.textContent=feedbackText(card);
    }
    async function dismissTask(card){
      if(terminal(card.view)){remove(card);return;}
      if(!canReply()||card.loading||card.unavailable||card.view?.can_dismiss!==true)return;
      const token=generation,selected={binding:getBinding(),revision:card.view.revision,action:'dismiss'};
      card.loading=true;controls(card);
      try{
        const receipt=await api(`interaction/tasks/${card.id}`,selected);
        if(!owns(card,token))return;
        if(receipt.id!==card.id||receipt.binding!==selected.binding||receipt.revision!==selected.revision||receipt.dismissed!==true)throw Error('Dismissal receipt changed');
        remove(card);
      }catch(error){
        if(!owns(card,token))return;
        try{
          const current=await api(`interaction/tasks/${card.id}?offset=0`);
          if(!owns(card,token))return;
          metadata(card,current);
          if(terminal(card.view)){remove(card);return;}
          card.note.textContent='Dismissal is unconfirmed. Read the current action before trying again.';
        }catch{if(owns(card,token))card.note.textContent='Dismissal is unconfirmed. Reconnect to check the current action.';}
        if([401,403].includes(error.status))unauthorized(error);
      }finally{if(owns(card,token)){card.loading=false;controls(card);}}
    }
    function metadata(card,v){
      if(v.binding!==getBinding())throw Error('Conversation changed; refresh current actions.');
      if(terminal(card.view)&&!terminal(v))return;
      if(card.view?.revision!==v.revision){card.complete=false;card.body.textContent='';card.body.hidden=true;card.links.hidden=true;card.box.dataset.expanded='false';card.preview.hidden=false;card.read.textContent='Read action';card.offset=undefined;card.links.replaceChildren();}
      card.unavailable=false;card.view=v;card.title.textContent=`${v.label} · ${v.title}`;
      card.preview.textContent=v.preview||'';card.status.textContent=({pending:'An action for you.',answering:'Feedback receipt is unconfirmed.',unknown:'Earlier feedback is unconfirmed.',answered:'Feedback recorded.',completed:'Marked done by you.',withdrawn:'No longer needed.',cancelled:'Cancelled.',expired:'This action is closed.'})[v.state]||'Current action unavailable.';
      if(feedbackText(card))card.status.textContent=feedbackText(card);
      controls(card);
    }
    async function detail(card,continueReading=false){
      if(card.loading)return;const token=generation;card.loading=true;controls(card);
      const selectedRevision=card.view?.revision;
      const offset=continueReading?card.offset||0:0,revision=continueReading?card.view.revision:undefined;
      try{
        const query=new URLSearchParams({offset:String(offset)});if(revision)query.set('revision',revision);
        const v=await api(`interaction/tasks/${card.id}?${query}`);
        if(!owns(card,token))return;
        if(card.view?.revision!==selectedRevision&&v.revision!==card.view?.revision || terminal(card.view)&&!terminal(v))return;
        if(v.id!==card.id||revision&&v.revision!==revision||v.offset!==offset)throw Error('Task changed; read its current action.');
        const previous=card.body.textContent;metadata(card,v);
        card.body.textContent=(offset?previous:'')+v.body;card.offset=v.next_offset;card.complete=v.next_offset===null;
        card.preview.hidden=true;card.box.dataset.expanded='true';card.body.hidden=card.links.hidden=false;card.read.textContent=card.complete?'Collapse action':'Continue reading';
        card.links.replaceChildren();for(const link of v.links||[]){
          try{const url=new URL(link.url);if(!['http:','https:'].includes(url.protocol))continue;
            const a=el('a',link.label);a.href=url.href;a.target='_blank';a.rel='noopener noreferrer';card.links.append(a);
          }catch{}
        }
        card.note.textContent='';
      }catch(e){if(owns(card,token)){card.complete=false;card.note.textContent='Action unavailable. Refresh before replying.';if([401,403].includes(e.status))unauthorized(e);}}
      finally{if(owns(card,token)){card.loading=false;controls(card);}}
    }
    function create(v){
      const box=el('article');box.className='reply task-card';box.dataset.expanded='false';const title=el('h3'),status=el('p'),preview=el('p'),body=el('div'),links=el('div'),note=el('p');
      status.className='reply-status';body.className='reply-text';body.hidden=links.hidden=true;links.className='reply-actions';note.setAttribute('role','status');
      const read=el('button','Read action'),reply=el('button','Reply in conversation'),dismiss=el('button','Dismiss');read.type=reply.type=dismiss.type='button';
      box.append(title,status,preview,body,links,read,reply,dismiss,note);container.append(box);
      const card={id:v.id,box,title,status,preview,body,links,note,read,reply,dismiss,complete:false,loading:false};cards.set(v.id,card);
      read.addEventListener('click',()=>{if(card.complete&&box.dataset.expanded==='true'){box.dataset.expanded='false';body.hidden=links.hidden=true;preview.hidden=false;read.textContent='Read action';return;}return detail(card,card.offset!==undefined&&card.offset!==null);});
      dismiss.addEventListener('click',()=>dismissTask(card));
      reply.addEventListener('click',()=>{
        if(!canReply()||!card.complete||card.loading||card.view.state!=='pending'||card.view.binding!==getBinding())return;
        if(onReply?.(card.view)!==true)card.note.textContent='Your draft is kept. Send or clear it before preparing this reply.';
      });return card;
    }
    async function refresh(after=0){
      if(reading||!getBinding())return;reading=true;more.disabled=true;const token=generation;
      try{
        const page=await api(`interaction/tasks?after=${after}`);if(token!==generation)return;
        if(page.binding!==getBinding())throw Error('Conversation changed');setCsrf(page.csrf);
        const seen=new Set();for(const v of page.items){seen.add(v.id);metadata(cards.get(v.id)||create(v),v);}
        cursor=Math.max(cursor,page.cursor);more.hidden=!page.more;
        const absent=[...cards.values()].filter(c=>!seen.has(c.id)&&!terminal(c.view));
        const selected=absent.length?Array.from({length:Math.min(8,absent.length)},(_,n)=>absent[(probe+n)%absent.length]):[];probe+=selected.length;
        await Promise.all(selected.map(c=>detail(c)));if(token!==generation)return;
        section.hidden=!cards.size;state.textContent='You can respond here while sessions keep working.';
      }catch(e){if(token===generation){for(const card of cards.values()){card.unavailable=true;controls(card);}section.hidden=false;state.textContent='Human actions unavailable. Refresh to reconnect.';if([401,403].includes(e.status))unauthorized(e);}}
      finally{if(token===generation){reading=false;more.disabled=false;}}
    }
    more.addEventListener('click',()=>refresh(cursor));return {refresh,reset,observeInput,attentionItems:()=>[...cards.values()].filter(c=>c.view&&!terminal(c.view)).map(c=>({id:`task:${c.id}`,label:c.view.label,title:c.view.title,box:c.box,open:()=>detail(c)}))};
  }
  return {mount};
})();
