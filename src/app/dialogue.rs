//! Conversational decisions select retained original input; the controller dispatches it.
use super::*;
use crate::interaction::{InputReference, RequestRelation};
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
    json!({"type":"object","properties":{"input_ref":{"oneOf":[{"type":"integer","minimum":0},{"type":"string","format":"uuid"}]},"destinations":{"type":"array","minItems":1,"maxItems":5,"items":{"type":"object","properties":{"selector":{"type":"string"},"relation":{"type":"string","enum":["new","amends","context"]},"requests":{"type":"array","items":{"type":"string"},"maxItems":64}},"required":["selector","relation","requests"],"additionalProperties":false}}},"required":["input_ref","destinations"],"additionalProperties":false})
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
    #[serde(alias = "input_update")]
    input_ref: InputReference,
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
    if request.tool == "interaction_controller" && super::routing::enrolled(&app.shared, key) {
        return super::session_requests::tool_call(app, key, human, request, closed);
    }
    if request.tool == "interaction_controller" && request.arguments.to_string().len() <= 16000 {
        if let Ok(args) = serde_json::from_value::<ControllerArgs>(request.arguments.clone()) {
            request.tool = match args.operation.as_str() {
                "capabilities" => "controller_capabilities",
                "inputs" => "interaction_inputs",
                "candidates" => "interaction_candidates",
                "dispatch" => "interaction_dispatch",
                "pending_routes" => "interaction_pending_routes",
                "resolve_route" => "interaction_resolve_route",
                "create" => "interaction_create",
                "interview" => "interaction_interview",
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
                return Ok(json!({"schema":"telecodex.interaction-controller/v1","operations":{"capabilities":{"arguments":{}},"inputs":{"arguments":{},"description":"Recent original human text references within this native entry; no dispatch"},"candidates":{"description":"Bounded observer source interpretations; no dispatch","arguments":{"mode":"inventory or refresh","selectors":["one to five enrolled labels or stable tID; inventory can use an empty array"]}},"create":{"input_schema":super::creation::schema(),"description":"Create one justified new work session from an original retained input, with selected attributed context. Never create for ambiguity or a busy worker."},"interview":{"input_schema":super::creation::schema(),"description":"Create an explicitly requested independent interview from one to five selected observer sources and their pinned original evidence. Busy sources continue; this is a derived context package, not an exact native fork or publication."},"creation_resume":{"arguments":{"input_ref":"original integer Telegram update or conversation UUID"},"description":"Explicitly continue a retained known-unsent creation or confirmed topic. Unknown external effects and completed plans are inspected without replay."},"creation_status":{"arguments":{"input_ref":"original integer Telegram update or conversation UUID"},"description":"Inspect a retained creation; no retry or new work"},"pending_routes":{"arguments":{},"description":"Read up to twelve current original inputs awaiting a destination; no route. Ask the human when the intended pending original or destination is unclear."},"resolve_route":{"arguments":{"pending_input":"UUID of one pending original","input_ref":"later retained human clarification reference","selector":"one unambiguous enrolled target"},"description":"Resolve the selected pending original using a later actual human utterance, including a committed transcript. The target is an integrator interpretation attributed to that clarification, not rewritten human speech. The original message and first selection remain immutable; retries inspect without replay."},"dispatch":{"input_schema":dispatch_schema(),"description":"One durable selected target plan, then existing per-target delivery. Text is taken only from the original referenced ingress record. Amendments/context require actual request IDs. Admission/native acceptance is not proof of incorporation."}},"unsupported":["cross-body routing","global Stop"],"uncertainty":"Never repeat an uncertain operation; inspect retained outcomes before explicit recovery"}));
            }
            if request.tool=="interaction_inputs" {
                if request.arguments!=json!({}) {bail!("Unexpected input query arguments");}
                let refs=shared.store.interaction_entry_references(key,human,&request.thread_id)?;
                let mut inputs=vec![];
                for reference in refs {
                    let (source,_)=shared.store.interaction_entry_original(key,human,&request.thread_id,&reference,&format!("telegram:{}",shared.bot_id))?;
                    inputs.push(json!({"capture_order":shared.store.interaction_source_order(&source)?,"input_ref":reference,"input_update":reference.update(),"human":human,"origin":source.origin,"channel":source.channel,"transcription":source.transcription,"text_excerpt":source.text.chars().take(2000).collect::<String>(),"excerpt_bounded":true}));
                }
                return Ok(json!({"kind":"original_integrating_inputs","bounded":true,"inputs":inputs}));
            }
            if request.tool=="interaction_pending_routes" {
                if request.arguments!=json!({}) {bail!("Unexpected pending-route query arguments");}
                return shared.store.conversation_pending_routes(human,key,&request.thread_id);
            }
            if request.tool=="interaction_resolve_route" {
                return super::conversation_inputs::clarify(&app,human,&request.thread_id,request.arguments,&closed).await;
            }
            if request.tool=="interaction_interview" {
                return super::creation::interview(&app,key,human,&request.thread_id,request.arguments,&closed).await;
            }
            if request.tool=="interaction_create" {
                return super::creation::create(&app,key,human,&request.thread_id,request.arguments,&closed).await;
            }
            if matches!(request.tool.as_str(),"interaction_creation_status"|"interaction_creation_resume") {
                if request.arguments.as_object().is_none_or(|a|a.len()!=1) {bail!("Unexpected creation status arguments");}
                let reference:InputReference=serde_json::from_value(request.arguments.get("input_ref").or_else(||request.arguments.get("input_update")).context("Missing original input reference")?.clone())?;
                reference.validate()?;
                if request.tool=="interaction_creation_resume" {
                    return super::creation::resume(&app,key,human,&request.thread_id,reference,&closed).await;
                }
                let operation=shared.store.interaction_creation_for_input(key,human,reference)?;
                return Ok(operation);
            }
            if request.tool!="interaction_dispatch" {bail!("Unknown integrating tool");}
            let args:DispatchArgs=serde_json::from_value(request.arguments)?;
            args.input_ref.validate()?;
            if args.destinations.is_empty() || args.destinations.len()>5 {bail!("Invalid routing selection");}
            let (mut source,message)=shared.store.interaction_entry_original(key,human,&request.thread_id,&args.input_ref,&format!("telegram:{}",shared.bot_id))?;
            if source.text.trim().is_empty() || source.text.starts_with('/') || message.as_ref().is_some_and(|message|!(is_text_only_steer_candidate(message,&source.text) || source.transcription.is_some())) {bail!("This adapter requires original committed text");}
            let cfg=super::routing::profile(shared)?;
            let mut plan=vec![];
            for d in args.destinations {
                let matches=cfg.resolve(d.selector.strip_prefix('#').unwrap_or(&d.selector));
                if matches.len()!=1 || plan.iter().any(|(k,_,_)|*k==matches[0]) {bail!("Select distinct unambiguous enrolled destinations");}
                let target=matches[0];
                let observed=shared.store.interaction_observation(target,human)?.context("Destination ownership unavailable")?;
                // The owning store also admits a freshly created owned session
                // while its first native writer is still preparing.
                let relation=match d.relation.as_str() {
                    "new" if d.requests.is_empty()=>RequestRelation::New,
                    "amends" if !d.requests.is_empty() && d.requests.len()<=64=>RequestRelation::Amends(d.requests),
                    "context" if !d.requests.is_empty() && d.requests.len()<=64=>RequestRelation::Context(d.requests),
                    _=>bail!("Invalid request relation"),
                };
                plan.push((target,observed.session_id,relation));
            }
            plan.sort_by_key(|(key,_,_)|(key.chat_id,key.thread_id));
            source.target_sessions=plan.iter().map(|(_,id,_)|*id).collect();
            if closed.is_cancelled() {bail!("Tool request closed before admission");}
            let selected=plan.into_iter().map(|(key,_,relation)|(key,relation)).collect::<Vec<_>>();
            let admitted=shared.store.interaction_admit_entry_plan(key,human,&request.thread_id,&source,&selected,&closed)?;
            // Once this durable plan is admitted, its deliveries are independent
            // of the front dialogue. Native closure cannot silently retract work.
            let mut results=vec![];
            for(i,fresh)in admitted {
                let error=if fresh {
                    dispatch_original(&app,&i.id,human,&args.input_ref).await.err().map(|_|"Dispatch unconfirmed; inspect the retained input; no automatic retry")
                }else{None};
                let current=shared.store.interaction_instruction(&i.id,human)?.context("Retained input unavailable")?;
                results.push(json!({"target":i.target,"instruction":i.id,"requests":current.requests,"phase":current.phase.as_str(),"new_admission":fresh,"dispatch_error":error,"incorporation":"Not established by controller admission or native acceptance; inspect worker coverage"}));
            }
            Ok(json!({"kind":"integrator_routing_outcomes","input_ref":args.input_ref,"original_ingress_update":args.input_ref.update(),"original_human":human,"results":results}))
        }.await;
        let result=match outcome {Ok(value)=>crate::codex::dynamic_tool_result(true,value.to_string()),Err(_)=>crate::codex::dynamic_tool_result(false,"Routing unavailable for this original input, binding, generation or selection. No uncertain operation was retried. Inspect any retained per-target outcomes before recovery.".into())};
        let _ = send.send(result);
    });
    recv
}

/// Reuse Telegram task-local attribution only for an actual Telegram update.
pub(super) async fn dispatch_original(
    app: &App,
    id: &str,
    human: i64,
    reference: &InputReference,
) -> Result<()> {
    match reference.update() {
        Some(update) => {
            HUMAN_UPDATE_ID
                .scope(update, super::routing::dispatch(app, id, human))
                .await
        }
        None => super::routing::dispatch(app, id, human).await,
    }
}
