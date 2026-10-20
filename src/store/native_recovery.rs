//! Observation checkpoints are independent of native execution and Telegram receipts.
use super::*;
use anyhow::bail;
use serde_json::Value;

pub(crate) struct NativeCollection {
    pub local: i64,
    pub human: i64,
    pub key: SessionKey,
    pub native: String,
    pub anchor: String,
    pub stop_before: Option<String>,
    pub client_id: Option<String>,
    pub stop_client: Option<String>,
    pub next_local: Option<i64>,
}

impl Store {
    pub(super) fn init_native_recovery(&self) -> Result<()> {
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction()?;
        let baseline_exists:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='native_recovery_baseline')",[],|r|r.get(0))?;
        tx.execute_batch(
            "CREATE TABLE IF NOT EXISTS native_input_receipts(update_id INTEGER PRIMARY KEY REFERENCES incoming_updates(update_id),native_thread TEXT NOT NULL,native_turn TEXT NOT NULL,observed_at TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS native_collections(local_turn INTEGER PRIMARY KEY REFERENCES turns(id),anchor TEXT NOT NULL,first_turn TEXT NOT NULL,client_id TEXT,phase TEXT NOT NULL DEFAULT 'open',checked_at TEXT NOT NULL DEFAULT '',detail TEXT NOT NULL DEFAULT '');
             CREATE TABLE IF NOT EXISTS native_recovery_baseline(native_thread TEXT NOT NULL,native_turn TEXT NOT NULL,item_id TEXT NOT NULL,PRIMARY KEY(native_thread,native_turn,item_id));"
        )?;
        if !baseline_exists {
            // Pre-upgrade canonical items have legacy/unknown Telegram receipts.
            // Freeze their IDs atomically; a later collector must not resend them.
            tx.execute("INSERT INTO native_recovery_baseline SELECT native_thread,native_turn,item_id FROM interaction_native_messages",[])?;
        }
        // Existing explicit local session removal cascades its turns. Retire
        // their outbound work in the same transaction; keep original ingress
        // and native acceptance receipts, which belong to the retained journal.
        tx.execute_batch("CREATE TRIGGER IF NOT EXISTS native_collection_turn_deleted BEFORE DELETE ON turns BEGIN
            DELETE FROM topic_publication_parts WHERE publication IN (SELECT id FROM topic_publications WHERE local_turn=OLD.id);
            DELETE FROM topic_publications WHERE local_turn=OLD.id;
            DELETE FROM direct_topic_turns WHERE local_turn=OLD.id;
            DELETE FROM topic_projection_turns WHERE local_turn=OLD.id;
            DELETE FROM native_collections WHERE local_turn=OLD.id;
            END;")?;
        tx.commit()?;
        Ok(())
    }

