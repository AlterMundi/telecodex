//! Owner-local integrating conversation. No second model/session writer.
use super::*;
use crate::interaction::{
    Instruction, RequestRelation, ReturnRoute, SourceInput, TransportPhase as P, explicit_selector,
};

pub(super) struct Run {
    root: String,
    pending: Vec<String>,
    active: Option<ActiveTurnSteerHandle>,
}

pub(super) fn enrolled(shared: &AppShared, key: SessionKey) -> bool {
    shared
        .config
        .interaction
        .as_ref()
        .is_some_and(|c| c.chat_id == key.chat_id && c.target(key.thread_id).is_some())
}

pub(super) fn explicit(text: &str) -> bool {
    explicit_selector(text).is_some() || text.starts_with("/interaction")
}

fn route(message: &Message) -> ReturnRoute {
    ReturnRoute {
        chat_id: message.chat.id,
        thread_id: message.message_thread_id.unwrap_or(0),
        message_id: message.message_id,
    }
}

fn label(shared: &AppShared, key: SessionKey) -> String {
    shared
        .config
        .interaction
        .as_ref()
        .and_then(|c| c.targets.iter().find(|t| t.topic_id == key.thread_id))
        .map(|t| t.name.clone())
        .unwrap_or_else(|| format!("t{}", key.thread_id))
}

fn notice(shared: Arc<AppShared>, origin: ReturnRoute, text: String) {
    tokio::spawn(async move {
        let mut msg = SendMessage::html(
            origin.chat_id,
            Some(origin.thread_id).filter(|id| *id != 0),
            render_markdown_to_html(&text),
        );
        msg.reply_parameters = Some(crate::telegram::ReplyParameters {
            message_id: origin.message_id,
            allow_sending_without_reply: false,
        });
        if shared.telegram.send_message(msg).await.is_err() {
            tracing::warn!("Interaction notice could not be confirmed");
        }
    });
}

