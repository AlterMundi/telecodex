//! A finite, human-invoked Stop coordinator, independent of any model turn.
use super::*;
use crate::{
    codex::GoalControl,
    store::{StopOperation, StopTarget},
};
use serde_json::{Value, json};

pub(super) fn input_epoch(shared: &AppShared, key: SessionKey) -> Result<String> {
    match HUMAN_UPDATE_ID.try_with(|id| *id) {
        Ok(id) => shared
            .store
            .bot_state_value(&format!("stop_input_epoch:{id}"))?
            .context("Journaled input admission epoch missing"),
        Err(_) => shared.store.stop_admission_epoch(key),
    }
}
pub(super) async fn command(
    app: &App,
    human: i64,
    args: &str,
    nonce: Option<String>,
) -> Result<Value> {
    if let Some(selected) = args.strip_prefix("controller ") {
        return controller::command(app, human, selected).await;
    }
    if let Some(days) = args.strip_prefix("topics list ") {
        return app.shared.store.topic_inventory(human, days.parse()?);
    }
    if let Some(selected) = args.strip_prefix("observer ") {
        return observer::owner_command(app, human, selected);
    }
    if let Some(selected) = args.strip_prefix("matrix ") {
        return matrix::owner(app, human, selected);
    }
    if let Some(selected) = args.strip_prefix("presentation ") {
        return presentation::owner_read(app, human, selected);
    }
    if let Some(selected) = args.strip_prefix("requests ") {
        return conversation_requests::command(app, human, selected).await;
    }
    if let Some(selected) = args.strip_prefix("input ") {
        return conversation_inputs::command(app, human, selected).await;
    }
    if let Some(selected) = args.strip_prefix("attach-active ") {
        return goals::attach_active(app, human, selected).await;
    }
    if !app.shared.config.codex.shared_app_server {
        bail!(
            "Coordinated Stop requires the receiving shared native App Server; no replacement was started"
        );
    }
    if args == "all" {
        let (op, fresh) = app
            .shared
            .store
            .stop_begin(human, &nonce.unwrap_or_else(|| Uuid::now_v7().to_string()))?;
        if fresh {
            let worker = app.clone();
            let operation = op.clone();
            tokio::spawn(async move {
                run_stop(worker, operation).await;
            });
        }
        return Ok(public_status(app, &op));
    }
    if args == "restart" || args.starts_with("restart ") {
        let id = match args.strip_prefix("restart ") {
            Some(id) => id.trim().to_string(),
            None => app
                .shared
                .store
                .stop_latest(human)?
                .context("No Stop operation to restart")?,
        };
        let op = app.shared.store.stop_read(human, &id)?;
        if app.shared.store.stop_arm_restart(human, &id)? {
            maintenance_notice(app, &op, "before", "Restarting after coordinated Stop. Your active work will continue from retained history when the service returns.").await;
        }
        let mut result = public_status(app, &op);
        result["restart"] = json!(
            app.shared
                .store
                .bot_state_value(&format!("stop_restart:{id}"))?
        );
        result["detail"] = json!(
            "One service-return continuation is authorized. This command does not restart the host or service."
        );
        return Ok(result);
    }
    if args == "release" || args.starts_with("release ") {
        let id = match args.strip_prefix("release ") {
            Some(id) => id.trim().to_string(),
            None => app
                .shared
                .store
                .stop_latest(human)?
                .context("No Stop operation to release")?,
        };
        let mut op = app.shared.store.stop_read(human, &id)?;
        let previous = op.phase.clone();
        if matches!(
            previous.as_str(),
            "resumed" | "resumed_with_failures" | "released" | "superseded"
        ) {
            return Ok(public_status(app, &op));
        }
        op.phase = "released".into();
        app.shared.store.stop_save(&op, &previous)?;
        return Ok(public_status(app, &op));
    }
    if args == "status" {
        return match app.shared.store.stop_latest(human)? {
            Some(id) => Ok(public_status(app, &app.shared.store.stop_read(human, &id)?)),
            None => Ok(json!({"state":"no_operation"})),
        };
    }
    if args == "resume" {
        let id = app
            .shared
            .store
            .stop_latest(human)?
            .context("No Stop operation to resume")?;
        return resume(app, human, &id).await;
    }
    if let Some(id) = args.strip_prefix("status ") {
        let op = app.shared.store.stop_read(human, id.trim())?;
        return Ok(public_status(app, &op));
    }
    if let Some(id) = args.strip_prefix("resume ") {
        return resume(app, human, id.trim()).await;
    }
    bail!(
        "Usage: /stop all | /stop status ID | /stop resume ID | /stop restart ID | /stop release ID. Bare /stop retains its single-turn behavior."
    )
}
fn public_status(app: &App, op: &StopOperation) -> Value {
    json!({"schema":"telecodex.session-control/v1","operation":op.id,"state":op.phase,"scope":"this receiving provider's owned conversations","external_processes":"not certified by native turn status","subagents":"native descendants are enumerated and observed; recovery is delegated to the resumed parent, not claimed from queue admission","targets":op.targets.iter().map(|t|json!({"topic":t.key.thread_id,"state":t.phase,"detail":t.reason,"restart_notice":app.shared.store.bot_state_value(&notice_key(op,t,"before")).ok().flatten(),"return_notice":app.shared.store.bot_state_value(&notice_key(op,t,"after")).ok().flatten()})).collect::<Vec<_>>()})
}
fn notice_key(op: &StopOperation, t: &StopTarget, kind: &str) -> String {
    format!(
        "stop_notice:{}:{kind}:{}:{}",
        op.id, t.key.chat_id, t.key.thread_id
    )
}
async fn maintenance_notice(app: &App, op: &StopOperation, kind: &str, text: &str) {
    for t in op.targets.iter().filter(|t| t.was_busy || t.resume_goal) {
        if app.shared.store.stop_binding(op.human, t).is_err() {
            continue;
        }
        let key = notice_key(op, t, kind);
        if app
            .shared
            .store
            .bot_state_value(&key)
            .ok()
            .flatten()
            .is_some()
        {
            continue;
        }
        if app.shared.store.save_bot_state(&key, "attempted").is_err() {
            continue;
        }
        // No automatic resend after ambiguous Telegram delivery.
        let uncertain = tokio::time::timeout(
            Duration::from_secs(10),
            send_markdown_message(
                &app.shared.telegram,
                t.key.chat_id,
                Some(t.key.thread_id).filter(|id| *id != 0),
                text,
                None,
            ),
        )
        .await
        .map_or(true, |r| r.is_err());
        let _ = app.shared.store.save_bot_state(
            &key,
            if uncertain {
                "unconfirmed"
            } else {
                "delivered"
            },
        );
        if uncertain {
            tracing::warn!("Maintenance notice delivery unconfirmed; native history retained");
        }
    }
}
/// Consumes only an explicit, durable human maintenance request, once.
/// Ordinary startup does not install attention or resume arbitrary sessions.
pub(super) async fn recover_restart(app: App) -> Result<()> {
    let pending = app.shared.store.stop_claim_restarts()?;
    for (human, id) in pending {
        let op = app.shared.store.stop_read(human, &id)?;
        if op.phase != "stopped" {
            app.shared
                .store
                .save_bot_state(&format!("stop_restart:{id}"), "unconfirmed")?;
            continue;
        }
        maintenance_notice(&app,&op,"after","Service restarted after coordinated Stop. Resuming the work that was active; acceptance remains visible in /stop status.").await;
        let outcome = resume(&app, human, &id).await;
        app.shared.store.save_bot_state(
            &format!("stop_restart:{id}"),
            if outcome.is_ok() {
                "continuation_requested"
            } else {
                "unconfirmed"
            },
        )?;
        if outcome.is_err() {
            maintenance_notice(&app,&op,"failed","Service returned, but continuation could not be confirmed. Work remains retained; inspect /stop status before retrying.").await;
        }
    }
    Ok(())
}
async fn run_stop(app: App, mut op: StopOperation) {
    // The caller receives the operation receipt before its own turn is stopped.
    sleep(Duration::from_millis(250)).await;
    for index in 0..op.targets.len() {
        let mut t = op.targets[index].clone();
        let result = stop_target(&app, &mut op, &mut t, index).await;
        if let Err(error) = result {
            t.phase = "unconfirmed".into();
            t.reason = error.to_string();
        }
        op.targets[index] = t;
        if app.shared.store.stop_save(&op, "stopping").is_err() {
            return;
        }
    }
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        let mut quiet = true;
        for index in 0..op.targets.len() {
            let mut t = op.targets[index].clone();
            if t.phase == "unconfirmed" {
                quiet = false;
                continue;
            }
            let observed = observe(&app, op.human, &t).await;
            match observed {
                Ok(true) => {
                    t.phase = "native_stopped".into();
                    t.reason="Selected native session and descendant inactivity observed; external processes remain uncertified.".into();
                }
                Ok(false) => {
                    quiet = false;
                    if t.physical.is_none()
                        && app
                            .shared
                            .store
                            .get_session(t.key)
                            .ok()
                            .flatten()
                            .is_some_and(|s| !s.busy)
                    {
                        if let Some(native) = &t.native {
                            if app
                                .shared
                                .codex
                                .native_activity(native)
                                .await
                                .ok()
                                .as_deref()
                                == Some("active")
                            {
                                match app.shared.codex.live_native_turn(native).await {
                                    Ok(Some(physical)) => {
                                        t.was_busy = true;
                                        t.physical = Some(physical.clone());
                                        op.targets[index] = t.clone();
                                        if app.shared.store.stop_save(&op, "stopping").is_err() {
                                            return;
                                        }
                                        if let Err(e) = app
                                            .shared
                                            .codex
                                            .interrupt_selected_turn(native, &physical)
                                            .await
                                        {
                                            t.phase = "unconfirmed".into();
                                            t.reason = e.to_string();
                                        }
                                    }
                                    _ => {
                                        t.phase = "unconfirmed".into();
                                        t.reason="Delayed physical execution could not be selected; fence retained".into();
                                    }
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    quiet = false;
                    t.reason = e.to_string();
                }
            }
            op.targets[index] = t;
        }
        if quiet {
            op.phase = "stopped".into();
        } else if Instant::now() >= deadline {
            op.phase = "unconfirmed".into();
        }
        if app.shared.store.stop_save(&op, "stopping").is_err() {
            return;
        }
        if op.phase != "stopping" {
            return;
        }
        sleep(Duration::from_secs(1)).await;
    }
}
async fn stop_target(
    app: &App,
    op: &mut StopOperation,
    t: &mut StopTarget,
    index: usize,
) -> Result<()> {
    app.shared.store.stop_binding(op.human, t)?;
    let Some(native) = t.native.clone() else {
        if app.shared.store.get_session(t.key)?.is_some_and(|s| s.busy) {
            bail!("Native startup is still being resolved; fence retained");
        }
        t.phase = "native_stopped".into();
        return Ok(());
    };
    t.was_busy |= app.shared.codex.native_activity(&native).await? == "active";
    t.before = app.shared.codex.get_goal(&native).await?;
    t.phase = "stop_requested".into();
    op.targets[index] = t.clone();
    app.shared.store.stop_save(op, "stopping")?;
    let session = app
        .shared
        .store
        .get_session(t.key)?
        .context("Stop session disappeared")?;
    let captured = Arc::new(StdMutex::new(crate::codex::GoalStopSnapshot::default()));
    if goals::control_live(app, &session, GoalControl::Stop(captured.clone())).await? {
        let snapshot = captured
            .lock()
            .expect("goal stop snapshot mutex poisoned")
            .clone();
        t.before = snapshot.before;
        t.paused = snapshot.after;
        t.resume_goal =
            snapshot.was_staged || t.before.as_ref().is_some_and(|g| g.status == "active");
    } else {
        t.resume_goal = t.before.as_ref().is_some_and(|g| g.status == "active");
        t.paused = if t.resume_goal {
            Some(
                app.shared
                    .codex
                    .set_goal(&native, json!({"status":"paused"}))
                    .await?,
            )
        } else {
            t.before.clone()
        };
    }
    if t.resume_goal {
        let mut expected = t
            .before
            .clone()
            .context("Stopped goal definition missing")?;
        expected.status = t
            .paused
            .as_ref()
            .context("Goal pause missing")?
            .status
            .clone();
        if !t.paused.as_ref().is_some_and(|after| {
            expected.same_definition(after)
                && matches!(
                    after.status.as_str(),
                    "paused" | "complete" | "budgetLimited" | "usageLimited"
                )
        }) {
            bail!("Stopped goal changed; fence retained");
        }
    }
    op.targets[index] = t.clone();
    app.shared.store.stop_save(op, "stopping")?;
    app.shared.store.stop_binding(op.human, t)?;
    if !app.stop_session(t.key).await
        && app.shared.codex.native_activity(&native).await? == "active"
    {
        let physical = app
            .shared
            .codex
            .live_native_turn(&native)
            .await?
            .context("Detached active turn could not be selected")?;
        t.physical = Some(physical.clone());
        op.targets[index] = t.clone();
        app.shared.store.stop_save(op, "stopping")?;
        app.shared.store.stop_binding(op.human, t)?;
        app.shared
            .codex
            .interrupt_selected_turn(&native, &physical)
            .await?;
    }
    t.children = app.shared.codex.native_descendants(&native).await?;
    op.targets[index] = t.clone();
    app.shared.store.stop_save(op, "stopping")?;
    for child in &t.children {
        if let Some(before) = app.shared.codex.get_goal(child).await? {
            t.child_goals.insert(child.clone(), before.clone());
            op.targets[index] = t.clone();
            app.shared.store.stop_save(op, "stopping")?;
            if before.status == "active" {
                t.was_busy = true;
                app.shared
                    .codex
                    .set_goal(child, json!({"status":"paused"}))
                    .await?;
                let after = app
                    .shared
                    .codex
                    .get_goal(child)
                    .await?
                    .context("Child goal pause unavailable")?;
                let mut expected = before;
                expected.status = "paused".into();
                if !after.same_definition(&expected) {
                    bail!("Child goal changed during Stop");
                }
            }
        }
        if app.shared.codex.native_activity(child).await? == "active" {
            t.was_busy = true;
            let physical = app
                .shared
                .codex
                .live_native_turn(child)
                .await?
                .context("Active child turn missing")?;
            app.shared
                .codex
                .interrupt_selected_turn(child, &physical)
                .await?;
        }
    }
    Ok(())
}
async fn observe(app: &App, human: i64, t: &StopTarget) -> Result<bool> {
    app.shared.store.stop_binding(human, t)?;
    if app.shared.store.get_session(t.key)?.is_some_and(|s| s.busy) {
        return Ok(false);
    }
    if let Some(native) = &t.native {
        let state = app.shared.codex.native_activity(native).await?;
        if !matches!(state.as_str(), "idle" | "notLoaded") {
            return Ok(false);
        }
        let children = app.shared.codex.native_descendants(native).await?;
        for child in children {
            if !matches!(
                app.shared.codex.native_activity(&child).await?.as_str(),
                "idle" | "notLoaded"
            ) || app
                .shared
                .codex
                .get_goal(&child)
                .await?
                .is_some_and(|g| g.status == "active")
            {
                return Ok(false);
            }
        }
        if app
            .shared
            .codex
            .get_goal(native)
            .await?
            .is_some_and(|g| g.status == "active")
        {
            return Ok(false);
        }
    }
    Ok(true)
}
async fn resume(app: &App, human: i64, id: &str) -> Result<Value> {
    let mut op = app.shared.store.stop_read(human, id)?;
    if op.phase != "stopped" {
        bail!("Only an observed stopped operation can resume; inspect /stop status ID");
    }
    for t in &op.targets {
        if !observe(app, human, t).await? {
            bail!("Stop destination is no longer inactive");
        }
        if t.resume_goal && t.paused.as_ref().is_some_and(|g| g.status == "paused") {
            let now = app
                .shared
                .codex
                .get_goal(t.native.as_deref().context("Stop native binding missing")?)
                .await?;
            if !now
                .as_ref()
                .zip(t.paused.as_ref())
                .is_some_and(|(now, paused)| now.same_definition(paused))
                || app
                    .shared
                    .store
                    .bot_state_value(&goals::intent_key(t.key))?
                    .as_deref()
                    != Some(format!("stop:{}", op.id).as_str())
            {
                bail!("Paused goal changed; no continuation was submitted");
            }
        }
        if app
            .shared
            .store
            .bot_state_value(&format!(
                "native_writer_hold:{}:{}",
                t.key.chat_id, t.key.thread_id
            ))?
            .is_some()
        {
            bail!("Another maintenance operation still holds this destination");
        }
    }
    // Claim the finite resume before queuing anything. Restart does not retry it.
    op.phase = "resuming".into();
    for t in &mut op.targets {
        t.phase = "resume_intent".into();
    }
    app.shared.store.stop_save(&op, "stopped")?;
    for t in &op.targets {
        let result = resume_target(app, human, t, id).await;
        let phase = match result {
            Ok(true) => "resume_queued",
            Ok(false) => "resume_unchanged",
            Err(_) => "resume_unconfirmed",
        };
        app.shared
            .store
            .stop_resume_event(id, t.key, phase, None, None)?;
    }
    op = app.shared.store.stop_read(human, id)?;
    Ok(public_status(app, &op))
}
async fn resume_target(app: &App, human: i64, t: &StopTarget, id: &str) -> Result<bool> {
    app.shared.store.stop_binding(human, t)?;
    let session = app
        .shared
        .store
        .get_session(t.key)?
        .context("Resume session missing")?;
    if t.resume_goal {
        if t.paused.as_ref().is_none_or(|g| g.status != "paused") {
            return Ok(false);
        }
        let intent = Uuid::now_v7().to_string();
        app.shared
            .store
            .save_bot_state(&goals::intent_key(t.key), &intent)?;
        let activation = crate::codex::GoalActivation {
            intent,
            native_thread: session.codex_thread_id,
            expected: t.paused.clone(),
            objective: None,
            replace: false,
            attach_turn: None,
            conditional: None,
        };
        app.enqueue_stop_resume(TurnRequest {session_key:t.key,from_user_id:human,prompt:format!("Continue the native goal explicitly resumed by the human after coordinated Stop. Inspect retained history and completed effects before continuing. Recover unfinished subagent work through the native agent tools; the verified stopped descendants were {:?}; their prior goal states were {:?}. Use their native recovery surface to restore only formerly active, unchanged child goals. Do not claim a child resumed without its receipt.",t.children,t.child_goals.iter().map(|(id,g)|(id,&g.status)).collect::<Vec<_>>()),runtime_instructions:None,attachments:vec![],review_mode:None,override_search_mode:None},Some(activation),id).await?;
    } else if t.was_busy {
        app.enqueue_stop_resume(TurnRequest{session_key:t.key,from_user_id:human,prompt:format!("The human explicitly requested Resume after coordinated Stop. Continue the unfinished work from this native conversation's retained history. Inspect completed effects; do not replay prior input. Preserve previously paused or blocked goals. Recover unfinished subagent work using native agent tools; verified stopped descendants were {:?}; their prior goal states were {:?}. Restore only formerly active unchanged child goals through native recovery. Do not claim a child resumed without a receipt.",t.children,t.child_goals.iter().map(|(id,g)|(id,&g.status)).collect::<Vec<_>>()),runtime_instructions:None,attachments:vec![],review_mode:None,override_search_mode:None},None,id).await?;
    } else {
        return Ok(false);
    }
    Ok(true)
}

#[cfg(unix)]
pub(super) async fn start_control_socket(app: App) -> Result<()> {
    use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
    use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
    use tokio::net::UnixListener;
    let Some(config) = app.shared.config.session_control.clone() else {
        return Ok(());
    };
    let path = config.socket_path.clone();
    let parent = path.parent().context("Control socket parent missing")?;
    let meta = std::fs::symlink_metadata(parent)?;
    let uid = unsafe { libc::geteuid() };
    if !meta.is_dir() || meta.uid() != uid || meta.mode() & 0o077 != 0 {
        bail!("Control socket needs an owner-only directory");
    }
    if let Ok(old) = std::fs::symlink_metadata(&path) {
        if !old.file_type().is_socket() || old.uid() != uid {
            bail!("Refusing to replace an unsafe control path");
        }
        if tokio::net::UnixStream::connect(&path).await.is_ok() {
            bail!("A control listener already owns this path");
        }
        std::fs::remove_file(&path)?;
    }
    let listener = UnixListener::bind(&path)?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
    let capacity = Arc::new(tokio::sync::Semaphore::new(4));
    tokio::spawn(async move {
        loop {
            let (stream, _) = match listener.accept().await {
                Ok(value) => value,
                Err(_) => break,
            };
            if !stream.peer_cred().is_ok_and(|cred| cred.uid() == uid) {
                continue;
            }
            let permit = match capacity.clone().try_acquire_owned() {
                Ok(p) => p,
                Err(_) => continue,
            };
            let app = app.clone();
            let human = config.human_id;
            tokio::spawn(async move {
                let _permit = permit;
                let (read, mut write) = stream.into_split();
                let mut reader = BufReader::new(read.take(262145));
                let mut raw = Vec::new();
                let reply = match tokio::time::timeout(
                    Duration::from_secs(5),
                    reader.read_until(b'\n', &mut raw),
                )
                .await
                {
                    Ok(Ok(n)) if n > 0 && n <= 262144 => {
                        match serde_json::from_slice::<Value>(&raw) {
                            Ok(v) => match command(
                                &app,
                                human,
                                v["command"].as_str().unwrap_or(""),
                                v["nonce"].as_str().map(str::to_string),
                            )
                            .await
                            {
                                Ok(value) => json!({"ok":true,"result":value}),
                                Err(error) => json!({"ok":false,"error":error.to_string()}),
                            },
                            Err(_) => json!({"ok":false,"error":"Invalid control request"}),
                        }
                    }
                    _ => json!({"ok":false,"error":"Control request unavailable or oversized"}),
                };
                let mut bytes = serde_json::to_vec(&reply).unwrap_or_default();
                if bytes.len() > 262143 {
                    bytes = br#"{"ok":false,"error":"Control response exceeds receiving bound"}"#
                        .to_vec();
                }
                bytes.push(b'\n');
                let _ = write.write_all(&bytes).await;
            });
        }
    });
    Ok(())
}
#[cfg(not(unix))]
pub(super) async fn start_control_socket(app: App) -> Result<()> {
    if app.shared.config.session_control.is_some() {
        bail!("Owner-local session control requires Unix");
    }
    Ok(())
}