    /// Only explicitly registered new publication lifetimes acquire recovery.
    /// Never backfill an old stream with unknown historical Telegram receipts.
    pub fn native_collection_started(&self, local: i64, physical: &str) -> Result<()> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        conn.execute("INSERT INTO native_collections(local_turn,anchor,first_turn,phase) SELECT t.id,?2,?2,CASE WHEN EXISTS(SELECT 1 FROM direct_topic_turns d WHERE d.local_turn=t.id) THEN 'open' ELSE 'boundary' END FROM turns t JOIN sessions s ON s.id=t.session_id JOIN users u ON u.tg_user_id=t.from_user_id WHERE t.id=?1 AND t.native_thread_id=s.codex_thread_id AND s.force_fresh_thread=0 AND u.allowed=1 ON CONFLICT(local_turn) DO UPDATE SET anchor=excluded.anchor,first_turn=excluded.first_turn WHERE native_collections.first_turn=''",params![local,physical])?;
        Ok(())
    }

    pub fn claim_collector_intent(
        &self,
        key: SessionKey,
        expected: Option<&str>,
        intent: &str,
    ) -> Result<bool> {
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction()?;
        let name = format!("goal_intent:{}:{}", key.chat_id, key.thread_id);
        let actual: Option<String> = tx
            .query_row("SELECT value FROM bot_state WHERE key=?", [&name], |r| {
                r.get(0)
            })
            .optional()?;
        if actual.as_deref() != expected {
            return Ok(false);
        }
        let queued:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM incoming_updates WHERE status IN ('received','processing','queued','steering') AND json_extract(payload_json,'$.message.chat.id')=?1 AND COALESCE(json_extract(payload_json,'$.message.message_thread_id'),0)=?2)",params![key.chat_id,key.thread_id],|r|r.get(0))?;
        if queued {
            return Ok(false);
        }
        tx.execute("INSERT INTO bot_state(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![name,intent])?;
        tx.commit()?;
        Ok(true)
    }

    pub fn native_collection_rejected(&self, local: i64) -> Result<()> {
        self.conn.lock().expect("store mutex poisoned").execute("UPDATE native_collections SET phase='rejected',detail='Native explicitly rejected this input' WHERE local_turn=? AND first_turn=''",[local])?;
        Ok(())
    }

    pub fn native_collection_submitting(&self, local: i64, client: Option<&str>) -> Result<()> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        // Routed/browser/other-participant executions are observation boundaries,
        // never new direct observers or implicit projection enrollment.
        conn.execute("INSERT OR IGNORE INTO native_collections(local_turn,anchor,first_turn,client_id,phase) SELECT t.id,'','',?2,CASE WHEN EXISTS(SELECT 1 FROM direct_topic_turns d WHERE d.local_turn=t.id) THEN 'open' ELSE 'boundary' END FROM turns t JOIN sessions s ON s.id=t.session_id JOIN users u ON u.tg_user_id=t.from_user_id WHERE t.id=?1 AND t.native_thread_id=s.codex_thread_id AND s.force_fresh_thread=0 AND u.allowed=1",params![local,client])?;
        Ok(())
    }

    /// An exact RPC/history receipt survives a later collection failure and a
    /// foreground acknowledgement timeout. It grants no replay or execution.
    pub fn native_input_receipt(
        &self,
        local: i64,
        client: Option<&str>,
        physical: &str,
    ) -> Result<()> {
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction()?;
        input_receipt_on(&tx, local, client, physical)?;
        tx.commit()?;
        Ok(())
    }

    pub fn native_collection_next(&self) -> Result<Option<NativeCollection>> {
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction()?;
        let row=tx.query_row("SELECT n.local_turn,t.from_user_id,s.chat_id,s.thread_id,t.native_thread_id,n.anchor,(SELECT NULLIF(later.first_turn,'') FROM native_collections later JOIN turns lt ON lt.id=later.local_turn WHERE lt.session_id=t.session_id AND later.local_turn>n.local_turn AND later.phase!='rejected' ORDER BY later.local_turn LIMIT 1),n.client_id,(SELECT later.client_id FROM native_collections later JOIN turns lt ON lt.id=later.local_turn WHERE lt.session_id=t.session_id AND later.local_turn>n.local_turn AND later.phase!='rejected' ORDER BY later.local_turn LIMIT 1),(SELECT later.local_turn FROM native_collections later JOIN turns lt ON lt.id=later.local_turn WHERE lt.session_id=t.session_id AND later.local_turn>n.local_turn AND later.phase!='rejected' ORDER BY later.local_turn LIMIT 1) FROM native_collections n JOIN turns t ON t.id=n.local_turn JOIN sessions s ON s.id=t.session_id JOIN users u ON u.tg_user_id=t.from_user_id WHERE n.phase='open' AND t.native_thread_id IS NOT NULL AND s.creator_user_id=t.from_user_id AND u.allowed=1 AND s.codex_thread_id=t.native_thread_id AND s.force_fresh_thread=0 ORDER BY n.checked_at,n.local_turn LIMIT 1",[],|r|Ok(NativeCollection{local:r.get(0)?,human:r.get(1)?,key:SessionKey::new(r.get(2)?,Some(r.get(3)?)),native:r.get(4)?,anchor:r.get(5)?,stop_before:r.get(6)?,client_id:r.get(7)?,stop_client:r.get(8)?,next_local:r.get(9)?})).optional()?;
        if let Some(r) = &row {
            tx.execute(
                "UPDATE native_collections SET checked_at=?2 WHERE local_turn=?1",
                params![r.local, now_string()],
            )?;
        }
        tx.commit()?;
        Ok(row)
    }

    pub fn native_delivery_status(&self, key: SessionKey, human: i64) -> Result<String> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let owns:bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM sessions s JOIN users u ON u.tg_user_id=s.creator_user_id WHERE s.chat_id=?1 AND s.thread_id=?2 AND s.creator_user_id=?3 AND u.allowed=1)",params![key.chat_id,key.thread_id,human],|r|r.get(0))?;
        if !owns {
            return Ok(String::new());
        }
        let (ready,uncertain,rejected):(i64,i64,i64)=conn.query_row("SELECT COALESCE(SUM(p.phase IN ('ready','publishing')),0),COALESCE(SUM(p.phase='uncertain'),0),COALESCE(SUM(p.phase='rejected'),0) FROM topic_publications p JOIN sessions s ON s.id=p.session_id WHERE s.chat_id=?1 AND s.thread_id=?2 AND p.human_id=?3",params![key.chat_id,key.thread_id,human],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
        let observation:Option<String>=conn.query_row("SELECT n.detail FROM native_collections n JOIN turns t ON t.id=n.local_turn JOIN sessions s ON s.id=t.session_id WHERE s.chat_id=?1 AND s.thread_id=?2 AND t.from_user_id=?3 ORDER BY n.local_turn DESC LIMIT 1",params![key.chat_id,key.thread_id,human],|r|r.get(0)).optional()?;
        let accepted:i64=conn.query_row("SELECT COUNT(*) FROM native_input_receipts n JOIN incoming_updates i ON i.update_id=n.update_id JOIN turns t ON t.id=i.turn_id JOIN sessions s ON s.id=t.session_id WHERE s.chat_id=?1 AND s.thread_id=?2 AND t.from_user_id=?3 AND i.status='undetermined'",params![key.chat_id,key.thread_id,human],|r|r.get(0))?;
        let mut text = String::new();
        if ready > 0 {
            text.push_str(&format!(
                "\nTelegram delivery pending: {ready} retained messages."
            ));
        }
        if uncertain > 0 {
            text.push_str(&format!("\nTelegram delivery unconfirmed: {uncertain} messages; no automatic duplicate sends. /history opens their native conversation here."));
        }
        if rejected > 0 {
            text.push_str(&format!("\nTelegram rejected {rejected} retained messages; /history opens their native conversation here."));
        }
        if accepted > 0 {
            text.push_str(&format!("\nCodex accepted {accepted} retained inputs; their execution/collection ended separately. Do not resend them."));
        }
        if let Some(detail) = observation.filter(|v| !v.is_empty()) {
            text.push_str(&format!("\nNative observation: {detail}."));
        }
        Ok(text)
    }

    pub fn native_collection_failed(&self, local: i64) -> Result<()> {
        self.conn.lock().expect("store mutex poisoned").execute("UPDATE native_collections SET detail='Native observation unavailable; retained output recovery pending' WHERE local_turn=?",[local])?;
        Ok(())
    }

    /// Commit a complete bounded read, oldest first, into the existing durable
    /// destination queue. Exact item identities deduplicate live and recovered events.
    pub fn native_collection_observed(
        &self,
        selected: &NativeCollection,
        turns: &[Value],
        goal_active: bool,
    ) -> Result<Option<Value>> {
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction()?;
        let valid:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM native_collections n JOIN turns t ON t.id=n.local_turn JOIN sessions s ON s.id=t.session_id JOIN users u ON u.tg_user_id=t.from_user_id WHERE n.local_turn=?1 AND n.phase='open' AND n.anchor=?2 AND t.native_thread_id=?3 AND t.from_user_id=?4 AND s.creator_user_id=?4 AND u.allowed=1 AND s.codex_thread_id=?3 AND s.force_fresh_thread=0 AND s.chat_id=?5 AND s.thread_id=?6 AND (SELECT later.local_turn FROM native_collections later JOIN turns lt ON lt.id=later.local_turn WHERE lt.session_id=t.session_id AND later.local_turn>n.local_turn AND later.phase!='rejected' ORDER BY later.local_turn LIMIT 1) IS ?7)",params![selected.local,selected.anchor,selected.native,selected.human,selected.key.chat_id,selected.key.thread_id,selected.next_local],|r|r.get(0))?;
        if !valid {
            bail!("Native recovery owner or observation boundary changed");
        }
        if selected.stop_before.as_deref() == Some(selected.anchor.as_str()) {
            // A replacement collector subscribed to the same physical writer.
            // Its checkpoint owns future observation; existing queued receipts stay intact.
            tx.execute("UPDATE native_collections SET phase='transferred',detail='Observation transferred to replacement collector' WHERE local_turn=?",[selected.local])?;
            tx.commit()?;
            return Ok(None);
        }
        let end = turns.iter().position(|t| {
            selected
                .stop_before
                .as_ref()
                .is_some_and(|stop| t["id"] == *stop)
                || selected.stop_client.as_ref().is_some_and(|client| {
                    t["items"].as_array().is_some_and(|items| {
                        items
                            .iter()
                            .any(|i| i["type"] == "userMessage" && i["clientId"] == *client)
                    })
                })
        });
        // A later admission without a proved history boundary cannot authorize
        // reading past this observer's last checkpoint. Still recover that
        // checkpoint, so a definitely unaccepted successor cannot block it.
        let limit = end.unwrap_or_else(|| {
            if selected.next_local.is_some() {
                turns
                    .iter()
                    .position(|t| t["id"] == selected.anchor)
                    .unwrap_or(0)
                    + 1
            } else {
                turns.len()
            }
        });
        let turns = &turns[..limit.min(turns.len())];
        if selected.anchor.is_empty() {
            let first = turns.first().context("Native input receipt missing")?;
            let matched = first["items"].as_array().is_some_and(|items| {
                items.iter().any(|i| {
                    i["type"] == "userMessage"
                        && i["clientId"].as_str() == selected.client_id.as_deref()
                })
            });
            if !matched || selected.client_id.is_none() {
                bail!("Native admission identity was not proved");
            }
            tx.execute(
                "UPDATE native_collections SET first_turn=?2 WHERE local_turn=?1 AND first_turn=''",
                params![selected.local, first["id"].as_str()],
            )?;
        }
        for turn in turns {
            let id = turn["id"]
                .as_str()
                .context("Native recovery turn identity missing")?;
            for item in turn["items"]
                .as_array()
                .context("Native recovery items missing")?
            {
                if item["type"] == "userMessage" {
                    if let Some(client) = item["clientId"].as_str() {
                        input_receipt_on(&tx, selected.local, Some(client), id)?;
                    }
                } else if item["type"] == "agentMessage" && turn["status"] != "inProgress" {
                    let text = item["text"].as_str().unwrap_or_default();
                    if !text.trim().is_empty() {
                        super::interaction::record_native_message_on(
                            &tx,
                            selected.local,
                            &selected.native,
                            id,
                            item["id"]
                                .as_str()
                                .context("Native recovery item identity missing")?,
                            item["phase"].as_str(),
                            text,
                        )?;
                    }
                }
            }
        }
        let last = turns.last().context("Empty native recovery snapshot")?;
        let terminal = last["status"]
            .as_str()
            .is_some_and(|v| matches!(v, "completed" | "interrupted" | "failed"));
        let local_running: bool = tx.query_row(
            "SELECT status='running' FROM turns WHERE id=?",
            [selected.local],
            |r| r.get(0),
        )?;
        // Keep observing an active goal between physical turns; never resume it.
        tx.execute(
            "UPDATE native_collections SET anchor=?2,phase=?3,detail=?4 WHERE local_turn=?1",
            params![
                selected.local,
                last["id"].as_str(),
                if terminal && (!goal_active && !local_running || end.is_some()) {
                    "complete"
                } else {
                    "open"
                },
                if terminal && goal_active {
                    "Goal active; waiting for observed native continuation"
                } else {
                    "Native output reconciled"
                }
            ],
        )?;
        tx.commit()?;
        Ok(Some(last.clone()))
    }
}

