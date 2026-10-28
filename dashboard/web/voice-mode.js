"use strict";
// A foreground, bounded view. No persisted microphone opt-in or second sender.
window.ConversationVoiceMode=(()=>{
  function mount({portrait,onEnter,onLeave,onSpeak,canStart,canEnter=canStart}){
    if(!portrait || typeof document.createElement('dialog').showModal!=='function')return;
    const dialog=document.createElement('dialog');dialog.className='voice-mode';
    dialog.setAttribute('aria-label','Direct voice conversation');
    const heading=document.createElement('h2');heading.textContent='Conversation';
    const hint=document.createElement('p');hint.textContent='Only you and your daimon';
    const stage=document.createElement('div');stage.className='voice-mode-portrait';
    const status=document.createElement('p');status.className='voice-mode-state';status.setAttribute('role','status');
    const caption=document.createElement('p');caption.className='voice-mode-caption';
    const speak=document.createElement('button');speak.type='button';speak.textContent='Speak again';
    const back=document.createElement('button');back.type='button';back.textContent='Back to conversation';
    const controls=document.createElement('div');controls.className='voice-mode-controls';controls.append(speak,back);
    dialog.append(heading,hint,stage,status,caption,controls);document.body.append(dialog);
    let available=false,opened=false,marker,timer,rearm,waiting=false,heard=false,phase='idle',pending=false,playing=false,remaining,normal,continuous=false,listening=false,preparing=false,directPending=false,directWorking=false;
    function entryState(){
      portrait.dataset.voiceEntry=String(available);
      if(!available){
        if(normal)for(const [name,value] of normal){if(value===null)portrait.removeAttribute(name);else portrait.setAttribute(name,value);}
        return;
      }
      portrait.setAttribute('role',available&&!opened?'button':'img');portrait.tabIndex=available&&!opened?0:-1;
      portrait.setAttribute('aria-hidden',available?'false':'true');
      portrait.setAttribute('aria-label',opened?'Conversation avatar':'Open direct voice conversation');
      portrait.title=available&&!opened?'Direct voice conversation':'';
    }
    function close(){
      if(!opened)return;opened=false;clearTimeout(timer);clearTimeout(rearm);
      marker.replaceWith(portrait);marker=undefined;dialog.close();document.body.classList.remove('direct-voice-open');
      entryState();onLeave();portrait.focus();
    }
    function open(){
      if(opened || !available || document.hidden || !canEnter())return;
      if(onEnter()===false)return;
      opened=true;waiting=false;heard=false;pending=false;directPending=false;directWorking=false;listening=false;preparing=false;phase='idle';playing=false;remaining=undefined;
      entryState();
      marker=document.createComment('portrait return');portrait.before(marker);stage.append(portrait);
      document.body.classList.add('direct-voice-open');dialog.showModal();back.focus();
      timer=setTimeout(close,5*60*1000);status.textContent='Opening microphone…';caption.textContent='';speak.hidden=true;onSpeak();
    }
    function maybeRearm(){
      clearTimeout(rearm);
      if(continuous || !opened || !heard || playing || pending || phase!=='idle')return;
      rearm=setTimeout(()=>{if(opened && heard && !playing && phase==='idle' && !pending && canStart()){
        heard=false;waiting=false;speak.hidden=true;onSpeak();
      }},900);
    }
    function update(value){
      if(!opened)return;
      if(value.connected===false){close();return;}
      if(value.continuousState){listening=value.continuousState.active;preparing=value.continuousState.starting;directPending=value.continuousState.pending;directWorking=value.continuousState.working;}
      if(Object.hasOwn(value,'capture'))phase=value.capture||'idle';
      if(Object.hasOwn(value,'pending'))pending=value.pending;
      if(Number.isInteger(value.remaining)&&value.remaining>=0&&value.remaining<=120)remaining=value.remaining;
      if(Object.hasOwn(value,'playback')){playing=value.playback==='playing'||value.playback==='preparing';if(playing)heard=true;}
      if(value.sent){waiting=true;heard=false;}
      if(continuous){
        status.textContent=preparing?'Preparing voice':directPending?'Recording needs review':directWorking?'Saving your voice':playing&&listening?'Speaking · listening':listening?'Listening':'Voice paused';
        if(Object.hasOwn(value,'caption'))caption.textContent=value.caption;
        if(directPending)caption.textContent='Return to the conversation to review your recordings.';
        speak.hidden=preparing||listening||directPending;speak.disabled=directWorking||!canStart();speak.textContent='Start voice';
        dialog.dataset.phase=phase==='recording'?'listening':playing?'speaking':'waiting';return;
      }
      const recording=phase==='recording',working=phase==='permission'||phase==='finalizing'||value.uploading;
      status.textContent=recording?`Listening${remaining===undefined?'':` · ${Math.floor(remaining/60)}:${String(remaining%60).padStart(2,'0')} left`}`:working?'Saving your voice':playing?'Speaking':pending||phase==='ready'?'Recording needs review':waiting?'Waiting for your reply':'Ready';
      caption.textContent=recording?(value.message||''):pending||phase==='ready'?'Return to the conversation to review your recording.':!waiting&&!playing?(value.message||''):'';
      speak.hidden=recording||working||pending||phase==='ready'||playing;
      dialog.dataset.phase=recording?'listening':playing?'speaking':'waiting';
      maybeRearm();
    }
    portrait.addEventListener('click',open);
    portrait.addEventListener('keydown',event=>{if(event.key==='Enter'||event.key===' '){event.preventDefault();open();}});
    back.addEventListener('click',close);dialog.addEventListener('cancel',event=>{event.preventDefault();close();});
    speak.addEventListener('click',()=>{if(opened&&canStart()){waiting=false;heard=false;speak.hidden=true;onSpeak();}});
    document.addEventListener('visibilitychange',()=>{if(document.hidden)close();});
    window.addEventListener('pagehide',close);
    return {update,close,get active(){return opened;},configure(enabled,direct=false){
      if(opened&&continuous!==(direct===true))close();
      continuous=direct===true;
      if(enabled!==true&&!available)return;
      if(enabled===true&&!available)normal=['role','tabindex','aria-hidden','aria-label','title'].map(name=>[name,portrait.getAttribute(name)]);
      available=enabled===true;entryState();
      if(!available)close();
    }};
  }
  return {mount};
})();
