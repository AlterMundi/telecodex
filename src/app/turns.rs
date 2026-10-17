use super::presentation::{approval_waiting_text, request_telegram_approval};
use super::support::{is_message_not_modified, session_title_is_present, telegram_retry_after};
use super::*;
use std::path::{Path, PathBuf};
use std::time::Duration;
use uuid::Uuid;

pub(super) async fn process_turn(
    shared: Arc<AppShared>,
    cancel_slot: Arc<StdMutex<Option<CancellationToken>>>,
    steer_slot: Arc<StdMutex<Option<ActiveTurnSteerHandle>>>,
    mut queued: QueuedTurn,
) -> Result<()> {
    if shared
        .store
        .stop_admission_epoch(queued.request.session_key)?
        != queued.admission_epoch
    {
        if let Some(id) = queued.update_id {
            shared.store.stop_retain_input(id)?;
        }
        return Ok(());
    }
    if let Some(id) = queued.stop_resume.as_deref() {
        shared.store.stop_validate_resume(
            id,
            queued.request.session_key,
            queued.request.from_user_id,
        )?;
    }
    if queued.stop_resume.is_none() && shared.store.stop_fenced(queued.request.session_key)? {
        if let Some(id) = queued.update_id {
            shared.store.stop_retain_input(id)?;
        }
        return Ok(());
    }
    if shared
        .store
        .maintenance_writer_held(queued.request.session_key)?
    {
        if let Some(id) = queued.tribu_admission.as_deref() {
            shared.store.tribu_withdraw(id, "withdrawn-native-writer")?;
        }
        bail!(
            "Native writer held during controlled maintenance; input retained without submission"
        );
    }
    if let Some(goal) = queued.goal.as_ref() {
        if shared
            .store
            .bot_state_value(&super::goals::intent_key(queued.request.session_key))?
            .as_deref()
            != Some(goal.intent.as_str())
        {
            return send_markdown_message(&shared.telegram,queued.request.session_key.chat_id,Some(queued.request.session_key.thread_id).filter(|id|*id!=0),"Queued goal activation was cancelled by a newer goal command. No native input was submitted.",None).await.map(|_|());
        }
    }
    if !super::nexus::prepare(&shared, &mut queued).await? {
        return Ok(());
    }
    if !super::merging::prepare_turn(&shared, &mut queued)? {
        return Ok(());
    }
    if !super::tribu::prepare(&shared, &mut queued).await? {
        return Ok(());
    }
    let session = shared.store.ensure_session(
        queued.request.session_key,
        queued.request.from_user_id,
        &shared.session_defaults,
    )?;
    let session = if queued.merge.is_some() {
        session
    } else {
        resolve_session_codex_binding_from_history(&shared, session)?
    };
    super::nexus::validate_input_target(
        &shared,
        queued.update_id,
        session.key,
        queued.request.from_user_id,
    )?;
    let bounded = shared
        .config
        .tribu
        .as_ref()
        .filter(|p| p.key() == session.key);
    if bounded.is_some() {
        shared.store.tribu_check_thread(&session)?;
    }
    shared.store.set_session_busy(session.key, true)?;
    let turn_id = shared
        .store
        .record_turn_started(session.id, &queued.request)?;
    if let Some(id) = queued.nexus.as_deref() {
        shared.store.nexus_link(id, turn_id)?;
    }
    if let Some(id) = queued.merge.as_deref() {
        shared.store.merge_link(id, turn_id)?;
    }
    if let Some(id) = queued.stop_resume.as_deref() {
        shared.store.stop_resume_event(
            id,
            session.key,
            "resume_submitting",
            Some(turn_id),
            None,
        )?;
    }
    if queued.goal.is_some() {
        shared
            .store
            .save_bot_state(&format!("goal_turn:{turn_id}"), "native")?;
    }
    if let Some(id) = queued.tribu_admission.as_deref() {
        shared.store.tribu_link_turn(id, turn_id)?;
    }
    if let Some(update_id) = queued.update_id {
        shared.store.link_update_turn(update_id, turn_id)?;
    }
    let turn_workspace = prepare_turn_workspace(&session, turn_id)?;

    let cancel = CancellationToken::new();
    *cancel_slot.lock().expect("cancel mutex poisoned") = Some(cancel.clone());
    let mut chat_action_task = Some(spawn_chat_action_task(
        shared.telegram.clone(),
        session.key.chat_id,
        Some(session.key.thread_id).filter(|value| *value != 0),
        cancel.clone(),
    ));
    let limits_inline = latest_limits_snapshot_from_shared(&shared)
        .await
        .map_err(|error| {
            tracing::warn!("failed to read latest limits snapshot: {error:#}");
            error
        })
        .ok()
        .and_then(|snapshot| snapshot.and_then(|snapshot| format_limits_inline(&snapshot)));
    let turn_banner = turn_start_banner(limits_inline, &session);
    let thinking_text = "⏳";
    let placeholder_text = render_placeholder_html(thinking_text, turn_banner.as_deref());
    let thread_id = Some(session.key.thread_id).filter(|value| *value != 0);
    let draft_id = turn_id;
    let sink = if !shared.config.telegram.show_unfinished_messages {
        Arc::new(Mutex::new(LiveTurnSink::new_preview(
            shared.clone(),
            &session,
            turn_banner,
            None,
        )))
    } else if shared.config.telegram.use_message_drafts && queued.chat_kind == "private" {
        match shared
            .telegram
            .send_message_draft(SendMessageDraft::html(
                session.key.chat_id,
                thread_id,
                draft_id,
                placeholder_text.clone(),
            ))
            .await
        {
            Ok(_) => Arc::new(Mutex::new(LiveTurnSink::new_draft(
                shared.clone(),
                &session,
                turn_banner,
                draft_id,
            ))),
            Err(error) => {
                tracing::debug!(
                    "sendMessageDraft failed, falling back to placeholder message: {error:#}"
                );
                create_preview_sink_or_fail(
                    shared.clone(),
                    cancel_slot.clone(),
                    cancel.clone(),
                    &mut chat_action_task,
                    &queued,
                    &session,
                    turn_id,
                    &turn_workspace,
                    turn_banner,
                    placeholder_text,
                )
                .await?
            }
        }
    } else {
        create_preview_sink_or_fail(
            shared.clone(),
            cancel_slot.clone(),
            cancel.clone(),
            &mut chat_action_task,
            &queued,
            &session,
            turn_id,
            &turn_workspace,
            turn_banner,
            placeholder_text,
        )
        .await?
    };
    sink.lock().await.turn_id = Some(turn_id);
    let mut runtime_request = queued.request.clone();
    let handoff_key = format!(
        "handoff_notice:{}:{}",
        session.key.chat_id, session.key.thread_id
    );
    let notice_key = if shared.store.bot_state_value(&handoff_key)?.is_some() {
        Some(handoff_key)
    } else {
        session
            .codex_thread_id
            .as_deref()
            .map(|thread| format!("fork_notice:{thread}"))
    };
    if let Some(key) = notice_key.as_deref() {
        if let Some(notice) = shared.store.bot_state_value(key)? {
            shared.store.audit(
                Some(queued.request.from_user_id),
                "fork_notice_prepared",
                serde_json::json!({"session":session.id,"turn":turn_id}),
            )?;
            runtime_request.prompt = format!(
                "[Conversation origin]\n{notice}\n\n{}",
                runtime_request.prompt
            );
        }
    }

    enrich_audio_transcripts(&shared, &mut runtime_request, &turn_workspace, &sink).await;
    let runtime_request = prepare_runtime_request(&session, &runtime_request, &turn_workspace);
    let runtime_request = enrich_runtime_request_with_codex_history(&session, runtime_request);
    let runtime_request =
        enrich_runtime_request_with_context_pressure(&session, runtime_request).await;

    let (steer_tx, steer_rx) = mpsc::unbounded_channel();
    if queued.request.review_mode.is_none() {
        *steer_slot.lock().expect("steer mutex poisoned") = Some(ActiveTurnSteerHandle {
            turn_id,
            sender: steer_tx,
        });
    }

    let mut runner = shared.codex.clone();
    let mut deadline_task = None;
    let mut finish_deadline = None;
    if let Some(policy) = bounded {
        let started_at = chrono::Utc::now();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(policy.deadline_seconds);
        finish_deadline = Some(deadline + Duration::from_secs(10));
        let admission = queued
            .tribu_admission
            .clone()
            .context("missing Tribu admission")?;
        shared.store.tribu_deadline(
            &admission,
            &(started_at + chrono::Duration::seconds(policy.deadline_seconds as i64)).to_rfc3339(),
        )?;
        runner = runner.with_bounded_turn(crate::codex::BoundedTurn {
            started_at,
            deadline,
            seconds: policy.deadline_seconds,
            client_id: queued
                .tribu_admission
                .clone()
                .context("missing Tribu admission correlation")?,
        });
        let interrupt = cancel.clone();
        let deadline_shared = shared.clone();
        deadline_task = Some(tokio::spawn(async move {
            tokio::time::sleep_until(deadline).await;
            interrupt.cancel();
            if super::tribu::settle(&deadline_shared, &admission, true)
                .await
                .is_err()
            {
                tracing::warn!("Tribu deadline interruption unconfirmed; admission retained");
            }
        }));
    }
    let runner = if let Some(goal) = queued.goal.clone() {
        runner.with_goal_activation(goal)
    } else {
        runner
    };
    let runner = runner.with_client_message_id(queued.update_id.map(|id| {
        format!(
            "telegram:{}:{}:{id}",
            session.key.chat_id, queued.request.from_user_id
        )
    }));
    let physical_turn = Arc::new(StdMutex::new(None::<String>));
    let run =
        runner
            .run_turn(&session, &runtime_request, cancel.clone(), steer_rx, {
                let sink = sink.clone();
                let shared = shared.clone();
                let approval_cancel = cancel.clone();
                let requester_user_id = queued.request.from_user_id;
                let session_key = session.key;
                let preserve_thread_binding = queued.request.review_mode.is_none();
                let admission=queued.tribu_admission.clone();
                let physical_turn=physical_turn.clone();
                let stop_resume=queued.stop_resume.clone();
                let admission_epoch=queued.admission_epoch.clone();
                move |event| {
                    let sink = sink.clone();
                    let shared = shared.clone();
                    let cancel = approval_cancel.clone();
                    let admission=admission.clone();
                    let physical_turn=physical_turn.clone();
                    let stop_resume=stop_resume.clone();
                    let admission_epoch=admission_epoch.clone();
                    async move {
                        match event {
                            CodexEvent::ThreadStarted(thread_id) => {
                                shared.store.merge_thread(turn_id,&thread_id)?;
                                super::nexus::thread(&shared,turn_id,&thread_id)?;
                                if let Some(id)=admission.as_deref() {shared.store.tribu_native_thread(session_key,id,&thread_id)?;}
                                shared.store.record_turn_native_thread(turn_id, &thread_id)?;
                                if preserve_thread_binding {
                                    shared
                                        .store
                                        .set_session_codex_thread(session_key, &thread_id)?;
                                    if super::rename::sync_native_title(&shared, session_key).await.is_err() {
                                        let _ = send_markdown_message(
                                            &shared.telegram,
                                            session_key.chat_id,
                                            Some(session_key.thread_id).filter(|id| *id > 0),
                                            "The saved name could not be confirmed in Codex. Retry /rename with the intended name.",
                                            None,
                                        ).await;
                                    }
                                }
                                sink.lock()
                                    .await
                                    .handle_event(CodexEvent::ThreadStarted(thread_id))
                                    .await?;
                                Ok(CodexEventOutcome::None)
                            }
                            CodexEvent::ApprovalRequest(request) => {
                                let (mut response, receiver) = oneshot::channel();
                                let approval_cancel = cancel.child_token();
                                tokio::spawn(async move {
                                    if response.is_closed() { return; }
                                    let approval = request_telegram_approval(
                                        shared, session_key.chat_id,
                                        Some(session_key.thread_id).filter(|value| *value != 0),
                                        requester_user_id, request, approval_cancel.clone(),
                                    );
                                    tokio::pin!(approval);
                                    let result = tokio::select! {
                                        result = &mut approval => result,
                                        _ = response.closed() => {
                                            approval_cancel.cancel();
                                            let _ = approval.await;
                                            return;
                                        }
                                    };
                                    let _ = response.send(result.unwrap_or(CodexApprovalDecision::Cancel));
                                });
                                Ok(CodexEventOutcome::Approval(receiver))
                            }
                            CodexEvent::NativeInputRejected => {
                                if let Some(id)=stop_resume.as_deref(){shared.store.stop_resume_event(id,session_key,"resume_rejected",Some(turn_id),None)?;}

                                for id in shared.store.merge_local(turn_id)? {
                                    if shared.store.merge_leg(&id)?["phase"]=="submitting" {shared.store.merge_phase(&id,"rejected","Native explicitly rejected merge turn/start")?;}
                                }
                                for id in shared.store.nexus_local_requests(turn_id)? {
                                    if shared.store.nexus_request(&id)?.is_some_and(|r|r["phase"]=="submitting") { shared.store.nexus_receipt(&id,"rejected",None,None,Some("Native explicitly rejected turn/start"))?; }
                                }
                                Ok(CodexEventOutcome::None)
                            }
                            CodexEvent::NativeInputAccepted(native_id) => {
                                if let Some(id)=stop_resume.as_deref(){shared.store.stop_resume_event(id,session_key,"resume_accepted",Some(turn_id),Some(&native_id))?;}

                                shared.store.merge_input_accepted(turn_id,&native_id)?;
                                shared.store.nexus_input_accepted(turn_id,&native_id)?;
                                Ok(CodexEventOutcome::None)
                            }
                            CodexEvent::NativeMessageCompleted { thread_id, turn_id: native_turn, item_id, phase, text } => {
                                shared.store.record_native_message(turn_id, &thread_id, &native_turn, &item_id, phase.as_deref(), &text)?;
                                Ok(CodexEventOutcome::None)
                            }
                            CodexEvent::FinalAnswerCompleted(text) => {
                                if let Some(native)=physical_turn.lock().expect("native correlation mutex poisoned").as_deref(){shared.store.merge_answer(turn_id,native,&text)?;}
                                if !shared.store.nexus_local_requests(turn_id)?.is_empty() {
                                    if let Some(native)=physical_turn.lock().expect("native correlation mutex poisoned").as_deref() {shared.store.nexus_final(turn_id,native,&text)?;}
                                }
                                sink.lock().await.handle_event(CodexEvent::FinalAnswerCompleted(text)).await?;
                                Ok(CodexEventOutcome::None)
                            }
                            CodexEvent::NativeTurnStarted(native_id) => {
                                *physical_turn.lock().expect("native correlation mutex poisoned")=Some(native_id.clone());
                                if !shared.store.nexus_local_requests(turn_id)?.is_empty() { shared.store.nexus_native_started(turn_id,&native_id)?; }
                                if let Some(id)=admission.as_deref(){shared.store.tribu_native_turn(id,&native_id,None)?;}
                                Ok(CodexEventOutcome::None)
                            }
                            CodexEvent::NativeTurnSubmitting => {
                                if shared.store.stop_admission_epoch(session_key)? != admission_epoch {anyhow::bail!("Input admission was retired by coordinated Stop; never replay automatically");}
                                if let Some(id)=stop_resume.as_deref(){shared.store.stop_validate_resume(id,session_key,requester_user_id)?;}else if shared.store.stop_fenced(session_key)? {anyhow::bail!("Coordinated Stop prevents native submission; input retained");}

                                shared.store.merge_submitting(turn_id)?;
                                super::nexus::submitting(&shared,turn_id).await?;
                                if let Some(id)=admission.as_deref(){super::tribu::revalidate_submission(&shared,id).await?;shared.store.tribu_submitting(session_key,id)?;}
                                if shared.store.maintenance_writer_held(session_key)? {
                                    bail!("Native writer held during controlled maintenance; input retained without submission");
                                }
                                Ok(CodexEventOutcome::None)
                            }
                            CodexEvent::NativeTurnTerminal {id:native_id,status} => {
                                if let Some(id)=stop_resume.as_deref(){shared.store.stop_resume_event(id,session_key,&format!("resume_turn_{status}"),Some(turn_id),Some(&native_id))?;}

                                shared.store.merge_terminal(turn_id,&native_id,&status)?;
                                if !shared.store.nexus_local_requests(turn_id)?.is_empty() { shared.store.nexus_terminal(turn_id,&native_id,&status)?; }
                                if let Some(id)=admission.as_deref(){shared.store.tribu_native_turn(id,&native_id,Some(&status))?;}
                                Ok(CodexEventOutcome::None)
                            }
                            CodexEvent::UserInputRequest {
                                id,
                                request,
                                closed,
                            } => {
                                let receiver = questions::begin_dialogue(
                                    shared,
                                    session_key,
                                    turn_id,
                                    requester_user_id,
                                    id,
                                    request,
                                    closed,
                                )
                                .await?;
                                Ok(CodexEventOutcome::UserInput(receiver))
                            }
                            CodexEvent::UserInputClosed(id) => {
                                shared.store.resolve_question_answers(turn_id, &id, false)?;
                                shared.store.finish_native_interactions(turn_id, Some(&id), false)?;
                                shared.pending_questions.lock().await.retain(|_, p| {
                                    p.local_turn_id != turn_id || p.request_id != id
                                });
                                Ok(CodexEventOutcome::None)
                            }
                            CodexEvent::UserInputAnswered(id) => {
                                shared.store.resolve_question_answers(turn_id, &id, true)?;
                                shared.store.finish_native_interactions(turn_id, Some(&id), true)?;
                                Ok(CodexEventOutcome::None)
                            }
                            other => {
                                sink.lock().await.handle_event(other).await?;
                                Ok(CodexEventOutcome::None)
                            }
                        }
                    }
                }
            });
    let run_result = if let Some(deadline) = finish_deadline {
        match tokio::time::timeout_at(deadline, run).await {
            Ok(result) => result,
            Err(_) => Err(anyhow!(
                "Tribu native deadline reached; execution completion unconfirmed"
            )),
        }
    } else {
        run.await
    };
    if let Some(task) = deadline_task {
        let settled = queued
            .tribu_admission
            .as_deref()
            .map(|id| shared.store.tribu_execution(id))
            .transpose()?
            .flatten()
            .is_some_and(|v| {
                v["terminal"]
                    .as_str()
                    .is_some_and(|s| matches!(s, "completed" | "failed" | "interrupted"))
            });
        if settled || run_result.is_ok() {
            task.abort();
        }
    }

    shared
        .pending_questions
        .lock()
        .await
        .retain(|_, p| p.local_turn_id != turn_id);

    shared
        .store
        .finish_native_interactions(turn_id, None, false)?;
    *steer_slot.lock().expect("steer mutex poisoned") = None;
    *cancel_slot.lock().expect("cancel mutex poisoned") = None;
    cancel.cancel();
    if let Some(chat_action_task) = chat_action_task.take() {
        let _ = chat_action_task.await;
    }
    let final_result = async {
        match run_result {
            Ok(summary) => {
                if let Some(key)=notice_key.as_deref() { shared.store.take_bot_state(key)?; }
                shared.store.set_session_busy(session.key, false)?;
                let sink_for_success = sink.clone();
                let sink_for_failure = sink.clone();
                let result = finalize_foreground_turn(
                    ForegroundTurnSuccess {
                        store: &shared.store,
                        session: &session,
                        turn_id,
                        review_mode: queued.request.review_mode.is_some(),
                        summary: &summary,
                    },
                    || async {
                        if fs::symlink_metadata(turn_workspace.root.join(interactions::MANIFEST)).is_ok() {
                        let native=summary.codex_thread_id.as_deref().context("human task source native thread was not observed")?;
                        interactions::register_tasks(&shared,session.key,queued.request.from_user_id,turn_id,native,&turn_workspace.root).await?;
                        }
                        send_generated_artifacts(&shared, &session, turn_id, &turn_workspace.out_dir, &summary.assistant_text).await
                    },
                    || async move { sink_for_success.lock().await.finish(None).await },
                    |message| async move { sink_for_failure.lock().await.finish(Some(message)).await },
                )
                .await;
                if result.is_ok() {
                    notify_turn_completion(&shared, session.key).await;
                }
                result
            }
            Err(error) => {
                let status = if error.to_string().contains("cancelled") {
                    "cancelled"
                } else {
                    "failed"
                };
                shared.store.set_session_busy(session.key, false)?;
                let recovery_note = saved_thread_is_unavailable(&error).then(||
                    "The saved native thread is unavailable. Its binding and your input are preserved. Recover the native context or explicitly clear this topic to start a new context.".to_string());
                finish_failed_turn(&shared.store, turn_id, &sink, status, &error, recovery_note)
                    .await?;
                Err(error)
            }
        }
    }
    .await;

    if shared
        .store
        .bot_state_value(&format!("stopped_turn:{turn_id}"))?
        .is_some()
    {
        // Preserve inputs and partial artifacts needed by the explicit continuation.
        return final_result;
    }
    finish_turn_cleanup(
        &queued.request.attachments,
        &turn_workspace.root,
        final_result,
    )
}

