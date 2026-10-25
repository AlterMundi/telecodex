"use strict";
window.InteractionLive = (() => {
  function mount({headers, refresh, expired, fetchStream=fetch}) {
    let active=false, controller, retry, delay=1000, dirty=false, draining=false;
    async function update(){
      dirty=true;if(draining)return;draining=true;
      try{while(dirty && active){dirty=false;await refresh();}}
      finally{draining=false;}
    }
    async function connect(){
      if(!active)return;
      controller=new AbortController();
      try{
        const response=await fetchStream('api/interaction/events',{credentials:'same-origin',headers:headers(),signal:controller.signal});
        if(response.status===401 || response.status===403){stop();expired();return;}
        if(!response.ok || !response.body)throw Error('Conversation stream unavailable');
        delay=1000;
        const reader=response.body.getReader(),decoder=new TextDecoder();let pending='';
        try{
          while(active){
            const {done,value}=await reader.read();if(done)break;
            pending+=decoder.decode(value,{stream:true});
            if(pending.length>65536)throw Error('Conversation stream exceeded bound');
            let boundary;
            while((boundary=pending.indexOf('\n\n'))>=0){
              const event=pending.slice(0,boundary);pending=pending.slice(boundary+2);
              if(event.includes('event: expired')){stop();expired();return;}
              if(event.startsWith('data:'))void update();
            }
          }
        }finally{await reader.cancel().catch(()=>{});}
      }catch(error){if(controller?.signal.aborted)return;}
      if(active){retry=setTimeout(connect,delay);delay=Math.min(delay*2,10000);}
    }
    function start(){if(active)return;active=true;void connect();}
    function stop(){active=false;dirty=false;clearTimeout(retry);controller?.abort();}
    window.addEventListener?.('pagehide',()=>{clearTimeout(retry);controller?.abort();});
    window.addEventListener?.('pageshow',event=>{if(active && event.persisted){void update();void connect();}});
    return {start,stop};
  }
  return {mount};
})();
