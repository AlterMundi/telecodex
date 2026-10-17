//! Explicit native goal commands. Widgets are limited to edit and replacement.
use super::*;
use crate::codex::{GoalActivation, GoalControl, NativeGoal};
use serde_json::json;

const USAGE: &str = "Usage: /goal [<objective>|clear|edit [<objective>]|pause|resume]";
const EDIT_TITLE: &str = "Edit native goal";
const TTL: Duration = Duration::from_secs(15 * 60);

#[derive(Clone)]
pub(super) struct PendingGoalControl {
    key: SessionKey,
    requester: i64,
    message_id: i64,
    created: Instant,
    native_thread: String,
    expected: NativeGoal,
    objective: Option<String>,
}
impl PendingGoalControl {
    fn matches(&self, message: &Message, requester: i64) -> bool {
        self.key == SessionKey::new(message.chat.id, message.message_thread_id)
            && self.requester == requester
            && self.created.elapsed() < TTL
    }
}
fn summary(goal: &NativeGoal) -> String {
    let seconds = goal.time_used_seconds.max(0);
    let mut text = format!(
        "Goal: {}\n\nObjective: {}\n\nTime used: {}h {}m {}s\nTokens used: {}",
        goal.status,
        goal.objective,
        seconds / 3600,
        seconds / 60 % 60,
        seconds % 60,
        goal.tokens_used
    );
    if let Some(budget) = goal.token_budget {
        text.push_str(&format!("\nToken budget: {budget}"));
    }
    text.push_str("\n\n/goal edit · /goal clear");
    match goal.status.as_str() {
        "active" => text.push_str(" · /goal pause"),
        "paused" | "blocked" | "usageLimited" => text.push_str(" · /goal resume"),
        _ => {}
    }
    text
}
fn objective_arg(args: &str) -> Option<&str> {
    args.strip_prefix("edit ")
        .map(str::trim)
        .filter(|v| !v.is_empty())
}
fn goal_hint(objective: &str) -> String {
    let text: String = objective.chars().take(200).collect();
    if objective.chars().count() > 200 {
        format!("{text}…")
    } else {
        text
    }
}

