"use strict";
window.InteractionLive = (() => {
  function mount({headers, refresh, expired, fetchStream=fetch}) {
    let active=false, suspended=false, controller, retry, heartbeat, generation=0, delay=1000, dirty=false, draining=false;
    async function update(){
      dirty=true;if(draining)return;draining=true;
      try{while(dirty && active && !suspended){dirty=false;await refresh();}}
      finally{draining=false;}
    }
    function disconnect(){
      generation++;clearTimeout(retry);clearTimeout(heartbeat);retry=heartbeat=undefined;
      controller?.abort();controller=undefined;
    }
    async function connect(){
      if(!active || suspended)return;
      const owned=++generation, selected=new AbortController();controller=selected;
      const current=()=>active && !suspended && generation===owned && controller===selected;
      // The server sends a heartbeat every15s. Bound a stalled fetch/read without
      // adding a second poller; a reconnect only invalidates the retained view.
      const touch=()=>{if(!current())return;clearTimeout(heartbeat);heartbeat=setTimeout(()=>{if(current())selected.abort();},45000);};
      touch();
      try{
        const response=await fetchStream('api/interaction/events',{credentials:'same-origin',headers:headers(),signal:selected.signal});
        if(!current())return;
        if(response.status===401 || response.status===403){stop();expired();return;}
        if(!response.ok || !response.body)throw Error('Conversation stream unavailable');
        delay=1000;touch();
        const reader=response.body.getReader(),decoder=new TextDecoder();let pending='';
        try{
          while(current()){
            const {done,value}=await reader.read();if(!current() || done)break;
            touch();pending+=decoder.decode(value,{stream:true});
            if(pending.length>65536)throw Error('Conversation stream exceeded bound');
            let boundary;
            while((boundary=pending.indexOf('\n\n'))>=0){
              const event=pending.slice(0,boundary);pending=pending.slice(boundary+2);
              if(event.includes('event: expired')){stop();expired();return;}
              if(event.startsWith('data:'))void update();
            }
          }
        }finally{await reader.cancel().catch(()=>{});}
      }catch(error){/* Current connection failures use the same bounded retry. */}
      if(current()){
        clearTimeout(heartbeat);heartbeat=undefined;controller=undefined;
        retry=setTimeout(()=>{retry=undefined;void connect();},delay);delay=Math.min(delay*2,10000);
      }
    }
    function start(){if(active)return;active=true;suspended=false;void connect();}
    function stop(){active=false;dirty=false;suspended=false;disconnect();}
    function reconnect(){if(!active || suspended)return;disconnect();void update();void connect();}
    window.addEventListener?.('pagehide',()=>{if(active){suspended=true;disconnect();}});
    window.addEventListener?.('pageshow',()=>{if(active){suspended=false;reconnect();}});
    window.addEventListener?.('online',reconnect);
    return {start,stop};
  }
  return {mount};
})();
