//! Conversational decisions select retained original input; the controller dispatches it.
use super::*;
use crate::interaction::{RequestRelation, ReturnRoute, SourceInput};
use serde::Deserialize;
use serde_json::{Value, json};

pub(super) fn tool_specs() -> Vec<Value> {
    let mut specs = super::observer::tool_specs();
    // The native tool set is persisted at thread creation. A stable envelope
    // discovers evolving operations without replacing the human's conversation.
    specs.push(json!({"type":"function","name":"interaction_controller","description":"Use capabilities with empty arguments to discover currently supported controller operations and input schemas. Inputs exposes retained original human message references; candidates inspects observer evidence; dispatch routes exact originals with provenance. Never invent human text or infer completion from admission. Unknown operations are unsupported; no global Stop operation exists.","inputSchema":{"type":"object","properties":{"operation":{"type":"string"},"arguments":{"type":"object"}},"required":["operation","arguments"],"additionalProperties":false}}));
    specs
}

fn dispatch_schema() -> Value {
    json!({"type":"object","properties":{"input_update":{"type":"integer"},"destinations":{"type":"array","minItems":1,"maxItems":5,"items":{"type":"object","properties":{"selector":{"type":"string"},"relation":{"type":"string","enum":["new","amends","context"]},"requests":{"type":"array","items":{"type":"string"},"maxItems":64}},"required":["selector","relation","requests"],"additionalProperties":false}}},"required":["input_update","destinations"],"additionalProperties":false})
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ControllerArgs {
    operation: String,
    arguments: Value,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DispatchArgs {
    input_update: i64,
    destinations: Vec<Destination>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Destination {
    selector: String,
    relation: String,
    requests: Vec<String>,
}

pub(super) fn tool_call(
    app: App,
    key: SessionKey,
    human: i64,
    mut request: crate::codex::DynamicToolRequest,
    closed: CancellationToken,
) -> oneshot::Receiver<Value> {
    if request.tool == "interaction_controller" && request.arguments.to_string().len() <= 16000 {
        if let Ok(args) = serde_json::from_value::<ControllerArgs>(request.arguments.clone()) {
            request.tool = match args.operation.as_str() {
                "capabilities" => "controller_capabilities",
                "inputs" => "interaction_inputs",
                "candidates" => "interaction_candidates",
                "dispatch" => "interaction_dispatch",
                "create" => "interaction_create",
                "creation_status" => "interaction_creation_status",
                "creation_resume" => "interaction_creation_resume",
                _ => "unsupported_controller_operation",
            }
            .into();
            request.arguments = args.arguments;
        }
    }
    if request.tool == "interaction_candidates" {
        return super::observer::tool_call(app.shared.clone(), key, human, request);
    }
    let (send, recv) = oneshot::channel();
    tokio::spawn(async move {
        let outcome:Result<Value>=async {
            let shared=&app.shared;
            if closed.is_cancelled() {bail!("Tool request closed before admission");}
            super::observer::tool_owner(shared,key,human,&request.thread_id)?;
            if request.namespace.is_some() || request.arguments.to_string().len()>16000 {bail!("Unsupported integrating tool arguments");}
            if request.tool=="controller_capabilities" && request.arguments==json!({}) {
                return Ok(json!({"schema":"telecodex.interaction-controller/v1","operations":{"capabilities":{"arguments":{}},"inputs":{"arguments":{},"description":"Recent original human text references within this native entry; no dispatch"},"candidates":{"description":"Bounded observer source interpretations; no dispatch","arguments":{"mode":"inventory or refresh","selectors":["one to five enrolled labels or stable tID; inventory can use an empty array"]}},"create":{"input_schema":super::creation::schema(),"description":"Create one justified new work session from an original retained input, with selected attributed context. Never create for ambiguity or a busy worker."},"creation_resume":{"arguments":{"input_update":"original input reference"},"description":"Explicitly continue a retained known-unsent creation or confirmed topic. Unknown external effects and completed plans are inspected without replay."},"creation_status":{"arguments":{"input_update":"original input reference"},"description":"Inspect a retained creation; no retry or new work"},"dispatch":{"input_schema":dispatch_schema(),"description":"One durable selected target plan, then existing per-target delivery. Text is taken only from the original referenced ingress record. Amendments/context require actual request IDs. Admission/native acceptance is not proof of incorporation."}},"unsupported":["cross-body routing","global Stop"],"uncertainty":"Never repeat an uncertain operation; inspect retained outcomes before explicit recovery"}));
            }
            if request.tool=="interaction_inputs" {
                if request.arguments!=json!({}) {bail!("Unexpected input query arguments");}
                let inputs=shared.store.interaction_entry_inputs(key,human,&request.thread_id,None)?;
                return Ok(json!({"kind":"original_integrating_inputs","bounded":true,"inputs":inputs.into_iter().filter_map(|u|{let m=u.message?;let text=m.text?;Some(json!({"input_update":u.update_id,"human":human,"origin":{"chat_id":m.chat.id,"thread_id":m.message_thread_id,"message_id":m.message_id},"text_excerpt":text.chars().take(2000).collect::<String>(),"excerpt_bounded":true}))}).collect::<Vec<_>>()}));
            }
            if request.tool=="interaction_create" {
                return super::creation::create(&app,key,human,&request.thread_id,request.arguments,&closed).await;
            }
            if matches!(request.tool.as_str(),"interaction_creation_status"|"interaction_creation_resume") {
                if request.arguments.as_object().is_none_or(|a|a.len()!=1) {bail!("Unexpected creation status arguments");}
                let update=request.arguments["input_update"].as_i64().context("Missing original input reference")?;
                if request.tool=="interaction_creation_resume" {
                    return super::creation::resume(&app,key,human,&request.thread_id,update,&closed).await;
                }
                let operation=shared.store.interaction_creation_for_input(key,human,update)?;
                return Ok(operation);
            }
            if request.tool!="interaction_dispatch" {bail!("Unknown integrating tool");}
            let args:DispatchArgs=serde_json::from_value(request.arguments)?;
            if args.input_update<0 || args.destinations.is_empty() || args.destinations.len()>5 {bail!("Invalid routing selection");}
            let original=shared.store.interaction_entry_inputs(key,human,&request.thread_id,Some(args.input_update))?.pop().context("Original input unavailable or retired")?;
            let message=original.message.context("Original human message missing")?;
            let text=message.text.as_deref().context("Committed text missing")?;
            if text.trim().is_empty() || text.starts_with('/') || !is_text_only_steer_candidate(&message,text) {bail!("This adapter requires original committed text");}
            let cfg=super::routing::profile(shared)?;
            let mut plan=vec![];
            for d in args.destinations {
                let matches=cfg.resolve(d.selector.strip_prefix('#').unwrap_or(&d.selector));
                if matches.len()!=1 || plan.iter().any(|(k,_,_)|*k==matches[0]) {bail!("Select distinct unambiguous enrolled destinations");}
                let target=matches[0];
                let observed=shared.store.interaction_observation(target,human)?.context("Destination ownership unavailable")?;
                if observed.native_thread.is_none() {bail!("Destination has no native source yet");}
                let relation=match d.relation.as_str() {
                    "new" if d.requests.is_empty()=>RequestRelation::New,
                    "amends" if !d.requests.is_empty() && d.requests.len()<=64=>RequestRelation::Amends(d.requests),
                    "context" if !d.requests.is_empty() && d.requests.len()<=64=>RequestRelation::Context(d.requests),
                    _=>bail!("Invalid request relation"),
                };
                plan.push((target,observed.session_id,relation));
            }
            plan.sort_by_key(|(key,_,_)|(key.chat_id,key.thread_id));
            let source=SourceInput{channel:format!("telegram:{}",shared.bot_id),event_id:args.input_update.to_string(),update_id:Some(args.input_update),human_id:human,target_sessions:plan.iter().map(|(_,id,_)|*id).collect(),origin:ReturnRoute{chat_id:message.chat.id,thread_id:message.message_thread_id.unwrap_or(0),message_id:message.message_id},text:text.to_owned()};
            if closed.is_cancelled() {bail!("Tool request closed before admission");}
            let selected=plan.into_iter().map(|(key,_,relation)|(key,relation)).collect::<Vec<_>>();
            let admitted=shared.store.interaction_admit_entry_plan(key,human,&request.thread_id,&source,&selected,&closed)?;
            // Once this durable plan is admitted, its deliveries are independent
            // of the front dialogue. Native closure cannot silently retract work.
            let mut results=vec![];
            for(i,fresh)in admitted {
                let error=if fresh {
                    HUMAN_UPDATE_ID.scope(args.input_update,super::routing::dispatch(&app,&i.id,human)).await.err().map(|_|"Dispatch unconfirmed; inspect the retained input; no automatic retry")
                }else{None};
                let current=shared.store.interaction_instruction(&i.id,human)?.context("Retained input unavailable")?;
                results.push(json!({"target":i.target,"instruction":i.id,"requests":current.requests,"phase":current.phase.as_str(),"new_admission":fresh,"dispatch_error":error,"incorporation":"Not established by controller admission or native acceptance; inspect worker coverage"}));
            }
            Ok(json!({"kind":"integrator_routing_outcomes","original_ingress_update":args.input_update,"original_human":human,"results":results}))
        }.await;
        let result=match outcome {Ok(value)=>crate::codex::dynamic_tool_result(true,value.to_string()),Err(_)=>crate::codex::dynamic_tool_result(false,"Routing unavailable for this original input, binding, generation or selection. No uncertain operation was retried. Inspect any retained per-target outcomes before recovery.".into())};
        let _ = send.send(result);
    });
    recv
}
