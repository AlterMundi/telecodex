//! Human-scoped, durable Stop operations. A native fence is not host quiescence.
use super::*;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StopTarget {
    pub key: SessionKey,
    pub native: Option<String>,
    pub was_busy: bool,
    #[serde(default)]
    pub resume_goal: bool,
    pub before: Option<crate::codex::NativeGoal>,
    pub paused: Option<crate::codex::NativeGoal>,
    pub phase: String,
    pub reason: String,
    pub resume_turn: Option<i64>,
    pub physical: Option<String>,
    #[serde(default)]
    pub children: Vec<String>,
    #[serde(default)]
    pub child_goals: std::collections::HashMap<String, crate::codex::NativeGoal>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StopOperation {
    pub id: String,
    pub human: i64,
    pub phase: String,
    pub targets: Vec<StopTarget>,
}

pub(super) fn admission_epoch_on(conn: &Connection, key: SessionKey) -> Result<String> {
    Ok(conn.query_row("SELECT COALESCE(b.value,'initial') FROM sessions s LEFT JOIN bot_state b ON b.key='stop_admission_epoch:'||s.creator_user_id WHERE s.chat_id=? AND s.thread_id=?",params![key.chat_id,key.thread_id],|r|r.get(0))?)
}

pub(super) fn fenced_on(conn: &Connection, key: SessionKey) -> Result<bool> {
    Ok(conn.query_row("SELECT EXISTS(SELECT 1 FROM sessions s JOIN stop_operations o ON o.human=s.creator_user_id WHERE s.chat_id=? AND s.thread_id=? AND o.phase NOT IN ('resumed','resumed_with_failures','released','superseded'))",params![key.chat_id,key.thread_id],|r|r.get(0))?)
}
impl Store {
    pub(super) fn init_stopping(&self) -> Result<()> {
        self.conn.lock().expect("store mutex poisoned").execute_batch("CREATE TABLE IF NOT EXISTS stop_operations(id TEXT PRIMARY KEY,human INTEGER NOT NULL,nonce TEXT NOT NULL,phase TEXT NOT NULL,document TEXT NOT NULL,UNIQUE(human,nonce));")?;
        // Retain fences on restart. Never replay a native mutation or continuation.
        self.conn.lock().expect("store mutex poisoned").execute(
            "UPDATE stop_operations SET phase='unconfirmed' WHERE phase IN ('stopping','resuming')",
            [],
        )?;
        Ok(())
    }
    pub fn stop_begin(&self, human: i64, nonce: &str) -> Result<(StopOperation, bool)> {
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction()?;
        let allowed: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM users WHERE tg_user_id=? AND allowed=1)",
            [human],
            |r| r.get(0),
        )?;
        if !allowed {
            anyhow::bail!("Stop owner is not allowed");
        }
        let existing:Option<(String,String,String)>=tx.query_row("SELECT document,phase,nonce FROM stop_operations WHERE human=? AND (nonce=? OR phase NOT IN ('resumed','resumed_with_failures','released','superseded')) ORDER BY CASE WHEN nonce=? THEN 0 ELSE 1 END,rowid DESC LIMIT 1",params![human,nonce,nonce],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
        if let Some((doc, phase, prior_nonce)) = existing {
            let mut old: StopOperation = serde_json::from_str(&doc)?;
            old.phase = phase;
            if prior_nonce == nonce || !old.targets.iter().any(|t| t.phase.starts_with("resume_")) {
                return Ok((old, false));
            }
            tx.execute(
                "UPDATE stop_operations SET phase='superseded' WHERE id=?",
                [old.id],
            )?;
        }
        let targets = {
            let mut q=tx.prepare("SELECT chat_id,thread_id,codex_thread_id,busy FROM sessions WHERE creator_user_id=? ORDER BY id LIMIT 129")?;
            q.query_map([human], |r| {
                Ok(StopTarget {
                    key: SessionKey::new(r.get(0)?, Some(r.get(1)?)),
                    native: r.get(2)?,
                    was_busy: r.get(3)?,
                    resume_goal: false,
                    before: None,
                    paused: None,
                    phase: "requested".into(),
                    reason: String::new(),
                    resume_turn: None,
                    physical: None,
                    children: vec![],
                    child_goals: std::collections::HashMap::new(),
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?
        };
        if targets.is_empty() || targets.len() > 128 {
            anyhow::bail!("Stop requires between one and 128 owned sessions");
        }
        let op = StopOperation {
            id: Uuid::now_v7().to_string(),
            human,
            phase: "stopping".into(),
            targets,
        };
        tx.execute(
            "INSERT INTO stop_operations VALUES(?,?,?,?,?)",
            params![op.id, human, nonce, op.phase, serde_json::to_string(&op)?],
        )?;
        tx.execute("INSERT INTO bot_state(key,value) VALUES(?,?) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![format!("stop_admission_epoch:{human}"),op.id])?;
        for t in &op.targets {
            let intent = format!("goal_intent:{}:{}", t.key.chat_id, t.key.thread_id);
            tx.execute("INSERT INTO bot_state(key,value) VALUES(?,?) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![intent,format!("stop:{}",op.id)])?;
        }
        tx.execute("INSERT INTO bot_state(key,value) SELECT 'stopped_turn:'||t.id,? FROM turns t JOIN sessions s ON s.id=t.session_id WHERE s.creator_user_id=? AND t.status='running' ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![op.id,human])?;
        tx.commit()?;
        Ok((op, true))
    }
    /// An explicit maintenance request survives one restart, never retries effects.
    pub fn stop_arm_restart(&self, human: i64, id: &str) -> Result<bool> {
        let op = self.stop_read(human, id)?;
        if op.phase != "stopped" {
            anyhow::bail!("Restart requires an observed stopped operation");
        }
        let conn = self.conn.lock().expect("store mutex poisoned");
        Ok(conn.execute("INSERT INTO bot_state(key,value) SELECT ?,? WHERE EXISTS(SELECT 1 FROM stop_operations WHERE id=? AND human=? AND phase='stopped') ON CONFLICT(key) DO NOTHING",params![format!("stop_restart:{id}"),format!("armed:{}",self.instance_id),id,human])? == 1)
    }
    pub fn stop_claim_restarts(&self) -> Result<Vec<(i64, String)>> {
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction()?;
        let rows = {
            let mut q=tx.prepare("SELECT o.human,o.id FROM stop_operations o JOIN bot_state b ON b.key='stop_restart:'||o.id JOIN users u ON u.tg_user_id=o.human WHERE b.value LIKE 'armed:%' AND b.value!=? AND u.allowed=1")?;
            q.query_map([format!("armed:{}", self.instance_id)], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })?
            .collect::<rusqlite::Result<Vec<(i64, String)>>>()?
        };
        for (_, id) in &rows {
            tx.execute(
                "UPDATE bot_state SET value='claimed' WHERE key=? AND value LIKE 'armed:%'",
                [format!("stop_restart:{id}")],
            )?;
        }
        tx.commit()?;
        Ok(rows)
    }
    pub fn stop_latest(&self, human: i64) -> Result<Option<String>> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        Ok(conn.query_row("SELECT id FROM stop_operations WHERE human=? AND phase!='superseded' ORDER BY rowid DESC LIMIT 1",[human],|r|r.get(0)).optional()?)
    }
    pub fn stop_read(&self, human: i64, id: &str) -> Result<StopOperation> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let doc:String=conn.query_row("SELECT document FROM stop_operations WHERE id=? AND human=? AND EXISTS(SELECT 1 FROM users WHERE tg_user_id=? AND allowed=1)",params![id,human,human],|r|r.get(0)).context("No authorized Stop operation with that ID")?;
        let mut op: StopOperation = serde_json::from_str(&doc)?;
        op.phase = conn.query_row("SELECT phase FROM stop_operations WHERE id=?", [id], |r| {
            r.get(0)
        })?;
        Ok(op)
    }
    pub fn stop_save(&self, op: &StopOperation, expected: &str) -> Result<()> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        if conn.execute(
            "UPDATE stop_operations SET phase=?,document=? WHERE id=? AND human=? AND phase=?",
            params![
                op.phase,
                serde_json::to_string(op)?,
                op.id,
                op.human,
                expected
            ],
        )? != 1
        {
            anyhow::bail!("Stop operation changed; inspect status");
        }
        Ok(())
    }
    pub fn stop_binding(&self, human: i64, target: &StopTarget) -> Result<()> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let valid:bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM sessions s JOIN users u ON u.tg_user_id=s.creator_user_id WHERE s.chat_id=? AND s.thread_id=? AND s.creator_user_id=? AND u.allowed=1 AND s.codex_thread_id IS ?)",params![target.key.chat_id,target.key.thread_id,human,target.native],|r|r.get(0))?;
        if !valid {
            anyhow::bail!("Stop destination binding or owner changed");
        }
        Ok(())
    }
    /// Admission epochs survive release and restart: old queue entries never revive.
    pub fn stop_admission_epoch(&self, key: SessionKey) -> Result<String> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        admission_epoch_on(&conn, key)
    }
    pub fn stop_fenced(&self, key: SessionKey) -> Result<bool> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        fenced_on(&conn, key)
    }
    pub fn stop_validate_resume(&self, id: &str, key: SessionKey, human: i64) -> Result<()> {
        let op = self.stop_read(human, id)?;
        if op.phase != "resuming" {
            anyhow::bail!("Resume operation no longer admits native work");
        }
        let target = op
            .targets
            .iter()
            .find(|t| t.key == key)
            .context("Resume is outside this Stop scope")?;
        if !matches!(
            target.phase.as_str(),
            "resume_intent" | "resume_queued" | "resume_submitting"
        ) {
            anyhow::bail!("Resume input already transmitted or unconfirmed");
        }
        self.stop_binding(human, target)?;
        if self
            .bot_state_value(&format!(
                "native_writer_hold:{}:{}",
                key.chat_id, key.thread_id
            ))?
            .is_some()
        {
            anyhow::bail!("Independent maintenance hold remains");
        }
        Ok(())
    }
    pub fn stop_retain_input(&self, id: i64) -> Result<()> {
        self.conn.lock().expect("store mutex poisoned").execute("UPDATE incoming_updates SET status='stop_held',updated_at=? WHERE update_id=? AND turn_id IS NULL AND status IN ('processing','queued')",params![now_string(),id])?;
        Ok(())
    }
    pub fn stop_resume_event(
        &self,
        id: &str,
        key: SessionKey,
        phase: &str,
        local: Option<i64>,
        native: Option<&str>,
    ) -> Result<()> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let doc: String = conn.query_row(
            "SELECT document FROM stop_operations WHERE id=?",
            [id],
            |r| r.get(0),
        )?;
        let mut op: StopOperation = serde_json::from_str(&doc)?;
        let t = op
            .targets
            .iter_mut()
            .find(|t| t.key == key)
            .context("Resume target missing")?;
        if phase == "resume_queued" && t.phase != "resume_intent" {
            return Ok(());
        }
        if phase == "resume_accepted" && t.phase.starts_with("resume_turn_") {
            return Ok(());
        }
        if let Some(local) = local {
            if t.resume_turn.is_some_and(|old| old != local) {
                anyhow::bail!("Resume physical execution changed");
            }
        }
        t.phase = phase.to_string();
        if local.is_some() {
            t.resume_turn = local;
        }
        if let Some(native) = native {
            t.physical = Some(native.into());
        }
        let current: String =
            conn.query_row("SELECT phase FROM stop_operations WHERE id=?", [id], |r| {
                r.get(0)
            })?;
        if matches!(
            current.as_str(),
            "resuming" | "resumed" | "resumed_with_failures"
        ) && op.targets.iter().all(|t| {
            matches!(
                t.phase.as_str(),
                "resume_accepted"
                    | "resume_turn_completed"
                    | "resume_turn_failed"
                    | "resume_turn_interrupted"
                    | "resume_unchanged"
            )
        }) {
            op.phase = if op.targets.iter().any(|t| {
                matches!(
                    t.phase.as_str(),
                    "resume_turn_failed" | "resume_turn_interrupted"
                )
            }) {
                "resumed_with_failures"
            } else {
                "resumed"
            }
            .into();
        } else {
            op.phase = current;
        }
        conn.execute(
            "UPDATE stop_operations SET phase=?,document=? WHERE id=?",
            params![op.phase, serde_json::to_string(&op)?, id],
        )?;
        Ok(())
    }
    pub fn stop_metadata(&self, human: i64) -> Result<Value> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let mut q=conn.prepare("SELECT id,phase FROM stop_operations WHERE human=? AND phase NOT IN ('resumed','resumed_with_failures','released','superseded') ORDER BY rowid DESC")?;
        let rows = q
            .query_map([human], |r| {
                Ok(json!({"id":r.get::<_,String>(0)?,"state":r.get::<_,String>(1)?}))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(json!(rows))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::tests::defaults;
    #[test]
    fn stop_scope_new_topics_restart_and_nonce_are_durable() {
        let tmp = tempfile::tempdir().unwrap();
        let db = tmp.path().join("stop.sqlite");
        let store = Store::open(&db, &[100, 101], &defaults()).unwrap();
        let owned = SessionKey::new(100, Some(7));
        let other = SessionKey::new(101, Some(8));
        store.ensure_session(owned, 100, &defaults()).unwrap();
        store.ensure_session(other, 101, &defaults()).unwrap();
        let (op, fresh) = store.stop_begin(100, "human-request").unwrap();
        assert!(fresh);
        assert_eq!(op.targets.len(), 1);
        assert!(store.stop_fenced(owned).unwrap());
        assert!(!store.stop_fenced(other).unwrap());
        let new = SessionKey::new(100, Some(9));
        store.ensure_session(new, 100, &defaults()).unwrap();
        assert!(store.stop_fenced(new).unwrap());
        assert!(store.stop_read(101, &op.id).is_err());
        assert_eq!(store.stop_begin(100, "again").unwrap().0.id, op.id);
        drop(store);
        let store = Store::open(&db, &[100, 101], &defaults()).unwrap();
        assert_eq!(store.stop_read(100, &op.id).unwrap().phase, "unconfirmed");
        assert!(store.stop_fenced(owned).unwrap());
        assert_eq!(store.stop_begin(100, "human-request").unwrap().0.id, op.id);
    }
    #[test]
    fn retired_admission_and_explicit_restart_request_survive_reboot_once() {
        let tmp = tempfile::tempdir().unwrap();
        let db = tmp.path().join("stop.sqlite");
        let store = Store::open(&db, &[100], &defaults()).unwrap();
        let key = SessionKey::new(100, Some(7));
        store.ensure_session(key, 100, &defaults()).unwrap();
        let before = store.stop_admission_epoch(key).unwrap();
        let update:crate::telegram::Update=serde_json::from_value(serde_json::json!({"update_id":10,"message":{"message_id":1,"date":1,"chat":{"id":100,"type":"private"},"from":{"id":100,"is_bot":false,"first_name":"Synthetic"},"message_thread_id":7,"text":"slow attachment"}})).unwrap();
        store.admit_update(&update).unwrap();

        let (mut op, _) = store.stop_begin(100, "stop").unwrap();
        assert_ne!(before, store.stop_admission_epoch(key).unwrap());
        assert_eq!(
            store.bot_state_value("stop_input_epoch:10").unwrap(),
            Some(before.clone())
        );

        op.phase = "stopped".into();
        store.stop_save(&op, "stopping").unwrap();
        assert!(store.stop_arm_restart(100, &op.id).unwrap());
        assert!(!store.stop_arm_restart(100, &op.id).unwrap());
        assert!(store.stop_claim_restarts().unwrap().is_empty()); // Not the same boot.
        drop(store);
        let store = Store::open(&db, &[100], &defaults()).unwrap();
        assert_eq!(
            store.stop_claim_restarts().unwrap(),
            vec![(100, op.id.clone())]
        );
        assert!(store.stop_claim_restarts().unwrap().is_empty());
        op.phase = "released".into();
        store.stop_save(&op, "stopped").unwrap();
        assert!(!store.stop_fenced(key).unwrap());
        assert_ne!(before, store.stop_admission_epoch(key).unwrap());
    }
    #[test]
    fn resume_does_not_open_admission_or_claim_delivery_from_queue_attempts() {
        let tmp = tempfile::tempdir().unwrap();
        let db = tmp.path().join("stop.sqlite");
        let store = Store::open(&db, &[100], &defaults()).unwrap();
        let key = SessionKey::new(100, Some(7));
        store.ensure_session(key, 100, &defaults()).unwrap();
        let (mut op, _) = store.stop_begin(100, "stop").unwrap();
        op.phase = "stopped".into();
        op.targets[0].phase = "native_stopped".into();
        store.stop_save(&op, "stopping").unwrap();
        op.phase = "resuming".into();
        op.targets[0].phase = "resume_intent".into();
        store.stop_save(&op, "stopped").unwrap();
        assert!(store.stop_fenced(key).unwrap());
        store.stop_validate_resume(&op.id, key, 100).unwrap();
        store
            .stop_resume_event(&op.id, key, "resume_queued", None, None)
            .unwrap();
        assert!(store.stop_fenced(key).unwrap());
        assert_eq!(store.stop_read(100, &op.id).unwrap().phase, "resuming");
        store
            .stop_resume_event(&op.id, key, "resume_accepted", Some(99), Some("physical"))
            .unwrap();
        assert!(!store.stop_fenced(key).unwrap());
        assert_eq!(store.stop_read(100, &op.id).unwrap().phase, "resumed");
        store
            .stop_resume_event(&op.id, key, "resume_queued", None, None)
            .unwrap();
        assert_eq!(
            store.stop_read(100, &op.id).unwrap().targets[0].phase,
            "resume_accepted"
        );
        assert!(store.stop_validate_resume(&op.id, key, 100).is_err());
        store
            .stop_resume_event(
                &op.id,
                key,
                "resume_turn_failed",
                Some(99),
                Some("physical"),
            )
            .unwrap();
        assert_eq!(
            store.stop_read(100, &op.id).unwrap().phase,
            "resumed_with_failures"
        );
        assert!(!store.stop_fenced(key).unwrap()); // An accepted failed turn is never replayed.
    }
}
