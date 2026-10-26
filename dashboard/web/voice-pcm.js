/* Mono PCM16/16kHz capture. No inference, network, playback or retained history. */
class InteractionPCM extends AudioWorkletProcessor {
  constructor(){
    super();this.active=true;this.index=0;this.next=0;this.previous=0;this.ratio=sampleRate/16000;
    this.frames=[];this.port.onmessage=event=>{if(event.data==='finish'){this.active=false;this.flush();this.port.postMessage({finished:true});}};
  }
  flush(){
    if(!this.frames.length)return;
    const data=new ArrayBuffer(this.frames.length*2),view=new DataView(data);
    this.frames.forEach((value,index)=>view.setInt16(index*2,Math.round(Math.max(-1,Math.min(1,value))*32767),true));
    this.frames=[];this.port.postMessage({pcm:data},[data]);
  }
  process(inputs){
    if(!this.active)return false;
    const input=inputs[0]?.[0];if(!input)return true;
    for(const current of input){
      while(this.next<=this.index){
        const fraction=this.index===0?1:this.next-(this.index-1);
        this.frames.push(this.previous+(current-this.previous)*fraction);this.next+=this.ratio;
        if(this.frames.length===1600)this.flush();
      }
      this.previous=current;this.index++;
    }
    return true;
  }
}
registerProcessor('interaction-pcm',InteractionPCM);
