//! Scoped interaction records never resurrect a native response channel.

use super::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HumanInteraction {
    pub id: String,
    pub kind: String,
    pub state: String,
    pub requester: i64,
    pub native_thread: String,
    pub local_turn: i64,
    pub title: String,
    pub action: String,
    pub blocking: bool,
    pub source: serde_json::Value,
}

pub struct InteractionPage {
    pub items: Vec<HumanInteraction>,
    pub actionable_total: usize,
    pub pending_total: usize,
    pub history_total: usize,
}
#[derive(Debug, Clone)]
pub struct TaskWithdrawal {
    pub id: String,
    pub reason: String,
    pub links: serde_json::Value,
}

fn projected(
    mut item: HumanInteraction,
    state: String,
    current: Option<String>,
) -> HumanInteraction {
    item.state = if matches!(state.as_str(), "pending" | "answering" | "unknown")
        && current.as_deref() != Some(item.native_thread.as_str())
    {
        "source_changed".into()
    } else if state == "cancelled" && item.source["agentWithdrawal"].is_object() {
        "withdrawn".into()
    } else {
        state
    };
    item
}

// Reused by the observer so pending state and source cuts share its read snapshot.
pub(super) fn interaction_page_on(
    conn: &Connection,
    key: SessionKey,
    user: i64,
    offset: usize,
    history: bool,
) -> Result<InteractionPage> {
    let active =
        "h.state IN ('pending','answering','unknown') AND h.native_thread=s.codex_thread_id";
    let scope = "FROM human_interactions h JOIN sessions s ON s.id=h.session_id JOIN users u ON u.tg_user_id=h.requester WHERE s.chat_id=?1 AND s.thread_id=?2 AND h.requester=?3 AND u.allowed=1";
    let (actionable_total,pending_total,total):(usize,usize,usize)=conn.query_row(&format!("SELECT COALESCE(SUM(CASE WHEN {active} THEN 1 ELSE 0 END),0),COALESCE(SUM(CASE WHEN h.state='pending' AND h.native_thread=s.codex_thread_id THEN 1 ELSE 0 END),0),COUNT(*) {scope}"),params![key.chat_id,key.thread_id,user],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
    let items = {
        let query = format!(
            "SELECT h.payload_json,h.state,s.codex_thread_id {scope} AND (CASE WHEN {active} THEN 1 ELSE 0 END)=?5 ORDER BY CASE WHEN h.state='pending' THEN 0 ELSE 1 END,h.created_at DESC,h.id DESC LIMIT 51 OFFSET ?4"
        );
        let mut stmt = conn.prepare(&query)?;
        stmt.query_map(
            params![
                key.chat_id,
                key.thread_id,
                user,
                offset as i64,
                i64::from(!history)
            ],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, Option<String>>(2)?,
                ))
            },
        )?
        .map(|row| {
            let (raw, state, current) = row?;
            Ok(projected(serde_json::from_str(&raw)?, state, current))
        })
        .collect::<Result<Vec<_>>>()?
    };
    Ok(InteractionPage {
        items,
        actionable_total,
        pending_total,
        history_total: total - actionable_total,
    })
}

