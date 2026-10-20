"use strict";
// Original compact portrait, inspired by Ani/Oliva's ASCII-Interfaces orb/hub
// vocabulary. Operational state comes from evidence, never the cosmetic pose.
window.ConversationCompanion = (() => {
  const faces = {
    calm: ["o   o", "  v  "], curious: ["o   O", "  ?  "],
    focused: ["-   -", "  .  "], listening: ["O   O", "  v  "],
    speaking: ["^   ^", "  o  "], delighted: ["^   ^", " \\_/ "],
    uncertain: [".   .", "  ~  "],
  };
  function portrait(pose) {
    const [eyes, mouth] = faces[pose] || faces.calm;
    return `    . + .\n  .-~~~~~-.\n (  ${eyes}  )\n  \\ ${mouth} /\n   '.___.'\n  _/| : |\\_\n (__)___(__)`;
  }
  function mount({now=()=>Date.now()}={}) {
    const box=document.querySelector("#conversation-companion");
    if(!box)return;
    const face=box.querySelector("pre"), title=box.querySelector(".companion-caption");
    const phases=box.querySelector(".companion-phases");
    const motion=window.matchMedia?.("(prefers-reduced-motion: reduce)");
    let state={}, observed=0, expiry, animation, lastReply, lastPhases, rest="calm";
    const staleAfter=20000;
    function draw() {
      const fresh=state.connected && observed && now()-observed<staleAfter;
      const rows=[];
      const add=(icon,label)=>rows.push({icon,label});
      if(state.capture==="permission")add("◉","Opening microphone");
      if(state.capture==="recording")add("◉","Listening");
      if(state.capture==="finalizing" || state.uploading)add("↑","Saving voice");
      if(fresh && state.transcribing)add("✎","Transcribing");
      if(fresh){
        if(state.activity?.integrator_busy)add("✦","Conversation active");
        const sources=state.activity?.sources || [];
        const observer=sources.reduce((n,s)=>n+(s.known && Number(s.observer_synthesizing)||0),0);
        const workers=sources.filter(s=>s.known && s.busy).length;
        if(observer)add("◈",`Observer · ${observer}`);
        if(workers)add("⌘",`${workers} ${workers===1?"session":"sessions"} active`);
        if(state.activity?.truncated || sources.some(s=>!s.known))add("◇","Some session activity unconfirmed");
        if(state.routing)add("⇄","Routing pending");
      }
      if(state.question)add("?","A question for you");
      if(state.playback==="preparing")add("♫","Preparing voice");
      if(state.playback==="playing")add("♫","Speaking");
      if(!fresh)add("◇",state.connected?"Session activity unconfirmed":state.connected===false?"Reconnecting":"Connect to begin");
      const pose=state.capture==="recording"?"listening":state.playback==="playing"?"speaking":
        state.question?"curious":!fresh?"uncertain":rows.length?"focused":rest;
      const art=portrait(pose);
      if(face.textContent!==art)face.textContent=art;
      box.dataset.pose=pose;
      const caption=!state.connected?(state.connected===false?"Let’s reconnect.":"Ready when you are."):state.question?"Your turn, whenever you’re ready.":
        state.capture==="recording"?"I’m listening.":rows.length?"You can keep talking or writing.":"Where shall we go next?";
      if(title.textContent!==caption)title.textContent=caption;
      const signature=JSON.stringify(rows);
      if(signature!==lastPhases){
        const chips=rows.map(({icon,label})=>{
          const chip=document.createElement("span"),symbol=document.createElement("span");
          symbol.textContent=icon;symbol.setAttribute("aria-hidden","true");
          chip.append(symbol,document.createTextNode(label));return chip;
        });
        phases.replaceChildren(...chips);lastPhases=signature;
      }
      box.dataset.fresh=String(!!fresh);
    }
    function arm() {
      clearTimeout(expiry);
      if(!document.hidden && observed && state.connected)
        expiry=setTimeout(draw,Math.max(0,observed+staleAfter-now()));
    }
    function update(patch) {state={...state,...patch};draw();}
    function observe(activity) {
      state.activity=activity?.schema==="telecodex.interaction-activity/v1"?activity:undefined;
      observed=state.activity?now():0;state.connected=true;draw();arm();
    }
    function reply(source) {
      if(!source || source.kind?.endsWith("_progress") || !source.revision || !source.id)return;
      const stamp=Date.parse(source.observed_at);
      if(!Number.isFinite(stamp) || lastReply && stamp<=lastReply.time)return;
      lastReply={time:stamp,id:source.id,revision:source.revision};
      // An optional receiver-validated finite hint may affect only this pose.
      // No operation, label, text, timer or HTML comes from the model.
      const hint=source.companion_hint;
      rest=Object.hasOwn(faces,hint?.rest)?hint.rest:"calm";
      draw();
      if(!document.hidden && !motion?.matches){
        animation?.cancel();
        animation=face.animate?.([{transform:"rotate(0deg)"},{transform:"rotate(-5deg)"},{transform:"rotate(4deg)"},{transform:"rotate(0deg)"}],{duration:550,iterations:1});
      }
    }
    function reset(){clearTimeout(expiry);animation?.cancel();state={};observed=0;lastReply=undefined;rest="calm";draw();}
    document.addEventListener("visibilitychange",()=>{
      if(document.hidden){clearTimeout(expiry);animation?.cancel();}
      else {draw();arm();}
    });
    motion?.addEventListener?.("change",()=>{if(motion.matches)animation?.cancel();});
    box.hidden=false;draw();
    return {update,observe,reply,reset};
  }
  return {mount};
})();