pub(super) async fn handle_command(
    app: &App,
    message: &Message,
    requester: i64,
    args: &str,
) -> Result<()> {
    if let Err(error) = command(app, message, requester, args).await {
        app.send_plain_status(message.chat.id,message.message_thread_id,
            &format!("Goal command failed: {error:#}\nInspect /goal before retrying an unconfirmed change.")).await?;
    }
    Ok(())
}
async fn command(app: &App, message: &Message, requester: i64, args: &str) -> Result<()> {
    if is_primary_forum_dashboard(&app.shared.config, &message.chat, message.message_thread_id) {
        return app
            .send_plain_status(
                message.chat.id,
                message.message_thread_id,
                "Open a work topic to use /goal.",
            )
            .await;
    }
    let key = SessionKey::new(message.chat.id, message.message_thread_id);
    let session = app.ensure_resolved_session(key, requester)?;
    let current = match session.codex_thread_id.as_deref() {
        Some(thread) => app.shared.codex.get_goal(thread).await?,
        None => None,
    };
    match args {
        "" => {
            app.send_plain_status(
                key.chat_id,
                message.message_thread_id,
                &current
                    .as_ref()
                    .map(summary)
                    .unwrap_or_else(|| format!("No goal currently set.\n\n{USAGE}")),
            )
            .await
        }
        "clear" => {
            revoke_queued(app, key)?;
            if control_live(app, &session, GoalControl::Clear).await? {
                app.shared
                    .pending_goal_controls
                    .lock()
                    .await
                    .retain(|_, p| p.key != key);
                return app
                    .send_plain_status(message.chat.id, message.message_thread_id, "Goal cleared.")
                    .await;
            }
            let cleared = if let Some(thread) = session.codex_thread_id.as_deref() {
                app.shared.codex.clear_goal(thread).await?
            } else {
                false
            };
            app.shared
                .pending_goal_controls
                .lock()
                .await
                .retain(|_, p| p.key != key);
            app.send_plain_status(
                key.chat_id,
                message.message_thread_id,
                if cleared {
                    "Goal cleared."
                } else {
                    "No goal to clear."
                },
            )
            .await
        }
        "pause" => {
            revoke_queued(app, key)?;
            if control_live(app, &session, GoalControl::Pause).await? {
                let goal = app
                    .shared
                    .codex
                    .get_goal(
                        session
                            .codex_thread_id
                            .as_deref()
                            .context("native goal binding missing")?,
                    )
                    .await?
                    .context("No goal currently set")?;
                return app
                    .send_plain_status(message.chat.id, message.message_thread_id, &summary(&goal))
                    .await;
            }
            let thread = session
                .codex_thread_id
                .as_deref()
                .context("No goal currently set")?;
            let goal = app
                .shared
                .codex
                .set_goal(thread, json!({"status":"paused"}))
                .await?;
            app.send_plain_status(key.chat_id, message.message_thread_id, &summary(&goal))
                .await
        }
        "resume" => {
            let goal = current.context("No goal currently set")?;
            if matches!(goal.status.as_str(), "complete" | "budgetLimited") {
                bail!(
                    "This goal is terminal. Edit its objective or create a new goal instead of resuming it."
                );
            }
            activate(app, message, requester, &session, Some(goal), None, false).await
        }
        "edit" => {
            let goal = current.context("No goal currently set")?;
            let text = format!(
                "{EDIT_TITLE}\n\n{}\n\nReply to this message with the new objective. Ordinary messages keep steering. Cancel by ignoring this prompt; it expires in 15 minutes.",
                goal_hint(&goal.objective)
            );
            let outgoing = SendMessage::html(
                key.chat_id,
                message.message_thread_id,
                html_escape::encode_safe(&text).to_string(),
            );
            let receipt = app.shared.telegram.send_force_reply(outgoing).await?;
            remember(app, &receipt, requester, goal, None).await
        }
        _ => {
            if let Some(objective) = objective_arg(args) {
                edit(
                    app,
                    message,
                    requester,
                    &session,
                    current.context("No goal currently set")?,
                    objective,
                )
                .await
            } else {
                let mut objective = args.to_string();
                // Text documents can hold objectives beyond Telegram's message limit.
                if message.document.is_some() {
                    let attachments = app.download_attachments(message, &session).await?;
                    for attachment in &attachments {
                        if attachment.kind != AttachmentKind::Text {
                            bail!(
                                "Goal documents must be text files; resend an ordinary attachment separately."
                            );
                        }
                        if fs::metadata(&attachment.path)?.len() > 1024 * 1024 {
                            bail!("Goal objective text files are limited to 1 MiB.");
                        }
                        objective.push_str("\n\n");
                        objective.push_str(&fs::read_to_string(&attachment.path)?);
                    }
                }
                if let Some(goal) = current.clone().filter(|g| g.status != "complete") {
                    let token = Uuid::now_v7().simple().to_string();
                    let mut outgoing=SendMessage::html(key.chat_id,message.message_thread_id,
                        html_escape::encode_safe(&format!("Replace the current goal?\n\nNew objective: {}\n\nReplacement starts fresh accounting. Use /goal edit to preserve it.",goal_hint(&objective))).to_string());
                    outgoing.reply_markup = Some(InlineKeyboardMarkup {
                        inline_keyboard: vec![vec![
                            InlineKeyboardButton {
                                text: "Replace goal".into(),
                                callback_data: Some(format!("goal:{token}:replace")),
                                url: None,
                            },
                            InlineKeyboardButton {
                                text: "Cancel".into(),
                                callback_data: Some(format!("goal:{token}:cancel")),
                                url: None,
                            },
                        ]],
                    });
                    let receipt = app.shared.telegram.send_message(outgoing).await?;
                    remember_token(app, &receipt, requester, goal, Some(objective), token).await
                } else {
                    let replacement = current;
                    let replace = replacement.is_some();
                    activate(
                        app,
                        message,
                        requester,
                        &session,
                        replacement,
                        Some(objective),
                        replace,
                    )
                    .await
                }
            }
        }
    }
}
async fn remember(
    app: &App,
    message: &Message,
    requester: i64,
    goal: NativeGoal,
    objective: Option<String>,
) -> Result<()> {
    remember_token(
        app,
        message,
        requester,
        goal,
        objective,
        Uuid::now_v7().simple().to_string(),
    )
    .await
}
async fn remember_token(
    app: &App,
    message: &Message,
    requester: i64,
    goal: NativeGoal,
    objective: Option<String>,
    token: String,
) -> Result<()> {
    let key = SessionKey::new(message.chat.id, message.message_thread_id);
    let current = app
        .shared
        .store
        .get_session(key)?
        .context("goal destination session missing")?;
    if message.from.as_ref().map(|u| u.id) != Some(app.shared.bot_id)
        || current.codex_thread_id.as_deref() != Some(goal.thread_id.as_str())
    {
        bail!("goal control delivery did not confirm its destination");
    }
    let mut controls = app.shared.pending_goal_controls.lock().await;
    controls.retain(|_, p| p.created.elapsed() < TTL);
    if controls.len() >= 64 {
        bail!("too many pending goal controls; use /goal edit <objective> instead");
    }
    controls.insert(
        token,
        PendingGoalControl {
            key,
            requester,
            message_id: message.message_id,
            created: Instant::now(),
            native_thread: goal.thread_id.clone(),
            expected: goal,
            objective,
        },
    );
    Ok(())
}
async fn revalidate(app: &App, p: &PendingGoalControl) -> Result<crate::models::SessionRecord> {
    let session = app.ensure_resolved_session(p.key, p.requester)?;
    if session.codex_thread_id.as_deref() != Some(p.native_thread.as_str()) {
        bail!("The topic's native session changed. Open /goal again.");
    }
    let goal = app
        .shared
        .codex
        .get_goal(&p.native_thread)
        .await?
        .context("The goal was cleared. Open /goal again.")?;
    if !goal.same_definition(&p.expected) {
        bail!("The goal changed. Open /goal again.");
    }
    Ok(session)
}
async fn edit(
    app: &App,
    message: &Message,
    requester: i64,
    session: &crate::models::SessionRecord,
    expected: NativeGoal,
    objective: &str,
) -> Result<()> {
    let objective = materialize(app, objective)?;
    if expected.edited_status() == "active" {
        activate(
            app,
            message,
            requester,
            session,
            Some(expected),
            Some(objective),
            false,
        )
        .await
    } else {
        let goal=app.shared.codex.set_goal(&expected.thread_id,json!({"objective":objective,"status":expected.edited_status(),"tokenBudget":expected.token_budget})).await?;
        app.send_plain_status(message.chat.id, message.message_thread_id, &summary(&goal))
            .await
    }
}
async fn activate(
    app: &App,
    message: &Message,
    requester: i64,
    session: &crate::models::SessionRecord,
    expected: Option<NativeGoal>,
    objective: Option<String>,
    replace: bool,
) -> Result<()> {
    if app.shared.store.stop_fenced(session.key)? {
        bail!("Coordinated Stop holds this goal; no activation submitted");
    }
    let objective = objective
        .as_deref()
        .map(|s| materialize(app, s))
        .transpose()?;
    // A worker is already subscribed during live work. Metadata changes do not
    // interrupt its current turn; its event loop follows native continuation.
    let attached = app
        .workers
        .lock()
        .await
        .get(&session.key)
        .and_then(|w| w.steer.lock().expect("steer mutex poisoned").clone());
    if let Some(attached) = attached {
        let activation = GoalActivation {
            intent: Uuid::now_v7().to_string(),
            native_thread: session.codex_thread_id.clone(),
            expected,
            objective,
            replace,
        };
        let (response, ack) = oneshot::channel();
        attached.sender.send(CodexSteerRequest{client_message_id:None,text:String::new(),goal:Some(GoalControl::Activate(activation)),response})
            .map_err(|_|anyhow!("The goal subscriber just finished. Inspect /goal and request the change again; it was not queued."))?;
        match tokio::time::timeout(Duration::from_secs(12), ack).await {
            Ok(Ok(Ok(_))) => {}
            Ok(Ok(Err(error))) => bail!("Native goal update was not confirmed: {error:?}"),
            _ => bail!(
                "Native goal update acknowledgment unavailable; inspect /goal before retrying"
            ),
        }
        let goal = app
            .shared
            .codex
            .get_goal(
                session
                    .codex_thread_id
                    .as_deref()
                    .context("native goal binding missing")?,
            )
            .await?;
        app.send_plain_status(
            message.chat.id,
            message.message_thread_id,
            &goal
                .as_ref()
                .map(summary)
                .unwrap_or_else(|| "Goal already completed or cleared.".into()),
        )
        .await
    } else {
        if !app
            .ensure_codex_authenticated(message.chat.id, message.message_thread_id)
            .await?
        {
            return Ok(());
        }
        let intent = Uuid::now_v7().to_string();
        app.shared
            .store
            .save_bot_state(&intent_key(session.key), &intent)?;
        let activation = GoalActivation {
            intent,
            native_thread: session.codex_thread_id.clone(),
            expected,
            objective,
            replace,
        };
        let prompt="Continue the native goal requested by the human. Use its native state and account for progress there.".to_string();
        app.enqueue_goal_turn(
            TurnRequest {
                session_key: session.key,
                from_user_id: requester,
                prompt,
                runtime_instructions: None,
                attachments: vec![],
                review_mode: None,
                override_search_mode: None,
            },
            &message.chat.kind,
            Some(activation),
        )
        .await?;
        app.send_plain_status(message.chat.id,message.message_thread_id,"Starting native goal work. /goal shows its state; /goal pause stops continuation and /goal clear removes it. Neither interrupts a turn already running.").await
    }
}
fn materialize(app: &App, objective: &str) -> Result<String> {
    let objective = objective.trim();
    if objective.is_empty() {
        bail!("A goal objective cannot be empty");
    }
    if objective.chars().count() <= 4000 {
        return Ok(objective.to_string());
    }
    let root = app
        .shared
        .config
        .db_path
        .parent()
        .context("database has no private storage parent")?
        .join("goals");
    fs::create_dir_all(&root)?;
    if fs::symlink_metadata(&root)?.file_type().is_symlink() {
        bail!("goal storage cannot be a symlink");
    }
    let directory = root.join(Uuid::now_v7().to_string());
    fs::create_dir(&directory)?;
    let path = directory.join("objective.txt");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))?;
    }
    fs::write(&path, objective)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    }
    Ok(format!(
        "Read the complete goal objective at {} before continuing.",
        path.display()
    ))
}
pub(super) async fn handle_edit_reply(
    app: &App,
    message: &Message,
    requester: i64,
    text: &str,
) -> Result<bool> {
    let Some(reply) = message.reply_to_message.as_ref() else {
        return Ok(false);
    };
    if reply.from.as_ref().map(|u| u.id) != Some(app.shared.bot_id) {
        return Ok(false);
    }
    let control = {
        app.shared
            .pending_goal_controls
            .lock()
            .await
            .iter()
            .find(|(_, p)| {
                p.objective.is_none()
                    && p.message_id == reply.message_id
                    && p.matches(message, requester)
            })
            .map(|(t, p)| (t.clone(), p.clone()))
    };
    let Some((token, p)) = control else {
        if reply
            .text
            .as_deref()
            .is_some_and(|s| s.starts_with(EDIT_TITLE))
        {
            app.send_plain_status(message.chat.id,message.message_thread_id,"This goal editor expired or belongs to another user/topic. Open /goal edit again. No turn was started.").await?;
            return Ok(true);
        }
        return Ok(false);
    };
    if text.starts_with('/') {
        return Ok(false);
    }
    let result = async {
        if text.trim().is_empty()
            || message.document.is_some()
            || !message.photo.is_empty()
            || message.sticker.is_some()
        {
            bail!("Reply to the goal editor with a text objective.");
        }
        let session = revalidate(app, &p).await?;
        edit(app, message, requester, &session, p.expected, text).await
    }
    .await;
    if result.is_ok() {
        app.shared.pending_goal_controls.lock().await.remove(&token);
    }
    if let Err(e) = result {
        app.send_plain_status(
            message.chat.id,
            message.message_thread_id,
            &format!("Goal edit failed: {e:#}"),
        )
        .await?;
    }
    Ok(true)
}
pub(super) async fn handle_callback(
    app: &App,
    message: &Message,
    requester: i64,
    data: &str,
) -> Result<bool> {
    let Some(rest) = data.strip_prefix("goal:") else {
        return Ok(false);
    };
    let (token, action) = rest.split_once(':').unwrap_or((rest, ""));
    let control = app
        .shared
        .pending_goal_controls
        .lock()
        .await
        .get(token)
        .cloned();
    let Some(p) = control.filter(|p| {
        p.matches(message, requester) && p.message_id == message.message_id && p.objective.is_some()
    }) else {
        app.send_plain_status(
            message.chat.id,
            message.message_thread_id,
            "This goal control expired or belongs to another user/topic. Open /goal again.",
        )
        .await?;
        return Ok(true);
    };
    if !matches!(action, "cancel" | "replace") {
        return Ok(true);
    }
    app.shared.pending_goal_controls.lock().await.remove(token);
    let result = async {
        if action == "cancel" {
            return app
                .send_plain_status(
                    message.chat.id,
                    message.message_thread_id,
                    "Goal replacement cancelled.",
                )
                .await;
        }
        let session = revalidate(app, &p).await?;
        activate(
            app,
            message,
            requester,
            &session,
            Some(p.expected),
            p.objective,
            true,
        )
        .await
    }
    .await;
    if let Err(e) = result {
        app.send_plain_status(
            message.chat.id,
            message.message_thread_id,
            &format!("Goal replacement failed: {e:#}"),
        )
        .await?;
    }
    Ok(true)
}