impl Store {
    pub(super) fn init_interactions(&self) -> Result<()> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        conn.execute_batch("CREATE TABLE IF NOT EXISTS human_interactions(
            id TEXT PRIMARY KEY, session_id INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
            requester INTEGER NOT NULL REFERENCES users(tg_user_id), native_thread TEXT NOT NULL,
            local_turn INTEGER NOT NULL REFERENCES turns(id) ON DELETE CASCADE, kind TEXT NOT NULL CHECK(kind IN ('native','task')),
            state TEXT NOT NULL CHECK(state IN ('pending','answering','answered','completed','cancelled','expired','unknown')),
            instance_id TEXT NOT NULL, client_id TEXT,
            payload_json TEXT NOT NULL, created_at TEXT NOT NULL, updated_at TEXT NOT NULL,
            UNIQUE(local_turn,kind,client_id));
            CREATE INDEX IF NOT EXISTS human_interactions_scope
            ON human_interactions(session_id,requester,state);
            CREATE TABLE IF NOT EXISTS human_interaction_messages(
            chat_id INTEGER NOT NULL, thread_id INTEGER NOT NULL, message_id INTEGER NOT NULL,
            interaction_id TEXT NOT NULL REFERENCES human_interactions(id) ON DELETE CASCADE,
            PRIMARY KEY(chat_id,thread_id,message_id));")?;
        conn.execute("UPDATE human_interactions SET state=CASE WHEN state='pending' THEN 'expired' ELSE 'unknown' END,
            updated_at=?1 WHERE kind='native' AND state IN ('pending','answering') AND instance_id!=?2",
            params![now_string(),self.instance_id])?;
        Ok(())
    }

    pub fn save_interaction(
        &self,
        key: SessionKey,
        item: &HumanInteraction,
        client_id: Option<&str>,
    ) -> Result<()> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let session: Option<i64> = conn.query_row("SELECT s.id FROM sessions s JOIN users u
            ON u.tg_user_id=?3 JOIN turns t ON t.session_id=s.id AND t.id=?5
            WHERE s.chat_id=?1 AND s.thread_id=?2 AND s.codex_thread_id=?4 AND u.allowed=1 AND t.from_user_id=?3",
            params![key.chat_id,key.thread_id,item.requester,item.native_thread,item.local_turn],
            |r| r.get(0)).optional()?;
        let session = session.context("human interaction has no authorized source binding")?;
        conn.execute(
            "INSERT INTO human_interactions(id,session_id,requester,native_thread,local_turn,
            kind,state,instance_id,client_id,payload_json,created_at,updated_at)
            VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?11)",
            params![
                item.id,
                session,
                item.requester,
                item.native_thread,
                item.local_turn,
                item.kind,
                item.state,
                self.instance_id,
                client_id,
                serde_json::to_string(item)?,
                now_string()
            ],
        )?;
        Ok(())
    }

    #[cfg(test)]
    pub fn interactions(&self, key: SessionKey, user: i64) -> Result<Vec<HumanInteraction>> {
        let mut pending = self.interactions_page(key, user, 0, false)?.items;
        pending.extend(self.interactions_page(key, user, 0, true)?.items);
        Ok(pending)
    }

    pub fn interactions_page(
        &self,
        key: SessionKey,
        user: i64,
        offset: usize,
        history: bool,
    ) -> Result<InteractionPage> {
        // Counts and page share a read snapshot, including concurrent owner-local maintenance.
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction()?;
        let page = interaction_page_on(&tx, key, user, offset, history)?;
        tx.commit()?;
        Ok(page)
    }

    pub fn interaction(
        &self,
        key: SessionKey,
        user: i64,
        id: &str,
    ) -> Result<Option<HumanInteraction>> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let row = conn
            .query_row(
                "SELECT h.payload_json,h.state,s.codex_thread_id FROM human_interactions h
            JOIN sessions s ON s.id=h.session_id JOIN users u ON u.tg_user_id=h.requester
            WHERE s.chat_id=?1 AND s.thread_id=?2 AND h.requester=?3 AND h.id=?4 AND u.allowed=1",
                params![key.chat_id, key.thread_id, user, id],
                |r| {
                    Ok((
                        r.get::<_, String>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, Option<String>>(2)?,
                    ))
                },
            )
            .optional()?;
        row.map(|(payload, state, current)| {
            let mut item = projected(serde_json::from_str(&payload)?, state, current);
            if let Some(update) = item.source["answerUpdateId"].as_i64() {
                let delivery: Option<String> = conn
                    .query_row(
                        "SELECT status FROM incoming_updates WHERE update_id=?1",
                        [update],
                        |r| r.get(0),
                    )
                    .optional()?;
                item.source["delivery"] =
                    serde_json::json!(delivery.unwrap_or_else(|| "unknown".into()));
            }
            Ok(item)
        })
        .transpose()
    }

    #[cfg(test)]
    pub fn save_human_tasks(
        &self,
        key: SessionKey,
        tasks: &[(String, HumanInteraction)],
    ) -> Result<()> {
        self.save_human_requests(key, tasks, None)
    }
    pub fn save_human_requests(
        &self,
        key: SessionKey,
        tasks: &[(String, HumanInteraction)],
        withdrawal: Option<(i64, &str, i64, &[TaskWithdrawal])>,
    ) -> Result<()> {
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction()?;
        for (client, item) in tasks {
            let session: Option<i64> = tx
                .query_row(
                    "SELECT s.id FROM sessions s JOIN users u ON u.tg_user_id=?3
                JOIN turns t ON t.session_id=s.id AND t.id=?5 WHERE s.chat_id=?1 AND s.thread_id=?2
                AND s.codex_thread_id=?4 AND u.allowed=1 AND t.from_user_id=?3",
                    params![
                        key.chat_id,
                        key.thread_id,
                        item.requester,
                        item.native_thread,
                        item.local_turn
                    ],
                    |r| r.get(0),
                )
                .optional()?;
            let session = session.context("human task has no authorized source binding")?;
            tx.execute(
                "INSERT INTO human_interactions(id,session_id,requester,native_thread,local_turn,
                kind,state,instance_id,client_id,payload_json,created_at,updated_at)
                VALUES(?1,?2,?3,?4,?5,'task','pending',?6,?7,?8,?9,?9)",
                params![
                    item.id,
                    session,
                    item.requester,
                    item.native_thread,
                    item.local_turn,
                    self.instance_id,
                    client,
                    serde_json::to_string(item)?,
                    now_string()
                ],
            )?;
        }
        if let Some((user, native, turn, withdrawals)) = withdrawal {
            let session:Option<i64>=tx.query_row("SELECT s.id FROM sessions s JOIN users u ON u.tg_user_id=?3 JOIN turns t ON t.session_id=s.id AND t.id=?5 WHERE s.chat_id=?1 AND s.thread_id=?2 AND s.codex_thread_id=?4 AND u.allowed=1 AND t.from_user_id=?3",params![key.chat_id,key.thread_id,user,native,turn],|r|r.get(0)).optional()?;
            let session = session.context("withdrawal has no authorized current source binding")?;
            for entry in withdrawals {
                if entry.reason.trim().is_empty() || entry.reason.chars().count() > 2000 {
                    return Err(anyhow!("withdrawal needs a bounded explicit reason"));
                }
                let row:Option<(String,String)>=tx.query_row("SELECT state,payload_json FROM human_interactions WHERE id=?1 AND session_id=?2 AND requester=?3 AND native_thread=?4 AND kind='task'",params![entry.id,session,user,native],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
                let (state, raw) =
                    row.context("withdrawal target is not an owned task in this source session")?;
                let mut doc: serde_json::Value = serde_json::from_str(&raw)?;
                if state == "cancelled"
                    && doc["source"]["agentWithdrawal"]["reason"] == entry.reason
                    && doc["source"]["agentWithdrawal"]["links"] == entry.links
                {
                    continue;
                }
                if state != "pending" {
                    return Err(anyhow!(
                        "withdrawal target is already resolved or has an uncertain receipt"
                    ));
                }
                let at = now_string();
                doc["source"]["agentWithdrawal"] = serde_json::json!({"reason":entry.reason,"links":entry.links,"sourceTurn":turn,"at":at});
                tx.execute("UPDATE human_interactions SET state='cancelled',payload_json=?2,updated_at=?3 WHERE id=?1 AND state='pending'",params![entry.id,serde_json::to_string(&doc)?,at])?;
                tx.execute("INSERT INTO audit_log(actor_user_id,action,details_json,created_at) VALUES(NULL,'human_task_withdrawn_by_agent',?1,?2)",params![serde_json::json!({"interaction_id":entry.id,"requester":user,"source_turn":turn,"reason":entry.reason,"links":entry.links}).to_string(),at])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn update_native_interaction(
        &self,
        id: &str,
        title: &str,
        action: &str,
        index: usize,
    ) -> Result<()> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        conn.execute("UPDATE human_interactions SET payload_json=json_set(payload_json,'$.title',?2,'$.action',?3,'$.source.conversationIndex',?6),updated_at=?4
            WHERE id=?1 AND instance_id=?5 AND kind='native' AND state='pending'",
            params![id,title,action,now_string(),self.instance_id,index as i64])?;
        Ok(())
    }

    pub fn transition_interaction(
        &self,
        key: SessionKey,
        user: i64,
        id: &str,
        from: &str,
        to: &str,
    ) -> Result<bool> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        Ok(conn.execute("UPDATE human_interactions SET state=?5,updated_at=?6
            WHERE id=?4 AND requester=?3 AND state=?7 AND (kind='task' OR instance_id=?8) AND session_id IN
            (SELECT s.id FROM sessions s JOIN users u ON u.tg_user_id=?3
             WHERE s.chat_id=?1 AND s.thread_id=?2 AND u.allowed=1 AND s.codex_thread_id=human_interactions.native_thread)",
            params![key.chat_id,key.thread_id,user,id,to,now_string(),from,self.instance_id])?==1)
    }

    pub fn finish_native_interactions(
        &self,
        turn: i64,
        request: Option<&crate::user_input::NativeRequestId>,
        accepted: bool,
    ) -> Result<()> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let request = request.map(serde_json::to_string).transpose()?;
        conn.execute(
            "UPDATE human_interactions SET state=CASE
            WHEN ?3=1 AND state='answering' THEN 'answered'
            WHEN state='answering' THEN 'unknown' ELSE 'expired' END,updated_at=?4
            WHERE kind='native' AND local_turn=?1 AND state IN ('pending','answering')
            AND (?2 IS NULL OR json_extract(payload_json,'$.source.requestId')=?2)",
            params![turn, request, accepted, now_string()],
        )?;
        Ok(())
    }

    pub fn answer_human_task(
        &self,
        key: SessionKey,
        user: i64,
        id: &str,
        update: i64,
    ) -> Result<bool> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        Ok(conn.execute("UPDATE human_interactions SET state='answered',updated_at=?5,
            payload_json=json_set(payload_json,'$.source.answerUpdateId',?6)
            WHERE id=?4 AND kind='task' AND state='pending' AND requester=?3
            AND session_id IN (SELECT s.id FROM sessions s JOIN users u ON u.tg_user_id=?3
             WHERE s.chat_id=?1 AND s.thread_id=?2 AND u.allowed=1 AND s.codex_thread_id=human_interactions.native_thread)",
            params![key.chat_id,key.thread_id,user,id,now_string(),update])?==1)
    }

    pub fn record_interaction_message(
        &self,
        key: SessionKey,
        message: i64,
        id: &str,
    ) -> Result<()> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        conn.execute(
            "INSERT INTO human_interaction_messages VALUES(?1,?2,?3,?4)",
            params![key.chat_id, key.thread_id, message, id],
        )?;
        Ok(())
    }

    pub fn interaction_message(&self, key: SessionKey, message: i64) -> Result<Option<String>> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        conn.query_row("SELECT interaction_id FROM human_interaction_messages WHERE chat_id=?1 AND thread_id=?2 AND message_id=?3",
                       params![key.chat_id,key.thread_id,message],|r|r.get(0)).optional().map_err(Into::into)
    }
}