#[allow(clippy::too_many_arguments)]
async fn create_preview_sink_or_fail(
    shared: Arc<AppShared>,
    cancel_slot: Arc<StdMutex<Option<CancellationToken>>>,
    cancel: CancellationToken,
    chat_action_task: &mut Option<tokio::task::JoinHandle<()>>,
    queued: &QueuedTurn,
    session: &crate::models::SessionRecord,
    turn_id: i64,
    turn_workspace: &TurnWorkspace,
    turn_banner: Option<String>,
    placeholder_text: String,
) -> Result<Arc<Mutex<LiveTurnSink>>> {
    let placeholder = match shared
        .telegram
        .send_message(SendMessage::html(
            session.key.chat_id,
            Some(session.key.thread_id).filter(|value| *value != 0),
            placeholder_text,
        ))
        .await
        .context("failed to create placeholder message")
    {
        Ok(placeholder) => placeholder,
        Err(error) => {
            *cancel_slot.lock().expect("cancel mutex poisoned") = None;
            cancel.cancel();
            if let Some(chat_action_task) = chat_action_task.take() {
                let _ = chat_action_task.await;
            }
            if let Some(retry_after) = telegram_retry_after(&error) {
                notify_pre_codex_rate_limit(
                    shared.clone(),
                    session.key,
                    turn_id,
                    retry_after,
                    queued.request.attachments.is_empty(),
                );
            }
            finish_pre_codex_turn_failure(&shared, session, turn_id, &error)?;
            return finish_turn_cleanup(
                &queued.request.attachments,
                &turn_workspace.root,
                Err(error),
            );
        }
    };

    Ok(Arc::new(Mutex::new(LiveTurnSink::new_preview(
        shared,
        session,
        turn_banner,
        Some(TelegramMessageRef {
            chat_id: session.key.chat_id,
            message_id: placeholder.message_id,
        }),
    ))))
}

