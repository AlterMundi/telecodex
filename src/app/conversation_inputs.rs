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
    #[serde(default)]
    reply: Option<crate::store::ConversationReply>,
}

fn outcome(input: &ConversationInput, fresh: bool) -> Value {
    json!({"schema":"telecodex.conversation-input/v1","id":input.id,"phase":input.phase,
        "new_input":fresh,"retired":input.retired,"text":input.text,"voice":input.voice,"routing":input.routing,
        "entry_binding":[input.human,input.entry,input.session,input.native],"native_acceptance":if input.phase=="accepted" {"observed"}else{"unconfirmed"},
        "incorporation":"Not established by input retention or native acceptance"})
}

fn response(app: &App, input: &ConversationInput, fresh: bool) -> Result<Value> {
    let mut value = outcome(input, fresh);
    value["clarifiable"] = json!(
        input.phase == "awaiting_target"
            && app
                .shared
                .store
                .conversation_input_can_clarify(input.human, &input.id)?
    );
    if value["clarifiable"] == true {
        value["routing_choices"] = choices(app)?;
        value["binding"] = app
            .shared
            .store
            .conversation_request_binding(input.human, input.entry)?["binding"]
            .clone();
    }
    if input.phase == "answered" {
        if let Some(doc) =
            app.shared
                .store
                .conversation_answer_receipt(input.human, input.entry, &input.id)?
        {
            value["decision"] = json!({"id":doc["id"],"revision":doc["revision"],"recorded":true,"source_input":input.id,"recorded_at":doc["recorded_at"],"native_incorporation":"Not established by recording an answer"});
        }
    }
    value["continuation"] = json!(app.shared.store.conversation_continuation(
        input.human,
        input.entry,
        &input.id
    )?);
    if input
        .routing
        .as_ref()
        .is_some_and(|r| r.phase == "accepted")
    {
        value["native_acceptance"] = json!("observed");
    }
    Ok(value)
}
fn choices(app: &App) -> Result<Value> {
    let profile = super::routing::profile(&app.shared)?;
    Ok(json!(
        profile
            .targets
            .iter()
            .map(|t| json!({"selector":format!("t{}",t.topic_id),"label":t.name}))
            .collect::<Vec<_>>()
    ))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Resolve {
    binding: String,
    id: String,
    selector: String,
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
    if args == "voice capabilities" {
        let mut result = app
            .shared
            .store
            .conversation_request_binding(human, cfg.entry())?;
        result["voice_available"] = json!(app.shared.config.voice.is_some());
        result["max_bytes"] = json!(crate::voice::MAX_BYTES);
        result["max_seconds"] = json!(120);
        return Ok(result);
    }
    if let Some(body) = args.strip_prefix("voice submit ") {
        return voice_submit(app, human, cfg.entry(), body).await;
    }
    if let Some(body) = args.strip_prefix("route ") {
        let selected: Resolve = serde_json::from_str(body)?;
        let _ingress = app.shared.conversation_ingress.lock().await;
        let binding = app
            .shared
            .store
            .conversation_request_binding(human, cfg.entry())?;
        if binding["binding"] != selected.binding {
            bail!("Entry changed before routing selection");
        }
        let input = app
            .shared
            .store
            .conversation_input_read(human, cfg.entry(), &selected.id)?;
        deliver(app, input, Some(&selected.selector)).await?;
        return response(
            app,
            &app.shared
                .store
                .conversation_input_read(human, cfg.entry(), &selected.id)?,
            false,
        );
    }
    if let Some(page) = args.strip_prefix("list ") {
        let parts: Vec<_> = page.split_whitespace().collect();
        if parts.is_empty()
            || parts.len() > 2
            || parts
                .get(1)
                .is_some_and(|value| !matches!(*value, "before" | "after"))
        {
            bail!("Expected input list CURSOR [before|after]");
        }
        let mut page = app.shared.store.conversation_input_page(
            human,
            cfg.entry(),
            parts[0].parse()?,
            parts.get(1) != Some(&"after"),
        )?;
        page["routing_choices"] = choices(app)?;
        return Ok(page);
    }
    if let Some(id) = args.strip_prefix("status ") {
        return response(
            app,
            &app.shared
                .store
                .conversation_input_read(human, cfg.entry(), id)?,
            false,
        );
    }
    let request: Submit = serde_json::from_str(
        args.strip_prefix("submit ")
            .context("Expected input submit JSON, status UUID or list cursor")?,
    )?;
    // Preserve capture/handoff order for this configured integrating entry. The
    // lock is released before native acknowledgments, so later steering proceeds.
    if crate::interaction::explicit_selector(&request.text).is_none()
        && app
            .shared
            .store
            .conversation_input_read(human, cfg.entry(), &request.id)
            .is_err()
    {
        super::conversation_requests::validate_reply(app, human, request.reply.as_ref()).await?;
    }
    let interruption_targets = super::voice::snapshot(app).await;
    let _ingress = app.shared.conversation_ingress.lock().await;
    let (input, fresh) = app.shared.store.conversation_input_capture_reply(
        human,
        cfg.entry(),
        &request.binding,
        &request.id,
        &request.text,
        request.reply,
    )?;
    if fresh && input.phase == "retained" {
        if let Err(error) = super::voice::INTERRUPT_TARGETS
            .scope(interruption_targets, deliver(app, input.clone(), None))
            .await
        {
            app.shared
                .store
                .conversation_input_finish(human, &input.id)?;
            return Err(error);
        }
    }
    response(
        app,
        &app.shared
            .store
            .conversation_input_read(human, cfg.entry(), &input.id)?,
        fresh,
    )
}

async fn deliver(app: &App, input: ConversationInput, resolve: Option<&str>) -> Result<()> {
    if input
        .reply
        .as_ref()
        .is_some_and(|r| r.source.starts_with("request:") && !r.context_only)
        && crate::interaction::explicit_selector(&input.text).is_none()
        && resolve.is_none()
    {
        return super::conversation_requests::answer_original(app, &input).await;
    }
    let profile = super::routing::profile(&app.shared)?;
    let action = app
        .shared
        .store
        .conversation_route(input.human, &input.id, &profile, resolve)?;
    apply_route(app, input, action).await
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Clarification {
    pending_input: String,
    input_ref: crate::interaction::InputReference,
    selector: String,
}
pub(super) async fn clarify(
    app: &App,
    human: i64,
    native: &str,
    args: Value,
    closed: &CancellationToken,
) -> Result<Value> {
    let selected: Clarification = serde_json::from_value(args)?;
    let profile = super::routing::profile(&app.shared)?;
    if closed.is_cancelled() {
        bail!("Clarification request closed");
    }
    let action = app.shared.store.conversation_route_clarified(
        human,
        &selected.pending_input,
        &profile,
        &selected.selector,
        (native, &selected.input_ref, closed),
    )?;
    let input = app.shared.store.conversation_input_read(
        human,
        profile.entry(),
        &selected.pending_input,
    )?;
    apply_route(app, input, action).await?;
    response(
        app,
        &app.shared.store.conversation_input_read(
            human,
            profile.entry(),
            &selected.pending_input,
        )?,
        false,
    )
}

async fn apply_route(
    app: &App,
    input: ConversationInput,
    action: Option<crate::store::ConversationRouteAction>,
) -> Result<()> {
    match action {
        Some(crate::store::ConversationRouteAction::Worker(instruction, fresh)) => {
            if fresh
                && super::dialogue::dispatch_original(
                    app,
                    &instruction.id,
                    input.human,
                    &crate::interaction::InputReference::Conversation(input.id.clone()),
                )
                .await
                .is_err()
            {
                app.shared
                    .store
                    .interaction_unfinished(&instruction.id, input.human)?;
            }
            return Ok(());
        }
        Some(crate::store::ConversationRouteAction::Interrupt(expected)) => {
            app.shared
                .store
                .conversation_interruption_apply(input.human, &input.id, |target| {
                    if target == expected {
                        super::voice::interrupt_snapshot(target)
                    } else {
                        None
                    }
                })?;
            return Ok(());
        }
        None => {}
    }
    let current = app
        .shared
        .store
        .conversation_input_read(input.human, input.entry, &input.id)?;
    if current.phase != "retained" {
        return Ok(());
    }
    app.shared
        .store
        .conversation_input_event(input.human, &input.id, E::Queue)?;
    submit(app.clone(), input).await
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
        text: input.rendered(),
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
                prompt: input.rendered(),
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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct VoiceSubmit {
    binding: String,
    id: String,
    audio_sha256: String,
    mime: String,
    #[serde(default)]
    reply: Option<crate::store::ConversationReply>,
}

async fn voice_submit(app: &App, human: i64, entry: SessionKey, body: &str) -> Result<Value> {
    use crate::store::ConversationVoice;
    let request: VoiceSubmit = serde_json::from_str(body)?;
    let config = app
        .shared
        .config
        .voice
        .clone()
        .context("Voice input is not configured")?;
    if Uuid::parse_str(&request.id)
        .ok()
        .is_none_or(|v| v.to_string() != request.id)
    {
        bail!("Invalid voice capture identity");
    }
    let voice = ConversationVoice {
        audio_sha256: request.audio_sha256,
        mime: request.mime,
        transcription: None,
    };
    voice.validate()?;
    let directory =
        std::path::PathBuf::from(format!("{}.voice", app.shared.config.db_path.display()));
    let path = directory.join(&request.id);
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let folder = std::fs::symlink_metadata(&directory)?;
        let file = std::fs::symlink_metadata(&path)?;
        if !folder.is_dir()
            || folder.uid() != unsafe { libc::geteuid() }
            || folder.mode() & 0o077 != 0
            || !file.is_file()
            || file.uid() != unsafe { libc::geteuid() }
            || file.mode() & 0o077 != 0
            || file.len() == 0
            || file.len() > crate::voice::MAX_BYTES as u64
        {
            bail!("Voice capture file is not owner-private and bounded");
        }
    }
    if app
        .shared
        .store
        .conversation_input_read(human, entry, &request.id)
        .is_err()
    {
        super::conversation_requests::validate_reply(app, human, request.reply.as_ref()).await?;
    }
    let interruption_targets = super::voice::snapshot(app).await;
    let permit = app
        .shared
        .voice_preparations
        .clone()
        .try_acquire_owned()
        .ok();
    let _ingress = app.shared.conversation_ingress.lock().await;
    let (input, fresh) = app.shared.store.conversation_voice_capture_reply(
        human,
        entry,
        &request.binding,
        &request.id,
        voice,
        permit.is_some(),
        request.reply,
    )?;
    if fresh && input.phase == "preparing" {
        let worker = app.clone();
        let captured = input.clone();
        tokio::spawn(async move {
            let _permit = permit;
            let preparation = super::voice::INTERRUPT_TARGETS.scope(interruption_targets, async {
                worker
                    .shared
                    .store
                    .conversation_voice_validate(human, &captured.id)?;
                let expected = &captured
                    .voice
                    .as_ref()
                    .context("Missing voice capture")?
                    .audio_sha256;
                let (text, proof) = crate::voice::transcribe_committed(
                    &config,
                    &path,
                    None,
                    Some(&captured.id),
                    Some(expected),
                )
                .await?;
                let _ingress = worker.shared.conversation_ingress.lock().await;
                let ready = worker.shared.store.conversation_voice_commit(
                    human,
                    &captured.id,
                    &text,
                    proof,
                )?;
                if let Err(error) = deliver(&worker, ready, None).await {
                    worker
                        .shared
                        .store
                        .conversation_input_finish(human, &captured.id)?;
                    return Err(error);
                }
                Ok::<_, anyhow::Error>(())
            });
            let result = tokio::select! {biased;_ = worker.shared.voice_shutdown.cancelled()=>Err(anyhow!("Voice preparation ended for listener restart")),result=preparation=>result};
            if result.is_err() {
                let _ = worker
                    .shared
                    .store
                    .conversation_voice_fail(human, &captured.id);
                tracing::warn!(
                    "Committed conversation voice preparation unconfirmed; no automatic retry"
                );
            }
        });
    }
    response(
        app,
        &app.shared
            .store
            .conversation_input_read(human, entry, &input.id)?,
        fresh,
    )
}