pub(super) fn intent_key(key: SessionKey) -> String {
    format!("goal_intent:{}:{}", key.chat_id, key.thread_id)
}
fn revoke_queued(app: &App, key: SessionKey) -> Result<()> {
    app.shared
        .store
        .save_bot_state(&intent_key(key), &Uuid::now_v7().to_string())
}
pub(super) async fn control_live(
    app: &App,
    session: &crate::models::SessionRecord,
    control: GoalControl,
) -> Result<bool> {
    let attached = app
        .workers
        .lock()
        .await
        .get(&session.key)
        .and_then(|w| w.steer.lock().expect("steer mutex poisoned").clone());
    let Some(attached) = attached else {
        return Ok(false);
    };
    let (response, ack) = oneshot::channel();
    attached
        .sender
        .send(CodexSteerRequest {
            client_message_id: None,
            text: String::new(),
            goal: Some(control),
            response,
        })
        .map_err(|_| anyhow!("Goal subscriber finished; inspect /goal before retrying"))?;
    match tokio::time::timeout(Duration::from_secs(12), ack).await {
        Ok(Ok(Ok(_))) => Ok(true),
        Ok(Ok(Err(error))) => bail!("Goal mutation was not confirmed: {error:?}"),
        _ => bail!("Goal acknowledgment unavailable; inspect /goal before retrying"),
    }
}