pub(super) async fn handle(app: &App, message: &Message, human: i64, text: &str) -> Result<bool> {
    let Some(cfg) = app.shared.config.interaction.as_ref() else {
        return Ok(false);
    };
    let key = SessionKey::new(message.chat.id, message.message_thread_id);
    if key != cfg.entry() && !enrolled(&app.shared, key) {
        return Ok(false);
    }
    if human != cfg.human_id {
        return Ok(true);
    }
    let origin = route(message);
    let update = HUMAN_UPDATE_ID
        .try_with(|id| *id)
        .ok()
        .context("Interaction input lacks its ingress journal identity")?;
    if text.starts_with("/interaction") {
        let args = text.split_whitespace().skip(1).collect::<Vec<_>>();
        if let [verb, id] = args.as_slice() {
            let Some(i) = app.shared.store.interaction_instruction(id, human)? else {
                notice(app.shared.clone(), origin, "Unknown input.".into());
                return Ok(true);
            };
            if !enrolled(&app.shared, i.target) {
                notice(
                    app.shared.clone(),
                    origin,
                    "That conversation is not enrolled in this profile.".into(),
                );
                return Ok(true);
            }
            if *verb == "cancel" {
                let cancelled = app.shared.store.interaction_cancel_unsent(id, human)?;
                notice(app.shared.clone(),origin,if cancelled{"Input cancelled before native submission."}else{"The worker may already have received that input. Select its topic and use /interrupt to interrupt the current turn."}.into());
                return Ok(true);
            }
            if *verb == "retry" {
                if app.shared.store.interaction_recover(id, human)? {
                    dispatch(app, id, human).await?;
                    notice(
                        app.shared.clone(),
                        origin,
                        "Known-unsent input queued for recovery.".into(),
                    );
                } else {
                    notice(
                        app.shared.clone(),
                        origin,
                        "That input is not known-unsent; it was not replayed.".into(),
                    );
                }
                return Ok(true);
            }
        }
        let mut status = String::from("Interaction status");
        for t in &cfg.targets {
            let target = SessionKey::new(cfg.chat_id, Some(t.topic_id));
            let recent = app.shared.store.interaction_recent(human, target)?;
            let state = recent
                .first()
                .map(|i| i.phase.as_str())
                .unwrap_or("no admitted input");
            status.push_str(&format!("\n#{}: {}", t.name, state));
            if let Some(i) = recent.first() {
                status.push_str(&format!("\nInput: `{}`", i.id));
            }
        }
        notice(app.shared.clone(), origin, status);
        return Ok(true);
    }
    let inherited = if let Some(reply) = &message.reply_to_message {
        let reference = ReturnRoute {
            message_id: reply.message_id,
            ..origin.clone()
        };
        if let Some(binding) = app
            .shared
            .store
            .interaction_message_target(&reference, human)?
        {
            vec![binding]
        } else {
            app.shared
                .store
                .interaction_reply(&reference, human)?
                .into_iter()
                .map(|i| (i.target, i.requests))
                .collect()
        }
    } else {
        Vec::new()
    };
    let pending = message
        .reply_to_message
        .as_ref()
        .map(|r| {
            app.shared.store.interaction_hold_reply(
                &ReturnRoute {
                    message_id: r.message_id,
                    ..origin.clone()
                },
                human,
            )
        })
        .transpose()?
        .flatten();
    let selected = explicit_selector(text);
    if let Some(pending) = pending.as_ref() {
        if text == "/cancel" && pending["phase"] == "awaiting_target" {
            app.shared.store.interaction_close_hold(
                pending["id"].as_str().unwrap(),
                human,
                false,
            )?;
            notice(
                app.shared.clone(),
                origin,
                "Pending routing cancelled.".into(),
            );
            return Ok(true);
        }
        if let Some((selector, body)) = selected {
            let matches = cfg.resolve(selector);
            if matches.len() == 1 && pending["phase"] == "awaiting_target" {
                let mut original: SourceInput = serde_json::from_value(pending["source"].clone())?;
                let previous: Vec<(SessionKey, Vec<String>)> =
                    serde_json::from_value(pending["inherited"].clone())?;
                let target = matches[0];
                let record = app
                    .shared
                    .store
                    .get_session(target)?
                    .context("Selected interaction session missing")?;
                original.target_sessions = vec![record.id];
                let relation = previous
                    .into_iter()
                    .find(|(key, _)| *key == target)
                    .filter(|(_, r)| !r.is_empty())
                    .map(|(_, r)| RequestRelation::Amends(r))
                    .unwrap_or(RequestRelation::New);
                let (i, fresh) = app
                    .shared
                    .store
                    .interaction_admit(&original, target, relation)?;
                app.shared.store.interaction_close_hold(
                    pending["id"].as_str().unwrap(),
                    human,
                    true,
                )?;
                if fresh {
                    dispatch(app, &i.id, human).await?;
                }
                if !body.is_empty() {
                    admit_message(
                        app,
                        message,
                        update,
                        target,
                        body,
                        RequestRelation::Amends(i.requests),
                    )
                    .await?;
                }
                return Ok(true);
            }
            if body.is_empty() && pending["phase"] != "awaiting_target" {
                notice(app.shared.clone(),origin,"That routing question was already resolved or cancelled. The earlier input was not dispatched again.".into());
                return Ok(true);
            }
        }
    }
    let (targets, body) = match selected {
        Some((name, body)) => (cfg.resolve(name), body),
        None if inherited.len() == 1 => (vec![inherited[0].0], text),
        None if enrolled(&app.shared, key) => (vec![key], text),
        None => return Ok(false), // The integrating conversation remains conversational.
    };
    if targets.len() != 1 {
        let source = SourceInput {
            channel: format!("telegram:{}", app.shared.bot_id),
            event_id: update.to_string(),
            update_id: Some(update),
            human_id: human,
            target_sessions: vec![],
            origin: origin.clone(),
            text: body.to_owned(),
        };
        let id = app.shared.store.interaction_hold(&source, &inherited)?;
        let choices = cfg
            .targets
            .iter()
            .map(|t| format!("#{} (#t{})", t.name, t.topic_id))
            .collect::<Vec<_>>()
            .join(", ");
        ask_target(
            app.shared.clone(),
            id,
            human,
            origin,
            format!("Which conversation? Reply with a selector: {choices}"),
        );
        return Ok(true);
    }
    let target = targets[0];
    if !enrolled(&app.shared, target) {
        notice(
            app.shared.clone(),
            origin,
            "That conversation is not enrolled in this profile.".into(),
        );
        return Ok(true);
    }
    if body == "/interrupt" {
        let requested = app.stop_session(target).await;
        notice(
            app.shared.clone(),
            origin,
            if requested {
                format!(
                    "Turn interruption requested for #{}. Goals, children and external effects retain their own state.",
                    label(&app.shared, target)
                )
            } else {
                format!(
                    "No listener-owned running turn was selected in #{}.",
                    label(&app.shared, target)
                )
            },
        );
        return Ok(true);
    }
    if body.starts_with('/') {
        if selected.is_some() {
            notice(app.shared.clone(), origin, "Only /interrupt is supported after a topic selector. Other controls belong in the selected conversation.".into());
            return Ok(true);
        }
        return Ok(false);
    }
    if body.is_empty() {
        let shared = app.shared.clone();
        let name = label(&shared, target);
        let origin = origin.clone();
        tokio::spawn(async move {
            let mut m = SendMessage::html(
                origin.chat_id,
                Some(origin.thread_id),
                format!(
                    "Selected #{}. Reply here with your message.",
                    html_escape::encode_safe(&name)
                ),
            );
            m.reply_parameters = Some(crate::telegram::ReplyParameters {
                message_id: origin.message_id,
                allow_sending_without_reply: false,
            });
            if let Ok(m) = shared.telegram.send_message(m).await {
                let _ = shared.store.interaction_bind_message(
                    human,
                    &ReturnRoute {
                        chat_id: m.chat.id,
                        thread_id: m.message_thread_id.unwrap_or(0),
                        message_id: m.message_id,
                    },
                    target,
                    &[],
                );
            }
        });
        return Ok(true);
    }
    if !is_text_only_steer_candidate(message, text) {
        notice(app.shared.clone(),origin,"This router currently accepts committed text. The input was retained without dispatch.".into());
        return Ok(true);
    }
    let relation = inherited
        .into_iter()
        .find(|(key, _)| *key == target)
        .filter(|(_, r)| !r.is_empty())
        .map(|(_, r)| RequestRelation::Amends(r))
        .unwrap_or(RequestRelation::New);
    admit_message(app, message, update, target, body, relation).await?;
    Ok(true)
}

