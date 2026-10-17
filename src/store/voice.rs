//! Immutable derived voice content alongside, never inside, original ingress JSON.
use super::*;
use crate::voice::TranscriptSource;
use anyhow::bail;

pub(super) fn content_on(
    conn: &Connection,
    update: &crate::telegram::Update,
    human: i64,
) -> Result<(String, Option<TranscriptSource>)> {
    let m = update
        .message
        .as_ref()
        .context("Missing original message")?;
    if m.from.as_ref().is_none_or(|u| u.id != human || u.is_bot) {
        bail!("Original author mismatch");
    }
    if let Some(text) = &m.text {
        return Ok((text.clone(), None));
    }
    let voice = m.voice.as_ref().context("No committed text or voice")?;
    let (text,raw):(String,String)=conn.query_row("SELECT v.text,v.source FROM interaction_voice_inputs v JOIN incoming_updates i ON i.update_id=v.update_id JOIN sessions binding ON binding.id=v.session_id AND binding.creator_user_id=v.human_id AND binding.codex_thread_id IS v.native JOIN users u ON u.tg_user_id=v.human_id JOIN bot_state epoch ON epoch.key='stop_input_epoch:'||i.update_id LEFT JOIN bot_state current ON current.key='stop_admission_epoch:'||v.human_id WHERE v.update_id=?1 AND v.human_id=?2 AND v.phase='committed' AND u.allowed=1 AND epoch.value=COALESCE(current.value,'initial')",params![update.update_id,human],|r|Ok((r.get(0)?,r.get(1)?)))?;
    let source: TranscriptSource = serde_json::from_str(&raw)?;
    if source.telegram_file_id != voice.file_id {
        bail!("Original voice changed");
    }
    Ok((text, Some(source)))
}

fn validate_on(conn: &Connection, update: i64, key: SessionKey, human: i64) -> Result<bool> {
    let exists: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM interaction_voice_inputs WHERE update_id=?)",
        [update],
        |r| r.get(0),
    )?;
    if !exists {
        return Ok(false);
    }
    let valid: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM interaction_voice_inputs v JOIN sessions s ON s.id=v.session_id JOIN users u ON u.tg_user_id=v.human_id JOIN bot_state epoch ON epoch.key='stop_input_epoch:'||v.update_id LEFT JOIN bot_state current ON current.key='stop_admission_epoch:'||v.human_id WHERE v.update_id=?1 AND v.human_id=?2 AND s.creator_user_id=?2 AND s.chat_id=?3 AND s.thread_id=?4 AND u.allowed=1 AND s.codex_thread_id IS v.native AND v.phase='committed' AND epoch.value=COALESCE(current.value,'initial'))", params![update,human,key.chat_id,key.thread_id], |r| r.get(0))?;
    if !valid {
        bail!("Voice entry binding or authority retired; original retained without replay");
    }
    Ok(true)
}

