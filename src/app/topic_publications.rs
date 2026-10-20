//! Nonblocking secondary delivery using canonical native/input identities.
use super::*;

struct Lease {
    shared: Arc<AppShared>,
    id: i64,
}
impl Drop for Lease {
    fn drop(&mut self) {
        let _ = self.shared.store.topic_publication_release(self.id);
    }
}

pub(super) fn enabled(shared: &AppShared, key: SessionKey, human: i64) -> bool {
    shared.config.interaction.as_ref().is_some_and(|c| {
        c.telegram_projection && c.human_id == human && super::routing::enrolled(shared, key)
    })
}

pub(super) fn recover(shared: Arc<AppShared>) -> Result<()> {
    for key in shared.store.topic_publication_targets()? {
        wake(shared.clone(), key);
    }
    Ok(())
}

pub(super) fn wake(shared: Arc<AppShared>, key: SessionKey) {
    let Ok(Some(human)) = shared.store.topic_publication_owner(key) else {
        return;
    };
    for files in [false, true] {
        let shared = shared.clone();
        tokio::spawn(async move {
            if drain(&shared, key, human, files).await.is_err() {
                tracing::warn!("Topic projection retained without confirmed publication");
            }
        });
    }
}

async fn drain(shared: &Arc<AppShared>, key: SessionKey, human: i64, files: bool) -> Result<()> {
    'queue: while let Some(publication) = if files {
        shared.store.topic_publication_claim_files(key, human)?
    } else {
        shared.store.topic_publication_claim(key, human)?
    } {
        let _lease = Lease {
            shared: shared.clone(),
            id: publication.id,
        };
        let chunks = if publication.kind == "artifact" {
            vec![publication.body.clone()]
        } else {
            split_text(&publication.body, shared.config.max_text_chunk)
        };
        let parts = shared
            .store
            .topic_publication_parts(publication.id, &chunks)?;
        for (part, body) in parts {
            let result = if publication.kind == "artifact" {
                send_file(shared, key, human, &publication, part).await
            } else {
                shared
                    .telegram
                    .send_message_checked(key.chat_id, || {
                        if (!publication.direct && !enabled(shared, key, human)
                            || publication.direct
                                && shared
                                    .store
                                    .conversation_turn_return(publication.local_turn)?)
                            || !shared
                                .store
                                .topic_publication_reserve(publication.id, part)?
                        {
                            return Ok(None);
                        }
                        Ok(Some((
                            SendMessage::html(
                                key.chat_id,
                                Some(key.thread_id).filter(|id| *id > 0),
                                render_markdown_to_html(&body),
                            ),
                            (),
                        )))
                    })
                    .await
            };
            match result {
                Ok(Some((message, ())))
                    if message.chat.id == publication.target.chat_id
                        && message.message_thread_id.unwrap_or(0)
                            == publication.target.thread_id =>
                {
                    shared.store.topic_publication_receipt(
                        publication.id,
                        part,
                        Some(message.message_id),
                        false,
                    )?;
                }
                Ok(None) => break,
                failed => {
                    if failed
                        .as_ref()
                        .err()
                        .and_then(|e| e.downcast_ref::<crate::telegram::TelegramError>())
                        .is_some_and(|e| e.retry_after.is_some())
                    {
                        shared.store.topic_publication_retry(publication.id, part)?;
                        continue 'queue; // Existing Telegram cooldown handles this explicit rejection.
                    }
                    let rejected = failed
                        .as_ref()
                        .err()
                        .and_then(|e| e.downcast_ref::<crate::telegram::TelegramError>())
                        .is_some_and(|e| e.status.is_client_error());
                    shared
                        .store
                        .topic_publication_receipt(publication.id, part, None, rejected)?;
                    break;
                }
            }
        }
        shared.store.topic_publication_finish(publication.id)?;
    }
    Ok(())
}

// Limit frozen upload buffers independently of native workers. A slow secondary
// network request must never reserve a native execution slot.
static FILE_UPLOADS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);

async fn send_file(
    shared: &Arc<AppShared>,
    key: SessionKey,
    human: i64,
    publication: &crate::store::topic_publications::TopicPublication,
    part: i64,
) -> Result<Option<(crate::telegram::Message, ())>> {
    let _permit = FILE_UPLOADS.acquire().await?;
    let Some(cfg) = shared
        .config
        .interaction
        .as_ref()
        .and_then(|c| c.artifact_store.clone())
    else {
        return Ok(None);
    };
    let file = match shared.store.artifact_get(human, key, &publication.source) {
        Ok(file) => file,
        Err(_) => return Ok(None),
    };
    let bytes = match tokio::task::spawn_blocking(move || {
        super::artifact_outputs::verified_bytes(&cfg, &file)
    })
    .await?
    {
        Ok(bytes) => bytes,
        Err(_) => {
            tracing::warn!("Retained file unavailable before publication; no upload attempted");
            return Ok(None);
        }
    };
    let path = Path::new(&publication.body);
    let extension = path
        .extension()
        .and_then(|v| v.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let field = match extension.as_str() {
        "png" | "jpg" | "jpeg" | "webp" => "photo",
        "mp3" | "wav" | "m4a" | "ogg" => "audio",
        "mp4" | "mov" | "webm" => "video",
        _ => "document",
    };
    let mime = super::turns::mime_type_for_path(path);
    let caption = shared.store.topic_file_caption(publication.id)?;
    shared
        .telegram
        .send_file_bytes_checked(
            key.chat_id,
            Some(key.thread_id).filter(|t| *t > 0),
            field,
            bytes,
            &publication.body,
            mime.as_deref(),
            Some(&caption),
            || {
                // Immutable file audiences and the explicit source projection
                // are independent of a later text return-surface selection.
                if !enabled(shared, key, human)
                    || shared
                        .store
                        .artifact_get(human, key, &publication.source)
                        .is_err()
                {
                    return Ok(false);
                }
                shared.store.topic_publication_reserve(publication.id, part)
            },
        )
        .await
        .map(|result| result.map(|message| (message, ())))
}
