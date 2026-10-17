//! Original authenticated conversation inputs, independent of Telegram updates.
//! Capturing text is not native acceptance; uncertain transport is never replayed.
use super::*;
use anyhow::bail;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationInput {
    pub id: String,
    pub human: i64,
    pub entry: SessionKey,
    pub session: i64,
    pub native: String,
    pub epoch: String,
    pub text: String,
    pub phase: String,
    pub local_turn: Option<i64>,
    pub retired: bool,
}

#[derive(Debug, Clone, Copy)]
pub enum ConversationInputEvent {
    Queue,
    Begin { local_turn: i64, steer: bool },
    Accepted { local_turn: i64 },
    Rejected { local_turn: i64 },
    Uncertain { local_turn: i64 },
}

pub(super) fn read_on(conn: &Connection, id: &str, human: i64) -> Result<ConversationInput> {
    let (doc, phase, local_turn, retired): (String, String, Option<i64>, bool) = conn
        .query_row(
            "SELECT doc,phase,local_turn,retired FROM conversation_inputs WHERE id=?1 AND human_id=?2",
            params![id, human],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .context("Original conversation input unavailable")?;
    let mut input: ConversationInput = serde_json::from_str(&doc)?;
    input.phase = phase;
    input.local_turn = local_turn;
    input.retired = retired;
    Ok(input)
}

pub(super) fn eligible_on(conn: &Connection, input: &ConversationInput) -> Result<()> {
    let (session, native) = super::presentation::entry_on(conn, input.human, input.entry)?;
    if input.retired
        || session != input.session
        || native != input.native
        || super::stopping::admission_epoch_on(conn, input.entry)? != input.epoch
        || super::stopping::fenced_on(conn, input.entry)?
        || super::maintenance_writer_held_on(conn, input.entry)?
    {
        bail!("Original conversation input binding or admission generation retired");
    }
    Ok(())
}

impl Store {
    pub(super) fn init_conversation_inputs(&self) -> Result<()> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        conn.execute_batch("CREATE TABLE IF NOT EXISTS conversation_inputs(
            id TEXT PRIMARY KEY,human_id INTEGER NOT NULL,session_id INTEGER NOT NULL,
            native_thread TEXT NOT NULL,phase TEXT NOT NULL,local_turn INTEGER,retired INTEGER NOT NULL DEFAULT 0,
            doc TEXT NOT NULL,instance_id TEXT NOT NULL,created_at TEXT NOT NULL);
            CREATE INDEX IF NOT EXISTS conversation_inputs_entry ON conversation_inputs(session_id,human_id,native_thread);")?;
        // These are retained source identities, not live foreign keys. Retire
        // monotonically even when a binding or grant is subsequently restored.
        conn.execute_batch("CREATE TRIGGER IF NOT EXISTS conversation_input_binding_retired AFTER UPDATE OF codex_thread_id,force_fresh_thread,creator_user_id ON sessions BEGIN
            UPDATE conversation_inputs SET retired=1 WHERE session_id=NEW.id AND (native_thread IS NOT NEW.codex_thread_id OR NEW.force_fresh_thread=1 OR human_id!=NEW.creator_user_id); END;
            CREATE TRIGGER IF NOT EXISTS conversation_input_session_deleted AFTER DELETE ON sessions BEGIN UPDATE conversation_inputs SET retired=1 WHERE session_id=OLD.id; END;
            CREATE TRIGGER IF NOT EXISTS conversation_input_owner_revoked AFTER UPDATE OF allowed ON users WHEN NEW.allowed=0 BEGIN UPDATE conversation_inputs SET retired=1 WHERE human_id=NEW.tg_user_id; END;")?;
        conn.execute("UPDATE conversation_inputs SET phase=CASE WHEN phase IN ('sending','steering') THEN 'uncertain' ELSE 'retained_unsent' END WHERE instance_id!=?1 AND phase IN ('retained','queued','sending','steering')",[&self.instance_id])?;
        Ok(())
    }

    /// Retain exact owner-authenticated text before any native write. UUID reuse
    /// inspects the original outcome; it never authorizes another submission.
    pub fn conversation_input_capture(
        &self,
        human: i64,
        entry: SessionKey,
        binding: &str,
        id: &str,
        text: &str,
    ) -> Result<(ConversationInput, bool)> {
        if Uuid::parse_str(id)
            .ok()
            .is_none_or(|uuid| uuid.to_string() != id)
            || text.trim().is_empty()
            || text.len() > 32 * 1024
            || text.contains('\0')
        {
            bail!("Invalid bounded conversation input");
        }
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction()?;
        let (session, native) = super::presentation::entry_on(&tx, human, entry)?;
        if binding != super::presentation::entry_binding(human, entry, session, &native) {
            bail!("Selected conversation changed before input capture");
        }
        let existing: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM conversation_inputs WHERE id=?1)",
            [id],
            |r| r.get(0),
        )?;
        if existing {
            let input = read_on(&tx, id, human)?;
            if input.entry != entry || input.text != text {
                bail!("Original conversation input identity is immutable");
            }
            return Ok((input, false));
        }
        let input = ConversationInput {
            id: id.into(),
            human,
            entry,
            session,
            native,
            epoch: super::stopping::admission_epoch_on(&tx, entry)?,
            text: text.into(),
            phase: if super::stopping::fenced_on(&tx, entry)?
                || super::maintenance_writer_held_on(&tx, entry)?
            {
                "retained_unsent"
            } else {
                "retained"
            }
            .into(),
            local_turn: None,
            retired: false,
        };
        tx.execute("INSERT INTO conversation_inputs(id,human_id,session_id,native_thread,phase,doc,instance_id,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",params![input.id,human,session,input.native,input.phase,serde_json::to_string(&input)?,self.instance_id,now_string()])?;
        tx.commit()?;
        Ok((input, true))
    }

    pub fn conversation_input_read(
        &self,
        human: i64,
        entry: SessionKey,
        id: &str,
    ) -> Result<ConversationInput> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        // Owners may inspect a retained old binding, but cannot dispatch it.
        let (session, _) = super::presentation::entry_on(&conn, human, entry)?;
        let input = read_on(&conn, id, human)?;
        if input.entry != entry || input.session != session {
            bail!("Input belongs to another conversation");
        }
        Ok(input)
    }

    pub fn conversation_input_validate(&self, human: i64, id: &str) -> Result<()> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let input = read_on(&conn, id, human)?;
        eligible_on(&conn, &input)?;
        if !matches!(input.phase.as_str(), "queued" | "sending" | "steering") {
            bail!("Conversation input is not eligible for native submission");
        }
        Ok(())
    }

    pub fn conversation_input_finish(&self, human: i64, id: &str) -> Result<()> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        conn.execute("UPDATE conversation_inputs SET phase=CASE WHEN phase='queued' THEN 'retained_unsent' ELSE 'uncertain' END WHERE id=?1 AND human_id=?2 AND instance_id=?3 AND phase IN ('queued','sending','steering')",params![id,human,self.instance_id])?;
        Ok(())
    }

    /// Begin events validate the current owner/binding/epoch in the same SQLite
    /// transaction. Observed acknowledgments remain recordable after revocation
    /// or a fence: recording an effect does not authorize future execution.
    pub fn conversation_input_event(
        &self,
        human: i64,
        id: &str,
        event: ConversationInputEvent,
    ) -> Result<ConversationInput> {
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction()?;
        let input = read_on(&tx, id, human)?;
        if let ConversationInputEvent::Accepted { local_turn } = event
            && input.phase == "accepted"
            && input.local_turn == Some(local_turn)
        {
            return Ok(input);
        }
        let (phase, local) = match event {
            ConversationInputEvent::Queue
                if matches!(input.phase.as_str(), "retained" | "rejected") =>
            {
                eligible_on(&tx, &input)?;
                ("queued", None)
            }
            ConversationInputEvent::Begin { local_turn, steer }
                if input.phase == "queued"
                    || (input.phase == "rejected" && input.local_turn == Some(local_turn)) =>
            {
                eligible_on(&tx, &input)?;
                let matches:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM turns WHERE id=?1 AND session_id=?2 AND from_user_id=?3 AND (native_thread_id IS NULL OR native_thread_id=?4))",params![local_turn,input.session,human,input.native],|r|r.get(0))?;
                if !matches {
                    bail!("Conversation input execution mismatch");
                }
                (if steer { "steering" } else { "sending" }, Some(local_turn))
            }
            ConversationInputEvent::Accepted { local_turn }
                if input.local_turn == Some(local_turn)
                    && matches!(input.phase.as_str(), "sending" | "steering" | "uncertain") =>
            {
                ("accepted", Some(local_turn))
            }
            ConversationInputEvent::Rejected { local_turn }
                if input.local_turn == Some(local_turn)
                    && matches!(input.phase.as_str(), "sending" | "steering") =>
            {
                ("rejected", Some(local_turn))
            }
            ConversationInputEvent::Uncertain { local_turn }
                if input.local_turn == Some(local_turn)
                    && matches!(input.phase.as_str(), "sending" | "steering") =>
            {
                ("uncertain", Some(local_turn))
            }
            _ => bail!("Conversation input cannot be replayed or settled by another execution"),
        };
        let changed=tx.execute("UPDATE conversation_inputs SET phase=?1,local_turn=?2 WHERE id=?3 AND human_id=?4 AND phase=?5 AND instance_id=?6",params![phase,local,id,human,input.phase,self.instance_id])?;
        if changed != 1 {
            bail!("Conversation input belongs to a retired listener");
        }
        let updated = read_on(&tx, id, human)?;
        tx.commit()?;
        Ok(updated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ConversationInputEvent as E;

    fn setup() -> (tempfile::NamedTempFile, Store, SessionKey, String) {
        let file = tempfile::NamedTempFile::new().unwrap();
        let store = Store::open(file.path(), &[100], &super::super::tests::defaults()).unwrap();
        let key = SessionKey::new(100, Some(1));
        store
            .ensure_session(key, 100, &super::super::tests::defaults())
            .unwrap();
        store.set_session_codex_thread(key, "entry-native").unwrap();
        let binding = store.interaction_presentation_list(100, key, 0, 0).unwrap()["binding"]
            .as_str()
            .unwrap()
            .to_string();
        (file, store, key, binding)
    }

    fn turn(store: &Store, key: SessionKey) -> i64 {
        let session = store.get_session(key).unwrap().unwrap();
        let local = store
            .record_turn_started(
                session.id,
                &TurnRequest {
                    session_key: key,
                    from_user_id: 100,
                    prompt: "Original".into(),
                    runtime_instructions: None,
                    attachments: vec![],
                    review_mode: None,
                    override_search_mode: None,
                },
            )
            .unwrap();
        store
            .record_turn_native_thread(local, "entry-native")
            .unwrap();
        local
    }

    #[test]
    fn exact_original_survives_reconnect_without_a_telegram_update_or_second_submission() {
        let (_file, store, key, binding) = setup();
        let id = Uuid::now_v7().to_string();
        let original = "  Keep this exact message.\n¿Y la corrección? 🦉 ";
        let (input, fresh) = store
            .conversation_input_capture(100, key, &binding, &id, original)
            .unwrap();
        assert!(fresh);
        assert_eq!(input.text, original);
        assert!(
            !store
                .conversation_input_capture(100, key, &binding, &id, original)
                .unwrap()
                .1
        );
        assert!(
            store
                .conversation_input_capture(100, key, &binding, &id, "Edited")
                .is_err()
        );
        assert!(
            store
                .conversation_input_capture(101, key, &binding, &id, original)
                .is_err()
        );
        assert!(
            store
                .conversation_input_capture(100, key, "stale", &id, original)
                .is_err()
        );
        store.conversation_input_event(100, &id, E::Queue).unwrap();
        assert!(store.conversation_input_event(100, &id, E::Queue).is_err());
        let local = turn(&store, key);
        store
            .conversation_input_event(
                100,
                &id,
                E::Begin {
                    local_turn: local,
                    steer: true,
                },
            )
            .unwrap();
        assert!(
            store
                .conversation_input_event(
                    100,
                    &id,
                    E::Accepted {
                        local_turn: local + 1
                    }
                )
                .is_err()
        );
        store
            .conversation_input_event(100, &id, E::Accepted { local_turn: local })
            .unwrap();
        let (again, fresh) = store
            .conversation_input_capture(100, key, &binding, &id, original)
            .unwrap();
        assert!(!fresh);
        assert_eq!(again.phase, "accepted");
        assert_eq!(again.local_turn, Some(local));
        assert!(store.conversation_input_event(100, &id, E::Queue).is_err());
        let conn = store.conn.lock().unwrap();
        assert_eq!(
            conn.query_row("SELECT count(*) FROM incoming_updates", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }

    #[test]
    fn restart_retains_known_unsent_and_uncertain_inputs_without_rearming_them() {
        let (file, store, key, binding) = setup();
        let queued = Uuid::now_v7().to_string();
        let sent = Uuid::now_v7().to_string();
        for id in [&queued, &sent] {
            store
                .conversation_input_capture(100, key, &binding, id, "Retain original")
                .unwrap();
            store.conversation_input_event(100, id, E::Queue).unwrap();
        }
        let local = turn(&store, key);
        store
            .conversation_input_event(
                100,
                &sent,
                E::Begin {
                    local_turn: local,
                    steer: false,
                },
            )
            .unwrap();
        drop(store);
        let store = Store::open(file.path(), &[100], &super::super::tests::defaults()).unwrap();
        assert_eq!(
            store
                .conversation_input_read(100, key, &queued)
                .unwrap()
                .phase,
            "retained_unsent"
        );
        assert_eq!(
            store
                .conversation_input_read(100, key, &sent)
                .unwrap()
                .phase,
            "uncertain"
        );
        assert!(
            store
                .conversation_input_event(100, &queued, E::Queue)
                .is_err()
        );
        assert!(
            store
                .conversation_input_event(100, &sent, E::Queue)
                .is_err()
        );
        assert!(
            !store
                .conversation_input_capture(100, key, &binding, &sent, "Retain original")
                .unwrap()
                .1
        );
    }

    #[test]
    fn retired_epochs_and_bindings_never_revive_but_observed_receipts_are_retained() {
        let (_file, store, key, binding) = setup();
        let id = Uuid::now_v7().to_string();
        store
            .conversation_input_capture(100, key, &binding, &id, "Correct the work")
            .unwrap();
        store.conversation_input_event(100, &id, E::Queue).unwrap();
        let local = turn(&store, key);
        store
            .conversation_input_event(
                100,
                &id,
                E::Begin {
                    local_turn: local,
                    steer: true,
                },
            )
            .unwrap();
        store
            .save_bot_state("stop_admission_epoch:100", "new-human-admission-generation")
            .unwrap();
        store
            .conn
            .lock()
            .unwrap()
            .execute("UPDATE users SET allowed=0 WHERE tg_user_id=100", [])
            .unwrap();
        // An actual native acknowledgment remains evidence after grant revocation.
        let observed = store
            .conversation_input_event(100, &id, E::Accepted { local_turn: local })
            .unwrap();
        assert_eq!(observed.phase, "accepted");
        assert!(store.conversation_input_read(100, key, &id).is_err());
        store
            .conn
            .lock()
            .unwrap()
            .execute("UPDATE users SET allowed=1 WHERE tg_user_id=100", [])
            .unwrap();
        assert!(eligible_on(&store.conn.lock().unwrap(), &observed).is_err());
        store
            .set_session_codex_thread(key, "different-native")
            .unwrap();
        assert_eq!(
            store.conversation_input_read(100, key, &id).unwrap().native,
            "entry-native"
        );
        assert!(
            store
                .conversation_input_capture(
                    100,
                    key,
                    &binding,
                    &Uuid::now_v7().to_string(),
                    "New message"
                )
                .is_err()
        );
    }

    #[test]
    fn restored_grant_or_binding_cannot_revive_an_original_input() {
        for change in ["clear", "revoke"] {
            let (_file, store, key, binding) = setup();
            let id = Uuid::now_v7().to_string();
            store
                .conversation_input_capture(100, key, &binding, &id, "Keep original authority")
                .unwrap();
            store.conversation_input_event(100, &id, E::Queue).unwrap();
            if change == "clear" {
                store.clear_session_conversation(key).unwrap();
                store.set_session_codex_thread(key, "entry-native").unwrap();
            } else {
                let conn = store.conn.lock().unwrap();
                conn.execute("UPDATE users SET allowed=0 WHERE tg_user_id=100", [])
                    .unwrap();
                conn.execute("UPDATE users SET allowed=1 WHERE tg_user_id=100", [])
                    .unwrap();
            }
            assert!(
                store
                    .conversation_input_read(100, key, &id)
                    .unwrap()
                    .retired
            );
            assert!(store.conversation_input_validate(100, &id).is_err());
        }
    }

    #[test]
    fn deletion_preserves_originals_and_maintenance_hold_prevents_submission() {
        let (_file, store, key, binding) = setup();
        let id = Uuid::now_v7().to_string();
        let (original, _) = store
            .conversation_input_capture(100, key, &binding, &id, "Preserve despite deletion")
            .unwrap();
        store.conversation_input_event(100, &id, E::Queue).unwrap();
        store
            .save_bot_state("native_writer_hold:100:1", "fixture-hold")
            .unwrap();
        assert!(store.conversation_input_validate(100, &id).is_err());
        let local = turn(&store, key);
        assert!(
            store
                .conversation_input_event(
                    100,
                    &id,
                    E::Begin {
                        local_turn: local,
                        steer: true
                    }
                )
                .is_err()
        );
        store.conversation_input_finish(100, &id).unwrap();
        assert_eq!(
            store.conversation_input_read(100, key, &id).unwrap().phase,
            "retained_unsent"
        );
        store.take_bot_state("native_writer_hold:100:1").unwrap();
        let accepted_id = Uuid::now_v7().to_string();
        store
            .conversation_input_capture(100, key, &binding, &accepted_id, "Accepted original")
            .unwrap();
        store
            .conversation_input_event(100, &accepted_id, E::Queue)
            .unwrap();
        store
            .conversation_input_event(
                100,
                &accepted_id,
                E::Begin {
                    local_turn: local,
                    steer: true,
                },
            )
            .unwrap();
        store
            .conversation_input_event(100, &accepted_id, E::Uncertain { local_turn: local })
            .unwrap();
        store
            .conversation_input_event(100, &accepted_id, E::Accepted { local_turn: local })
            .unwrap();
        store.delete_session(key).unwrap();
        let retained = read_on(&store.conn.lock().unwrap(), &id, 100).unwrap();
        assert_eq!(retained.text, original.text);
        assert!(retained.retired);
        let accepted = read_on(&store.conn.lock().unwrap(), &accepted_id, 100).unwrap();
        assert!(accepted.retired);
        assert_eq!(accepted.phase, "accepted");
        assert_eq!(accepted.local_turn, Some(local));
        let new = store
            .ensure_session(key, 100, &super::super::tests::defaults())
            .unwrap();
        store.set_session_codex_thread(key, "entry-native").unwrap();
        assert_ne!(new.id, original.session);
        assert!(store.conversation_input_validate(100, &id).is_err());
    }
}
