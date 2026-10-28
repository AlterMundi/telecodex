//! Owner-scoped native requests and exact, non-replaying decision receipts.
use super::*;
use anyhow::bail;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const CANONICAL_SCOPE: &str = " AND EXISTS(
    SELECT 1 FROM conversation_returns p WHERE p.human_id=?2 AND p.native_thread=h.native_thread
    AND (EXISTS(SELECT 1 FROM conversation_inputs c WHERE c.id=p.source_input AND c.human_id=?2
        AND json_extract(c.doc,'$.entry.chat_id')=?3 AND json_extract(c.doc,'$.entry.thread_id')=?4)
      OR EXISTS(SELECT 1 FROM interaction_instructions i JOIN interaction_sources x ON x.id=i.input_id
        WHERE i.session_id=s.id AND x.human_id=?2 AND x.origin_chat=?3 AND x.origin_thread=?4
        AND x.origin_message IS NULL AND json_extract(i.doc,'$.local_turn')=h.local_turn)))";

pub struct ConversationRequest {
    pub item: HumanInteraction,
    pub key: SessionKey,
    pub binding: String,
    pub entry_binding: Value,
    pub scope: String,
    pub capture_version: String,
    pub internal: bool,
}

pub(super) fn read_on(
    conn: &Connection,
    human: i64,
    entry: SessionKey,
    id: &str,
) -> Result<ConversationRequest> {
    read_scoped_on(conn, human, entry, id, false)
}