fn ask_target(shared: Arc<AppShared>, id: String, human: i64, origin: ReturnRoute, text: String) {
    tokio::spawn(async move {
        let result = shared
            .telegram
            .send_message_checked(origin.chat_id, || {
                if shared.store.interaction_hold_state(&id, human)?.as_deref()
                    != Some("awaiting_target")
                {
                    return Ok(None);
                }
                let mut m = SendMessage::html(
                    origin.chat_id,
                    Some(origin.thread_id),
                    render_markdown_to_html(&text),
                );
                m.reply_parameters = Some(crate::telegram::ReplyParameters {
                    message_id: origin.message_id,
                    allow_sending_without_reply: false,
                });
                Ok(Some((m, ())))
            })
            .await;
        if let Ok(Some((m, _))) = result {
            let _ = shared.store.interaction_hold_message(
                &id,
                human,
                &ReturnRoute {
                    chat_id: m.chat.id,
                    thread_id: m.message_thread_id.unwrap_or(0),
                    message_id: m.message_id,
                },
            );
        }
    });
}

async fn admit_message(
    app: &App,
    message: &Message,
    update: i64,
    target: SessionKey,
    text: &str,
    relation: RequestRelation,
) -> Result<()> {
    let session = app
        .shared
        .store
        .get_session(target)?
        .context("Selected interaction session missing")?;
    let source = SourceInput {
        channel: format!("telegram:{}", app.shared.bot_id),
        event_id: update.to_string(),
        update_id: Some(update),
        human_id: message
            .from
            .as_ref()
            .context("Interaction author missing")?
            .id,
        target_sessions: vec![session.id],
        origin: route(message),
        text: text.to_owned(),
    };
    let (i, fresh) = app
        .shared
        .store
        .interaction_admit(&source, target, relation)?;
    if fresh {
        dispatch(app, &i.id, i.human_id).await?;
    }
    Ok(())
}

