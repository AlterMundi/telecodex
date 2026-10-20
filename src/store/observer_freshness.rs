//! Transaction-local activity invalidation; no inference, polling or new event bus.
use super::*;
use crate::store::observer_cards::ObserverCard;

pub struct ObserverFreshness {
    pub locally_reconciled: bool,
    pub state: &'static str,
    pub observed_cut: Option<i64>,
    pub current_cut: i64,
    pub checked_at: Option<String>,
    pub unsettled_inputs: usize,
}

impl ObserverFreshness {
    /// A local equality receipt has a bounded reuse age, never a promise that
    /// external native writers cannot have advanced the sampled history.
    pub fn needs_refresh(&self, now: chrono::DateTime<chrono::Utc>, max_age_seconds: i64) -> bool {
        if !self.locally_reconciled {
            return true;
        }
        let Some(checked) = self
            .checked_at
            .as_deref()
            .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
        else {
            return true;
        };
        let age = now.signed_duration_since(checked);
        age < chrono::Duration::zero() || age >= chrono::Duration::seconds(max_age_seconds.max(0))
    }
}

pub(super) fn cut_on(conn: &Connection, session: i64, native: &str) -> Result<i64> {
    Ok(conn.query_row("SELECT revision FROM interaction_observer_activity WHERE session_id=?1 AND native_thread=?2",params![session,native],|r|r.get(0)).optional()?.unwrap_or(0))
}

pub(super) fn checked_on(
    conn: &Connection,
    session: i64,
    native: &str,
    source: &str,
    cut: Option<i64>,
    checked_at: Option<&str>,
    instance: &str,
) -> Result<()> {
    if let (Some(cut), Some(checked_at)) = (
        cut.filter(|n| *n >= 0),
        checked_at.filter(|t| chrono::DateTime::parse_from_rfc3339(t).is_ok()),
    ) {
        conn.execute("INSERT INTO interaction_observer_checks(session_id,native_thread,source_revision,activity_cut,checked_at,instance_id) VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(session_id,native_thread,source_revision) DO UPDATE SET activity_cut=excluded.activity_cut,checked_at=excluded.checked_at,instance_id=excluded.instance_id",
            params![session,native,source,cut,checked_at,instance])?;
    }
    Ok(())
}

pub(super) fn freshness_on(
    conn: &Connection,
    session: i64,
    native: &str,
    source: &str,
    instance: &str,
) -> Result<ObserverFreshness> {
    let current_cut = cut_on(conn, session, native)?;
    let receipt:Option<(i64,String,String)>=conn.query_row("SELECT activity_cut,checked_at,instance_id FROM interaction_observer_checks WHERE session_id=?1 AND native_thread=?2 AND source_revision=?3",params![session,native,source],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
    // An external-only native change may have been read without changing the
    // local activity counter. A later unsettled source must not be hidden by
    // the older card's recent local-equality receipt.
    let newer_source_unsettled: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM interaction_observer_attempts a WHERE a.session_id=?1 AND a.native_thread=?2 AND a.source_revision!=?3 AND a.phase IN ('summarizing','uncertain') AND a.rowid>COALESCE((SELECT MAX(original.rowid) FROM interaction_observer_attempts original JOIN interaction_observer_cards c ON c.attempt_id=original.id WHERE c.session_id=?1 AND c.native_thread=?2 AND original.source_revision=?3),0))",
        params![session,native,source], |r|r.get(0))?;
    let state = match &receipt {
        None => "not checked against local activity",
        Some((_, _, old)) if old != instance => "native reconciliation required after restart",
        Some((cut, _, _)) if *cut != current_cut => "new local activity since source read",
        Some(_) if newer_source_unsettled => {
            "newer native source synthesis is pending or uncertain"
        }
        Some(_) => "no local change since source read; external history remains sampled",
    };
    let unsettled_inputs=conn.query_row("SELECT COUNT(*) FROM interaction_instructions WHERE session_id=?1 AND phase IN ('queued','submitting','steering','uncertain','retained_unsent') AND (json_extract(doc,'$.native_thread') IS ?2 OR json_extract(doc,'$.native_thread') IS NULL)",params![session,native],|r|r.get(0))?;
    Ok(ObserverFreshness {
        locally_reconciled: receipt.as_ref().is_some_and(|(cut, _, old)| {
            *cut == current_cut && old == instance && !newer_source_unsettled
        }),
        state,
        observed_cut: receipt.as_ref().map(|r| r.0),
        current_cut,
        checked_at: receipt.map(|r| r.1),
        unsettled_inputs,
    })
}