impl Store {
    pub(super) fn init_voice(&self) -> Result<()> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        conn.execute_batch("CREATE TABLE IF NOT EXISTS interaction_voice_inputs(update_id INTEGER PRIMARY KEY REFERENCES incoming_updates(update_id),human_id INTEGER NOT NULL,session_id INTEGER NOT NULL,native TEXT,phase TEXT NOT NULL,text TEXT,source TEXT,instance_id TEXT NOT NULL);")?;
        conn.execute_batch("CREATE TRIGGER IF NOT EXISTS voice_binding_retired AFTER UPDATE OF codex_thread_id,force_fresh_thread,creator_user_id ON sessions BEGIN
            UPDATE interaction_voice_inputs SET phase='retired' WHERE session_id=NEW.id AND phase IN ('preparing','committed') AND (native IS NOT NEW.codex_thread_id OR NEW.force_fresh_thread=1 OR human_id!=NEW.creator_user_id);
            END;
            CREATE TRIGGER IF NOT EXISTS voice_owner_revoked AFTER UPDATE OF allowed ON users WHEN NEW.allowed=0 BEGIN
            UPDATE interaction_voice_inputs SET phase='retired' WHERE human_id=NEW.tg_user_id AND phase IN ('preparing','committed');
            END;")?;
        conn.execute("UPDATE interaction_voice_inputs SET phase='uncertain' WHERE phase='preparing' AND instance_id!=?1",[&self.instance_id])?;
        Ok(())
    }
    pub fn voice_begin(&self, update: i64, key: SessionKey, human: i64) -> Result<()> {
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction()?;
        let eligible:Option<(i64,Option<String>,String)>=tx.query_row("SELECT s.id,s.codex_thread_id,i.payload_json FROM incoming_updates i JOIN sessions s ON s.chat_id=?2 AND s.thread_id=?3 JOIN users u ON u.tg_user_id=s.creator_user_id JOIN bot_state epoch ON epoch.key='stop_input_epoch:'||i.update_id LEFT JOIN bot_state current ON current.key='stop_admission_epoch:'||s.creator_user_id WHERE i.update_id=?1 AND i.status='processing' AND i.instance_id=?5 AND s.creator_user_id=?4 AND u.allowed=1 AND json_extract(i.payload_json,'$.message.from.id')=?4 AND json_extract(i.payload_json,'$.message.chat.id')=?2 AND json_extract(i.payload_json,'$.message.message_thread_id')=?3 AND epoch.value=COALESCE(current.value,'initial')",params![update,key.chat_id,key.thread_id,human,self.instance_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
        let (session, native, raw) = eligible.context("Voice ingress not eligible")?;
        let original: crate::telegram::Update = serde_json::from_str(&raw)?;
        let m = original.message.context("Missing original voice")?;
        if m.voice.is_none()
            || m.text.is_some()
            || m.caption.is_some()
            || !m.photo.is_empty()
            || m.audio.is_some()
            || m.document.is_some()
            || m.video.is_some()
            || m.sticker.is_some()
        {
            bail!("Voice adapter requires one original voice clip");
        }
        tx.execute("INSERT INTO interaction_voice_inputs(update_id,human_id,session_id,native,phase,instance_id) VALUES(?1,?2,?3,?4,'preparing',?5)",params![update,human,session,native,self.instance_id])?;
        tx.execute(
            "UPDATE incoming_updates SET status='queued',updated_at=?2 WHERE update_id=?1",
            params![update, now_string()],
        )?;
        tx.commit()?;
        Ok(())
    }
    pub fn voice_commit(
        &self,
        update: i64,
        human: i64,
        text: &str,
        source: &TranscriptSource,
    ) -> Result<()> {
        if text.trim().is_empty()
            || text.len() > 16000
            || source.audio_sha256.len() != 64
            || !source.audio_sha256.bytes().all(|c| c.is_ascii_hexdigit())
            || source.configured_transcriber.is_empty()
        {
            bail!("Invalid committed transcript");
        }
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction()?;
        let raw:String=tx.query_row("SELECT i.payload_json FROM interaction_voice_inputs v JOIN incoming_updates i ON i.update_id=v.update_id JOIN sessions s ON s.id=v.session_id JOIN users u ON u.tg_user_id=s.creator_user_id JOIN bot_state epoch ON epoch.key='stop_input_epoch:'||i.update_id LEFT JOIN bot_state current ON current.key='stop_admission_epoch:'||v.human_id WHERE v.update_id=?1 AND v.human_id=?2 AND v.phase='preparing' AND v.instance_id=?3 AND i.instance_id=?3 AND i.status='queued' AND s.creator_user_id=?2 AND u.allowed=1 AND s.codex_thread_id IS v.native AND epoch.value=COALESCE(current.value,'initial')",params![update,human,self.instance_id],|r|r.get(0))?;
        let original: crate::telegram::Update = serde_json::from_str(&raw)?;
        if original
            .message
            .and_then(|m| m.voice)
            .is_none_or(|v| v.file_id != source.telegram_file_id)
        {
            bail!("Transcription media identity changed");
        }
        tx.execute("UPDATE interaction_voice_inputs SET phase='committed',text=?2,source=?3 WHERE update_id=?1",params![update,text,serde_json::to_string(source)?])?;
        tx.execute(
            "UPDATE incoming_updates SET status='processing',updated_at=?2 WHERE update_id=?1",
            params![update, now_string()],
        )?;
        tx.commit()?;
        Ok(())
    }
    /// Recheck the original entry generation at queue and final native-write seams.
    /// Non-voice updates use their existing admission checks unchanged.
    pub fn voice_validate(&self, update: Option<i64>, key: SessionKey, human: i64) -> Result<()> {
        let Some(update) = update else {
            return Ok(());
        };
        let conn = self.conn.lock().expect("store mutex poisoned");
        validate_on(&conn, update, key, human)?;
        Ok(())
    }

    /// A first voice may create its own native thread. Certify that one binding
    /// atomically rather than permitting a replacement observed after an await.
    pub fn voice_bind_native(
        &self,
        update: Option<i64>,
        key: SessionKey,
        human: i64,
        native: &str,
    ) -> Result<bool> {
        let Some(update) = update else {
            return Ok(false);
        };
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction()?;
        if !validate_on(&tx, update, key, human)? {
            return Ok(false);
        }
        let prior: Option<String> = tx.query_row(
            "SELECT native FROM interaction_voice_inputs WHERE update_id=?",
            [update],
            |r| r.get(0),
        )?;
        if prior.as_deref().is_some_and(|old| old != native) {
            bail!("Voice native generation changed");
        }
        if prior.is_none() {
            // Initial establishment continues this entry generation. Carry its
            // pending clips before the binding trigger runs; a human clear or
            // rebind already retired those rows and cannot be revived here.
            tx.execute("UPDATE interaction_voice_inputs SET native=?2 WHERE session_id=(SELECT session_id FROM interaction_voice_inputs WHERE update_id=?1) AND human_id=?3 AND instance_id=?4 AND native IS NULL AND phase IN ('preparing','committed')", params![update,native,human,self.instance_id])?;
        }
        tx.execute("UPDATE sessions SET codex_thread_id=?3,force_fresh_thread=0,updated_at=?4 WHERE chat_id=?1 AND thread_id=?2", params![key.chat_id,key.thread_id,native,now_string()])?;
        tx.commit()?;
        Ok(true)
    }
    pub fn voice_finish_dispatch(&self, update: i64) -> Result<()> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        conn.execute("UPDATE incoming_updates SET status='handled',updated_at=?2 WHERE update_id=?1 AND instance_id=?3 AND status='processing'",params![update,now_string(),self.instance_id])?;
        Ok(())
    }
    pub fn voice_fail(&self, update: i64) -> Result<()> {
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction()?;
        tx.execute("UPDATE interaction_voice_inputs SET phase='uncertain' WHERE update_id=?1 AND instance_id=?2 AND phase='preparing'",params![update,self.instance_id])?;
        tx.execute("UPDATE incoming_updates SET status='undetermined',updated_at=?2 WHERE update_id=?1 AND instance_id=?3 AND status IN ('queued','processing')",params![update,now_string(),self.instance_id])?;
        tx.commit()?;
        Ok(())
    }
    #[cfg(test)]
    pub fn interaction_input_content(
        &self,
        update: &crate::telegram::Update,
        human: i64,
    ) -> Result<(String, Option<TranscriptSource>)> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        content_on(&conn, update, human)
    }
    pub fn voice_content_for_message(
        &self,
        update: i64,
        message: &crate::telegram::Message,
        human: i64,
    ) -> Result<Option<(String, TranscriptSource)>> {
        if message.voice.is_none() {
            return Ok(None);
        }
        let conn = self.conn.lock().expect("store mutex poisoned");
        let raw: String = conn.query_row(
            "SELECT payload_json FROM incoming_updates WHERE update_id=?1",
            [update],
            |r| r.get(0),
        )?;
        let original: crate::telegram::Update = serde_json::from_str(&raw)?;
        if serde_json::to_value(original.message.as_ref())? != serde_json::to_value(message)? {
            bail!("Voice source does not match retained original");
        }
        let (text, source) = content_on(&conn, &original, human)?;
        Ok(Some((
            text,
            source.context("Committed voice provenance unavailable")?,
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interaction::{RequestRelation, ReturnRoute, SourceInput};
    use serde_json::json;
    fn setup() -> (
        tempfile::NamedTempFile,
        Store,
        SessionKey,
        crate::telegram::Update,
    ) {
        let file = tempfile::NamedTempFile::new().unwrap();
        let defaults = super::super::tests::defaults();
        let store = Store::open(file.path(), &[100], &defaults).unwrap();
        let key = SessionKey::new(100, Some(1));
        store.ensure_session(key, 100, &defaults).unwrap();
        store.set_session_codex_thread(key, "entry-native").unwrap();
        let original=serde_json::from_value(json!({"update_id":20,"message":{"message_id":20,"message_thread_id":1,"from":{"id":100,"is_bot":false,"first_name":"Human"},"chat":{"id":100,"type":"private"},"voice":{"file_id":"synthetic-voice-file"}}})).unwrap();
        store.admit_update(&original).unwrap();
        store.begin_update(20).unwrap();
        (file, store, key, original)
    }
    fn proof() -> TranscriptSource {
        TranscriptSource {
            audio_sha256: "0".repeat(64),
            telegram_file_id: "synthetic-voice-file".into(),
            configured_transcriber: "fixture/no-inference".into(),
            committed_at: now_string(),
            audio_millis: 1000,
            preparation_millis: 50,
            transcription_millis: 20,
            interpretation: "machine transcription, not exact written speech".into(),
        }
    }
    fn status(store: &Store) -> String {
        store
            .conn
            .lock()
            .unwrap()
            .query_row(
                "SELECT status FROM incoming_updates WHERE update_id=20",
                [],
                |r| r.get(0),
            )
            .unwrap()
    }
    #[test]
    fn original_preserved_and_fast_preparation_cannot_be_settled_by_outer_dispatch() {
        let (_file, store, key, original) = setup();
        store.voice_begin(20, key, 100).unwrap();
        assert!(store.interaction_input_content(&original, 100).is_err());
        store.finish_update_dispatch(20, true).unwrap();
        assert_eq!(status(&store), "queued");
        let proof = proof();
        store
            .voice_commit(20, 100, "Una tarea de voz", &proof)
            .unwrap();
        store.finish_update_dispatch(20, true).unwrap();
        assert_eq!(status(&store), "processing");
        assert_eq!(
            store.interaction_input_content(&original, 100).unwrap(),
            ("Una tarea de voz".into(), Some(proof))
        );
        let raw: String = store
            .conn
            .lock()
            .unwrap()
            .query_row(
                "SELECT payload_json FROM incoming_updates WHERE update_id=20",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let retained: crate::telegram::Update = serde_json::from_str(&raw).unwrap();
        assert!(retained.message.as_ref().unwrap().text.is_none());
        assert_eq!(
            serde_json::to_value(retained).unwrap(),
            serde_json::to_value(original).unwrap()
        );
        assert!(
            store
                .voice_commit(20, 100, "replacement", &super::tests::proof())
                .is_err()
        );
        store.voice_finish_dispatch(20).unwrap();
        assert_eq!(status(&store), "handled");
    }
    #[test]
    fn changed_native_owner_or_stop_epoch_prevents_late_transcript_admission() {
        for change in ["native", "epoch", "owner"] {
            let (_file, store, key, _) = setup();
            store.voice_begin(20, key, 100).unwrap();
            match change {
                "native" => store
                    .set_session_codex_thread(key, "changed-native")
                    .unwrap(),
                "epoch" => store
                    .save_bot_state("stop_admission_epoch:100", "different-epoch")
                    .unwrap(),
                _ => {
                    store
                        .conn
                        .lock()
                        .unwrap()
                        .execute("UPDATE sessions SET creator_user_id=101", [])
                        .unwrap();
                }
            }
            assert!(
                store
                    .voice_commit(
                        20,
                        100,
                        "Never dispatch this stale voice",
                        &super::tests::proof()
                    )
                    .is_err()
            );
            store.voice_fail(20).unwrap();
            assert_eq!(status(&store), "undetermined");
        }
    }
    #[test]
    fn interrupted_preparation_is_retained_without_restart_retry() {
        let (file, store, key, original) = setup();
        store.voice_begin(20, key, 100).unwrap();
        drop(store);
        let store = Store::open(file.path(), &[100], &super::super::tests::defaults()).unwrap();
        assert_eq!(status(&store), "undetermined");
        assert!(
            store
                .voice_commit(20, 100, "late result", &super::tests::proof())
                .is_err()
        );
        assert!(store.interaction_input_content(&original, 100).is_err());
        let phase: String = store
            .conn
            .lock()
            .unwrap()
            .query_row("SELECT phase FROM interaction_voice_inputs", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(phase, "uncertain");
    }
    #[test]
    fn deleted_entry_preserves_original_and_transcription_without_rebinding() {
        let (_file, store, key, original) = setup();
        store.voice_begin(20, key, 100).unwrap();
        store
            .voice_commit(20, 100, "Historical voice", &proof())
            .unwrap();
        let previous = store.get_session(key).unwrap().unwrap().id;
        store.delete_session(key).unwrap();
        let replacement = store
            .ensure_session(key, 100, &super::super::tests::defaults())
            .unwrap();
        assert_ne!(previous, replacement.id);
        assert!(store.interaction_input_content(&original, 100).is_err());
        let retained: i64 = store
            .conn
            .lock()
            .unwrap()
            .query_row(
                "SELECT session_id FROM interaction_voice_inputs WHERE update_id=20",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(retained, previous);
    }
    #[test]
    fn post_commit_generation_and_owner_changes_reject_queue_and_native_write() {
        for change in ["clear", "rebind", "owner", "revoke", "epoch"] {
            let (_file, store, key, _) = setup();
            store.voice_begin(20, key, 100).unwrap();
            store
                .voice_commit(20, 100, "Original generation only", &proof())
                .unwrap();
            store.voice_validate(Some(20), key, 100).unwrap();
            match change {
                "clear" => store.clear_session_conversation(key).unwrap(),
                "rebind" => {
                    store.set_session_codex_thread(key, "replacement").unwrap();
                    store.set_session_codex_thread(key, "entry-native").unwrap();
                }
                "owner" => {
                    let conn = store.conn.lock().unwrap();
                    conn.execute("UPDATE sessions SET creator_user_id=101", [])
                        .unwrap();
                    conn.execute("UPDATE sessions SET creator_user_id=100", [])
                        .unwrap();
                }
                "revoke" => {
                    let conn = store.conn.lock().unwrap();
                    conn.execute("UPDATE users SET allowed=0", []).unwrap();
                    conn.execute("UPDATE users SET allowed=1", []).unwrap();
                }
                _ => store
                    .save_bot_state("stop_admission_epoch:100", "retired")
                    .unwrap(),
            }
            assert!(
                store.voice_validate(Some(20), key, 100).is_err(),
                "{change}"
            );
            assert!(
                store
                    .voice_bind_native(Some(20), key, 100, "replacement")
                    .is_err(),
                "{change}"
            );
        }
    }
    #[test]
    fn first_voice_binds_only_its_own_new_native_and_clear_always_retires_it() {
        for clear in [false, true] {
            let (_file, store, key, _) = setup();
            store.clear_session_conversation(key).unwrap();
            store.voice_begin(20, key, 100).unwrap();
            store
                .voice_commit(20, 100, "First native voice", &proof())
                .unwrap();
            if clear {
                store.clear_session_conversation(key).unwrap();
                assert!(
                    store
                        .voice_bind_native(Some(20), key, 100, "new-native")
                        .is_err()
                );
                assert!(
                    store
                        .get_session(key)
                        .unwrap()
                        .unwrap()
                        .codex_thread_id
                        .is_none()
                );
            } else {
                assert!(
                    store
                        .voice_bind_native(Some(20), key, 100, "new-native")
                        .unwrap()
                );
                store.voice_validate(Some(20), key, 100).unwrap();
                assert!(
                    store
                        .voice_bind_native(Some(20), key, 100, "other-native")
                        .is_err()
                );
                assert_eq!(
                    store
                        .get_session(key)
                        .unwrap()
                        .unwrap()
                        .codex_thread_id
                        .as_deref(),
                    Some("new-native")
                );
            }
        }
    }
    #[test]
    fn own_initial_native_carries_other_pending_clips_but_not_retired_inputs() {
        for cleared in [false, true] {
            let (_file, store, key, mut original) = setup();
            store.clear_session_conversation(key).unwrap();
            store.voice_begin(20, key, 100).unwrap();
            original.update_id = 21;
            original.message.as_mut().unwrap().message_id = 21;
            store.admit_update(&original).unwrap();
            store.begin_update(21).unwrap();
            store.voice_begin(21, key, 100).unwrap();
            store
                .voice_commit(20, 100, "First voice", &proof())
                .unwrap();
            if cleared {
                store.clear_session_conversation(key).unwrap();
            }
            if cleared {
                assert!(
                    store
                        .voice_bind_native(Some(20), key, 100, "own-native")
                        .is_err()
                );
                assert!(
                    store
                        .voice_commit(21, 100, "Later correction", &proof())
                        .is_err()
                );
            } else {
                store
                    .voice_bind_native(Some(20), key, 100, "own-native")
                    .unwrap();
                store
                    .voice_commit(21, 100, "Later correction", &proof())
                    .unwrap();
                store.voice_validate(Some(21), key, 100).unwrap();
                assert_eq!(
                    store.interaction_input_content(&original, 100).unwrap().0,
                    "Later correction"
                );
            }
        }
    }
    #[test]
    fn native_entry_plan_requires_the_retained_transcript_and_its_provenance() {
        let (_file, store, key, original) = setup();
        store.voice_begin(20, key, 100).unwrap();
        let proof = proof();
        store
            .voice_commit(20, 100, "Investiguemos el gesto", &proof)
            .unwrap();
        store.mark_update_queued(20).unwrap();
        let session = store.get_session(key).unwrap().unwrap();
        let turn = store
            .record_turn_started(
                session.id,
                &TurnRequest {
                    session_key: key,
                    from_user_id: 100,
                    prompt: "attributed voice entry".into(),
                    runtime_instructions: None,
                    attachments: vec![],
                    review_mode: None,
                    override_search_mode: None,
                },
            )
            .unwrap();
        store
            .record_turn_native_thread(turn, "entry-native")
            .unwrap();
        store.link_update_turn(20, turn).unwrap();
        assert_eq!(
            store
                .interaction_entry_inputs(key, 100, "entry-native", None)
                .unwrap()
                .len(),
            1
        );
        let target = SessionKey::new(100, Some(7));
        let worker = store
            .ensure_session(target, 100, &super::super::tests::defaults())
            .unwrap();
        store
            .set_session_codex_thread(target, "worker-native")
            .unwrap();
        let mut input = SourceInput {
            transcription: Some(proof.clone()),
            channel: "telegram:fixture".into(),
            event_id: "20".into(),
            update_id: Some(20),
            human_id: 100,
            target_sessions: vec![worker.id],
            origin: ReturnRoute {
                chat_id: 100,
                thread_id: 1,
                message_id: Some(20),
            },
            text: "Investiguemos el gesto".into(),
        };
        let closed = tokio_util::sync::CancellationToken::new();
        let plan = [(target, RequestRelation::New)];
        input.transcription = None;
        assert!(
            store
                .interaction_admit_entry_plan(key, 100, "entry-native", &input, &plan, &closed)
                .is_err()
        );
        input.transcription = Some(proof);
        input.text = "model-authored replacement".into();
        assert!(
            store
                .interaction_admit_entry_plan(key, 100, "entry-native", &input, &plan, &closed)
                .is_err()
        );
        input.text = "Investiguemos el gesto".into();
        assert_eq!(
            store
                .interaction_admit_entry_plan(key, 100, "entry-native", &input, &plan, &closed)
                .unwrap()
                .len(),
            1
        );
        assert!(original.message.unwrap().text.is_none());
    }
}