fn cleanup_paths(attachments: &[LocalAttachment], turn_root: &Path, remove_turn_root: bool) {
    for path in attachments.iter().map(|attachment| &attachment.path) {
        if let Err(error) = fs::remove_file(path) {
            tracing::warn!(
                "failed to remove temp attachment {}: {error}",
                path.display()
            );
        }
    }
    if !remove_turn_root {
        return;
    }
    if let Err(error) = fs::remove_dir_all(turn_root) {
        tracing::warn!(
            "failed to remove turn workspace {}: {error}",
            turn_root.display()
        );
    }
}

pub(super) fn finish_turn_cleanup<T>(
    attachments: &[LocalAttachment],
    turn_root: &Path,
    result: Result<T>,
) -> Result<T> {
    // Failed or uncertain deliveries need their output retained for explicit recovery.
    let output_exists = match fs::read_dir(turn_root.join("out")) {
        Ok(mut entries) => entries.next().is_some(),
        Err(error) => error.kind() != std::io::ErrorKind::NotFound,
    };
    let declaration_exists = fs::symlink_metadata(turn_root.join(interactions::MANIFEST)).is_ok();
    cleanup_paths(
        attachments,
        turn_root,
        result.is_ok() || !(output_exists || declaration_exists),
    );
    result
}

pub(super) struct ForegroundTurnSuccess<'a> {
    pub(super) store: &'a Store,
    pub(super) session: &'a crate::models::SessionRecord,
    pub(super) turn_id: i64,
    pub(super) review_mode: bool,
    pub(super) summary: &'a crate::codex::RunSummary,
}

pub(super) async fn finalize_foreground_turn<
    SendArtifacts,
    SendArtifactsFut,
    FinishSuccess,
    FinishSuccessFut,
    FinishFailure,
    FinishFailureFut,
