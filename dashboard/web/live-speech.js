"use strict";
/* Frozen native prefixes: source selection only, no recognition or work dispatch. */
window.LiveSpeech=(()=>{
  const hash=async text=>[...new Uint8Array(await crypto.subtle.digest("SHA-256",new TextEncoder().encode(text)))].map(n=>n.toString(16).padStart(2,"0")).join("");
  const identity=value=>typeof value.native_turn==="string" && typeof value.native_item==="string"
    && [value.native_turn,value.native_item].every(id=>/^[A-Za-z0-9_-]{1,128}$/.test(id))
    ?`${value.native_turn}:${value.native_item}`:undefined;
  function mount({read,write}={}){
    let binding,draft;const records=new Map();
    const storageKey=()=>"interaction-live-speech:"+binding;
    const integer=(value,max)=>Number.isSafeInteger(value) && value>=0 && value<=max;
    const snapshot=record=>({key:record.key,attempted:record.attempted,completed:record.completed,last:{
      id:record.last.id,revision:record.last.revision,kind:"integrator_live",
      native_turn:record.last.native_turn,native_item:record.last.native_item,
      speech_start:record.last.speech_start,speech_end:record.last.speech_end},
      partial:record.partial && {...record.partial},finished:[...record.finished]});
    function persist(){
      if(!binding || !write)return;
      // Retain selectors and media ranges only, never provisional text. A reload
      // holds these attempts until an explicit Listen and canonical validation.
      const saved=[...records.values()].filter(record=>record.last && !record.changed && !record.retired)
        .map(record=>record.canonicalSource?record.backup:snapshot(record)).filter(Boolean);
      const raw=JSON.stringify(saved);
      try{if(raw.length<=32768)write(storageKey(),raw);}catch{}
    }
    function restore(){
      if(!binding || !read)return;
      try{
        const raw=read(storageKey());if(!raw || raw.length>32768)return;
        const saved=JSON.parse(raw);if(!Array.isArray(saved) || saved.length>24)return;
        for(const value of saved){
          const last=value?.last,key=last && identity(last),end=value?.attempted;
          if(!key || value.key!==key || !integer(end,65536) || end===0
            || !integer(value.completed,end) || !integer(last.speech_start,end)
            || last.speech_start>=end || last.speech_end!==end || last.kind!=="integrator_live"
            || !new RegExp(`^live:${key}:${last.speech_start}:${end}:[a-f0-9]{64}$`).test(last.id)
            || typeof last.revision!=="string" || !/^[a-f0-9]{64}$/.test(last.revision)
            || !Array.isArray(value.finished) || value.finished.length>128)continue;
          if(value.finished.some(range=>!Array.isArray(range) || range.length!==2
            || !integer(range[0],end) || !integer(range[1],end) || range[0]>=range[1]))continue;
          const partial=value.partial;
          if(partial && (typeof partial.source!=="string" || partial.source.length>512
            || !integer(partial.start,end) || !integer(partial.end,end) || partial.start>partial.end))continue;
          records.set(key,{key,attempted:end,completed:value.completed,last,
            partial,finished:new Map(value.finished),interrupted:true,restored:true});
        }
      }catch{}
    }
    function reset(){binding=undefined;draft=undefined;records.clear();}
    function observe(value,nextBinding){
      if(binding!==nextBinding){reset();binding=nextBinding;restore();}
      const key=value && identity(value);
      if(!key || typeof value.text!=="string" || new TextEncoder().encode(value.text).length>65536){draft=undefined;return;}
      let record=records.get(key);
      if(!record){record={key,attempted:0,completed:0,finished:new Map()};records.set(key,record);while(records.size>24)records.delete(records.keys().next().value);}
      draft={value,record};
    }
    async function next(){
      if(!draft || draft.record.canonical || draft.record.pending || draft.record.interrupted)return;
      const {value,record}=draft,chars=Array.from(value.text),start=record.attempted;
      const window=chars.slice(start,start+(start?640:240)).join("");
      // A complete sentence/paragraph is useful speech; incomplete punctuation,
      // links and code remain visible but wait for more text or confirmation.
      const matches=[...window.matchAll(/[.!?]["'”’)]*\s+|\n+/gu)];
      if(!matches.length)return;
      const match=matches.at(-1),prefixWindow=window.slice(0,match.index+match[0].length);
      const prefix=chars.slice(0,start).join("")+prefixWindow;
      if((prefix.match(/```/g)||[]).length%2 || (prefix.replace(/```[^]*?```/g,"").match(/`/g)||[]).length%2
        || (prefix.match(/\[/g)||[]).length!==(prefix.match(/\]/g)||[]).length
        || (prefix.match(/\(/g)||[]).length!==(prefix.match(/\)/g)||[]).length)return;
      const end=Array.from(prefix).length;
      if(end<=start)return;
      // Hashing is an owned selection, not a paid/played attempt. Canonical
      // arrival or capture can invalidate it before the player admits it.
      const reservation={start,end};record.pending=reservation;
      const selectedBinding=binding,id=`live:${record.key}:${start}:${end}:${await hash(prefix)}`;
      const revision=await hash(`${selectedBinding}:${id}`);
      if(binding!==selectedBinding || records.get(record.key)!==record || draft?.record!==record || record.canonical || record.pending!==reservation)return;
      const source={id,revision,kind:"integrator_live",native_turn:value.native_turn,native_item:value.native_item,
        speech_start:start,speech_end:end,preview:prefixWindow,observed_at:value.observed_at,provisional:true};
      reservation.source=source;return source;
    }
    function admit(source){
      const record=records.get(identity(source));
      if(!record || record.canonical || draft?.record!==record || record.pending?.source!==source || record.attempted!==source.speech_start)return false;
      record.pending=undefined;record.attempted=source.speech_end;record.last=source;persist();return true;
    }
    function unsent(source){
      const record=source && records.get(identity(source));
      if(record && record.pending?.source===source)record.pending=undefined;
    }
    function interrupt(source){
      const record=source && records.get(identity(source));
      if(record){record.interrupted=true;persist();}
    }
    function continueSource(source){
      const record=source && records.get(identity(source));
      if(!record)return;
      record.interrupted=false;
      return continuation(source);
    }
    function continuation(source){
      const record=source && records.get(identity(source));
      if(record && !record.interrupted && source.kind==="integrator_live" && record.canonicalSource && !record.changed && record.attempted<record.canonicalSource.text_characters)
        return {...record.canonicalSource,speech_start:record.attempted};
    }
    function resume(source){
      const record=records.get(identity(source));
      if(record?.validatedSource?.id!==source.id || record.validatedSource.revision!==source.revision)return;
      return resumeOffset(record);
    }
    function canonicalSaved(source){
      const record=source && records.get(identity(source));
      if(record?.canonicalSource?.id!==source.id || record.canonicalSource.revision!==source.revision)return;
      record.retired=true;persist();
    }
    function needsReconcile(source){
      const record=source && records.get(identity(source));
      return !!record?.last && ["integrator_answer","integrator_progress"].includes(source.kind)
        && (record.validatedSource?.id!==source.id || record.validatedSource.revision!==source.revision || !record.canonicalSource);
    }
    function resumeOffset(record){
      const partial=record.partial;
      // A cursor is current media position, not a monotonic hearing receipt.
      // It may move backwards, but cannot jump over an earlier unsaid gap.
      return partial && partial.start<=record.completed?partial.end:record.completed;
    }
    function checkpoint(source,start,end){
      const record=source && records.get(identity(source));
      if(record && record.canonicalSource?.id===source.id && record.canonicalSource.revision===source.revision
        && source.kind!=="integrator_live" && integer(start,source.text_characters) && integer(end,source.text_characters)
        && end>=start && start<=resumeOffset(record)){
        // Explicit canonical playback can extend the contiguous live cursor,
        // but a later chunk still cannot jump over an earlier unsaid gap.
        record.completed=resumeOffset(record);record.partial={source:source.id,start,end};
        return {source:record.canonicalSource,resume:resumeOffset(record)};
      }
      if(!record || record.changed || source.kind!=="integrator_live"
        || !Number.isSafeInteger(start) || !Number.isSafeInteger(end)
        || start<source.speech_start || end<start || end>source.speech_end
        || source.speech_end>record.attempted)return;
      record.partial={source:source.id,start,end};
      persist();
      return record.canonicalSource?{source:record.canonicalSource,resume:resumeOffset(record)}:undefined;
    }
    function ended(source,start=source.speech_start,end=source.speech_end){
      const record=records.get(identity(source));
      if(!record || record.changed && source.kind==="integrator_live" || !Number.isSafeInteger(start) || !Number.isSafeInteger(end) || end<=start)return;
      if(record.partial?.source===source.id)record.partial=undefined;
      if(record.finished.size<128)record.finished.set(start,end);
      // Completed later ranges never erase an earlier interrupted/failed gap.
      let advance=true;
      while(advance){advance=false;
        if(record.partial && record.partial.start<=record.completed){
          record.completed=Math.max(record.completed,record.partial.end);record.partial=undefined;advance=true;
        }
        for(const [begin,stop] of record.finished){
        if(begin<=record.completed && stop>record.completed){record.completed=stop;advance=true;}
        if(stop<=record.completed)record.finished.delete(begin);
      }}
      persist();
      return record.canonicalSource?{source:record.canonicalSource,resume:record.completed}:undefined;
    }
    async function reconcile(source,validate){
      const record=records.get(identity(source));
      if(!record || !["integrator_answer","integrator_progress"].includes(source.kind))return {source};
      const mapping={source};record.mapping=mapping;
      if(record.last && !record.canonicalSource)record.backup=snapshot(record);
      record.canonical=source.id;record.canonicalSource=undefined;record.pending=undefined;
      if(!record.last){record.canonicalSource=source;record.validatedSource=source;return {source};}
      // Validate the full frozen prefix against the canonical item, not a preview.
      let changed=false;
      try{await validate(record.last,record.last.speech_start);}catch(error){
        if(error.status!==409)throw error;changed=true;
      }
      if(records.get(record.key)!==record || record.mapping!==mapping)return {source,skip:true};
      record.canonicalSource=source;record.validatedSource=source;
      if(changed){record.changed=true;record.completed=0;record.partial=undefined;record.finished.clear();}
      persist();
      return {source:{...source,speech_start:changed?0:record.attempted},changed,
        resume:changed?0:resumeOffset(record),skip:record.restored===true || !changed && (record.interrupted || record.attempted>=source.text_characters)};
    }
    return {observe,next,admit,reconcile,ended,resume,checkpoint,canonicalSaved,needsReconcile,unsent,interrupt,continueSource,continuation,reset};
  }
  return {mount};
})();
