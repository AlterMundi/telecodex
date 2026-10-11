//! Explicit durable human tasks share the native question entry point.

use super::*;
use crate::store::HumanInteraction;
use serde::Deserialize;

pub(super) const MANIFEST: &str = "human-requests.json";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Declaration {
    schema: String,
    requests: Vec<Task>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Task {
    id: String,
    title: String,
    action: String,
    #[serde(default)]
    links: Vec<Link>,
}

#[derive(Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
struct Link {
    label: String,
    url: String,
}

pub(super) fn register_tasks(
    shared: &AppShared,
    session: SessionKey,
    user: i64,
    turn: i64,
    native: &str,
    root: &Path,
) -> Result<()> {
    let path = root.join(MANIFEST);
    let meta = match fs::symlink_metadata(&path) {
        Ok(meta) => meta,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    if !meta.file_type().is_file() || meta.len() > 65_536 {
        bail!("human request declaration must be a regular file of at most 64 KiB");
    }
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    use std::io::Read;
    let mut bytes = Vec::new();
    let file = options.open(&path)?;
    if !file.metadata()?.is_file() {
        bail!("human request declaration is not a regular file");
    }
    file.take(65_537).read_to_end(&mut bytes)?;
    if bytes.len() > 65_536 {
        bail!("human request declaration exceeds 64 KiB");
    }
    let declaration: Declaration =
        serde_json::from_slice(&bytes).map_err(|_| anyhow!("invalid human request declaration"))?;
    if declaration.schema != "telecodex.human-requests/v1" || declaration.requests.len() > 10 {
        bail!("unsupported human request declaration schema or count");
    }
    let mut ids = std::collections::HashSet::new();
    let mut tasks = Vec::new();
    for task in declaration.requests {
        if task.id.is_empty()
            || task.id.len() > 64
            || !task
                .id
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
            || !ids.insert(task.id.clone())
            || task.title.trim().is_empty()
            || task.title.chars().count() > 120
            || task.action.trim().is_empty()
            || task.action.chars().count() > 8_000
            || task.links.len() > 5
        {
            bail!("unsupported human request ID, title, action or links");
        }
        for link in &task.links {
            let url = reqwest::Url::parse(&link.url).context("invalid human request link")?;
            if !matches!(url.scheme(), "http" | "https")
                || !url.username().is_empty()
                || url.password().is_some()
                || link.url.len() > 2_048
                || link.label.trim().is_empty()
                || link.label.chars().count() > 80
            {
                bail!("unsupported human request link");
            }
        }
        tasks.push((
            task.id,
            HumanInteraction {
                id: Uuid::now_v7().simple().to_string(),
                kind: "task".into(),
                state: "pending".into(),
                requester: user,
                native_thread: native.into(),
                local_turn: turn,
                title: task.title,
                action: task.action,
                blocking: false,
                source: serde_json::json!({"links":task.links}),
            },
        ));
    }
    shared.store.save_human_tasks(session, &tasks)?;
    Ok(())
}

async fn send_item(
    shared: &AppShared,
    message: &Message,
    item: &HumanInteraction,
    reply: bool,
) -> Result<()> {
    let kind = if item.kind == "native" {
        if item.blocking {
            "native question · blocking"
        } else {
            "native question · nonblocking"
        }
    } else {
        "human task · survives the turn"
    };
    let live = if item.kind == "native" && item.state == "pending" {
        shared
            .pending_questions
            .lock()
            .await
            .get(&item.id)
            .is_some_and(|p| !p.closed.is_cancelled() && !p.responder.is_closed())
    } else {
        true
    };
    let state = match if live {
        item.state.as_str()
    } else {
        "controls are opening"
    } {
        "source_changed" => "source session changed",
        "unknown" => "outcome unknown",
        "answering" => "answer receipt pending",
        s => s,
    };
    let mut text = format!(
        "Human request: {}\n\n{}\n\n{} · {}",
        item.title, item.action, kind, state
    );
    if let Some(delivery) = item.source["delivery"].as_str() {
        text.push_str(&format!("\nFeedback delivery: {delivery}. Answered means your feedback was recorded, not that the model has accepted it."));
    }
    let mut rows = Vec::new();
    for link in item.source["links"].as_array().into_iter().flatten() {
        if let (Some(label), Some(url)) = (link["label"].as_str(), link["url"].as_str()) {
            rows.push(vec![InlineKeyboardButton {
                text: label.into(),
                url: Some(url.into()),
                callback_data: None,
            }]);
        }
    }
    if item.kind == "task" && item.state == "pending" {
        rows.push(vec![InlineKeyboardButton {
            text: "Reply with feedback".into(),
            url: None,
            callback_data: Some(format!("hp:{}:reply", item.id)),
        }]);
        rows.push(vec![
            InlineKeyboardButton {
                text: "Done".into(),
                url: None,
                callback_data: Some(format!("hp:{}:done", item.id)),
            },
            InlineKeyboardButton {
                text: "Cancel request".into(),
                url: None,
                callback_data: Some(format!("hp:{}:cancel", item.id)),
            },
        ]);
    }
    let chunks = split_text(&text, 1800);
    let count = chunks.len();
    for (index, chunk) in chunks.into_iter().enumerate() {
        let chunk = if index == 0 {
            chunk
        } else {
            format!("Human request: {} (continued)\n\n{chunk}", item.title)
        };
        let mut outgoing = SendMessage::html(
            message.chat.id,
            message.message_thread_id,
            html_escape::encode_safe(&chunk).to_string(),
        );
        if index + 1 == count && !reply && !rows.is_empty() {
            outgoing.reply_markup = Some(InlineKeyboardMarkup {
                inline_keyboard: rows.clone(),
            });
        }
        let sent = if reply && index + 1 == count {
            shared.telegram.send_force_reply(outgoing).await?
        } else {
            shared.telegram.send_message(outgoing).await?
        };
        if sent.chat.id != message.chat.id
            || sent.message_thread_id.unwrap_or(0) != message.message_thread_id.unwrap_or(0)
        {
            bail!("human request delivery did not confirm its topic");
        }
        shared.store.record_interaction_message(
            SessionKey::new(message.chat.id, message.message_thread_id),
            sent.message_id,
            &item.id,
        )?;
    }
    Ok(())
}

pub(super) async fn handle_callback(
    shared: &AppShared,
    message: &Message,
    user: i64,
    data: &str,
) -> Result<bool> {
    let Some(rest) = data.strip_prefix("hp:") else {
        return Ok(false);
    };
    let Some((id, action)) = rest.split_once(':') else {
        return Ok(true);
    };
    let key = SessionKey::new(message.chat.id, message.message_thread_id);
    let Some(item) = shared.store.interaction(key, user, id)? else {
        questions::notice(
            shared,
            message,
            "This request is not available for you in this topic.",
        )
        .await?;
        return Ok(true);
    };
    if action == "open" {
        send_item(shared, message, &item, false).await?;
        return Ok(true);
    }
    if item.kind != "task" || item.state != "pending" {
        questions::notice(
            shared,
            message,
            "This request cannot accept that action. Use /pending to inspect its current state.",
        )
        .await?;
        return Ok(true);
    }
    match action {
        "reply" => send_item(shared, message, &item, true).await?,
        "done" | "cancel" => {
            let state = if action == "done" {
                "completed"
            } else {
                "cancelled"
            };
            if shared
                .store
                .transition_interaction(key, user, id, "pending", state)?
            {
                questions::notice(
                    shared,
                    message,
                    &format!("Human request {state}. No new model turn was started."),
                )
                .await?;
            }
        }
        _ => {}
    }
    Ok(true)
}

pub(super) enum TextAnswer {
    None,
    Handled,
    Forward(String),
}

pub(super) async fn handle_text(
    shared: &AppShared,
    message: &Message,
    user: i64,
    text: &str,
) -> Result<TextAnswer> {
    if !is_text_only_steer_candidate(message, text) || text.is_empty() || text.starts_with('/') {
        return Ok(TextAnswer::None);
    }
    let Some(quoted) = message.reply_to_message.as_ref() else {
        return Ok(TextAnswer::None);
    };
    let key = SessionKey::new(message.chat.id, message.message_thread_id);
    let id = shared.store.interaction_message(key, quoted.message_id)?;
    let recognized = quoted
        .from
        .as_ref()
        .is_some_and(|f| f.is_bot && f.id == shared.bot_id)
        && quoted
            .text
            .as_deref()
            .is_some_and(|t| t.starts_with("Human request: "));
    if id.is_none() && !recognized {
        return Ok(TextAnswer::None);
    }
    let item = match id {
        Some(id) => shared.store.interaction(key, user, &id)?,
        None => None,
    };
    if let Some(item) = item.filter(|i| i.kind == "task" && i.state == "pending") {
        if shared.store.answer_human_task(
            key,
            user,
            &item.id,
            HUMAN_UPDATE_ID
                .try_with(|id| *id)
                .context("human task answer has no journal update")?,
        )? {
            return Ok(TextAnswer::Forward(format!(
                "[Feedback for human request: {}]\n{}",
                item.title, text
            )));
        }
    }
    questions::notice(shared,message,"This request has no current answer target. Use /pending to reopen it. No new turn was started.").await?;
    Ok(TextAnswer::Handled)
}
