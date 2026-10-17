//! Canonical native output identity for the integrating conversation.
//!
//! Native completion records describe observed items, not which instructions a
//! worker incorporated. Request coverage and publication are separate states.
use super::*;

impl Store {
    pub(super) fn init_interaction(&self) -> Result<()> {
        self.conn
            .lock()
            .expect("store mutex poisoned")
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS interaction_native_messages(
                native_thread TEXT NOT NULL,
                native_turn TEXT NOT NULL,
                item_id TEXT NOT NULL,
                local_turn INTEGER NOT NULL REFERENCES turns(id) ON DELETE CASCADE,
                phase TEXT,
                text TEXT NOT NULL,
                observed_at TEXT NOT NULL,
                PRIMARY KEY(native_thread,native_turn,item_id)
             );
             CREATE INDEX IF NOT EXISTS interaction_native_messages_local
                ON interaction_native_messages(local_turn);",
            )?;
        Ok(())
    }

    /// Preserve one immutable canonical item before any text-only presentation.
    /// Exact repeated observations are harmless; conflicting reuse is an error.
    #[allow(clippy::too_many_arguments)]
    pub fn record_native_message(
        &self,
        local_turn: i64,
        thread: &str,
        turn: &str,
        item: &str,
        phase: Option<&str>,
        text: &str,
    ) -> Result<bool> {
        if thread.is_empty() || turn.is_empty() || item.is_empty() {
            return Err(anyhow!("native output identity is incomplete"));
        }
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction()?;
        let bound: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM turns WHERE id=?1 AND native_thread_id=?2)",
            params![local_turn, thread],
            |r| r.get(0),
        )?;
        if !bound {
            return Err(anyhow!("native output does not match the owning execution"));
        }
        let inserted = tx.execute(
            "INSERT OR IGNORE INTO interaction_native_messages
             (native_thread,native_turn,item_id,local_turn,phase,text,observed_at)
             VALUES(?1,?2,?3,?4,?5,?6,?7)",
            params![thread, turn, item, local_turn, phase, text, now_string()],
        )? == 1;
        if !inserted {
            let matches: bool = tx.query_row(
                "SELECT phase IS ?4 AND text=?5 FROM interaction_native_messages
                 WHERE native_thread=?1 AND native_turn=?2 AND item_id=?3",
                params![thread, turn, item, phase, text],
                |r| r.get(0),
            )?;
            if !matches {
                return Err(anyhow!(
                    "completed native item identity was reused with different content"
                ));
            }
        }
        tx.commit()?;
        Ok(inserted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn execution(store: &Store, thread: &str) -> i64 {
        let key = SessionKey::new(100, Some(7));
        let session = store
            .ensure_session(key, 100, &super::super::tests::defaults())
            .unwrap();
        let id = store
            .record_turn_started(
                session.id,
                &TurnRequest {
                    session_key: key,
                    from_user_id: 100,
                    prompt: "A request".into(),
                    runtime_instructions: None,
                    attachments: vec![],
                    review_mode: None,
                    override_search_mode: None,
                },
            )
            .unwrap();
        store.record_turn_native_thread(id, thread).unwrap();
        id
    }

    #[test]
    fn distinct_items_survive_one_turn_and_repeated_observations() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let store = Store::open(file.path(), &[100], &super::super::tests::defaults()).unwrap();
        let local = execution(&store, "thread");
        assert!(
            store
                .record_native_message(
                    local,
                    "thread",
                    "turn",
                    "A",
                    Some("final_answer"),
                    "First answer"
                )
                .unwrap()
        );
        assert!(
            !store
                .record_native_message(
                    local,
                    "thread",
                    "turn",
                    "A",
                    Some("final_answer"),
                    "First answer"
                )
                .unwrap()
        );
        assert!(
            store
                .record_native_message(
                    local,
                    "thread",
                    "turn",
                    "B",
                    Some("final_answer"),
                    "Second answer"
                )
                .unwrap()
        );
        assert!(
            store
                .record_native_message(
                    local,
                    "thread",
                    "turn",
                    "A",
                    Some("final_answer"),
                    "Conflicting answer"
                )
                .is_err()
        );
        drop(store);
        let store = Store::open(file.path(), &[100], &super::super::tests::defaults()).unwrap();
        assert!(
            !store
                .record_native_message(
                    local,
                    "thread",
                    "turn",
                    "A",
                    Some("final_answer"),
                    "First answer"
                )
                .unwrap()
        );
        let count: i64 = store
            .conn
            .lock()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM interaction_native_messages",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn unrelated_thread_and_incomplete_identity_cannot_be_recorded() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let store = Store::open(file.path(), &[100], &super::super::tests::defaults()).unwrap();
        let local = execution(&store, "thread");
        assert!(
            store
                .record_native_message(local, "other", "turn", "A", None, "answer")
                .is_err()
        );
        assert!(
            store
                .record_native_message(local, "thread", "turn", "", None, "answer")
                .is_err()
        );
    }
}
