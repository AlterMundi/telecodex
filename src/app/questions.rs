//! A pending question never blocks the native RPC loop or becomes a new turn.

use super::*;

pub(super) struct PendingDialogue {
    pub local_turn_id: i64,
    pub request_id: NativeRequestId,
    session: SessionKey,
    pub(super) native_session: SessionKey,
    return_parent: Option<i64>,
    pub(super) requester: i64,
    pub(super) request: UserInputRequest,
    pub(super) index: usize,
    message_id: i64,
    message_ids: Vec<i64>,
    awaiting_text: bool,
    pub(super) answers: UserInputResponse,
    pub(super) closed: CancellationToken,
    pub(super) responder: oneshot::Sender<UserInputDecision>,
}

impl PendingDialogue {
    pub(super) fn retire_telegram_controls(&mut self) {
        self.message_id = 0;
        self.message_ids.clear();
        self.awaiting_text = false;
    }

    pub(super) fn apply_answer(&mut self, value: String) {
        self.answers.answers.insert(
            self.request.questions[self.index].id.clone(),
            QuestionAnswer {
                answers: vec![value],
            },
        );
        self.index += 1;
        self.awaiting_text = false;
    }
}

async fn show_question(
    shared: &Arc<AppShared>,
    token: &str,
    pending: &mut PendingDialogue,
) -> Result<()> {
    let question = &pending.request.questions[pending.index];
    shared.store.update_native_interaction(
        token,
        &question.header,
        &question.question,
        pending.index,
    )?;
    if shared
        .store
        .conversation_turn_return(pending.local_turn_id)?
    {
        pending.retire_telegram_controls();
        // Keep the same pending native responder and durable human interaction.
        // Retain its response channel for the canonical owner interface.
        return Ok(());
    }
    let mut text = format!(
        "Codex question {}/{}: {}\n\n{}",
        pending.index + 1,
        pending.request.questions.len(),
        question.header,
        question.question
    );
    let mut rows = Vec::new();
    if let Some(options) = &question.options {
        for (index, option) in options.iter().enumerate() {
            text.push_str(&format!("\n\n{} — {}", option.label, option.description));
            rows.push(vec![InlineKeyboardButton {
                text: option.label.chars().take(80).collect(),
                callback_data: Some(format!("q:{token}:{}:{index}", pending.index)),
                url: None,
            }]);
        }
    }
    if !pending.request.is_blocking {
        text.push_str("\n\nCodex can continue while this question is open.");
    }
    text.push_str("\n\nReply to this message to answer, or tap Write answer then send text. Other text keeps steering the turn.");
    rows.push(vec![
        InlineKeyboardButton {
            text: if question.is_other {
                "Other answer"
            } else {
                "Write answer"
            }
            .to_string(),
            callback_data: Some(format!("q:{token}:{}:text", pending.index)),
            url: None,
        },
        InlineKeyboardButton {
            text: "Cancel turn".to_string(),
            callback_data: Some(format!("q:{token}:{}:cancel", pending.index)),
            url: None,
        },
    ]);
    pending.message_ids.clear();
    // Bound UTF-16 length too: Telegram entities count astral characters twice.
    let chunks = split_text(&text, 1900);
    let count = chunks.len();
    for (index, chunk) in chunks.into_iter().enumerate() {
        let chunk = if index == 0 {
            chunk
        } else {
            format!(
                "Codex question {}/{} (continued)\n\n{chunk}",
                pending.index + 1,
                pending.request.questions.len()
            )
        };
        let mut outgoing = SendMessage::html(
            pending.session.chat_id,
            Some(pending.session.thread_id).filter(|id| *id != 0),
            html_escape::encode_safe(&chunk).to_string(),
        );
        if index + 1 == count {
            outgoing.reply_markup = Some(InlineKeyboardMarkup {
                inline_keyboard: rows.clone(),
            });
        }
        outgoing.reply_parameters =
            pending
                .return_parent
                .map(|message_id| crate::telegram::ReplyParameters {
                    message_id,
                    allow_sending_without_reply: false,
                });
        // Controls must never fall back to another topic or open after retirement.
        let message = super::presentation::turn_telegram(shared, Some(pending.local_turn_id))
            .send_message_checked(pending.session.chat_id, || {
                if shared
                    .store
                    .conversation_turn_return(pending.local_turn_id)?
                    || pending.closed.is_cancelled()
                    || pending.responder.is_closed()
                {
                    return Ok(None);
                }
                Ok(Some((outgoing, ())))
            })
            .await;
        let message = match message {
            Ok(Some((message, _))) => message,
            Ok(None)
                if shared
                    .store
                    .conversation_turn_return(pending.local_turn_id)? =>
            {
                pending.message_id = 0;
                pending.message_ids.clear();
                pending.awaiting_text = false;
                return Ok(());
            }
            Err(error) if error.is::<crate::telegram::PublicationRetired>() => {
                pending.message_id = 0;
                pending.message_ids.clear();
                pending.awaiting_text = false;
                return Ok(());
            }
            Ok(None) => bail!("Native question retired before publication"),
            Err(error) => return Err(error),
        };
        if message.chat.id != pending.session.chat_id
            || message.message_thread_id.unwrap_or(0) != pending.session.thread_id
        {
            anyhow::bail!("native question delivery did not confirm its Telegram topic");
        }
        shared.store.record_question_message(pending.session, message.message_id, pending.local_turn_id,
            &serde_json::json!({"requestId":pending.request_id,"threadId":pending.request.thread_id,
                "turnId":pending.request.turn_id,"itemId":pending.request.item_id,"questionId":question.id}))?;
        pending.message_id = message.message_id;
        pending.message_ids.push(message.message_id);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn begin_dialogue(
    shared: Arc<AppShared>,
    native_session: SessionKey,
    local_turn_id: i64,
    requester: i64,
    request_id: NativeRequestId,
    request: UserInputRequest,
    closed: CancellationToken,
    return_route: Option<crate::interaction::ReturnRoute>,
) -> Result<oneshot::Receiver<UserInputDecision>> {
    request.validate()?;
    let (tx, rx) = oneshot::channel();
    // Telegram cooldown/delivery must not hold the native event pump.
    tokio::spawn(async move {
        let surface = return_route
            .as_ref()
            .map(|r| SessionKey::new(r.chat_id, Some(r.thread_id)))
            .unwrap_or(native_session);
        let closure = closed.clone();
        match publish_dialogue(
            shared.clone(),
            native_session,
            local_turn_id,
            requester,
            request_id,
            request,
            closed,
            return_route.clone(),
        )
        .await
        {
            Ok(receiver) => {
                if let Ok(decision) = receiver.await {
                    let _ = tx.send(decision);
                }
            }
            Err(_) => {
                tracing::warn!("Native question publication could not be confirmed");
                let retired = closure.is_cancelled();
                let _ = tx.send(UserInputDecision::Cancel);
                if !retired
                    && !shared
                        .store
                        .conversation_turn_return(local_turn_id)
                        .unwrap_or(true)
                {
                    let mut message = SendMessage::html(surface.chat_id,Some(surface.thread_id).filter(|id|*id!=0),"Native question delivery did not confirm its Telegram topic. No answer was sent; turn cancellation was requested.".into());
                    message.reply_parameters =
                        return_route.and_then(|r| r.message_id).map(|message_id| {
                            crate::telegram::ReplyParameters {
                                message_id,
                                allow_sending_without_reply: false,
                            }
                        });
                    let _ = shared.telegram.send_message(message).await;
                }
            }
        }
    });
    Ok(rx)
}

#[allow(clippy::too_many_arguments)]
async fn publish_dialogue(
    shared: Arc<AppShared>,
    native_session: SessionKey,
    local_turn_id: i64,
    requester: i64,
    request_id: NativeRequestId,
    request: UserInputRequest,
    closed: CancellationToken,
    return_route: Option<crate::interaction::ReturnRoute>,
) -> Result<oneshot::Receiver<UserInputDecision>> {
    request.validate()?;
    let token = Uuid::now_v7().simple().to_string();
    let (responder, receiver) = oneshot::channel();
    let mut pending = PendingDialogue {
        local_turn_id,
        request_id,
        session: return_route
            .as_ref()
            .map(|r| SessionKey::new(r.chat_id, Some(r.thread_id)))
            .unwrap_or(native_session),
        native_session,
        return_parent: return_route.as_ref().and_then(|r| r.message_id),
        requester,
        request,
        index: 0,
        message_id: 0,
        message_ids: Vec::new(),
        awaiting_text: false,
        answers: UserInputResponse::default(),
        responder,
        closed,
    };
    let first = &pending.request.questions[0];
    shared.store.save_interaction(native_session, &crate::store::HumanInteraction {
        id: token.clone(), kind: "native".into(), state: "pending".into(), requester,
        native_thread: pending.request.thread_id.clone(), local_turn: local_turn_id,
        title: first.header.clone(), action: first.question.clone(), blocking: pending.request.is_blocking,
        source: serde_json::json!({"kind":"question","request":pending.request,"requestId":serde_json::to_string(&pending.request_id)?,
            "threadId":pending.request.thread_id,"turnId":pending.request.turn_id,"itemId":pending.request.item_id}),
    }, None)?;
    show_question(&shared, &token, &mut pending).await?;
    if !pending.closed.is_cancelled() && !pending.responder.is_closed() {
        shared.pending_questions.lock().await.insert(token, pending);
    }
    Ok(receiver)
}

pub(super) async fn notice(shared: &AppShared, message: &Message, text: &str) -> Result<()> {
    shared
        .telegram
        .send_message(SendMessage::html(
            message.chat.id,
            message.message_thread_id,
            html_escape::encode_safe(text).to_string(),
        ))
        .await?;
    Ok(())
}

fn belongs(pending: &PendingDialogue, key: SessionKey, user: i64, message_id: i64) -> bool {
    pending.session == key
        && pending.requester == user
        && pending.message_id == message_id
        && !pending.responder.is_closed()
        && !pending.closed.is_cancelled()
}

async fn answer(
    shared: Arc<AppShared>,
    token: String,
    mut pending: PendingDialogue,
    value: String,
) -> Result<()> {
    if pending.closed.is_cancelled() || pending.responder.is_closed() {
        return Ok(());
    }
    interactions::disarm_text(&shared, pending.session, pending.requester).await;
    if !shared
        .store
        .native_request_pending(pending.requester, &token, pending.local_turn_id)?
    {
        shared.pending_questions.lock().await.insert(token, pending);
        return Ok(());
    }
    if let Ok(update_id) = HUMAN_UPDATE_ID.try_with(|id| *id) {
        shared.store.begin_question_answer(
            update_id,
            pending.local_turn_id,
            &pending.request_id,
        )?;
    }
    let final_answer = pending.index + 1 == pending.request.questions.len();
    if final_answer
        && !shared.store.transition_interaction(
            pending.native_session,
            pending.requester,
            &token,
            "pending",
            "answering",
        )?
    {
        shared.pending_questions.lock().await.insert(token, pending);
        return Ok(());
    }
    pending.apply_answer(value);
    if final_answer {
        // Send once. A closed native connection must never cause a new turn/replay.
        let _ = pending
            .responder
            .send(UserInputDecision::Answers(pending.answers));
    } else {
        show_question(&shared, &token, &mut pending).await?;
        if !pending.closed.is_cancelled() && !pending.responder.is_closed() {
            shared.pending_questions.lock().await.insert(token, pending);
        }
    }
    Ok(())
}

pub(super) async fn handle_callback(
    shared: Arc<AppShared>,
    message: &Message,
    user: i64,
    data: &str,
) -> Result<bool> {
    if data == "qpending" {
        show_pending(shared, message, user).await?;
        return Ok(true);
    }
    let page = data
        .strip_prefix("qpage:")
        .map(|v| (v, false))
        .or_else(|| data.strip_prefix("qhistory:").map(|v| (v, true)));
    if let Some((offset, history)) = page {
        if let Ok(offset) = offset.parse::<usize>() {
            if offset <= 1_000_000 && offset % 50 == 0 {
                show_pending_page(shared, message, user, offset, history).await?;
            }
        }
        return Ok(true);
    }
    if let Some(token) = data.strip_prefix("qopen:") {
        let key = SessionKey::new(message.chat.id, message.message_thread_id);
        let mut guard = shared.pending_questions.lock().await;
        let valid = guard.get(token).is_some_and(|p| {
            shared
                .store
                .native_request_pending(p.requester, token, p.local_turn_id)
                .unwrap_or(false)
                && p.session == key
                && p.requester == user
                && !p.closed.is_cancelled()
                && !p.responder.is_closed()
        });
        if !valid {
            drop(guard);
            let opening = shared
                .store
                .interaction(key, user, token)?
                .is_some_and(|i| i.kind == "native" && i.state == "pending");
            notice(&shared,message,if opening {"Question controls are still opening. Use /pending again when delivery completes."}
                else {"This question is no longer pending for you in this topic."}).await?;
        } else {
            let mut pending = guard.remove(token).expect("checked pending question");
            drop(guard);
            show_question(&shared, token, &mut pending).await?;
            if !pending.closed.is_cancelled() && !pending.responder.is_closed() {
                shared
                    .pending_questions
                    .lock()
                    .await
                    .insert(token.to_string(), pending);
            }
        }
        return Ok(true);
    }
    if !data.starts_with("q:") {
        return Ok(false);
    }
    let mut parts = data.split(':');
    let _ = parts.next();
    let token = parts.next().unwrap_or("");
    let index = parts.next().and_then(|s| s.parse::<usize>().ok());
    let action = parts.next().unwrap_or("");
    let key = SessionKey::new(message.chat.id, message.message_thread_id);
    let mut guard = shared.pending_questions.lock().await;
    let valid = guard.get(token).is_some_and(|p| {
        belongs(p, key, user, message.message_id)
            && shared
                .store
                .native_request_pending(p.requester, token, p.local_turn_id)
                .unwrap_or(false)
            && Some(p.index) == index
            && parts.next().is_none()
    });
    if !valid {
        drop(guard);
        notice(
            &shared,
            message,
            "This question is closed or belongs to another user/topic. No answer was sent.",
        )
        .await?;
        return Ok(true);
    }
    if action == "text" {
        for pending in guard
            .values_mut()
            .filter(|p| p.session == key && p.requester == user)
        {
            pending.awaiting_text = false;
        }
        let mut pending = guard.remove(token).expect("checked pending question");
        pending.awaiting_text = true;
        drop(guard);
        interactions::disarm_text(&shared, key, user).await;
        let question = &pending.request.questions[pending.index];
        let prompt = format!(
            "Codex question {}/{}: {}\n\nSend your answer as text, or reply directly to the question. /stop cancels the turn.",
            pending.index + 1,
            pending.request.questions.len(),
            question.header
        );
        let sent = shared
            .telegram
            .send_force_reply(SendMessage::html(
                key.chat_id,
                Some(key.thread_id).filter(|id| *id != 0),
                html_escape::encode_safe(&prompt).to_string(),
            ))
            .await;
        let result = match sent {
            Ok(sent)
                if sent.chat.id == key.chat_id
                    && sent.message_thread_id.unwrap_or(0) == key.thread_id =>
            {
                let recorded = shared.store.record_question_message(key, sent.message_id, pending.local_turn_id,
                    &serde_json::json!({"requestId":pending.request_id,"threadId":pending.request.thread_id,
                        "turnId":pending.request.turn_id,"itemId":pending.request.item_id,"questionId":question.id}));
                pending.message_ids.push(sent.message_id);
                recorded
            }
            Ok(_) => Err(anyhow!(
                "free-text prompt delivery did not confirm its Telegram topic"
            )),
            Err(error) => Err(error),
        };
        if !pending.closed.is_cancelled() && !pending.responder.is_closed() {
            shared
                .pending_questions
                .lock()
                .await
                .insert(token.to_string(), pending);
        }
        result?;
    } else if action == "cancel" {
        let pending = guard.remove(token).expect("checked pending question");
        drop(guard);
        shared.store.transition_interaction(
            pending.native_session,
            user,
            token,
            "pending",
            "cancelled",
        )?;
        let _ = pending.responder.send(UserInputDecision::Cancel);
    } else {
        let value = guard.get(token).and_then(|p| {
            action
                .parse::<usize>()
                .ok()
                .and_then(|i| p.request.questions[p.index].options.as_ref()?.get(i))
                .map(|option| option.label.clone())
        });
        if let Some(value) = value {
            let pending = guard.remove(token).expect("checked pending question");
            drop(guard);
            answer(shared, token.to_string(), pending, value).await?;
        } else {
            drop(guard);
            notice(
                &shared,
                message,
                "Invalid answer control. No answer was sent.",
            )
            .await?;
        }
    }
    Ok(true)
}

pub(super) async fn disarm_text(shared: &AppShared, key: SessionKey, user: i64) {
    for pending in shared
        .pending_questions
        .lock()
        .await
        .values_mut()
        .filter(|p| p.session == key && p.requester == user)
    {
        pending.awaiting_text = false;
    }
}

pub(super) async fn handle_text_answer(
    shared: Arc<AppShared>,
    message: &Message,
    user: i64,
    text: &str,
) -> Result<bool> {
    if !is_text_only_steer_candidate(message, text) || text.starts_with('/') {
        return Ok(false);
    }
    let key = SessionKey::new(message.chat.id, message.message_thread_id);
    let reply = message.reply_to_message.as_ref().map(|m| m.message_id);
    let mut guard = shared.pending_questions.lock().await;
    let token = guard
        .iter()
        .find(|(token, p)| {
            shared
                .store
                .native_request_pending(p.requester, token, p.local_turn_id)
                .unwrap_or(false)
                && p.session == key
                && p.requester == user
                && !p.responder.is_closed()
                && !p.closed.is_cancelled()
                && reply.is_some_and(|id| p.message_ids.contains(&id))
        })
        .map(|(token, _)| token.clone());
    if let Some(token) = token {
        let pending = guard.remove(&token).expect("selected pending question");
        drop(guard);
        answer(shared, token, pending, text.to_string()).await?;
        return Ok(true);
    }
    drop(guard);
    if let Some(reply) = reply {
        // Telegram can display a question before its HTTP receipt reaches SQLite.
        // A crash in that gap must not turn a quoted answer into a fresh prompt.
        let unrecorded_question = message.reply_to_message.as_ref().is_some_and(|quoted| {
            quoted
                .from
                .as_ref()
                .is_some_and(|from| from.is_bot && from.id == shared.bot_id)
                && quoted
                    .text
                    .as_deref()
                    .is_some_and(|text| text.starts_with("Codex question "))
        });
        if shared.store.is_question_message(key, reply)? || unrecorded_question {
            notice(&shared,message,"This question is no longer waiting for your answer. No new turn was started; inspect /status before continuing.").await?;
            return Ok(true);
        }
    }
    // Arming explicitly selects the next text. Telegram topic clients can
    // attach a root-message reply even when the human did not quote a question.
    // Recognized expired questions above must never consume this capture.
    let mut guard = shared.pending_questions.lock().await;
    let token = guard
        .iter()
        .find(|(token, p)| {
            shared
                .store
                .native_request_pending(p.requester, token, p.local_turn_id)
                .unwrap_or(false)
                && p.session == key
                && p.requester == user
                && p.awaiting_text
                && !p.closed.is_cancelled()
                && !p.responder.is_closed()
        })
        .map(|(token, _)| token.clone());
    if let Some(token) = token {
        let pending = guard.remove(&token).expect("armed question");
        drop(guard);
        answer(shared, token, pending, text.to_string()).await?;
        return Ok(true);
    }
    Ok(false)
}

pub(super) async fn show_pending(
    shared: Arc<AppShared>,
    message: &Message,
    user: i64,
) -> Result<()> {
    show_pending_page(shared, message, user, 0, false).await
}

fn request_button_label(state: &str, title: &str) -> String {
    let mut short = title.chars().take(40).collect::<String>();
    if title.chars().count() > 40 {
        short.push('…');
    }
    format!("{state} · {short}")
}
fn request_state_label(state: &str, spanish: bool) -> &'static str {
    match (state, spanish) {
        ("pending", true) => "⏳ Pendiente",
        ("pending", false) => "⏳ Pending",
        ("answering", true) => "⌛ Confirmando",
        ("answering", false) => "⌛ Receipt pending",
        ("unknown", true) => "⚠️ Sin confirmar",
        ("unknown", false) => "⚠️ Unknown receipt",
        ("source_changed", true) => "↪️ Fuente cambió",
        ("source_changed", false) => "↪️ Source changed",
        ("answered", true) => "💬 Respondido",
        ("answered", false) => "💬 Answered",
        ("completed", true) => "✅ Completado",
        ("completed", false) => "✅ Completed",
        ("withdrawn", true) => "↩️ Retirado",
        ("withdrawn", false) => "↩️ Withdrawn",
        ("cancelled", true) => "◻️ Cancelado",
        ("cancelled", false) => "◻️ Cancelled",
        ("expired", true) => "⌛ Vencido",
        ("expired", false) => "⌛ Expired",
        (_, true) => "⚠️ Comprobar",
        (_, false) => "⚠️ Inspect",
    }
}

async fn show_pending_page(
    shared: Arc<AppShared>,
    message: &Message,
    user: i64,
    offset: usize,
    history: bool,
) -> Result<()> {
    let key = SessionKey::new(message.chat.id, message.message_thread_id);
    let spanish = matches!(
        shared.config.telegram.human_request_language,
        crate::config::HumanRequestLanguage::Es
    );
    let live = {
        let guard = shared.pending_questions.lock().await;
        guard
            .iter()
            .filter(|(_, p)| {
                p.session == key
                    && p.requester == user
                    && !p.closed.is_cancelled()
                    && !p.responder.is_closed()
            })
            .map(|(id, p)| {
                (
                    id.clone(),
                    format!(
                        "{} ({}/{})",
                        p.request.questions[p.index].header,
                        p.index + 1,
                        p.request.questions.len()
                    ),
                )
            })
            .collect::<HashMap<_, _>>()
    };
    let page = shared.store.interactions_page(key, user, offset, history)?;
    let total = if history {
        page.history_total
    } else {
        page.actionable_total
    };
    let mut rows = Vec::new();
    for item in page.items.iter().take(50) {
        let (state, title, callback) =
            if let Some(title) = live.get(&item.id).filter(|_| item.state == "pending") {
                (
                    if spanish {
                        if item.blocking {
                            "⏳ Bloqueante"
                        } else {
                            "⏳ No bloqueante"
                        }
                    } else if item.blocking {
                        "⏳ Blocking"
                    } else {
                        "⏳ Nonblocking"
                    },
                    title.as_str(),
                    format!("qopen:{}", item.id),
                )
            } else if item.kind == "native" && item.state == "pending" {
                (
                    if spanish {
                        "⏳ Abriendo controles"
                    } else {
                        "⏳ Opening controls"
                    },
                    item.title.as_str(),
                    format!("qopen:{}", item.id),
                )
            } else {
                (
                    request_state_label(&item.state, spanish),
                    item.title.as_str(),
                    format!("hp:{}:open", item.id),
                )
            };
        rows.push(vec![InlineKeyboardButton {
            text: request_button_label(state, title),
            callback_data: Some(callback),
            url: None,
        }]);
    }
    let mut nav = Vec::new();
    let prefix = if history { "qhistory" } else { "qpage" };
    for (label, next) in [
        (
            if spanish { "Anterior" } else { "Previous" },
            offset.checked_sub(50),
        ),
        (
            if spanish { "Siguiente" } else { "Next" },
            (page.items.len() > 50).then_some(offset + 50),
        ),
    ] {
        if let Some(next) = next {
            nav.push(InlineKeyboardButton {
                text: label.into(),
                callback_data: Some(format!("{prefix}:{next}")),
                url: None,
            });
        }
    }
    if !nav.is_empty() {
        rows.push(nav);
    }
    rows.push(vec![InlineKeyboardButton {
        text: if history {
            format!(
                "{} ({})",
                if spanish { "Pendientes" } else { "Pending" },
                page.actionable_total
            )
        } else {
            format!(
                "{} ({})",
                if spanish { "Historial" } else { "History" },
                page.history_total
            )
        },
        callback_data: Some(if history { "qpending" } else { "qhistory:0" }.into()),
        url: None,
    }]);
    let showing = page.items.len().min(50);
    let start = if showing == 0 { 0 } else { offset + 1 };
    let end = if showing == 0 { 0 } else { offset + showing };
    let text = if history {
        if spanish {
            format!(
                "Historial · {total} registros en este topic\nMostrando {start}–{end}. {pending} solicitudes sin resolver.\nLos registros cerrados son de consulta; no se reenvían.",
                pending = page.actionable_total
            )
        } else {
            format!(
                "History · {total} records in this topic\nShowing {start}–{end}. {pending} unresolved requests.\nClosed records are read-only; inputs are never replayed.",
                pending = page.actionable_total
            )
        }
    } else {
        let receipts = page.actionable_total - page.pending_total;
        let empty = if total == 0 {
            if spanish {
                "\nNo hay solicitudes pendientes para vos en este topic."
            } else {
                "\nNo pending requests for you in this topic."
            }
        } else {
            ""
        };
        if spanish {
            format!(
                "Pendientes · {} para vos en este topic\n{} recibos por comprobar · Mostrando {start}–{end} de {total}.{empty}",
                page.pending_total, receipts
            )
        } else {
            format!(
                "Human requests · {} pending for you in this topic\n{} receipts to inspect · Showing {start}–{end} of {total}.{empty}",
                page.pending_total, receipts
            )
        }
    };
    let mut outgoing = SendMessage::html(message.chat.id, message.message_thread_id, text);
    outgoing.reply_markup = Some(InlineKeyboardMarkup {
        inline_keyboard: rows,
    });
    shared.telegram.send_message(outgoing).await?;
    Ok(())
}

#[cfg(test)]
mod mobile_tests {
    use super::*;
    #[test]
    fn long_titles_never_hide_lifecycle_on_mobile() {
        for spanish in [true, false] {
            for state in [
                "pending",
                "answered",
                "completed",
                "cancelled",
                "withdrawn",
                "unknown",
            ] {
                let prefix = request_state_label(state, spanish);
                let label =
                    request_button_label(prefix, &"Título largo con evidencia 🌿 ".repeat(10));
                assert!(label.starts_with(prefix));
                assert!(label.contains("Título largo"));
                assert!(label.ends_with('…'));
                assert!(label.chars().count() < 80);
            }
        }
    }
}
