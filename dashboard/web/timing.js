"use strict";
/* Bounded, page-local observation. No media, transcript, storage or network. */
window.ConversationTiming=(()=>{
  function mount({now=()=>performance.now()}={}){
    const list=document.querySelector('#conversation-timing-records'),pause=document.querySelector('#conversation-timing-pause'),clear=document.querySelector('#conversation-timing-clear');
    if(!list || !pause || !clear)return;
    const recordings=new Map(),sources=new Map();let binding,sequence=0,pauseMillis;
    const positive=value=>Number.isSafeInteger(value)&&value>0;
    const duration=(start,end)=>Number.isFinite(start)&&Number.isFinite(end)&&end>=start?`${((end-start)/1000).toFixed(2)} s`:'not observed';
    const element=(tag,text)=>{const node=document.createElement(tag);node.textContent=text;return node;};
    function render(){
      pause.textContent=pauseMillis===undefined?'':`Pause applied: ${pauseMillis.toFixed(1)} ms (browser handler, not acoustic silence).`;
      const rows=[];
      for(const recording of [...recordings.values()].reverse()){
        const stages=recording.stages,peers=[...recordings.values()].filter(r=>positive(recording.local)&&r.local===recording.local);
        const shared=peers.length>1;
        const matches=!shared&&positive(recording.local)?[...sources.values()].filter(s=>s.local===recording.local):[];
        const first=field=>Math.min(...matches.map(s=>s[field]).filter(t=>Number.isFinite(t)&&t>=stages.send));
        const row=element('li','');row.append(element('strong',`Recording ${recording.sequence}`));
        row.append(element('p',`First caption: ${duration(stages.started,stages.partial)} · Stop → saved: ${duration(stages.ended,stages.retained)}`));
        row.append(element('p',shared?'Shared native turn; response timing unassigned.':`Send → same-turn text: ${duration(stages.send,first('seen'))} · Send → playback: ${duration(stages.send,first('playing'))}`));
        rows.push(row);
      }
      if(!rows.length)rows.push(element('li','No recordings measured on this page.'));
      list.replaceChildren(...rows);
    }
    function reset(){recordings.clear();sources.clear();sequence=0;pauseMillis=undefined;render();}
    clear.addEventListener('click',reset);
    function bind(value){if(binding!==value){binding=value;reset();}}
    function mark(id,stage){
      if(typeof id!=='string'||id.length>128||!['requested','started','partial','ended','send','ready','retained','discarded'].includes(stage))return;
      let recording=recordings.get(id);
      if(!recording){if(stage!=='requested')return;recording={sequence:++sequence,stages:{}};recordings.set(id,recording);while(recordings.size>8)recordings.delete(recordings.keys().next().value);}
      if(recording.stages[stage]!==undefined)return;
      recording.stages[stage]=now();render();
    }
    function input(value){
      const recording=recordings.get(value.id);
      if(!recording||!positive(value.local_turn)||value.phase!=='accepted'||recording.local===value.local_turn)return;
      recording.local=value.local_turn;render();
    }
    function source(value,playing=false){
      if(!value?.kind?.startsWith('integrator_')||value.current===false||value.correction_pending||value.native_binding_current===false)return;
      const key=typeof value.native_turn==='string'&&typeof value.native_item==='string'?`${value.native_turn}:${value.native_item}`:value.id;
      if(typeof key!=='string'||key.length>512)return;
      let local=value.local_turn;
      if(!positive(local)&&typeof value.native_turn==='string')local=[...sources.values()].find(s=>s.turn===value.native_turn)?.local;
      if(!positive(local))return;
      let item=sources.get(key);
      if(!item){item={local,turn:value.native_turn};sources.set(key,item);while(sources.size>16)sources.delete(sources.keys().next().value);}
      if(item.local!==local)return;
      const field=playing?'playing':'seen';if(item[field]!==undefined)return;
      item[field]=now();render();
    }
    function pauseApplied(start){if(!Number.isFinite(start))return;const elapsed=now()-start;if(elapsed>=0&&elapsed<60000){pauseMillis=elapsed;render();}}
    render();return {bind,mark,input,source,pauseApplied};
  }
  return {mount};
})();
