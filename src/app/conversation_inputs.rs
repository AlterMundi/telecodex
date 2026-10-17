//! Authenticated conversation ingress into the existing native worker and steer.
use super::*;
use crate::store::{ConversationInput, ConversationInputEvent as E};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Submit {
    binding: String,
    id: String,
    text: String,
}

fn outcome(input: &ConversationInput, fresh: bool) -> Value {
    json!({"schema":"telecodex.conversation-input/v1","id":input.id,"phase":input.phase,
        "new_input":fresh,"retired":input.retired,"native_acceptance":if input.phase=="accepted" {"observed"}else{"unconfirmed"},
        "incorporation":"Not established by input retention or native acceptance"})
}

pub(super) async fn command(app: &App, human: i64, args: &str) -> Result<Value> {
    let cfg = app
        .shared
        .config
        .interaction
        .as_ref()
        .context("Missing integrating entry")?;
    if cfg.human_id != human {
        bail!("Conversation input owner mismatch");
    }
    if let Some(id) = args.strip_prefix("status ") {
        return Ok(outcome(
            &app.shared
                .store
                .conversation_input_read(human, cfg.entry(), id)?,
            false,
        ));
    }
    let request: Submit = serde_json::from_str(
        args.strip_prefix("submit ")
            .context("Expected input submit JSON or status UUID")?,
    )?;
    // Preserve capture/handoff order for this configured integrating entry. The
    // lock is released before native acknowledgments, so later steering proceeds.
    let _ingress = app.shared.conversation_ingress.lock().await;
    let (input, fresh) = app.shared.store.conversation_input_capture(
        human,
        cfg.entry(),
        &request.binding,
        &request.id,
        &request.text,
    )?;
    if fresh && input.phase == "retained" {
        app.shared
            .store
            .conversation_input_event(human, &input.id, E::Queue)?;
        if let Err(error) = submit(app.clone(), input.clone()).await {
            app.shared
                .store
                .conversation_input_finish(human, &input.id)?;
            return Err(error);
        }
    }
    Ok(outcome(
        &app.shared
            .store
            .conversation_input_read(human, cfg.entry(), &input.id)?,
        fresh,
    ))
}

async fn submit(app: App, input: ConversationInput) -> Result<()> {
    let handle = app.worker_for(input.entry).await?;
    let active = handle.steer.lock().expect("steer mutex poisoned").clone();
    let Some(active) = active else {
        return enqueue(&app, &input, handle.sender);
    };
    app.shared.store.conversation_input_event(
        input.human,
        &input.id,
        E::Begin {
            local_turn: active.turn_id,
            steer: true,
        },
    )?;
    let (send, receive) = oneshot::channel();
    let shared = app.shared.clone();
    let id = input.id.clone();
    let human = input.human;
    let packet = CodexSteerRequest {
        admission_check: Some(Arc::new(move || {
            shared.store.conversation_input_validate(human, &id)
        })),
        client_message_id: Some(format!("conversation:{}", input.id)),
        text: input.text.clone(),
        goal: None,
        response: send,
    };
    if active.sender.send(packet).is_err() {
        app.shared.store.conversation_input_event(
            human,
            &input.id,
            E::Rejected {
                local_turn: active.turn_id,
            },
        )?;
        app.shared
            .store
            .conversation_input_event(human, &input.id, E::Queue)?;
        return enqueue(&app, &input, handle.sender);
    }
    tokio::spawn(async move {
        let result: Result<()> = async {
            match tokio::time::timeout(Duration::from_secs(10), receive).await {
                Ok(Ok(Ok(_))) => {
                    app.shared.store.conversation_input_event(
                        human,
                        &input.id,
                        E::Accepted {
                            local_turn: active.turn_id,
                        },
                    )?;
                }
                Ok(Ok(Err(CodexSteerError::Rejected(_)))) => {
                    app.shared.store.conversation_input_event(
                        human,
                        &input.id,
                        E::Rejected {
                            local_turn: active.turn_id,
                        },
                    )?;
                    app.shared
                        .store
                        .conversation_input_event(human, &input.id, E::Queue)?;
                    enqueue(&app, &input, handle.sender)?;
                }
                _ => {
                    app.shared.store.conversation_input_event(
                        human,
                        &input.id,
                        E::Uncertain {
                            local_turn: active.turn_id,
                        },
                    )?;
                }
            }
            Ok(())
        }
        .await;
        if result.is_err() {
            let _ = app.shared.store.conversation_input_finish(human, &input.id);
            tracing::warn!(
                "Conversation input outcome retained without confirmed submission; no automatic replay"
            );
        }
    });
    Ok(())
}

fn enqueue(
    app: &App,
    input: &ConversationInput,
    sender: mpsc::UnboundedSender<QueuedTurn>,
) -> Result<()> {
    app.shared
        .store
        .conversation_input_validate(input.human, &input.id)?;
    sender
        .send(QueuedTurn {
            request: TurnRequest {
                session_key: input.entry,
                from_user_id: input.human,
                prompt: input.text.clone(),
                runtime_instructions: None,
                attachments: vec![],
                review_mode: None,
                override_search_mode: None,
            },
            chat_kind: "conversation".into(),
            update_id: None,
            conversation_input: Some(input.id.clone()),
            tribu_admission: None,
            goal: None,
            nexus: None,
            merge: None,
            stop_resume: None,
            interaction: None,
            admission_epoch: input.epoch.clone(),
        })
        .map_err(|_| anyhow!("Conversation worker unavailable"))?;
    Ok(())
}