>(
    context: ForegroundTurnSuccess<'_>,
    send_artifacts: SendArtifacts,
    finish_success: FinishSuccess,
    finish_failure: FinishFailure,
) -> Result<()>
where
    SendArtifacts: FnOnce() -> SendArtifactsFut,
    SendArtifactsFut: std::future::Future<Output = Result<()>>,
    FinishSuccess: FnOnce() -> FinishSuccessFut,
    FinishSuccessFut: std::future::Future<Output = Result<()>>,
    FinishFailure: FnOnce(String) -> FinishFailureFut,
    FinishFailureFut: std::future::Future<Output = Result<()>>,
{
    let success_result = async {
        if !context.review_mode {
            if let Some(thread_id) = context.summary.codex_thread_id.as_deref() {
                context
                    .store
                    .set_session_codex_thread(context.session.key, thread_id)?;
            }
        }
        context
            .store
            .set_last_assistant_text(context.session.key, Some(&context.summary.assistant_text))?;
        send_artifacts()
            .await
            .context("failed to deliver generated artifacts")?;
        finish_success()
            .await
            .context("failed to finalize Telegram turn message")?;
        context.store.record_turn_finished(
            context.turn_id,
            "completed",
            Some(&context.summary.assistant_text),
        )?;
        Ok(())
    }
    .await;

    if let Err(error) = success_result {
        context
            .store
            .record_turn_finished(context.turn_id, "failed", None)?;
        if let Err(finish_error) = finish_failure(format!("Turn failed: {error:#}")).await {
            tracing::warn!("failed to report foreground turn failure: {finish_error:#}");
        }
        return Err(error);
    }

    Ok(())
}

async fn finish_failed_turn(
    store: &Store,
    turn_id: i64,
    sink: &Arc<Mutex<LiveTurnSink>>,
    status: &str,
    error: &anyhow::Error,
    recovery_note: Option<String>,
) -> Result<()> {
    store.record_turn_finished(turn_id, status, None)?;
    let mut message = format!("Turn {status}: {error:#}");
    if let Some(note) = recovery_note {
        message.push_str("\n\n");
        message.push_str(&note);
    }
    sink.lock().await.finish(Some(message)).await?;
    Ok(())
}

fn finish_pre_codex_turn_failure(
    shared: &Arc<AppShared>,
    session: &crate::models::SessionRecord,
    turn_id: i64,
    error: &anyhow::Error,
) -> Result<()> {
    shared.store.set_session_busy(session.key, false)?;
    let assistant_text = if let Some(retry_after) = telegram_retry_after(error) {
        format!(
            "Telegram rate limit prevented this turn from starting. Retry after {retry_after}s."
        )
    } else {
        format!("Turn failed before Codex started: {error:#}")
    };
    shared
        .store
        .record_turn_finished(turn_id, "failed", Some(&assistant_text))?;
    Ok(())
}

fn notify_pre_codex_rate_limit(
    shared: Arc<AppShared>,
    session_key: SessionKey,
    turn_id: i64,
    retry_after: u64,
    can_retry: bool,
) {
    tokio::spawn(async move {
        let mut wait_seconds = retry_after.saturating_add(1);
        for attempt in 0..3 {
            sleep(Duration::from_secs(wait_seconds)).await;
            let mut request = SendMessage::html(
                session_key.chat_id,
                Some(session_key.thread_id).filter(|value| *value != 0),
                render_pre_codex_rate_limit_notice(turn_id, retry_after, can_retry),
            );
            if can_retry {
                request.reply_markup = Some(rate_limit_retry_keyboard(turn_id));
            }
            match shared.telegram.send_message(request).await {
                Ok(_) => return,
                Err(error) => {
                    if let Some(next_retry_after) = telegram_retry_after(&error) {
                        wait_seconds = next_retry_after.saturating_add(1);
                        tracing::warn!(
                            "Telegram still rate-limited retry notice for turn {turn_id}; attempt {} retry after {next_retry_after}s",
                            attempt + 1
                        );
                        continue;
                    }
                    tracing::warn!(
                        "failed to send Telegram rate-limit retry notice for turn {turn_id}: {error:#}"
                    );
                    return;
                }
            }
        }
    });
}

fn render_pre_codex_rate_limit_notice(turn_id: i64, retry_after: u64, can_retry: bool) -> String {
    let retry_line = if can_retry {
        "I waited for the recommended backoff. Use Retry to send the same prompt again."
    } else {
        "I waited for the recommended backoff. Send the request again manually because attachments cannot be retried."
    };
    format!(
        "Telegram rate limit hit before Codex could start turn <code>{turn_id}</code>.\n\
         Telegram asked to retry after <code>{retry_after}s</code>.\n\n{retry_line}"
    )
}

pub(super) fn rate_limit_retry_keyboard(turn_id: i64) -> InlineKeyboardMarkup {
    InlineKeyboardMarkup {
        inline_keyboard: vec![vec![InlineKeyboardButton {
            text: "Retry".to_string(),
            callback_data: Some(format!("cmd:/retry {turn_id}")),
            url: None,
        }]],
    }
}

async fn notify_turn_completion(shared: &Arc<AppShared>, session_key: SessionKey) {
    let Some(text) =
        turn_completion_notification_text(&shared.config.telegram.completion_notify_usernames)
    else {
        return;
    };
    let result = shared
        .telegram
        .send_message(SendMessage::html(
            session_key.chat_id,
            Some(session_key.thread_id).filter(|value| *value != 0),
            html_escape::encode_safe(&text).to_string(),
        ))
        .await;
    if let Err(error) = result {
        tracing::warn!("failed to send turn completion notification: {error:#}");
    }
}

pub(super) fn turn_completion_notification_text(usernames: &[String]) -> Option<String> {
    if usernames.is_empty() {
        return None;
    }
    Some(format!("Готово, {} fyi ✅", usernames.join(" ")))
}

pub(super) fn saved_thread_is_unavailable(error: &anyhow::Error) -> bool {
    let pretty = format!("{error:#}").to_ascii_lowercase();
    let display = error.to_string().to_ascii_lowercase();
    let matches = |text: &str| {
        text.contains("no rollout found for thread id")
            || (text.contains("rollout")
                && text.contains("thread id")
                && text.contains("code -32600"))
    };
    matches(&pretty) || matches(&display)
}

struct LiveTurnSink {
    turn_id: Option<i64>,
    shared: Arc<AppShared>,
    session_key: SessionKey,
    messages: Vec<TelegramMessageRef>,
    draft_id: Option<i64>,
    limits_inline: Option<String>,
    pending_text: String,
    has_assistant_text: bool,
    message_committed: bool,
    last_flushed_text: String,
    last_flush_at: Instant,
    edit_backoff_until: Option<Instant>,
}

impl LiveTurnSink {
    fn new_preview(
        shared: Arc<AppShared>,
        session: &crate::models::SessionRecord,
        limits_inline: Option<String>,
        placeholder: Option<TelegramMessageRef>,
    ) -> Self {
        let pending_text = if placeholder.is_some() {
            "⏳".to_string()
        } else {
            String::new()
        };
        Self {
            shared,
            session_key: session.key,
            turn_id: None,
            messages: placeholder.into_iter().collect(),
            draft_id: None,
            limits_inline,
            pending_text,
            has_assistant_text: false,
            message_committed: false,
            last_flushed_text: String::new(),
            last_flush_at: Instant::now() - Duration::from_secs(60),
            edit_backoff_until: None,
        }
    }

    fn new_draft(
        shared: Arc<AppShared>,
        session: &crate::models::SessionRecord,
        limits_inline: Option<String>,
        draft_id: i64,
    ) -> Self {
        Self {
            shared,
            session_key: session.key,
            turn_id: None,
            messages: Vec::new(),
            draft_id: Some(draft_id),
            limits_inline,
            pending_text: "⏳".to_string(),
            has_assistant_text: false,
            message_committed: false,
            last_flushed_text: String::new(),
            last_flush_at: Instant::now() - Duration::from_secs(60),
            edit_backoff_until: None,
        }
    }