fn packet(shared: &AppShared, i: &Instruction) -> Result<String> {
    let context = shared
        .store
        .interaction_request_inputs(&i.requests, i.human_id)?;
    Ok(format!(
        "[Interaction controller]\nCurrent instruction: {}\nAdmission sequence: {}\nRelation: {}\nAddressed requests: {}\nOriginal requests (context, not a command to repeat completed effects): {}\n\nCurrent human input:\n{}\n\nFor each final answer, append a final line <daimon-result>{{\"requests\":[addressed request IDs],\"covered\":[instruction IDs actually incorporated],\"completed\":[request IDs actually finished],\"deferred\":[unaddressed instruction IDs]}}</daimon-result>. This is your declaration, not native proof. Never invent IDs. Retain the normal answer text before that line. Do not claim completion merely because a turn ends. Later admission sequences take precedence over earlier conflicting instructions. Prior uncertain actions must not be repeated just from this context.\n",
        i.id,
        i.sequence,
        serde_json::to_string(&i.relation)?,
        serde_json::to_string(&i.requests)?,
        serde_json::to_string(&context)?,
        i.text
    ))
}

fn reserve_run(shared: &AppShared, i: &Instruction) -> Option<Option<ActiveTurnSteerHandle>> {
    {
        let mut runs = shared.routing_runs.lock().expect("routing mutex poisoned");
        if let Some(run) = runs.get_mut(&i.target) {
            if let Some(active) = &run.active {
                Some(Some(active.clone()))
            } else {
                run.pending.push(i.id.clone());
                None
            }
        } else {
            runs.insert(
                i.target,
                Run {
                    root: i.id.clone(),
                    pending: vec![],
                    active: None,
                },
            );
            Some(None)
        }
    }
}

async fn dispatch(app: &App, id: &str, human: i64) -> Result<()> {
    let i = app
        .shared
        .store
        .interaction_instruction(id, human)?
        .context("Interaction input missing")?;
    if i.phase != P::Queued {
        return Ok(());
    }
    let action = reserve_run(&app.shared, &i);
    match action {
        Some(Some(active)) => steer(app.clone(), i, active)?,
        Some(None) => {
            if let Err(error) = enqueue_root(app, &i).await {
                finished(&app.shared, i.target, &i.id, human);
                failure(app.shared.clone(), &i.id, human, &error.to_string());
                return Err(error);
            }
        }
        None => {}
    }
    Ok(())
}

async fn enqueue_root(app: &App, i: &Instruction) -> Result<()> {
    let source = app.shared.store.interaction_source(&i.id, i.human_id)?;
    super::nexus::validate_input_target(&app.shared, source.update_id, i.target, i.human_id)?;
    let prompt = super::nexus::input_prompt(
        &app.shared,
        source.update_id,
        i.target,
        i.human_id,
        &packet(&app.shared, i)?,
    )?;
    let epoch = stopping::input_epoch(&app.shared, i.target)?;
    let worker = app.worker_for(i.target).await?;
    worker
        .sender
        .send(QueuedTurn {
            request: TurnRequest {
                session_key: i.target,
                from_user_id: i.human_id,
                prompt,
                runtime_instructions: None,
                attachments: vec![],
                review_mode: None,
                override_search_mode: None,
            },
            chat_kind: if i.target.chat_id < 0 {
                "supergroup"
            } else {
                "private"
            }
            .into(),
            update_id: None,
            tribu_admission: None,
            goal: None,
            nexus: None,
            merge: None,
            stop_resume: None,
            interaction: Some(i.id.clone()),
            admission_epoch: epoch,
        })
        .map_err(|_| anyhow!("Interaction worker unavailable"))?;
    Ok(())
}

