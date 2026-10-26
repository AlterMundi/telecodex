(async()=>{
 const vocabulary=__VOCAB__,rate=48000;
 async function render(keys,changes=[]){
  let ctx;
  class OfflineDevice{
   constructor(){ctx=this;this.audio=new OfflineAudioContext(1,12*rate,rate);this.destination=this.audio.destination;this.sampleRate=rate;this.state='running';}
   get currentTime(){return this.audio.currentTime;}
   createGain(){return this.audio.createGain();}createOscillator(){return this.audio.createOscillator();}
   async resume(){this.state='running';this.onstatechange?.();}async close(){this.state='closed';}
  }
  const engine=ConversationSound.renderer({Context:OfflineDevice});await engine.start();engine.setVolume(1);
  const composition=components=>({voices:components.length?[{id:'synthetic',components}]:[]});
  engine.render(composition(keys),vocabulary);
  for(const [at,next,quiet,ducking]of changes)ctx.audio.suspend(at).then(()=>{if(quiet!==undefined)engine.setQuiet(quiet);if(ducking!==undefined)engine.setDucking(ducking);engine.render(composition(next),vocabulary);return ctx.audio.resume();});
  const rendered=await ctx.audio.startRendering(),data=rendered.getChannelData(0);
  let peak=0,jump=0,tail=0,jumpAt=0;
  for(let i=0;i<data.length;i++){peak=Math.max(peak,Math.abs(data[i]));if(i && Math.abs(data[i]-data[i-1])>jump){jump=Math.abs(data[i]-data[i-1]);jumpAt=i/rate;}if(i>3*rate)tail=Math.max(tail,Math.abs(data[i]));}
  const amplitude=hz=>{let re=0,im=0,weight=0;const start=4*rate,n=8*rate;for(let i=0;i<n;i++){const w=.5-.5*Math.cos(2*Math.PI*i/(n-1)),p=2*Math.PI*hz*(i+start)/rate;re+=w*data[i+start]*Math.cos(p);im+=w*data[i+start]*Math.sin(p);weight+=w;}return 2*Math.hypot(re,im)/weight;};
  return {peak,jump,jumpAt,tail,amplitude};
 }
 const steady=[];
 for(const key of ['idle','active','pending']){
  const factor=vocabulary.components[key],carrier=vocabulary.fundamental_hz*factor,pulse=.25*factor,r=await render([key]);
  const signal={key,carrier,pulse,peak:r.peak,maxAdjacentDelta:r.jump,carrierAmplitude:r.amplitude(carrier),lowerSideband:r.amplitude(carrier-pulse),upperSideband:r.amplitude(carrier+pulse),unwantedSecondHarmonic:r.amplitude(carrier*2)};
  if(signal.peak>.60001||Math.abs(signal.carrierAmplitude-.45)>.004||Math.abs(signal.lowerSideband-.075)>.004||Math.abs(signal.upperSideband-.075)>.004||signal.unwantedSecondHarmonic>.001)throw Error('Unexpected synthesized spectrum '+JSON.stringify(signal));
  steady.push(signal);
 }
 const ducked=await render(['idle'],[[2,['idle'],undefined,.18]]);const duckedAmplitude=ducked.amplitude(40.4);if(Math.abs(duckedAmplitude-.081)>.003)throw Error('Reply ducking muted or distorted the Beacon');
 const transitions=await render(['idle','active','tools','goal','pending','blocked'],[[.08,['idle']],[.13,['idle','goal']],[.19,['idle','pending']],[.24,['idle','active','tools']],[.31,['idle','active']],[.37,['idle','blocked']],[.45,['idle','goal'],true],[.49,['idle','goal'],false],[.56,['idle'],true],[.61,['idle'],false],[.7,['idle'],undefined,.18],[.72,['idle'],undefined,1],[.74,['idle'],undefined,.18],[.76,['idle'],undefined,1],[1,[]]]);
 if(transitions.peak>.60001 || transitions.jump>.04 || transitions.tail>1e-7)throw Error('Unbounded transition '+JSON.stringify(transitions));
 return {offlineAudio:true,physicalPlayback:false,pcmRetained:false,sampleRate:rate,steady,replyDucking:{factor:.18,carrierAmplitude:duckedAmplitude},transitions:{peak:transitions.peak,maxAdjacentDelta:transitions.jump,settledTail:transitions.tail,jumpAt:transitions.jumpAt}};
})()