impl Store {
    pub(super) fn init_observer_freshness(&self) -> Result<()> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        conn.execute_batch("CREATE TABLE IF NOT EXISTS interaction_observer_activity(
            session_id INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,native_thread TEXT NOT NULL,
            revision INTEGER NOT NULL,PRIMARY KEY(session_id,native_thread));
            CREATE TABLE IF NOT EXISTS interaction_observer_checks(
            session_id INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,native_thread TEXT NOT NULL,
            source_revision TEXT NOT NULL,activity_cut INTEGER NOT NULL,checked_at TEXT NOT NULL,instance_id TEXT NOT NULL,
            PRIMARY KEY(session_id,native_thread,source_revision));")?;
        // Static owning-table triggers only maintain a derived counter. Each
        // bump commits or rolls back with its original evidence transaction.
        for (name, table, event, condition, session, native) in [
            (
                "message_insert",
                "interaction_native_messages",
                "AFTER INSERT",
                "",
                "(SELECT session_id FROM turns WHERE id=NEW.local_turn)",
                "NEW.native_thread",
            ),
            (
                "message_delete",
                "interaction_native_messages",
                "BEFORE DELETE",
                "",
                "(SELECT session_id FROM turns WHERE id=OLD.local_turn)",
                "OLD.native_thread",
            ),
            (
                "input_insert",
                "interaction_instructions",
                "AFTER INSERT",
                "",
                "NEW.session_id",
                "s.codex_thread_id",
            ),
            (
                "input_update",
                "interaction_instructions",
                "AFTER UPDATE",
                "WHEN NEW.phase IS NOT OLD.phase OR NEW.doc IS NOT OLD.doc",
                "NEW.session_id",
                "s.codex_thread_id",
            ),
            (
                "turn_insert",
                "turns",
                "AFTER INSERT",
                "",
                "NEW.session_id",
                "COALESCE(NEW.native_thread_id,s.codex_thread_id)",
            ),
            (
                "turn_update",
                "turns",
                "AFTER UPDATE",
                "WHEN NEW.status IS NOT OLD.status OR NEW.native_thread_id IS NOT OLD.native_thread_id OR NEW.prompt IS NOT OLD.prompt OR NEW.assistant_text IS NOT OLD.assistant_text",
                "NEW.session_id",
                "COALESCE(NEW.native_thread_id,s.codex_thread_id)",
            ),
            (
                "turn_delete",
                "turns",
                "BEFORE DELETE",
                "",
                "OLD.session_id",
                "COALESCE(OLD.native_thread_id,s.codex_thread_id)",
            ),
            (
                "pending_insert",
                "human_interactions",
                "AFTER INSERT",
                "",
                "NEW.session_id",
                "NEW.native_thread",
            ),
            (
                "pending_update",
                "human_interactions",
                "AFTER UPDATE",
                "WHEN NEW.state IS NOT OLD.state OR NEW.payload_json IS NOT OLD.payload_json",
                "NEW.session_id",
                "NEW.native_thread",
            ),
            (
                "pending_delete",
                "human_interactions",
                "BEFORE DELETE",
                "",
                "OLD.session_id",
                "OLD.native_thread",
            ),
        ] {
            conn.execute_batch(&format!("CREATE TRIGGER IF NOT EXISTS interaction_observer_{name} {event} ON {table} {condition} BEGIN
                INSERT INTO interaction_observer_activity(session_id,native_thread,revision)
                SELECT s.id,{native},1 FROM sessions s WHERE s.id={session} AND {native} IS NOT NULL
                ON CONFLICT(session_id,native_thread) DO UPDATE SET revision=revision+1;
                END;"))?;
        }
        Ok(())
    }

    /// Capture before the native read. Activity arriving during the read or
    /// synthesis must leave the resulting card visibly stale.
    pub fn observer_activity_cut(&self, key: SessionKey, human: i64, native: &str) -> Result<i64> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let session = observer_cards::binding(&conn, key, human, native)?;
        cut_on(&conn, session, native)
    }

