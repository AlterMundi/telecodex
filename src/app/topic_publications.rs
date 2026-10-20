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
    if !shared
        .config
        .interaction
        .as_ref()
        .is_some_and(|c| c.telegram_projection)
    {
        return Ok(());
    }
    for key in shared.store.topic_publication_targets()? {
        wake(shared.clone(), key);
    }
    Ok(())
}

pub(super) fn wake(shared: Arc<AppShared>, key: SessionKey) {
    let Some(human) = shared.config.interaction.as_ref().map(|c| c.human_id) else {
        return;
    };
    if !enabled(&shared, key, human) {
        return;
    }
    tokio::spawn(async move {
        if drain(&shared, key, human).await.is_err() {
            tracing::warn!("Topic projection retained without confirmed publication");
        }
    });
}

async fn drain(shared: &Arc<AppShared>, key: SessionKey, human: i64) -> Result<()> {
    'queue: while let Some(publication) = shared.store.topic_publication_claim(key, human)? {
        let _lease = Lease {
            shared: shared.clone(),
            id: publication.id,
        };
        let chunks = split_text(&publication.body, shared.config.max_text_chunk);
        let parts = shared
            .store
            .topic_publication_parts(publication.id, &chunks)?;
        for (part, body) in parts {
            let result = shared
                .telegram
                .send_message_checked(key.chat_id, || {
                    if !enabled(shared, key, human)
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
                .await;
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
