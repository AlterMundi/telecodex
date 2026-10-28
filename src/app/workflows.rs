//! Explicit integrator progression and source-only worker reads. No model wakeup.
use super::*;
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Step {
    id: String,
    selector: String,
    after: Option<String>,
    scope: String,
    checkpoint: String,
    milestone: Option<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Declare {
    request_id: String,
    input_ref: crate::interaction::InputReference,
    expires_at: String,
    steps: Vec<Step>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Selection {
    workflow: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Advance {
    workflow: String,
    revision: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Finish {
    workflow: String,
    revision: String,
    checkpoint: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Supersede {
    workflow: String,
    revision: String,
    input_ref: crate::interaction::InputReference,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Question {
    question: String,
    revision: String,
    #[serde(default)]
    offset: usize,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Part {
    workflow: String,
    step: String,
    revision: String,
    #[serde(default)]
    offset: usize,
}

pub(super) fn capabilities(value: &mut Value, integrating: bool) {
    value["operations"]["workflow_result"] = json!({"arguments":{"workflow":"workflow ID","step":"predecessor step ID","revision":"exact canonical source revision","offset":"optional character offset"},"description":"Read at most4000 characters of this assigned step's declared predecessor result; follow next_offset. Session-authored evidence, never human instructions or proof of integration."});
    if !integrating {
        return;
    }
    value["operations"]["workflow_declare"] = json!({"arguments":{"input_ref":"accepted original human reference","request_id":"stable identifier, max64 alphanumeric/_/-","expires_at":"finite RFC3339 UTC deadline, at most7days","steps":[{"id":"stable step ID","selector":"one existing enrolled owner","after":"previous step ID, or null for first","scope":"integrator-authored bounded step scope, max4000bytes","checkpoint":"expected receiving/result checkpoint, max512bytes","milestone":"optional human-selected reporting milestone, max80bytes"}]},"description":"Retain one immutable workflow of at most4 ordered steps from this accepted original. No native dispatch. Select only work already authorized by the original; workers cannot create workflows. Declaring a helper does not finish the integrating human task."});
    value["operations"]["workflow_question"] = json!({"arguments":{"question":"native question ID","revision":"exact capture_version from attention source","offset":"optional character offset"},"description":"Read bounded current workflow-native question evidence. Not an answer/approval API; use existing selected-human question controls."});
    value["operations"]["workflow_supersede"] = json!({"arguments":{"workflow":"workflow ID","revision":"exact workflow_status revision","input_ref":"later accepted human correction original"},"description":"Retire future steps under a later actual human original; preserve accepted and uncertain native effects. Does not cancel unrelated work or transfer ownership."});
    value["operations"]["workflow_status"] = json!({"arguments":{"workflow":"workflow ID"},"description":"Current instruction/coverage/readiness evidence. Final helper completion leaves awaiting_integration, never overall completion."});
    value["operations"]["workflow_advance"] = json!({"arguments":{"workflow":"workflow ID","revision":"exact workflow_status revision"},"description":"Explicitly admit one ready predeclared step through the existing worker dispatch. Never retransmits an existing queued/accepted/uncertain request. Inspect changed revisions before progressing."});
    value["operations"]["workflow_attention"] = json!({"arguments":{},"description":"Read bounded model-free source envelopes for current workflow results and reconciliation. No continuous polling or model wakeup. Read source text on demand."});
    value["operations"]["workflow_reported"] = json!({"arguments":{"workflow":"workflow ID","revision":"exact attention envelope revision"},"description":"Acknowledge an integrated offer. This is neither a hearing receipt, publication grant nor task completion."});
    value["operations"]["workflow_finish"] = json!({"arguments":{"workflow":"workflow ID","revision":"exact workflow_status revision","checkpoint":"integrator's final checkpoint, max2000bytes"},"description":"Retain final integration only after every required step is currently completed with cumulative source coverage. No automatic publication."});
}

pub(super) async fn tool(
    app: &App,
    key: SessionKey,
    human: i64,
    request: &crate::codex::DynamicToolRequest,
    operation: &str,
    arguments: Value,
    closed: &CancellationToken,
) -> Result<Value> {
    if closed.is_cancelled() {
        bail!("Workflow tool request closed");
    }
    let store = &app.shared.store;
    if operation == "workflow_result" {
        let args: Part = serde_json::from_value(arguments)?;
        return store.interaction_workflow_result(
            human,
            key,
            &request.thread_id,
            &args.workflow,
            &args.step,
            &args.revision,
            args.offset,
        );
    }
    super::observer::tool_owner(&app.shared, key, human, &request.thread_id)?;
    match operation {
        "workflow_declare" => {
            let args: Declare = serde_json::from_value(arguments)?;
            let cfg = super::routing::profile(&app.shared)?;
            let mut steps = vec![];
            for step in args.steps {
                let matches =
                    cfg.resolve(step.selector.strip_prefix('#').unwrap_or(&step.selector));
                if matches.len() != 1 {
                    bail!("Select one unambiguous enrolled workflow owner");
                }
                steps.push(crate::store::WorkflowStep {
                    id: step.id,
                    owner: matches[0],
                    after: step.after,
                    scope: step.scope,
                    checkpoint: step.checkpoint,
                    milestone: step.milestone,
                });
            }
            let declared = store.interaction_workflow_declare(
                human,
                key,
                &request.thread_id,
                &format!("telegram:{}", app.shared.bot_id),
                crate::store::WorkflowPlan {
                    request_id: args.request_id,
                    input_ref: args.input_ref,
                    expires_at: args.expires_at,
                    steps,
                },
                closed,
            );
            match declared {
                Err(error) if error.to_string() == crate::store::WORKFLOW_CAPACITY_ERROR => Ok(
                    json!({"kind":"workflow_capacity_reached","admitted":false,"max_retained_workflows":256,"max_document_bytes":262144,"recovery":"Retained history was preserved. Continue or inspect existing workflows; additional workflow admission requires an explicitly authorized compatible journal archival/retirement route. No automatic deletion or replay."}),
                ),
                other => other,
            }
        }
        "workflow_question" => {
            let args: Question = serde_json::from_value(arguments)?;
            store.interaction_workflow_question(
                human,
                key,
                &request.thread_id,
                &args.question,
                &args.revision,
                args.offset,
            )
        }
        "workflow_supersede" => {
            let args: Supersede = serde_json::from_value(arguments)?;
            store.interaction_workflow_supersede(
                human,
                key,
                &request.thread_id,
                &format!("telegram:{}", app.shared.bot_id),
                &args.workflow,
                &args.revision,
                args.input_ref,
            )
        }
        "workflow_status" => {
            let args: Selection = serde_json::from_value(arguments)?;
            store.interaction_workflow_status(human, key, &request.thread_id, &args.workflow)
        }
        "workflow_attention" if arguments == json!({}) => {
            store.interaction_workflow_attention(human, key)
        }
        "workflow_advance" => {
            let args: Advance = serde_json::from_value(arguments)?;
            let instruction = store.interaction_workflow_advance(
                human,
                key,
                &request.thread_id,
                &args.workflow,
                &args.revision,
                closed,
            )?;
            let mut outcome = None;
            if let Some(i) = instruction {
                let error = super::routing::dispatch(app, &i.id, human)
                    .await
                    .err()
                    .map(|_| "Dispatch unconfirmed; inspect retained instruction without replay");
                outcome = Some(
                    json!({"instruction":store.interaction_instruction(&i.id,human)?,"dispatch_error":error}),
                );
            }
            Ok(
                json!({"workflow":store.interaction_workflow_status(human,key,&request.thread_id,&args.workflow)?,"new_admission":outcome,"automatic_native_return":false,"meaning":"Retained workflow progression only. Native acceptance, result coverage, integration and human publication remain separate."}),
            )
        }
        "workflow_reported" => {
            let args: Advance = serde_json::from_value(arguments)?;
            store.interaction_workflow_reported(
                human,
                key,
                &request.thread_id,
                &args.workflow,
                &args.revision,
            )?;
            Ok(
                json!({"acknowledged_offer":args.revision,"hearing_receipt":false,"workflow_completed":false}),
            )
        }
        "workflow_finish" => {
            let args: Finish = serde_json::from_value(arguments)?;
            store.interaction_workflow_finish(
                human,
                key,
                &request.thread_id,
                &args.workflow,
                &args.revision,
                &args.checkpoint,
            )
        }
        _ => bail!("Unsupported workflow operation"),
    }
}

/// Event-driven progression stays with the integrating controller's retained
/// plan. A worker result cannot add steps or create a fresh human instruction.
pub(super) fn returned(
    app: App,
    delivery: &crate::interaction::Delivery,
    human: i64,
) -> Result<()> {
    if let Some(instruction) = app
        .shared
        .store
        .interaction_workflow_return(human, &delivery.id)?
    {
        let dispatch_app = app.clone();
        tokio::spawn(async move {
            let _ = super::routing::dispatch(&dispatch_app, &instruction.id, human).await;
        });
    }
    let event_app = app.clone();
    tokio::spawn(async move {
        let _ = attention(event_app, human).await;
    });
    Ok(())
}

/// Only committed meaningful workflow events reserve native returns. There is
/// no startup scan, timer, progress rewrite or peer attention loop here.
pub(super) fn attention(
    app: App,
    human: i64,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send>> {
    Box::pin(async move {
        let cfg = super::routing::profile(&app.shared)?;
        if cfg.human_id != human || !cfg.native_workflow_returns {
            return Ok(());
        }
        for selected in app
            .shared
            .store
            .workflow_native_reserve(human, cfg.entry())?
        {
            let id = selected.id.clone();
            if deliver(&app, selected).await.is_err() {
                // Reservation without any native boundary remains known unsent.
                app.shared
                    .store
                    .workflow_native_outcome(&id, "retained_unsent", None)?;
            }
        }
        Ok(())
    })
}
async fn deliver(app: &App, selected: crate::store::ReturnDispatch) -> Result<()> {
    let worker = app.worker_for(selected.entry).await?;
    let active = worker.steer.lock().expect("steer mutex poisoned").clone();
    let output = crate::codex::NativeToolOutput {
        name: "workflow_attention".into(),
        namespace: None,
        output: selected.output.clone(),
        retain: true,
    };
    if let Some(active) = active {
        app.shared
            .store
            .workflow_native_link(&selected.id, active.turn_id)?;
        app.shared.store.workflow_native_submitting(&selected.id)?;
        let id = selected.id.clone();
        let shared = app.shared.clone();
        let (send, receive) = oneshot::channel();
        let packet = crate::codex::CodexSteerRequest {
            tool_output: Some(output),
            observation: None,
            images: vec![],
            text: String::new(),
            client_message_id: Some(format!("workflow-return:{id}")),
            goal: None,
            admission_check: Some(Arc::new(move || shared.store.workflow_native_validate(&id))),
            response: send,
        };
        if active.sender.send(packet).is_err() {
            app.shared
                .store
                .workflow_native_outcome(&selected.id, "retained_unsent", None)?;
            return Ok(());
        }
        let shared = app.shared.clone();
        tokio::spawn(async move {
            let outcome = tokio::time::timeout(Duration::from_secs(10), receive).await;
            let (phase, turn) = match outcome {
                Ok(Ok(Ok(Some(turn)))) => ("accepted", Some(turn)),
                Ok(Ok(Err(crate::codex::CodexSteerError::Rejected(_)))) => ("rejected", None),
                _ => ("uncertain", None),
            };
            let _ = shared
                .store
                .workflow_native_outcome(&selected.id, phase, turn.as_deref());
        });
    } else {
        worker
            .sender
            .send(QueuedTurn {
                request: TurnRequest {
                    session_key: selected.entry,
                    from_user_id: selected.human,
                    prompt: String::new(),
                    runtime_instructions: None,
                    attachments: vec![],
                    review_mode: None,
                    override_search_mode: None,
                },
                chat_kind: "conversation".into(),
                update_id: None,
                conversation_input: None,
                workflow_return: Some(selected.id),
                tribu_admission: None,
                goal: None,
                nexus: None,
                merge: None,
                stop_resume: None,
                interaction: None,
                admission_epoch: selected.epoch,
            })
            .map_err(|_| anyhow!("Integrating worker unavailable"))?;
    }
    Ok(())
}