    /// Native text history omits structured question/answer RPCs. Preserve their
    /// scoped journal evidence beside the matching original turn, without
    /// equating a recorded answer with native incorporation or completion.
    pub fn observer_question_evidence(
        &self,
        key: SessionKey,
        human: i64,
        source: &mut serde_json::Value,
    ) -> Result<()> {
        let native = source["native"]
            .as_str()
            .context("Observer source missing native")?
            .to_owned();
        let conn = self.conn.lock().expect("store mutex poisoned");
        let session = observer_cards::binding(&conn, key, human, &native)?;
        let mut stmt = conn.prepare("SELECT h.id,h.state,h.payload_json,d.doc FROM human_interactions h LEFT JOIN conversation_decisions d ON d.request_id=h.id AND d.human_id=h.requester WHERE h.session_id=?1 AND h.requester=?2 AND h.native_thread=?3 AND h.kind='native' ORDER BY h.created_at DESC,h.id,d.created_at LIMIT 65")?;
        let rows = stmt
            .query_map(params![session, human, native], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, Option<String>>(3)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut remaining = 8000usize;
        if rows.len() > 64 {
            source["bounded"] = serde_json::json!(true);
        }
        for (id, state, payload, decision) in rows.into_iter().take(64) {
            let item: serde_json::Value = serde_json::from_str(&payload)?;
            if item["source"]["kind"] != "question" {
                continue;
            }
            let Some(turn) = item["source"]["turnId"].as_str() else {
                continue;
            };
            let prefix = format!("[Native source turn {turn};");
            let receipt = decision
                .as_deref()
                .map(serde_json::from_str::<serde_json::Value>)
                .transpose()?;
            let secret = item["source"]["request"]["questions"]
                .as_array()
                .is_some_and(|qs| qs.iter().any(|q| q["isSecret"] == true));
            let evidence = serde_json::json!({"request":id,"state":state,"question":item["action"],
                "recorded_answer":if secret {serde_json::json!("[Secret response omitted]")}else{receipt.as_ref().map(|r|r["resolved"].clone()).unwrap_or(serde_json::Value::Null)},
                "recorded_at":receipt.as_ref().map(|r|r["recorded_at"].clone()),
                "meaning":"Local structured request journal, not new human speech. Recorded answer is distinct from native incorporation or task completion. An answered request is not an unanswered question; unknown/cancelled/withdrawn outcomes are not answers."});
            let text = format!(
                "\n[Structured question evidence] {}",
                serde_json::to_string(&evidence)?
            );
            if text.len() > remaining {
                source["bounded"] = serde_json::json!(true);
                continue;
            }
            for field in ["entries", "in_progress_entries"] {
                if let Some(entries) = source[field].as_array_mut() {
                    if let Some(entry) = entries
                        .iter_mut()
                        .find(|e| e.as_str().is_some_and(|v| v.starts_with(&prefix)))
                    {
                        *entry = serde_json::json!(entry.as_str().unwrap().to_owned() + &text);
                        remaining -= text.len();
                        break;
                    }
                }
            }
        }
        // Answer-only changes must invalidate an otherwise unchanged native cut.
        source["revision"] = serde_json::json!(
            Uuid::new_v5(
                &Uuid::NAMESPACE_OID,
                serde_json::to_string(&serde_json::json!([
                    source["revision"],
                    source["entries"],
                    source["in_progress_entries"],
                    source["bounded"]
                ]))?
                .as_bytes()
            )
            .to_string()
        );
        Ok(())
    }

    pub fn observer_freshness(
        &self,
        key: SessionKey,
        human: i64,
        card: &ObserverCard,
    ) -> Result<ObserverFreshness> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let session = observer_cards::binding(&conn, key, human, &card.native)?;
        freshness_on(
            &conn,
            session,
            &card.native,
            &card.source_revision,
            &self.instance_id,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        interaction::{RequestRelation, ReturnRoute, SourceInput},
        store::{HumanInteraction, ObserverPreparation},
    };
    use serde_json::{Value, json};

    fn source(cut: i64, revision: &str) -> Value {
        json!({"native":"native-source","revision":revision,"boundary":"physical","turn_ids":["physical"],
            "entries":["[Native source turn physical] Human request and attributed assistant statement"],
            "in_progress_entries":[],"bounded":false,"text_truncated":false,"observer_activity_cut":cut,"observer_checked_at":"2026-10-16T01:02:03+00:00"})
    }

    #[test]
    fn structured_answers_change_observer_source_without_claiming_incorporation() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let defaults = super::super::tests::defaults();
        let store = Store::open(file.path(), &[100], &defaults).unwrap();
        let key = SessionKey::new(100, Some(7));
        let session = store.ensure_session(key, 100, &defaults).unwrap();
        store
            .set_session_codex_thread(key, "native-source")
            .unwrap();
        let turn = store
            .record_turn_started(
                session.id,
                &TurnRequest {
                    session_key: key,
                    from_user_id: 100,
                    prompt: "Request".into(),
                    runtime_instructions: None,
                    attachments: vec![],
                    review_mode: None,
                    override_search_mode: None,
                },
            )
            .unwrap();
        let item = HumanInteraction {
            id: "question".into(),
            kind: "native".into(),
            state: "pending".into(),
            requester: 100,
            native_thread: "native-source".into(),
            local_turn: turn,
            title: "Format".into(),
            action: "Table or chart?".into(),
            blocking: true,
            source: json!({"kind":"question","turnId":"physical","request":{"questions":[{"isSecret":false}]}}),
        };
        store.save_interaction(key, &item, None).unwrap();
        let base = json!({"native":"native-source","revision":"native-cut","boundary":"physical","turn_ids":["physical"],
            "entries":["[Native source turn physical; native status completed; completed]\nassistant: Done"],"in_progress_entries":[],"bounded":false});
        let mut pending = base.clone();
        store
            .observer_question_evidence(key, 100, &mut pending)
            .unwrap();
        assert!(pending["entries"][0].as_str().unwrap().contains("pending"));
        {
            let conn = store.conn.lock().unwrap();
            conn.execute(
                "UPDATE human_interactions SET state='answered' WHERE id='question'",
                [],
            )
            .unwrap();
            let receipt = json!({"resolved":{"answer":"Original color sums"},"recorded_at":"2026-10-20T10:58:39Z"});
            conn.execute("INSERT INTO conversation_decisions(human_id,entry_session,entry_chat,entry_topic,request_id,revision,doc,created_at) VALUES(100,?1,100,7,'question','choice',?2,'now')",params![session.id,receipt.to_string()]).unwrap();
        }
        let mut answered = base.clone();
        store
            .observer_question_evidence(key, 100, &mut answered)
            .unwrap();
        let text = answered["entries"][0].as_str().unwrap();
        assert!(
            text.contains("Original color sums")
                && text.contains("answered")
                && text.contains("distinct from native incorporation")
        );
        assert_ne!(pending["revision"], answered["revision"]);
        assert_eq!(answered["turn_ids"], base["turn_ids"]);
        let mut repeat = base.clone();
        store
            .observer_question_evidence(key, 100, &mut repeat)
            .unwrap();
        assert_eq!(answered, repeat);
        assert!(
            store
                .observer_question_evidence(key, 101, &mut base.clone())
                .is_err()
        );
        let mut foreign = base.clone();
        foreign["native"] = json!("other-native");
        assert!(
            store
                .observer_question_evidence(key, 100, &mut foreign)
                .is_err()
        );
        let mut omitted = base.clone();
        omitted["entries"][0] = json!("[Native source turn different; completed] unrelated");
        store
            .observer_question_evidence(key, 100, &mut omitted)
            .unwrap();
        assert!(!omitted.to_string().contains("Original color sums"));
        let mut secret = item.clone();
        secret.source["request"]["questions"][0]["isSecret"] = json!(true);
        {
            let conn = store.conn.lock().unwrap();
            conn.execute(
                "UPDATE human_interactions SET payload_json=?1 WHERE id='question'",
                [serde_json::to_string(&secret).unwrap()],
            )
            .unwrap();
        }
        let mut redacted = base;
        store
            .observer_question_evidence(key, 100, &mut redacted)
            .unwrap();
        assert!(!redacted.to_string().contains("Original color sums"));
        assert!(redacted.to_string().contains("Secret response omitted"));
    }