    async fn handle_event(&mut self, event: CodexEvent) -> Result<()> {
        match event {
            CodexEvent::AssistantText(ref text)
            | CodexEvent::CommentaryCompleted(ref text)
            | CodexEvent::FinalAnswerCompleted(ref text)
                if text.trim().is_empty() =>
            {
                // Empty native items are lifecycle events, not Telegram messages.
                // Do not erase a draft or mutate an already published answer.
                return Ok(());
            }
            CodexEvent::Progress(text) => {
                if self.shared.config.telegram.show_unfinished_messages && !self.has_assistant_text
                {
                    self.pending_text = progress_status_text(&text);
                }
            }
            CodexEvent::AssistantText(text) => {
                self.begin_next_message();
                self.pending_text = text;
                self.has_assistant_text = true;
            }
            CodexEvent::CommentaryCompleted(text) | CodexEvent::FinalAnswerCompleted(text) => {
                self.begin_next_message();
                self.pending_text = text;
                self.has_assistant_text = true;
                self.flush(true).await?;
                // Only an acknowledged permanent publication may freeze this message.
                if self.last_flushed_text != self.pending_text {
                    anyhow::bail!("completed assistant publication was deferred by Telegram");
                }
                self.message_committed = true;
                return Ok(());
            }
            CodexEvent::ThreadStarted(thread_id) => {
                tracing::debug!("codex thread started: {thread_id}");
            }
            CodexEvent::NativeMessageCompleted { .. }
            | CodexEvent::UserInputRequest { .. }
            | CodexEvent::NativeInputRejected
            | CodexEvent::NativeInputAccepted(_)
            | CodexEvent::NativeTurnStarted(_)
            | CodexEvent::NativeTurnSubmitting
            | CodexEvent::NativeTurnTerminal { .. }
            | CodexEvent::UserInputClosed(_)
            | CodexEvent::UserInputAnswered(_) => {}
            CodexEvent::ApprovalRequest(request) => {
                if self.shared.config.telegram.show_unfinished_messages && !self.has_assistant_text
                {
                    self.pending_text = approval_waiting_text(request.kind);
                }
            }
        }
        self.flush(false).await
    }

    async fn set_progress(&mut self, text: impl Into<String>) -> Result<()> {
        if self.shared.config.telegram.show_unfinished_messages && !self.has_assistant_text {
            self.pending_text = progress_status_text(&text.into());
            self.flush(false).await?;
        }
        Ok(())
    }

    async fn finish(&mut self, final_error: Option<String>) -> Result<()> {
        if self.message_committed {
            if final_error.is_none() {
                return Ok(());
            }
            self.begin_next_message();
            self.pending_text.clear();
        }
        if let Some(final_error) = final_error {
            self.pending_text = if self.pending_text.trim().is_empty() {
                final_error
            } else {
                format!("{}\n\n{}", self.pending_text, final_error)
            };
        }
        self.flush(true).await?;
        self.message_committed = self.last_flushed_text == self.pending_text;
        Ok(())
    }

    fn begin_next_message(&mut self) {
        if self.message_committed {
            // References to committed messages must never be reused for another item.
            self.messages.clear();
            self.last_flushed_text.clear();
            self.message_committed = false;
        }
    }

    async fn flush(&mut self, force: bool) -> Result<()> {
        if (!force && !self.shared.config.telegram.show_unfinished_messages)
            || (force && self.pending_text.trim().is_empty())
        {
            return Ok(());
        }
        if self.message_committed {
            return Ok(());
        }
        if self
            .edit_backoff_until
            .is_some_and(|until| until <= Instant::now())
        {
            self.edit_backoff_until = None;
        }
        if self
            .edit_backoff_until
            .is_some_and(|until| until > Instant::now())
        {
            return Ok(());
        }
        let visible_text = self.visible_text(force);
        let min_flush_interval = self.min_flush_interval();
        if !force
            && self.last_flushed_text == visible_text
            && self.last_flush_at.elapsed() < min_flush_interval
        {
            return Ok(());
        }
        if !force && self.last_flush_at.elapsed() < min_flush_interval {
            return Ok(());
        }

        if self.draft_id.is_some() && !force {
            return self.flush_draft(&visible_text).await;
        }

        let chunks = if force {
            split_text(&visible_text, self.shared.config.max_text_chunk)
        } else {
            vec![truncate_for_live_update(
                &visible_text,
                self.shared.config.max_text_chunk,
            )]
        };
        for (idx, chunk) in chunks.iter().enumerate() {
            let html = render_markdown_to_html(chunk);
            if let Some(existing) = self
                .messages
                .get(idx)
                .cloned()
                .filter(|_| self.draft_id.is_none())
            {
                self.edit_message(existing, chunk, &html).await?;
            } else {
                let result = self
                    .shared
                    .telegram
                    .send_message(SendMessage::html(
                        self.session_key.chat_id,
                        Some(self.session_key.thread_id).filter(|value| *value != 0),
                        html.clone(),
                    ))
                    .await;
                let message = match result {
                    Ok(message) => message,
                    Err(error) if self.defer_after_retry_after(&error, "telegram send chunk") => {
                        return Ok(());
                    }
                    Err(error) => return Err(error),
                };
                let reference = TelegramMessageRef {
                    chat_id: self.session_key.chat_id,
                    message_id: message.message_id,
                };
                self.messages.push(reference);
            }
        }

        if force {
            if let Some(turn_id) = self.turn_id {
                for (reference, chunk) in self.messages.iter().zip(chunks.iter()) {
                    self.shared.store.record_handoff_message(
                        self.session_key,
                        turn_id,
                        reference.message_id,
                        chunk,
                    )?;
                }
            }
        }
        self.last_flushed_text = visible_text;
        self.last_flush_at = Instant::now();
        Ok(())
    }

    fn min_flush_interval(&self) -> Duration {
        let configured = Duration::from_millis(self.shared.config.edit_debounce_ms);
        configured.max(crate::telegram::outbound_interval_for_chat(
            self.session_key.chat_id,
        ))
    }

    async fn flush_draft(&mut self, visible_text: &str) -> Result<()> {
        let Some(draft_id) = self.draft_id else {
            return Ok(());
        };
        let draft_text = truncate_for_live_update(visible_text, self.shared.config.max_text_chunk);
        let html = render_markdown_to_html(&draft_text);
        match self
            .shared
            .telegram
            .send_message_draft(SendMessageDraft::html(
                self.session_key.chat_id,
                Some(self.session_key.thread_id).filter(|value| *value != 0),
                draft_id,
                html,
            ))
            .await
        {
            Ok(_) => {
                self.last_flushed_text = visible_text.to_string();
                self.last_flush_at = Instant::now();
            }
            Err(error) if self.defer_after_retry_after(&error, "telegram draft update") => {}
            Err(error) => {
                tracing::debug!("sendMessageDraft update failed: {error:#}");
                self.last_flushed_text = visible_text.to_string();
                self.last_flush_at = Instant::now();
            }
        }
        Ok(())
    }

    fn visible_text(&self, force: bool) -> String {
        if force {
            return self.pending_text.clone();
        }
        match self.limits_inline.as_deref() {
            Some(limits_inline) if !limits_inline.is_empty() => {
                format!("{limits_inline}\n{}", self.pending_text)
            }
            _ => self.pending_text.clone(),
        }
    }

    async fn edit_message(
        &mut self,
        reference: TelegramMessageRef,
        raw_text: &str,
        html: &str,
    ) -> Result<()> {
        let result = self
            .shared
            .telegram
            .edit_message_text(EditMessageText::html(
                reference.chat_id,
                reference.message_id,
                html.to_string(),
            ))
            .await;
        match result {
            Ok(_) => Ok(()),
            Err(error) => {
                if is_message_not_modified(&error) {
                    return Ok(());
                }
                if self.defer_after_retry_after(&error, "telegram edit") {
                    return Ok(());
                }
                let fallback = html_escape::encode_safe(raw_text).to_string();
                let fallback_result = self
                    .shared
                    .telegram
                    .edit_message_text(EditMessageText::html(
                        reference.chat_id,
                        reference.message_id,
                        fallback,
                    ))
                    .await;
                match fallback_result {
                    Ok(_) => Ok(()),
                    Err(fallback_error) if is_message_not_modified(&fallback_error) => Ok(()),
                    Err(fallback_error)
                        if self
                            .defer_after_retry_after(&fallback_error, "telegram edit fallback") =>
                    {
                        Ok(())
                    }
                    Err(fallback_error) => Err(fallback_error)
                        .with_context(|| format!("telegram edit fallback failed after: {error:#}")),
                }
            }
        }
    }

    fn defer_after_retry_after(&mut self, error: &anyhow::Error, label: &str) -> bool {
        let Some(retry_after) = telegram_retry_after(error) else {
            return false;
        };
        let until = Instant::now() + Duration::from_secs(retry_after.saturating_add(1));
        self.edit_backoff_until = Some(until);
        tracing::warn!("{label} hit Telegram rate limit, backing off for {retry_after}s");
        true
    }
}

