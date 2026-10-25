"use strict";
window.Conversation = (() => {
  const $ = (selector) => document.querySelector(selector);
  const node = (tag, text, cls) => {
    const value = document.createElement(tag);
    if (text !== undefined) value.textContent = text;
    if (cls) value.className = cls;
    return value;
  };
  function mount({api, headers, setCsrf, modeChanged, telegram, readOnly=false, standalone=false, onUnauthorized}) {
    let canFiles=false,filesConnected=true;
    const files=new Map(),fileDownloads=new Map(),fileStatus=new Map();let fileJobs=[];
    const fileCredential=()=>JSON.stringify(Object.entries(headers()).filter(([key])=>key.toLowerCase()==="authorization"));
    function clearFiles(){for(const work of fileDownloads.values())work.abort();fileDownloads.clear();files.clear();fileStatus.clear();fileJobs=[];canFiles=false;}
    function renderFiles(card){
      const sameSource=file=>file.native_source===card.source.id || file.result_id && file.result_id===card.source.result_id;
      const selected=[...new Map((canFiles?[...(card.source.artifacts||[]),...[...files.values()].filter(sameSource)]:[]).map(file=>[file.id,file])).values()].map(file=>({...file,available:!!file.available && (!file.expires_at || Date.parse(file.expires_at)>Date.now())}));
      const stage=canFiles?(fileJobs.find(sameSource)?.phase || card.source.artifact_status):null,preparing=stage==="pending" || stage==="running";
      if(!selected.length && !preparing && stage!=="partial"){if(card.files)card.files.hidden=true;return;}
      if(!card.files){card.files=node("div",undefined,"artifact-files");card.files.setAttribute("role","group");card.files.setAttribute("aria-label","Generated files");card.box.append(card.files);}
      const version=JSON.stringify([filesConnected,stage,selected,[...fileDownloads.keys()],[...fileStatus]]);
      if(card.files.dataset.version===version){card.files.hidden=false;return;}
      card.files.dataset.version=version;card.files.hidden=false;card.files.replaceChildren(node("h4","Files"));
      if(preparing || stage==="partial"){const status=node("p",preparing?"Preparing files… You can keep chatting.":"Some files could not be prepared.","reply-status");status.setAttribute("role","status");card.files.append(status);}
      for(const file of selected){
        const row=node("div",undefined,"artifact-file"),button=node("button",`↓ ${file.name||"File"}`),status=node("span",fileStatus.get(file.id)||(!filesConnected?"Files connection unavailable":file.available?`${Math.ceil((file.bytes||0)/1024)} KB`:"Unavailable or expired"),"reply-status");
        button.type="button";button.disabled=!filesConnected || !file.available || fileDownloads.has(file.id) || fileDownloads.size>=2;status.setAttribute("role","status");
        button.addEventListener("click",()=>downloadFile(file));row.append(button,status);card.files.append(row);
      }
    }
    function refreshFiles(){for(const card of cards.values())renderFiles(card);}
    async function downloadFile(file){
      if(!canFiles || fileDownloads.has(file.id) || fileDownloads.size>=2 || !file.available)return;
      if(!/^[a-f0-9-]{36}$/.test(file.id) || !/^[a-f0-9]{64}$/.test(file.sha256) || !Number.isSafeInteger(file.bytes) || file.bytes<0 || file.bytes>256*1024*1024)return;
      const controller=new AbortController(),credential=fileCredential(),deadline=setTimeout(()=>controller.abort(),120000);
      fileDownloads.set(file.id,controller);fileStatus.set(file.id,"Downloading…");refreshFiles();
      try{
        const response=await fetch(`api/interaction/artifacts/${encodeURIComponent(file.id)}`,{headers:headers(),credentials:"same-origin",signal:controller.signal});
        if(!response.ok)throw Object.assign(Error("File unavailable or access changed."),{status:response.status});
        if(response.headers.get("X-Artifact-Sha256")!==file.sha256)throw Error("File changed. Refresh before downloading.");
        const blob=await response.blob();if(blob.size!==file.bytes)throw Error("Download interrupted. Try the same file again.");
        const hash=[...new Uint8Array(await crypto.subtle.digest("SHA-256",await blob.arrayBuffer()))].map(value=>value.toString(16).padStart(2,"0")).join("");
        if(hash!==file.sha256)throw Error("File verification failed. Nothing was saved.");
        if(controller.signal.aborted || fileDownloads.get(file.id)!==controller || !canFiles || fileCredential()!==credential || files.has(file.id) && files.get(file.id).sha256!==file.sha256)return;
        const url=URL.createObjectURL(blob),link=node("a");link.href=url;link.download=file.name;link.hidden=true;document.body.append(link);link.click();link.remove();setTimeout(()=>URL.revokeObjectURL(url),10000);
        fileStatus.set(file.id,"Download ready");
      }catch(error){if(fileDownloads.get(file.id)===controller && fileCredential()===credential){fileStatus.set(file.id,error.name==="AbortError"?"Download stopped. You can try again.":error.message||"File unavailable.");if(error.status===401)failed(error);}}
      finally{clearTimeout(deadline);if(fileDownloads.get(file.id)===controller)fileDownloads.delete(file.id);refreshFiles();}
    }
    let voiceEnabled=false,voiceSuspended=false,captureBusy=false,voiceBusy=false,voiceContext;
    // Reuse the same presence renderer in the fixed header, freeing conversation space.
    if(standalone){
      const presence=$("#conversation-companion"),identity=$("header .identity"),tools=$("header .header-tools"),beacon=$("#conversation-sound-toggle");
      if(presence && identity)identity.append(presence);
      if(tools && beacon){tools.insertBefore(beacon,tools.firstChild);beacon.title="Session sound · Beacon";}
      $("#conversation-sound").append($("#conversation-sound-controls"),$("#conversation-sound-feedback"));
    }
    const companion=standalone?window.ConversationCompanion?.mount():undefined;
    const sound=window.ConversationSound?.mount({api,unauthorized:failed,getAuthority:fileCredential});
    const presentState=patch=>{companion?.update(patch);sound?.update(patch);};
    if(standalone)$("#conversation-diagnostics").open=false;
    const speechGaps={chunk:180,reply:450,question:1200};
    let speechWait;
    const voiceQueue=[],voiceSeen=new Set(),voiceFinalTimes=new Map();
    const requestControls = window.RequestControls.mount({api,getBinding:()=>binding,setCsrf,unauthorized:failed,onReply:selectFocus,onListen:listenTo,getCanListen:()=>canListen,getCanDecide:()=>!readOnly&&!standalone,onSource:offerVoice});
    const taskControls=window.HumanTaskControls?.mount({api,getBinding:()=>binding,setCsrf,unauthorized:failed,canReply:()=>!readOnly,onReply:task=>{
      if(readOnly || posting || draft.value.trim())return false;
      voiceContext=undefined;selectFocus({source:`task:${task.id}`,revision:task.revision,context_only:true},`${task.label} · ${task.title}`);return true;
    }});
    const goalControls=window.GoalControls?.mount({api,getBinding:()=>binding,readOnly,onChoose:message=>{
      if(readOnly || posting || draft.value.trim())return false;
      clearFocus();voiceContext=undefined;draft.value=message;draft.focus?.();compose.scrollIntoView?.({block:"nearest"});return true;
    }});
    const visual = readOnly ? undefined : window.VisualInput?.mount({headers,getBinding:()=>binding,unauthorized:failed});
    const microphone = readOnly ? undefined : window.VoiceInput?.mount({api,headers,getBinding:()=>binding,onInput:(value,version)=>{inputCard(value);if(timeline)timeline.scrollTop=timeline.scrollHeight;if(version===focusVersion)clearFocus();},getReply:()=>{const reference=replyFocus?.reference||voiceContext;return reference&&({reference,version:focusVersion});},getVisual:()=>visual?.captureForVoice?visual.captureForVoice():visual?.capture(),onVisualSent:reference=>visual?.consumed(reference),pauseAudio:beginVoiceCapture,onActivity:(value,detail)=>{captureBusy=value;presentState({capture:detail?.phase,uploading:detail?.uploading,transcribing:detail?.transcribing});if(!value)drainVoice();},unauthorized:failed,telegram});
    const graph = readOnly || standalone ? window.RoutingGraph?.mount({inspect:selection=>{
      const card=selection.input?inputs.get(selection.input):cards.get(selection.source);
      if(!card)return;
      if(selection.input){$("#conversation-sent").open=true;card.trace.open=true;readTrace(card);}
      if(timeline && card.source?.kind==="worker_progress")$("#conversation-diagnostics").open=true;
      card.box.scrollIntoView({block:"nearest"});
    }}) : undefined;
    document.body.classList.toggle("standalone-conversation",standalone);
    if(graph){$("#conversation-flow").hidden=false;$("#conversation-flow").open=readOnly;}
    const timeline = standalone ? $("#conversation-timeline") : undefined;
    const historyTools=timeline?node("div",undefined,"conversation-tools timeline-history"):undefined;
    if(timeline){
      const diagnostics=$("#conversation-diagnostics");
      diagnostics.open=false;
      diagnostics.querySelector?.(".conversation-diagnostics-content")?.prepend($("#conversation-observation"));
      timeline.hidden=false;$("#conversation-replies").hidden=false;$("#conversation-sent").hidden=true;
      $("#conversation-inputs-older").textContent="Earlier messages";
      historyTools.append($("#conversation-inputs-older"),$("#conversation-older"));
      diagnostics.querySelector?.(".conversation-diagnostics-content")?.append(historyTools);
    }
    function atTimelineEnd() {return !!timeline && timeline.scrollHeight-timeline.scrollTop-timeline.clientHeight<80;}
    function placeInTimeline(box, key, timestamp, nearEnd=atTimelineEnd()) {
      if(!timeline)return;
      box.dataset.timelineKey=key;
      box.dataset.timelineTime=String(Date.parse(timestamp)||Number(box.dataset.timelineTime)||Date.now());
      const following=[...timeline.children].find(item=>item!==box &&
        (Number(item.dataset.timelineTime)>Number(box.dataset.timelineTime) ||
        (item.dataset.timelineTime===box.dataset.timelineTime && item.dataset.timelineKey>key)));
      timeline.insertBefore(box,following||null);
      if(nearEnd)timeline.scrollTop=timeline.scrollHeight;
    }
    let graphCursor=0;
    const cards = new Map(),progressCards=new Map(),settledProgress=new Map();
    function clearProgress(){for(const card of progressCards.values())card.box.remove();progressCards.clear();settledProgress.clear();}
    function sessionProgress(source){
      if(!timeline || !source.target || !["worker_progress","worker_result"].includes(source.kind))return;
      const key=`${source.target.chat_id}:${source.target.thread_id??""}`,stamp=Date.parse(source.observed_at)||0,previous=progressCards.get(key);
      if(source.kind==="worker_result"){
        settledProgress.set(key,Math.max(settledProgress.get(key)||0,stamp));
        if(previous && previous.stamp<=stamp){previous.box.remove();progressCards.delete(key);}return;
      }
      if(stamp<=(settledProgress.get(key)||0) || previous && previous.stamp>stamp)return;
      let card=previous;
      if(!card){
        const box=node("article",undefined,"reply session-update"),title=node("h3"),body=node("p",undefined,"reply-text"),inspect=node("button","Inspect update");
        inspect.type="button";box.append(title,body,inspect);card={box,title,body,inspect};progressCards.set(key,card);
        inspect.addEventListener("click",()=>{const original=cards.get(card.source);if(!original)return;$("#conversation-diagnostics").open=true;original.box.scrollIntoView?.({block:"nearest"});});
      }
      card.source=source.id;card.stamp=stamp;card.title.textContent=`Update from ${source.label}`;card.body.textContent=source.preview;
      placeInTimeline(card.box,`session-update:${key}`,source.observed_at);
    }

    // Read visible answers through the existing immutable source endpoint. Two
    // finite readers keep long history from blocking input or flooding the API.
    const textQueue=[];let textJobs=0;
    const textObserver=timeline && window.IntersectionObserver?new window.IntersectionObserver(entries=>{
      for(const entry of entries){const card=cards.get(entry.target.dataset.sourceId);if(card){card.textVisible=entry.isIntersecting;if(card.textVisible)queueText(card);}}
    },{root:timeline,rootMargin:"200px"}):undefined;
    function autoTextEligible(card){return card && card.textVisible && cards.get(card.source.id)===card
      && ["integrator_answer","worker_result","matrix_result"].includes(card.source.kind)
      && card.source.native_binding_current!==false && !card.source.correction_pending
      && Number.isSafeInteger(card.source.text_characters) && card.source.text_characters>Array.from(card.source.preview).length
      && !(card.expanded && card.offset===null) && card.autoRevision!==card.source.revision;}
    function queueText(card){
      if(!autoTextEligible(card))return;
      card.autoWanted=true;
      if(!card.textLoading && !textQueue.includes(card))textQueue.push(card);
      drainText();
    }
    function drainText(){
      while(textJobs<2 && textQueue.length && enabled && ready && !document.hidden){
        const card=textQueue.shift();if(!autoTextEligible(card)||card.textLoading)continue;
        const revision=card.source.revision;textJobs++;
        void readCard(card,true).finally(()=>{
          textJobs--;if(card.autoWanted && card.source.revision!==revision)queueText(card);drainText();
        });
      }
    }
    function clearText(){textQueue.length=0;textObserver?.disconnect();}
    async function readCard(card,automatic=false){
      if(card.textLoading || cards.get(card.source.id)!==card)return;
      const selected=card.source;card.textLoading=true;card.read.disabled=true;card.error.textContent="";
      if(automatic)card.autoRevision=selected.revision;
      try{
        let parts=0;
        do{
          const result=await part(selected,card.offset);
          if(cards.get(selected.id)!==card || card.source.revision!==selected.revision)return;
          const following=atTimelineEnd();
          card.body.textContent=(card.expanded?card.body.textContent:"")+result.text;
          card.expanded=true;card.offset=result.next_offset;
          card.read.hidden=card.offset===null;card.read.textContent="Continue reading";
          if(following && timeline)timeline.scrollTop=timeline.scrollHeight;
          parts++;
        }while(automatic && card.offset!==null && parts<12 && card.body.textContent.length<16000 && enabled && !document.hidden && card.textVisible);
        if(automatic && card.offset!==null && parts<12 && card.body.textContent.length<16000 && (!enabled || document.hidden || !card.textVisible)){
          card.autoRevision=undefined;card.autoWanted=true;
        }
      }catch(error){
        if(cards.get(selected.id)===card && card.source.revision===selected.revision){
          card.error.textContent=error.message;
          if(error.status===401 || error.status===403)failed(error);
        }
      }finally{card.textLoading=false;card.read.disabled=false;if(automatic && card.autoWanted && card.autoRevision===undefined)queueText(card);}
    }

    const inputs = new Map();
    const compose = $("#conversation-compose"), draft = $("#conversation-message"), send = $("#conversation-send");
    const sendState = $("#conversation-send-state");
    const composerNotice=(text,attention=true)=>{sendState.textContent=text;sendState.setAttribute('data-attention',String(attention));};
    const inputNotice=value=>composerNotice(phaseText(value),['awaiting_target','preparation_unconfirmed','retained_unsent','uncertain','reselection'].includes(value.phase) || (!value.phase && !value.routing));
    let replyFocus, focusVersion = 0, routingChoices = [], presentationProfile;
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
    function savedRecovery(card) {
      if(card.value.retired)return null;
      return (card.value.recoveries||[]).find(r=>r.instruction &&
        !(card.value.routing?.instruction===r.instruction && card.value.routing.phase==="accepted"));
    }
    function renderRetry(card) {
      const recovery=savedRecovery(card);
      card.retry.hidden=readOnly || card.value.read_only===true || !card.value.binding || (!recovery && !!card.value.phase);
      card.retry.textContent=recovery?`Try again with #${recovery.label}`:"Retry this same message";
      card.retry.disabled=!!card.retryBusy || !!card.retryNeedsCheck;
      if(card.retryNeedsCheck)card.check.hidden=false;
    }
    function inputActivity() {
      presentState({transcribing:[...inputs.values()].some(c=>c.value.voice && c.value.phase==="preparing"),
        routing:[...inputs.values()].some(c=>["requested","queued","submitting","steering"].includes(c.value.routing?.phase))});
    }
    function resetInputs() {
      companion?.reset();sound?.reset();goalControls?.reset();presentationProfile=undefined;
      disableVoice();graph?.reset();graphCursor=0;timeline?.replaceChildren();
      inputGeneration++; inputs.clear(); inputCursor = 0; inputNewest = 0; inputBinding = undefined;
      latestComposerId = undefined; composerNotice("",false);clearFocus();routingChoices=[];
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
      const followingLive=atTimelineEnd();
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
          const recovery=savedRecovery(card);
          if (readOnly || (card.value.phase && !recovery) || retry.disabled) return;
          const owned = inputGeneration, observation = ++inputObservation;
          card.retryBusy=true;retry.disabled = true;
          try {
            const result = recovery ? await api(`interaction/inputs/${card.value.id}/retry`,{binding:card.value.binding,instruction:recovery.instruction}) : await api("interaction/inputs", {id:card.value.id,binding:card.value.binding,text:card.value.text,...(card.value.reply?{reply:card.value.reply}:{}),...(card.value.visual?{visual:card.value.visual}:{})});
            if (owned === inputGeneration) inputCard(result, observation);
          } catch(e) {
            if (owned !== inputGeneration) return;
            if (recovery)card.retryNeedsCheck=true;
            if (observation >= card.observed) status.textContent = phaseText(card.value) + (recovery?" Recovery unconfirmed. Check saved status before trying again.":" The same message identity was used.");
            if (e.status === 401 || e.status === 403) failed(e);
          } finally {card.retryBusy=false;renderRetry(card);}
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
        renderRetry(card);
        if (card.value.id === latestComposerId) inputNotice(card.value);
        renderRoute(card);graph?.input(card.value);inputActivity();return card;
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
      card.retryNeedsCheck=false;
      if (!card.value.phase && value.phase && draft.value === card.value.text) draft.value = "";
      // Preserve full original text after reading it, and an unknown POST's text.
      card.value = {...card.value, ...value};
      card.imageButton.hidden=!card.value.visual;
      card.text.textContent = card.value.voice ? (card.value.text || card.value.preview ? "Automatic voice transcript:\n"+(card.value.text || card.value.preview) : "Original voice recording saved.") : card.value.text ?? card.value.preview;
      card.status.textContent = phaseText(card.value);
      card.check.hidden=readOnly || card.value.read_only===true || (["accepted","routed","answered"].includes(card.value.phase) && (!!card.value.text || (card.value.preview?.length||0)<1000));
      microphone?.observe(card.value,phaseText(card.value));
      if(card.value.phase && card.value.focusVersion===focusVersion)clearFocus();
      if (card.value.id === latestComposerId) inputNotice(card.value);
      renderRetry(card);
      renderRoute(card);
      card.box.dataset.sequence = String(card.value.capture_order ?? (card.value.ordering_unknown?Number.MIN_SAFE_INTEGER:card.value.sequence ?? Number.MAX_SAFE_INTEGER));
      const container = $("#conversation-inputs");
      const following = [...container.children].find(item => item !== card.box
        && Number(item.dataset.sequence) < Number(card.box.dataset.sequence));
      if(timeline)placeInTimeline(card.box,`input:${card.value.id}`,card.value.received_at,followingLive);
      else container.insertBefore(card.box, following || null);
      graph?.input(card.value);
      inputActivity();
      return card;
    }
    async function inputPage(before = 0, direction = "before", remember = true) {
      const owned = inputGeneration, observation = ++inputObservation;
      const result = await api(`interaction/inputs?before=${before}&direction=${direction}`);
      if (owned !== inputGeneration) return;
      if (result.binding !== binding) throw new Error("Conversation changed. Refresh before sending.");
      if (inputBinding && inputBinding !== result.binding) {resetInputs(); requestControls.reset(); taskControls?.reset(); microphone?.reset(); visual?.reset();}
      inputBinding = result.binding;
      setCsrf(result.csrf);routingChoices=result.routing_choices||[];
      for (const value of result.items) inputCard(value, observation);
      if (direction === "before" && remember) {
        inputCursor = result.cursor || 0;
        $("#conversation-inputs-older").hidden = !result.more;
      }
      if (direction === "after" || (remember && !inputNewest)) inputNewest = Math.max(inputNewest, result.newest || 0);
      draft.disabled = readOnly; send.disabled = readOnly || posting;microphone?.syncControls?.();
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
      if(microphone?.ownsSend?.())return microphone.submit();
      if (readOnly || posting || !ready || !binding || inputBinding !== binding) return;
      const text = draft.value;
      if (!text.trim() || text.includes("\0") || new TextEncoder().encode(text).length > 32768) {
        composerNotice("Write a message of at most 32 KiB."); return;
      }
      if ([...inputs.values()].some(card => !card.value.phase && card.value.text === text)) {
        composerNotice("This message has an unconfirmed submission. Use Check status before sending it again.");
        $("#conversation-sent").open = true; return;
      }
      let selectedVisual;
      posting=true;send.disabled=true;
      try {if(visual)selectedVisual=await visual.capture();}
      catch(e){posting=false;send.disabled=!inputBinding;microphone?.syncControls?.();composerNotice(e.message||"Select your image again.");return;}
      const id = crypto.randomUUID(), owned = inputGeneration, observation = ++inputObservation, focusAtSubmit=focusVersion;
      const context=replyFocus?.reference||voiceContext;
      const reply=context&&JSON.parse(JSON.stringify(context));
      posting = true; send.disabled = true; latestComposerId = id;
      inputCard({id,text,binding,reply,visual:selectedVisual,focusVersion:focusAtSubmit}, observation); if(timeline)timeline.scrollTop=timeline.scrollHeight; $("#conversation-sent").open = true;
      composerNotice("Sending…",false);
      try {
        const result = await api("interaction/inputs", {binding, id, text,...(reply?{reply}:{}),...(selectedVisual?{visual:selectedVisual}:{})});
        if (owned !== inputGeneration) return;
        const confirmed = inputCard(result, observation);
        visual?.consumed(selectedVisual);
        if(focusAtSubmit===focusVersion)clearFocus();
        if (draft.value === text) draft.value = "";
        inputNotice(confirmed.value);
      } catch (e) {
        if (owned !== inputGeneration) return;
        if(e.visualReselection){inputCard({id,phase:"reselection"},observation);composerNotice("Select the image again, then send. This attempt did not submit the message.");return;}
        const retained = inputs.get(id)?.value;
        if (id === latestComposerId) composerNotice(retained?.phase ? phaseText(retained)
          : "Submission unconfirmed. Check this message status; it will not be resent automatically.");
        if (e.status === 401 || e.status === 403) failed(e);
      } finally { posting = false; send.disabled = !inputBinding;microphone?.syncControls?.(); }
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

    const playbackAllowed = () => !document.hidden || (standalone && voiceEnabled && !voiceSuspended);
    function voiceKey(source) {return source.kind==="native_question"?`${source.id}:${source.revision}`:source.id;}
    function voiceStatus() {
      const button=$("#conversation-voice-replies");
      button.checked=voiceEnabled && !voiceSuspended;
      button.setAttribute("aria-pressed",String(voiceEnabled));
      (document.querySelector("#conversation-voice-label")||button).textContent=voiceEnabled?voiceSuspended?"Voice paused":"Voice on":"Voice off";
      button.title=voiceSuspended?"Voice replies paused · tap to resume":`Read new replies aloud${voiceQueue.length?` · ${voiceQueue.length} waiting`:""}`;
    }
    function enableVoice() {
      if(readOnly || !canListen)return;
      const resume=voiceSuspended;voiceSuspended=false;
      if(!voiceEnabled || resume){
        voiceEnabled=true;voiceSeen.clear();
        for(const card of cards.values()){voiceSeen.add(voiceKey(card.source));rememberFinal(card.source);}
        for(const source of requestControls.audioSources?.()||[])voiceSeen.add(voiceKey(source));
      }
      if(active && !audio.ended)voiceBusy=true;
      voiceStatus();
    }
    function beginVoiceCapture() {
      draft.blur?.();enableVoice();voiceBusy=false;stop("Listening to your input. Replies remain in the conversation.");
    }
    const speaker=source=>source.kind.startsWith("integrator_")?"integrator":JSON.stringify(source.target);
    function rememberFinal(source) {
      if(!["worker_result","integrator_answer"].includes(source.kind) || source.current===false || source.correction_pending)return;
      const key=speaker(source),time=Date.parse(source.observed_at)||0;
      voiceFinalTimes.set(key,Math.max(voiceFinalTimes.get(key)||0,time));
    }
    function offerVoice(source) {
      if(!voiceEnabled || voiceSeen.has(voiceKey(source)))return;
      voiceSeen.add(voiceKey(source));
      if(voiceSuspended)return;
      if(active?.source.id===source.id && active.source.revision!==source.revision)stop("This reply changed. Refresh before listening.");
      if(source.current===false || source.correction_pending || source.native_binding_current===false)return;
      const observed=Date.parse(source.observed_at)||0;
      if(source.kind.endsWith("_progress") && observed && observed<=(voiceFinalTimes.get(speaker(source))||0))return;
      rememberFinal(source);
      const sameSpeaker=other=>source.kind.startsWith("integrator_")?other.kind==="integrator_progress":
        other.kind==="worker_progress" && JSON.stringify(other.target)===JSON.stringify(source.target);
      if(source.kind.endsWith("_progress") || ["worker_result","integrator_answer"].includes(source.kind)){
        for(let n=voiceQueue.length-1;n>=0;n--)if(sameSpeaker(voiceQueue[n]))voiceQueue.splice(n,1);
      }
      if(source.kind==="native_question"){
        const nextReply=voiceQueue.findIndex(item=>item.kind!=="native_question");
        voiceQueue.splice(nextReply<0?voiceQueue.length:nextReply,0,source);
      }else voiceQueue.push(source);
      voiceStatus();Promise.resolve().then(drainVoice);
    }
    // A single generation-owned waiting state belongs to this player, including
    // gaps when no audio element is active. Cancellation always settles its waiter.
    function cancelSpeechWait() {
      if(!speechWait)return;
      const previous=speechWait;speechWait=undefined;
      clearTimeout(previous.timer);previous.resolve(false);
    }
    function waitForSpeech(kind, source) {
      cancelSpeechWait();
      const owned=generation;
      return new Promise(resolve=>{
        const waiting={generation:owned,source,resolve,kind};
        speechWait=waiting;
        waiting.timer=setTimeout(()=>{
          if(speechWait!==waiting)return;
          speechWait=undefined;
          resolve(generation===owned && enabled && playbackAllowed() && !captureBusy && !voiceSuspended);
        },speechGaps[kind]);
      });
    }
    async function nextSpokenReply(kind="reply", source) {
      if(!voiceEnabled || voiceSuspended)return;
      voiceBusy=true;
      if(await waitForSpeech(kind,source)){voiceBusy=false;drainVoice();}
    }
    async function drainVoice() {
      if(!voiceEnabled || voiceSuspended || voiceBusy || speechWait || captureBusy || !enabled || !canListen || !playbackAllowed())return;
      const next=voiceQueue.shift();voiceStatus();if(!next)return;
      voiceBusy=true;
      const result=await listenTo(next,0,{queued:true});
      if(result?.failed && result.generation===generation && !active)
        nextSpokenReply("reply",next);
    }
    function disableVoice() {
      voiceEnabled=false;voiceBusy=false;voiceQueue.length=0;voiceFinalTimes.clear();voiceContext=undefined;voiceStatus();stop();
    }
    $("#conversation-voice-replies").addEventListener("click",()=>{
      if(voiceEnabled && !voiceSuspended)disableVoice();else {enableVoice();drainVoice();}
    });
    function stop(text = "Audio stopped.") {
      presentState({playback:undefined});
      cancelSpeechWait();voiceBusy=false;generation++;
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
      presentState({playback:undefined});
      cancelSpeechWait();voiceBusy=false;voiceQueue.length=0;
      voiceSuspended=true;voiceStatus();
      generation++;
      if (!active) return;
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
      $(".identity h1").textContent = standalone ? "Daimon" : readOnly ? "Routing monitor" : enabled ? "Conversation" : "Sessions";
      document.title = standalone ? "Daimon Interaction" : `${enabled ? "Conversation" : "Sessions"} · Telecodex`;
      if (navigate && !readOnly) {
        const url = new URL(location.href);
        if (enabled) url.searchParams.set("view", "conversation");
        else url.searchParams.delete("view");
        history.replaceState(null, "", url);
      }
      try { telegram?.BackButton?.[enabled ? "show" : "hide"]?.(); } catch (_) {}
      if (!enabled) {pause(); sound?.reset(); microphone?.deactivate();}
      modeChanged(enabled, initial);
    }
    function failed(error) {
      presentState({connected:false});
      if (error.status === 401 || error.status === 403) {
        onUnauthorized?.(error);
        clearFiles();
        pause();stop(); cards.clear();clearProgress();clearText(); initialized = false; resetInputs(); requestControls.reset(); taskControls?.reset(); microphone?.reset(); visual?.reset();
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
      const followingLive=atTimelineEnd();
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
        if(!timeline || source.kind==="worker_progress")container.insertBefore(box, following || null);
        card = {source, box, title, status, body, read, listen, error, offset:0, expanded:false};
        cards.set(source.id, card);
        reply.addEventListener("click",()=>selectFocus({source:card.source.id,revision:card.source.revision,...(card.source.kind==="matrix_result"?{context_only:true}:{})},card.source.label||"this session"));
        read.addEventListener("click",()=>readCard(card));
        listen.hidden=readOnly;listen.addEventListener("click", () => !readOnly && listenTo(card.source));
      } else if (card.source.text_sha256 !== source.text_sha256) {
        card.body.textContent = source.preview;
        card.autoRevision=undefined;card.autoWanted=false;
        card.offset = 0; card.expanded = false; card.read.hidden = false;
        card.read.textContent = "Read reply";
      }
      for(let n=voiceQueue.length-1;n>=0;n--)
        if(voiceQueue[n].id===source.id && voiceQueue[n].revision!==source.revision)voiceQueue.splice(n,1);
      if (active?.source.id === source.id && active.source.revision !== source.revision)
        stop("This reply changed. Refresh before listening.");
      const textChanged=card.source.revision!==source.revision;
      card.source = source;card.box.dataset.sourceId=source.id;
      if(!card.expanded && Number.isSafeInteger(source.text_characters) && source.text_characters===Array.from(source.preview).length)card.read.hidden=true;
      for(const file of source.artifacts||[])files.set(file.id,file);
      renderFiles(card);
      const integrator=source.kind.startsWith("integrator_");
      const progress=source.kind.endsWith("_progress");
      card.title.textContent = integrator ? "Conversation" : source.label;
      card.status.textContent = progress ? "Progress update." : source.kind === "integrator_answer" ? "Reply from this conversation."
        : source.correction_pending ? "A later correction is pending."
        : !source.native_binding_current ? "From an earlier session."
        : !source.current ? "Instruction coverage has not been confirmed."
        : source.kind === "matrix_result" ? "Reply from the selected daimon. Work completion is not independently verified."
        : source.completed ? "Session reports this request complete." : "Session progress; work may continue.";
      card.box.dataset.pending = String(!!source.correction_pending || source.native_binding_current === false);
      card.listen.hidden = !canListen;
      if(!progress)graph?.result(source);
      sessionProgress(source);
      if(source.kind!=="worker_progress"){
        placeInTimeline(card.box,source.id,source.observed_at,followingLive);
        if(textChanged)textObserver?.unobserve(card.box);
        textObserver?.observe(card.box);
      }
    }
    async function page(cursor, direction, rememberHistory = true) {
      const value = await api(`interaction?${new URLSearchParams({...cursor, direction})}`);
      if (binding && value.binding !== binding) {
        stop(); cards.clear();clearProgress();clearText(); initialized = false; resetInputs(); requestControls.reset(); taskControls?.reset(); microphone?.reset(); visual?.reset();
        newest = {delivery:0,native:0,matrix:0}; oldest = {delivery:0,native:0,matrix:0};
        $("#conversation-answers").replaceChildren(); $("#conversation-results").replaceChildren();
      }
      if(standalone){
        const profile=value.presentation_profile==="explorer"?"explorer":"simple";
        if(profile!==presentationProfile){
          $("#conversation-diagnostics").open=profile==="explorer";
          document.body.dataset.presentationProfile=profile;
          presentationProfile=profile;
        }
      }
      companion?.configure?.(value.expression_pack,value.expression_pack_sha256);
      companion?.observe(value.activity);
      void sound?.refresh();
      binding = value.binding;
      canFiles=!readOnly && value.can_download_files===true;
      if(!canFiles)clearFiles();
      else if(value.artifacts?.state==="unavailable")filesConnected=false;
      else {filesConnected=true;fileJobs=value.artifacts?.jobs||[];files.clear();for(const card of cards.values())for(const file of card.source.artifacts||[])files.set(file.id,file);for(const file of value.artifacts?.items||[])files.set(file.id,file);}
      refreshFiles();
      await goalControls?.observe(value.goal_controls);
      microphone?.configure(value.can_speak);
      setCsrf(value.csrf);
      canListen = !readOnly && value.can_listen;
      if(!canListen && (voiceEnabled || active || speechWait))pause();
      $("#conversation-voice-replies").disabled=!canListen;
      for (const source of value.items)upsert(source);
      if(direction==="after")for(const source of value.items)companion?.reply(source);
      if(direction==="after")for(const source of [...value.items].sort((a,b)=>(Date.parse(a.observed_at)||0)-(Date.parse(b.observed_at)||0)))offerVoice(source);
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
      if (!enabled || !ready || reading || (!playbackAllowed() && !force)) return;
      drainText();
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
          for (let n = 0; n < 4 && more && enabled && playbackAllowed(); n++) {
            const value = await page(newest, "after");
            if (!initialized) { newest = {delivery:0,native:0,matrix:0}; await page(newest,"before"); initialized = true; break; }
            more = value.more;
          }
          if (enabled && playbackAllowed()) await page({delivery:0,native:0,matrix:0}, "before", false);
        }
        if (active && enabled && playbackAllowed()) {
          const selected = active, owned = generation;
          try { await part(selected.source, selected.offset); }
          catch (_) {
            if (active === selected && generation === owned)
              stop("This reply is no longer current. Refresh before listening.");
          }
        }
        $("#conversation-state").textContent = cards.size ? `${cards.size} ${cards.size===1?"reply":"replies"} available.`
          : "No replies yet. Work can continue while this view is closed.";
        if (!inputBinding) await inputPage();
        else {
          let more = true;
          for (let n=0; n<4 && more; n++) {
            const result = await inputPage(inputNewest,"after"); more = !!result?.more;
          }
          await inputPage(0,"before",false);
        }
        if(!readOnly){
          await requestControls.refresh();
          await taskControls?.refresh();
          const questions=requestControls.audioSources?.()||[];
          presentState({question:questions.length>0});
          const current=source=>questions.some(q=>q.id===source.id && q.revision===source.revision);
          for(let n=voiceQueue.length-1;n>=0;n--)
            if(voiceQueue[n].kind==="native_question" && !current(voiceQueue[n]))voiceQueue.splice(n,1);
          if(active?.source.kind==="native_question" && !current(active.source))
            stop("This question is no longer current. Refresh before answering.");
        }
        await refreshTelegram();
        for(const card of inputs.values())if(card.trace.open)await readTrace(card);
        if(graph){
          const closed=[...inputs.values()].filter(card=>!card.trace.open);
          for(let n=0;n<Math.min(6,closed.length);n++)await readTrace(closed[(graphCursor+n)%closed.length]);
          graphCursor=closed.length?(graphCursor+6)%closed.length:0;
          graph.flush();
        }
        drainVoice();
      } catch (e) { failed(e); }
      finally { reading = false; $("#conversation-refresh").disabled = false; $("#conversation-older").disabled = false; }
    }

    async function startPlayback() {
      const selected = active;
      if (!selected || !blobUrl || !playbackAllowed() || !enabled) return;
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
        selected.segmentEnded = false;
        selected.auto = true;
        audio.muted = false;
        await audio.play();
        if (active !== selected || generation !== owned) return;
        if(replyFocus?.reference.context_only){clearFocus();selected.focusVersion=focusVersion;}
        voiceContext=["worker_result","matrix_result","native_question"].includes(selected.source.kind)
          ?{source:selected.source.id,revision:selected.source.revision,context_only:true}:undefined;
        toggle.textContent = "Pause audio";
        audioState("Playing this reply.");
        presentState({playback:"playing"});
      } catch (e) {
        if (active !== selected || generation !== owned) return;
        selected.verifying = false;
        audio.pause(); audio.muted = true;
        if (e.name === "NotAllowedError") {
          toggle.disabled = false; toggle.textContent = "Play audio";
          audioState("Audio ready. Press Play audio to listen.");
        } else {
          stop("This reply changed or is unavailable. Refresh before listening.");
          return {failed:true,generation};
        }
      }
    }
    async function listenTo(source, offset = 0, chain) {
      if(!chain?.queued)voiceSuspended=false;
      stop();
      if(voiceEnabled)voiceBusy=true;
      const owned = generation;
      active = {source, offset, generation:owned, auto:false, focusVersion:chain?.focusVersion ?? focusVersion,queued:!!chain?.queued};
      const selected = active;
      $("#conversation-player").hidden = false;
      $("#conversation-playing").textContent = source.kind === "integrator_answer" ? "Conversation reply" : source.label;
      presentState({playback:"preparing"});
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
        return await startPlayback();
      } catch (e) {
        if (active !== selected || generation !== owned) return;
        stop();voiceBusy=false;
        if (e.status === 401 || e.status === 403) failed(e);
        $("#conversation-state").textContent = e.name === "AbortError" ? "Audio preparation ended. Your text replies remain available."
          : e.message || "Audio unavailable. Text replies remain available.";
        return {failed:true,generation};
      } finally { clearTimeout(deadline); if (requestAbort === controller) requestAbort = undefined; }
    }
    audio.addEventListener("pause",()=>{if(active && !active.verifying && !requestAbort)presentState({playback:undefined});});
    audio.addEventListener("playing",()=>{
      if(active && !active.verifying && !audio.muted)presentState({playback:"playing"});
    });
    audio.addEventListener("ended", async () => {
      if (!audio.ended || !active || active.segmentEnded || active.verifying || !active.auto || !playbackAllowed() || !enabled) return;
      presentState({playback:undefined});
      const selected=active, owned=generation;selected.segmentEnded=true;
      if (selected.next !== null) {
        audioState("A short pause before continuing…");
        if(await waitForSpeech("chunk",selected.source)){
          const result=await listenTo(selected.source,selected.next,selected);
          if(result?.failed && result.generation===generation && !active)nextSpokenReply("reply",selected.source);
        }
        return;
      }
      audioState("End of reply."); toggle.textContent = "Play audio again";
      if(selected.source.kind!=="native_question" || selected.finished){nextSpokenReply("reply",selected.source);return;}
      selected.finished=true;
      try {
        await part(selected.source, selected.offset);
        if(active!==selected || generation!==owned || !selected.auto || !playbackAllowed() || !enabled || focusVersion!==selected.focusVersion)return;
        if(!voiceEnabled)selectFocus({source:selected.source.id,revision:selected.source.revision,context_only:true},selected.source.label);
        audioState("Question finished. Press Talk to answer or change topic.");
      } catch (_) {
        if(active===selected && generation===owned)stop("This question changed. Refresh before answering.");
      }
      if(voiceEnabled && generation===owned)nextSpokenReply("question",selected.source);
    });
    toggle.addEventListener("click", () => {
      if (audio.paused) { cancelSpeechWait();voiceSuspended=false;voiceStatus();if (audio.ended) audio.currentTime = 0; startPlayback(); }
      else pause();
    });
    $("#conversation-stop").addEventListener("click",()=>{sound?.disable();disableVoice();});
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
    document.addEventListener("visibilitychange", () => {
      if(document.hidden){if(!playbackAllowed())pause();return;}
      if(standalone && voiceEnabled && !voiceSuspended && enabled && ready){
        // A browser suspension preserves intent; a human pause clears active.auto.
        if(active?.auto && audio.ended && !active.segmentEnded)audio.dispatchEvent?.(new Event("ended"));
        else if(active?.auto && audio.paused && blobUrl && !active.verifying)startPlayback();
        else if(!active && !speechWait){voiceBusy=false;drainVoice();}
      }
      refresh(true);
    });
    window.addEventListener("pagehide", event => {
      if(event?.persisted && standalone && voiceEnabled && !voiceSuspended)return;
      pause();stop();clearFiles();
    });
    window.addEventListener("pageshow", event => {
      if(event?.persisted && enabled && ready){
        if(active?.auto && audio.paused && blobUrl && !active.verifying && playbackAllowed())startPlayback();
        refresh(true);
      }
    });
    window.addEventListener("offline", () => {
      presentState({connected:false});
      pause(); $("#conversation-state").textContent = "Offline. Replies already loaded remain readable.";
    });
    window.addEventListener("online", () => { if (enabled && ready && !document.hidden) refresh(true); });
    telegram?.onEvent?.("deactivated", pause);
    telegram?.BackButton?.onClick?.(() => setMode(false));
    if(standalone){$("#conversation-back").hidden=true;$("#conversation-telegram-older").hidden=true;}
    if(readOnly){document.body.classList.toggle("monitor-mode",true);$("#conversation-sent").open=true;}
    setMode(initialMode(), true, false);
    return {refresh, pause, stop:()=>{sound?.disable();disableVoice();},
      unauthenticated() { ready=false; failed({status:401,message:document.documentElement.dataset.sharedAuth==='true'?"Sign in again to reconnect.":"Open your conversation access file to reconnect."}); },
      authenticated() { ready = true; $("#conversation-open").disabled = false; }};
  }
  return {mount};
})();