    #[test]
    fn newer_external_source_is_not_hidden_by_a_recent_local_receipt() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let defaults = super::super::tests::defaults();
        let store = Store::open(file.path(), &[100], &defaults).unwrap();
        let key = SessionKey::new(100, Some(7));
        store.ensure_session(key, 100, &defaults).unwrap();
        store
            .set_session_codex_thread(key, "native-source")
            .unwrap();
        let ObserverPreparation::Ready(first) = store
            .observer_prepare(key, 100, &source(0, "one"), 0, false)
            .unwrap()
        else {
            panic!()
        };
        let card = store
            .observer_commit(key, 100, &first, "First interpretation", &[0])
            .unwrap();
        let read_at = chrono::DateTime::parse_from_rfc3339(&card.observed_at)
            .unwrap()
            .with_timezone(&chrono::Utc);
        assert!(
            !store
                .observer_freshness(key, 100, &card)
                .unwrap()
                .needs_refresh(read_at, 60)
        );
        let mut changed = source(0, "two");
        changed["entries"][0] = json!("An external native writer corrected the decision.");
        let ObserverPreparation::Ready(second) = store
            .observer_prepare(key, 100, &changed, 1, false)
            .unwrap()
        else {
            panic!()
        };
        for uncertain in [false, true] {
            if uncertain {
                store.observer_uncertain(&second.id).unwrap();
            }
            let freshness = store.observer_freshness(key, 100, &card).unwrap();
            assert_eq!(freshness.current_cut, 0);
            assert!(freshness.state.contains("pending or uncertain"));
            assert!(freshness.needs_refresh(read_at, 60));
        }
        assert!(
            matches!(store.observer_prepare(key,100,&changed,1,false).unwrap(),ObserverPreparation::Retained(phase) if phase=="uncertain")
        );
        let mut superseding = source(0, "three");
        superseding["entries"][0] = json!("A later source includes and resolves the correction.");
        let ObserverPreparation::Ready(third) = store
            .observer_prepare(key, 100, &superseding, 1, false)
            .unwrap()
        else {
            panic!()
        };
        let current = store
            .observer_commit(key, 100, &third, "Later interpretation", &[0])
            .unwrap();
        assert!(
            !store
                .observer_freshness(key, 100, &current)
                .unwrap()
                .needs_refresh(read_at, 60)
        );
    }

    #[test]
    fn activity_is_atomic_idempotent_and_remains_stale_during_synthesis() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let defaults = super::super::tests::defaults();
        let store = Store::open(file.path(), &[100, 101], &defaults).unwrap();
        let key = SessionKey::new(100, Some(7));
        let session = store.ensure_session(key, 100, &defaults).unwrap();
        store
            .set_session_codex_thread(key, "native-source")
            .unwrap();
        let turn = store
            .record_turn_started(
                session.id,
                &TurnRequest {
                    session_key: key,
                    from_user_id: 100,
                    prompt: "Request".into(),
                    runtime_instructions: None,
                    attachments: vec![],
                    review_mode: None,
                    override_search_mode: None,
                },
            )
            .unwrap();
        store
            .record_turn_native_thread(turn, "native-source")
            .unwrap();
        store
            .record_native_message(
                turn,
                "native-source",
                "physical",
                "first",
                Some("commentary"),
                "Working",
            )
            .unwrap();
        let cut = store
            .observer_activity_cut(key, 100, "native-source")
            .unwrap();
        assert!(cut > 0);
        let ObserverPreparation::Ready(attempt) = store
            .observer_prepare(key, 100, &source(cut, "one"), 0, false)
            .unwrap()
        else {
            panic!()
        };
        let card = store
            .observer_commit(key, 100, &attempt, "Attributed interpretation", &[0])
            .unwrap();
        assert_eq!(card.observed_at, "2026-10-16T01:02:03+00:00");
        assert_ne!(card.committed_at, card.observed_at);
        let freshness = store.observer_freshness(key, 100, &card).unwrap();
        let read_at = chrono::DateTime::parse_from_rfc3339(&card.observed_at)
            .unwrap()
            .with_timezone(&chrono::Utc);
        assert!(!freshness.needs_refresh(read_at + chrono::Duration::seconds(59), 60));
        assert!(freshness.needs_refresh(read_at + chrono::Duration::seconds(60), 60));
        assert!(freshness.needs_refresh(read_at - chrono::Duration::seconds(1), 60));
        assert_eq!(
            store
                .observer_freshness(key, 100, &card)
                .unwrap()
                .checked_at
                .as_deref(),
            Some(card.observed_at.as_str())
        );
        assert!(
            store
                .observer_freshness(key, 100, &card)
                .unwrap()
                .state
                .starts_with("no local change")
        );
        store
            .record_native_message(
                turn,
                "native-source",
                "physical",
                "first",
                Some("commentary"),
                "Working",
            )
            .unwrap();
        assert_eq!(
            store
                .observer_activity_cut(key, 100, "native-source")
                .unwrap(),
            cut
        );
        {
            let mut conn = store.conn.lock().unwrap();
            let tx = conn.transaction().unwrap();
            tx.execute("INSERT INTO interaction_native_messages(native_thread,native_turn,item_id,local_turn,phase,text,observed_at) VALUES('native-source','physical','rolled-back',?1,'commentary','Never committed',?2)",params![turn,now_string()]).unwrap();
            tx.rollback().unwrap();
        }
        assert_eq!(
            store
                .observer_activity_cut(key, 100, "native-source")
                .unwrap(),
            cut
        );
        let task = HumanInteraction {
            id: "task".into(),
            kind: "task".into(),
            state: "pending".into(),
            requester: 100,
            native_thread: "native-source".into(),
            local_turn: turn,
            title: "External observation".into(),
            action: "Observe".into(),
            blocking: false,
            source: json!({}),
        };
        store.save_interaction(key, &task, Some("task")).unwrap();
        assert!(
            store
                .observer_freshness(key, 100, &card)
                .unwrap()
                .state
                .starts_with("new local activity")
        );
        let newer = store
            .observer_activity_cut(key, 100, "native-source")
            .unwrap();
        assert!(matches!(
            store
                .observer_prepare(key, 100, &source(newer, "one"), 1, false)
                .unwrap(),
            ObserverPreparation::Unchanged(_)
        ));
        assert!(
            store
                .observer_freshness(key, 100, &card)
                .unwrap()
                .state
                .starts_with("no local change")
        );
        let ObserverPreparation::Ready(attempt) = store
            .observer_prepare(key, 100, &source(newer, "two"), 1, false)
            .unwrap()
        else {
            panic!()
        };
        store
            .record_native_message(
                turn,
                "native-source",
                "physical",
                "later",
                Some("commentary"),
                "New activity while synthesizing",
            )
            .unwrap();
        let card = store
            .observer_commit(key, 100, &attempt, "Older source interpretation", &[0])
            .unwrap();
        assert!(
            store
                .observer_freshness(key, 100, &card)
                .unwrap()
                .state
                .starts_with("new local activity")
        );
        let admitted = store
            .interaction_admit(
                &SourceInput {
                    transcription: None,
                    channel: "fixture".into(),
                    event_id: "1".into(),
                    update_id: Some(1),
                    human_id: 100,
                    target_sessions: vec![session.id],
                    origin: ReturnRoute {
                        chat_id: 100,
                        thread_id: 1,
                        message_id: Some(1),
                    },
                    text: "Queued correction".into(),
                },
                key,
                RequestRelation::New,
            )
            .unwrap()
            .0;
        assert_eq!(
            store
                .observer_freshness(key, 100, &card)
                .unwrap()
                .unsettled_inputs,
            1
        );
        assert!(
            store
                .observer_activity_cut(key, 101, "native-source")
                .is_err()
        );
        assert!(store.observer_freshness(key, 101, &card).is_err());
        assert!(!admitted.id.is_empty());
        store
            .record_turn_finished(turn, "completed", Some("Done"))
            .unwrap();
        drop(store);
        let store = Store::open(file.path(), &[100, 101], &defaults).unwrap();
        let card = store
            .observer_card(key, 100, "native-source")
            .unwrap()
            .unwrap();
        assert_eq!(
            store.observer_freshness(key, 100, &card).unwrap().state,
            "native reconciliation required after restart"
        );
        let check = store
            .observer_activity_cut(key, 100, "native-source")
            .unwrap();
        assert!(matches!(
            store
                .observer_prepare(key, 100, &source(check, "two"), 2, false)
                .unwrap(),
            ObserverPreparation::Unchanged(_)
        ));
        assert!(
            store
                .observer_freshness(key, 100, &card)
                .unwrap()
                .state
                .starts_with("no local change")
        );
        let count: i64 = store
            .conn
            .lock()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM interaction_observer_cards", [], |r| {
                r.get(0)
            })
            .unwrap();
        store.delete_session(key).unwrap();
        let retained: i64 = store
            .conn
            .lock()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM interaction_observer_cards", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(retained, count);
        let replacement = store.ensure_session(key, 100, &defaults).unwrap();
        assert_ne!(replacement.id, session.id);
        store
            .set_session_codex_thread(key, "native-source")
            .unwrap();
        assert!(
            store
                .observer_card(key, 100, "native-source")
                .unwrap()
                .is_none()
        );
    }
}
