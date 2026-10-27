// Canonical source stays untouched; Markdown is a local presentation projection.
(() => {
  const parser=window.markdownit({html:false,linkify:false,typographer:false,maxNesting:20});
  const raw=new WeakMap();
  parser.renderer.rules.image=(tokens,index,options,env)=>parser.utils.escapeHtml(parser.renderer.renderInlineAsText(tokens[index].children || [],options,env));
  const link=parser.renderer.rules.link_open || ((tokens,index,options,env,self)=>self.renderToken(tokens,index,options));
  parser.renderer.rules.link_open=(tokens,index,options,env,self)=>{
    tokens[index].attrSet('target','_blank');tokens[index].attrSet('rel','noopener noreferrer');
    return link(tokens,index,options,env,self);
  };
  function html(text){
    const offsets=[0];let offset=0;
    for(const line of text.split('\n')){offset+=Array.from(line).length+1;offsets.push(offset);}
    const tokens=parser.parse(text,{});
    for(const token of tokens)if(token.map && token.type!=='inline'){
      token.attrSet('data-md-start',String(offsets[token.map[0]]));
      token.attrSet('data-md-end',String(Math.min(Array.from(text).length,offsets[token.map[1]])));
    }
    return parser.renderer.render(tokens,parser.options,{});
  }
  function render(element,text){
    raw.set(element,text);element.classList.add('markdown-text');element.innerHTML=html(text);
  }
  function source(element){return raw.get(element) ?? element.textContent;}
  function range(element,position){
    // Block source maps preserve the canonical offset despite hidden Markdown
    // punctuation/link targets. Within a block, progress remains approximate.
    let block,span=Infinity;
    for(const candidate of element.querySelectorAll('[data-md-start]')){
      const start=Number(candidate.dataset.mdStart),end=Number(candidate.dataset.mdEnd);
      if(start<=position && position<end && end-start<span){block=candidate;span=end-start;}
    }
    block=block || element;
    const start=Number(block.dataset.mdStart || 0),end=Number(block.dataset.mdEnd || Array.from(source(element)).length);
    const fraction=Math.max(0,Math.min(1,(position-start)/Math.max(1,end-start)));
    const walker=document.createTreeWalker(block,window.NodeFilter.SHOW_TEXT),nodes=[];let count=0,node;
    while((node=walker.nextNode())){const length=Array.from(node.textContent).length;if(length){nodes.push({node,length});count+=length;}}
    if(!count)return;
    let wanted=Math.min(count-1,Math.floor(fraction*count));
    for(const {node,length} of nodes){
      if(wanted>=length){wanted-=length;continue;}
      const chars=Array.from(node.textContent),at=chars.slice(0,wanted).join('').length,result=document.createRange();
      result.setStart(node,at);result.setEnd(node,at+chars[wanted].length);return result;
    }
  }
  window.ConversationMarkdown={html,render,source,range};
})();
