//! Shared model-free recovery. Its lifecycle belongs to the backend, not a UI.
use super::*;

pub(super) async fn run(app: App) {
    let settings = app.shared.config.goal_recovery.clone();
    if !settings.enabled || !app.shared.config.codex.shared_app_server {
        return;
    }
    let mut interval = tokio::time::interval(Duration::from_secs(settings.interval_seconds));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        tokio::select! {
            _=app.shared.shutdown.cancelled()=>return,
            _=app.shared.goal_recovery_wakeup.notified()=>{},
            _=interval.tick()=>{},
        }
        if tick(&app, settings.batch_size).await.is_err() {
            tracing::warn!(
                "Shared goal recovery observation remains pending; no uncertain work replayed"
            );
        }
        if let Some(settings) = &settings.alerts {
            if publish_alerts(&app, settings).await.is_err() {
                tracing::warn!(
                    "Goal recovery Alerts acceptance unavailable; immutable projection retained"
                );
            }
        }
    }
}

async fn publish_alerts(app: &App, settings: &crate::config::GoalRecoveryAlerts) -> Result<()> {
    use std::process::Stdio;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    app.shared.store.goal_recovery_alerts_prepare(settings)?;
    for payload in app.shared.store.goal_recovery_alerts_due(settings)? {
        if app.shared.shutdown.is_cancelled() {
            return Ok(());
        }
        let input = serde_json::to_vec(&payload)?;
        anyhow::ensure!(input.len() <= 16384, "Safe alert projection exceeded bound");
        let mut child = tokio::process::Command::new(&settings.command[0])
            .args(&settings.command[1..])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()?;
        let mut stdin = child
            .stdin
            .take()
            .context("Alert adapter input unavailable")?;
        let stdout = child
            .stdout
            .take()
            .context("Alert adapter output unavailable")?;
        let observed: Result<serde_json::Value> =
            tokio::time::timeout(Duration::from_secs(8), async {
                stdin.write_all(&input).await?;
                drop(stdin);
                let mut raw = Vec::new();
                stdout.take(32769).read_to_end(&mut raw).await?;
                anyhow::ensure!(raw.len() <= 32768, "Alert adapter response exceeded bound");
                anyhow::ensure!(
                    child.wait().await?.success(),
                    "Alert adapter acceptance unavailable"
                );
                Ok(serde_json::from_slice(&raw)?)
            })
            .await
            .context("Alert adapter deadline exceeded")?;
        // One deadline includes stdin, response and process exit. Upserts may replay exact content;
        // native goal writes and Telegram delivery claims never use this path.
        app.shared
            .store
            .goal_recovery_alert_accepted(&payload, &observed?)?;
    }
    Ok(())
}

async fn tick(app: &App, batch: usize) -> Result<()> {
    for job in app.shared.store.goal_retries_due(batch)? {
        if app.shared.shutdown.is_cancelled() {
            return Ok(());
        }
        if let Err(_error) = recover(app, &job).await {
            // One target's unresolved control must not starve unrelated jobs.
            // Retain its existing no-replay fence and back off observation.
            app.shared.store.goal_retry_defer(
                &job.id,
                "waiting",
                "Admission or observation unavailable; retained native effects are not replayed",
            )?;
        }
    }
    Ok(())
}

async fn recover(app: &App, job: &crate::store::GoalRetry) -> Result<()> {
    if !app.shared.store.goal_retry_validate(&job.id)? {
        app.shared.store.goal_retry_defer(
            &job.id,
            "superseded",
            "Current ownership, direction, delegation, policy or writer fence changed",
        )?;
        return Ok(());
    }
    let observed = app.shared.codex.goal_recovery_head(&job.native).await;
    let (snapshot, latest) = match observed {
        Ok(value) => value,
        Err(_) => {
            app.shared.store.goal_retry_defer(
                &job.id,
                "waiting",
                "Native snapshot/latest terminal turn could not be verified; no mutation",
            )?;
            return Ok(());
        }
    };
    let Some(value) = app
        .shared
        .store
        .goal_retry_claim(&job.id, &snapshot, &latest)?
    else {
        return Ok(());
    };
    let control = value["id"]
        .as_str()
        .context("Recovery control identity missing")?;
    let attempt = value["intent"]["attempt"]
        .as_str()
        .context("Recovery attempt missing")?;
    let result: Result<serde_json::Value> = async {
        let target = app
            .shared
            .store
            .get_session(job.target)?
            .context("Recovery target no longer available")?;
        let plan: crate::store::GoalPlan = serde_json::from_value(value["intent"]["plan"].clone())?;
        super::goal_controls::activate_control(
            app, job.human, job.target, control, &value, &plan, &target,
        )
        .await
    }
    .await;
    if result.is_err() {
        // Only the exact retained write boundary determines unsent/uncertain.
        app.shared
            .store
            .goal_control_retire_execution(job.human, job.target, control, attempt)?;
    }
    Ok(())
}