pub(super) fn truncate_for_live_update(text: &str, max_len: usize) -> String {
    if max_len == 0 {
        return String::new();
    }
    split_text(text, max_len)
        .into_iter()
        .next()
        .unwrap_or_default()
}

fn render_placeholder_html(status: &str, limits_banner: Option<&str>) -> String {
    let status = html_escape::encode_safe(status);
    match limits_banner {
        Some(limits_banner) if !limits_banner.is_empty() => {
            format!(
                "{}\n<i>{status}</i>",
                html_escape::encode_safe(limits_banner)
            )
        }
        _ => format!("<i>{status}</i>"),
    }
}

fn turn_start_banner(
    limits_inline: Option<String>,
    session: &crate::models::SessionRecord,
) -> Option<String> {
    let mut lines = Vec::new();
    if let Some(limits_inline) = limits_inline.filter(|value| !value.trim().is_empty()) {
        lines.push(limits_inline);
    }
    if session.service_tier.as_deref() == Some("fast") {
        lines.push(
            "⚡ Fast mode is ON for this session; Codex may use faster inference with higher plan usage. Disable it with `/fast off`.".to_string(),
        );
    }
    if lines.is_empty() {
        None
    } else {
        Some(lines.join("\n"))
    }
}

fn progress_status_text(text: &str) -> String {
    let text = text.trim();
    if text.is_empty() {
        "⏳".to_string()
    } else {
        format!("⏳ {text}")
    }
}

async fn enrich_audio_transcripts(
    shared: &Arc<AppShared>,
    request: &mut TurnRequest,
    workspace: &TurnWorkspace,
    sink: &Arc<Mutex<LiveTurnSink>>,
) {
    let Some(model_dir) = shared.handy_model_dir.clone() else {
        return;
    };

    let total = request
        .attachments
        .iter()
        .filter(|attachment| {
            matches!(
                attachment.kind,
                AttachmentKind::Audio | AttachmentKind::Voice
            )
        })
        .count();

    if total == 0 {
        return;
    }

    for (idx, attachment) in request
        .attachments
        .iter_mut()
        .filter(|attachment| {
            matches!(
                attachment.kind,
                AttachmentKind::Audio | AttachmentKind::Voice
            )
        })
        .enumerate()
    {
        let label = format!(
            "Transcribing audio {}/{} with Handy model...",
            idx + 1,
            total
        );
        if let Err(error) = sink.lock().await.set_progress(label).await {
            tracing::debug!("failed to update transcription progress: {error:#}");
        }

        match transcribe_audio_file(
            model_dir.clone(),
            attachment.path.clone(),
            workspace.root.clone(),
        )
        .await
        {
            Ok(transcript) => {
                attachment.transcript = Some(transcript);
            }
            Err(error) => {
                tracing::warn!(
                    "audio transcription failed for {}: {error:#}",
                    attachment.path.display()
                );
            }
        }
    }
}

fn has_preferred_audio_transcript(attachment: &LocalAttachment) -> bool {
    matches!(
        attachment.kind,
        AttachmentKind::Audio | AttachmentKind::Voice
    ) && attachment.transcript.is_some()
}

fn is_default_attachment_prompt(prompt: &str) -> bool {
    prompt.trim() == "Analyze the attached files."
}

fn prepare_turn_workspace(
    session: &crate::models::SessionRecord,
    turn_id: i64,
) -> Result<TurnWorkspace> {
    let root = session
        .cwd
        .join(".telecodex")
        .join("turns")
        .join(format!("{turn_id}-{}", Uuid::now_v7()));
    let out_dir = root.join("out");
    fs::create_dir_all(&out_dir)?;
    Ok(TurnWorkspace { root, out_dir })
}

pub(super) fn prepare_runtime_request(
    session: &crate::models::SessionRecord,
    request: &TurnRequest,
    workspace: &TurnWorkspace,
) -> TurnRequest {
    let mut user_prompt_sections = Vec::new();
    let mut instruction_sections = Vec::new();
    let non_transcript_attachment_lines = request
        .attachments
        .iter()
        .filter(|attachment| !has_preferred_audio_transcript(attachment))
        .map(|attachment| {
            let mime = attachment.mime_type.as_deref().unwrap_or("unknown");
            format!(
                "- {} -> {} ({mime}, {})",
                attachment.file_name,
                attachment.path.display(),
                attachment_kind_label(attachment.kind)
            )
        })
        .collect::<Vec<_>>();
    let transcript_texts = request
        .attachments
        .iter()
        .filter_map(|attachment| {
            attachment
                .transcript
                .as_ref()
                .map(|transcript| transcript.text.trim())
        })
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>();

    let omit_default_attachment_prompt = is_default_attachment_prompt(&request.prompt)
        && !transcript_texts.is_empty()
        && request
            .attachments
            .iter()
            .all(has_preferred_audio_transcript);

    if !request.prompt.trim().is_empty() && !omit_default_attachment_prompt {
        user_prompt_sections.push(request.prompt.trim().to_string());
    }

    for transcript_text in transcript_texts {
        user_prompt_sections.push(transcript_text.to_string());
    }

    if !non_transcript_attachment_lines.is_empty() {
        user_prompt_sections.push(format!(
            "Local files for this turn:\n{}\nUse these exact files as input if the request refers to an attachment.",
            non_transcript_attachment_lines.join("\n")
        ));
    }

    if !request.attachments.is_empty() && !non_transcript_attachment_lines.is_empty() {
        instruction_sections.push(format!(
            "Attached local files:\n{}\nRead and use them if relevant.",
            non_transcript_attachment_lines.join("\n")
        ));
    }

    instruction_sections.push(format!(
        "This is the current turn's output contract. It supersedes output directories in earlier conversation context. If you generate final deliverable images or files for the user, save only the final files in this directory:\n{}\nTelecodex automatically uploads files from that directory back to Telegram after your turn completes. Do not use /tmp paths, local Markdown image links, base64 blobs, or invented upload methods to deliver files. view_image is only for local inspection. Keep intermediate scratch files outside this directory.",
        workspace.out_dir.display()
    ));

    instruction_sections.push(
        "Keep Telegram conversation natural and direct. Use ordinary chat for discussion, simple confirmations, status questions and spontaneous feedback. Use native question widgets only when the human explicitly asks for one or a consequential choice genuinely needs structured human input; otherwise continue with reasonable judgment or ask briefly in chat. Declare a durable /pending task only for a concrete important external action that must remain discoverable after this turn, such as a needed device test or credential handoff. Do not turn ordinary questions, optional suggestions or routine follow-up into pending tasks. Do not duplicate a request that is already pending."
            .to_string(),
    );

    instruction_sections.push(format!(
        "If a human action must remain recoverable after this turn (for example a phone test), explicitly declare it in {}. This optional regular JSON file is outside out and is not uploaded. Schema: {{\"schema\":\"telecodex.human-requests/v1\",\"requests\":[{{\"id\":\"phone-test\",\"title\":\"Test playback\",\"action\":\"Open the app and report playback.\",\"links\":[]}}]}}. Use at most 10 requests, unique IDs (64 ASCII letters/digits/dash/underscore), titles <=120 characters, actions <=8000 characters and at most 5 public http(s) links {{\"label\":\"App\",\"url\":\"https://example.org\"}}. Never put secrets or private endpoints into previews or links. Telecodex admits the declaration only after successful native completion and scopes it to this turn's human, topic and native thread. /pending (also /questions) reopens it. Do not declare a request already answered; ordinary prose creates no durable request. To retire a task issued in this same human/topic/native thread when its action is no longer needed, add optional withdrawals: [{{\"id\":\"retained-request-id\",\"reason\":\"Explicit reason and observed source\",\"links\":[]}}]. Use the exact retained task ID, at most 10 total requests/withdrawals, and reasons <=2000 characters. Native questions and uncertain receipts cannot be withdrawn this way. Withdrawal records an agent decision with an audit trail, never human approval or completion.",
        workspace.root.join(interactions::MANIFEST).display()
    ));

    let mut runtime_request = request.clone();
    runtime_request.prompt = user_prompt_sections.join("\n\n");
    runtime_request.runtime_instructions = Some(instruction_sections.join("\n\n"));
    if runtime_request.prompt.trim().is_empty() {
        runtime_request.prompt = "Follow the session instructions.".to_string();
    }
    if session.session_prompt.is_some() {
        tracing::debug!("session prompt is active for {:?}", session.key);
    }
    runtime_request
}

