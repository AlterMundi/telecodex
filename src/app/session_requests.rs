//! Nonblocking local dependencies: native workers never hold a tool response
//! waiting for another capacity slot. Canonical results return as attributed context.
use super::*;
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    operation: String,
    arguments: Value,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SendArgs {
    parent_instruction: String,
    request_id: String,
    selector: String,
    text: String,
}

pub(super) fn tool_call(
    app: App,
    key: SessionKey,
    human: i64,
    request: crate::codex::DynamicToolRequest,
    closed: CancellationToken,
) -> oneshot::Receiver<Value> {
    let (send, recv) = oneshot::channel();
    tokio::spawn(async move {
        let result:Result<Value>=async {
            let cfg=super::routing::profile(&app.shared)?;
            if cfg.human_id!=human || cfg.target(key.thread_id)!=Some(key) || cfg.chat_id!=key.chat_id || request.namespace.is_some() || request.tool!="interaction_controller" || request.arguments.to_string().len()>20000 || closed.is_cancelled(){bail!("Unsupported local session tool");}
            let owned=app.shared.store.interaction_observation(key,human)?.context("Caller ownership unavailable")?;
            if owned.native_thread.as_deref()!=Some(&request.thread_id){bail!("Caller native binding changed");}
            let args:Envelope=serde_json::from_value(request.arguments.clone())?;
            match args.operation.as_str() {
                "capabilities" if args.arguments==json!({})=>{let mut capabilities=json!({"schema":"telecodex.local-session/v1","operations":{"capabilities":{},"inputs":{},"companion_hint":super::presentation::companion_capability(&app)?,"session_requests":{},"session_receive":{"arguments":{"request_id":"retained local request id; select existing canonical result without rerunning helper"}},"session_request":{"arguments":{"parent_instruction":"accepted original human instruction in this execution","request_id":"stable caller-chosen alphanumeric/_/- identifier; never change to hide uncertainty","selector":"one unambiguous enrolled name or tID","text":"selected session-authored question/context only, at most16000bytes"}}},"targets":cfg.targets.iter().map(|t|json!({"name":t.name,"stable_selector":format!("t{}",t.topic_id)})).collect::<Vec<_>>(),"policy":"One dependency level, at most four calls per human instruction. An interview cannot send session-authored requests to its pinned source work; select a separate explicit human instruction instead. Acknowledge and continue other work or end this turn with deferred work; do not poll or wait in a tool loop. A current worker-declared completed result returns as context to this logical session. Native transport is not proof of incorporation. No new-session creation, human impersonation or cross-body operation."});super::interactions::task_capabilities(&mut capabilities);if app.shared.store.goal_delegation_allows_actor(human,key,&request.thread_id)? {super::goal_controls::delegated_capabilities(&mut capabilities);}Ok(capabilities)},
                "human_tasks"|"human_task"|"withdraw_task"=>super::interactions::live_task_tool(&app,key,human,&request,&args.operation,args.arguments,&closed).await,
                "companion_hint"=>super::presentation::companion_hint(&app,key,human,&request,args.arguments,&closed).await,
                "inputs" if args.arguments==json!({})=>{
                    let inputs=super::routing::session_inputs(&app.shared,key,human,&request.thread_id,&request.turn_id)?;
                    Ok(json!({"inputs":inputs.into_iter().map(|i|{let author=app.shared.store.interaction_author(&i.id,human)?;Ok(json!({"instruction":i.id,"requests":i.requests,"author":author,"eligible_human_parent":author.is_none(),"text_excerpt":i.text.chars().take(2000).collect::<String>(),"excerpt_bounded":true}))}).collect::<Result<Vec<_>>>()?}))
                }
                "session_requests" if args.arguments==json!({})=>Ok(json!({"requests":app.shared.store.interaction_session_requests(key,human,&request.thread_id)?,"recovery":"Inspect retained input phases. Only explicit known-unsent input recovery can retry; never replay uncertain admission."})),
                "session_receive"=>{
                    let object=args.arguments.as_object().context("Invalid retained selection")?;
                    if object.len()!=1 {bail!("Invalid retained selection");}
                    let id=object.get("request_id").and_then(Value::as_str).context("Missing retained request id")?;
                    let selected=app.shared.store.interaction_session_recover(key,human,&request.thread_id,id,&closed)?;
                    let admitted=selected.as_ref().map(|i|i.id.clone());
                    if let Some(i)=selected {let _=super::routing::dispatch(&app,&i.id,human).await;}
                    Ok(json!({"request":app.shared.store.interaction_session_selected(key,human,&request.thread_id,id)?,"new_return_instruction":admitted,"requests":app.shared.store.interaction_session_requests(key,human,&request.thread_id)?,"policy":"Only a retained completed canonical result was selected. Existing returns are inspection-only, including uncertain transmission. No helper work was replayed."}))
                }
                "session_request"=>{
                    let args:SendArgs=serde_json::from_value(args.arguments)?;
                    let selector=args.selector.strip_prefix("/self#").or_else(||args.selector.strip_prefix('#')).unwrap_or(&args.selector);
                    let targets=cfg.resolve(selector);
                    if targets.len()!=1{ return Ok(json!({"kind":"clarification_required","selector":args.selector,"candidates":targets,"admitted":false})); }
                    // Validate the actual native execution as well as the durable
                    // parent. Caller-selected text never becomes human authorship.
                    let inputs=super::routing::session_inputs(&app.shared,key,human,&request.thread_id,&request.turn_id)?;
                    if !inputs.iter().any(|i|i.id==args.parent_instruction){bail!("Parent is not in this active execution");}
                    let (call,i,fresh)=app.shared.store.interaction_session_request(key,human,&request.thread_id,&request.turn_id,&args.parent_instruction,&args.request_id,targets[0],&args.text,&closed)?;
                    let error=if fresh {super::routing::dispatch(&app,&i.id,human).await.err().map(|_|"Dispatch unconfirmed; inspect retained instruction without replay")}else{None};
                    let instruction=app.shared.store.interaction_instruction(&i.id,human)?;
                    Ok(json!({"kind":"local_session_request","request":call,"instruction":instruction,"new_admission":fresh,"dispatch_error":error,"waiting":"Nonblocking admission. Continue other work or finish this turn with the dependency deferred. Its canonical current completed result will return as session-authored context; do not poll."}))
                }
                _=>bail!("Unsupported local worker operation"),
            }
        }.await;
        let value=match result{Ok(v)=>crate::codex::dynamic_tool_result(true,v.to_string()),Err(_)=>crate::codex::dynamic_tool_result(false,"Local session request unavailable for this owner, active parent, generation, selection or dependency budget. No uncertain request was replayed.".into())};
        let _ = send.send(value);
    });
    recv
}

pub(super) fn returned(
    app: App,
    delivery: &crate::interaction::Delivery,
    human: i64,
) -> Result<()> {
    if let Some(i) = app
        .shared
        .store
        .interaction_session_return(&delivery.id, human)?
    {
        tokio::spawn(async move {
            let _ = super::routing::dispatch(&app, &i.id, human).await;
        });
    }
    Ok(())
}
