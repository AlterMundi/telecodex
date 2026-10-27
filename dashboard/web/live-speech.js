"use strict";
/* Frozen native prefixes: source selection only, no recognition or work dispatch. */
window.LiveSpeech=(()=>{
  const hash=async text=>[...new Uint8Array(await crypto.subtle.digest("SHA-256",new TextEncoder().encode(text)))].map(n=>n.toString(16).padStart(2,"0")).join("");
  const identity=value=>typeof value.native_turn==="string" && typeof value.native_item==="string"
    && [value.native_turn,value.native_item].every(id=>/^[A-Za-z0-9_-]{1,128}$/.test(id))
    ?`${value.native_turn}:${value.native_item}`:undefined;
  function mount(){
    let binding,draft;const records=new Map();
    function reset(){binding=undefined;draft=undefined;records.clear();}
    function observe(value,nextBinding){
      if(binding!==nextBinding){reset();binding=nextBinding;}
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
      record.pending=undefined;record.attempted=source.speech_end;record.last=source;return true;
    }
    function unsent(source){
      const record=source && records.get(identity(source));
      if(record && record.pending?.source===source)record.pending=undefined;
    }
    function interrupt(source){
      const record=source && records.get(identity(source));
      if(record)record.interrupted=true;
    }
    function continueSource(source){
      const record=source && records.get(identity(source));
      if(!record)return;
      record.interrupted=false;
      if(source.kind==="integrator_live" && record.canonicalSource && !record.changed && record.attempted<record.canonicalSource.text_characters)
        return {...record.canonicalSource,speech_start:record.attempted};
    }
    function resume(source){
      const record=records.get(identity(source));
      if(record?.canonical!==source.id)return;
      return record.completed;
    }
    function ended(source,start=source.speech_start,end=source.speech_end){
      const record=records.get(identity(source));
      if(!record || record.changed && source.kind==="integrator_live" || !Number.isSafeInteger(start) || !Number.isSafeInteger(end) || end<=start)return;
      if(record.finished.size<128)record.finished.set(start,end);
      // Completed later ranges never erase an earlier interrupted/failed gap.
      let advance=true;
      while(advance){advance=false;for(const [begin,stop] of record.finished){
        if(begin<=record.completed && stop>record.completed){record.completed=stop;advance=true;}
        if(stop<=record.completed)record.finished.delete(begin);
      }}
      return record.canonicalSource?{source:record.canonicalSource,resume:record.completed}:undefined;
    }
    async function reconcile(source,validate){
      const record=records.get(identity(source));
      if(!record || !["integrator_answer","integrator_progress"].includes(source.kind))return {source};
      record.canonical=source.id;record.canonicalSource=source;record.pending=undefined;
      if(!record.last)return {source};
      // Validate the full frozen prefix against the canonical item, not a preview.
      let changed=false;
      try{await validate(record.last,record.last.speech_start);}catch(error){
        if(error.status!==409)throw error;changed=true;
      }
      if(changed){record.changed=true;record.completed=0;record.finished.clear();}
      return {source:{...source,speech_start:changed?0:record.attempted},changed,
        resume:changed?0:record.completed,skip:!changed && (record.interrupted || record.attempted>=source.text_characters)};
    }
    return {observe,next,admit,reconcile,ended,resume,unsent,interrupt,continueSource,reset};
  }
  return {mount};
})();