fn steer(app: App, i: Instruction, active: ActiveTurnSteerHandle) -> Result<()> {
    stopping::input_epoch(&app.shared, i.target)?;
    app.shared
        .store
        .interaction_transport(&i.id, i.human_id, active.turn_id, P::Steering, None)?;
    let (tx, rx) = oneshot::channel();
    let sent = active
        .sender
        .send(CodexSteerRequest {
            text: packet(&app.shared, &i)?,
            client_message_id: Some(i.id.clone()),
            goal: None,
            response: tx,
        })
        .is_ok();
    tokio::spawn(async move {
        let result = if sent {
            tokio::time::timeout(Duration::from_secs(10), rx)
                .await
                .ok()
                .and_then(Result::ok)
        } else {
            Some(Err(CodexSteerError::Rejected(
                "Owner channel closed before submission".into(),
            )))
        };
        match result {
            Some(Ok(Some(native))) => {
                let _ = app.shared.store.interaction_transport(
                    &i.id,
                    i.human_id,
                    active.turn_id,
                    P::Accepted,
                    Some(&native),
                );
            }
            Some(Err(CodexSteerError::Rejected(_))) => {
                if app
                    .shared
                    .store
                    .interaction_transport(&i.id, i.human_id, active.turn_id, P::Rejected, None)
                    .is_ok()
                    && app
                        .shared
                        .store
                        .interaction_requeue_rejected(&i.id, i.human_id)
                        .unwrap_or(false)
                {
                    let next = app
                        .shared
                        .store
                        .interaction_instruction(&i.id, i.human_id)
                        .ok()
                        .flatten();
                    if let Some(next) = next {
                        {
                            let mut runs = app
                                .shared
                                .routing_runs
                                .lock()
                                .expect("routing mutex poisoned");
                            if runs
                                .get(&i.target)
                                .and_then(|r| r.active.as_ref())
                                .is_some_and(|a| a.turn_id == active.turn_id)
                            {
                                runs.remove(&i.target);
                            }
                        }
                        match reserve_run(&app.shared, &next) {
                            Some(Some(new_active)) => {
                                let result = steer(app.clone(), next, new_active);
                                if result.is_err() {
                                    failure(
                                        app.shared.clone(),
                                        &i.id,
                                        i.human_id,
                                        "steer unavailable",
                                    );
                                }
                            }
                            Some(None) => {
                                let result = enqueue_root(&app, &next).await;
                                if result.is_err() {
                                    finished(&app.shared, i.target, &i.id, i.human_id);
                                    failure(
                                        app.shared.clone(),
                                        &i.id,
                                        i.human_id,
                                        "worker unavailable",
                                    );
                                }
                            }
                            None => {}
                        }
                    }
                }
            }
            _ => {
                let _ = app.shared.store.interaction_transport(
                    &i.id,
                    i.human_id,
                    active.turn_id,
                    P::Uncertain,
                    None,
                );
                notice(app.shared.clone(),i.origin,"The worker may have received this input; acceptance is unconfirmed. It was retained without replay.".into());
            }
        }
    });
    Ok(())
}

pub(super) fn started(
    app: &App,
    key: SessionKey,
    root: &str,
    active: ActiveTurnSteerHandle,
) -> Result<()> {
    let pending = {
        let mut runs = app
            .shared
            .routing_runs
            .lock()
            .expect("routing mutex poisoned");
        if let Some(run) = runs.get_mut(&key).filter(|run| run.root == root) {
            run.active = Some(active.clone());
            std::mem::take(&mut run.pending)
        } else {
            vec![]
        }
    };
    for id in pending {
        if let Some(i) = app.shared.store.interaction_instruction(
            &id,
            app.shared.config.interaction.as_ref().unwrap().human_id,
        )? {
            if i.phase == P::Queued {
                steer(app.clone(), i, active.clone())?;
            }
        }
    }
    Ok(())
}