#[cfg(test)]
mod lifecycle_tests {
    use super::*;
    use serde_json::json;
    fn fixture() -> (tempfile::TempDir, Store, SessionKey, i64) {
        let root = tempfile::tempdir().unwrap();
        let defaults = SessionDefaults {
            cwd: root.path().into(),
            model: None,
            reasoning_effort: None,
            session_prompt: None,
            sandbox_mode: "read-only".into(),
            approval_policy: "never".into(),
            search_mode: crate::config::SearchMode::Disabled,
            add_dirs: vec![],
        };
        let store = Store::open(&root.path().join("db"), &[100, 101], &defaults).unwrap();
        let key = SessionKey::new(100, Some(7));
        let session = store.ensure_session(key, 100, &defaults).unwrap();
        store.set_session_codex_thread(key, "source").unwrap();
        let turn = store
            .record_turn_started(
                session.id,
                &TurnRequest {
                    session_key: key,
                    from_user_id: 100,
                    prompt: "Human-directed work".into(),
                    runtime_instructions: None,
                    attachments: vec![],
                    review_mode: None,
                    override_search_mode: None,
                },
            )
            .unwrap();
        (root, store, key, turn)
    }
    fn task(id: &str, turn: i64) -> HumanInteraction {
        HumanInteraction {
            id: id.into(),
            kind: "task".into(),
            state: "pending".into(),
            requester: 100,
            native_thread: "source".into(),
            local_turn: turn,
            title: "A long mobile title ".repeat(5),
            action: "Do the selected human action".into(),
            blocking: false,
            source: json!({"links":[]}),
        }
    }
    fn withdrawal(id: &str) -> TaskWithdrawal {
        TaskWithdrawal {
            id: id.into(),
            reason: "The selected handoff is already observed; no human approval is inferred"
                .into(),
            links: json!([{ "label":"Evidence","url":"https://example.org/receipt" }]),
        }
    }
    #[test]
    fn pages_have_topic_totals_and_separate_terminal_history() {
        let (_root, store, key, turn) = fixture();
        for n in 0..60 {
            let id = format!("task-{n:03}");
            store
                .save_human_tasks(key, &[(id.clone(), task(&id, turn))])
                .unwrap();
        }
        store
            .transition_interaction(key, 100, "task-000", "pending", "completed")
            .unwrap();
        store
            .transition_interaction(key, 100, "task-001", "pending", "cancelled")
            .unwrap();
        let mut unknown = task("native-unknown", turn);
        unknown.kind = "native".into();
        unknown.state = "unknown".into();
        store.save_interaction(key, &unknown, None).unwrap();
        let page = store.interactions_page(key, 100, 0, false).unwrap();
        assert_eq!(
            (
                page.actionable_total,
                page.pending_total,
                page.history_total,
                page.items.len()
            ),
            (59, 58, 2, 51)
        );
        assert_eq!(
            store
                .interactions_page(key, 100, 50, false)
                .unwrap()
                .items
                .len(),
            9
        );
        let history = store.interactions_page(key, 100, 0, true).unwrap();
        assert_eq!(history.items.len(), 2);
        assert!(
            history
                .items
                .iter()
                .all(|i| matches!(i.state.as_str(), "completed" | "cancelled"))
        );
        assert!(
            store
                .interactions_page(key, 101, 0, false)
                .unwrap()
                .items
                .is_empty()
        );
        assert!(
            store
                .interactions_page(SessionKey::new(100, Some(8)), 100, 0, true)
                .unwrap()
                .items
                .is_empty()
        );
        store.set_session_codex_thread(key, "replacement").unwrap();
        assert_eq!(
            store
                .interactions_page(key, 100, 0, false)
                .unwrap()
                .actionable_total,
            0
        );
        assert!(
            store
                .interactions_page(key, 100, 0, true)
                .unwrap()
                .items
                .iter()
                .any(|i| i.state == "source_changed")
        );
    }
    #[test]
    fn withdrawal_is_scoped_audited_idempotent_and_cannot_reopen() {
        let (_root, store, key, turn) = fixture();
        store
            .save_human_tasks(key, &[("old".into(), task("old", turn))])
            .unwrap();
        let entry = withdrawal("old");
        store
            .save_human_requests(
                key,
                &[],
                Some((100, "source", turn, std::slice::from_ref(&entry))),
            )
            .unwrap();
        let item = store.interaction(key, 100, "old").unwrap().unwrap();
        assert_eq!(item.state, "withdrawn");
        assert_eq!(item.source["agentWithdrawal"]["sourceTurn"], turn);
        assert_eq!(item.action, "Do the selected human action");
        assert_eq!(
            store
                .interactions_page(key, 100, 0, false)
                .unwrap()
                .pending_total,
            0
        );
        assert_eq!(
            store
                .interactions_page(key, 100, 0, true)
                .unwrap()
                .history_total,
            1
        );
        store
            .save_human_requests(key, &[], Some((100, "source", turn, &[entry])))
            .unwrap();
        assert_eq!(
            store
                .conn
                .lock()
                .unwrap()
                .query_row(
                    "SELECT COUNT(*) FROM audit_log WHERE action='human_task_withdrawn_by_agent'",
                    [],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
            1
        );
        assert!(
            !store
                .transition_interaction(key, 100, "old", "pending", "completed")
                .unwrap()
        );
        assert!(!store.answer_human_task(key, 100, "old", 1).unwrap());
        let mut changed = withdrawal("old");
        changed.reason = "Another claimed reason".into();
        assert!(
            store
                .save_human_requests(key, &[], Some((100, "source", turn, &[changed])))
                .is_err()
        );
    }
    #[test]
    fn mixed_declaration_rolls_back_and_wrong_binding_native_or_resolved_targets_refuse() {
        let (_root, store, key, turn) = fixture();
        store
            .save_human_tasks(key, &[("old".into(), task("old", turn))])
            .unwrap();
        let entries = [withdrawal("old"), withdrawal("missing")];
        assert!(
            store
                .save_human_requests(
                    key,
                    &[("new".into(), task("new", turn))],
                    Some((100, "source", turn, &entries))
                )
                .is_err()
        );
        assert!(store.interaction(key, 100, "new").unwrap().is_none());
        assert_eq!(
            store.interaction(key, 100, "old").unwrap().unwrap().state,
            "pending"
        );
        for (wrong_key, user, native) in [
            (SessionKey::new(100, Some(8)), 100, "source"),
            (key, 101, "source"),
            (key, 100, "other-source"),
        ] {
            assert!(
                store
                    .save_human_requests(
                        wrong_key,
                        &[],
                        Some((user, native, turn, &[withdrawal("old")]))
                    )
                    .is_err()
            );
        }
        let mut channel = task("channel", turn);
        channel.kind = "native".into();
        channel.state = "unknown".into();
        store.save_interaction(key, &channel, None).unwrap();
        assert!(
            store
                .save_human_requests(
                    key,
                    &[],
                    Some((100, "source", turn, &[withdrawal("channel")]))
                )
                .is_err()
        );
        assert_eq!(
            store
                .interaction(key, 100, "channel")
                .unwrap()
                .unwrap()
                .state,
            "unknown"
        );
        store
            .transition_interaction(key, 100, "old", "pending", "completed")
            .unwrap();
        assert!(
            store
                .save_human_requests(key, &[], Some((100, "source", turn, &[withdrawal("old")])))
                .is_err()
        );
        assert_eq!(
            store.interaction(key, 100, "old").unwrap().unwrap().state,
            "completed"
        );
    }
}
