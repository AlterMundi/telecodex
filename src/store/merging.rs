//! Immutable context packets and per-destination native receipts.
use super::*;
use serde_json::{Value, json};

fn check(conn: &Connection, target: &Value) -> Result<()> {
    let native = target["native"].as_str();
    let found: bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM sessions s JOIN users u ON u.tg_user_id=?3 WHERE s.id=?4 AND s.chat_id=?1 AND s.thread_id=?2 AND s.creator_user_id=?3 AND u.allowed=1 AND s.codex_thread_id IS ?5 AND (?5 IS NOT NULL OR s.force_fresh_thread=1))",params![target["key"]["chat_id"].as_i64(),target["key"]["thread_id"].as_i64(),target["human"].as_i64(),target["session"].as_i64(),native],|r|r.get(0))?;
    if !found {
        return Err(anyhow!("merge source or destination binding changed"));
    }
    Ok(())
}

impl Store {
    pub(super) fn init_merging(&self) -> Result<()> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        conn.execute_batch("CREATE TABLE IF NOT EXISTS merge_operations(id TEXT PRIMARY KEY,origin_session INTEGER NOT NULL,requester INTEGER NOT NULL,update_id INTEGER NOT NULL UNIQUE,doc TEXT NOT NULL,phase TEXT NOT NULL,instance_id TEXT NOT NULL,created_at TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS merge_legs(id TEXT PRIMARY KEY,operation TEXT NOT NULL REFERENCES merge_operations(id),target TEXT NOT NULL,packet TEXT NOT NULL,phase TEXT NOT NULL,local_turn INTEGER,native_turn TEXT,reply TEXT,reason TEXT,instance_id TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS merge_executions(local_turn INTEGER NOT NULL,native_turn TEXT NOT NULL,reply TEXT,terminal TEXT,PRIMARY KEY(local_turn,native_turn));")?;
        conn.execute("UPDATE merge_operations SET phase='unconfirmed' WHERE phase IN ('preparing','creating_topic') AND instance_id!=?",[&self.instance_id])?;
        conn.execute("UPDATE merge_legs SET phase=CASE WHEN phase='queued' THEN 'retained_queue' ELSE 'unconfirmed' END,reason='Listener restarted; no native input is replayed' WHERE phase IN ('queued','dispatching','submitting','steering') AND instance_id!=?",[&self.instance_id])?;
        let accepted = {
            let mut stmt = conn.prepare("SELECT id FROM merge_legs WHERE phase='accepted'")?;
            stmt.query_map([], |r| r.get::<_, String>(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?
        };
        drop(conn);
        for id in accepted {
            self.merge_sync(&id)?;
        }
        Ok(())
    }
    pub fn merge_begin(&self, doc: &Value, update: i64) -> Result<bool> {
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction()?;
        for source in doc["sources"].as_array().context("merge sources missing")? {
            check(&tx, source)?;
        }
        let exists: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM merge_operations WHERE update_id=?)",
            [update],
            |r| r.get(0),
        )?;
        if exists {
            return Ok(false);
        }
        tx.execute(
            "INSERT INTO merge_operations VALUES(?,?,?,?,?,'preparing',?,?)",
            params![
                doc["id"].as_str(),
                doc["sources"][0]["session"].as_i64(),
                doc["human"].as_i64(),
                update,
                serde_json::to_string(doc)?,
                self.instance_id,
                now_string()
            ],
        )?;
        tx.execute("INSERT INTO audit_log(actor_user_id,action,details_json,created_at) VALUES(?,'merge_requested',?,?)",params![doc["human"].as_i64(),json!({"operation":doc["id"],"mode":doc["mode"]}).to_string(),now_string()])?;
        tx.commit()?;
        Ok(true)
    }
    pub fn merge_operation_phase(&self, id: &str, phase: &str) -> Result<()> {
        self.conn.lock().expect("store mutex poisoned").execute(
            "UPDATE merge_operations SET phase=? WHERE id=?",
            params![phase, id],
        )?;
        Ok(())
    }
    pub fn merge_operation_failed(&self, id: &str) -> Result<()> {
        self.conn.lock().expect("store mutex poisoned").execute("UPDATE merge_operations SET phase=CASE WHEN phase='preparing' THEN 'preparation_failed' WHEN phase='creating_topic' THEN 'unconfirmed' ELSE phase END WHERE id=?",[id])?;
        Ok(())
    }
    pub fn merge_ready(&self, doc: &Value, legs: &[(String, Value, String)]) -> Result<()> {
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction()?;
        for source in doc["sources"].as_array().context("merge sources missing")? {
            check(&tx, source)?;
        }
        for (id, target, packet) in legs {
            check(&tx, target)?;
            tx.execute("INSERT INTO merge_legs(id,operation,target,packet,phase,instance_id) VALUES(?,?,?,?,'queued',?)",params![id,doc["id"].as_str(),serde_json::to_string(target)?,packet,self.instance_id])?;
        }
        let count=tx.execute("UPDATE merge_operations SET doc=?,phase='ready' WHERE id=? AND phase IN ('preparing','creating_topic') AND instance_id=?",params![serde_json::to_string(doc)?,doc["id"].as_str(),self.instance_id])?;
        if count != 1 {
            return Err(anyhow!(
                "merge operation cannot replace its retained packet"
            ));
        }
        tx.commit()?;
        Ok(())
    }
    pub fn merge_prepared(&self, doc: &Value) -> Result<()> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        for source in doc["sources"].as_array().context("merge sources missing")? {
            check(&conn, source)?;
        }
        let n = conn.execute(
            "UPDATE merge_operations SET doc=? WHERE id=? AND phase='preparing' AND instance_id=?",
            params![
                serde_json::to_string(doc)?,
                doc["id"].as_str(),
                self.instance_id
            ],
        )?;
        if n != 1 {
            return Err(anyhow!("merge preparation is no longer current"));
        }
        Ok(())
    }
    pub fn merge_leg(&self, id: &str) -> Result<Value> {
        self.conn.lock().expect("store mutex poisoned").query_row("SELECT target,packet,phase,local_turn,native_turn,reply,reason,operation FROM merge_legs WHERE id=?",[id],|r|{
            let raw:String=r.get(0)?;let target:Value=serde_json::from_str(&raw).map_err(|e|rusqlite::Error::FromSqlConversionFailure(0,rusqlite::types::Type::Text,Box::new(e)))?;
            Ok(json!({"id":id,"target":target,"packet":r.get::<_,String>(1)?,"phase":r.get::<_,String>(2)?,"local_turn":r.get::<_,Option<i64>>(3)?,"native_turn":r.get::<_,Option<String>>(4)?,"reply":r.get::<_,Option<String>>(5)?,"reason":r.get::<_,Option<String>>(6)?,"operation":r.get::<_,String>(7)?}))
        }).map_err(Into::into)
    }
    pub fn merge_claim(&self, id: &str, phase: &str, turn: Option<i64>) -> Result<bool> {
        let leg = self.merge_leg(id)?;
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction()?;
        check(&tx, &leg["target"])?;
        let n=tx.execute("UPDATE merge_legs SET phase=?,local_turn=? WHERE id=? AND phase='queued' AND instance_id=?",params![phase,turn,id,self.instance_id])?;
        tx.commit()?;
        Ok(n == 1)
    }
    pub fn merge_resume(&self, key: SessionKey, human: i64, id: &str) -> Result<Vec<String>> {
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction()?;
        let origin:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM merge_operations m JOIN sessions s ON s.id=m.origin_session JOIN users u ON u.tg_user_id=m.requester WHERE m.id=?1 AND m.requester=?2 AND s.creator_user_id=?2 AND u.allowed=1 AND s.chat_id=?3 AND s.thread_id=?4)",params![id,human,key.chat_id,key.thread_id],|r|r.get(0))?;
        if !origin {
            return Err(anyhow!("merge operation is unavailable in this scope"));
        }
        let ids = {
            let mut stmt=tx.prepare("SELECT id,target FROM merge_legs WHERE operation=? AND phase IN ('retained_queue','detached_queue','rejected') AND native_turn IS NULL")?;
            let rows = stmt
                .query_map([id], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?;
            for (_, target) in &rows {
                check(&tx, &serde_json::from_str(target)?)?;
            }
            rows.into_iter().map(|(id, _)| id).collect::<Vec<_>>()
        };
        for leg in &ids {
            tx.execute(
                "UPDATE merge_legs SET phase='queued',instance_id=? WHERE id=?",
                params![self.instance_id, leg],
            )?;
        }
        tx.commit()?;
        Ok(ids)
    }
    pub fn merge_link(&self, id: &str, turn: i64) -> Result<()> {
        self.conn.lock().expect("store mutex poisoned").execute(
            "UPDATE merge_legs SET local_turn=? WHERE id=? AND phase='dispatching'",
            params![turn, id],
        )?;
        Ok(())
    }
    pub fn merge_phase(&self, id: &str, phase: &str, reason: &str) -> Result<()> {
        self.conn.lock().expect("store mutex poisoned").execute("UPDATE merge_legs SET phase=?,reason=? WHERE id=? AND phase NOT IN ('observed','accepted')",params![phase,reason,id])?;
        Ok(())
    }
    pub fn merge_local(&self, turn: i64) -> Result<Vec<String>> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let mut stmt = conn.prepare("SELECT id FROM merge_legs WHERE local_turn=?")?;
        Ok(stmt
            .query_map([turn], |r| r.get(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?)
    }
    pub fn merge_thread(&self, turn: i64, native: &str) -> Result<()> {
        for id in self.merge_local(turn)? {
            let mut leg = self.merge_leg(&id)?;
            if let Some(expected) = leg["target"]["native"].as_str() {
                check(
                    &self.conn.lock().expect("store mutex poisoned"),
                    &leg["target"],
                )?;
                if expected != native {
                    return Err(anyhow!("merge native thread mismatch; target preserved"));
                }
            } else {
                let conn = self.conn.lock().expect("store mutex poisoned");
                check(&conn, &leg["target"])?;
                leg["target"]["native"] = json!(native);
                conn.execute("UPDATE merge_legs SET target=? WHERE id=? AND local_turn=? AND phase='dispatching'",params![serde_json::to_string(&leg["target"])?,id,turn])?;
            }
        }
        Ok(())
    }
    pub fn merge_submitting(&self, turn: i64) -> Result<()> {
        for id in self.merge_local(turn)? {
            let leg = self.merge_leg(&id)?;
            if leg["phase"] == "dispatching" {
                let conn = self.conn.lock().expect("store mutex poisoned");
                check(&conn, &leg["target"])?;
                conn.execute(
                    "UPDATE merge_legs SET phase='submitting' WHERE id=? AND phase='dispatching'",
                    [id],
                )?;
            }
        }
        Ok(())
    }
    pub fn merge_accept(&self, id: &str, native: &str) -> Result<()> {
        self.conn.lock().expect("store mutex poisoned").execute("UPDATE merge_legs SET phase='accepted',native_turn=? WHERE id=? AND phase IN ('submitting','steering','unconfirmed') AND (native_turn IS NULL OR native_turn=?)",params![native,id,native])?;
        self.merge_sync(id)
    }
    pub fn merge_input_accepted(&self, turn: i64, native: &str) -> Result<()> {
        for id in self.merge_local(turn)? {
            if self.merge_leg(&id)?["phase"] == "submitting" {
                self.merge_accept(&id, native)?;
            }
        }
        Ok(())
    }
    fn merge_sync(&self, id: &str) -> Result<()> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        conn.execute("UPDATE merge_legs SET reply=(SELECT e.reply FROM merge_executions e WHERE e.local_turn=merge_legs.local_turn AND e.native_turn=merge_legs.native_turn),phase=CASE (SELECT e.terminal FROM merge_executions e WHERE e.local_turn=merge_legs.local_turn AND e.native_turn=merge_legs.native_turn) WHEN 'completed' THEN CASE WHEN (SELECT e.reply FROM merge_executions e WHERE e.local_turn=merge_legs.local_turn AND e.native_turn=merge_legs.native_turn) IS NOT NULL THEN 'observed' ELSE 'accepted' END WHEN 'failed' THEN 'accepted_failed' WHEN 'interrupted' THEN 'accepted_interrupted' ELSE 'accepted' END,reason=(SELECT CASE WHEN e.terminal IS NOT NULL THEN 'Native input accepted; terminal: '||e.terminal||'; semantic incorporation requires separate evidence' ELSE NULL END FROM merge_executions e WHERE e.local_turn=merge_legs.local_turn AND e.native_turn=merge_legs.native_turn) WHERE id=? AND phase='accepted'",[id])?;
        Ok(())
    }
    pub fn merge_answer(&self, turn: i64, native: &str, text: &str) -> Result<()> {
        if text.trim().is_empty() {
            return Ok(());
        }
        let ids = self.merge_local(turn)?;
        if ids.is_empty() {
            return Ok(());
        }
        self.conn.lock().expect("store mutex poisoned").execute("INSERT INTO merge_executions(local_turn,native_turn,reply) VALUES(?,?,?) ON CONFLICT(local_turn,native_turn) DO UPDATE SET reply=excluded.reply",params![turn,native,text])?;
        for id in ids {
            self.merge_sync(&id)?;
        }
        Ok(())
    }
    pub fn merge_terminal(&self, turn: i64, native: &str, status: &str) -> Result<()> {
        let ids = self.merge_local(turn)?;
        if ids.is_empty() {
            return Ok(());
        }
        self.conn.lock().expect("store mutex poisoned").execute("INSERT INTO merge_executions(local_turn,native_turn,terminal) VALUES(?,?,?) ON CONFLICT(local_turn,native_turn) DO UPDATE SET terminal=excluded.terminal",params![turn,native,status])?;
        for id in ids {
            self.merge_sync(&id)?;
        }
        Ok(())
    }
    pub fn merge_finish(&self, id: &str) -> Result<()> {
        let leg = self.merge_leg(id)?;
        match leg["phase"].as_str() {
            Some("queued") => {
                self.conn.lock().expect("store mutex poisoned").execute(
                    "UPDATE merge_legs SET phase='detached_queue',reason='Worker ended before dispatch; explicit recovery required' WHERE id=? AND phase='queued' AND local_turn IS NULL AND native_turn IS NULL AND instance_id=?",
                    params![id,self.instance_id],
                )?;
                Ok(())
            }
            Some("dispatching") => {
                self.merge_phase(id, "rejected", "Worker ended before native submission")
            }
            Some("submitting" | "steering") => self.merge_phase(
                id,
                "unconfirmed",
                "Native acknowledgement unavailable; do not replay",
            ),
            _ => Ok(()),
        }
    }
    pub fn merge_status(&self, key: SessionKey, human: i64) -> Result<String> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let mut stmt=conn.prepare("SELECT m.id,m.phase,json_extract(l.target,'$.title'),l.phase,l.reason FROM merge_operations m JOIN sessions origin ON origin.id=m.origin_session JOIN users u ON u.tg_user_id=m.requester LEFT JOIN merge_legs l ON l.operation=m.id WHERE m.requester=?3 AND u.allowed=1 AND origin.creator_user_id=?3 AND origin.chat_id=?1 AND (origin.thread_id=?2 OR (json_extract(l.target,'$.key.thread_id')=?2 AND json_extract(l.target,'$.key.chat_id')=?1 AND EXISTS(SELECT 1 FROM sessions d WHERE d.id=json_extract(l.target,'$.session') AND d.creator_user_id=?3))) ORDER BY m.created_at DESC,m.id,l.id LIMIT 20")?;
        let rows = stmt.query_map(params![key.chat_id, key.thread_id, human], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, Option<String>>(4)?,
            ))
        })?;
        let mut lines =
            vec!["Context merges (native receipt is separate from semantic incorporation):".into()];
        for row in rows {
            let (op, phase, leg, state, reason) = row?;
            let state = state.unwrap_or(phase);
            let label = match state.as_str() {
                "preparing" => "Preparing source context",
                "preparation_failed" => "Preparation failed before destination mutation",
                "creating_topic" => "Creating new topic; receipt pending",
                "queued" => "Queued for native incorporation",
                "retained_queue" => "Unsent queue retained after restart",
                "detached_queue" => "Waiting for existing detached writer",
                "dispatching" => "Preparing native input",
                "submitting" | "steering" => "Awaiting native acceptance receipt",
                "accepted" => "Native input accepted; contextual response pending",
                "observed" => {
                    "Native response observed; semantic incorporation requires separate evidence"
                }
                "accepted_failed" => "Native input accepted; turn failed",
                "accepted_interrupted" => "Native input accepted; turn interrupted",
                "unconfirmed" => "Delivery unconfirmed; inspect without replay",
                "rejected" => "Known non-admission",
                s => s,
            };
            lines.push(format!(
                "{op} · {} · {}{}",
                leg.unwrap_or_else(|| "preparation".into()),
                label,
                reason
                    .filter(|s| !s.is_empty())
                    .map(|s| format!(" · {s}"))
                    .unwrap_or_default()
            ));
        }
        if lines.len() == 1 {
            lines.push("No context merges in this topic.".into());
        }
        Ok(lines.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (tempfile::TempDir, Store, SessionKey, i64, Value) {
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
        let other = SessionKey::new(100, Some(8));
        let a = store.ensure_session(key, 100, &defaults).unwrap();
        let b = store.ensure_session(other, 100, &defaults).unwrap();
        store.set_session_codex_thread(key, "a").unwrap();
        store.set_session_codex_thread(other, "b").unwrap();
        let turn = store
            .record_turn_started(
                a.id,
                &TurnRequest {
                    session_key: key,
                    from_user_id: 100,
                    prompt: "Human work".into(),
                    runtime_instructions: None,
                    attachments: vec![],
                    review_mode: None,
                    override_search_mode: None,
                },
            )
            .unwrap();
        let doc = json!({"id":"op","human":100,"mode":"both","sources":[{"session":a.id,"key":key,"human":100,"native":"a"},{"session":b.id,"key":other,"human":100,"native":"b"}]});
        assert!(store.merge_begin(&doc, 1).unwrap());
        assert!(!store.merge_begin(&doc, 1).unwrap());
        store
            .merge_ready(
                &doc,
                &[(
                    "op:0".into(),
                    doc["sources"][0].clone(),
                    "Immutable context".into(),
                )],
            )
            .unwrap();
        (root, store, key, turn, doc)
    }
    #[test]
    fn unstarted_worker_retains_queue_for_explicit_recovery() {
        let (_root, store, key, _turn, _doc) = fixture();
        store.merge_finish("op:0").unwrap();
        store.merge_finish("op:0").unwrap();
        assert_eq!(store.merge_leg("op:0").unwrap()["phase"], "detached_queue");
        assert!(!store.merge_claim("op:0", "dispatching", None).unwrap());
        assert_eq!(store.merge_resume(key, 100, "op").unwrap(), vec!["op:0"]);
        assert!(store.merge_claim("op:0", "dispatching", None).unwrap());
        store.merge_finish("op:0").unwrap();
        assert_eq!(store.merge_leg("op:0").unwrap()["phase"], "rejected");
    }

    #[test]
    fn start_ack_cannot_accept_steering_and_fast_observations_survive_late_ack() {
        let (_root, store, _key, turn, _doc) = fixture();
        assert!(store.merge_claim("op:0", "steering", Some(turn)).unwrap());
        store.merge_input_accepted(turn, "native-run").unwrap();
        // The same start-only filtering applies to unrelated native rejection.
        for id in store.merge_local(turn).unwrap() {
            if store.merge_leg(&id).unwrap()["phase"] == "submitting" {
                store
                    .merge_phase(&id, "rejected", "Unrelated start rejection")
                    .unwrap();
            }
        }
        assert_eq!(store.merge_leg("op:0").unwrap()["phase"], "steering");
        store
            .merge_answer(
                turn,
                "native-run",
                "Received ideas; open disagreement retained",
            )
            .unwrap();
        store
            .merge_terminal(turn, "native-run", "completed")
            .unwrap();
        assert_eq!(store.merge_leg("op:0").unwrap()["phase"], "steering");
        store.merge_accept("op:0", "native-run").unwrap();
        let leg = store.merge_leg("op:0").unwrap();
        assert_eq!(leg["phase"], "observed");
        assert!(leg["reply"].as_str().unwrap().contains("open disagreement"));
    }
    #[test]
    fn failed_or_interrupted_terminal_keeps_acceptance_and_never_authorizes_replay() {
        for status in ["failed", "interrupted"] {
            let (_root, store, key, turn, _doc) = fixture();
            store.merge_claim("op:0", "steering", Some(turn)).unwrap();
            store.merge_terminal(turn, "native-run", status).unwrap();
            store.merge_accept("op:0", "native-run").unwrap();
            assert_eq!(
                store.merge_leg("op:0").unwrap()["phase"],
                format!("accepted_{status}")
            );
            assert!(store.merge_resume(key, 100, "op").unwrap().is_empty());
            assert_eq!(
                store.merge_leg("op:0").unwrap()["native_turn"],
                "native-run"
            );
        }
    }
    #[test]
    fn scope_binding_and_immutable_restart_recovery() {
        let (root, store, key, turn, doc) = fixture();
        assert!(store.merge_resume(key, 101, "op").is_err());
        assert!(
            store
                .merge_resume(SessionKey::new(100, Some(8)), 100, "op")
                .is_err()
        );
        assert!(
            store
                .merge_ready(
                    &doc,
                    &[(
                        "other".into(),
                        doc["sources"][0].clone(),
                        "Changed packet".into()
                    )]
                )
                .is_err()
        );
        store.set_session_codex_thread(key, "changed").unwrap();
        assert!(
            store
                .merge_claim("op:0", "dispatching", Some(turn))
                .is_err()
        );
        store.set_session_codex_thread(key, "a").unwrap();
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
        drop(store);
        let reopened = Store::open(&root.path().join("db"), &[100, 101], &defaults).unwrap();
        assert_eq!(
            reopened.merge_leg("op:0").unwrap()["phase"],
            "retained_queue"
        );
        assert!(!reopened.merge_claim("op:0", "dispatching", None).unwrap());
        assert_eq!(reopened.merge_resume(key, 100, "op").unwrap(), vec!["op:0"]);
        assert!(reopened.merge_claim("op:0", "dispatching", None).unwrap());
        assert!(!reopened.merge_claim("op:0", "dispatching", None).unwrap());
        assert_eq!(
            reopened.merge_leg("op:0").unwrap()["packet"],
            "Immutable context"
        );
    }
    #[test]
    fn restart_reconciles_durable_physical_evidence_without_input_replay() {
        let (root, store, key, turn, _doc) = fixture();
        store.merge_claim("op:0", "steering", Some(turn)).unwrap();
        store.merge_accept("op:0", "native-run").unwrap();
        // Simulate a process boundary after evidence persisted, before projection.
        store
            .conn
            .lock()
            .unwrap()
            .execute(
                "INSERT INTO merge_executions VALUES(?,?,?,'failed')",
                params![turn, "native-run", "Retained physical output"],
            )
            .unwrap();
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
        drop(store);
        let reopened = Store::open(&root.path().join("db"), &[100, 101], &defaults).unwrap();
        let leg = reopened.merge_leg("op:0").unwrap();
        assert_eq!(leg["phase"], "accepted_failed");
        assert_eq!(leg["reply"], "Retained physical output");
        assert!(reopened.merge_resume(key, 100, "op").unwrap().is_empty());
    }
    #[test]
    fn new_topic_template_cannot_overwrite_an_intervening_writer_or_foreign_owner() {
        let (_root, store, key, _turn, _doc) = fixture();
        let original = store.get_session(key).unwrap().unwrap();
        let fresh = SessionKey::new(100, Some(12));
        let defaults = SessionDefaults {
            cwd: original.cwd.clone(),
            model: None,
            reasoning_effort: None,
            session_prompt: None,
            sandbox_mode: "read-only".into(),
            approval_policy: "never".into(),
            search_mode: crate::config::SearchMode::Disabled,
            add_dirs: vec![],
        };
        store.ensure_session(fresh, 100, &defaults).unwrap();
        let precheck = store.get_session(fresh).unwrap().unwrap();
        assert!(!precheck.busy && precheck.codex_thread_id.is_none());
        // The other topic's first input arrives after the caller's read.
        store
            .set_session_codex_thread(fresh, "human-started-thread")
            .unwrap();
        assert!(
            store
                .apply_empty_session_template(fresh, &original, 100)
                .is_err()
        );
        assert_eq!(
            store
                .get_session(fresh)
                .unwrap()
                .unwrap()
                .codex_thread_id
                .as_deref(),
            Some("human-started-thread")
        );
        let foreign = SessionKey::new(100, Some(13));
        store.ensure_session(foreign, 101, &defaults).unwrap();
        assert!(
            store
                .apply_empty_session_template(foreign, &original, 100)
                .is_err()
        );
        assert_eq!(store.session_creator(foreign).unwrap(), 101);
        let empty = SessionKey::new(100, Some(14));
        store.ensure_session(empty, 100, &defaults).unwrap();
        assert!(
            store
                .apply_empty_session_template(empty, &original, 100)
                .is_ok()
        );
        for (topic, payload) in [
            (
                15,
                json!({"message":{"chat":{"id":100},"message_thread_id":15,"from":{"id":100},"text":"/model human-model"}}),
            ),
            (
                16,
                json!({"callback_query":{"from":{"id":100},"message":{"chat":{"id":100},"message_thread_id":16}}}),
            ),
        ] {
            let pending = SessionKey::new(100, Some(topic));
            store.ensure_session(pending, 100, &defaults).unwrap();
            let read = store.get_session(pending).unwrap().unwrap();
            assert!(!read.busy && read.codex_thread_id.is_none());
            store
                .conn
                .lock()
                .unwrap()
                .execute(
                    "INSERT INTO incoming_updates VALUES(?,?,'handled',?,NULL,?)",
                    params![topic, payload.to_string(), store.instance_id, now_string()],
                )
                .unwrap();
            assert!(
                store
                    .apply_empty_session_template(pending, &original, 100)
                    .is_err()
            );
            assert_eq!(
                store.get_session(pending).unwrap().unwrap().codex_thread_id,
                None
            );
        }
    }
}