fn read_scoped_on(
    conn: &Connection,
    human: i64,
    entry: SessionKey,
    id: &str,
    internal: bool,
) -> Result<ConversationRequest> {
    let selection = if internal {
        " AND NOT(s.chat_id=?3 AND s.thread_id=?4)"
    } else {
        CANONICAL_SCOPE
    };
    let (session, native) = super::presentation::entry_on(conn, human, entry)?;
    let (raw, state, chat, topic, instance): (String, String, i64, i64, String) = conn
        .query_row(
            &format!(
                "SELECT h.payload_json,h.state,s.chat_id,s.thread_id,h.instance_id
         FROM human_interactions h JOIN sessions s ON s.id=h.session_id
         JOIN turns t ON t.id=h.local_turn JOIN users u ON u.tg_user_id=h.requester
         WHERE h.id=?1 AND h.kind='native' AND h.requester=?2 AND u.allowed=1
         AND s.creator_user_id=?2 AND t.from_user_id=?2 AND t.session_id=s.id
         AND h.native_thread=s.codex_thread_id AND t.native_thread_id=h.native_thread
         AND s.force_fresh_thread=0 {selection}"
            ),
            params![id, human, entry.chat_id, entry.thread_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .context(if internal {
            "Native request is outside current owned internal sessions"
        } else {
            "Native request is outside this owned canonical conversation"
        })?;
    let mut item: HumanInteraction = serde_json::from_str(&raw)?;
    item.state = state;
    let key = SessionKey::new(chat, Some(topic));
    let binding = super::presentation::entry_binding(conn, human, entry, session)?;
    let proof = json!([
        binding,
        key,
        item.native_thread,
        item.local_turn,
        instance,
        super::stopping::admission_epoch_on(conn, entry)?,
        super::stopping::admission_epoch_on(conn, key)?
    ]);
    let proof = if internal {
        json!(["internal-pending/v1", proof])
    } else {
        proof
    };
    let scope = format!("{:x}", Sha256::digest(proof.to_string().as_bytes()));
    let index = item.source["conversationIndex"].as_u64().unwrap_or(0);
    let proposal = &item.source["request"]["questions"][index as usize];
    let capture_version = format!(
        "{:x}",
        Sha256::digest(
            json!([scope, item.id, index, proposal, item.title, item.action])
                .to_string()
                .as_bytes()
        )
    );
    Ok(ConversationRequest {
        item,
        key,
        binding,
        entry_binding: json!([human, entry, session, native]),
        scope,
        capture_version,
        internal,
    })
}

/// Snapshot only current canonical owned questions; keep bodies out of originals.
/// The bound exceeds the twenty-context qualification and does not block capture.
pub(super) fn capture_questions_on(
    conn: &Connection,
    human: i64,
    entry: SessionKey,
) -> Result<std::collections::BTreeMap<String, String>> {
    let mut statement = conn.prepare(&format!(
        "SELECT h.id FROM human_interactions h
        JOIN sessions s ON s.id=h.session_id JOIN users u ON u.tg_user_id=h.requester
        JOIN turns t ON t.id=h.local_turn
        WHERE h.kind='native' AND h.state='pending' AND h.requester=?2 AND u.allowed=1
        AND s.creator_user_id=?2 AND s.codex_thread_id=h.native_thread AND s.force_fresh_thread=0
        AND t.session_id=s.id AND t.from_user_id=?2 AND t.native_thread_id=h.native_thread
        AND t.status='running' AND json_extract(h.payload_json,'$.source.kind')='question'
        {CANONICAL_SCOPE} ORDER BY h.rowid LIMIT 128"
    ))?;
    let ids = statement
        .query_map(params![0, human, entry.chat_id, entry.thread_id], |r| {
            r.get::<_, String>(0)
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    ids.into_iter()
        .map(|id| {
            let current = read_on(conn, human, entry, &id)?;
            Ok((id, current.capture_version))
        })
        .collect()
}

impl Store {
    pub(super) fn init_conversation_requests(&self) -> Result<()> {
        // Retirement must cover exactly the same direct/derived canonical
        // lineage as request admission, including a recreated entry address.
        let lineage = |row: &str| {
            CANONICAL_SCOPE
                .replace("?2", "h.requester")
                .replace("?3", &format!("{row}.chat_id"))
                .replace("?4", &format!("{row}.thread_id"))
        };
        let on_update = lineage("NEW");
        let on_delete = lineage("OLD");
        self.conn.lock().expect("store mutex poisoned").execute_batch(&format!("BEGIN IMMEDIATE; DROP TRIGGER IF EXISTS native_request_binding_retired; CREATE TABLE IF NOT EXISTS conversation_decisions(
            human_id INTEGER NOT NULL,entry_session INTEGER NOT NULL,entry_chat INTEGER NOT NULL,entry_topic INTEGER NOT NULL,
            request_id TEXT NOT NULL,revision TEXT NOT NULL,doc TEXT NOT NULL,created_at TEXT NOT NULL,
            PRIMARY KEY(human_id,entry_session,request_id,revision));
            CREATE TABLE IF NOT EXISTS conversation_request_order(
              sequence INTEGER PRIMARY KEY AUTOINCREMENT,request_id TEXT NOT NULL UNIQUE);
            INSERT INTO conversation_request_order(request_id) SELECT h.id FROM human_interactions h
              WHERE h.kind='native' AND NOT EXISTS(SELECT 1 FROM conversation_request_order o WHERE o.request_id=h.id) ORDER BY h.rowid;
            CREATE TRIGGER IF NOT EXISTS native_request_discovery_order AFTER INSERT ON human_interactions WHEN NEW.kind='native'
            BEGIN INSERT INTO conversation_request_order(request_id) VALUES(NEW.id); END;
            CREATE TRIGGER IF NOT EXISTS native_request_binding_retired AFTER UPDATE OF codex_thread_id,force_fresh_thread,creator_user_id ON sessions
            WHEN (OLD.codex_thread_id IS NOT NULL AND OLD.codex_thread_id IS NOT NEW.codex_thread_id) OR OLD.creator_user_id!=NEW.creator_user_id OR NEW.force_fresh_thread=1
            BEGIN UPDATE human_interactions SET state=CASE WHEN state='pending' THEN 'expired' ELSE 'unknown' END
              WHERE kind='native' AND state IN ('pending','answering') AND (session_id=NEW.id OR id IN(
                SELECT h.id FROM human_interactions h JOIN sessions s ON s.id=h.session_id WHERE 1=1 {on_update})); END;
            CREATE TRIGGER IF NOT EXISTS native_request_entry_deleted BEFORE DELETE ON sessions
            BEGIN UPDATE human_interactions SET state=CASE WHEN state='pending' THEN 'expired' ELSE 'unknown' END
              WHERE kind='native' AND state IN ('pending','answering') AND (session_id=OLD.id OR id IN(
                SELECT h.id FROM human_interactions h JOIN sessions s ON s.id=h.session_id WHERE 1=1 {on_delete})); END;
            CREATE TRIGGER IF NOT EXISTS native_request_grant_retired AFTER UPDATE OF allowed ON users WHEN NEW.allowed=0
            BEGIN UPDATE human_interactions SET state=CASE WHEN state='pending' THEN 'expired' ELSE 'unknown' END
              WHERE kind='native' AND requester=NEW.tg_user_id AND state IN ('pending','answering'); END; COMMIT;"))?;
        Ok(())
    }
    pub fn conversation_question_at_capture(
        &self,
        human: i64,
        entry: SessionKey,
        input_id: &str,
        question: &str,
    ) -> Result<bool> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let input = super::conversation_inputs::read_on(&conn, input_id, human)?;
        super::conversation_inputs::eligible_on(&conn, &input)?;
        if input.entry != entry {
            bail!("Original belongs to another entry");
        }
        let current = read_on(&conn, human, entry, question)?;
        Ok(input
            .question_versions
            .as_ref()
            .and_then(|v| v.get(question))
            == Some(&current.capture_version))
    }

    pub fn conversation_answer_receipt(
        &self,
        human: i64,
        entry: SessionKey,
        input_id: &str,
    ) -> Result<Option<Value>> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let (session, _) = super::presentation::entry_on(&conn, human, entry)?;
        let raw:Option<String> = conn.query_row("SELECT doc FROM conversation_decisions WHERE human_id=?1 AND entry_session=?2 AND entry_chat=?3 AND entry_topic=?4 AND json_extract(doc,'$.source_input')=?5",params![human,session,entry.chat_id,entry.thread_id,input_id],|r|r.get(0)).optional()?;
        raw.map(|r| serde_json::from_str(&r).map_err(Into::into))
            .transpose()
    }

    pub fn conversation_request_binding(&self, human: i64, entry: SessionKey) -> Result<Value> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let (session, native) = super::presentation::entry_on(&conn, human, entry)?;
        Ok(
            json!({"binding":super::presentation::entry_binding(&conn,human,entry,session)?,"entry_binding":[human,entry,session,native]}),
        )
    }
    /// Receipt retention has no live-session/turn foreign key and never restores
    /// a responder. Current entry ownership is still required for inspection.
    pub fn conversation_decision_receipt(
        &self,
        human: i64,
        entry: SessionKey,
        id: &str,
        revision: &str,
    ) -> Result<Option<Value>> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let (session, _) = super::presentation::entry_on(&conn, human, entry)?;
        let raw:Option<String>=conn.query_row("SELECT doc FROM conversation_decisions WHERE human_id=?1 AND entry_session=?2 AND entry_chat=?3 AND entry_topic=?4 AND request_id=?5 AND revision=?6",params![human,session,entry.chat_id,entry.thread_id,id,revision],|r|r.get(0)).optional()?;
        raw.map(|raw| serde_json::from_str(&raw).map_err(Into::into))
            .transpose()
    }
    pub fn native_request_pending(&self, human: i64, id: &str, local: i64) -> Result<bool> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        Ok(conn.query_row("SELECT EXISTS(SELECT 1 FROM human_interactions h JOIN sessions s ON s.id=h.session_id
          JOIN users u ON u.tg_user_id=h.requester WHERE h.id=?1 AND h.requester=?2 AND h.local_turn=?3 AND h.instance_id=?4
          AND h.state='pending' AND h.kind='native' AND u.allowed=1 AND s.creator_user_id=?2 AND s.codex_thread_id=h.native_thread AND s.force_fresh_thread=0)",params![id,human,local,self.instance_id],|r|r.get(0))?)
    }

    pub fn conversation_request_channel_closed(&self, human: i64, id: &str) -> Result<()> {
        self.conn.lock().expect("store mutex poisoned").execute(
            "UPDATE human_interactions SET state='expired',updated_at=?4 WHERE id=?1 AND requester=?2 AND instance_id=?3 AND state IN ('pending','answering')",
            params![id,human,self.instance_id,now_string()])?;
        Ok(())
    }

    /// Keyset navigation includes retained outcomes; no responder is recreated.
    pub fn conversation_request_ids(
        &self,
        human: i64,
        entry: SessionKey,
        after: i64,
    ) -> Result<(Vec<(i64, String)>, bool)> {
        if after < 0 {
            bail!("Invalid request cursor");
        }
        let conn = self.conn.lock().expect("store mutex poisoned");
        super::presentation::entry_on(&conn, human, entry)?;
        // The outer query bounds candidates to canonical owned contexts from
        // this entry; detail/decision revalidate the complete source binding.
        let mut statement=conn.prepare(&format!("SELECT o.sequence,h.id FROM human_interactions h JOIN conversation_request_order o ON o.request_id=h.id
            JOIN sessions s ON s.id=h.session_id JOIN users u ON u.tg_user_id=h.requester
            JOIN turns t ON t.id=h.local_turn
            WHERE o.sequence>?1 AND h.kind='native' AND h.requester=?2 AND u.allowed=1
            AND s.creator_user_id=?2 AND s.codex_thread_id=h.native_thread AND s.force_fresh_thread=0
            AND t.session_id=s.id AND t.from_user_id=?2 AND t.native_thread_id=h.native_thread
            AND h.state IN ('pending','answering','unknown')
            {CANONICAL_SCOPE}
            ORDER BY o.sequence LIMIT 9"))?;
        let mut rows = statement
            .query_map(params![after, human, entry.chat_id, entry.thread_id], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let more = rows.len() > 8;
        rows.truncate(8);
        Ok((rows, more))
    }

    pub fn conversation_request(
        &self,
        human: i64,
        entry: SessionKey,
        id: &str,
    ) -> Result<ConversationRequest> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        read_on(&conn, human, entry, id)
    }

    /// Explicit internal-session access; canonical readers retain their existing scope.
    pub fn internal_native_request(
        &self,
        human: i64,
        entry: SessionKey,
        id: &str,
    ) -> Result<ConversationRequest> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        read_scoped_on(&conn, human, entry, id, true)
    }

    /// Commit the explicit human decision before consuming its live responder.
    /// Repeating its revision inspects the receipt, never executes again.
    #[allow(clippy::too_many_arguments)]
    pub fn conversation_request_decide(
        &self,
        human: i64,
        entry: SessionKey,
        expected: &ConversationRequest,
        revision: &str,
        action: &Value,
        proposal: &Value,
        resolved: &Value,
        next_state: &str,
        next_question: Option<(&str, &str, usize)>,
        source_input: Option<&str>,
        interpretation: Option<(&str, &tokio_util::sync::CancellationToken, bool)>,
    ) -> Result<bool> {
        if revision.len() != 64
            || !revision.bytes().all(|b| b.is_ascii_hexdigit())
            || !matches!(next_state, "pending" | "answering" | "cancelled")
        {
            bail!("Invalid decision");
        }
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction()?;
        if expected.internal && source_input.is_some() {
            bail!("Internal-session decisions require their explicit controls");
        }
        let current = read_scoped_on(&tx, human, entry, &expected.item.id, expected.internal)?;
        if current.scope != expected.scope || current.binding != expected.binding {
            bail!("Native request binding changed");
        }
        let original = source_input
            .map(|id| {
                let input = super::conversation_inputs::read_on(&tx, id, human)?;
                super::goal_controls::unclaimed_on(&tx,human,"conversation",&input.id)?;
                super::conversation_inputs::eligible_on(&tx, &input)?;
                let contextual = input.reply.as_ref().is_some_and(|r| r.context_only);
                let eligible = if let Some((native, closed, explicit)) = interpretation {
                    let linked: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM turns WHERE id=?1 AND session_id=?2 AND from_user_id=?3 AND native_thread_id=?4)",params![input.local_turn,input.session,human,native],|r|r.get(0))?;
                    let routed: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM interaction_sources WHERE channel='conversation' AND event_id=?1 AND human_id=?2)",params![input.id,human],|r|r.get(0))?;
                    let creating: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM interaction_creations WHERE human_id=?1 AND entry_chat=?2 AND entry_topic=?3 AND input_update=?4)",params![human,entry.chat_id,entry.thread_id,input.id],|r|r.get(0))?;
                    (contextual || explicit) && input.native == native && linked && !routed && !creating && !closed.is_cancelled()
                        && matches!(input.phase.as_str(),"sending"|"steering"|"accepted")
                } else {
                    !contextual && input.phase == "retained"
                };
                if input.entry != entry
                    || !eligible
                    || if interpretation.is_some_and(|(_, _, explicit)| explicit) {
                        input.question_versions.as_ref().and_then(|v| v.get(&current.item.id))
                            != Some(&current.capture_version)
                    } else {
                        input.reply.as_ref().is_none_or(|r| {
                            r.source != format!("request:{}", expected.item.id) || r.revision != revision
                        }) || input.focus.as_ref().is_none_or(|f| {
                            f.retired || f.target != current.key
                                || f.native.as_deref() != Some(current.item.native_thread.as_str())
                        })
                    }
                    || action != &json!({"kind":"answer","text":input.question_answer_text(std::path::Path::new(tx.path().unwrap_or("")))})
                    || current.item.source["kind"] != "question"
                {
                    bail!("Original answer source or selected question changed");
                }
                Ok::<_, anyhow::Error>(input)
            })
            .transpose()?;
        let mut item = current.item;
        let (entry_session, _) = super::presentation::entry_on(&tx, human, entry)?;
        let prior:Option<String>=tx.query_row("SELECT doc FROM conversation_decisions WHERE human_id=?1 AND entry_session=?2 AND request_id=?3 AND revision=?4",params![human,entry_session,item.id,revision],|r|r.get(0)).optional()?;
        if let Some(prior) = prior {
            if serde_json::from_str::<Value>(&prior)?["action"] != *action {
                bail!("A recorded decision cannot be replaced");
            }
            return Ok(false);
        }
        let live:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM human_interactions h JOIN turns t ON t.id=h.local_turn WHERE h.id=?1 AND h.state='pending' AND h.instance_id=?2 AND t.status='running')",params![item.id,self.instance_id],|r|r.get(0))?;
        if !live
            || super::stopping::fenced_on(&tx, entry)?
            || super::stopping::fenced_on(&tx, current.key)?
            || super::maintenance_writer_held_on(&tx, entry)?
            || super::maintenance_writer_held_on(&tx, current.key)?
        {
            bail!("Native request no longer accepts a decision");
        }
        if !item.source["conversationDecisions"].is_object() {
            item.source["conversationDecisions"] = json!({});
        }
        let recorded_at = now_string();
        let receipt = json!({"id":item.id,"revision":revision,"action":action,"proposal":proposal,"resolved":resolved,
            "recorded_at":recorded_at,"binding":expected.binding,"source_scope":expected.scope,
            "native_thread":item.native_thread,"local_turn":item.local_turn,"target":current.key,"source_input":source_input});
        tx.execute("INSERT INTO conversation_decisions(human_id,entry_session,entry_chat,entry_topic,request_id,revision,doc,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",params![human,entry_session,entry.chat_id,entry.thread_id,item.id,revision,receipt.to_string(),recorded_at])?;
        item.source["conversationDecisions"][revision] = json!({"recorded_at":recorded_at});
        if let Some((title, question, index)) = next_question {
            item.title = title.into();
            item.action = question.into();
            item.source["conversationIndex"] = json!(index);
        }
        item.state = next_state.into();
        tx.execute(
            "UPDATE human_interactions SET state=?2,payload_json=?3,updated_at=?4 WHERE id=?1",
            params![
                item.id,
                next_state,
                serde_json::to_string(&item)?,
                now_string()
            ],
        )?;
        if let Some(input) = original {
            let changed=tx.execute("UPDATE conversation_inputs SET phase='answered' WHERE id=?1 AND human_id=?2 AND phase=?4 AND instance_id=?3",params![input.id,human,self.instance_id,input.phase])?;
            if changed != 1 {
                bail!("Original answer source no longer current");
            }
        }
        if interpretation.is_some_and(|(_, closed, _)| closed.is_cancelled()) {
            bail!("Context interpretation closed before commitment");
        }
        tx.commit()?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (
        tempfile::NamedTempFile,
        Store,
        SessionKey,
        SessionKey,
        String,
    ) {
        let file = tempfile::NamedTempFile::new().unwrap();
        let defaults = super::super::tests::defaults();
        let store = Store::open(file.path(), &[100, 200], &defaults).unwrap();
        let entry = SessionKey::new(100, Some(1));
        let worker = SessionKey::new(100, Some(7));
        for (key, native) in [(entry, "entry-native"), (worker, "worker-native")] {
            let session = store.ensure_session(key, 100, &defaults).unwrap();
            store.set_session_codex_thread(key, native).unwrap();
            let local = store
                .record_turn_started(
                    session.id,
                    &TurnRequest {
                        session_key: key,
                        from_user_id: 100,
                        prompt: "Synthetic request".into(),
                        runtime_instructions: None,
                        attachments: vec![],
                        review_mode: None,
                        override_search_mode: None,
                    },
                )
                .unwrap();
            store.record_turn_native_thread(local, native).unwrap();
            store
                .save_interaction(
                    key,
                    &HumanInteraction {
                        id: format!("request-{}", key.thread_id),
                        kind: "native".into(),
                        state: "pending".into(),
                        requester: 100,
                        native_thread: native.into(),
                        local_turn: local,
                        title: "Choose".into(),
                        action: "Exact proposal".into(),
                        blocking: true,
                        source: json!({"kind":"question"}),
                    },
                    None,
                )
                .unwrap();
        }
        let binding = store
            .interaction_presentation_list(100, entry, 0, 0)
            .unwrap()["binding"]
            .as_str()
            .unwrap()
            .to_string();
        let source = Uuid::now_v7().to_string();
        store
            .conversation_input_capture(100, entry, &binding, &source, "Original")
            .unwrap();
        store.conn.lock().unwrap().execute("INSERT INTO conversation_returns(human_id,native_thread,source_input,selected_at) VALUES(100,'worker-native',?1,?2)",params![source,now_string()]).unwrap();
        (file, store, entry, worker, binding)
    }
    fn record(
        store: &Store,
        entry: SessionKey,
        current: &ConversationRequest,
        revision: &str,
        action: &Value,
        state: &str,
        next: Option<(&str, &str, usize)>,
    ) -> Result<bool> {
        store.conversation_request_decide(100,entry,current,revision,action,
            &json!({"kind":"question","question_id":"scope","question_index":0,"body":"Exact proposal and option descriptions"}),
            &json!({"question_id":"scope","answer":"Selected exact option"}),state,next,None,None)
    }
    #[test]
    fn internal_request_controls_do_not_widen_canonical_sources() {
        let (_file, store, entry, worker, _binding) = fixture();
        let canonical = store.conversation_request(100, entry, "request-7").unwrap();
        let internal = store
            .internal_native_request(100, entry, "request-7")
            .unwrap();
        assert_ne!(canonical.scope, internal.scope);
        assert_ne!(canonical.capture_version, internal.capture_version);
        assert!(internal.internal);
        assert!(!canonical.internal);
        assert!(
            store
                .internal_native_request(200, entry, "request-7")
                .is_err()
        );
        assert!(
            store
                .internal_native_request(100, worker, "request-7")
                .is_err()
        );
        store
            .conn
            .lock()
            .unwrap()
            .execute("DELETE FROM conversation_returns", [])
            .unwrap();
        assert!(store.conversation_request(100, entry, "request-7").is_err());
        let direct = store
            .internal_native_request(100, entry, "request-7")
            .unwrap();
        assert!(
            record(
                &store,
                entry,
                &direct,
                &"b".repeat(64),
                &json!({"kind":"option","index":0}),
                "answering",
                None
            )
            .unwrap()
        );
        assert_eq!(
            store
                .interaction(worker, 100, "request-7")
                .unwrap()
                .unwrap()
                .state,
            "answering"
        );
    }

    #[test]
    fn canonical_request_scope_and_exact_receipt_prevent_duplicate_or_changed_decisions() {
        let (_file, store, entry, worker, _binding) = fixture();
        assert_eq!(
            store
                .conversation_request_ids(100, entry, 0)
                .unwrap()
                .0
                .len(),
            1
        );
        assert!(store.conversation_request(200, entry, "request-7").is_err());
        assert!(store.conversation_request(100, entry, "request-1").is_err());
        assert!(
            store
                .conversation_request(100, worker, "request-7")
                .is_err()
        );
        let current = store.conversation_request(100, entry, "request-7").unwrap();
        let revision = "a".repeat(64);
        let action = json!({"kind":"option","index":0});
        assert!(
            record(
                &store,
                entry,
                &current,
                &revision,
                &action,
                "pending",
                Some(("Next", "Next question", 1))
            )
            .unwrap()
        );
        assert!(!record(&store, entry, &current, &revision, &action, "pending", None).unwrap());
        assert!(
            record(
                &store,
                entry,
                &current,
                &revision,
                &json!({"kind":"answer","text":"Changed"}),
                "pending",
                None
            )
            .is_err()
        );
        let receipt = store
            .conversation_decision_receipt(100, entry, "request-7", &revision)
            .unwrap()
            .unwrap();
        assert_eq!(receipt["action"], action);
        assert_eq!(receipt["proposal"]["question_id"], "scope");
        assert_eq!(receipt["resolved"]["answer"], "Selected exact option");
        let next = store.conversation_request(100, entry, "request-7").unwrap();
        assert_eq!(next.item.action, "Next question");
        assert_eq!(next.item.source["conversationIndex"], 1);
        assert!(
            record(
                &store,
                entry,
                &next,
                &"b".repeat(64),
                &json!({"kind":"cancel"}),
                "cancelled",
                None
            )
            .unwrap()
        );
        assert!(
            record(
                &store,
                entry,
                &next,
                &"c".repeat(64),
                &action,
                "answering",
                None
            )
            .is_err()
        );
        assert_eq!(
            store
                .conn
                .lock()
                .unwrap()
                .query_row("SELECT count(*) FROM incoming_updates", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
    #[test]
    fn binding_owner_and_grant_restoration_cannot_revive_a_pending_native_request() {
        for change in ["worker", "entry", "grant", "owner"] {
            let (_file, store, entry, worker, _) = fixture();
            let current = store.conversation_request(100, entry, "request-7").unwrap();
            match change {
                "worker" | "entry" => {
                    let key = if change == "worker" { worker } else { entry };
                    let native = if change == "worker" {
                        "worker-native"
                    } else {
                        "entry-native"
                    };
                    store.set_session_codex_thread(key, "replacement").unwrap();
                    store.set_session_codex_thread(key, native).unwrap();
                }
                "owner" => {
                    let conn = store.conn.lock().unwrap();
                    conn.execute(
                        "UPDATE sessions SET creator_user_id=200 WHERE thread_id=7",
                        [],
                    )
                    .unwrap();
                    conn.execute(
                        "UPDATE sessions SET creator_user_id=100 WHERE thread_id=7",
                        [],
                    )
                    .unwrap();
                }
                _ => {
                    let conn = store.conn.lock().unwrap();
                    conn.execute("UPDATE users SET allowed=0 WHERE tg_user_id=100", [])
                        .unwrap();
                    conn.execute("UPDATE users SET allowed=1 WHERE tg_user_id=100", [])
                        .unwrap();
                }
            }
            let fresh = store.conversation_request(100, entry, "request-7").unwrap();
            assert_eq!(fresh.item.state, "expired");
            for snapshot in [&current, &fresh] {
                assert!(
                    !store
                        .native_request_pending(100, "request-7", snapshot.item.local_turn)
                        .unwrap()
                );
                assert!(
                    record(
                        &store,
                        entry,
                        snapshot,
                        &"d".repeat(64),
                        &json!({"kind":"cancel"}),
                        "cancelled",
                        None
                    )
                    .is_err(),
                    "{change} restoration revived request"
                );
            }
        }
    }
    #[test]
    fn request_discovery_cursors_survive_deleted_rowid_reuse_and_restart() {
        let (file, store, entry, _, _) = fixture();
        let base = store
            .conversation_request(100, entry, "request-7")
            .unwrap()
            .item;
        for n in 0..10 {
            let mut item = base.clone();
            item.id = format!("extra-{n}");
            store
                .save_interaction(SessionKey::new(100, Some(7)), &item, None)
                .unwrap();
        }
        let first = store.conversation_request_ids(100, entry, 0).unwrap();
        assert_eq!(first.0.len(), 8);
        assert!(first.1);
        let rest = store
            .conversation_request_ids(100, entry, first.0.last().unwrap().0)
            .unwrap();
        assert_eq!(rest.0.len(), 3);
        let cursor = rest.0.last().unwrap().0;
        let old_rowid: i64 = store
            .conn
            .lock()
            .unwrap()
            .query_row("SELECT max(rowid) FROM human_interactions", [], |r| {
                r.get(0)
            })
            .unwrap();
        store
            .conn
            .lock()
            .unwrap()
            .execute("DELETE FROM human_interactions WHERE id LIKE 'extra-%'", [])
            .unwrap();
        let mut item = base.clone();
        item.id = "new-after-deletion".into();
        store
            .save_interaction(SessionKey::new(100, Some(7)), &item, None)
            .unwrap();
        let new_rowid: i64 = store
            .conn
            .lock()
            .unwrap()
            .query_row(
                "SELECT rowid FROM human_interactions WHERE id=?1",
                [&item.id],
                |r| r.get(0),
            )
            .unwrap();
        assert!(
            new_rowid <= old_rowid,
            "Fixture did not reuse deleted rowids"
        );
        let new = store.conversation_request_ids(100, entry, cursor).unwrap();
        assert_eq!(new.0.len(), 1);
        assert_eq!(new.0[0].1, item.id);
        assert!(new.0[0].0 > cursor);
        let highest = new.0[0].0;
        drop(store);
        let reopened =
            Store::open(file.path(), &[100, 200], &super::super::tests::defaults()).unwrap();
        item.id = "new-after-restart".into();
        reopened
            .save_interaction(SessionKey::new(100, Some(7)), &item, None)
            .unwrap();
        let new = reopened
            .conversation_request_ids(100, entry, highest)
            .unwrap();
        assert_eq!(new.0.len(), 1);
        assert_eq!(new.0[0].1, item.id);
    }
    #[test]
    fn derived_helper_retirement_uses_the_same_lineage_as_request_admission() {
        use crate::interaction::{RequestRelation, ReturnRoute, SourceInput, TransportPhase};
        for deleted in [false, true] {
            let (_file, store, entry, worker, _) = fixture();
            let local = store
                .conversation_request(100, entry, "request-7")
                .unwrap()
                .item
                .local_turn;
            let source = SourceInput {
                transcription: None,
                channel: "local-session".into(),
                event_id: "derived-call".into(),
                update_id: None,
                human_id: 100,
                target_sessions: vec![store.get_session(worker).unwrap().unwrap().id],
                origin: ReturnRoute {
                    chat_id: entry.chat_id,
                    thread_id: entry.thread_id,
                    message_id: None,
                },
                text: "Derived helper request".into(),
            };
            let (instruction, _) = store
                .interaction_admit(&source, worker, RequestRelation::New)
                .unwrap();
            store
                .interaction_transport(
                    &instruction.id,
                    100,
                    local,
                    TransportPhase::Submitting,
                    None,
                )
                .unwrap();
            store
                .interaction_transport(
                    &instruction.id,
                    100,
                    local,
                    TransportPhase::Accepted,
                    Some("worker-turn"),
                )
                .unwrap();
            store.conn.lock().unwrap().execute("UPDATE conversation_returns SET source_input='derived-call' WHERE native_thread='worker-native'",[]).unwrap();
            let original = store.conversation_request(100, entry, "request-7").unwrap();
            if deleted {
                let id = store.get_session(entry).unwrap().unwrap().id;
                store
                    .conn
                    .lock()
                    .unwrap()
                    .execute("DELETE FROM sessions WHERE id=?1", [id])
                    .unwrap();
                store
                    .ensure_session(entry, 100, &super::super::tests::defaults())
                    .unwrap();
                store
                    .set_session_codex_thread(entry, "entry-native")
                    .unwrap();
            } else {
                store
                    .set_session_codex_thread(entry, "replacement-entry")
                    .unwrap();
                store
                    .set_session_codex_thread(entry, "entry-native")
                    .unwrap();
            }
            let fresh = store.conversation_request(100, entry, "request-7").unwrap();
            assert_eq!(fresh.item.state, "expired");
            assert!(
                record(
                    &store,
                    entry,
                    &original,
                    &"c".repeat(64),
                    &json!({"kind":"cancel"}),
                    "cancelled",
                    None
                )
                .is_err()
            );
        }
    }
    #[test]
    fn receipt_outlives_live_turn_and_session_deletion_without_restoring_a_responder() {
        let (_file, store, entry, worker, _) = fixture();
        let current = store.conversation_request(100, entry, "request-7").unwrap();
        let revision = "e".repeat(64);
        assert!(
            record(
                &store,
                entry,
                &current,
                &revision,
                &json!({"kind":"option","index":0}),
                "answering",
                None
            )
            .unwrap()
        );
        store
            .conn
            .lock()
            .unwrap()
            .execute("DELETE FROM turns WHERE id=?1", [current.item.local_turn])
            .unwrap();
        let worker_id = store.get_session(worker).unwrap().unwrap().id;
        store
            .conn
            .lock()
            .unwrap()
            .execute("DELETE FROM sessions WHERE id=?1", [worker_id])
            .unwrap();
        assert!(store.conversation_request(100, entry, "request-7").is_err());
        let receipt = store
            .conversation_decision_receipt(100, entry, "request-7", &revision)
            .unwrap()
            .unwrap();
        assert_eq!(receipt["resolved"]["answer"], "Selected exact option");
        assert_eq!(receipt["proposal"]["question_index"], 0);
        assert!(
            store
                .conversation_decision_receipt(200, entry, "request-7", &revision)
                .is_err()
        );
    }
    #[test]
    fn maintenance_and_listener_restart_do_not_recreate_request_authority() {
        let (file, store, entry, _, _) = fixture();
        let current = store.conversation_request(100, entry, "request-7").unwrap();
        let action = json!({"kind":"cancel"});
        let revision = "f".repeat(64);
        store
            .conn
            .lock()
            .unwrap()
            .execute(
                "INSERT INTO bot_state(key,value) VALUES('native_writer_hold:100:7','fixture')",
                [],
            )
            .unwrap();
        assert!(
            record(
                &store,
                entry,
                &current,
                &revision,
                &action,
                "cancelled",
                None
            )
            .is_err()
        );
        store
            .conn
            .lock()
            .unwrap()
            .execute(
                "DELETE FROM bot_state WHERE key='native_writer_hold:100:7'",
                [],
            )
            .unwrap();
        drop(store);
        let reopened =
            Store::open(file.path(), &[100, 200], &super::super::tests::defaults()).unwrap();
        assert_eq!(
            reopened
                .conversation_request(100, entry, "request-7")
                .unwrap()
                .item
                .state,
            "expired"
        );
        assert!(
            record(
                &reopened,
                entry,
                &current,
                &revision,
                &action,
                "cancelled",
                None
            )
            .is_err()
        );
    }
}
