/* Mono PCM16/16kHz capture. No inference, network, playback or retained history. */
class InteractionPCM extends AudioWorkletProcessor {
  constructor(){
    super();this.active=true;this.index=0;this.next=0;this.previous=0;this.ratio=sampleRate/16000;
    this.observe=false;this.total=0;this.lastEnergy=0;this.energy=0;this.frames=[];this.port.onmessage=event=>{if(event.data==='observe')this.observe=true;if(event.data&&typeof event.data.boundary==='string'&&/^[a-zA-Z0-9:-]{1,80}$/.test(event.data.boundary)){this.flush();this.port.postMessage({boundary:event.data.boundary,totalBytes:this.total*2});}if(event.data==='finish'){this.active=false;this.flush();this.port.postMessage({finished:true});}};
  }
  flush(){
    if(!this.frames.length)return;
    const data=new ArrayBuffer(this.frames.length*2),view=new DataView(data);
    this.frames.forEach((value,index)=>view.setInt16(index*2,Math.round(Math.max(-1,Math.min(1,value))*32767),true));
    this.frames=[];this.port.postMessage({pcm:data,totalBytes:this.total*2},[data]);
  }
  process(inputs){
    if(!this.active)return false;
    const input=inputs[0]?.[0];if(!input)return true;
    for(const current of input){
      while(this.next<=this.index){
        const fraction=this.index===0?1:this.next-(this.index-1);
        const value=this.previous+(current-this.previous)*fraction;
        if(this.total>=300*16000){this.active=false;this.flush();this.port.postMessage({captureError:'session_limit'});return false;}
        this.frames.push(value);this.total++;this.energy+=value*value;this.next+=this.ratio;
        // A conservative acoustic freshness hint, not a speech/turn classifier.
        // It fences a delayed server verdict even before this chunk is flushed.
        if(this.observe&&this.total%320===0){if(Math.sqrt(this.energy/320)>.018)this.lastEnergy=this.total*2;this.energy=0;this.port.postMessage({activityBytes:this.lastEnergy,totalBytes:this.total*2});}
        if(this.frames.length===1600)this.flush();
      }
      this.previous=current;this.index++;
    }
    return true;
  }
}
registerProcessor('interaction-pcm',InteractionPCM);
