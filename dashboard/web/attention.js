"use strict";
// Navigation over the existing ledgers. Opening a card never sends a decision.
window.AttentionCarousel = (() => {
  function mount({line,panel,getItems,localOnly=false}) {
    if(!line || !panel)return;
    const el=(tag,text)=>{const n=document.createElement(tag);if(text)n.textContent=text;return n;};
    const strip=el("div"),previous=el("button","‹"),current=el("button"),next=el("button","›"),all=el("button");
    strip.className=localOnly?"for-you-navigation":"pending-carousel";strip.hidden=true;strip.setAttribute("aria-label",localOnly?"For you":"Pending requests");
    for(const b of [previous,current,next,all])b.type="button";
    previous.setAttribute("aria-label","Previous pending request");next.setAttribute("aria-label","Next pending request");
    current.className="pending-current";all.className="pending-all";
    if(localOnly){
      all.innerHTML='<svg viewBox="0 0 24 24" width="20" height="20" aria-hidden="true" fill="none" stroke="currentColor" stroke-width="1.8"><path d="m3 6 2 2 3-4M11 6h10M3 13l2 2 3-4M11 13h10M11 20h10"/></svg><span class="pending-count"></span>';
      strip.append(all);
    }else strip.append(previous,current,next,all);
    line.append(strip);
    const dialog=el("dialog"),heading=el("h2",localOnly?"For you":"Pending requests"),closeButton=el("button","Close"),scope=el("p","From this conversation and its routed sessions.");
    dialog.className="pending-panel";dialog.setAttribute("aria-label",localOnly?"For you":"Pending requests");
    closeButton.type="button";scope.className="note";
    dialog.append(heading,closeButton,scope,panel);document.body.append(dialog);
    let items=[],selected,position=0,timer,hover=false,focused=false,drag,dragged=false;
    const reduced=window.matchMedia?.("(prefers-reduced-motion: reduce)");
    function schedule(){
      clearTimeout(timer);
      if(!localOnly && items.length>1 && !document.hidden && !dialog.open && !hover && !focused && !drag && !reduced?.matches)
        timer=setTimeout(()=>step(1),6500);
    }
    function render(){
      strip.hidden=!localOnly&&!items.length;
      if(!localOnly)line.dataset.pending=String(!!items.length);
      scope.textContent=items.length?"From this conversation and its routed sessions.":"Nothing pending for you in this conversation.";
      previous.disabled=next.disabled=items.length<2;
      const item=items[position];selected=item?.id;
      current.textContent=item?`${item.label||"Conversation"} · ${item.title||"Your response is needed"}`:"";
      current.title=current.textContent;current.setAttribute("aria-label",`Open pending request: ${current.textContent}`);
      if(localOnly){
        all.querySelector(".pending-count").textContent=items.length?String(items.length):"";
        all.setAttribute("aria-label",`For you · ${items.length} pending requests`);all.title="For you";
      }else {all.textContent=`${items.length} pending`;all.setAttribute("aria-label",`Open list · ${items.length} pending requests`);}
      strip.title=items.length?`${position+1} of ${items.length}`:"";schedule();
    }
    function step(delta){if(!items.length)return;position=(position+delta+items.length)%items.length;render();}
    function refresh(){
      const values=getItems(),index=values.findIndex(v=>v.id===selected);
      items=values;position=index>=0?index:Math.min(position,Math.max(0,items.length-1));render();
    }
    function close(restore=true){
      if(dialog.open){if(dialog.close)dialog.close();else dialog.removeAttribute("open");}
      if(restore)(localOnly?all:items.length?current:line).focus?.();schedule();
    }
    async function open(item){
      clearTimeout(timer);
      if(!dialog.open){if(dialog.showModal)dialog.showModal();else dialog.setAttribute("open","");}
      closeButton.focus();
      if(item){
        // Re-select from current metadata so an obsolete carousel row cannot
        // invoke a removed or different request after a concurrent refresh.
        const selected=getItems().find(v=>v.id===item.id);
        if(!selected){refresh();scope.textContent="This request changed. Choose a current pending request.";return;}
        await selected.open();
        if(!dialog.open)return;
        selected.box.setAttribute("tabindex","-1");selected.box.focus();selected.box.scrollIntoView?.({block:"start"});
      }
    }
    previous.addEventListener("click",()=>step(-1));next.addEventListener("click",()=>step(1));
    current.addEventListener("click",()=>{if(dragged){dragged=false;return;}void open(items[position]);});
    all.addEventListener("click",()=>void open());closeButton.addEventListener("click",()=>close());
    dialog.addEventListener("cancel",event=>{event.preventDefault();close();});
    dialog.addEventListener("close",()=>schedule());
    strip.addEventListener("mouseenter",()=>{hover=true;schedule();});
    strip.addEventListener("mouseleave",()=>{hover=false;schedule();});
    strip.addEventListener("focusin",()=>{focused=true;schedule();});
    strip.addEventListener("focusout",event=>{focused=strip.contains(event.relatedTarget);schedule();});
    strip.addEventListener("keydown",event=>{if(["ArrowLeft","ArrowRight"].includes(event.key)){event.preventDefault();step(event.key==="ArrowLeft"?-1:1);}});
    current.addEventListener("pointerdown",event=>{drag={id:event.pointerId,x:event.clientX,y:event.clientY};dragged=false;current.setPointerCapture?.(event.pointerId);schedule();});
    current.addEventListener("pointerup",event=>{
      if(!drag || drag.id!==event.pointerId)return;
      const dx=event.clientX-drag.x,dy=event.clientY-drag.y;drag=undefined;
      if(Math.abs(dx)>36 && Math.abs(dx)>Math.abs(dy)){dragged=true;step(dx<0?1:-1);}else schedule();
    });
    current.addEventListener("pointercancel",()=>{drag=undefined;dragged=false;schedule();});
    document.addEventListener("visibilitychange",schedule);reduced?.addEventListener?.("change",schedule);
    // Existing card readers remain the writers. Observe their metadata changes,
    // without another network poller or a second pending store.
    new MutationObserver(refresh).observe(panel,{childList:true,subtree:true,characterData:true});
    refresh();return {refresh,close};
  }
  return {mount};
})();
