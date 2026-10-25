"use strict";
// Original compact portrait, inspired by Ani/Oliva's ASCII-Interfaces orb/hub
// vocabulary. Operational state comes from evidence, never the cosmetic pose.
window.ConversationCompanion = (() => {
  const fallbackExpressions=new Set(["calm","curious","focused","listening","speaking","delighted","uncertain"]);
  // Source's interval field: an abstract presence, not a generic human face.
  // Variations are authored locally; no inference is spent rendering a frame.
  function portrait(pose,frame=0) {
    const centres={calm:" . : . ",curious:" . ? . ",focused:" > : < ",
      listening:"(  :  )",speaking:"~ .:. ~",delighted:"+  *  +",uncertain:" .   . "};
    const centre=pose==="speaking"?["~ .:. ~","< ::: >","~ :.: ~"][frame%3]:centres[pose]||centres.calm;
    const rows=["         . : .", "      .:'     ':.", "    .:   .---.   :.",
      "   :   .'     '.   :", ` --:--( ${centre} )--:--`,
      "   :   '.     .'   :", "    ':.  '---'  .:'", "       ':.   .:'", "          ':'"];
    const halo=frame===1?"*":frame===2?"+":".";
    return rows.map(row=>row.replaceAll(".",halo).padEnd(26)).join("\n");
  }
  function mount({now=()=>Date.now()}={}) {
    const box=document.querySelector("#conversation-companion");
    if(!box)return;
    const face=box.querySelector("pre"), title=box.querySelector(".companion-caption");
    const phases=box.querySelector(".companion-phases");
    const motion=window.matchMedia?.("(prefers-reduced-motion: reduce)");
    let state={}, observed=0, expiry, animation, lastReply, lastPhases, rest="calm";
    const staleAfter=20000;
    let pack, catalog, packSignature, frameTimer, frameIndex=0, frameExpression,lastExpression;
    function stopFrames(){clearTimeout(frameTimer);frameTimer=undefined;frameIndex=0;frameExpression=undefined;}
    let ambientTimer,ambientIndex=0,ambientKey,sonicFrame,sonicAt=0;
    function stopAmbient(){clearTimeout(ambientTimer);ambientTimer=undefined;ambientIndex=0;ambientKey=undefined;}
    function ambientPeriod(){
      if(state.playback==="playing")return 160;
      let factor=1;
      if(state.connected && observed && now()-observed<staleAfter){
        if(state.activity?.integrator_busy || state.activity?.sources?.some(s=>s.known && s.busy))factor=2;
        if(state.activity?.sources?.some(s=>s.known && s.observer_synthesizing>0))factor=3;
      }
      if(sonicFrame?.connected===true && now()-sonicAt<staleAfter){
        for(const source of sonicFrame.sources){
          if(!source || typeof source!=="object")continue;
          const age=sonicFrame.observed_at*1000-Number(source.observed_at)*1000+now()-sonicAt;
          if(source.fresh!==true || !Number.isFinite(age) || age<0 || age>staleAfter)continue;
          if(source.execution==="working")factor=Math.max(factor,2);
          if(source.tools_running===true)factor=Math.max(factor,3);
          if(source.goal_state==="active")factor=Math.max(factor,4);
          if(source.pending===true)factor=Math.max(factor,7);
          if(source.error===true || ["blocked","usageLimited","budgetLimited"].includes(source.goal_state))factor=11;
        }
      }
      return Math.round(1200/(1+Math.log2(factor)));
    }
    function ambient(expression,frames){
      const interval=ambientPeriod(),enabled=!document.hidden && !motion?.matches && (state.connected || state.capture || state.playback);
      const key=JSON.stringify([expression,interval,frames.frames.length,!!enabled]);
      if(key===ambientKey)return;
      stopAmbient();ambientKey=key;box.dataset.cycle=String(interval);
      if(!enabled || frames.frames.length<2)return;
      const tick=()=>{if(ambientKey!==key)return;ambientTimer=undefined;ambientIndex=(ambientIndex+1)%frames.frames.length;draw(false);if(ambientKey===key)ambientTimer=setTimeout(tick,interval);};
      ambientTimer=setTimeout(tick,interval);
    }

    function validPack(value){
      const ident=x=>typeof x==="string" && /^[a-zA-Z0-9_.-]{1,80}$/.test(x);
      if(!value || value.schema!=="daimon.ascii-expression/v1" || !ident(value.id) || !ident(value.revision)
        || JSON.stringify(value).length>262144 || !value.expressions || Object.keys(value.expressions).length>64)return false;
      for(const [id,entry] of Object.entries(value.expressions)){
        if(!ident(id) || typeof entry.alt!=="string" || !entry.alt || entry.alt.length>1000 || /[\x00-\x1f\x7f]/.test(entry.alt)
          || !entry.variants || !Object.keys(entry.variants).length || Object.keys(entry.variants).length>8)return false;
        for(const [name,v] of Object.entries(entry.variants))if(!ident(name) || !Array.isArray(v.frames) || !v.frames.length || v.frames.length>32
          || !Number.isInteger(v.frame_ms) || v.frame_ms<40 || v.frame_ms>2000 || v.frame_ms*v.frames.length>10000
          || v.frames.some(f=>typeof f!=="string" || !f || /[^\x20-\x7e\n]/.test(f) || f.split("\n").length>100 || f.split("\n").some(l=>l.length>240)))return false;
      }
      return ["cli","telegram","app"].every(client=>{
        const d=value.defaults?.[client];return d && Number.isInteger(d.columns) && d.columns>0 && d.columns<=240
          && Number.isInteger(d.rows) && d.rows>0 && d.rows<=100 && !!value.expressions[d.expression]?.variants[d.variant];
      });
    }
    function configure(value,digest){
      let selected;
      try {if(typeof digest==="string" && /^[a-f0-9]{64}$/.test(digest) && validPack(value))selected=value;}catch{}
      const signature=JSON.stringify([selected,digest]);
      if(signature===packSignature)return;
      stopAmbient();stopFrames();animation?.cancel();pack=selected;catalog=digest;packSignature=signature;lastReply=undefined;
      rest=pack?pack.defaults.app.expression:"calm";lastExpression=undefined;draw(false);
    }
    function selection(expression){
      const entry=pack.expressions[expression] || pack.expressions[pack.defaults.app.expression];
      const d=pack.defaults.app;
      // Fit actual available width using this client's rendered monospace glyph.
      let columns=d.columns;
      const width=box.getBoundingClientRect?.().width;
      if(width && window.getComputedStyle){
        const style=window.getComputedStyle(face), context=document.createElement("canvas").getContext?.("2d");
        if(context){context.font=style.font;const glyph=context.measureText("M").width;
          if(glyph>0)columns=Math.min(columns,Math.max(1,Math.floor((width-24)/glyph)));}
      }
      for(const key of [...new Set([d.variant,...Object.keys(entry.variants)])]){
        const v=entry.variants[key];
        if(v && v.frames.every(f=>f.split("\n").length<=d.rows && f.split("\n").every(l=>l.length<=columns)))return {entry,...v};
      }
      return {entry,frames:[entry.alt],frame_ms:0,fallback:true};
    }
    function framesFor(expression){return pack?selection(expression):{frames:[0,1,2].map(frame=>portrait(expression,frame)),frame_ms:240};}
    function playFrames(expression){
      stopFrames();if(document.hidden || motion?.matches)return;
      const selected=framesFor(expression);if(selected.frames.length<2)return;
      frameExpression=expression;
      const tick=()=>{frameIndex++;
        if(frameIndex>=selected.frames.length){stopFrames();draw(false);return;}
        draw(false);frameTimer=setTimeout(tick,selected.frame_ms);
      };
      frameTimer=setTimeout(tick,selected.frame_ms);
    }
    function draw(animateChange=true) {
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
        if(observer)add("◉",`Observer · ${observer}`);
        if(workers)add("⌘",`${workers} ${workers===1?"session":"sessions"} active`);
        if(state.activity?.truncated || sources.some(s=>!s.known))add("◇","Some session activity unconfirmed");
        if(state.routing)add("⇄","Routing pending");
      }
      if(state.question)add("?","A question for you");
      if(state.playback==="preparing")add("♫","Preparing voice");
      if(state.playback==="playing")add("♫","Speaking");
      if(!fresh)add("◇",state.connected?"Session activity unconfirmed":state.connected===false?"Reconnecting":"Connect to begin");
      const pose=state.capture==="recording"?"listening":state.playback==="playing"?"speaking":
        state.question?"curious":!fresh?"uncertain":rows.length && !lastReply?"focused":rest;
      const expression=pack?(Object.hasOwn(pack.expressions,pose)?pose:rest):pose;
      const changed=expression!==lastExpression;lastExpression=expression;
      if(changed){stopFrames();animation?.cancel();if(animateChange && (state.connected || state.capture==="recording"))playFrames(expression);}
      const selected=pack?selection(expression):undefined,frames=selected||framesFor(expression);
      ambient(expression,frames);
      const art=frames.frames[frameExpression===expression?Math.min(frameIndex,frames.frames.length-1):ambientIndex%frames.frames.length];
      if(selected){face.setAttribute("aria-hidden","false");face.setAttribute("role","img");face.setAttribute("aria-label",selected.entry.alt);face.title=pack.authorship;face.dataset.fallback=String(!!selected.fallback);}
      else {face.setAttribute("aria-hidden","true");face.setAttribute("aria-label","Companion portrait");face.dataset.fallback="false";}
      if(face.textContent!==art)face.textContent=art;
      box.dataset.pose=expression;
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
      stopFrames();
      rest=pack?(hint?.catalog===catalog && hint?.pack===pack.id && hint?.revision===pack.revision && Object.hasOwn(pack.expressions,hint?.rest)
        ?hint.rest:pack.defaults.app.expression):(fallbackExpressions.has(hint?.rest)?hint.rest:"calm");
      draw(false);
      animation?.cancel();
      if(hint?.gesture!=="none" && !document.hidden && !motion?.matches)playFrames(rest);
      const gesture=["wave","nod","none"].includes(hint?.gesture)?hint.gesture:"wave";
      if(!pack && !document.hidden && !motion?.matches && gesture!=="none"){
        const frames=gesture==="nod"
          ?[{transform:"translateY(0)"},{transform:"translateY(4px)"},{transform:"translateY(0)"}]
          :[{transform:"scale(1) rotate(0deg)",opacity:1},{transform:"scale(1.08) rotate(-3deg)",opacity:.65},{transform:"scale(1.04) rotate(3deg)",opacity:1},{transform:"scale(1) rotate(0deg)",opacity:1}];
        animation=face.animate?.(frames,{duration:850,iterations:1});
      }
    }
    function reset(){clearTimeout(expiry);stopAmbient();sonicFrame=undefined;sonicAt=0;stopFrames();animation?.cancel();state={};observed=0;lastReply=undefined;rest=pack?pack.defaults.app.expression:"calm";lastExpression=undefined;draw(false);}
    document.addEventListener("visibilitychange",()=>{
      if(document.hidden){clearTimeout(expiry);stopAmbient();stopFrames();animation?.cancel();draw();}
      else {draw();arm();}
    });
    motion?.addEventListener?.("change",()=>{if(motion.matches){stopAmbient();stopFrames();animation?.cancel();}draw();});
    box.hidden=false;draw();
    return {update,observe,reply,reset,configure,sonic(value){if(value?.schema!=="telecodex.sonic-state/v1" || !Array.isArray(value.sources) || value.sources.length>65 || !Number.isFinite(value.observed_at))return;sonicFrame=value;sonicAt=now();draw(false);}};
  }
  return {mount};
})();