fn input_receipt_on(
    conn: &Connection,
    local: i64,
    client: Option<&str>,
    physical: &str,
) -> Result<()> {
    let native: Option<String> = conn.query_row(
        "SELECT native_thread_id FROM turns WHERE id=?",
        [local],
        |r| r.get(0),
    )?;
    let Some(native) = native else {
        return Ok(());
    };
    let update = if let Some(client) = client {
        let parts: Vec<_> = client.split(':').collect();
        if parts.len() != 4 || parts[0] != "telegram" {
            return Ok(());
        }
        let (Ok(chat), Ok(human), Ok(update)) = (
            parts[1].parse::<i64>(),
            parts[2].parse::<i64>(),
            parts[3].parse::<i64>(),
        ) else {
            return Ok(());
        };
        conn.query_row("SELECT i.update_id FROM incoming_updates i JOIN turns t ON t.id=i.turn_id JOIN sessions s ON s.id=t.session_id WHERE i.update_id=?1 AND t.session_id=(SELECT session_id FROM turns WHERE id=?2) AND t.native_thread_id=(SELECT native_thread_id FROM turns WHERE id=?2) AND t.from_user_id=?3 AND s.chat_id=?4 AND json_extract(i.payload_json,'$.message.from.id')=?3 AND json_extract(i.payload_json,'$.message.chat.id')=?4 AND COALESCE(json_extract(i.payload_json,'$.message.message_thread_id'),0)=s.thread_id",params![update,local,human,chat],|r|r.get::<_,i64>(0)).optional()?
    } else {
        conn.query_row(
            "SELECT MIN(update_id) FROM incoming_updates WHERE turn_id=?",
            [local],
            |r| r.get::<_, Option<i64>>(0),
        )?
    };
    if let Some(update) = update {
        conn.execute(
            "INSERT OR IGNORE INTO native_input_receipts VALUES(?1,?2,?3,?4)",
            params![update, native, physical, now_string()],
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn start(s: &Store) -> (i64, SessionKey) {
        let key = SessionKey::new(100, Some(7));
        let session = s
            .ensure_session(key, 100, &super::super::tests::defaults())
            .unwrap();
        s.set_session_codex_thread(key, "native").unwrap();
        let local = s
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
        s.record_turn_native_thread(local, "native").unwrap();
        s.topic_direct_register(local, 100, key).unwrap();
        s.native_collection_started(local, "first").unwrap();
        (local, key)
    }
    fn snapshot(id: &str, status: &str, text: &str) -> Value {
        json!({"id":id,"status":status,"items":[{"id":format!("{id}-answer"),"type":"agentMessage","phase":"final_answer","text":text}]})
    }
    #[test]
    fn routed_successor_bounds_direct_recovery_without_enrolling_an_observer() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let s = Store::open(file.path(), &[100], &super::super::tests::defaults()).unwrap();
        let (old, key) = start(&s);
        s.record_turn_finished(old, "failed", None).unwrap();
        let session = s
            .ensure_session(key, 100, &super::super::tests::defaults())
            .unwrap();
        let next = s
            .record_turn_started(
                session.id,
                &TurnRequest {
                    session_key: key,
                    from_user_id: 100,
                    prompt: "Browser input".into(),
                    runtime_instructions: None,
                    attachments: vec![],
                    review_mode: None,
                    override_search_mode: None,
                },
            )
            .unwrap();
        s.record_turn_native_thread(next, "native").unwrap();
        s.native_collection_submitting(next, Some("conversation:next"))
            .unwrap();
        let selected = s.native_collection_next().unwrap().unwrap();
        assert_eq!(selected.local, old);
        assert_eq!(selected.next_local, Some(next));
        assert_eq!(selected.stop_client.as_deref(), Some("conversation:next"));
        // An unconfirmed successor still permits this last proven checkpoint,
        // but cannot authorize unrelated later output to enter its direct queue.
        let observed = s
            .native_collection_observed(
                &selected,
                &[
                    snapshot("first", "completed", "Old reply"),
                    snapshot("second", "inProgress", "Other surface reply"),
                ],
                true,
            )
            .unwrap()
            .unwrap();
        assert_eq!(observed["id"], "first");
        s.native_collection_started(next, "second").unwrap();
        let selected = s.native_collection_next().unwrap().unwrap();
        assert_eq!(selected.stop_before.as_deref(), Some("second"));
        let observed = s
            .native_collection_observed(
                &selected,
                &[
                    snapshot("first", "completed", "Old reply"),
                    snapshot("second", "completed", "Other surface reply"),
                ],
                false,
            )
            .unwrap()
            .unwrap();
        assert_eq!(observed["id"], "first");
        assert!(s.native_collection_next().unwrap().is_none());
        assert!(!s.topic_projection_active(next).unwrap());
        let conn = s.conn.lock().unwrap();
        assert_eq!(
            conn.query_row(
                "SELECT count(*) FROM topic_publications WHERE body='Old reply'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
        assert_eq!(
            conn.query_row(
                "SELECT count(*) FROM topic_publications WHERE body='Other surface reply'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
    }
    #[test]
    fn explicit_session_removal_retires_delivery_without_erasing_original_receipts() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let s = Store::open(file.path(), &[100], &super::super::tests::defaults()).unwrap();
        let (local, key) = start(&s);
        let update: crate::telegram::Update = serde_json::from_value(json!({"update_id":42,"message":{"message_id":42,"message_thread_id":7,"from":{"id":100,"is_bot":false,"first_name":"Synthetic"},"chat":{"id":100,"type":"private"},"text":"Original"}})).unwrap();
        s.admit_update(&update).unwrap();
        s.begin_update(42).unwrap();
        s.begin_update_steer(42, local).unwrap();
        s.native_input_receipt(local, Some("telegram:100:100:42"), "first")
            .unwrap();
        s.record_native_message(
            local,
            "native",
            "first",
            "final",
            Some("final_answer"),
            "Retained answer",
        )
        .unwrap();
        let p = s.topic_publication_claim(key, 100).unwrap().unwrap();
        s.topic_publication_parts(p.id, &[p.body]).unwrap();
        assert!(s.topic_publication_reserve(p.id, 0).unwrap());
        // Explicit removal also retires a potentially accepted send; it never
        // recreates the destination or retries that unknown network operation.
        s.delete_session(key).unwrap();
        assert!(s.native_collection_next().unwrap().is_none());
        assert!(s.topic_publication_claim(key, 100).unwrap().is_none());
        let conn = s.conn.lock().unwrap();
        assert_eq!(
            conn.query_row(
                "SELECT count(*) FROM incoming_updates WHERE update_id=42",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
        assert_eq!(
            conn.query_row(
                "SELECT count(*) FROM native_input_receipts WHERE update_id=42",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
        assert_eq!(
            conn.query_row("SELECT count(*) FROM topic_publication_parts", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert!(
            !conn
                .prepare("PRAGMA foreign_key_check")
                .unwrap()
                .exists([])
                .unwrap()
        );
    }
    #[test]
    fn missed_completion_is_recovered_once_and_pending_delivery_survives_restart() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let s = Store::open(file.path(), &[100], &super::super::tests::defaults()).unwrap();
        let (local, key) = start(&s);
        s.record_turn_finished(local, "failed", None).unwrap();
        let selected = s.native_collection_next().unwrap().unwrap();
        s.native_collection_observed(
            &selected,
            &[snapshot("first", "completed", "Recovered final")],
            false,
        )
        .unwrap();
        assert!(s.native_collection_next().unwrap().is_none());
        drop(s);
        let s = Store::open(file.path(), &[100], &super::super::tests::defaults()).unwrap();
        let p = s.topic_publication_claim(key, 100).unwrap().unwrap();
        assert_eq!(p.body, "Recovered final");
        s.topic_publication_parts(p.id, &[p.body]).unwrap();
        assert!(s.topic_publication_reserve(p.id, 0).unwrap());
        s.topic_publication_receipt(p.id, 0, Some(700), false)
            .unwrap();
        s.topic_publication_finish(p.id).unwrap();
        assert!(s.topic_publication_claim(key, 100).unwrap().is_none());
        let count: i64 = s
            .conn
            .lock()
            .unwrap()
            .query_row(
                "SELECT count(*) FROM handoff_messages WHERE message_id=700",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }
    #[test]
    fn active_history_is_not_published_as_a_completed_item_and_goal_gap_stays_observed() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let s = Store::open(file.path(), &[100], &super::super::tests::defaults()).unwrap();
        let (local, key) = start(&s);
        let selected = s.native_collection_next().unwrap().unwrap();
        s.native_collection_observed(
            &selected,
            &[snapshot("first", "inProgress", "Partial")],
            true,
        )
        .unwrap();
        assert!(s.topic_publication_claim(key, 100).unwrap().is_none());
        let selected = s.native_collection_next().unwrap().unwrap();
        s.native_collection_observed(&selected, &[snapshot("first", "completed", "Full")], true)
            .unwrap();
        assert!(s.native_collection_next().unwrap().is_some());
        assert!(
            s.native_delivery_status(key, 100)
                .unwrap()
                .contains("waiting for observed native continuation")
        );
        s.record_native_message(
            local,
            "native",
            "first",
            "first-answer",
            Some("final_answer"),
            "Full",
        )
        .unwrap();
        assert_eq!(
            s.topic_publication_inventory(100).unwrap()["items"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        s.set_session_codex_thread(key, "replacement").unwrap();
        assert!(
            s.native_collection_observed(
                &selected,
                &[snapshot("first", "completed", "Full")],
                false
            )
            .is_err()
        );
        assert!(s.topic_publication_claim(key, 100).unwrap().is_none());
    }
    #[test]
    fn acceptance_survives_failed_collection_and_late_receipt_without_replay() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let s = Store::open(file.path(), &[100], &super::super::tests::defaults()).unwrap();
        let (local, key) = start(&s);
        let update:crate::telegram::Update=serde_json::from_value(json!({"update_id":42,"message":{"message_id":42,"message_thread_id":7,"from":{"id":100,"is_bot":false,"first_name":"Synthetic"},"chat":{"id":100,"type":"private"},"text":"Correction"}})).unwrap();
        s.admit_update(&update).unwrap();
        s.begin_update(42).unwrap();
        s.begin_update_steer(42, local).unwrap();
        s.mark_update_steer_undetermined(42).unwrap();
        assert_eq!(s.uncertain_update_ids(100, key).unwrap(), vec![42]);
        s.record_turn_finished(local, "failed", None).unwrap();
        s.native_input_receipt(local, Some("telegram:100:101:42"), "first")
            .unwrap();
        assert_eq!(s.uncertain_update_ids(100, key).unwrap(), vec![42]);
        s.native_input_receipt(local, Some("telegram:100:100:42"), "first")
            .unwrap();
        assert!(s.uncertain_update_ids(100, key).unwrap().is_empty());
        assert!(
            s.native_delivery_status(key, 100)
                .unwrap()
                .contains("Codex accepted 1")
        );
        assert!(!s.admit_update(&update).unwrap());
        assert!(s.begin_update(42).is_err());
    }
    #[test]
    fn new_execution_boundary_cannot_steal_old_outputs() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let s = Store::open(file.path(), &[100], &super::super::tests::defaults()).unwrap();
        let (old, key) = start(&s);
        let stale = s.native_collection_next().unwrap().unwrap();
        let (new, _) = start(&s);
        s.conn.lock().unwrap().execute("UPDATE native_collections SET anchor='second',first_turn='second' WHERE local_turn=?",[new]).unwrap();
        let turns = vec![
            snapshot("first", "completed", "Old answer"),
            snapshot("second", "completed", "New answer"),
        ];
        assert!(s.native_collection_observed(&stale, &turns, true).is_err());
        s.conn
            .lock()
            .unwrap()
            .execute(
                "UPDATE native_collections SET checked_at='' WHERE local_turn=?",
                [old],
            )
            .unwrap();
        let selected = s.native_collection_next().unwrap().unwrap();
        assert_eq!(selected.stop_before.as_deref(), Some("second"));
        s.native_collection_observed(&selected, &turns, true)
            .unwrap();
        let p = s.topic_publication_claim(key, 100).unwrap().unwrap();
        assert_eq!(p.body, "Old answer");
    }
    #[test]
    fn same_physical_reattachment_transfers_observation_without_an_empty_range() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let s = Store::open(file.path(), &[100], &super::super::tests::defaults()).unwrap();
        let (old, key) = start(&s);
        let (new, _) = start(&s);
        let selected = s.native_collection_next().unwrap().unwrap();
        assert_eq!(selected.local, old);
        s.native_collection_observed(
            &selected,
            &[snapshot("first", "inProgress", "Partial")],
            true,
        )
        .unwrap();
        let next = s.native_collection_next().unwrap().unwrap();
        assert_eq!(next.local, new);
        assert!(s.native_delivery_status(key, 101).unwrap().is_empty());
    }

    #[test]
    fn uncertain_initial_submission_requires_exact_client_receipt_and_rejection_does_not_block_old_output()
     {
        let file = tempfile::NamedTempFile::new().unwrap();
        let s = Store::open(file.path(), &[100], &super::super::tests::defaults()).unwrap();
        let (old, key) = start(&s);
        let (new, _) = start(&s);
        s.conn.lock().unwrap().execute("UPDATE native_collections SET anchor='',first_turn='',client_id='telegram:100:100:44' WHERE local_turn=?",[new]).unwrap();
        let old_selected = s.native_collection_next().unwrap().unwrap();
        assert_eq!(old_selected.local, old);
        s.native_collection_observed(
            &old_selected,
            &[snapshot("first", "completed", "Known prior reply")],
            false,
        )
        .unwrap();
        let selected = s.native_collection_next().unwrap().unwrap();
        assert_eq!(selected.local, new);
        assert!(
            s.native_collection_observed(
                &selected,
                &[snapshot("unrelated", "completed", "Do not import")],
                false
            )
            .is_err()
        );
        let mut matched = snapshot("admitted", "completed", "Matched reply");
        matched["items"].as_array_mut().unwrap().insert(0,json!({"type":"userMessage","id":"human","clientId":"telegram:100:100:44","content":[]}));
        s.native_collection_observed(&selected, &[matched], false)
            .unwrap();
        assert_eq!(
            s.topic_publication_inventory(100).unwrap()["items"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        let (rejected, _) = start(&s);
        s.conn
            .lock()
            .unwrap()
            .execute(
                "UPDATE native_collections SET anchor='',first_turn='' WHERE local_turn=?",
                [rejected],
            )
            .unwrap();
        s.native_collection_rejected(rejected).unwrap();
        assert!(s.topic_publication_claim(key, 100).unwrap().is_some());
    }
    #[test]
    fn upgrade_freezes_legacy_item_ids_without_reclassifying_new_receipts() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let s = Store::open(file.path(), &[100], &super::super::tests::defaults()).unwrap();
        let (local, key) = start(&s);
        s.record_native_message(
            local,
            "native",
            "first",
            "legacy-item",
            Some("final_answer"),
            "Legacy output",
        )
        .unwrap();
        {
            let conn = s.conn.lock().unwrap();
            conn.execute("DELETE FROM topic_publications", []).unwrap();
            conn.execute("DROP TABLE native_recovery_baseline", [])
                .unwrap();
        }
        drop(s);
        let s = Store::open(file.path(), &[100], &super::super::tests::defaults()).unwrap();
        s.record_native_message(
            local,
            "native",
            "first",
            "legacy-item",
            Some("final_answer"),
            "Legacy output",
        )
        .unwrap();
        assert!(s.topic_publication_claim(key, 100).unwrap().is_none());
        s.record_native_message(
            local,
            "native",
            "first",
            "new-item",
            Some("final_answer"),
            "New output",
        )
        .unwrap();
        assert_eq!(
            s.topic_publication_claim(key, 100).unwrap().unwrap().body,
            "New output"
        );
        let columns: i64 = s
            .conn
            .lock()
            .unwrap()
            .query_row(
                "SELECT count(*) FROM pragma_table_info('topic_projection_turns')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            columns, 2,
            "Previous binary's positional registration INSERT must remain valid"
        );
    }
    #[test]
    fn observation_attachment_cannot_replace_a_newer_human_goal_intent() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let s = Store::open(file.path(), &[100], &super::super::tests::defaults()).unwrap();
        let (_, key) = start(&s);
        assert!(s.claim_collector_intent(key, None, "initial").unwrap());
        let name = format!("goal_intent:{}:{}", key.chat_id, key.thread_id);
        s.save_bot_state(&name, "new-human-request").unwrap();
        assert!(
            !s.claim_collector_intent(key, Some("initial"), "stale-observer")
                .unwrap()
        );
        assert_eq!(
            s.bot_state_value(&name).unwrap().as_deref(),
            Some("new-human-request")
        );
        let update:crate::telegram::Update=serde_json::from_value(json!({"update_id":55,"message":{"message_id":55,"message_thread_id":7,"from":{"id":100,"is_bot":false,"first_name":"Synthetic"},"chat":{"id":100,"type":"private"},"text":"New queued human input"}})).unwrap();
        s.admit_update(&update).unwrap();
        assert!(
            !s.claim_collector_intent(key, Some("new-human-request"), "observer")
                .unwrap()
        );
    }
}
