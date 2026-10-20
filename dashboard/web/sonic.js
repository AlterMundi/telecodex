"use strict";
// Source's authored state vocabulary is art; native facts and its rendering stay separate.
window.ConversationSound = (() => {
  const states=new Set(["absent","active","paused","blocked","usageLimited","budgetLimited","complete"]);
  function state(source,serverNow,elapsed=0) {
    const age=serverNow-Number(source.observed_at)*1000+elapsed;
    if(!source.fresh || !Number.isFinite(age) || age<0 || age>20000 || !states.has(source.goal_state)
      || !["working","idle"].includes(source.execution) || typeof source.waiting_for_human!=="boolean")return "unknown";
    if(source.waiting_for_human)return "waiting";
    if(source.execution==="working")return "working";
    if(["blocked","usageLimited","budgetLimited"].includes(source.goal_state))return "blocked";
    return source.goal_state==="active"?"goal_idle":"passive";
  }
  function validate(v) {
    const number=(key,lo,hi)=>typeof v[key]==="number" && Number.isFinite(v[key]) && v[key]>=lo && v[key]<=hi;
    return v?.schema==="daimon.sonic-expression/v1" && [v.id,v.revision].every(x=>typeof x==="string" && /^[A-Za-z0-9_.-]{1,80}$/.test(x))
      && [v.author,v.label].every(x=>typeof x==="string" && x.length>0 && x.length<=256)
      && number("fundamental_hz",20,256) && number("pulse_hz",0,4) && number("attack_ms",20,2000) && number("release_ms",20,2000)
      && number("peak_gain",0.001,0.2) && Number.isInteger(v.max_sessions) && v.max_sessions>=1 && v.max_sessions<=32 && ["sine","triangle"].includes(v.wave)
      && ["working","waiting","blocked","goal_idle","passive","unknown"].every(key=>Array.isArray(v.intervals?.[key]) && [0,2].includes(v.intervals[key].length)
        && v.intervals[key].every(n=>Number.isInteger(n) && n>=1 && n<=64));
  }
  function plan(frame,vocabulary,elapsed=0) {
    if(frame?.schema!=="telecodex.sonic-state/v1" || !Array.isArray(frame.sources) || frame.sources.length>65 || !Number.isFinite(frame.observed_at))throw Error("State observation unavailable");
    const seen=new Set(),rank={waiting:0,blocked:1,working:2,goal_idle:3,unknown:4,passive:5};
    const sources=frame.sources.filter(s=>typeof s.id==="string" && /^-?\d+:\d+$/.test(s.id) && !seen.has(s.id) && seen.add(s.id))
      .map(s=>({...s,state:state({...s,fresh:s.fresh && frame.connected===true},frame.observed_at*1000,elapsed)}));
    const eligible=sources.filter(s=>Number.isSafeInteger(s.session) && s.session>0 && vocabulary.intervals[s.state].length===2)
      .sort((a,b)=>rank[a.state]-rank[b.state] || a.id.localeCompare(b.id));
    return {sources,voices:eligible.slice(0,vocabulary.max_sessions),omitted:eligible.length-Math.min(eligible.length,vocabulary.max_sessions),
      unconfirmed:sources.filter(s=>s.state==="unknown").length,truncated:!!frame.truncated};
  }
  function renderer({Context=window.AudioContext||window.webkitAudioContext,changed=()=>{}}={}) {
    let context,master,active,retiring,quiet=false,volume=0.3,signature="",catalog;
    const halt=group=>{if(!group)return;clearTimeout(group.timer);for(const node of group.nodes){try{node.stop?.();}catch{}try{node.disconnect();}catch{}}};
    function stop(){const old=context;context=undefined;master?.disconnect();halt(active);halt(retiring);active=retiring=master=undefined;signature="";old?.close().catch(()=>{});changed(false);}
    function level(){if(master && context){const p=master.gain;p.cancelScheduledValues(context.currentTime);p.setValueAtTime(quiet?0:volume*(catalog?.peak_gain||0)/2,context.currentTime);}}
    function render(composition,v){
      catalog=v;level();if(!context || context.state!=="running")return {voices:[],dropped:composition.voices.length,running:false};
      const voices=composition.voices.filter(s=>v.fundamental_hz*v.intervals[s.state][1]/v.intervals[s.state][0]<context.sampleRate*0.45);
      const key=JSON.stringify([v,voices.map(s=>[s.id,s.state])]);
      if(key===signature)return {voices,dropped:composition.voices.length-voices.length,running:true};
      signature=key;halt(retiring);retiring=active;const at=context.currentTime;
      if(retiring){retiring.gain.gain.cancelScheduledValues(at);retiring.gain.gain.setTargetAtTime(0,at,v.release_ms/3000);
        const owned=retiring;owned.timer=setTimeout(()=>{halt(owned);if(retiring===owned)retiring=undefined;},v.release_ms*2);}
      const gain=context.createGain();gain.gain.setValueAtTime(0,at);gain.gain.linearRampToValueAtTime(1,at+v.attack_ms/1000);gain.connect(master);
      active={gain,nodes:[gain]};
      if(voices.length){
        const tone=(frequency,amplitude,id)=>{
          const oscillator=context.createOscillator(),level=context.createGain();oscillator.type=v.wave;oscillator.frequency.value=frequency;
          oscillator.sessionIdentity=id;level.gain.value=amplitude;oscillator.connect(level);level.connect(gain);oscillator.start(at);active.nodes.push(oscillator,level);
          if(v.pulse_hz){const lfo=context.createOscillator(),depth=context.createGain();lfo.frequency.value=v.pulse_hz;depth.gain.value=amplitude*0.25;
            lfo.connect(depth);depth.connect(level.gain);level.gain.value=amplitude*0.75;lfo.start(at);active.nodes.push(lfo,depth);}
        };
        tone(v.fundamental_hz,0.5,"anchor");
        for(const source of voices){const r=v.intervals[source.state];tone(v.fundamental_hz*r[1]/r[0],0.5/voices.length,source.id);}
      }
      return {voices,dropped:composition.voices.length-voices.length,running:true};
    }
    return {async start(){if(!Context)throw Error("State sound is unavailable in this browser");
        const owned=context || (context=new Context());
        if(!master){master=owned.createGain();master.gain.value=0;master.connect(owned.destination);}
        owned.onstatechange=()=>{if(context!==owned)return;if(owned.state!=="running"){halt(active);halt(retiring);active=retiring=undefined;signature="";master.gain.value=0;}changed(owned.state==="running");};
        await owned.resume();if(context!==owned){owned.close().catch(()=>{});return false;}changed(owned.state==="running");return owned.state==="running";},
      render,stop,setVolume(value){volume=Math.max(0,Math.min(1,Number(value)||0));level();},setQuiet(value){quiet=!!value;level();}};
  }
  function mount({api,unauthorized=()=>{},getAuthority=()=>"",now=()=>performance.now()}={}) {
    const button=document.querySelector("#conversation-sound-toggle"),status=document.querySelector("#conversation-sound-status"),score=document.querySelector("#conversation-sound-score"),volume=document.querySelector("#conversation-sound-volume");
    if(!button || !status || !score || !volume)return;
    let enabled=false,suspended=false,generation=0,authority,request,frame,vocabulary,catalog,received=0,requestElapsed=0,expiry,local={};
    const engine=renderer({changed(running){suspended=!running;if(enabled){button.textContent=running?"≈ Mute state sound":"≈ Resume state sound";if(!running)status.textContent="Audio suspended · tap to resume";}}});
    const isQuiet=()=>["permission","recording","finalizing"].includes(local.capture) || local.uploading===true || ["playing","preparing"].includes(local.playback);
    function disable(){enabled=false;authority=undefined;generation++;clearTimeout(expiry);request=undefined;frame=undefined;engine.stop();button.setAttribute("aria-pressed","false");button.textContent="≈ State sound";status.textContent="Off";score.replaceChildren();}
    function draw(){
      if(!enabled || !frame || !vocabulary)return;
      const elapsed=Math.max(0,now()-received)+requestElapsed,composition=plan(frame,vocabulary,elapsed);
      if(!frame.configured){disable();status.textContent="State observer is not configured";return;}
      const played=engine.render(composition,vocabulary);
      const label=document.querySelector("#conversation-sound-vocabulary");if(label)label.textContent=`${vocabulary.label} · ${vocabulary.fundamental_hz} Hz anchor · ${vocabulary.wave} · revision ${vocabulary.revision}`;
      const silent=isQuiet();
      engine.setQuiet(silent);
      status.textContent=!played.running?"Audio suspended · tap to resume":Number(volume.value)===0?"Muted · state labels remain available":silent?"Quiet during voice capture or reply":`${played.voices.length} session voices · ${composition.unconfirmed} unconfirmed · ${composition.omitted+played.dropped} omitted${composition.truncated?" · additional sources unavailable":""}`;
      const rows=composition.sources.map(source=>{const row=document.createElement("li");
        const ratio=vocabulary.intervals[source.state];row.textContent=`${source.label||source.id} · ${source.state.replaceAll("_"," ")} · ${ratio.length?ratio.join(":"):"quiet"}${played.voices.some(v=>v.id===source.id)?"":" · outside this mix"}`;return row;});score.replaceChildren(...rows);
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
      }catch(error){if(owned!==generation)return;engine.stop();frame=undefined;status.textContent=error.message||"Session state unavailable";if([401,403].includes(error.status)){disable();unauthorized(error);}}
      finally{if(request===pending)request=undefined;}
    }
    button.addEventListener("click",async()=>{
      if(enabled && !suspended){disable();return;}request=undefined;enabled=true;authority=getAuthority();const owned=++generation;button.setAttribute("aria-pressed","true");button.textContent="≈ Mute state sound";status.textContent="Opening audio…";
      try{if(!await engine.start() || owned!==generation)return;status.textContent="Waiting for session state";await refresh();}catch(error){if(owned===generation){disable();status.textContent=error.message;}}
    });
    volume.addEventListener("input",()=>engine.setVolume(Number(volume.value)));
    document.addEventListener("visibilitychange",()=>{if(document.hidden)disable();});
    window.addEventListener("pagehide",disable);
    window.addEventListener("offline",disable);
    return {refresh,reset(){disable();local={};},disable,update(patch){local={...local,...patch};engine.setQuiet(isQuiet());if(enabled && frame)draw();}};
  }
  return {state,validate,plan,renderer,mount};
})();
