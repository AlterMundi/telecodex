//! Bounded read-only native observation on the existing listener heartbeat.
//! No input, goal mutation, inference, peer attention or service restart.
use super::*;

pub(super) async fn tick(app: &App) -> Result<()> {
    let mut seen = std::collections::HashSet::new();
    let mut selected = Vec::new();
    for _ in 0..4 {
        let Some(row) = app.shared.store.native_collection_next()? else {
            break;
        };
        if !seen.insert(row.local) {
            break;
        }
        if !app.shared.store.native_writer_held(row.key)? {
            selected.push(row);
        }
    }
    let outcomes =
        futures_util::future::join_all(selected.into_iter().map(|selected| async move {
            match app
                .shared
                .codex
                .recovery_history(
                    &selected.native,
                    &selected.anchor,
                    selected.client_id.as_deref(),
                )
                .await
            {
                Ok((turns, goal_active)) => {
                    app.shared
                        .store
                        .native_collection_observed(&selected, &turns, goal_active)?;
                    super::topic_publications::wake(app.shared.clone(), selected.key);
                    if let Some(turn) = turns.last().filter(|t| {
                        matches!(
                            t["status"].as_str(),
                            Some("completed" | "failed" | "interrupted")
                        )
                    }) {
                        let handle = app.workers.lock().await.get(&selected.key).cloned();
                        let active = handle
                            .and_then(|w| w.steer.lock().expect("steer mutex poisoned").clone());
                        if let Some(active) = active {
                            let text = turn["items"]
                                .as_array()
                                .and_then(|items| {
                                    items.iter().rev().find(|i| {
                                        i["type"] == "agentMessage" && i["phase"] == "final_answer"
                                    })
                                })
                                .and_then(|i| i["text"].as_str())
                                .map(str::to_owned);
                            let (response, _) = oneshot::channel();
                            let _ = active.sender.send(CodexSteerRequest {
                                observation: Some(crate::codex::NativeTerminalObservation {
                                    turn_id: turn["id"]
                                        .as_str()
                                        .context("Native observation identity missing")?
                                        .into(),
                                    status: turn["status"]
                                        .as_str()
                                        .context("Native observation status missing")?
                                        .into(),
                                    goal_active,
                                    assistant_text: text,
                                }),
                                images: vec![],
                                text: String::new(),
                                client_message_id: None,
                                goal: None,
                                admission_check: None,
                                response,
                            });
                        }
                    }
                    if let Some(turn) = turns.last().filter(|t| t["status"] == "inProgress") {
                        let physical = turn["id"]
                            .as_str()
                            .context("Native active identity missing")?;
                        let active = app
                            .workers
                            .lock()
                            .await
                            .get(&selected.key)
                            .is_some_and(|w| {
                                w.cancel.lock().expect("cancel mutex poisoned").is_some()
                            });
                        if !active {
                            let args = format!(
                                "{} {} {} {}",
                                selected.key.chat_id,
                                selected.key.thread_id,
                                selected.native,
                                physical
                            );
                            // Existing attach-active verifies the writer again and
                            // cannot kick off a turn or mutate/resume a native goal.
                            super::goals::attach_active(app, selected.human, &args).await?;
                        }
                    }
                }
                Err(error) => {
                    app.shared.store.native_collection_failed(selected.local)?;
                    return Err(error);
                }
            }
            Ok(())
        }))
        .await;
    for result in outcomes {
        result?;
    }
    Ok(())
}
