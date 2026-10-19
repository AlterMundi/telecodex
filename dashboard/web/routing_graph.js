"use strict";
window.RoutingGraph = (() => {
  const preview = value => Array.from(String(value || "")).slice(0,300).join("");
  class Timeline {
    constructor() { this.reset(); }
    reset() { this.nodes=new Map();this.edges=new Map();this.lanes=new Map([["integrator","Your conversation"]]);this.version=0; }
    put(id,value) {
      const previous=this.nodes.get(id), next={...previous,...value,id,index:previous?.index??this.nodes.size};
      next.text=preview(next.text);
      if(!this.lanes.has(next.lane))this.lanes.set(next.lane,next.laneLabel||next.lane);
      if(JSON.stringify(previous)!==JSON.stringify(next)){this.nodes.set(id,next);this.version++;if(previous?.lane.startsWith("unlinked:") && previous.lane!==next.lane && ![...this.nodes.values()].some(node=>node.lane===previous.lane))this.lanes.delete(previous.lane);}
      return next;
    }
    edge(from,to,label) {
      const key=JSON.stringify([from,to,label]);
      if(!this.edges.has(key)){this.edges.set(key,{from,to,label});this.version++;}
    }
    input(value) {
      const id="input:"+value.id,decision="decision:"+value.id;
      const voice=!!value.voice;
      this.put(id,{lane:"integrator",kind:voice?"voice":"input",title:(value.origin_kind==="telegram"?"Telegram · ":"")+(voice?"Voice transcript":"Text input"),text:value.text||value.preview||"Waiting for transcription",status:value.phase||"Saved",inspect:{input:value.id}});
      this.put(decision,{lane:"integrator",kind:"decision",title:"Routing",text:value.phase==="preparing"?"Waiting for a confirmed transcript":value.phase==="awaiting_target"?"Destination needs clarification":value.phase==="answered"?"Answer recorded for a session question":"Input retained by the integrating conversation",status:value.phase||"Saved",inspect:{input:value.id}});
      this.edge(id,decision,"input");
    }
    trace(value) {
      if(!this.nodes.has("input:"+value.id))this.input(value);
      for(const route of value.routes||[]) {
        const id="route:"+route.instruction;
        const lane=route.target?"session:"+JSON.stringify(route.target):"session-label:"+route.label;
        const relation={new:"New task",amends:"Correction",context:"Context"}[route.relation?.kind]||"Routed input";
        this.put(id,{lane,laneLabel:"#"+route.label,kind:"route",title:relation+" → #"+route.label,text:route.forwarded_original,status:route.transport_phase||"Retained route",inspect:{input:value.id}});
        this.edge("decision:"+value.id,id,"routed");
        for(const result of [...(route.results||[])].reverse()) {
          this.result({id:result.source,label:"#"+route.label,kind:"worker_result",preview:result.text_excerpt,...result},lane);
          // A related request can expose a result from BEFORE this correction.
          // Only declared coverage is an incorporation edge for this input.
          if(result.instruction_covered===true && result.declared===true)this.edge(id,result.source,"incorporated");
        }
      }
    }
    result(source,lane) {
      const existing=this.nodes.get(source.id), integrator=source.kind==="integrator_answer";
      const selectedLane=lane||(source.target?"session:"+JSON.stringify(source.target):undefined)||existing?.lane||(integrator?"integrator":"unlinked:"+source.id);
      const status=integrator?"Exact input link not retained":source.correction_pending?"Later correction not covered":source.current===false?"Earlier or uncovered result":source.completed?"Session reports completion":"Session progress";
      this.put(source.id,{lane:selectedLane,laneLabel:source.label||"Session",kind:"response",title:integrator?"Integrator reply":source.label||"Session reply",text:source.preview??source.text_excerpt,status,inspect:{source:source.id}});
    }
  }
  function mount({inspect}) {
    const section=document.querySelector("#routing-graph");if(!section)return;
    const viewport=document.querySelector("#routing-graph-viewport"),surface=document.querySelector("#routing-graph-surface");
    const live=document.querySelector("#routing-graph-live"),state=document.querySelector("#routing-graph-state");
    const model=new Timeline();let following=true,rendered=-1;
    const pendingInputs=new Map(),pendingTraces=new Map(),pendingSources=new Map();
    const column=330,row=280,width=292,height=248;
    section.hidden=false;
    const element=(tag,text,cls)=>{const n=document.createElement(tag);if(text!==undefined)n.textContent=text;if(cls)n.className=cls;return n;};
    function pause(){following=false;live.hidden=false;state.textContent="Browsing history · Return to live to follow new interactions.";}
    for(const event of ["wheel","pointerdown","touchstart"])viewport.addEventListener(event,pause,{passive:true});
    viewport.addEventListener("keydown",event=>{if(["ArrowLeft","ArrowRight","ArrowUp","ArrowDown","Home","End","PageUp","PageDown"].includes(event.key))pause();});
    let scrollFrame;
    viewport.addEventListener("scroll",()=>{if(scrollFrame)return;scrollFrame=requestAnimationFrame(()=>{scrollFrame=undefined;render(false);});});
    window.addEventListener("resize",()=>render(false));
    live.addEventListener("click",()=>{following=true;live.hidden=true;render(true);});
    function render(follow=false) {
      const focused=surface.contains(document.activeElement)?document.activeElement?.dataset?.nodeId:undefined;
      const lanes=[...model.lanes.keys()],nodes=[...model.nodes.values()];
      surface.style.width=Math.max(viewport.clientWidth,model.nodes.size*column+40)+"px";
      surface.style.height=Math.max(320,lanes.length*row)+"px";
      if(follow && following && nodes.length){
        const latest=nodes.at(-1);viewport.scrollLeft=Math.max(0,(latest.index+1)*column-viewport.clientWidth+30);
        const y=lanes.indexOf(latest.lane)*row;viewport.scrollTop=Math.max(0,y-viewport.clientHeight/2+row/2);
      }
      const min=viewport.scrollLeft-column,max=viewport.scrollLeft+viewport.clientWidth+column;
      const visible=nodes.filter(n=>n.index*column>=min && n.index*column<=max);
      const svg=document.createElementNS("http://www.w3.org/2000/svg","svg");svg.classList.add("routing-edges");svg.setAttribute("aria-hidden","true");
      svg.setAttribute("width",Math.max(viewport.clientWidth,model.nodes.size*column+40));svg.setAttribute("height",Math.max(320,lanes.length*row));
      const defs=document.createElementNS(svg.namespaceURI,"defs"),marker=document.createElementNS(svg.namespaceURI,"marker");
      marker.setAttribute("id","routing-edge-arrow");marker.setAttribute("viewBox","0 0 8 8");marker.setAttribute("refX","8");marker.setAttribute("refY","4");marker.setAttribute("markerWidth","7");marker.setAttribute("markerHeight","7");marker.setAttribute("orient","auto");
      const arrow=document.createElementNS(svg.namespaceURI,"path");arrow.setAttribute("d","M0 0 L8 4 L0 8 Z");arrow.setAttribute("class","routing-arrow");marker.append(arrow);defs.append(marker);svg.append(defs);
      const visibleIds=new Set(visible.map(n=>n.id));
      for(const edge of model.edges.values()) {
        if(!visibleIds.has(edge.from)&&!visibleIds.has(edge.to))continue;
        const from=model.nodes.get(edge.from),to=model.nodes.get(edge.to);if(!from||!to)continue;
        const x1=from.index*column+width+20,y1=lanes.indexOf(from.lane)*row+height/2+24;
        const x2=to.index*column+20,y2=lanes.indexOf(to.lane)*row+height/2+24;
        const path=document.createElementNS(svg.namespaceURI,"path");
        path.setAttribute("d",`M ${x1} ${y1} C ${x1+50} ${y1}, ${x2-50} ${y2}, ${x2} ${y2}`);path.setAttribute("class","routing-edge "+edge.label);path.setAttribute("marker-end","url(#routing-edge-arrow)");
        const title=document.createElementNS(svg.namespaceURI,"title");title.textContent=edge.label;path.append(title);svg.append(path);
      }
      const items=[svg];
      for(const value of visible) {
        const card=element("button",undefined,"routing-node routing-"+value.kind);card.type="button";card.dataset.nodeId=value.id;card.title=value.text;
        card.style.left=(value.index*column+20)+"px";card.style.top=(lanes.indexOf(value.lane)*row+24)+"px";
        card.append(element("span",value.title,"routing-node-title"),element("span",value.text,"routing-node-text"),element("span",value.status,"routing-node-status"));
        card.setAttribute("aria-label",value.title+". "+value.status+". Inspect retained evidence");
        card.addEventListener("click",()=>{pause();inspect(value.inspect);});items.push(card);
      }
      lanes.forEach((key,index)=>{const label=element("div",model.lanes.get(key),"routing-lane-label");label.style.top=(index*row)+"px";label.style.left=viewport.scrollLeft+"px";items.push(label);});
      surface.replaceChildren(...items);rendered=model.version;
      if(focused)(items.find(item=>item.dataset?.nodeId===focused)||viewport).focus({preventScroll:true});
      if(following)state.textContent=nodes.length?`${nodes.length} observed steps · following live. Initial history is reconstructed; later steps append when observed.`:"Waiting for retained inputs and replies.";
    }
    function flush() {
      const tracedSources=new Set([...pendingTraces.values()].flatMap(value=>(value.routes||[]).flatMap(route=>(route.results||[]).map(result=>result.source))));
      const inputs=[...pendingInputs.values()].sort((a,b)=>(a.capture_order??a.sequence??0)-(b.capture_order??b.sequence??0));
      for(const value of inputs){model.input(value);const trace=pendingTraces.get(value.id);if(trace){model.trace(trace);pendingTraces.delete(value.id);}}
      for(const value of pendingTraces.values())model.trace(value);
      for(const value of pendingSources.values())if(!tracedSources.has(value.id))model.result(value);
      pendingInputs.clear();pendingTraces.clear();pendingSources.clear();
      if(model.version!==rendered)render(true);
    }
    function reset(){model.reset();pendingInputs.clear();pendingTraces.clear();pendingSources.clear();following=true;live.hidden=true;rendered=-1;viewport.scrollLeft=viewport.scrollTop=0;render();}
    render();
    return {input:value=>pendingInputs.set(value.id,value),trace:value=>pendingTraces.set(value.id,value),result:value=>pendingSources.set(value.id,value),flush,reset};
  }
  return {Timeline,mount,preview};
})();
