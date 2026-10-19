//! A finalized voice clip becomes one attributed input; slow ASR holds no dispatcher.
use super::*;

tokio::task_local! {
    // Tokens identify exact owned executions. Never look up a newer current turn
    // when a delayed transcript finally identifies its destination.
    pub(super) static INTERRUPT_TARGETS: HashMap<SessionKey, CancellationToken>;
}

pub(super) fn interrupt_snapshot(target: SessionKey) -> Option<bool> {
    INTERRUPT_TARGETS
        .try_with(|targets| {
            targets.get(&target).is_some_and(|token| {
                if token.is_cancelled() {
                    return false;
                }
                token.cancel();
                true
            })
        })
        .ok()
}

pub(super) async fn snapshot(app: &App) -> HashMap<SessionKey, CancellationToken> {
    app.workers
        .lock()
        .await
        .iter()
        .filter_map(|(key, worker)| {
            worker
                .cancel
                .lock()
                .expect("cancel mutex poisoned")
                .clone()
                .map(|token| (*key, token))
        })
        .collect()
}

pub(super) async fn handle(app: &App, message: &Message, human: i64) -> Result<bool> {
    let Some(config) = app.shared.config.voice.clone() else {
        return Ok(false);
    };
    let Some(profile) = &app.shared.config.interaction else {
        return Ok(false);
    };
    let key = SessionKey::new(message.chat.id, message.message_thread_id);
    if key != profile.entry() || human != profile.human_id || message.voice.is_none() {
        return Ok(false);
    }
    let update = HUMAN_UPDATE_ID
        .try_with(|id| *id)
        .context("Voice lacks retained ingress")?;
    if message.text.is_some()
        || message.caption.is_some()
        || !message.photo.is_empty()
        || message.document.is_some()
        || message.audio.is_some()
        || message.video.is_some()
        || message.sticker.is_some()
    {
        app.shared.store.finish_update_dispatch(update, false)?;
        routing::notice(app.shared.clone(),origin(message),"This voice adapter accepts one voice clip without mixed attachments or a caption. The original input is retained without dispatch.".into());
        return Ok(true);
    }
    let permit = match app.shared.voice_preparations.clone().try_acquire_owned() {
        Ok(permit) => permit,
        Err(_) => {
            app.shared.store.finish_update_dispatch(update, false)?;
            routing::notice(app.shared.clone(),origin(message),"Voice preparation is full. This original clip is retained without transcription or dispatch; text controls remain available.".into());
            return Ok(true);
        }
    };
    let interruption_targets = snapshot(app).await;
    app.ensure_session(key, human)?;
    app.shared.store.voice_begin(update, key, human)?;
    let app = app.clone();
    let message = message.clone();
    tokio::spawn(async move {
        let _permit = permit;
        let preparation=INTERRUPT_TARGETS.scope(interruption_targets,HUMAN_UPDATE_ID.scope(update,async {
            let source=message.voice.as_ref().context("Missing original voice")?;
            let database=app.shared.config.db_path.clone();
            let reservation=tokio::task::spawn_blocking(move || crate::voice::reserve_telegram_download(&database)).await??;
            let attachment=app.download_attachment_bounded(&source.file_id,Some("voice.ogg"),source.mime_type.as_deref(),AttachmentKind::Voice,&reservation.directory,Some(crate::voice::MAX_BYTES as u64)).await?;
            drop(reservation);
            let(text,proof)=crate::voice::transcribe(&config,&attachment.path,&source.file_id).await?;
            if app.maintenance_writer_held(&message,human,Some(&text)).await? {bail!("Voice source held during preparation");}
            app.shared.store.voice_commit(update,human,&text,&proof)?;
            let cleanup_path=attachment.path.clone();
            let cleanup_hash=proof.audio_sha256.clone();
            tokio::task::spawn_blocking(move || {
                if crate::voice::retire_transcribed_clip(&cleanup_path,&cleanup_hash).is_err() {
                    tracing::warn!("Committed voice download cleanup deferred");
                }
            });
            routing::notice(app.shared.clone(),origin(&message),format!("Automatic voice transcript:\n{}",text.chars().take(1800).collect::<String>()));
            if routing::handle(&app,&message,human,&text).await? {return Ok(());}
            // Voice bypasses the general slash-command dispatcher. Routing/status
            // and scoped controls above retain their existing trusted path; other
            // content reaches the dialogue with explicit machine provenance.
            if !app.ensure_codex_authenticated(message.chat.id,message.message_thread_id).await? {bail!("Native authentication unavailable");}
            let rendered=format!("[Committed voice input]\nOriginal ingress update: {update}\nProvenance: {}\nAn older ingress remains older even if transcription finished after later text. This is an automatic interpretation of the original human clip; clarify uncertain hearing.\n\n{}",serde_json::to_string(&proof)?,text);
            if app.try_steer_active_turn(key,human,&rendered).await? {return Ok(());}
            app.enqueue_turn(TurnRequest{session_key:key,from_user_id:human,prompt:rendered,runtime_instructions:None,attachments:vec![],review_mode:None,override_search_mode:None},&message.chat.kind).await
        }));
        let result = tokio::select! {
            biased;
            _ = app.shared.voice_shutdown.cancelled() => Err(anyhow!("Voice preparation cancelled for listener restart")),
            result = preparation => result,
        };
        if result.is_err() {
            let _ = app.shared.store.voice_fail(update);
            routing::notice(app.shared.clone(),origin(&message),"Voice preparation or admission is unconfirmed. The original input and any committed transcript remain recorded; no automatic retry was made. Text controls remain available.".into());
        } else {
            let _ = app.shared.store.voice_finish_dispatch(update);
        }
    });
    Ok(true)
}
fn origin(message: &Message) -> crate::interaction::ReturnRoute {
    crate::interaction::ReturnRoute {
        chat_id: message.chat.id,
        thread_id: message.message_thread_id.unwrap_or(0),
        message_id: Some(message.message_id),
    }
}
