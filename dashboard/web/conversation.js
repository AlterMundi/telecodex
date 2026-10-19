"use strict";
window.Conversation = (() => {
  const $ = (selector) => document.querySelector(selector);
  const node = (tag, text, cls) => {
    const value = document.createElement(tag);
    if (text !== undefined) value.textContent = text;
    if (cls) value.className = cls;
    return value;
  };
  function mount({api, headers, setCsrf, modeChanged, telegram, readOnly=false, standalone=false}) {
    const requestControls = window.RequestControls.mount({api,getBinding:()=>binding,setCsrf,unauthorized:failed,onReply:selectFocus,onListen:listenTo,getCanListen:()=>canListen,getCanDecide:()=>!readOnly&&!standalone});
    const visual = readOnly ? undefined : window.VisualInput?.mount({headers,getBinding:()=>binding,unauthorized:failed});
    const microphone = readOnly ? undefined : window.VoiceInput?.mount({api,headers,getBinding:()=>binding,onInput:(value,version)=>{inputCard(value);if(version===focusVersion)clearFocus();},getReply:()=>replyFocus&&({reference:replyFocus.reference,version:focusVersion}),getVisual:()=>visual?.capture(),onVisualSent:reference=>visual?.consumed(reference),pauseAudio:pause,unauthorized:failed,telegram});
    const graph = readOnly || standalone ? window.RoutingGraph?.mount({inspect:selection=>{
      const card=selection.input?inputs.get(selection.input):cards.get(selection.source);
      if(!card)return;
      if(selection.input){$("#conversation-sent").open=true;card.trace.open=true;readTrace(card);}
      card.box.scrollIntoView({block:"nearest"});
    }}) : undefined;
    const timeline = standalone ? $("#conversation-timeline") : undefined;
    if(timeline){
      timeline.hidden=false;$("#conversation-replies").hidden=true;$("#conversation-sent").hidden=true;
      timeline.before($("#conversation-inputs-older"),$("#conversation-older"));
    }
    function placeInTimeline(box, key, timestamp) {
      if(!timeline)return;
      const nearEnd=timeline.scrollHeight-timeline.scrollTop-timeline.clientHeight<80;
      box.dataset.timelineKey=key;
      box.dataset.timelineTime=String(Date.parse(timestamp)||Number(box.dataset.timelineTime)||Date.now());
      const following=[...timeline.children].find(item=>item!==box &&
        (Number(item.dataset.timelineTime)>Number(box.dataset.timelineTime) ||
        (item.dataset.timelineTime===box.dataset.timelineTime && item.dataset.timelineKey>key)));
      timeline.insertBefore(box,following||null);
      if(nearEnd)timeline.scrollTop=timeline.scrollHeight;
    }
    let graphCursor=0;
    const cards = new Map();
    const inputs = new Map();
    const compose = $("#conversation-compose"), draft = $("#conversation-message"), send = $("#conversation-send");
    const sendState = $("#conversation-send-state");
    let replyFocus, focusVersion = 0, routingChoices = [];
    let posting = false, inputCursor = 0, inputNewest = 0, inputBinding, inputGeneration = 0, inputObservation = 0, latestComposerId;
    let telegramCursor=0,telegramNewest=0,telegramLoaded=false,telegramReading=false;
    const phaseText = value => value.origin_kind==="telegram" ? `Telegram input retained (${value.phase}). Inspect its routing path for delivery evidence.${value.ordering_unknown?" Historical capture order unavailable.":""}` : value.retired ? "Saved from an earlier conversation binding."
      : value.routing ? routeText(value.routing) : ({awaiting_target:"Choose the session for this saved message.",preparing:"Recording saved; preparing its automatic transcript.", preparation_unconfirmed:"Recording saved; transcription unconfirmed. No work was submitted automatically.", retained:"Saved; waiting to send.", queued:"Saved; waiting to send.",
          sending:"Sending to your conversation…", steering:"Sending to your conversation…",
          accepted:"Sent to your conversation. Work may continue.", matrix_requested:"Saved for the selected daimon; awaiting its authorized execution and reply.",
          reselection:"This attempt did not submit the message. Select the image again, then send.", answered:"Answer recorded for the selected question. Work may continue.",
          retained_unsent:"Saved; not sent.", uncertain:"Saved; delivery unconfirmed. Check status."})[value.phase]
        || "Submission unconfirmed. Check status before sending this message again.";
    function routeText(route) {
      if(!route.instruction)return `#${route.label}: `+({requested:"turn interruption requested.",no_active_turn:"no selected running turn.",scope_unavailable:"the original turn selection is unavailable; no interruption was made.",unconfirmed:"interruption remains unconfirmed; nothing will be repeated."}[route.phase]||"control outcome retained.");
      return `#${route.label}: `+({accepted:"sent to this session. Work may continue.",queued:"saved; waiting for this session.",submitting:"sending to this session…",steering:"sending to this session…",retained_unsent:"saved; not sent.",uncertain:"delivery unconfirmed. Check status.",cancelled:"not sent; withdrawn.",rejected:"not accepted by this execution."}[route.phase]||"route retained; inspect its current state.");
    }
    function clearFocus() {replyFocus=undefined;focusVersion++;$("#conversation-focus").hidden=true;}
    function selectFocus(reference,label) {
      replyFocus={reference:JSON.parse(JSON.stringify(reference)),label};focusVersion++;
      $("#conversation-focus-label").textContent=`Reply to ${label}`;$("#conversation-focus").hidden=false;
      draft.focus?.();compose.scrollIntoView?.({block:"nearest"});
    }
    $("#conversation-focus-clear").addEventListener("click",clearFocus);
    async function routeInput(card,frozen) {
      if(readOnly || card.routeBusy || inputs.get(card.value.id)!==card)return;
      card.routePending=frozen;card.routeBusy=true;renderRoute(card);
      const owned=inputGeneration,observation=++inputObservation;
      try {
        const result=await api(`interaction/inputs/${card.value.id}/route`,frozen);
        if(owned===inputGeneration && inputs.get(card.value.id)===card)inputCard(result,observation);
      } catch(e) {
        if(owned!==inputGeneration)return;
        card.routeNote.textContent="Selection unconfirmed. Check message status before choosing again.";
        if(e.status===401 || e.status===403)failed(e);
      } finally {if(owned===inputGeneration){card.routeBusy=false;renderRoute(card);}}
    }
    function renderRoute(card) {
      const selected=card.select.value||"";
      if(card.choiceVersion!==JSON.stringify(routingChoices)){
        const placeholder=node("option","Choose a session");placeholder.value="";placeholder.disabled=true;placeholder.selected=true;
        card.select.replaceChildren(placeholder);
        for(const choice of routingChoices){const option=node("option",`#${choice.label}`);option.value=choice.selector;card.select.append(option);}
        card.select.value=routingChoices.some(c=>c.selector===selected)?selected:"";card.choiceVersion=JSON.stringify(routingChoices);
      }
      if(card.value.routing){card.routePending=undefined;card.routeNote.textContent="";}
      card.routeBox.hidden=readOnly || card.value.read_only || card.value.phase!=="awaiting_target";
      card.select.disabled=!!card.routePending || card.routeBusy || card.value.retired;
      card.routeButton.textContent=card.routePending?"Retry this selection":"Send to session";
      card.routeButton.disabled=card.routeBusy || card.value.retired || (!card.routePending && !card.select.value);
      card.continueButton.hidden=readOnly || card.value.read_only || !card.value.continuation;
    }
    function resetInputs() {
      graph?.reset();graphCursor=0;timeline?.replaceChildren();
      inputGeneration++; inputs.clear(); inputCursor = 0; inputNewest = 0; inputBinding = undefined;
      latestComposerId = undefined; sendState.textContent = "";clearFocus();routingChoices=[];
      telegramCursor=0;telegramNewest=0;telegramLoaded=false;
      $("#conversation-inputs").replaceChildren();
      $("#conversation-inputs-older").hidden = true;
      $("#conversation-telegram-older").hidden=true;$("#conversation-telegram-state").textContent="";
      draft.disabled = true; send.disabled = true;
    }
    async function readTrace(card) {
      if (card.traceBusy) return;
      const owned=inputGeneration;
      card.traceBusy=true;card.traceRefresh.disabled=true;
      card.traceState.textContent="Reading saved routing evidence…";
      try {
        const value=await api(`interaction/inputs/${encodeURIComponent(card.value.id)}/trace`);
        if(owned!==inputGeneration || inputs.get(card.value.id)!==card)return;
        if(value.id!==card.value.id || value.schema!=="telecodex.conversation-trace/v1")throw new Error("Wrong routing observation");
        graph?.trace(value);
        if(graph && !card.trace.open)return;
        const original=node("section");
        original.append(node("h3",value.voice?"Voice transcript":"Original text"),node("p",value.text||"No transcript is confirmed yet.","reply-text"));
        traceTextMore(card,original,original.children[1],"original",value.text_next_offset);
        const stages=[original];
        for(const route of value.routes) {
          const step=node("section",undefined,"route-step");
          const relation=({new:"New task",amends:"Correction",context:"Additional context"})[route.relation?.kind]||"Related input";
          step.append(node("h3",`→ #${route.label} · ${relation}`),node("p",routeText({...route,phase:route.transport_phase}),"reply-status"));
          step.append(node("p","Exact original forwarded to this session:","note"),node("p",route.forwarded_original,"reply-text"));
          traceTextMore(card,step,step.children[3],route.instruction,route.forwarded_next_offset);
          if(!route.target_available)step.append(node("p","This session is no longer available in your current scope. Its response is not exposed.","note"));
          for(const result of route.results) {
            const state=!result.current?"Earlier result; a later correction may apply.":result.correction_pending?"A correction is still pending.":result.instruction_covered&&result.completed?"Session declared this input incorporated and the request completed.":"The session has not declared this input fully incorporated and completed.";
            step.append(node("p",state,"reply-status"),node("p",result.text_excerpt,"reply-text"));
            if(result.text_bounded)step.append(node("p","Response excerpt; read the complete reply under From your sessions.","note"));
          }
          if(route.more_results)step.append(node("p","Earlier related results remain in reply history.","note"));
          const identity=node("details"),summary=node("summary","Request references");
          identity.append(summary,node("p",`Instruction: ${route.instruction}\nRequests: ${route.requests.join(", ")}`,"route-identity"));step.append(identity);stages.push(step);
        }
        if(!value.routes.length)stages.push(node("p",value.answer?"Recorded as an answer to a session question; recording an answer does not prove incorporation.":value.routing?routeText(value.routing):"No work-session dispatch is recorded. The input may remain with the integrator or await clarification.","note"));
        stages.push(node("p",value.evidence,"note"));
        card.traceContent.replaceChildren(...stages);
        card.traceState.textContent=`Saved input: ${value.phase}. Observation only; no message was resent.`;
      } catch(e) {
        if(owned!==inputGeneration || inputs.get(card.value.id)!==card)return;
        card.traceState.textContent="Routing observation unavailable. Previous evidence is retained; nothing was resent.";
        if([401,403].includes(e.status))failed(e);
      } finally {card.traceBusy=false;card.traceRefresh.disabled=false;}
    }
    function traceTextMore(card,section,text,source,nextOffset) {
      const cached=card.traceText.get(source);
      if(cached && cached.text.startsWith(text.textContent)) {
        text.textContent=cached.text;nextOffset=cached.nextOffset;
      } else card.traceText.set(source,{text:text.textContent,nextOffset});
      if(nextOffset===null || nextOffset===undefined)return;
      const more=node("button","Read more saved text");more.type="button";section.append(more);
      more.addEventListener("click",async()=>{
        const owned=inputGeneration,offset=nextOffset;more.disabled=true;
        try {
          const value=await api(`interaction/inputs/${encodeURIComponent(card.value.id)}/trace/${encodeURIComponent(source)}?offset=${offset}`);
          if(owned!==inputGeneration || inputs.get(card.value.id)!==card)return;
          if(value.id!==card.value.id || value.source!==source || value.offset!==offset)throw new Error("Wrong saved text");
          text.textContent+=value.text;nextOffset=value.next_offset;
          card.traceText.set(source,{text:text.textContent,nextOffset});
          more.hidden=nextOffset===null;more.textContent="Read more saved text";
        }catch(e){more.textContent="Read unavailable; try again";if([401,403].includes(e.status))failed(e);}
        finally{more.disabled=false;}
      });
    }
    function inputCard(value, observed = ++inputObservation) {
      let card = inputs.get(value.id);
      if (!card) {
        const box = node("article", undefined, "reply");
        const text = node("div", "", "reply-text"), status = node("p", "", "reply-status");
        const check = node("button", "Check status / read message"); check.type = "button";
        const retry = node("button", "Retry this same message"); retry.type = "button"; retry.hidden = true;
        const actions = node("div", undefined, "reply-actions"); actions.append(check,retry);
        const continueButton=node("button","Reply to this session");continueButton.type="button";continueButton.hidden=true;actions.append(continueButton);
        const routeBox=node("div",undefined,"reply-route"),select=node("select"),routeButton=node("button","Send to session"),routeNote=node("p","","reply-error");
        select.setAttribute("aria-label","Destination session");routeButton.type="button";routeBox.hidden=true;routeBox.append(select,routeButton,routeNote);
        const imageButton=node("button","Show selected image"),imageView=node("canvas");imageButton.type="button";imageButton.hidden=true;imageView.hidden=true;imageView.className="input-visual";imageView.setAttribute("role","img");
        imageButton.addEventListener("click",async()=>{
          if(!imageView.hidden){imageView.hidden=true;imageView.width=imageView.height=0;imageButton.textContent="Show selected image";return;}
          const owned=inputGeneration,controller=new AbortController(),deadline=setTimeout(()=>controller.abort(),20000);imageButton.disabled=true;imageButton.textContent="Loading selected image…";
          try{
            const response=await fetch(`api/interaction/inputs/${encodeURIComponent(card.value.id)}/image`,{headers:headers(),credentials:"same-origin",signal:controller.signal});
            if(!response.ok)throw Object.assign(Error("Image unavailable"),{status:response.status});
            const bitmap=await createImageBitmap(await response.blob());
            if(owned!==inputGeneration){bitmap.close();return;}
            const ref=card.value.visual;imageView.width=bitmap.width;imageView.height=bitmap.height;const context=imageView.getContext("2d");context.drawImage(bitmap,0,0);bitmap.close();
            context.strokeStyle="#fff";context.lineWidth=Math.max(3,ref.width/200);context.beginPath();context.arc(ref.point.x,ref.point.y,Math.max(6,ref.width/80),0,2*Math.PI);context.stroke();
            imageView.setAttribute("aria-label",`Captured image. Selected point X ${ref.point.x}, Y ${ref.point.y}.`);imageView.hidden=false;imageButton.textContent="Hide selected image";
          }catch(e){status.textContent="Selected image unavailable. The message was not resent.";if([401,403].includes(e.status))failed(e);}
          finally{clearTimeout(deadline);imageButton.disabled=false;if(imageView.hidden)imageButton.textContent="Show selected image";}
        });
        const trace=node("details",undefined,"routing-trace"),traceSummary=node("summary","Show routing path"),traceState=node("p","","note"),traceContent=node("div"),traceRefresh=node("button","Refresh route");
        traceRefresh.type="button";traceState.setAttribute("role","status");trace.append(traceSummary,traceState,traceContent,traceRefresh);
        if(timeline){box.classList.add?.("human-message");box.append(node("h3","You"));}
        box.append(text,status,actions,routeBox,imageButton,imageView,trace);
        card = {box, text, imageButton,imageView,status, check, retry, value, observed,continueButton,routeBox,select,routeButton,routeNote,trace,traceState,traceContent,traceRefresh,traceText:new Map()};
        trace.addEventListener("toggle",()=>trace.open&&readTrace(card));
        traceRefresh.addEventListener("click",()=>readTrace(card));
        continueButton.addEventListener("click",()=>card.value.continuation&&selectFocus(card.value.continuation,`#${card.value.routing?.label||"session"}`));
        select.addEventListener("change",()=>renderRoute(card));
        routeButton.addEventListener("click",()=>routeInput(card,card.routePending||{binding,selector:select.value})); inputs.set(value.id, card);
        check.addEventListener("click", async () => {
          const owned = inputGeneration, observation = ++inputObservation;
          check.disabled = true;
          try {
            const result = await api(`interaction/inputs/${encodeURIComponent(value.id)}`);
            if (owned !== inputGeneration || inputs.get(value.id) !== card) return;
            inputCard(result, observation);
          } catch (e) {
            if (owned !== inputGeneration) return;
            if (observation >= card.observed) status.textContent = phaseText(card.value) + " Status check unavailable; no resend was made.";
            if (e.status === 401 || e.status === 403) failed(e);
          } finally { check.disabled = false; }
        });
        retry.addEventListener("click", async () => {
          if (readOnly || card.value.phase || retry.disabled) return;
          const owned = inputGeneration, observation = ++inputObservation;
          retry.disabled = true;
          try {
            const result = await api("interaction/inputs", {id:card.value.id,binding:card.value.binding,text:card.value.text,...(card.value.reply?{reply:card.value.reply}:{}),...(card.value.visual?{visual:card.value.visual}:{})});
            if (owned === inputGeneration) inputCard(result, observation);
          } catch(e) {
            if (owned !== inputGeneration) return;
            if (observation >= card.observed) status.textContent = phaseText(card.value) + " The same message identity was used.";
            if (e.status === 401 || e.status === 403) failed(e);
          } finally {retry.disabled=false;}
        });
      }
      if (observed < card.observed) {
        // A request that started earlier can nevertheless observe a later commit.
        if (["accepted","answered","matrix_requested","retained_unsent"].includes(value.phase) &&
            (!["answered","matrix_requested"].includes(card.value.phase) || value.phase===card.value.phase)) {
          if (!card.value.phase && draft.value === card.value.text) draft.value = "";
          card.value.phase = value.phase;
        }
        if(value.routing && (!card.value.routing || (value.routing.phase==="accepted" && card.value.routing.instruction===value.routing.instruction))){
          card.value.routing=value.routing;card.value.phase=value.phase;
          if(value.continuation)card.value.continuation=value.continuation;
        }
        card.value.retired = !!card.value.retired || !!value.retired;
        if(card.value.phase && card.value.focusVersion===focusVersion)clearFocus();
        card.status.textContent = phaseText(card.value);
        microphone?.observe(card.value,phaseText(card.value));
        card.retry.hidden = readOnly || !!card.value.phase || !card.value.binding;
        if (card.value.id === latestComposerId) sendState.textContent = phaseText(card.value);
        renderRoute(card);graph?.input(card.value);return card;
      }
      // Native acceptance and retirement cannot be undone by a late snapshot.
      if (["answered","matrix_requested","retained_unsent"].includes(card.value.phase) ||
          (card.value.phase==="accepted" && !["answered","matrix_requested"].includes(value.phase))) value = {...value,phase:card.value.phase};
      if(card.value.routing) {
        // Destination selection is immutable, even if a later-started request
        // sampled an earlier database state before the route committed.
        if(!value.routing || value.routing.instruction!==card.value.routing.instruction || value.routing.target?.thread_id!==card.value.routing.target?.thread_id)
          value={...value,routing:card.value.routing,phase:card.value.phase,continuation:card.value.continuation};
        else if(card.value.routing.phase==="accepted")value={...value,routing:{...value.routing,phase:"accepted"}};
      }
      value = {...value,retired:!!value.retired || !!card.value.retired};
      card.observed = observed;
      if (!card.value.phase && value.phase && draft.value === card.value.text) draft.value = "";
      // Preserve full original text after reading it, and an unknown POST's text.
      card.value = {...card.value, ...value};
      card.imageButton.hidden=!card.value.visual;
      card.text.textContent = card.value.voice ? (card.value.text || card.value.preview ? "Automatic voice transcript:\n"+(card.value.text || card.value.preview) : "Original voice recording saved.") : card.value.text ?? card.value.preview;
      card.status.textContent = phaseText(card.value);
      card.check.hidden=readOnly || card.value.read_only===true;
      microphone?.observe(card.value,phaseText(card.value));
      if(card.value.phase && card.value.focusVersion===focusVersion)clearFocus();
      if (card.value.id === latestComposerId) sendState.textContent = phaseText(card.value);
      card.retry.hidden = readOnly || !!card.value.phase || !card.value.binding;
      renderRoute(card);
      card.box.dataset.sequence = String(card.value.capture_order ?? (card.value.ordering_unknown?Number.MIN_SAFE_INTEGER:card.value.sequence ?? Number.MAX_SAFE_INTEGER));
      const container = $("#conversation-inputs");
      const following = [...container.children].find(item => item !== card.box
        && Number(item.dataset.sequence) < Number(card.box.dataset.sequence));
      if(timeline)placeInTimeline(card.box,`input:${card.value.id}`,card.value.received_at);
      else container.insertBefore(card.box, following || null);
      graph?.input(card.value);
      return card;
    }
    async function inputPage(before = 0, direction = "before", remember = true) {
      const owned = inputGeneration, observation = ++inputObservation;
      const result = await api(`interaction/inputs?before=${before}&direction=${direction}`);
      if (owned !== inputGeneration) return;
      if (result.binding !== binding) throw new Error("Conversation changed. Refresh before sending.");
      if (inputBinding && inputBinding !== result.binding) {resetInputs(); requestControls.reset(); microphone?.reset(); visual?.reset();}
      inputBinding = result.binding;
      setCsrf(result.csrf);routingChoices=result.routing_choices||[];
      for (const value of result.items) inputCard(value, observation);
      if (direction === "before" && remember) {
        inputCursor = result.cursor || 0;
        $("#conversation-inputs-older").hidden = !result.more;
      }
      if (direction === "after" || (remember && !inputNewest)) inputNewest = Math.max(inputNewest, result.newest || 0);
      draft.disabled = readOnly; send.disabled = readOnly || posting;
      return result;
    }
    async function telegramPage(before=0,direction="before",remember=true) {
      const owned=inputGeneration,observation=++inputObservation;
      const result=await api(`interaction/telegram-inputs?before=${before}&direction=${direction}`);
      if(owned!==inputGeneration)return;
      if(result.binding!==binding || result.schema!=="telecodex.telegram-input-list/v1")throw new Error("Telegram observation binding changed");
      for(const value of result.items)inputCard(value,observation);
      if(direction==="before" && remember){telegramCursor=result.cursor||0;$("#conversation-telegram-older").hidden=!result.more;}
      if(direction==="after" || !telegramLoaded)telegramNewest=Math.max(telegramNewest,result.newest||0);
      telegramLoaded=true;
      return result;
    }
    async function refreshTelegram() {
      if(standalone || telegramReading || !enabled || !ready || !$("#conversation-sent").open)return;
      const owned=inputGeneration;telegramReading=true;
      try {
        if(!telegramLoaded)await telegramPage();
        else {
          let more=true;
          for(let n=0;n<4 && more;n++){const page=await telegramPage(telegramNewest,"after",false);more=!!page?.more;}
          if(owned===inputGeneration)await telegramPage(0,"before",false);
        }
        if(owned===inputGeneration)$("#conversation-telegram-state").textContent="Telegram and app/device inputs share the retained capture order. Historical inputs without that order are labelled separately.";
      }catch(e){if(owned===inputGeneration)$("#conversation-telegram-state").textContent="Telegram observation unavailable. Saved app/device inputs remain visible; nothing was resent.";if([401,403].includes(e.status))failed(e);}
      finally{telegramReading=false;}
    }
    $("#conversation-sent").addEventListener("toggle",()=>refreshTelegram());
    $("#conversation-telegram-older").addEventListener("click",async()=>{
      const button=$("#conversation-telegram-older");button.disabled=true;
      try{await telegramPage(telegramCursor);}catch(e){$("#conversation-telegram-state").textContent="Earlier Telegram inputs unavailable; nothing was resent.";if([401,403].includes(e.status))failed(e);}
      finally{button.disabled=false;}
    });
    compose.addEventListener("submit", async event => {
      event.preventDefault();
      if (readOnly || posting || !ready || !binding || inputBinding !== binding) return;
      const text = draft.value;
      if (!text.trim() || text.includes("\0") || new TextEncoder().encode(text).length > 32768) {
        sendState.textContent = "Write a message of at most 32 KiB."; return;
      }
      if ([...inputs.values()].some(card => !card.value.phase && card.value.text === text)) {
        sendState.textContent = "This message has an unconfirmed submission. Use Check status before sending it again.";
        $("#conversation-sent").open = true; return;
      }
      let selectedVisual;
      posting=true;send.disabled=true;
      try {if(visual)selectedVisual=await visual.capture();}
      catch(e){posting=false;send.disabled=!inputBinding;sendState.textContent=e.message||"Select your image again.";return;}
      const id = crypto.randomUUID(), owned = inputGeneration, observation = ++inputObservation, focusAtSubmit=focusVersion;
      const reply=replyFocus?.reference&&JSON.parse(JSON.stringify(replyFocus.reference));
      posting = true; send.disabled = true; latestComposerId = id;
      inputCard({id,text,binding,reply,visual:selectedVisual,focusVersion:focusAtSubmit}, observation); $("#conversation-sent").open = true;
      sendState.textContent = "Sending…";
      try {
        const result = await api("interaction/inputs", {binding, id, text,...(reply?{reply}:{}),...(selectedVisual?{visual:selectedVisual}:{})});
        if (owned !== inputGeneration) return;
        const confirmed = inputCard(result, observation);
        visual?.consumed(selectedVisual);
        if(focusAtSubmit===focusVersion)clearFocus();
        if (draft.value === text) draft.value = "";
        sendState.textContent = phaseText(confirmed.value);
      } catch (e) {
        if (owned !== inputGeneration) return;
        if(e.visualReselection){inputCard({id,phase:"reselection"},observation);sendState.textContent="Select the image again, then send. This attempt did not submit the message.";return;}
        const retained = inputs.get(id)?.value;
        if (id === latestComposerId) sendState.textContent = retained?.phase ? phaseText(retained)
          : "Submission unconfirmed. Check this message status; it will not be resent automatically.";
        if (e.status === 401 || e.status === 403) failed(e);
      } finally { posting = false; send.disabled = !inputBinding; }
    });
    $("#conversation-inputs-older").addEventListener("click", async () => {
      const button = $("#conversation-inputs-older"); button.disabled = true;
      try { await inputPage(inputCursor); } catch (e) { failed(e); }
      finally { button.disabled = false; }
    });

    const audio = $("#conversation-audio");
    audio.controls = false;
    audio.hidden = true;
    const toggle = node("button", "Play audio");
    toggle.type = "button";
    toggle.id = "conversation-play";
    audio.before(toggle);
    let enabled = false, ready = false, reading = false, binding, canListen = false;
    let newest = {delivery:0, native:0, matrix:0}, oldest = {delivery:0, native:0, matrix:0};
    let initialized = false, active, generation = 0, requestAbort, blobUrl;
    const audioState = (text) => { $("#conversation-audio-state").textContent = text; };
    const initialMode = () => readOnly || new URL(location.href).searchParams.get("view") === "conversation"
      || new URLSearchParams(telegram?.initData || "").get("start_param") === "conversation";

    function stop(text = "Audio stopped.") {
      generation++;
      requestAbort?.abort(); requestAbort = undefined;
      active = undefined;
      audio.pause(); audio.muted = true;
      audio.removeAttribute("src"); audio.load();
      if (blobUrl) URL.revokeObjectURL(blobUrl);
      blobUrl = undefined;
      toggle.disabled = true;
      audioState(text);
      $("#conversation-player").hidden = true;
    }
    function pause() {
      if (!active) return;
      generation++;
      requestAbort?.abort(); requestAbort = undefined;
      active.generation = generation;
      active.auto = false;
      audio.pause(); audio.muted = true;
      toggle.textContent = "Play audio";
      toggle.disabled = !blobUrl;
      audioState("Paused. Your replies remain available.");
    }
    function setMode(value, initial = false, navigate = true) {
      if(standalone && !value)return;
      enabled = value;
      document.body.classList.toggle("conversation-mode", enabled);
      $("#conversation").hidden = !enabled;
      $(".identity h1").textContent = readOnly ? "Routing monitor" : enabled ? "Conversation" : "Sessions";
      document.title = `${enabled ? "Conversation" : "Sessions"} · Telecodex`;
      if (navigate && !readOnly) {
        const url = new URL(location.href);
        if (enabled) url.searchParams.set("view", "conversation");
        else url.searchParams.delete("view");
        history.replaceState(null, "", url);
      }
      try { telegram?.BackButton?.[enabled ? "show" : "hide"]?.(); } catch (_) {}
      if (!enabled) {pause(); microphone?.deactivate();}
      modeChanged(enabled, initial);
    }
    function failed(error) {
      if (error.status === 401 || error.status === 403) {
        stop(); cards.clear(); initialized = false; resetInputs(); requestControls.reset(); microphone?.reset(); visual?.reset();
        $("#conversation-answers").replaceChildren();
        $("#conversation-results").replaceChildren();
      } else pause();
      $("#conversation-state").textContent = error.name === "TypeError" ? "Connection unavailable. Refresh to reconnect."
        : error.message || "Replies are unavailable. Refresh to continue.";
    }
    function part(source, offset) {
      const query = new URLSearchParams({source:source.id, revision:source.revision, offset:String(offset)});
      return api(`interaction/part?${query}`);
    }
    function upsert(source) {
      let card = cards.get(source.id);
      if (!card) {
        const container = $(source.kind === "integrator_answer" ? "#conversation-answers" : "#conversation-results");
        const box = node("article", undefined, "reply");
        const title = node("h3");
        const status = node("p", "", "reply-status");
        const body = node("div", source.preview, "reply-text");
        const actions = node("div", undefined, "reply-actions");
        const read = node("button", "Read reply"); read.type = "button";
        const listen = node("button", "Listen"); listen.type = "button";
        const error = node("p", "", "reply-error"); error.setAttribute("role", "status");
        const reply=node("button","Reply to this session");reply.type="button";reply.hidden=readOnly || !["worker_result","matrix_result"].includes(source.kind);
        actions.append(read, listen,reply); box.append(title, status, body, actions, error);
        box.dataset.sequence = String(source.sequence);
        const following = [...container.children].find((item) => Number(item.dataset.sequence) < source.sequence);
        if(!timeline)container.insertBefore(box, following || null);
        card = {source, box, title, status, body, read, listen, error, offset:0, expanded:false};
        cards.set(source.id, card);
        reply.addEventListener("click",()=>selectFocus({source:card.source.id,revision:card.source.revision,...(card.source.kind==="matrix_result"?{context_only:true}:{})},card.source.label||"this session"));
        read.addEventListener("click", async () => {
          read.disabled = true; error.textContent = "";
          const selected = card.source;
          try {
            const result = await part(selected, card.offset);
            if (cards.get(selected.id) !== card) return;
            if (card.source.revision !== selected.revision) throw new Error("Reply changed. Refresh before continuing.");
            body.textContent = (card.expanded ? body.textContent : "") + result.text;
            card.expanded = true;
            card.offset = result.next_offset;
            read.hidden = card.offset === null;
            read.textContent = "Continue reading";
          } catch (e) { error.textContent = e.message; if (e.status === 401 || e.status === 403) failed(e); }
          finally { read.disabled = false; }
        });
        listen.hidden=readOnly;listen.addEventListener("click", () => !readOnly && listenTo(card.source));
      } else if (card.source.text_sha256 !== source.text_sha256) {
        card.body.textContent = source.preview;
        card.offset = 0; card.expanded = false; card.read.hidden = false;
        card.read.textContent = "Read reply";
      }
      if (active?.source.id === source.id && active.source.revision !== source.revision)
        stop("This reply changed. Refresh before listening.");
      card.source = source;
      const integrator=source.kind.startsWith("integrator_");
      const progress=source.kind.endsWith("_progress");
      card.title.textContent = integrator ? "Conversation" : source.label;
      card.status.textContent = progress ? "Working…" : source.kind === "integrator_answer" ? "Reply from this conversation."
        : source.correction_pending ? "A later correction is pending."
        : !source.native_binding_current ? "From an earlier session."
        : !source.current ? "Instruction coverage has not been confirmed."
        : source.kind === "matrix_result" ? "Reply from the selected daimon. Work completion is not independently verified."
        : source.completed ? "Session reports this request complete." : "Session progress; work may continue.";
      card.box.dataset.pending = String(!!source.correction_pending || source.native_binding_current === false);
      card.listen.hidden = !canListen;
      if(!progress)graph?.result(source);
      placeInTimeline(card.box,source.id,source.observed_at);
    }
    async function page(cursor, direction, rememberHistory = true) {
      const value = await api(`interaction?${new URLSearchParams({...cursor, direction})}`);
      if (binding && value.binding !== binding) {
        stop(); cards.clear(); initialized = false; resetInputs(); requestControls.reset(); microphone?.reset(); visual?.reset();
        newest = {delivery:0,native:0,matrix:0}; oldest = {delivery:0,native:0,matrix:0};
        $("#conversation-answers").replaceChildren(); $("#conversation-results").replaceChildren();
      }
      binding = value.binding;
      microphone?.configure(value.can_speak);
      setCsrf(value.csrf);
      canListen = !readOnly && value.can_listen;
      for (const source of value.items) upsert(source);
      for (const card of cards.values()) card.listen.hidden = !canListen;
      if (direction === "before" && rememberHistory) {
        oldest = value.cursor;
        $("#conversation-older").hidden = !value.more;
      }
      // Recent metadata may jump ahead of a backlog still being read forward.
      if (direction === "after" || (rememberHistory && !initialized)) {
        newest.delivery = Math.max(newest.delivery, value.newest.delivery);
        newest.native = Math.max(newest.native, value.newest.native);
        newest.matrix = Math.max(newest.matrix || 0, value.newest.matrix || 0);
      }
      return value;
    }
    async function refresh(force = false) {
      if (!enabled || !ready || reading || (document.hidden && !force)) return;
      reading = true;
      $("#conversation-refresh").disabled = true;
      $("#conversation-older").disabled = true;
      try {
        if (!initialized) {
          newest = {delivery:0,native:0,matrix:0};
          await page({delivery:0,native:0,matrix:0}, "before");
          initialized = true;
        } else {
          let more = true;
          for (let n = 0; n < 4 && more && enabled && !document.hidden; n++) {
            const value = await page(newest, "after");
            if (!initialized) { newest = {delivery:0,native:0,matrix:0}; await page(newest,"before"); initialized = true; break; }
            more = value.more;
          }
          if (enabled && !document.hidden) await page({delivery:0,native:0,matrix:0}, "before", false);
        }
        if (active && enabled && !document.hidden) {
          const selected = active, owned = generation;
          try { await part(selected.source, selected.offset); }
          catch (_) {
            if (active === selected && generation === owned)
              stop("This reply is no longer current. Refresh before listening.");
          }
        }
        $("#conversation-state").textContent = cards.size ? `${cards.size} replies available.`
          : "No replies yet. Work can continue while this view is closed.";
        if (!inputBinding) await inputPage();
        else {
          let more = true;
          for (let n=0; n<4 && more; n++) {
            const result = await inputPage(inputNewest,"after"); more = !!result?.more;
          }
          await inputPage(0,"before",false);
        }
        if(!readOnly)await requestControls.refresh();
        await refreshTelegram();
        for(const card of inputs.values())if(card.trace.open)await readTrace(card);
        if(graph){
          const closed=[...inputs.values()].filter(card=>!card.trace.open);
          for(let n=0;n<Math.min(6,closed.length);n++)await readTrace(closed[(graphCursor+n)%closed.length]);
          graphCursor=closed.length?(graphCursor+6)%closed.length:0;
          graph.flush();
        }
      } catch (e) { failed(e); }
      finally { reading = false; $("#conversation-refresh").disabled = false; $("#conversation-older").disabled = false; }
    }

    async function startPlayback() {
      const selected = active;
      if (!selected || !blobUrl || document.hidden || !enabled) return;
      const owned = selected.generation;
      const position = audio.currentTime;
      selected.verifying = true;
      audio.muted = true;
      // Unlock this element on a user gesture; source validation happens muted.
      try {
        const started = audio.play();
        const check = part(selected.source, selected.offset);
        await Promise.all([started, check]);
        if (active !== selected || generation !== owned) return;
        audio.pause(); audio.currentTime = position;
        selected.verifying = false;
        selected.auto = true;
        audio.muted = false;
        await audio.play();
        if (active !== selected || generation !== owned) return;
        toggle.textContent = "Pause audio";
        audioState("Playing this reply.");
      } catch (e) {
        if (active !== selected || generation !== owned) return;
        selected.verifying = false;
        audio.pause(); audio.muted = true;
        if (e.name === "NotAllowedError") {
          toggle.disabled = false; toggle.textContent = "Play audio";
          audioState("Audio ready. Press Play audio to listen.");
        } else stop("This reply changed or is unavailable. Refresh before listening.");
      }
    }
    async function listenTo(source, offset = 0, chain) {
      stop();
      const owned = generation;
      active = {source, offset, generation:owned, auto:false, focusVersion:chain?.focusVersion ?? focusVersion};
      const selected = active;
      $("#conversation-player").hidden = false;
      $("#conversation-playing").textContent = source.kind === "integrator_answer" ? "Conversation reply" : source.label;
      audioState("Preparing audio…"); toggle.disabled = true;
      requestAbort = new AbortController();
      const controller = requestAbort;
      const deadline = setTimeout(() => controller.abort(), 145000);
      try {
        const response = await fetch("api/interaction/audio", {method:"POST",credentials:"same-origin",
          headers:headers(),signal:controller.signal,body:JSON.stringify({source:source.id,revision:source.revision,offset})});
        if (!response.ok) {
          const value = await response.json();
          const error = new Error(value.detail || "Audio unavailable. Continue reading or refresh.");
          error.status = response.status; throw error;
        }
        if (response.headers.get("x-presentation-source") !== source.id
            || response.headers.get("x-presentation-revision") !== source.revision
            || response.headers.get("x-presentation-offset") !== String(offset)) throw new Error("Audio source mismatch");
        const stream = response.body.getReader(); const chunks = []; let length = 0;
        while (true) {
          const {value, done} = await stream.read(); if (done) break;
          length += value.byteLength;
          if (length > 8 * 1024 * 1024) { await stream.cancel(); throw new Error("Audio exceeds playback limit"); }
          chunks.push(value);
        }
        const blob = new Blob(chunks, {type:"audio/wav"});
        const hash = [...new Uint8Array(await crypto.subtle.digest("SHA-256", await blob.arrayBuffer()))]
          .map((byte) => byte.toString(16).padStart(2,"0")).join("");
        if (hash !== response.headers.get("x-audio-sha256")) throw new Error("Audio integrity check failed");
        if (active !== selected || generation !== owned) return;
        const next = response.headers.get("x-presentation-next-offset");
        selected.next = next === "" ? null : Number(next);
        if (selected.next !== null && (!Number.isSafeInteger(selected.next) || selected.next <= offset)) throw new Error("Invalid audio continuation");
        blobUrl = URL.createObjectURL(blob);
        audio.src = blobUrl;
        toggle.disabled = false;
        await startPlayback();
      } catch (e) {
        if (active !== selected || generation !== owned) return;
        stop();
        if (e.status === 401 || e.status === 403) failed(e);
        $("#conversation-state").textContent = e.name === "AbortError" ? "Audio preparation ended. Your text replies remain available."
          : e.message || "Audio unavailable. Text replies remain available.";
      } finally { clearTimeout(deadline); if (requestAbort === controller) requestAbort = undefined; }
    }
    audio.addEventListener("ended", async () => {
      if (!audio.ended || !active || active.verifying || !active.auto || document.hidden || !enabled) return;
      const selected=active, owned=generation;
      if (selected.next !== null) return listenTo(selected.source, selected.next, selected);
      audioState("End of reply."); toggle.textContent = "Play audio again";
      if(selected.source.kind!=="native_question" || selected.finished)return;
      selected.finished=true;
      try {
        await part(selected.source, selected.offset);
        if(active!==selected || generation!==owned || !selected.auto || document.hidden || !enabled || focusVersion!==selected.focusVersion)return;
        selectFocus({source:selected.source.id,revision:selected.source.revision},selected.source.label);
        audioState("Question finished. Press Talk to answer.");
      } catch (_) {
        if(active===selected && generation===owned)stop("This question changed. Refresh before answering.");
      }
    });
    toggle.addEventListener("click", () => {
      if (audio.paused) { if (audio.ended) audio.currentTime = 0; startPlayback(); }
      else pause();
    });
    $("#conversation-stop").addEventListener("click", () => stop());
    $("#conversation-open").addEventListener("click", () => setMode(true));
    $("#conversation-back").addEventListener("click", () => setMode(false));
    $("#conversation-refresh").addEventListener("click", () => refresh(true));
    $("#conversation-older").addEventListener("click", async () => {
      if (reading) return;
      reading = true;
      $("#conversation-older").disabled = true;
      try { await page(oldest, "before"); }
      catch (e) { failed(e); }
      finally { reading = false; $("#conversation-older").disabled = false; }
    });
    document.addEventListener("visibilitychange", () => { if (document.hidden) pause(); });
    window.addEventListener("pagehide", () => stop());
    window.addEventListener("offline", () => {
      pause(); $("#conversation-state").textContent = "Offline. Replies already loaded remain readable.";
    });
    window.addEventListener("online", () => { if (enabled && ready && !document.hidden) refresh(true); });
    telegram?.onEvent?.("deactivated", pause);
    telegram?.BackButton?.onClick?.(() => setMode(false));
    if(standalone){$("#conversation-back").hidden=true;$("#conversation-telegram-older").hidden=true;}
    if(readOnly){document.body.classList.toggle("monitor-mode",true);$("#conversation-sent").open=true;}
    setMode(initialMode(), true, false);
    return {refresh, pause, stop,
      unauthenticated() { ready=false; failed({status:401,message:"Open your conversation access file to reconnect."}); },
      authenticated() { ready = true; $("#conversation-open").disabled = false; }};
  }
  return {mount};
})();