async fn enrich_runtime_request_with_context_pressure(
    session: &crate::models::SessionRecord,
    request: TurnRequest,
) -> TurnRequest {
    let Some(note) = context_pressure_note_for_session(session).await else {
        return request;
    };
    apply_context_pressure_note_to_request(request, &note)
}

fn apply_context_pressure_note_to_request(mut request: TurnRequest, note: &str) -> TurnRequest {
    // Codex app-server applies developer instructions when a thread is started or
    // resumed, but existing-thread resume instructions are not a reliable
    // per-turn delivery channel. Put the runtime note into the next turn's input
    // so the model sees it immediately on milestone crossings.
    request.prompt = if request.prompt.trim().is_empty() {
        note.to_string()
    } else {
        format!("{note}\n\nUser message:\n{}", request.prompt)
    };
    request
}

async fn context_pressure_note_for_session(
    session: &crate::models::SessionRecord,
) -> Option<String> {
    let python = std::env::var("TELECODEX_CONTEXT_PRESSURE_PYTHON")
        .unwrap_or_else(|_| "python3".to_string());
    let budimo_root = PathBuf::from(
        std::env::var("TELECODEX_CONTEXT_PRESSURE_BUDIMO_ROOT")
            .unwrap_or_else(|_| "/home/hermes/mobius-workspace/repos/budimo".to_string()),
    );
    let pythonpath = [
        budimo_root.join("modules/context-common"),
        budimo_root.join("modules/context-check"),
        budimo_root.join("modules/context-pressure"),
    ]
    .into_iter()
    .map(|path| path.display().to_string())
    .collect::<Vec<_>>()
    .join(":");
    let mut command = tokio::process::Command::new(python);
    command
        .arg("-m")
        .arg("context_pressure.preturn")
        .arg("--agent")
        .arg("o")
        .arg("--thread-id")
        .arg(session.key.thread_id.to_string())
        .env("PYTHONPATH", pythonpath);
    if let Ok(store) = std::env::var("CONTEXT_PRESSURE_ACTIVATION_STORE") {
        command.arg("--store").arg(store);
    }
    if let Ok(state_store) = std::env::var("CONTEXT_PRESSURE_STATE_STORE") {
        command.arg("--state-store").arg(state_store);
    }
    if std::env::var("CONTEXT_PRESSURE_ALLOW_DRY_RUN").as_deref() == Ok("1") {
        command.arg("--allow-dry-run");
    }
    let output = match tokio::time::timeout(Duration::from_millis(900), command.output()).await {
        Ok(Ok(output)) => output,
        Ok(Err(error)) => {
            tracing::debug!("context pressure preturn helper failed to spawn: {error:#}");
            return None;
        }
        Err(_) => {
            tracing::debug!("context pressure preturn helper timed out");
            return None;
        }
    };
    if !output.status.success() {
        tracing::debug!(
            "context pressure preturn helper exited with status {:?}",
            output.status.code()
        );
        return None;
    }
    let note = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if note.is_empty() { None } else { Some(note) }
}

fn enrich_runtime_request_with_codex_history(
    session: &crate::models::SessionRecord,
    mut request: TurnRequest,
) -> TurnRequest {
    let Some(thread_id) = session.codex_thread_id.as_deref() else {
        return request;
    };
    let Ok(history) = read_thread_history(&default_codex_home(), thread_id, 8) else {
        return request;
    };
    if history.is_empty() {
        return request;
    }
    let context = format_codex_history_context(&history);
    if context.is_empty() {
        return request;
    }
    request.runtime_instructions = Some(match request.runtime_instructions.take() {
        Some(existing) if !existing.trim().is_empty() => format!("{existing}\n\n{context}"),
        _ => context,
    });
    request
}

pub(super) fn format_codex_history_context(entries: &[CodexHistoryEntry]) -> String {
    let mut lines = vec![
        "Recent conversation context from the selected Codex session:".to_string(),
        "Use this as prior chat history and continue from it instead of starting from scratch."
            .to_string(),
    ];
    for entry in entries {
        let label = if entry.role == "assistant" {
            "Assistant"
        } else {
            "User"
        };
        lines.push(format!(
            "{label}: {}",
            truncate_history_context(&entry.text).replace('\n', " ")
        ));
    }
    lines.join("\n")
}

fn truncate_history_context(text: &str) -> String {
    const LIMIT: usize = 900;
    if text.chars().count() <= LIMIT {
        return text.to_string();
    }
    text.chars().take(LIMIT).collect::<String>() + "..."
}

fn spawn_chat_action_task(
    telegram: TelegramClient,
    chat_id: i64,
    thread_id: Option<i64>,
    cancel: CancellationToken,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        while !cancel.is_cancelled() {
            if let Err(error) = telegram
                .send_chat_action(chat_id, thread_id, ChatAction::Typing)
                .await
            {
                tracing::debug!("sendChatAction failed: {error:#}");
            }
            tokio::select! {
                _ = cancel.cancelled() => break,
                _ = sleep(Duration::from_secs(4)) => {}
            }
        }
    })
}

async fn latest_limits_snapshot_from_shared(
    shared: &Arc<AppShared>,
) -> Result<Option<LimitsSnapshot>> {
    let mut cache = shared.limits_cache.lock().await;
    if let Some(cached) = cache.as_ref() {
        if cached.fetched_at.elapsed() < Duration::from_secs(60) {
            return Ok(Some(cached.snapshot.clone()));
        }
    }

    let snapshot = find_latest_limits_snapshot(&default_codex_home())?;
    if let Some(snapshot) = snapshot.clone() {
        *cache = Some(CachedLimitsSnapshot {
            fetched_at: Instant::now(),
            snapshot,
        });
    }
    Ok(snapshot)
}

pub(super) fn resolve_session_codex_binding_from_history(
    shared: &Arc<AppShared>,
    session: crate::models::SessionRecord,
) -> Result<crate::models::SessionRecord> {
    if shared
        .config
        .tribu
        .as_ref()
        .is_some_and(|p| p.key() == session.key)
    {
        shared.store.tribu_check_thread(&session)?;
        return Ok(session);
    }
    if super::nexus::tracks(shared, session.key)? {
        return Ok(session);
    }
    if session.force_fresh_thread || !shared.config.codex.auto_attach_latest_history {
        return Ok(session);
    }
    let codex_home = default_codex_home();
    if let Some(thread_id) = session.codex_thread_id.as_deref() {
        if crate::codex_history::is_thread_active(&codex_home, thread_id)? {
            return Ok(session);
        }
    }
    let Some(summary) = latest_thread_for_cwd(&codex_home, &session.cwd)? else {
        return Ok(session);
    };
    shared
        .store
        .set_session_codex_thread(session.key, &summary.id)?;
    if !session_title_is_present(&session) {
        shared.store.set_session_title(
            session.key,
            Some(summary.title.trim()).filter(|title| !title.is_empty()),
        )?;
    }
    shared
        .store
        .get_session(session.key)?
        .ok_or_else(|| anyhow!("failed to reload bound session"))
}

