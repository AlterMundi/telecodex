"use strict";
// Source's authored state vocabulary is art; native facts and its rendering stay separate.
window.ConversationSound = (() => {
  const states=new Set(["absent","active","paused","blocked","usageLimited","budgetLimited","complete"]);
  function state(source,serverNow,elapsed=0) {
    const age=serverNow-Number(source.observed_at)*1000+elapsed;
    if(!source.fresh || !Number.isFinite(age) || age<0 || age>20000 || !states.has(source.goal_state)
      || !["working","idle"].includes(source.execution) || typeof source.waiting_for_human!=="boolean")return "unknown";
    if(source.waiting_for_human || source.pending===true)return "waiting";
    if(source.execution==="working")return "working";
    if(["blocked","usageLimited","budgetLimited"].includes(source.goal_state))return "blocked";
    return source.goal_state==="active"?"goal_idle":"passive";
  }
  function components(source,serverNow,elapsed=0) {
    const age=serverNow-Number(source.observed_at)*1000+elapsed;
    if(!source.fresh || !Number.isFinite(age) || age<0 || age>20000)return [];
    const result=[];
    if(["idle","working"].includes(source.execution))result.push("idle");
    if(source.execution==="working")result.push("active");
    if(source.tools_running===true)result.push("tools");
    if(source.goal_state==="active")result.push("goal");
    if(source.pending===true)result.push("pending");
    if(source.error===true || ["blocked","usageLimited","budgetLimited"].includes(source.goal_state))result.push("blocked");
    return result;
  }
  function validate(v) {
    const number=(key,lo,hi)=>typeof v[key]==="number" && Number.isFinite(v[key]) && v[key]>=lo && v[key]<=hi;
    return v?.schema==="daimon.sonic-expression/v2" && [v.id,v.revision].every(x=>typeof x==="string" && /^[A-Za-z0-9_.-]{1,80}$/.test(x))
      && [v.author,v.label].every(x=>typeof x==="string" && x.length>0 && x.length<=256)
      && number("fundamental_hz",20,256) && number("pulse_hz",0,1) && number("pulse_depth",0,0.5) && number("attack_ms",20,2000) && number("release_ms",20,2000)
      && number("peak_gain",0.001,0.8) && Number.isInteger(v.max_sessions) && v.max_sessions>=1 && v.max_sessions<=32 && v.wave==="sine"
      && !["partials","texture_hz","texture_depth","intervals"].some(key=>key in v)
      && Object.keys(v.components||{}).length===6 && ["idle","active","tools","goal","pending","blocked"].every(key=>Number.isInteger(v.components?.[key]) && v.components[key]>=1 && v.components[key]<=64);
  }
  function plan(frame,vocabulary,elapsed=0) {
    if(frame?.schema!=="telecodex.sonic-state/v1" || !Array.isArray(frame.sources) || frame.sources.length>65 || !Number.isFinite(frame.observed_at))throw Error("State observation unavailable");
    const seen=new Set(),priority=s=>s.components.includes("pending")?0:s.components.includes("blocked")?1:s.components.includes("active")?2:s.components.includes("goal")?3:4;
    const sources=frame.sources.filter(s=>typeof s.id==="string" && /^-?\d+:\d+$/.test(s.id) && !seen.has(s.id) && seen.add(s.id))
      .map(s=>{const observed={...s,fresh:s.fresh && frame.connected===true};return {...s,state:state(observed,frame.observed_at*1000,elapsed),components:components(observed,frame.observed_at*1000,elapsed)};});
    const eligible=sources.filter(s=>Number.isSafeInteger(s.session) && s.session>0 && s.components.length>0)
      .sort((a,b)=>priority(a)-priority(b) || a.id.localeCompare(b.id));
    return {sources,voices:eligible.slice(0,vocabulary.max_sessions),omitted:eligible.length-Math.min(eligible.length,vocabulary.max_sessions),
      unconfirmed:sources.filter(s=>s.state==="unknown" || [s.tools_running,s.error,s.pending].some(value=>typeof value!=="boolean")).length,truncated:!!frame.truncated};
  }
  function renderer({Context=window.AudioContext||window.webkitAudioContext,changed=()=>{}}={}) {
    let context,master,breath,lfo,modulation,quiet=false,volume=0.3,catalog,masterEnvelope,pulseKey,signature;
    const tones=new Map();
    // Track our linear envelopes rather than reading an AudioParam's scheduled target.
    const valueAt=(envelope,at)=>!envelope?0:at>=envelope.end?envelope.target:
      envelope.from+(envelope.target-envelope.from)*Math.max(0,(at-envelope.start)/(envelope.end-envelope.start));
    function ramp(node,envelope,target,at,seconds,force=false){
      if(!force && envelope?.target===target)return envelope;
      const from=valueAt(envelope,at);node.cancelScheduledValues(at);node.setValueAtTime(from,at);
      node.linearRampToValueAtTime(target,at+seconds);
      return {from,target,start:at,end:at+seconds};
    }
    const halt=tone=>{clearTimeout(tone.timer);tone.oscillator.stop();tone.oscillator.disconnect();tone.level.disconnect();};
    function stop(){
      const old=context;if(!old)return;
      const owned=[...tones.values()];tones.clear();
      const nodes=[master,breath,lfo,modulation];
      masterEnvelope=ramp(master.gain,masterEnvelope,0,old.currentTime,0.012);
      // Finish the fade before disconnect/close, even when a new context starts meanwhile.
      setTimeout(()=>{for(const tone of owned)halt(tone);for(const node of nodes){try{node?.stop?.();}catch{}try{node?.disconnect();}catch{}}old.close().catch(()=>{});},20);
      context=master=breath=lfo=modulation=masterEnvelope=undefined;pulseKey=signature=undefined;changed(false);
    }
    function level(){if(master && context)masterEnvelope=ramp(master.gain,masterEnvelope,quiet?0:volume*(catalog?.peak_gain||0),context.currentTime,0.012);}
    function render(composition,v){
      catalog=v;level();if(!context || context.state!=="running")return {voices:[],dropped:composition.voices.length,running:false};
      const voices=composition.voices.filter(s=>s.components.every(key=>v.fundamental_hz*v.components[key]<context.sampleRate*0.45));
      const at=context.currentTime,nextPulse=JSON.stringify([v.pulse_hz,v.pulse_depth]);
      if(nextPulse!==pulseKey){
        pulseKey=nextPulse;const depth=v.pulse_hz?v.pulse_depth:0;
        breath.gain.setValueAtTime(1-depth,at);modulation.gain.setValueAtTime(depth,at);lfo.frequency.setValueAtTime(v.pulse_hz,at);
      }
      // Equal frequencies share a phase-continuous carrier; membership stays in the score.
      const weights=new Map();
      for(const source of voices)for(const key of source.components){const hz=v.fundamental_hz*v.components[key];weights.set(hz,(weights.get(hz)||0)+1/(voices.length*source.components.length));}
      const seconds=Math.max(v.attack_ms,v.release_ms)/1000;
      const key=JSON.stringify([[...weights].sort((a,b)=>a[0]-b[0]),seconds]);
      if(key===signature)return {voices,dropped:composition.voices.length-voices.length,running:true};
      signature=key;
      for(const [hz,weight] of weights){
        let tone=tones.get(hz);
        if(!tone){const oscillator=context.createOscillator(),gain=context.createGain();gain.gain.setValueAtTime(0,at);oscillator.type="sine";oscillator.frequency.value=hz;
          oscillator.connect(gain);gain.connect(master);oscillator.start(at);tone={oscillator,level:gain};tones.set(hz,tone);}
        clearTimeout(tone.timer);tone.timer=undefined;
        tone.envelope=ramp(tone.level.gain,tone.envelope,weight,at,seconds,true);
      }
      for(const [hz,tone] of tones)if(!weights.has(hz)){
        clearTimeout(tone.timer);
        tone.envelope=ramp(tone.level.gain,tone.envelope,0,at,seconds,true);
        tone.timer=setTimeout(()=>{if(tones.get(hz)===tone){halt(tone);tones.delete(hz);}},seconds*1000+20);
      }
      return {voices,dropped:composition.voices.length-voices.length,running:true};
    }
    return {async start(){if(!Context)throw Error("State sound is unavailable in this browser");
        const owned=context || (context=new Context());
        if(!master){master=owned.createGain();master.gain.value=0;breath=owned.createGain();lfo=owned.createOscillator();modulation=owned.createGain();
          breath.gain.value=1;modulation.gain.value=0;lfo.type="sine";lfo.frequency.value=0;
          master.connect(breath);breath.connect(owned.destination);lfo.connect(modulation);modulation.connect(breath.gain);lfo.start();}
        owned.onstatechange=()=>{if(context!==owned)return;if(owned.state!=="running"){for(const tone of tones.values())halt(tone);tones.clear();master.gain.value=0;masterEnvelope=signature=undefined;}changed(owned.state==="running");};
        await owned.resume();if(context!==owned){owned.close().catch(()=>{});return false;}changed(owned.state==="running");return owned.state==="running";},
      render,stop,setVolume(value){volume=Math.max(0,Math.min(1,Number(value)||0));level();},setQuiet(value){quiet=!!value;level();}};
  }
  function mount({api,unauthorized=()=>{},getAuthority=()=>"",now=()=>performance.now()}={}) {
    const button=document.querySelector("#conversation-sound-toggle"),status=document.querySelector("#conversation-sound-status"),score=document.querySelector("#conversation-sound-score"),volume=document.querySelector("#conversation-sound-volume");
    if(!button || !status || !score || !volume)return;
    const feedback=document.querySelector("#conversation-sound-feedback"),controls=document.querySelector("#conversation-sound-controls");
    function setStatus(detail,brief=detail){status.textContent=detail;if(feedback){feedback.textContent=brief;feedback.hidden=detail==="Off";}if(controls)controls.hidden=!enabled;}
    let enabled=false,suspended=false,generation=0,authority,request,frame,vocabulary,catalog,received=0,requestElapsed=0,expiry,local={};
    const engine=renderer({changed(running){suspended=!running;if(enabled){button.textContent=running?"≈ Mute Beacon":"≈ Resume Beacon";if(!running)setStatus("Audio suspended · tap to resume");}}});
    const isQuiet=()=>["permission","recording","finalizing"].includes(local.capture) || local.uploading===true || ["playing","preparing"].includes(local.playback);
    function disable(){enabled=false;authority=undefined;generation++;clearTimeout(expiry);request=undefined;frame=undefined;engine.stop();button.setAttribute("aria-pressed","false");button.textContent="≈ Beacon";setStatus("Off");score.replaceChildren();}
    function draw(){
      if(!enabled || !frame || !vocabulary)return;
      const elapsed=Math.max(0,now()-received)+requestElapsed,composition=plan(frame,vocabulary,elapsed);
      if(!frame.configured){disable();setStatus("State observer is not configured");return;}
      const played=engine.render(composition,vocabulary);
      const label=document.querySelector("#conversation-sound-vocabulary");if(label)label.textContent=`${vocabulary.label} · ${vocabulary.fundamental_hz} Hz F1 · ${vocabulary.wave} · revision ${vocabulary.revision}`;
      const silent=isQuiet();
      engine.setQuiet(silent);
      setStatus(!played.running?"Audio suspended · tap to resume":Number(volume.value)===0?"Muted · state labels remain available":silent?"Quiet during voice capture or reply":`${played.voices.length} session voices · ${composition.unconfirmed} unconfirmed · ${composition.omitted+played.dropped} omitted${composition.truncated?" · additional sources unavailable":""}`, !played.running?"Beacon paused · tap to resume":Number(volume.value)===0?"Beacon muted":silent?"Beacon quiet while you speak or listen":played.voices.length?`Beacon · ${played.voices.length} session voice${played.voices.length===1?"":"s"}${composition.unconfirmed?" · some state unconfirmed":""}`:"Beacon quiet · no active session voices");
      const rows=composition.sources.map(source=>{const row=document.createElement("li");
        const notes=source.components.map(key=>`F${vocabulary.components[key]}`);row.textContent=`${source.label||source.id} · ${source.state.replaceAll("_"," ")} · ${notes.length?notes.join(" + "):"quiet"}${played.voices.some(v=>v.id===source.id)?"":" · outside this mix"}`;return row;});score.replaceChildren(...rows);
      clearTimeout(expiry);
      const remaining=frame.sources.map(s=>20001-(frame.observed_at*1000-Number(s.observed_at)*1000+elapsed)).filter(t=>Number.isFinite(t)&&t>0);
      if(remaining.length)expiry=setTimeout(()=>{try{draw();}catch{engine.stop();}},Math.min(...remaining));
    }
    async function refresh(){
      if(enabled && authority!==getAuthority()){disable();return;}
      if(!enabled || request || document.hidden)return;
      const owned=generation,start=now(),pending={};request=pending;
      try {
        const value=await api("interaction/sonic");if(!enabled || owned!==generation || authority!==getAuthority()){if(owned===generation)disable();return;}
        if(typeof value.vocabulary_json!=="string" || value.vocabulary_json.length>65536 || !/^[a-f0-9]{64}$/.test(value.vocabulary_sha256))throw Error("Sound vocabulary unavailable");
        const key=value.vocabulary_sha256+value.vocabulary_json;
        if(key!==catalog){const digest=await crypto.subtle.digest("SHA-256",new TextEncoder().encode(value.vocabulary_json));
          if(!enabled || owned!==generation || authority!==getAuthority()){if(owned===generation)disable();return;}
          if([...new Uint8Array(digest)].map(n=>n.toString(16).padStart(2,"0")).join("")!==value.vocabulary_sha256)throw Error("Sound vocabulary changed");
          const selected=JSON.parse(value.vocabulary_json);if(!validate(selected))throw Error("Unsupported sound vocabulary");vocabulary=selected;catalog=key;}
        frame=value;received=now();requestElapsed=Math.max(0,received-start);draw();
      }catch(error){if(owned!==generation)return;engine.stop();frame=undefined;setStatus(error.message||"Session state unavailable");if([401,403].includes(error.status)){disable();unauthorized(error);}}
      finally{if(request===pending)request=undefined;}
    }
    button.addEventListener("click",async()=>{
      if(enabled && !suspended){disable();return;}request=undefined;enabled=true;authority=getAuthority();const owned=++generation;button.setAttribute("aria-pressed","true");button.textContent="≈ Mute Beacon";setStatus("Opening audio…");
      try{if(!await engine.start() || owned!==generation)return;setStatus("Waiting for session state");await refresh();}catch(error){if(owned===generation){disable();setStatus(error.message);}}
    });
    volume.addEventListener("input",()=>engine.setVolume(Number(volume.value)));
    document.addEventListener("visibilitychange",()=>{if(document.hidden)disable();});
    window.addEventListener("pagehide",disable);
    window.addEventListener("offline",disable);
    return {refresh,reset(){disable();local={};},disable,update(patch){local={...local,...patch};engine.setQuiet(isQuiet());if(enabled && frame)draw();}};
  }
  return {state,components,validate,plan,renderer,mount};
})();