pub(super) fn finished(shared: &Arc<AppShared>, key: SessionKey, root: &str, human: i64) {
    let pending = {
        let mut runs = shared.routing_runs.lock().expect("routing mutex poisoned");
        if runs.get(&key).is_some_and(|run| run.root == root) {
            runs.remove(&key).map(|run| run.pending).unwrap_or_default()
        } else {
            vec![]
        }
    };
    let _ = shared.store.interaction_unfinished(root, human);
    for id in pending {
        let _ = shared.store.interaction_unfinished(&id, human);
    }
}

pub(super) fn failure(shared: Arc<AppShared>, root: &str, human: i64, error: &str) {
    if let Ok(Some(i)) = shared.store.interaction_instruction(root, human) {
        let _ = shared.store.interaction_unfinished(root, human);
        let message = if error.contains("cancelled") {
            format!(
                "#{}: turn interrupted. Completed effects remain; goal/child state is separate.",
                label(&shared, i.target)
            )
        } else {
            format!(
                "#{} could not finish. The input and any uncertain outcome are retained. Use /interaction to inspect it.",
                label(&shared, i.target)
            )
        };
        notice(shared, i.origin, message);
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn result(
    shared: Arc<AppShared>,
    root: &str,
    human: i64,
    local: i64,
    thread: &str,
    turn: &str,
    item: &str,
    text: &str,
) -> Result<()> {
    let deliveries = shared
        .store
        .interaction_reserve_result(root, human, local, thread, turn, item, text)?;
    for delivery in deliveries {
        let shared = shared.clone();
        tokio::spawn(async move {
            let Some(d) = shared
                .store
                .interaction_begin_delivery(&delivery.id, human)
                .ok()
                .flatten()
            else {
                return;
            };
            let name = label(&shared, d.target);
            let pending = render_markdown_to_html(&format!(
                "#{} has an earlier result; your correction is still pending.",
                name
            ));
            let qualification = if d.declared {
                ""
            } else {
                "\nWorker instruction coverage was not reported."
            };
            let body = format!("#{}{}\n\n{}", name, qualification, d.body);
            let mut last = None;
            let mut complete = true;
            for (index, part) in split_text(&body, 3000).into_iter().enumerate() {
                let html = render_markdown_to_html(&part);
                let sent = shared
                    .telegram
                    .send_message_checked(d.origin.chat_id, || {
                        let Some((fresh, text)) = shared.store.interaction_reserve_part(
                            &d.id,
                            human,
                            index as i64,
                            &html,
                            &pending,
                        )?
                        else {
                            return Ok(None);
                        };
                        let mut message = SendMessage::html(
                            fresh.origin.chat_id,
                            Some(fresh.origin.thread_id).filter(|id| *id != 0),
                            text,
                        );
                        message.reply_parameters = Some(crate::telegram::ReplyParameters {
                            message_id: fresh.origin.message_id,
                            allow_sending_without_reply: false,
                        });
                        Ok(Some((message, fresh.correction_pending)))
                    })
                    .await;
                match sent {
                    Ok(Some((message, correction)))
                        if message.chat.id == d.origin.chat_id
                            && message.message_thread_id.unwrap_or(0) == d.origin.thread_id =>
                    {
                        if shared
                            .store
                            .interaction_part_receipt(
                                &d.id,
                                human,
                                index as i64,
                                message.message_id,
                            )
                            .is_err()
                        {
                            complete = false;
                            break;
                        }
                        last = Some(message.message_id);
                        if correction {
                            break;
                        }
                    }
                    _ => {
                        complete = false;
                        break;
                    }
                }
            }
            let _ = shared.store.interaction_finish_delivery(
                &d.id,
                human,
                if complete { last } else { None },
            );
        });
    }
    Ok(())
}

pub(super) fn retained_artifacts(shared: Arc<AppShared>, root: &str, human: i64) {
    if let Ok(Some(i)) = shared.store.interaction_instruction(root, human) {
        notice(shared,i.origin,"Generated files remain retained with the worker turn. This router currently returns text; file delivery is not yet enabled.".into());
    }
}