async fn send_generated_artifacts(
    shared: &Arc<AppShared>,
    session: &crate::models::SessionRecord,
    turn_id: i64,
    out_dir: &Path,
    assistant_text: &str,
) -> Result<()> {
    validate_artifact_references(out_dir, assistant_text)?;
    let directory = fs::symlink_metadata(out_dir)?;
    if !directory.is_dir() || directory.file_type().is_symlink() {
        bail!("Artifact output directory is not a regular directory; no files were uploaded");
    }

    let mut entries = Vec::new();
    for entry in fs::read_dir(out_dir)? {
        let entry = entry?;
        let metadata = entry.metadata()?;
        if metadata.file_type().is_symlink() {
            bail!("Artifact delivery refused a symbolic link; no linked files were uploaded");
        }
        if !metadata.is_file() {
            bail!("Artifact output supports only top-level regular files; no files were uploaded");
        }
        entries.push(entry.path());
    }
    entries.sort();
    if entries.len() > 10 {
        bail!("Artifact delivery supports at most 10 files per turn; no files were uploaded");
    }
    for (accepted, path) in entries.into_iter().enumerate() {
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        let action = match extension.as_str() {
            "png" | "jpg" | "jpeg" | "webp" => ChatAction::UploadPhoto,
            "mp3" | "wav" | "m4a" | "ogg" => ChatAction::UploadAudio,
            "mp4" | "mov" | "webm" => ChatAction::UploadVideo,
            _ => ChatAction::UploadDocument,
        };
        let _ = shared
            .telegram
            .send_chat_action(
                session.key.chat_id,
                Some(session.key.thread_id).filter(|value| *value != 0),
                action,
            )
            .await;
        let file_name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("artifact.bin")
            .to_string();
        let mime_type = mime_type_for_path(&path);
        shared.store.audit(
            None,
            "artifact_delivery_attempted",
            serde_json::json!({
                "session":session.id,"turn":turn_id,"file":file_name
            }),
        )?;
        let receipt = async { match action {
            ChatAction::UploadPhoto => {
                shared
                    .telegram
                    .send_photo(
                        session.key.chat_id,
                        Some(session.key.thread_id).filter(|value| *value != 0),
                        &path,
                        &file_name,
                        mime_type.as_deref(),
                    )
                    .await
            }
            ChatAction::UploadAudio => {
                shared
                    .telegram
                    .send_audio(
                        session.key.chat_id,
                        Some(session.key.thread_id).filter(|value| *value != 0),
                        &path,
                        &file_name,
                        mime_type.as_deref(),
                    )
                    .await
            }
            ChatAction::UploadVideo => {
                shared
                    .telegram
                    .send_video(
                        session.key.chat_id,
                        Some(session.key.thread_id).filter(|value| *value != 0),
                        &path,
                        &file_name,
                        mime_type.as_deref(),
                    )
                    .await
            }
            _ => {
                shared
                    .telegram
                    .send_document(
                        session.key.chat_id,
                        Some(session.key.thread_id).filter(|value| *value != 0),
                        &path,
                        &file_name,
                        mime_type.as_deref(),
                    )
                    .await
            }
        }}.await.with_context(|| format!(
            "Artifact '{file_name}' delivery unconfirmed; {accepted} file(s) already accepted by Telegram. Output is retained. No automatic retry."
        ))?;
        if receipt.chat.id != session.key.chat_id
            || receipt.message_thread_id.unwrap_or(0) != session.key.thread_id
        {
            bail!(
                "Artifact receipt does not match this topic; delivery unconfirmed. Output is retained. No automatic retry."
            );
        }
        shared.store.audit(
            None,
            "artifact_delivery_accepted",
            serde_json::json!({
                "session":session.id,"turn":turn_id,"file":file_name,"message_id":receipt.message_id
            }),
        )?;
    }

    Ok(())
}

fn validate_artifact_references(out_dir: &Path, assistant_text: &str) -> Result<()> {
    // Inspect only explicit textual output references; never inspect historical directories.
    let pattern = regex::Regex::new(r#"\.telecodex/turns/[^\s/`<>]+/out(?:/[^\s`<>\)\]\"]+)?"#)?;
    let current = out_dir.to_string_lossy();
    let current = current
        .find(".telecodex/turns/")
        .map(|i| &current[i..])
        .unwrap_or(&current);
    for reference in pattern.find_iter(assistant_text) {
        let reference = reference
            .as_str()
            .trim_end_matches(['.', ',', ';', ':', '!']);
        if reference != current && !reference.starts_with(&format!("{current}/")) {
            bail!(
                "The answer references an earlier turn's output directory. No historical files were uploaded. Generate deliverables in the current turn's output directory."
            );
        }
        if let Some(file) = reference.strip_prefix(&format!("{current}/")) {
            let relative = Path::new(file);
            if relative.components().count() != 1
                || relative
                    .components()
                    .any(|c| !matches!(c, std::path::Component::Normal(_)))
                || !out_dir.join(relative).is_file()
            {
                bail!(
                    "The answer references a missing or unsupported current-turn artifact. Its delivery is not confirmed."
                );
            }
        }
    }
    Ok(())
}

fn attachment_kind_label(kind: AttachmentKind) -> &'static str {
    match kind {
        AttachmentKind::Image => "image",
        AttachmentKind::Text => "text",
        AttachmentKind::Audio => "audio",
        AttachmentKind::Voice => "voice",
        AttachmentKind::Video => "video",
        AttachmentKind::Document => "document",
    }
}

pub(super) fn classify_document_kind(
    mime_type: Option<&str>,
    file_name: Option<&str>,
) -> AttachmentKind {
    let mime_type = mime_type.unwrap_or_default().to_ascii_lowercase();
    if mime_type.starts_with("image/") {
        return AttachmentKind::Image;
    }
    if mime_type.starts_with("text/")
        || matches!(
            file_name
                .and_then(|name| Path::new(name).extension())
                .and_then(|value| value.to_str())
                .map(|value| value.to_ascii_lowercase())
                .as_deref(),
            Some("txt" | "md" | "json" | "yaml" | "yml" | "toml" | "csv" | "tsv" | "log")
        )
    {
        return AttachmentKind::Text;
    }
    AttachmentKind::Document
}

pub(super) fn sanitize_file_name(file_name: &str, fallback_extension: &str) -> String {
    let mut cleaned = file_name
        .chars()
        .map(|ch| match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '.' | '_' | '-' => ch,
            _ => '_',
        })
        .collect::<String>();
    if cleaned.is_empty() {
        cleaned = format!("file.{fallback_extension}");
    } else if Path::new(&cleaned).extension().is_none() && !fallback_extension.is_empty() {
        cleaned.push('.');
        cleaned.push_str(fallback_extension);
    }
    cleaned
}

fn mime_type_for_path(path: &Path) -> Option<String> {
    match path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "png" => Some("image/png".to_string()),
        "jpg" | "jpeg" => Some("image/jpeg".to_string()),
        "webp" => Some("image/webp".to_string()),
        "gif" => Some("image/gif".to_string()),
        "mp3" => Some("audio/mpeg".to_string()),
        "wav" => Some("audio/wav".to_string()),
        "m4a" => Some("audio/mp4".to_string()),
        "ogg" => Some("audio/ogg".to_string()),
        "mp4" => Some("video/mp4".to_string()),
        "mov" => Some("video/quicktime".to_string()),
        "webm" => Some("video/webm".to_string()),
        "pdf" => Some("application/pdf".to_string()),
        "txt" => Some("text/plain".to_string()),
        "md" => Some("text/markdown".to_string()),
        "json" => Some("application/json".to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SearchMode;

    fn session_with_service_tier(service_tier: Option<&str>) -> crate::models::SessionRecord {
        crate::models::SessionRecord {
            id: 1,
            key: SessionKey::new(1, Some(2)),
            session_title: None,
            codex_thread_id: None,
            force_fresh_thread: false,
            updated_at: "2026-03-13T10:00:00Z".to_string(),
            cwd: std::env::temp_dir(),
            model: None,
            reasoning_effort: None,
            service_tier: service_tier.map(ToOwned::to_owned),
            collaboration_mode: None,
            session_prompt: None,
            sandbox_mode: "workspace-write".to_string(),
            approval_policy: "never".to_string(),
            search_mode: SearchMode::Disabled,
            add_dirs: vec![],
            busy: false,
        }
    }

    #[test]
    fn turn_start_banner_mentions_fast_mode() {
        let session = session_with_service_tier(Some("fast"));
        let banner = turn_start_banner(Some("Limits: ok".to_string()), &session).unwrap();

        assert!(banner.contains("Limits: ok"));
        assert!(banner.contains("⚡ Fast mode is ON"));
        assert!(banner.contains("/fast off"));
    }

    #[test]
    fn context_pressure_note_enters_turn_prompt_input() {
        let request = TurnRequest {
            session_key: SessionKey::new(1, Some(2)),
            from_user_id: 42,
            prompt: "Hi. What should we do next?".to_string(),
            runtime_instructions: Some("existing runtime instruction".to_string()),
            attachments: vec![],
            review_mode: None,
            override_search_mode: None,
        };

        let enriched = apply_context_pressure_note_to_request(
            request,
            "[CONTEXT PRESSURE] 200K reached; preserve state.",
        );

        assert!(
            enriched
                .prompt
                .starts_with("[CONTEXT PRESSURE] 200K reached; preserve state.")
        );
        assert!(enriched.prompt.contains(
            "User message:
Hi. What should we do next?"
        ));
        assert_eq!(
            enriched.runtime_instructions.as_deref(),
            Some("existing runtime instruction")
        );
    }
    #[test]
    fn turn_start_banner_omits_fast_mode_when_disabled() {
        let session = session_with_service_tier(None);
        let banner = turn_start_banner(Some("Limits: ok".to_string()), &session).unwrap();

        assert_eq!(banner, "Limits: ok");
    }
}
