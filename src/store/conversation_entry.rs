//! Logical conversation identity also exists before its first native context.
use super::*;
use anyhow::bail;

impl Store {
    pub(super) fn init_conversation_entry(&self) -> Result<()> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        conn.execute_batch("CREATE TABLE IF NOT EXISTS conversation_entry_generations(
            session_id INTEGER PRIMARY KEY REFERENCES sessions(id) ON DELETE CASCADE,
            generation TEXT NOT NULL);
            INSERT OR IGNORE INTO conversation_entry_generations(session_id,generation)
            SELECT id,lower(hex(randomblob(16))) FROM sessions;
            CREATE TRIGGER IF NOT EXISTS conversation_entry_created AFTER INSERT ON sessions BEGIN
            INSERT INTO conversation_entry_generations(session_id,generation) VALUES(NEW.id,lower(hex(randomblob(16)))); END;
            CREATE TRIGGER IF NOT EXISTS conversation_entry_replaced AFTER UPDATE OF codex_thread_id,force_fresh_thread,creator_user_id ON sessions
            WHEN OLD.creator_user_id!=NEW.creator_user_id OR NEW.force_fresh_thread=1 OR
                (OLD.codex_thread_id IS NOT NULL AND OLD.codex_thread_id IS NOT NEW.codex_thread_id) BEGIN
            UPDATE conversation_entry_generations SET generation=lower(hex(randomblob(16))) WHERE session_id=NEW.id; END;
            CREATE TRIGGER IF NOT EXISTS conversation_entry_grant_revoked AFTER UPDATE OF allowed ON users WHEN NEW.allowed=0 BEGIN
            UPDATE conversation_entry_generations SET generation=lower(hex(randomblob(16))) WHERE session_id IN
                (SELECT id FROM sessions WHERE creator_user_id=NEW.tg_user_id); END;")?;
        Ok(())
    }

    pub fn conversation_initial_binding(
        &self,
        human: i64,
        entry: SessionKey,
    ) -> Result<Option<String>> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let (session, native) = super::presentation::entry_on(&conn, human, entry)?;
        if !native.is_empty() {
            return Ok(None);
        }
        Ok(Some(super::presentation::entry_binding(
            &conn, human, entry, session,
        )?))
    }

    /// Only the existing writer's first native-context event establishes this
    /// binding. No prompt is invented and no native operation occurs here.
    pub fn conversation_bind_initial(
        &self,
        human: i64,
        entry: SessionKey,
        local_turn: i64,
        expected: &str,
        native: &str,
    ) -> Result<bool> {
        if native.is_empty() {
            bail!("Native context identity is missing");
        }
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction()?;
        let (session, current) = super::presentation::entry_on(&tx, human, entry)?;
        if super::presentation::entry_binding(&tx, human, entry, session)? != expected
            || super::stopping::fenced_on(&tx, entry)?
            || super::maintenance_writer_held_on(&tx, entry)?
        {
            bail!("Initial conversation generation retired");
        }
        let owned:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM turns WHERE id=?1 AND session_id=?2 AND from_user_id=?3 AND status='running' AND (native_thread_id IS NULL OR native_thread_id=?4))",params![local_turn,session,human,native],|r|r.get(0))?;
        if !owned {
            bail!("Initial native context does not belong to this writer");
        }
        if !current.is_empty() {
            if current != native {
                bail!("Another native context already owns this entry");
            }
            return Ok(false);
        }
        let epoch = super::stopping::admission_epoch_on(&tx, entry)?;
        // Carry derived context references before the native-binding retirement
        // triggers run. Phase and process ownership stay unchanged: old listeners
        // cannot replay their work, while retained history stays readable.
        tx.execute("UPDATE conversation_inputs SET native_thread=?1,doc=json_set(doc,'$.native',?1)
            WHERE session_id=?2 AND human_id=?3 AND native_thread='' AND retired=0 AND json_extract(doc,'$.epoch')=?4",
            params![native,session,human,epoch])?;
        tx.execute("UPDATE interaction_voice_inputs SET native=?1 WHERE session_id=?2 AND human_id=?3 AND native IS NULL AND instance_id=?4 AND phase IN ('preparing','committed') AND update_id IN
            (SELECT update_id FROM incoming_updates i JOIN bot_state e ON e.key='stop_input_epoch:'||i.update_id WHERE e.value=?5)",
            params![native,session,human,self.instance_id,epoch])?;
        tx.execute(
            "UPDATE sessions SET codex_thread_id=?1,force_fresh_thread=0,updated_at=?2 WHERE id=?3",
            params![native, now_string(), session],
        )?;
        tx.execute("INSERT OR IGNORE INTO conversation_returns(human_id,native_thread,source_input,selected_at)
            SELECT human_id,?1,id,?2 FROM conversation_inputs WHERE session_id=?3 AND human_id=?4 AND native_thread=?1 AND retired=0
            AND phase IN ('queued','sending','steering','routed','accepted','uncertain') ORDER BY created_at LIMIT 1",
            params![native,now_string(),session,human])?;
        tx.commit()?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{ConversationInputEvent as E, ConversationVoice};
    fn setup() -> (tempfile::NamedTempFile, Store, SessionKey, String) {
        let file = tempfile::NamedTempFile::new().unwrap();
        let store = Store::open(file.path(), &[100], &super::super::tests::defaults()).unwrap();
        let key = SessionKey::new(100, Some(1));
        store
            .ensure_session(key, 100, &super::super::tests::defaults())
            .unwrap();
        store.clear_session_conversation(key).unwrap();
        let binding = store.conversation_request_binding(100, key).unwrap()["binding"]
            .as_str()
            .unwrap()
            .to_owned();
        (file, store, key, binding)
    }
    fn turn(store: &Store, key: SessionKey) -> i64 {
        let session = store.get_session(key).unwrap().unwrap();
        store
            .record_turn_started(
                session.id,
                &crate::models::TurnRequest {
                    session_key: key,
                    from_user_id: 100,
                    prompt: "The actual original human text".into(),
                    runtime_instructions: None,
                    attachments: vec![],
                    review_mode: None,
                    override_search_mode: None,
                },
            )
            .unwrap()
    }
    #[test]
    fn cold_read_and_first_real_turn_preserve_binding_pending_voice_and_original_identity() {
        let (_file, store, key, binding) = setup();
        assert!(
            store.interaction_presentation_list(100, key, 0, 0).unwrap()["items"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert!(
            store
                .get_session(key)
                .unwrap()
                .unwrap()
                .codex_thread_id
                .is_none()
        );
        let id = Uuid::now_v7().to_string();
        store
            .conversation_input_capture(100, key, &binding, &id, "The actual original human text")
            .unwrap();
        let clip = Uuid::now_v7().to_string();
        store
            .conversation_voice_capture(
                100,
                key,
                &binding,
                &clip,
                ConversationVoice {
                    audio_sha256: "a".repeat(64),
                    mime: "audio/wav".into(),
                    transcription: None,
                },
                true,
            )
            .unwrap();
        store.conversation_input_event(100, &id, E::Queue).unwrap();
        assert_eq!(
            store
                .conn
                .lock()
                .unwrap()
                .query_row("SELECT count(*) FROM conversation_returns", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
        let local = turn(&store, key);
        assert!(
            store
                .conversation_bind_initial(100, key, local, &binding, "first-native")
                .unwrap()
        );
        store
            .record_turn_native_thread(local, "first-native")
            .unwrap();
        let observed = store.conversation_input_read(100, key, &id).unwrap();
        assert_eq!(observed.native, "first-native");
        assert!(!observed.retired);
        assert_eq!(observed.text, "The actual original human text");
        let pending = store.conversation_input_read(100, key, &clip).unwrap();
        assert_eq!(pending.phase, "preparing");
        assert_eq!(pending.native, "first-native");
        store.conversation_voice_validate(100, &clip).unwrap();
        assert_eq!(
            store.conversation_request_binding(100, key).unwrap()["binding"],
            binding
        );
        assert!(
            !store
                .conversation_input_capture(100, key, &binding, &id, &observed.text)
                .unwrap()
                .1
        );
        store
            .conversation_input_event(
                100,
                &id,
                E::Begin {
                    local_turn: local,
                    steer: false,
                },
            )
            .unwrap();
        store
            .conversation_input_event(100, &id, E::Accepted { local_turn: local })
            .unwrap();
        assert!(store.conversation_turn_return(local).unwrap());
        assert!(
            !store
                .conversation_bind_initial(100, key, local, &binding, "first-native")
                .unwrap()
        );
    }
    #[test]
    fn empty_entry_clear_and_grant_aba_retire_the_captured_initial_generation() {
        for change in ["clear", "grant"] {
            let (_file, store, key, binding) = setup();
            let id = Uuid::now_v7().to_string();
            store
                .conversation_input_capture(100, key, &binding, &id, "An earlier original")
                .unwrap();
            let local = turn(&store, key);
            if change == "clear" {
                store.clear_session_conversation(key).unwrap();
            } else {
                store.conn.lock().unwrap().execute_batch("UPDATE users SET allowed=0 WHERE tg_user_id=100;UPDATE users SET allowed=1 WHERE tg_user_id=100;").unwrap();
            }
            assert!(
                store
                    .conversation_input_read(100, key, &id)
                    .unwrap()
                    .retired
            );
            assert_ne!(
                store.conversation_request_binding(100, key).unwrap()["binding"],
                binding
            );
            assert!(
                store
                    .conversation_bind_initial(100, key, local, &binding, "late-native")
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
        }
    }
    #[test]
    fn restart_preserves_logical_binding_and_unsent_history_without_replay() {
        let (file, store, key, binding) = setup();
        let id = Uuid::now_v7().to_string();
        store
            .conversation_input_capture(100, key, &binding, &id, "Retained before restart")
            .unwrap();
        drop(store);
        let store = Store::open(file.path(), &[100], &super::super::tests::defaults()).unwrap();
        assert_eq!(
            store.conversation_request_binding(100, key).unwrap()["binding"],
            binding
        );
        let local = turn(&store, key);
        store
            .conversation_bind_initial(100, key, local, &binding, "fresh-native")
            .unwrap();
        let retained = store.conversation_input_read(100, key, &id).unwrap();
        assert_eq!(retained.phase, "retained_unsent");
        assert_eq!(retained.native, "fresh-native");
        assert!(!retained.retired);
        assert!(store.conversation_input_event(100, &id, E::Queue).is_err());
    }
}
