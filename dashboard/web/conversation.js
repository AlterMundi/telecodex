"use strict";
window.Conversation = (() => {
  const $ = (selector) => document.querySelector(selector);
  const node = (tag, text, cls) => {
    const value = document.createElement(tag);
    if (text !== undefined) value.textContent = text;
    if (cls) value.className = cls;
    return value;
  };
  function messageTime(element, timestamp) {
    const date = new Date(timestamp);
    element.hidden = !timestamp || !Number.isFinite(date.getTime());
    if(element.hidden)return;
    const locale="en"; // Match the current English UI; full localization is tracked separately.
    const weekday=new Intl.DateTimeFormat(locale,{weekday:"short"}).format(date).replace(/\.$/,"").toLocaleLowerCase(locale);
    element.textContent=`${weekday} ${String(date.getHours()).padStart(2,"0")}:${String(date.getMinutes()).padStart(2,"0")}`;
    element.dateTime=date.toISOString();
    element.title=date.toLocaleString(locale,{dateStyle:"full",timeStyle:"long",hour12:false});
  }
  function mount({api, headers, setCsrf, modeChanged, telegram, readOnly=false, standalone=false, onUnauthorized}) {
    const timing=readOnly?undefined:window.ConversationTiming?.mount();
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
    let voiceEnabled=false,voiceSuspended=false,captureBusy=false,voiceBusy=false,voiceContext,voiceMode,conversationOnly=false,voiceModePrevious;
    // Reuse the same presence renderer in the fixed header, freeing conversation space.
    if(standalone){
      const presence=$("#conversation-companion"),identity=$("header .identity"),tools=$("header .header-tools"),beacon=$("#conversation-sound-toggle");
      if(presence && identity)identity.append(presence);
      const build=$("#client-build"),labels=presence?.querySelector?.("div");
      if(build && labels)labels.prepend(build);
      if(tools && beacon){tools.insertBefore(beacon,tools.firstChild);beacon.title="Session sound · Beacon";}
      $("#conversation-sound").append($("#conversation-sound-controls"),$("#conversation-sound-feedback"));
      const compose = $("#conversation-compose"), playback = $("#conversation-audio-tools");
      const dock = node("div"); dock.id = "conversation-dock";
      compose.before(dock); dock.append(playback, compose);
    }
    const companion=standalone?window.ConversationCompanion?.mount():undefined;
    if(standalone){
      const line=$("#conversation-presence"),caption=$("#conversation-companion .companion-caption"),phases=$("#conversation-companion .companion-phases");
      if(line && caption){
        const track=document.createElement("div");track.className="companion-status-track";
        if(phases){phases.setAttribute("role","presentation");phases.removeAttribute?.("aria-live");track.append(phases);}track.append(caption);line.append(track);line.hidden=false;
        let motion;const reduced=window.matchMedia?.("(prefers-reduced-motion: reduce)");
        const fit=()=>{
          motion?.cancel();motion=undefined;track.title=track.textContent;
          const distance=track.scrollWidth-line.clientWidth;
          if(distance>1 && !reduced?.matches){
            const travel=distance/18*1000,duration=travel+2000;
            motion=track.animate?.([{transform:"translateX(0)",offset:0},{transform:"translateX(0)",offset:1000/duration},{transform:`translateX(${-distance}px)`,offset:(1000+travel)/duration},{transform:`translateX(${-distance}px)`,offset:1}],{duration,iterations:Infinity,direction:"alternate",easing:"linear"});
          }
        };
        if(window.ResizeObserver)new window.ResizeObserver(fit).observe(line);
        if(window.MutationObserver)new window.MutationObserver(fit).observe(track,{childList:true,characterData:true,subtree:true});
        reduced?.addEventListener?.("change",fit);fit();
      }
    }
    const sound=window.ConversationSound?.mount({api,unauthorized:failed,getAuthority:fileCredential,onState:value=>companion?.sonic?.(value)});
    const presentState=patch=>{companion?.update(patch);sound?.update(patch);voiceMode?.update(patch);};
    let inspectorReturnFocus;
    if(standalone){
      const inspector=$("#conversation-diagnostics"),inspectButton=$("#conversation-inspect");
      inspector.open=false;inspectButton.hidden=false;
      inspector.setAttribute("role","region");inspector.setAttribute("aria-label","Under the hood");
      const placeInspector=()=>{const header=$("header");if(header?.getBoundingClientRect)inspector.style.setProperty("--inspector-top",`${header.getBoundingClientRect().bottom+8}px`);};
      inspectButton.addEventListener("click",()=>{inspector.open=!inspector.open;placeInspector();if(inspector.open)inspector.querySelector?.("summary")?.focus?.();});
      inspector.addEventListener("toggle",()=>{inspectButton.setAttribute("aria-expanded",String(inspector.open));if(inspector.open)placeInspector();else {const target=inspectorReturnFocus||inspectButton;inspectorReturnFocus=undefined;target.focus?.({preventScroll:true});}});
      document.addEventListener("keydown",event=>{if(event.key==="Escape" && inspector.open){inspector.open=false;inspectButton.focus?.();}});
      window.addEventListener("resize",placeInspector);
      if(window.ResizeObserver)new window.ResizeObserver(placeInspector).observe($("header"));
      const apps=$(".apps-return");if(apps){apps.title="Apps";apps.setAttribute("aria-label","Apps");}
      placeInspector();
    }
    const speechGaps={chunk:180,reply:450,question:1200};
    let speechWait;
    const voiceQueue=[],voiceSeen=new Set(),voiceFinalTimes=new Map();
    const queuedPreparations=new Map(),skippedMessages=new Map();
    const messageKey=source=>sameNative(source,source)?JSON.stringify([source.native_turn,source.native_item]):JSON.stringify([source?.id,source?.revision]);
    const sameMessage=(a,b)=>a && b && (a.id===b.id && a.revision===b.revision || sameNative(a,b));
    const preparationKey=(source,offset=source?.speech_start||0)=>JSON.stringify([source?.id,source?.revision,offset]);
    const sameNative=(a,b)=>a && b && typeof a.native_turn==="string" && typeof a.native_item==="string" && a.native_turn===b.native_turn && a.native_item===b.native_item;
    function removeQueued(index){
      const [source]=voiceQueue.splice(index,1),key=preparationKey(source),prepared=queuedPreparations.get(key);
      prepared?.controller.abort();queuedPreparations.delete(key);return source;
    }
    function clearVoiceQueue(){for(const prepared of queuedPreparations.values())prepared.controller.abort();queuedPreparations.clear();voiceQueue.length=0;}
    function cancelPreparedMessage(source){
      if(sameNative(prefetched?.source||prefetched?.owner,source)){prefetched.controller.abort();prefetched=undefined;}
      for(let n=voiceQueue.length-1;n>=0;n--)if(sameNative(voiceQueue[n],source) && queuedPreparations.has(preparationKey(voiceQueue[n])))removeQueued(n);
    }
    function parkPrepared(){
      if(!prefetched?.ahead)return;
      const prepared=prefetched;prefetched=undefined;
      if(!prepared.source || active?.interrupted){prepared.controller.abort();return;}
      const source={...prepared.source,speech_start:prepared.offset};queuedPreparations.set(preparationKey(source),prepared);
      const existing=voiceQueue.findIndex(value=>value.id===source.id && value.revision===source.revision);
      if(existing>=0)voiceQueue[existing]=source;
      else {const nextReply=voiceQueue.findIndex(value=>value.kind!=="native_question");voiceQueue.splice(nextReply<0?voiceQueue.length:nextReply,0,source);}
    }

    const liveSpeech=window.LiveSpeech?.mount(standalone?{
      read:key=>window.localStorage?.getItem(key),write:(key,value)=>window.localStorage?.setItem(key,value)
    }:undefined);let canSpeakLive=false;
    let pendingNavigation,internalNavigation,internalControls;
    const requestControls = window.RequestControls.mount({api,getBinding:()=>binding,setCsrf,unauthorized:failed,onReply:(...args)=>{pendingNavigation?.close(false);return selectFocus(...args);},onListen:listenTo,getCanListen:()=>canListen,getCanDecide:()=>!readOnly&&!standalone,onSource:offerVoice});
    const taskControls=window.HumanTaskControls?.mount({api,getBinding:()=>binding,setCsrf,unauthorized:failed,canReply:()=>!readOnly,onReply:task=>{
      if(readOnly || posting || draft.value.trim())return false;
      pendingNavigation?.close(false);voiceContext=undefined;selectFocus({source:`task:${task.id}`,revision:task.revision,context_only:true},`${task.label} · ${task.title}`);return true;
    }});
    const goalControls=window.GoalControls?.mount({api,getBinding:()=>binding,readOnly,onChoose:message=>{
      if(readOnly || posting || draft.value.trim())return false;
      pendingNavigation?.close(false);clearFocus();voiceContext=undefined;draft.value=message;draft.focus?.();compose.scrollIntoView?.({block:"nearest"});return true;
    }});
    if(standalone && !readOnly)pendingNavigation=window.AttentionCarousel?.mount({
      line:$("header .header-tools"),panel:$("#conversation-attention"),localOnly:true,
      getItems:()=>[...(requestControls.attentionItems?.()||[]),...(taskControls?.attentionItems?.()||[]),...(goalControls?.attentionItems?.()||[])]
    });
    if(standalone && !readOnly){
      const inflight=new Map();
      const scopedApi=async(path,body)=>{
        const match=path.match(/^interaction\/(requests|tasks)(\?.*)?$/);
        if(match && body===undefined){
          const key=match[2]||"";
          let work=inflight.get(key);
          if(!work){work=api(`interaction/internal${key}`);inflight.set(key,work);work.finally(()=>{if(inflight.get(key)===work)inflight.delete(key);}).catch(()=>{});}
          const page=await work;
          return {...page,items:page.items.filter(v=>match[1]==="tasks"?v.kind==="task":v.kind!=="task")};
        }
        return api(path.replace(/^interaction\/(requests|tasks)/,"interaction/internal/$1"),body);
      };
      const requests=window.RequestControls.mount({api:scopedApi,getBinding:()=>binding,setCsrf,unauthorized:failed,prefix:"internal",allowCancel:false,getCanDecide:view=>view?.kind==="question"});
      const tasks=window.HumanTaskControls?.mount({api:scopedApi,getBinding:()=>binding,setCsrf,unauthorized:failed,prefix:"internal"});
      const panel=$("#internal-attention");panel.hidden=false;
      internalNavigation=window.AttentionCarousel?.mount({line:$("#conversation-presence"),panel,getItems:()=>[...requests.attentionItems(),...(tasks?.attentionItems()||[])]});
      internalControls={refresh:()=>Promise.all([requests.refresh(),tasks?.refresh()]),reset:()=>{internalNavigation?.close(false);requests.reset();tasks?.reset();}};
    }
    const attachedFiles = readOnly ? undefined : window.FileInput?.mount({headers,getBinding:()=>binding,unauthorized:failed});
    const visual = readOnly ? undefined : window.VisualInput?.mount({headers,getBinding:()=>binding,unauthorized:failed});
    const microphone = readOnly ? undefined : window.VoiceInput?.mount({api,headers,getBinding:()=>binding,onTiming:(id,stage)=>timing?.mark(id,stage),onInput:(value,version)=>{voiceMode?.update({sent:true});inputCard(value);if(timeline)timeline.scrollTop=timeline.scrollHeight;if(version===focusVersion)clearFocus();},getReply:()=>{const reference=replyFocus?.reference||voiceContext;return reference&&({reference,version:focusVersion});},getPlayback:()=>playbackContext(true),getCaptureQuestions:()=>canLimitCaptureQuestions?requestControls.captureQuestions():undefined,getFiles:()=>attachedFiles?.capture(),onFilesSent:refs=>attachedFiles?.consumed(refs),getVisual:()=>visual?.captureForVoice?visual.captureForVoice():visual?.capture(),onVisualSent:reference=>visual?.consumed(reference),pauseAudio:beginVoiceCapture,onActivity:(value,detail)=>{captureBusy=value;presentState({capture:detail?.phase,uploading:detail?.uploading,transcribing:detail?.transcribing,pending:detail?.pending,message:detail?.message,remaining:detail?.remaining});if(!value){if(voiceEnabled && active?.interrupted)toggle.disabled=!blobUrl;drainVoice();}},unauthorized:failed,telegram});
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
    let routeMapReturn;
    const mapBack=$("#routing-graph-return"),mapPanel=$("#conversation-flow"),mapInspector=$("#conversation-diagnostics");
    function closeRoutingMap(restore=true){
      const saved=routeMapReturn;if(!saved)return;routeMapReturn=undefined;mapBack.hidden=true;mapPanel.open=false;
      if(!restore)return;
      if(!saved.inspectorOpen && mapInspector.open)inspectorReturnFocus=saved.origin;
      mapInspector.open=saved.inspectorOpen;
      requestAnimationFrame(()=>{
        if(timeline)timeline.scrollTop=saved.timelineTop;
        const content=mapInspector.querySelector?.(".conversation-diagnostics-content");if(content)content.scrollTop=saved.inspectorTop;
        if(saved.origin?.isConnected!==false)saved.origin.focus?.({preventScroll:true});
      });
    }
    function openRoutingMap(source,origin){
      if(!graph || !source?.id)return;
      const content=mapInspector.querySelector?.(".conversation-diagnostics-content");
      const saved={origin,inspectorOpen:mapInspector.open,timelineTop:timeline?.scrollTop||0,inspectorTop:content?.scrollTop||0};routeMapReturn=saved;
      mapBack.hidden=false;mapInspector.open=true;mapPanel.open=true;
      graph.result(source);graph.flush();
      requestAnimationFrame(()=>{if(routeMapReturn!==saved)return;mapPanel.scrollIntoView?.({block:"start"});graph.focus(source.id);});
    }
    function routingMapButton(source){
      const button=node("button",undefined,"routing-map-open");button.type="button";button.hidden=!graph;
      button.setAttribute("aria-label","Show routing map");button.title="Show routing map";
      button.innerHTML='<svg class="control-icon" viewBox="0 0 24 24" aria-hidden="true" focusable="false"><circle cx="5" cy="5" r="2"/><circle cx="19" cy="12" r="2"/><circle cx="5" cy="19" r="2"/><path d="M7 5h3v14H7m3-7h7"/></svg>';
      button.addEventListener("click",()=>openRoutingMap(source(),button));return button;
    }
    mapBack?.addEventListener("click",()=>closeRoutingMap());
    mapPanel?.addEventListener("toggle",()=>{if(!mapPanel.open)closeRoutingMap();});
    mapInspector?.addEventListener("toggle",()=>{if(!mapInspector.open)closeRoutingMap();});

    const responseDraft=node("article",undefined,"reply response-draft");
    const responseDraftBody=node("div",undefined,"reply-text");
    responseDraft.id="conversation-response-draft";responseDraft.hidden=true;
    responseDraft.setAttribute("aria-label","Response being written");
    responseDraft.append(node("h3","Writing…"),responseDraftBody);
    function showResponseDraft(value){
      const text=value?.provisional===true && typeof value.text==="string" && value.text.length<=65536?value.text:"";
      const follow=atTimelineEnd();
      responseDraft.hidden=!text.trim();responseDraft.dataset.nativeKey=value?`${value.native_turn}:${value.native_item}`:"";
      if(responseDraftBody.dataset.text!==text){setReplyText(responseDraftBody,text);responseDraftBody.dataset.text=text;}
      if(!responseDraft.hidden){
        timing?.source({...value,kind:"integrator_live"});
        if(timeline)placeInTimeline(responseDraft,"response-draft",value.observed_at,follow);
        else $("#conversation-answers").append(responseDraft);
      }
    }
    const historyTools=timeline?node("div",undefined,"conversation-tools timeline-history"):undefined;
    if(timeline){
      const diagnostics=$("#conversation-diagnostics");
      diagnostics.open=false;
      diagnostics.querySelector?.(".conversation-diagnostics-content")?.prepend($("#conversation-observation"));
      timeline.hidden=false;$("#conversation-replies").hidden=false;$("#conversation-sent").hidden=true;
      $("#conversation-inputs-older").textContent="Earlier messages";
      historyTools.append($("#conversation-inputs-older"),$("#conversation-older"),$("#conversation-telegram-older"));
      diagnostics.querySelector?.(".conversation-diagnostics-content")?.append(historyTools);
    }
    function atTimelineEnd() {return !!timeline && !followingSpeech() && timeline.scrollHeight-timeline.scrollTop-timeline.clientHeight<80;}
    function arrangeTimelineReplies() {
      if(!timeline)return;
      const reference=box=>JSON.stringify([box.dataset.sourceId,box.dataset.sourceRevision]);
      const visible=new Map([...timeline.children].filter(box=>box.dataset.sourceId && !box.dataset.replyAnchor).map(box=>[reference(box),box]));
      const needed=new Set();
      for(const card of inputs.values()){
        const reply=card.value.reply;if(!reply)continue;
        const key=JSON.stringify([reply.source,reply.revision]);
        if(visible.has(key))continue;
        const original=cards.get(reply.source)?.source;
        if(original?.revision!==reply.revision || original.kind!=="worker_progress")continue;
        needed.add(key);
        if(replyAnchors.has(key))continue;
        // A coalesced update must not replace the exact update the person answered.
        // Keep a compact visible anchor; routing still carries only the original reference.
        const box=node("article",undefined,"reply session-update"),time=node("time",undefined,"message-time");
        box.append(node("h3",`Referenced update from ${original.label}`),node("p",original.preview,"reply-text"),time);
        messageTime(time,original.observed_at);box.dataset.replyAnchor="true";
        box.dataset.sourceId=original.id;box.dataset.sourceRevision=original.revision;
        box.dataset.timelineKey=`reply-source:${key}`;box.dataset.timelineTime=String(Date.parse(original.observed_at)||0);
        replyAnchors.set(key,box);timeline.append(box);
      }
      for(const [key,box] of replyAnchors)if(!needed.has(key)){box.remove();replyAnchors.delete(key);}
      const chronological=[...timeline.children].sort((a,b)=>Number(a.dataset.timelineTime)-Number(b.dataset.timelineTime)||String(a.dataset.timelineKey).localeCompare(String(b.dataset.timelineKey)));
      const anchors=new Map(chronological.filter(box=>box.dataset.sourceId).map(box=>[reference(box),box]));
      const groups=new Map(),attached=new Set();
      for(const box of chronological){
        if(!box.dataset.replySource)continue;
        const anchor=anchors.get(JSON.stringify([box.dataset.replySource,box.dataset.replyRevision]));
        if(!anchor || anchor===box)continue;
        if(!groups.has(anchor))groups.set(anchor,[]);groups.get(anchor).push(box);attached.add(box);
      }
      const ordered=chronological.filter(box=>!attached.has(box)).flatMap(box=>[box,...(groups.get(box)||[])]);
      // Move existing nodes only: expanded text, media, focus and drafts stay intact.
      ordered.forEach((box,index)=>{if(timeline.children[index]!==box)timeline.insertBefore(box,timeline.children[index]||null);});
    }
    function placeInTimeline(box, key, timestamp, nearEnd=atTimelineEnd()) {
      if(!timeline)return;
      box.dataset.timelineKey=key;
      const parsedTime=Date.parse(timestamp);
      box.dataset.timelineTime=String(Number.isFinite(parsedTime)?parsedTime:Number(box.dataset.timelineTime)||Date.now());
      const following=[...timeline.children].find(item=>item!==box &&
        (Number(item.dataset.timelineTime)>Number(box.dataset.timelineTime) ||
        (item.dataset.timelineTime===box.dataset.timelineTime && item.dataset.timelineKey>key)));
      timeline.insertBefore(box,following||null);
      arrangeTimelineReplies();
      if(nearEnd)timeline.scrollTop=timeline.scrollHeight;
    }
    let graphCursor=0;
    const cards = new Map(),progressCards=new Map(),settledProgress=new Map(),replyAnchors=new Map();
    // Device-local audio attention, scoped to this conversation binding. No
    // transcript, labels, native work state or playback backlog is persisted.
    const mutedSessions=new Set(),soloSessions=new Set();let muteBinding,canReplyUpdates=false;
    function sessionKey(source){
      if(["integrator_answer","integrator_progress","integrator_live"].includes(source.kind) || source.kind==="native_question" && source.audio_track==="conversation")return "conversation";
      const target=source.target;
      if(!["worker_progress","worker_result","native_question"].includes(source.kind) || !target
        || !Number.isSafeInteger(target.chat_id) || target.thread_id!=null && !Number.isSafeInteger(target.thread_id))return;
      return JSON.stringify([target.chat_id,target.thread_id??null]);
    }
    const sessionMuted=source=>{const key=sessionKey(source);if(conversationOnly)return key!=="conversation";return key?mutedSessions.has(key)||soloSessions.size>0&&!soloSessions.has(key):soloSessions.size>0;};
    function restoreSessionKeys(saved,target){
      if(!Array.isArray(saved)||saved.length>256)return;
      for(const key of saved){
        if(key==="conversation"){target.add(key);continue;}
        if(typeof key!=="string" || key.length>64)continue;
        try{const pair=JSON.parse(key);
          if(Array.isArray(pair)&&pair.length===2&&Number.isSafeInteger(pair[0])&&(pair[1]===null||Number.isSafeInteger(pair[1]))
            &&JSON.stringify(pair)===key)target.add(key);
        }catch{}
      }
    }
    function loadSessionMutes(){
      if(muteBinding===binding)return;muteBinding=binding;mutedSessions.clear();soloSessions.clear();
      try{
        const raw=window.localStorage?.getItem("interaction-session-audio:"+binding);
        if(raw){
          if(raw.length>32768)return;
          const saved=JSON.parse(raw);if(saved?.version!==1)return;
          restoreSessionKeys(saved.muted,mutedSessions);restoreSessionKeys(saved.solo,soloSessions);
        }else{
          const legacy=window.localStorage?.getItem("interaction-session-mutes:"+binding);
          if(legacy&&legacy.length<=32768)restoreSessionKeys(JSON.parse(legacy),mutedSessions);
        }
      }catch{}
    }
    function audioFilterChanged(){
      try{
        window.localStorage?.setItem("interaction-session-audio:"+binding,JSON.stringify({version:1,muted:[...mutedSessions],solo:[...soloSessions]}));
        window.localStorage?.removeItem?.("interaction-session-mutes:"+binding);
      }catch{}
      for(const other of [...cards.values(),...progressCards.values()])muteButtonState(other);
      if(prefetched && sessionMuted(prefetched.source||prefetched.owner)){prefetched.controller.abort();prefetched=undefined;}
      for(let n=voiceQueue.length-1;n>=0;n--)if(sessionMuted(voiceQueue[n]))removeQueued(n);
      if(active?.queued && sessionMuted(active.source))stop("Automatic audio filtered for this session. Its updates remain visible.");
      voiceStatus();Promise.resolve().then(drainVoice);
    }
    function muteButtonState(card){
      if(!card.mute)return;
      const key=sessionKey(card.muteSource),muted=mutedSessions.has(key),solo=soloSessions.has(key),label=key==="conversation"?"Conversation":card.muteSource.label||"this session";
      card.mute.setAttribute("aria-pressed",String(muted));
      card.mute.setAttribute("aria-label",`${muted?"Unmute":"Mute"} automatic audio updates from ${label}`);
      card.mute.title=card.mute.getAttribute?.("aria-label") || `${muted?"Unmute":"Mute"} automatic audio updates from ${label}`;
      card.mute.disabled=!muted&&mutedSessions.size>=256;
      if(card.mute.disabled)card.mute.title="Session mute limit reached. Unmute another session first.";
      card.mute.innerHTML='<svg viewBox="0 0 24 24" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M11 5 6 9H3v6h3l5 4Z"/>'+
        (muted?'<path d="m16 9 5 6m0-6-5 6"/>':'<path d="M15 8a6 6 0 0 1 0 8m3-11a10 10 0 0 1 0 14"/>')+'</svg>';
      card.solo.setAttribute("aria-pressed",String(solo));
      card.solo.setAttribute("aria-label",`${solo?"Remove solo from":"Solo"} automatic audio updates from ${label}`);
      card.solo.title=card.solo.getAttribute?.("aria-label");
      card.solo.disabled=!solo&&soloSessions.size>=256;
      card.reset.hidden=mutedSessions.size===0&&soloSessions.size===0;
    }
    function sessionTitle(card,source,label){
      card.muteSource=source;
      const key=sessionKey(source);
      if(readOnly || !key){card.title.textContent=label;card.titleLabel=undefined;card.muteAttached=false;return;}
      // Refresh labels without detaching focused keyboard controls.
      if(!card.titleLabel){card.title.textContent="";card.titleLabel=node("span");card.title.append(card.titleLabel);}
      card.titleLabel.textContent=label;
      if(!card.mute){
        card.audioControls=node("span",undefined,"session-audio-controls");
        card.mute=node("button",undefined,"session-audio-mute");card.mute.type="button";
        card.mute.addEventListener("click",()=>{
          const selected=sessionKey(card.muteSource);if(!selected)return;
          if(mutedSessions.has(selected))mutedSessions.delete(selected);
          else {if(mutedSessions.size>=256)return;mutedSessions.add(selected);}
          audioFilterChanged();
        });
        card.solo=node("button","S","session-audio-solo");card.solo.type="button";
        card.solo.addEventListener("click",()=>{
          const selected=sessionKey(card.muteSource);if(!selected)return;
          if(soloSessions.has(selected))soloSessions.delete(selected);
          else {if(soloSessions.size>=256)return;soloSessions.add(selected);}
          // Solo starts a fresh mix; previously muted tracks never reappear muted.
          mutedSessions.clear();audioFilterChanged();
        });
        card.reset=node("button",undefined,"session-audio-reset");card.reset.type="button";
        card.reset.setAttribute("aria-label","Hear all sessions: clear mute and solo");card.reset.title="Hear all sessions";
        card.reset.innerHTML='<svg viewBox="0 0 24 24" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="M3 10a9 9 0 1 1 1 7M3 4v6h6"/></svg>';
        card.reset.addEventListener("click",()=>{mutedSessions.clear();soloSessions.clear();audioFilterChanged();});
      }
      muteButtonState(card);
      if(!card.muteAttached){card.audioControls.replaceChildren(card.mute,card.solo,card.reset);card.title.append(card.audioControls);card.muteAttached=true;}
    }
    function clearProgress(){for(const card of progressCards.values())card.box.remove();progressCards.clear();settledProgress.clear();}
    const replyText=body=>window.ConversationMarkdown?.source(body) ?? body.textContent;
    const setReplyText=(body,text)=>window.ConversationMarkdown?window.ConversationMarkdown.render(body,text):(body.textContent=text);
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
        const box=node("article",undefined,"reply session-update"),title=node("h3"),body=node("p",undefined,"reply-text"),listen=node("button","Listen");
        const time=node("time",undefined,"message-time"),reply=node("button","Reply to this update");reply.type="button";reply.className="session-update-reply";
        listen.type="button";listen.className="session-update-listen";box.append(title,body,listen,reply,routingMapButton(()=>cards.get(card.source)?.source),time);card={box,title,body,listen,reply,time};progressCards.set(key,card);
        reply.addEventListener("click",()=>{const original=cards.get(card.source);if(!original || original.source.revision!==card.revision || !canReplyUpdates || readOnly)return;selectFocus({source:card.source,revision:card.revision},original.source.label||"this session");});
        listen.addEventListener("click",()=>{const original=cards.get(card.source);if(!original || original.source.revision!==card.revision || !canListen)return;return listenCard(original);});
      }
      const changed=card.source!==source.id || card.revision!==source.revision;
      card.source=source.id;card.revision=source.revision;card.stamp=stamp;sessionTitle(card,source,`Update from ${source.label}`);
      card.box.dataset.sourceId=source.id;card.box.dataset.sourceRevision=source.revision;
      const original=cards.get(source.id);
      setReplyText(card.body,original?.expanded && original.source.revision===source.revision?replyText(original.body):source.preview);
      card.listen.hidden=!canListen;card.listen.setAttribute("aria-label",`Listen to update from ${source.label}`);
      card.reply.hidden=readOnly||!canReplyUpdates;
      messageTime(card.time,source.observed_at);
      placeInTimeline(card.box,`session-update:${key}`,source.observed_at);
      if(changed)textObserver?.unobserve(card.box);
      textObserver?.observe(card.box);
    }

    // Read visible answers through the existing immutable source endpoint. Two
    // finite readers keep long history from blocking input or flooding the API.
    const textQueue=[];let textJobs=0;
    const textObserver=timeline && window.IntersectionObserver?new window.IntersectionObserver(entries=>{
      for(const entry of entries){const card=cards.get(entry.target.dataset.sourceId);if(card){card.textVisible=entry.isIntersecting;if(card.textVisible)queueText(card);}}
    },{root:timeline,rootMargin:"200px"}):undefined;
    function autoTextEligible(card){return card && card.textVisible && cards.get(card.source.id)===card
      && ["integrator_answer","integrator_progress","worker_progress","worker_result","matrix_result"].includes(card.source.kind)
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
    function appendTextPart(card,result){
      const following=atTimelineEnd();
      setReplyText(card.body,(card.expanded?replyText(card.body):"")+result.text);
      card.expanded=true;card.offset=result.next_offset;
      for(const visible of progressCards.values())if(visible.source===card.source.id && visible.revision===card.source.revision)setReplyText(visible.body,replyText(card.body));
      card.read.hidden=card.offset===null;card.read.textContent="Continue reading";
      if(following && timeline)timeline.scrollTop=timeline.scrollHeight;
    }
    async function readCard(card,automatic=false){
      if(card.textLoading || cards.get(card.source.id)!==card)return;
      const selected=card.source;card.textLoading=true;card.read.disabled=true;card.error.textContent="";
      if(automatic)card.autoRevision=selected.revision;
      try{
        let parts=0;
        do{
          const result=await part(selected,card.offset);
          if(cards.get(selected.id)!==card || card.source.revision!==selected.revision)return;
          appendTextPart(card,result);
          parts++;
        }while(automatic && card.offset!==null && parts<12 && replyText(card.body).length<16000 && enabled && !document.hidden && card.textVisible);
        if(automatic && card.offset!==null && parts<12 && replyText(card.body).length<16000 && (!enabled || document.hidden || !card.textVisible)){
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
    const phaseText = value => value.origin_kind==="telegram" ? `Telegram input retained (${value.phase}). Inspect its routing path for delivery evidence.${value.ordering_unknown?" Historical capture order unavailable.":""}${!value.received_at?" Historical capture time unavailable.":""}` : value.retired ? "Saved from an earlier conversation binding."
      : value.routing ? routeText(value.routing) : ({awaiting_target:"Choose the session for this saved message.",preparing:"Confirming transcript…", preparation_unconfirmed:"Recording saved; transcription unconfirmed. No work was submitted automatically.", retained:"Saved; waiting to send.", queued:"Saved; waiting to send.",
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
    function resetInputs() {closeRoutingMap(false);
      voiceMode?.close();showResponseDraft();
      companion?.reset();sound?.reset();goalControls?.reset();presentationProfile=undefined;replyAnchors.clear();
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
      timing?.input(value);
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
        const imageButton=node("button","Show selected image"),imageView=node("div");imageButton.type="button";imageButton.hidden=true;imageView.hidden=true;imageView.className="input-visual";imageView.setAttribute("role","img");
        imageButton.addEventListener("click",async()=>{
          const references=Array.isArray(card.value.visual)?card.value.visual:[card.value.visual];
          const showLabel=references.length>1?`Show images (${references.length})`:"Show selected image";
          if(!imageView.hidden){imageView.hidden=true;imageView.replaceChildren();imageButton.textContent=showLabel;return;}
          const owned=inputGeneration,controller=new AbortController(),deadline=setTimeout(()=>controller.abort(),20000);imageButton.disabled=true;imageButton.textContent="Loading images…";
          try{
            const views=[];
            for(const [index,ref] of references.entries()){
              const response=await fetch(`api/interaction/inputs/${encodeURIComponent(card.value.id)}/image?index=${index}`,{headers:headers(),credentials:"same-origin",signal:controller.signal});
              if(!response.ok)throw Object.assign(Error("Image unavailable"),{status:response.status});
              const bitmap=await createImageBitmap(await response.blob());
              if(owned!==inputGeneration){bitmap.close();return;}
              const view=node("canvas");view.className="input-visual";view.width=bitmap.width;view.height=bitmap.height;const context=view.getContext("2d");context.drawImage(bitmap,0,0);bitmap.close();
              if(ref.point){context.strokeStyle="#fff";context.lineWidth=Math.max(3,ref.width/200);context.beginPath();context.arc(ref.point.x,ref.point.y,Math.max(6,ref.width/80),0,2*Math.PI);context.stroke();}
              view.setAttribute("role","img");view.setAttribute("aria-label",`Captured image ${index+1} of ${references.length}.`+(ref.point?` Selected point X ${ref.point.x}, Y ${ref.point.y}.`:" Whole image; no point selected."));views.push(view);
            }
            imageView.replaceChildren(...views);imageView.hidden=false;imageButton.textContent=references.length>1?"Hide images":"Hide selected image";
          }catch(e){status.textContent="Selected image unavailable. The message was not resent.";if([401,403].includes(e.status))failed(e);}
          finally{clearTimeout(deadline);imageButton.disabled=false;if(imageView.hidden)imageButton.textContent=showLabel;}
        });
        const trace=node("details",undefined,"routing-trace"),traceSummary=node("summary","Show routing path"),traceState=node("p","","note"),traceContent=node("div"),traceRefresh=node("button","Refresh route");
        traceRefresh.type="button";traceState.setAttribute("role","status");trace.append(traceSummary,traceState,traceContent,traceRefresh);
        if(timeline){box.classList.add?.("human-message");box.append(node("h3","You"));}
        const time=node("time",undefined,"message-time");
        box.append(text,status,actions,routeBox,imageButton,imageView,trace,time);
        card = {box, text,time, imageButton,imageView,status, check, retry, value, observed,continueButton,routeBox,select,routeButton,routeNote,trace,traceState,traceContent,traceRefresh,traceText:new Map()};
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
            const result = recovery ? await api(`interaction/inputs/${card.value.id}/retry`,{binding:card.value.binding,instruction:recovery.instruction}) : await api("interaction/inputs", {id:card.value.id,binding:card.value.binding,text:card.value.text,...(card.value.reply?{reply:card.value.reply}:{}),...(card.value.visual?{visual:card.value.visual}:{}),...(card.value.files?.length?{files:card.value.files}:{}),...(card.value.playback?{playback:card.value.playback}:{})});
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
      taskControls?.observeInput?.(card.value);
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
      messageTime(card.time,card.value.received_at);
      if(card.fileList)card.fileList.remove();
      if(card.value.files?.length){card.fileList=node('div');card.fileList.className='input-files';card.value.files.forEach((file,index)=>{const button=node('button',file.name);button.type='button';button.disabled=!attachedFiles;button.addEventListener('click',async()=>{button.disabled=true;try{await attachedFiles.download(file,card.value.id,index);button.textContent=file.name;}catch(error){button.textContent=error.message||'Original file unavailable';}finally{button.disabled=!attachedFiles;}});card.fileList.append(button);});card.imageButton.parentNode.append(card.fileList);}
      card.imageButton.hidden=!card.value.visual;
      if(card.imageView.hidden)card.imageButton.textContent=Array.isArray(card.value.visual)?`Show images (${card.value.visual.length})`:"Show selected image";
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
      card.box.dataset.replySource=card.value.reply?.source||"";card.box.dataset.replyRevision=card.value.reply?.revision||"";
      if(timeline){
        const original=cards.get(card.value.reply?.source)?.source;
        card.box.children[0].textContent=card.value.reply?`You · reply to ${original?.kind?.startsWith("integrator_")?"Conversation":original?.label||"earlier message"}`:"You";
      }
      if(timeline)placeInTimeline(card.box,`input:${card.value.id}`,card.value.received_at || (card.value.origin_kind==="telegram"?"1970-01-01T00:00:00Z":undefined),followingLive);
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
      if (inputBinding && inputBinding !== result.binding) {resetInputs(); requestControls.reset(); internalControls?.reset(); taskControls?.reset(); microphone?.reset(); visual?.reset();attachedFiles?.reset();}
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
      if(telegramReading || !enabled || !ready || (!standalone && !$("#conversation-sent").open))return;
      const owned=inputGeneration;telegramReading=true;
      try {
        if(!telegramLoaded)await telegramPage();
        else {
          let more=true;
          for(let n=0;n<4 && more;n++){const page=await telegramPage(telegramNewest,"after",false);more=!!page?.more;if(n===3 && more)refreshAgain=true;}
          if(owned===inputGeneration)await telegramPage(0,"before",false);
        }
        if(owned===inputGeneration)$("#conversation-telegram-state").textContent="Telegram and app/device inputs share the retained capture order. Historical inputs without that order are labelled separately.";
      }catch(e){if(owned===inputGeneration)$("#conversation-telegram-state").textContent="Telegram observation unavailable. Saved app/device inputs remain visible; nothing was resent.";if(e.status===401)failed(e);}
      finally{telegramReading=false;}
    }
    $("#conversation-sent").addEventListener("toggle",()=>refreshTelegram());
    $("#conversation-telegram-older").addEventListener("click",async()=>{
      const button=$("#conversation-telegram-older");button.disabled=true;
      try{await telegramPage(telegramCursor);}catch(e){$("#conversation-telegram-state").textContent="Earlier Telegram inputs unavailable; nothing was resent.";if([401,403].includes(e.status))failed(e);}
      finally{button.disabled=false;}
    });
    draft.addEventListener("keydown", event => {
      if(event.key!=="Enter" || event.shiftKey || event.ctrlKey || event.altKey || event.metaKey
        || event.isComposing || event.keyCode===229)return;
      event.preventDefault();
      if(!event.repeat && !send.disabled && !readOnly){
        if(typeof compose.requestSubmit==="function")compose.requestSubmit();else send.click();
      }
    });
    compose.addEventListener("submit", async event => {
      event.preventDefault();
      if(microphone?.ownsSend?.())return microphone.submit();
      if (readOnly || posting || !ready || !binding || inputBinding !== binding) return;
      const text = draft.value;
      if (!text.trim() && !attachedFiles?.hasFiles() || text.includes("\0") || new TextEncoder().encode(text).length > 32768) {
        composerNotice("Write a message of at most 32 KiB."); return;
      }
      if ([...inputs.values()].some(card => !card.value.phase && card.value.text === text)) {
        composerNotice("This message has an unconfirmed submission. Use Check status before sending it again.");
        $("#conversation-sent").open = true; return;
      }
      let selectedVisual,selectedFiles;
      posting=true;send.disabled=true;
      try {if(visual)selectedVisual=await visual.capture();if(attachedFiles)selectedFiles=await attachedFiles.capture();}
      catch(e){posting=false;send.disabled=!inputBinding;microphone?.syncControls?.();composerNotice(e.message||"Select your image again.");return;}
      const id = crypto.randomUUID(), owned = inputGeneration, observation = ++inputObservation, focusAtSubmit=focusVersion;
      const context=replyFocus?.reference||voiceContext;
      const reply=context&&JSON.parse(JSON.stringify(context)),playback=playbackContext();
      posting = true; send.disabled = true; latestComposerId = id;
      inputCard({id,text,binding,reply,playback,visual:selectedVisual,files:selectedFiles,focusVersion:focusAtSubmit}, observation); if(timeline)timeline.scrollTop=timeline.scrollHeight; $("#conversation-sent").open = true;
      composerNotice("Sending…",false);
      try {
        const result = await api("interaction/inputs", {binding, id, text,...(reply?{reply}:{}),...(selectedVisual?{visual:selectedVisual}:{}),...(selectedFiles?.length?{files:selectedFiles}:{}),...(playback?{playback}:{})});
        if (owned !== inputGeneration) return;
        const confirmed = inputCard(result, observation);
        visual?.consumed(selectedVisual);attachedFiles?.consumed(selectedFiles);
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
    const playLabel = text => {
      toggle.textContent = text; toggle.title = text;
      toggle.setAttribute("aria-label", text);
      toggle.dataset.playbackState = text.startsWith("Pause") ? "pause" : "play";
    };
    playLabel("Play audio");
    const follow = $("#conversation-follow");
    const followPointers=new Set();let followTouch=false,followSettle;
    const followHeld=()=>followPointers.size>0 || followTouch || !!followSettle;
    const resumeFollow=()=>{if(!followHeld())followPlayback(true);};
    const saveFollow=()=>{try{window.localStorage?.setItem("interaction-follow-audio",String(follow.checked));}catch{}};
    if(standalone && follow){
      $("#conversation-follow-control").hidden=false;
      follow.checked=true;
      follow.title="Follow spoken progress; position within each audio segment is approximate";
      try{follow.checked=window.localStorage?.getItem("interaction-follow-audio")!=="false";}catch{}
      follow.addEventListener("change",()=>{saveFollow();if(follow.checked)followPlayback(true);});
      const settleFollow=()=>{clearTimeout(followSettle);followSettle=setTimeout(()=>{followSettle=undefined;resumeFollow();},180);};
      timeline.addEventListener("wheel",settleFollow,{passive:true});
      timeline.addEventListener("pointerdown",event=>{followPointers.add(event.pointerId);});
      const releasePointer=event=>{followPointers.delete(event.pointerId);resumeFollow();};
      window.addEventListener("pointerup",releasePointer);
      window.addEventListener("pointercancel",releasePointer);
      timeline.addEventListener("touchstart",()=>{followTouch=true;},{passive:true});
      const releaseTouch=event=>{followTouch=!!event.touches?.length;resumeFollow();};
      window.addEventListener("touchend",releaseTouch,{passive:true});
      window.addEventListener("touchcancel",releaseTouch,{passive:true});
      timeline.addEventListener("keydown",event=>{
        if(["ArrowUp","ArrowDown","PageUp","PageDown","Home","End"," "].includes(event.key)
          && !["INPUT","TEXTAREA","BUTTON","SELECT"].includes(event.target?.tagName))settleFollow();
      });
    }
    function followingSpeech(){
      return standalone && follow?.checked && !followHeld() && voiceEnabled && !voiceSuspended && !document.hidden
        && active?.auto && !active.verifying && !audio.paused && !audio.muted;
    }
    function followCard(){
      if(active?.source.kind==="integrator_live"){
        const source=active.source,key=`${source.native_turn}:${source.native_item}`;
        if(!responseDraft.hidden && responseDraft.dataset.nativeKey===key)return {box:responseDraft,body:responseDraftBody,expanded:true};
        return [...cards.values()].find(card=>card.source.native_turn===source.native_turn && card.source.native_item===source.native_item);
      }
      const original=cards.get(active?.source.id);
      if(original?.source.revision!==active?.source.revision)return;
      return timeline?.contains?.(original?.box)?original:[...progressCards.values()]
        .find(value=>value.source===active.source.id && value.revision===active.source.revision);
    }
    async function loadFollowText(selected){
      if(selected.source.kind==="integrator_live"){
        const card=followCard();if(!card?.source || card.textLoading || selected.followTextRequested)return;
        selected.followTextRequested=true;await readCard(card);if(active===selected)followPlayback();return;
      }
      const card=cards.get(selected.source.id);
      if(!card || card.textLoading || selected.followTextRequested)return;
      selected.followTextRequested=true;
      if(card.source.revision===selected.source.revision && card.offset===selected.offset)appendTextPart(card,selected.verified);
      // Reuse the canonical reader, keeping one bounded request chain per segment.
      for(let n=0;n<12 && active===selected && followingSpeech()
        && card.source.revision===selected.source.revision
        && (!card.expanded || Array.from(replyText(card.body)).length<selected.textEnd);n++){
        const before=replyText(card.body);
        await readCard(card);
        if(replyText(card.body)===before || card.offset===null)break;
      }
      if(active===selected && !followingSpeech())selected.followTextRequested=false;
      if(active===selected)followPlayback();
    }
    function followPlayback(force=false){
      if(!followingSpeech() || $("#conversation-diagnostics").open)return;
      const card=followCard();
      if(!card || !timeline?.contains?.(card.box))return;
      const selected=active, viewport=timeline.getBoundingClientRect();
      const scroll=top=>timeline.scrollTo({top:Math.max(0,top),behavior:window.matchMedia?.("(prefers-reduced-motion: reduce)")?.matches?"instant":"smooth"});
      if(!selected.followed && selected.offset===0){
        scroll(timeline.scrollTop+card.box.getBoundingClientRect().top-viewport.top);
        selected.followed=true;
        return;
      }
      if(!Number.isSafeInteger(selected.textEnd))return;
      const original=cards.get(selected.source.id)||card;
      if(!original.expanded || Array.from(replyText(original.body)).length<selected.textEnd){
        void loadFollowText(selected);return;
      }
      if(!card.body.firstChild || !document.createRange)return;
      // Media progress guides a reading window, not a claimed word alignment.
      const timed=Number.isFinite(audio.duration) && audio.duration>0 && Number.isFinite(audio.currentTime);
      if(!timed && selected.followed && !force)return;
      const fraction=timed?Math.max(0,Math.min(1,audio.currentTime/audio.duration)):0;
      const characters=Array.from(replyText(card.body));
      const wordOffset=selected.streaming?.position?.(audio.currentTime);
      const aligned=Number.isSafeInteger(wordOffset) && wordOffset>=selected.offset && wordOffset<=selected.textEnd;
      const position=Math.min(characters.length-1,aligned?wordOffset:selected.offset+Math.floor(Math.max(0,selected.textEnd-selected.offset-1)*fraction));
      if(position<0)return;
      const start=characters.slice(0,position).join("").length;
      const range=window.ConversationMarkdown?window.ConversationMarkdown.range(card.body,position):(()=>{
        const result=document.createRange();result.setStart(card.body.firstChild,start);
        result.setEnd(card.body.firstChild,start+characters[position].length);return result;
      })();
      if(!range)return;
      const point=range.getBoundingClientRect();
      if(!point.height || !viewport.height)return;
      if(force || point.top<viewport.top || point.bottom>viewport.bottom-24)
        scroll(timeline.scrollTop+point.top-viewport.top-viewport.height*.25);
      selected.followed=true;
    }
    let enabled = false, ready = false, reading = false, binding, canListen = false, canStreamSpeech = false, canReportPlayback=false,canReportWordPosition=false,canLimitCaptureQuestions=false;
    let newest = {delivery:0, native:0, matrix:0}, oldest = {delivery:0, native:0, matrix:0};
    let initialized = false, active, generation = 0, requestAbort, blobUrl, prefetched;
    const audioState = (text) => { $("#conversation-audio-state").textContent = text; };
    const initialMode = () => readOnly || new URL(location.href).searchParams.get("view") === "conversation"
      || new URLSearchParams(telegram?.initData || "").get("start_param") === "conversation";

    const playbackAllowed = () => !document.hidden || (standalone && voiceEnabled && !voiceSuspended);
    function voiceKey(source) {return source.kind==="native_question"?`${source.id}:${source.revision}`:source.id;}
    function voiceStatus() {
      const button=$("#conversation-voice-replies");
      const state=voiceEnabled?(voiceSuspended?"paused":"on"):"off";
      button.dataset.voiceState=state;
      button.setAttribute("aria-pressed",String(state==="on"));
      button.setAttribute("aria-label",state==="on"?"Disable spoken replies":state==="paused"?"Resume spoken replies":"Enable spoken replies");
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
    // Client media progress is advisory. Persist only source/revision and the
    // last aligned word or segment boundary, never transcript text or a hearing claim.
    const speechPositions=new Map();let positionBinding;
    const positionKey=()=>"interaction-speech-position:"+binding;
    function positionLabel(card){
      const skipping=active && !active.segmentEnded && !voiceSuspended && (active.auto && !audio.paused || requestAbort) && sameMessage(active.source,card.source);
      const saved=speechPositions.get(card.source.id);
      card.listen.textContent=skipping?"Skip":saved?.revision===card.source.revision?"Resume audio":"Listen";
      card.listen.title=skipping?"Skip the rest of this reply and continue the queue":saved?.revision===card.source.revision?"Resume from the saved word or segment boundary; part of a word may repeat":"Listen to this reply";
    }
    function playbackLabels(){
      for(const card of cards.values())positionLabel(card);
      for(const card of progressCards.values()){
        const original=cards.get(card.source);if(!original || original.source.revision!==card.revision)continue;
        card.listen.textContent=original.listen.textContent;
        card.listen.title=original.listen.title;
        card.listen.setAttribute("aria-label",`${original.listen.textContent} update`);
      }
    }
    async function skipReply(){
      const selected=active;if(!selected || captureBusy)return;
      // A skip is an explicit selection, never a claim that unsaid text was heard.
      skippedMessages.set(messageKey(selected.source),true);
      while(skippedMessages.size>128)skippedMessages.delete(skippedMessages.keys().next().value);
      liveSpeech?.interrupt(selected.source);
      for(let n=voiceQueue.length-1;n>=0;n--)if(sameMessage(voiceQueue[n],selected.source))removeQueued(n);
      if(prefetched?.ahead && prefetched.selection)await prefetched.selection;
      if(active!==selected)return;
      if(prefetched?.source && !sameMessage(prefetched.source,selected.source))parkPrepared();
      stop("Reply skipped. Continuing the queue.");
      if(voiceEnabled && !voiceSuspended)drainVoice();
    }
    function savePositions(){
      if(!standalone || !binding)return false;
      try{const storage=window.localStorage;if(!storage)return false;storage.setItem(positionKey(),JSON.stringify([...speechPositions.values()]));return true;}catch{return false;}
    }
    function loadPositions(){
      if(positionBinding===binding)return;positionBinding=binding;speechPositions.clear();skippedMessages.clear();
      if(!standalone)return;
      try{
        const raw=window.localStorage?.getItem(positionKey());if(!raw || raw.length>16384)return;
        const saved=JSON.parse(raw);if(!Array.isArray(saved)||saved.length>24)return;
        for(const value of saved)if(value && typeof value.source==='string' && value.source.length<=256
          && typeof value.revision==='string' && value.revision.length<=256
          && Number.isSafeInteger(value.offset) && value.offset>=0 && value.offset<=1000000)
          speechPositions.set(value.source,{source:value.source,revision:value.revision,offset:value.offset});
      }catch{}
    }
    function forgetPosition(source){
      if(speechPositions.delete(source.id)){if(savePositions())liveSpeech?.canonicalSaved(source);const card=cards.get(source.id);if(card)positionLabel(card);}
    }
    function rememberPosition(){
      if(!active || active.verifying || !blobUrl)return;
      const source=active.source;
      if(source.current===false || source.correction_pending || source.native_binding_current===false)return;
      const word=alignedPosition();
      if(source.kind==="integrator_live"){
        if(!active.segmentEnded)rememberLiveCursor(liveSpeech?.checkpoint(source,active.offset,word??active.offset));
        return;
      }
      const normal=active.segmentEnded?(Number.isSafeInteger(active.next)?active.next:active.textEnd):(word??active.offset);
      const contiguous=liveSpeech?.checkpoint(source,active.offset,normal)?.resume ?? liveSpeech?.resume(source);
      // A provider word cursor can refine this segment, but cannot hide an
      // earlier interrupted/failed live prefix. No duration interpolation.
      const offset=contiguous===undefined || contiguous>=active.offset?normal:Math.min(normal,contiguous);
      if(active.next===null && offset===active.textEnd && (active.segmentEnded || word===active.textEnd)){
        forgetPosition(source);return;
      }
      speechPositions.delete(source.id);speechPositions.set(source.id,{source:source.id,revision:source.revision,offset});
      while(speechPositions.size>24)speechPositions.delete(speechPositions.keys().next().value);
      if(savePositions())liveSpeech?.canonicalSaved(source);
      const card=cards.get(source.id);if(card)positionLabel(card);
    }
    function rememberLiveCursor(cursor){
      if(!cursor)return;
      if(cursor.resume>=cursor.source.text_characters){forgetPosition(cursor.source);return;}
      speechPositions.delete(cursor.source.id);speechPositions.set(cursor.source.id,{source:cursor.source.id,revision:cursor.source.revision,offset:cursor.resume});
      while(speechPositions.size>24)speechPositions.delete(speechPositions.keys().next().value);
      if(savePositions())liveSpeech?.canonicalSaved(cursor.source);
      const card=cards.get(cursor.source.id);if(card)positionLabel(card);
    }
    function alignedPosition(){
      if(!canReportWordPosition || !active || !Number.isFinite(audio.currentTime) || audio.currentTime<0 || audio.currentTime>3600)return;
      const offset=active.streaming?.position?.(audio.currentTime);
      if(Number.isSafeInteger(offset) && offset>=active.offset && offset<=active.textEnd && offset<=1000000)return offset;
    }
    function playbackContext(interrupted=false){
      if(!canReportPlayback || !active || !blobUrl || active.verifying || active.segmentEnded
        || active.source.current===false || active.source.correction_pending || active.source.native_binding_current===false)return;
      const millis=value=>Number.isFinite(value) && value>=0 && value<=3600?Math.floor(value*1000):undefined;
      const position=millis(audio.currentTime),duration=millis(audio.duration);
      if(position===undefined)return;
      const wordOffset=alignedPosition();
      const word=wordOffset===undefined?{}:{word_offset:wordOffset};
      return {source:active.source.id,revision:active.source.revision,segment_offset:active.offset,...word,
        position_millis:position,duration_millis:duration>0?duration:null,state:interrupted?'interrupted':audio.paused?'paused':'playing'};
    }
    function beginVoiceCapture() {
      const pauseStarted=active && !audio.paused && !audio.muted?window.performance?.now?.():undefined;
      liveSpeech?.interrupt(active?.source);
      for(const prepared of queuedPreparations.values())liveSpeech?.interrupt(prepared.source);
      for(let n=voiceQueue.length-1;n>=0;n--)if(queuedPreparations.has(preparationKey(voiceQueue[n])))removeQueued(n);
      draft.blur?.();enableVoice();voiceBusy=false;cancelSpeechWait();
      if(active && blobUrl && !active.verifying){
        rememberPosition();active.generation=++generation;active.auto=false;active.interrupted=true;
        audio.pause();audio.muted=true;toggle.disabled=true;playLabel("Resume audio");
        presentState({playback:undefined});
        audioState("Listening to your input. The interrupted reply can be resumed.");
      }else stop("Listening to your input. Replies remain in the conversation.");
      timing?.pauseApplied(pauseStarted);
    }
    const speaker=source=>source.kind.startsWith("integrator_")?"integrator":JSON.stringify(source.target);
    function rememberFinal(source) {
      if(!["worker_result","integrator_answer"].includes(source.kind) || source.current===false || source.correction_pending)return;
      const key=speaker(source),time=Date.parse(source.observed_at)||0;
      voiceFinalTimes.set(key,Math.max(voiceFinalTimes.get(key)||0,time));
    }
    function offerVoice(source) {
      if(!voiceEnabled || voiceSeen.has(voiceKey(source)) || skippedMessages.has(messageKey(source)))return;
      voiceSeen.add(voiceKey(source));
      if(voiceSuspended)return;
      if(active?.source.id===source.id && active.source.revision!==source.revision)stop("This reply changed. Refresh before listening.");
      if(source.current===false || source.correction_pending || source.native_binding_current===false)return;
      const observed=Date.parse(source.observed_at)||0;
      if(source.kind.endsWith("_progress") && observed && observed<=(voiceFinalTimes.get(speaker(source))||0))return;
      rememberFinal(source);
      if(sessionMuted(source))return;
      const sameSpeaker=other=>source.kind.startsWith("integrator_")?other.kind==="integrator_progress":
        other.kind==="worker_progress" && JSON.stringify(other.target)===JSON.stringify(source.target);
      if(source.kind.endsWith("_progress") || ["worker_result","integrator_answer"].includes(source.kind)){
        for(let n=voiceQueue.length-1;n>=0;n--)if(sameSpeaker(voiceQueue[n]))removeQueued(n);
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
      if(!voiceEnabled || voiceSuspended || voiceBusy || speechWait || captureBusy || questionRefreshPending || !enabled || !canListen || !playbackAllowed())return;
      while(voiceQueue.length&&(sessionMuted(voiceQueue[0]) || skippedMessages.has(messageKey(voiceQueue[0]))))removeQueued(0);
      voiceBusy=true;const selectionGeneration=generation;
      if(prefetched?.ahead && !active?.interrupted)await prefetched.selection;
      if(generation!==selectionGeneration)return;
      if(questionRefreshPending){voiceBusy=false;return;}
      if(voiceQueue[0]?.kind==="native_question")parkPrepared();
      let next,prepared;
      if(voiceQueue[0]?.kind!=="native_question" && prefetched?.ahead && prefetched.source && !active?.interrupted){
        prepared=prefetched;next=prepared.source;
        const duplicate=voiceQueue.findIndex(source=>source.id===next.id && source.revision===next.revision);
        if(duplicate>=0)voiceQueue.splice(duplicate,1);
      }else next=voiceQueue.shift() || (canSpeakLive && !sessionMuted({kind:"integrator_live"})?await liveSpeech?.next():undefined);
      if(generation!==selectionGeneration){liveSpeech?.unsent(next);return;}
      if(questionRefreshPending || (next?.kind!=="native_question" && voiceQueue[0]?.kind==="native_question")){
        if(next && !prepared){
          if(next.kind==="integrator_live")liveSpeech?.unsent(next);
          else {const afterQuestions=voiceQueue.findIndex(source=>source.kind!=="native_question");voiceQueue.splice(afterQuestions<0?voiceQueue.length:afterQuestions,0,next);}
        }
        voiceBusy=false;if(!questionRefreshPending)drainVoice();return;
      }
      voiceStatus();if(!next){voiceBusy=false;return;}
      const offset=next.speech_start||0;
      prepared ||= queuedPreparations.get(preparationKey(next,offset));
      if(!voiceEnabled || voiceSuspended || captureBusy || !enabled || !playbackAllowed() || sessionMuted(next)){liveSpeech?.unsent(next);voiceBusy=false;return;}
      if(next.kind==="integrator_live" && !prepared && !liveSpeech?.admit(next)){voiceBusy=false;return;}
      const result=await listenTo(next,offset,{queued:true});
      if(result?.failed && result.generation===generation && !active)
        nextSpokenReply("reply",next);
    }
    function continueSpeech(){
      const remaining=liveSpeech?.continueSource(active?.source);
      if(!remaining || voiceQueue.some(source=>source.id===remaining.id && source.revision===remaining.revision))return;
      const nextReply=voiceQueue.findIndex(source=>source.kind!=="native_question");
      voiceQueue.splice(nextReply<0?voiceQueue.length:nextReply,0,remaining);
    }
    function disableVoice() {
      voiceEnabled=false;voiceBusy=false;clearVoiceQueue();voiceFinalTimes.clear();voiceContext=undefined;voiceStatus();stop();
    }
    $("#conversation-voice-replies").addEventListener("click",()=>{
      if(voiceEnabled && !voiceSuspended)disableVoice();else {
        enableVoice();
        if(captureBusy){voiceBusy=false;return;}
        if(active && blobUrl && !audio.ended){
          continueSpeech();voiceBusy=true;void startPlayback();
        }else{
          // A cancelled preparation or ended segment cannot own the queue
          // forever. Release it without retrying its synthesis automatically.
          if(active)stop();
          voiceBusy=false;drainVoice();
        }
      }
    });
    function stop(text = "Audio stopped.") {
      rememberPosition();
      presentState({playback:undefined});
      cancelSpeechWait();voiceBusy=false;generation++;
      requestAbort?.abort(); requestAbort = undefined;
      prefetched?.controller.abort();prefetched=undefined;
      active?.streaming?.cancel();
      active = undefined;playbackLabels();
      audio.pause(); audio.muted = true;
      audio.removeAttribute("src"); audio.load();
      if (blobUrl) URL.revokeObjectURL(blobUrl);
      blobUrl = undefined;
      toggle.disabled = true;
      if(mediaSession)try{mediaSession.playbackState="none";}catch{}
      audioState(text);
      $("#conversation-player").hidden = true;
    }
    function pause(reason="manual") {
      if(voiceEnabled && !voiceSuspended){
        const button=$("#conversation-voice-replies");
        button.dataset.lastPauseReason=reason;button.dataset.lastPauseAt=new Date().toISOString();
      }
      rememberPosition();
      presentState({playback:undefined});
      cancelSpeechWait();voiceBusy=false;clearVoiceQueue();
      voiceSuspended=true;voiceStatus();
      if(mediaSession)try{mediaSession.playbackState="paused";}catch{}
      generation++;
      if (!active) return;
      if(!active.streaming)requestAbort?.abort();
      requestAbort = undefined;
      active.generation = generation;
      active.auto = false;
      audio.pause(); audio.muted = true;
      playLabel("Play audio");
      toggle.disabled = !blobUrl;
      audioState("Paused. Your replies remain available.");playbackLabels();
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
      if (!enabled) {pause("view"); sound?.reset(); microphone?.deactivate();}
      modeChanged(enabled, initial);
    }
    // api() aborts a request at its deadline. That is a connection interruption,
    // not an explicit decision to pause spoken replies.
    const transientConnectionError=error=>["TypeError","AbortError"].includes(error.name) || error.status>=500;
    function failed(error) {
      showResponseDraft();
      presentState({connected:false});
      if (error.status === 401 || error.status === 403) {
        onUnauthorized?.(error);
        clearFiles();
        pause("access");stop(); cards.clear();clearProgress();clearText(); initialized = false; resetInputs(); requestControls.reset(); internalControls?.reset(); taskControls?.reset(); microphone?.reset(); visual?.reset();attachedFiles?.reset();
        $("#conversation-answers").replaceChildren();
        $("#conversation-results").replaceChildren();
      } else if(!(standalone && voiceEnabled && !voiceSuspended && transientConnectionError(error)))pause("observation");
      $("#conversation-state").textContent = error.name === "TypeError" ? "Connection unavailable. Refresh to reconnect."
        : error.message || "Replies are unavailable. Refresh to continue.";
    }
    function part(source, offset) {
      const query = new URLSearchParams({source:source.id, revision:source.revision, offset:String(offset)});
      return api(`interaction/part?${query}`);
    }
    function listenCard(card) {
      if(readOnly || captureBusy || card.listen.disabled)return;
      if(active && !active.segmentEnded && !voiceSuspended && (active.auto && !audio.paused || requestAbort) && sameMessage(active.source,card.source))return skipReply();
      const saved=speechPositions.get(card.source.id);
      if(active?.source.id===card.source.id && active.source.revision===card.source.revision && blobUrl && !audio.ended && (!saved || saved.offset>=active.offset)){
        voiceSuspended=false;voiceStatus();return startPlayback();
      }
      return listenTo(card.source,saved?.revision===card.source.revision?saved.offset:0);
    }
    function upsert(source) {
      const followingLive=atTimelineEnd();
      let card = cards.get(source.id);
      if (!card) {
        const container = $(source.kind === "integrator_answer" ? "#conversation-answers" : "#conversation-results");
        const box = node("article", undefined, "reply");
        const title = node("h3");
        const status = node("p", "", "reply-status");
        const body = node("div", undefined, "reply-text");setReplyText(body,source.preview);
        const actions = node("div", undefined, "reply-actions");
        const read = node("button", "Read reply"); read.type = "button";
        const listen = node("button", "Listen"); listen.type = "button";
        const error = node("p", "", "reply-error"); error.setAttribute("role", "status");
        const reply=node("button",source.kind.startsWith("integrator_")?"Reply to this message":"Reply to this session");reply.type="button";reply.hidden=readOnly || !["worker_result","matrix_result","integrator_answer","integrator_progress"].includes(source.kind);
        const time=node("time",undefined,"message-time");
        actions.append(read, listen,reply,routingMapButton(()=>card.source)); box.append(title, status, body, actions, error,time);
        box.dataset.sequence = String(source.sequence);
        const following = [...container.children].find((item) => Number(item.dataset.sequence) < source.sequence);
        if(!timeline || source.kind==="worker_progress")container.insertBefore(box, following || null);
        card = {source, box, title, status, body, read, listen, error,time, offset:0, expanded:false};
        cards.set(source.id, card);
        reply.addEventListener("click",()=>selectFocus({source:card.source.id,revision:card.source.revision,...(card.source.kind==="matrix_result" || card.source.kind.startsWith("integrator_")?{context_only:true}:{})},card.source.label||"this session"));
        read.addEventListener("click",()=>readCard(card));
        listen.hidden=readOnly;listen.addEventListener("click",()=>listenCard(card));
      } else if (card.source.text_sha256 !== source.text_sha256) {
        setReplyText(card.body,source.preview);
        card.autoRevision=undefined;card.autoWanted=false;
        card.offset = 0; card.expanded = false; card.read.hidden = false;
        card.read.textContent = "Read reply";
      }
      for(let n=voiceQueue.length-1;n>=0;n--)
        if(voiceQueue[n].id===source.id && voiceQueue[n].revision!==source.revision)removeQueued(n);
      if (active?.source.id === source.id && active.source.revision !== source.revision)
        stop("This reply changed. Refresh before listening.");
      const textChanged=card.source.revision!==source.revision;
      card.source = source;card.box.dataset.sourceId=source.id;card.box.dataset.sourceRevision=source.revision;
      if(!card.expanded && Number.isSafeInteger(source.text_characters) && source.text_characters===Array.from(source.preview).length)card.read.hidden=true;
      for(const file of source.artifacts||[])files.set(file.id,file);
      renderFiles(card);
      messageTime(card.time,source.observed_at);card.box.append(card.time);
      const integrator=source.kind.startsWith("integrator_");
      const progress=source.kind.endsWith("_progress");
      sessionTitle(card,source,integrator ? "Conversation" : source.label);
      card.status.textContent = progress ? "Progress update." : source.kind === "integrator_answer" ? "Reply from this conversation."
        : source.correction_pending ? "A later correction is pending."
        : !source.native_binding_current ? "From an earlier session."
        : !source.current ? "Instruction coverage has not been confirmed."
        : source.kind === "matrix_result" ? "Reply from the selected daimon. Work completion is not independently verified."
        : source.completed ? "Session reports this request complete." : "Session progress; work may continue.";
      card.box.dataset.pending = String(!!source.correction_pending || source.native_binding_current === false);
      card.listen.hidden = !canListen;
      if(source.current===false || source.correction_pending || source.native_binding_current===false
        || speechPositions.get(source.id)?.revision!==source.revision)forgetPosition(source);
      positionLabel(card);
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
        clearVoiceQueue();stop(); cards.clear();clearProgress();clearText(); initialized = false; resetInputs(); requestControls.reset(); internalControls?.reset(); taskControls?.reset(); microphone?.reset(); visual?.reset();attachedFiles?.reset();
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
      binding = value.binding;timing?.bind(binding);loadPositions();loadSessionMutes();canReportPlayback=value.can_report_playback===true;canReportWordPosition=value.can_report_word_position===true;canLimitCaptureQuestions=value.can_limit_capture_questions===true;canReplyUpdates=value.can_reply_updates===true;
      canFiles=!readOnly && value.can_download_files===true;
      if(!canFiles)clearFiles();
      else if(value.artifacts?.state==="unavailable")filesConnected=false;
      else {filesConnected=true;fileJobs=value.artifacts?.jobs||[];files.clear();for(const card of cards.values())for(const file of card.source.artifacts||[])files.set(file.id,file);for(const file of value.artifacts?.items||[])files.set(file.id,file);}
      refreshFiles();
      await goalControls?.observe(value.goal_controls);
      microphone?.configure(value.can_speak,value.can_stream_transcribe,value.streaming_transcription_mode);
      voiceMode?.configure(value.can_converse===true);
      setCsrf(value.csrf);
      canListen = !readOnly && value.can_listen;canStreamSpeech=canListen && !!value.can_stream_speech;
      canSpeakLive=canListen && value.can_speak_live_response===true;
      if(!canSpeakLive)liveSpeech?.reset();
      else liveSpeech?.observe(value.response_draft,binding);
      if(!canListen && (voiceEnabled || active || speechWait))pause("capability");
      $("#conversation-voice-replies").disabled=!canListen;
      for (const source of value.items){
        upsert(source);timing?.source(source);
        const card=cards.get(source.id);
        if(card)card.listen.disabled=canSpeakLive && liveSpeech?.needsReconcile(source)===true;
      }
      if(direction==="after")for(const source of value.items)companion?.reply(source);
      for(const source of [...value.items].sort((a,b)=>(Date.parse(a.observed_at)||0)-(Date.parse(b.observed_at)||0))){
        if(active?.source.kind==="integrator_live" && active.source.native_turn===source.native_turn && active.source.native_item===source.native_item)rememberPosition();
        const reconciled=canSpeakLive && (direction==="after" || liveSpeech?.needsReconcile(source))?await liveSpeech?.reconcile(source,part):undefined;
        if(reconciled?.changed)cancelPreparedMessage(source);
        if(reconciled?.changed && active?.source.kind==="integrator_live" && active.source.native_turn===source.native_turn && active.source.native_item===source.native_item)
          stop("The confirmed response changed. Its updated text remains available.");
        if(reconciled?.resume!==undefined){
          speechPositions.set(source.id,{source:source.id,revision:source.revision,offset:reconciled.resume});
          if(savePositions())liveSpeech?.canonicalSaved(source);
          const card=cards.get(source.id);if(card)positionLabel(card);
        }
        const card=cards.get(source.id);
        if(card?.source.revision===source.revision)card.listen.disabled=false;
        // Initial/history pages can contain a response confirmed while the
        // view was closed. Transfer its retained cursor without speaking history.
        if(reconciled?.skip)voiceSeen.add(voiceKey(source));
        else if(direction==="after")offerVoice(reconciled?.source||source);
      }
      playbackLabels();
      showResponseDraft(value.response_draft);
      if(active?.auto && !audio.paused && !active.verifying)prefetchNext(active);
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
    let refreshAgain = false, questionRefreshPending = false;
    async function refresh(force = false) {
      if(reading){refreshAgain ||= force;return;}
      if (!enabled || !ready || (!playbackAllowed() && !force)) return;
      drainText();
      reading = true;questionRefreshPending=!readOnly;
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
            if(n===3 && more)refreshAgain=true;
          }
          if (enabled && playbackAllowed()) await page({delivery:0,native:0,matrix:0}, "before", false);
        }
        // Resolve current questions before starting newly observed speech. Input
        // history, Telegram and inspector reads do not gate useful playback.
        const questionsCurrent=readOnly || await requestControls.refresh();
        if(!readOnly && questionsCurrent){
          const questions=requestControls.audioSources?.()||[];
          presentState({question:questions.length>0});
          const current=source=>questions.some(q=>q.id===source.id && q.revision===source.revision);
          for(let n=voiceQueue.length-1;n>=0;n--)
            if(voiceQueue[n].kind==="native_question" && !current(voiceQueue[n]))removeQueued(n);
          if(active?.source.kind==="native_question" && !current(active.source))
            stop("This question is no longer current. Refresh before answering.");
        }
        questionRefreshPending=!questionsCurrent;drainVoice();
        if (active && enabled && playbackAllowed()) {
          const selected = active, owned = generation;
          try { await part(selected.source, selected.offset); }
          catch (error) {
            if (active === selected && generation === owned){
              if(error.status===401 || error.status===403)failed(error);
              else if(!transientConnectionError(error))stop("This reply is no longer current. Refresh before listening.");
            }
          }
        }
        $("#conversation-state").textContent = cards.size ? `${cards.size} ${cards.size===1?"reply":"replies"} available.`
          : "No replies yet. Work can continue while this view is closed.";
        if (!inputBinding) await inputPage();
        else {
          let more = true;
          for (let n=0; n<4 && more; n++) {
            const result = await inputPage(inputNewest,"after"); more = !!result?.more;
            if(n===3 && more)refreshAgain=true;
          }
          await inputPage(0,"before",false);
        }
        if(!readOnly){await taskControls?.refresh();await internalControls?.refresh();}
        await refreshTelegram();
        void sound?.refresh({observeOnly:standalone || $("#conversation-diagnostics").open});
        for(const card of inputs.values())if(card.trace.open)await readTrace(card);
        if(graph){
          const closed=[...inputs.values()].filter(card=>!card.trace.open);
          for(let n=0;n<Math.min(6,closed.length);n++)await readTrace(closed[(graphCursor+n)%closed.length]);
          graphCursor=closed.length?(graphCursor+6)%closed.length:0;
          graph.flush();
        }
        drainVoice();
      } catch (e) { failed(e); }
      finally { reading = false; $("#conversation-refresh").disabled = false; $("#conversation-older").disabled = false;
        if(refreshAgain){refreshAgain=false;setTimeout(()=>refresh(true),0);}
      }
    }

    async function fallbackProgressive(selected,owned,position=audio.currentTime){
      if(!selected?.streaming || selected.usingBlob)return false;
      if(selected.fallbackPromise)return selected.fallbackPromise;
      audioState("Recovering playback from the same audio. No new synthesis requested.");
      selected.fallbackPromise=(async()=>{
        const result=await selected.streaming.done;
        if(active!==selected || generation!==owned || captureBusy || voiceSuspended || !enabled || !playbackAllowed())return true;
        if(!result?.complete || !result.blob?.size)return false;
        // Reuse only the already verified encoded bytes. No provider/network
        // retry, new paid attempt, rewritten source or lost playback cursor.
        selected.generation=++generation;
        selected.usingBlob=true;selected.resumePosition=Number.isFinite(position)&&position>=0?position:0;
        selected.streaming.cancel();
        blobUrl=URL.createObjectURL(result.blob);audio.src=blobUrl;
        await startPlayback();return true;
      })();
      const attempt=selected.fallbackPromise;
      try{return await attempt;}finally{if(selected.fallbackPromise===attempt)delete selected.fallbackPromise;}
    }
    async function audioDecodeFailed(selected=active){
      if(!selected || active!==selected)return;
      const owned=generation,position=audio.currentTime;
      if(await fallbackProgressive(selected,owned,position))return;
      if(active!==selected || generation!==owned)return;
      liveSpeech?.interrupt(selected.source);
      stop("Audio decoding stopped. Your text and playback position remain available.");
      if(voiceEnabled && !voiceSuspended)nextSpokenReply("reply",selected.source);
    }
    async function startPlayback() {
      const selected = active;
      if (!selected || !blobUrl || !playbackAllowed() || !enabled || captureBusy) return;
      const owned = selected.generation;
      const position = selected.resumePosition ?? audio.currentTime;
      selected.verifying = true;
      audio.muted = true;
      // Unlock this element on a user gesture; source validation happens muted.
      try {
        const started = audio.play();
        const check = part(selected.source, selected.offset);
        let [,verified]=await Promise.all([started, check]);
        if (active !== selected || generation !== owned) return;
        if(selected.end!==undefined){
          const chars=Array.from(verified.text),count=selected.end-selected.offset;
          if(count>chars.length)throw Error("Audio exceeds its canonical text part");
          const text=chars.slice(0,count).join("");
          const hash=[...new Uint8Array(await crypto.subtle.digest("SHA-256",new TextEncoder().encode(text)))].map(byte=>byte.toString(16).padStart(2,"0")).join("");
          if(hash!==selected.textHash)throw Error("Audio text prefix changed");
          if(active!==selected || generation!==owned)return;
          verified={...verified,text,next_offset:selected.next};
        }
        audio.pause(); audio.currentTime = position;delete selected.resumePosition;
        selected.verifying = false;
        selected.segmentEnded = false;
        selected.verified=verified;
        selected.textEnd=selected.offset+Array.from(verified.text).length;
        selected.auto = true;selected.interrupted=false;
        audio.muted = false;
        await audio.play();
        if (active !== selected || generation !== owned) return;
        if(replyFocus?.reference.context_only){clearFocus();selected.focusVersion=focusVersion;}
        voiceContext=["worker_result","matrix_result","native_question"].includes(selected.source.kind)
          ?{source:selected.source.id,revision:selected.source.revision,context_only:true}:undefined;
        playLabel("Pause audio");
        audioState("Playing this reply.");
        presentState({playback:"playing"});playbackLabels();
        if(mediaSession)try{
          if(window.MediaMetadata)mediaSession.metadata=new window.MediaMetadata({title:selected.source.label||"Conversation reply",artist:"Daimon Interaction"});
          mediaSession.playbackState="playing";
        }catch{}
        followPlayback();
        prefetchNext(selected);
      } catch (e) {
        if (active !== selected || generation !== owned) return;
        selected.verifying = false;
        audio.pause(); audio.muted = true;
        if(e.name!=="NotAllowedError" && !transientConnectionError(e) && selected.streaming && !selected.usingBlob){
          if(await fallbackProgressive(selected,owned,position))return;
          if(active!==selected || generation!==owned)return;
        }
        if (e.name === "NotAllowedError") {
          toggle.disabled = false; playLabel("Play audio");
          audioState("Audio ready. Press Play audio to listen.");
        } else if(standalone && voiceEnabled && !voiceSuspended && transientConnectionError(e)){
          selected.auto=true;playLabel("Play audio");toggle.disabled=false;
          audioState("Connection unavailable. Playback will resume after reconnection.");
        } else {
          stop("This reply changed or is unavailable. Refresh before listening.");
          return {failed:true,generation};
        }
      }
    }
    async function requestAudio(source,offset,controller){
      const deadline=setTimeout(()=>controller.abort(),145000);
      try{
        const response = await fetch("api/interaction/audio", {method:"POST",credentials:"same-origin",
          headers:headers(),signal:controller.signal,body:JSON.stringify({source:source.id,revision:source.revision,offset,...(canStreamSpeech && source.kind!=="native_question" && window.SpeechStream?.available()?{stream:true}:{})})});
        if (!response.ok) {
          const value = await response.json();
          const error = new Error(value.detail || "Audio unavailable. Continue reading or refresh.");
          error.status = response.status; throw error;
        }
        if (response.headers.get("x-presentation-source") !== source.id
            || response.headers.get("x-presentation-revision") !== source.revision
            || response.headers.get("x-presentation-offset") !== String(offset)) throw new Error("Audio source mismatch");
        if(response.headers.get("content-type")?.startsWith("application/x-ndjson")){
          if(!canStreamSpeech || !window.SpeechStream?.available() || response.headers.get("x-audio-encoding")!=="mp3")throw Error("Unexpected streaming audio");
          const nextHeader=response.headers.get("x-presentation-next-offset"),next=nextHeader===""?null:Number(nextHeader),end=Number(response.headers.get("x-presentation-end-offset"));
          if(!Number.isSafeInteger(end)||end<=offset||end>offset+1600||next!==null&&next!==end)throw Error("Invalid stream boundary");
          return {streaming:window.SpeechStream.open(response,controller.signal),next,end,textHash:response.headers.get("x-presentation-text-sha256"),prefetch:true};
        }
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
        const nextHeader=response.headers.get("x-presentation-next-offset"),next=nextHeader===""?null:Number(nextHeader);
        if(next!==null && (!Number.isSafeInteger(next)||next<=offset))throw Error("Invalid audio continuation");
        const endHeader=response.headers.get("x-presentation-end-offset"),end=endHeader?Number(endHeader):undefined;
        if(end!==undefined && (!Number.isSafeInteger(end)||end<=offset||end>offset+1600||next!==null&&next!==end))throw Error("Invalid audio segment boundary");
        return {blob,next,end,textHash:response.headers.get("x-presentation-text-sha256"),prefetch:response.headers.get("x-presentation-prefetch")==="true"};
      }finally{clearTimeout(deadline);}
    }
    function prefetchNext(selected){
      if(!selected.prefetch || voiceSuspended || captureBusy || active!==selected || prefetched || queuedPreparations.size || sessionMuted(selected.source))return;
      if(selected.source.kind!=="integrator_live"){
        if(selected.next===null)return;
        const controller=new AbortController(),source=selected.source,offset=selected.next;
        const waiting={controller,source,offset};prefetched=waiting;
        waiting.result=requestAudio(source,offset,controller).then(value=>({value}),error=>({error}));return;
      }
      if(!voiceEnabled || !canSpeakLive || !selected.auto || selected.interrupted || selected.segmentEnded)return;
      const owned=generation,waiting={controller:new AbortController(),owner:selected.source,ahead:true};prefetched=waiting;
      waiting.selection=(async()=>{
        const source=await liveSpeech?.next() || liveSpeech?.continuation(selected.source);
        if(!source || prefetched!==waiting || active!==selected || generation!==owned || voiceSuspended || captureBusy || selected.interrupted || sessionMuted(source)){
          liveSpeech?.unsent(source);waiting.controller.abort();if(prefetched===waiting)prefetched=undefined;return;
        }
        if(source.kind==="integrator_live" && !liveSpeech?.admit(source)){prefetched=undefined;return;}
        waiting.source=source;waiting.offset=source.speech_start||0;
        // Own exactly one preparation, including its failed/uncertain result.
        waiting.result=requestAudio(source,waiting.offset,waiting.controller).then(value=>({value}),error=>({error}));
      })().catch(()=>{waiting.controller.abort();if(prefetched===waiting)prefetched=undefined;});
    }
    async function listenTo(source, offset = 0, chain) {
      if(captureBusy)return;
      const key=preparationKey(source,offset);
      const prepared=queuedPreparations.get(key) || (prefetched?.source?.id===source.id && prefetched.source.revision===source.revision && prefetched.offset===offset?prefetched:undefined);
      if(prepared===prefetched)prefetched=undefined;
      queuedPreparations.delete(key);
      if(!chain?.queued){cancelPreparedMessage(source);for(let n=voiceQueue.length-1;n>=0;n--)if(sameNative(voiceQueue[n],source))removeQueued(n);}
      if(!chain?.queued){skippedMessages.delete(messageKey(source));liveSpeech?.continueSource(source);voiceSuspended=false;}
      stop();
      if(voiceEnabled)voiceBusy=true;
      const owned = generation;
      active = {source, offset, generation:owned, auto:false, focusVersion:chain?.focusVersion ?? focusVersion,queued:!!chain?.queued};
      const selected = active;playbackLabels();
      $("#conversation-player").hidden = false;
      $("#conversation-playing").textContent = source.kind.startsWith("integrator_") ? "Conversation reply" : source.label;
      presentState({playback:"preparing"});
      audioState("Preparing audio…"); toggle.disabled = true;
      requestAbort = prepared?.controller||new AbortController();
      const controller = requestAbort;playbackLabels();
      try {
        const result=prepared?await prepared.result:{value:await requestAudio(source,offset,controller)};
        if(result.error)throw result.error;
        const {blob,...metadata}=result.value;
        if(active!==selected || generation!==owned)return;
        Object.assign(selected,metadata);
        blobUrl = selected.streaming?.url || URL.createObjectURL(blob);
        selected.streaming?.onError(()=>{if(active===selected)stop("Speech stream ended. Your text remains available; no automatic retry.");});
        selected.streaming?.onDecoderError?.(()=>{if(active===selected)void audioDecodeFailed(selected);});
        if(active!==selected)return;
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
      } finally { if (requestAbort === controller) requestAbort = undefined; }
    }
    audio.addEventListener("error",()=>{void audioDecodeFailed();});
    audio.addEventListener("pause",()=>{if(active && !active.verifying && !requestAbort){playLabel("Play audio");presentState({playback:undefined});}});
    audio.addEventListener("timeupdate",()=>followPlayback());
    audio.addEventListener("playing",()=>{
      if(active && !active.verifying && !audio.muted){if(active.queued)timing?.source(active.source,true);playLabel("Pause audio");presentState({playback:"playing"});}
    });
    audio.addEventListener("ended", async () => {
      if (!audio.ended || !active || active.segmentEnded || active.verifying || !active.auto || !playbackAllowed() || !enabled) return;
      presentState({playback:undefined});
      const selected=active, owned=generation;selected.segmentEnded=true;playbackLabels();
      rememberLiveCursor(liveSpeech?.ended(selected.source,selected.offset,selected.textEnd));
      rememberPosition();
      if (selected.next !== null) {
        audioState("A short pause before continuing…");
        if(await waitForSpeech("chunk",selected.source)){
          const result=await listenTo(selected.source,selected.next,selected);
          if(result?.failed && result.generation===generation && !active)nextSpokenReply("reply",selected.source);
        }
        return;
      }
      audioState("End of reply."); playLabel("Play audio again");
      if(selected.source.kind!=="native_question" || selected.finished){nextSpokenReply(selected.source.kind==="integrator_live"?"chunk":"reply",selected.source);return;}
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
      if(captureBusy)return;
      if (audio.paused) { cancelSpeechWait();continueSpeech();voiceSuspended=false;voiceStatus();if (audio.ended) audio.currentTime = 0; startPlayback(); }
      else pause("player");
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
      if(document.hidden){if(!playbackAllowed())pause("hidden");return;}
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
      pause("pagehide");stop();clearFiles();
    });
    window.addEventListener("pageshow", event => {
      if(event?.persisted && enabled && ready){
        if(active?.auto && audio.paused && blobUrl && !active.verifying && playbackAllowed())startPlayback();
        refresh(true);
      }
    });
    window.addEventListener("offline", () => {
      presentState({connected:false});
      if(!standalone || !voiceEnabled || voiceSuspended)pause("offline");
      $("#conversation-state").textContent = "Offline. Replies already loaded remain readable.";
    });
    window.addEventListener("online", () => {
      if (enabled && ready && playbackAllowed()) {
        if(active?.auto && audio.ended && !active.segmentEnded)audio.dispatchEvent?.(new Event("ended"));
        else if(active?.auto && audio.paused && !audio.ended && blobUrl && !active.verifying)startPlayback();
        refresh(true);
      }
    });
    const mediaSession=standalone?window.navigator?.mediaSession:undefined;
    if(mediaSession){
      for(const [action,handler] of Object.entries({
        play:()=>{voiceSuspended=false;voiceStatus();if(active && blobUrl)startPlayback();else {enableVoice();drainVoice();}},
        pause:()=>pause("media-session"),stop:()=>disableVoice()
      }))try{mediaSession.setActionHandler(action,handler);}catch{}
    }
    telegram?.onEvent?.("deactivated", pause);
    telegram?.BackButton?.onClick?.(() => setMode(false));
    if(standalone){$("#conversation-back").hidden=true;$("#conversation-telegram-older").hidden=true;}
    if(readOnly){document.body.classList.toggle("monitor-mode",true);$("#conversation-sent").open=true;}
    if(standalone && !readOnly)voiceMode=window.ConversationVoiceMode?.mount({
      portrait:$("#conversation-companion pre"),canEnter:()=>!!microphone && !microphone.ownsSend() && !captureBusy,
      canStart:()=>!!microphone && !microphone.ownsSend() && !captureBusy && !voiceBusy && !speechWait,
      onEnter:()=>{
        voiceModePrevious={enabled:voiceEnabled,suspended:voiceSuspended};
        conversationOnly=true;stop();audioFilterChanged();enableVoice();sound?.update({conversationOnly:true});
      },
      onSpeak:()=>microphone?.startConversation(),
      onLeave:()=>{
        microphone?.hold();conversationOnly=false;sound?.update({conversationOnly:false});
        if(voiceModePrevious?.enabled){voiceSuspended=voiceModePrevious.suspended;voiceStatus();}
        else disableVoice();
        voiceModePrevious=undefined;audioFilterChanged();companion?.update({});
      }
    });
    setMode(initialMode(), true, false);
    return {refresh, pause, stop:()=>{sound?.disable();disableVoice();},
      unauthenticated() { ready=false; failed({status:401,message:document.documentElement.dataset.sharedAuth==='true'?"Sign in again to reconnect.":"Open your conversation access file to reconnect."}); },
      authenticated() { ready = true; $("#conversation-open").disabled = false; }};
  }
  return {mount};
})();
