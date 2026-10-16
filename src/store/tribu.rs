//! Durable admission belongs to the topic writer, never to native context.
use super::*;
use anyhow::bail;
use serde_json::{Value, json};

#[cfg(test)]
mod tests;

#[derive(Debug, Clone)]
pub struct TribuAdmission {
    pub id: String,
    pub state: String,
    pub record: Value,
    pub fresh: bool,
}

impl Store {
    pub fn tribu_check_owner(&self, key: SessionKey, human: i64) -> Result<()> {
        let creator: i64 = self.conn.lock().expect("store mutex poisoned").query_row(
            "SELECT creator_user_id FROM sessions WHERE chat_id=? AND thread_id=?",
            params![key.chat_id, key.thread_id],
            |r| r.get(0),
        )?;
        if creator != human {
            bail!("Tribu destination belongs to a different human");
        }
        Ok(())
    }
    pub fn tribu_deadline(&self, id: &str, deadline: &str) -> Result<()> {
        self.conn.lock().expect("store mutex poisoned").execute(
            "UPDATE tribu_admissions SET deadline_at=? WHERE id=?",
            params![deadline, id],
        )?;
        Ok(())
    }

    pub fn tribu_execution(&self, id: &str) -> Result<Option<Value>> {
        self.conn.lock().expect("store mutex poisoned").query_row("SELECT id,state,native_thread,native_turn,terminal,deadline_at FROM tribu_admissions WHERE id=?",[id],|r|Ok(json!({"id":r.get::<_,String>(0)?,"state":r.get::<_,String>(1)?,"native_thread":r.get::<_,Option<String>>(2)?,"native_turn":r.get::<_,Option<String>>(3)?,"terminal":r.get::<_,Option<String>>(4)?,"deadline_at":r.get::<_,Option<String>>(5)?}))).optional().map_err(Into::into)
    }

    pub fn tribu_unsettled(&self, key: SessionKey) -> Result<Vec<Value>> {
        self.tribu_controllable(key, false)
    }

    pub fn tribu_controllable(&self, key: SessionKey, include_running: bool) -> Result<Vec<Value>> {
        let ids = {
            let conn = self.conn.lock().expect("store mutex poisoned");
            let mut statement=conn.prepare("SELECT id FROM tribu_admissions WHERE chat_id=? AND topic_id=? AND (state='undetermined' OR (?3 AND state='running')) LIMIT 100")?;
            statement
                .query_map(params![key.chat_id, key.thread_id, include_running], |r| {
                    r.get::<_, String>(0)
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        ids.iter()
            .map(|id| {
                self.tribu_execution(id)?
                    .context("missing unsettled execution")
            })
            .collect()
    }

    pub fn tribu_reconcile(&self, id: &str, native_turn: &str, status: &str) -> Result<()> {
        if !matches!(status, "completed" | "failed" | "interrupted") {
            return Ok(());
        }
        self.tribu_native_turn(id, native_turn, Some(status))?;
        self.conn.lock().expect("store mutex poisoned").execute(
            "UPDATE tribu_admissions SET state=?,updated_at=? WHERE id=? AND state='undetermined'",
            params![status, now_string(), id],
        )?;
        Ok(())
    }
    pub fn tribu_submitting(&self, key: SessionKey, id: &str) -> Result<()> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let n=conn.execute("UPDATE tribu_admissions SET submitted=1,updated_at=?1 WHERE id=?2 AND state='running' AND NOT EXISTS(SELECT 1 FROM bot_state WHERE key=?3 AND value='true') AND NOT EXISTS(SELECT 1 FROM bot_state WHERE key=?4)",params![now_string(),id,tribu_pause_key(key),format!("native_writer_hold:{}:{}",key.chat_id,key.thread_id)])?;
        if n != 1 {
            bail!("Tribu paused or native writer held before submission");
        }
        Ok(())
    }
    pub(super) fn init_tribu(&self) -> Result<()> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS tribu_admissions (
            id TEXT PRIMARY KEY, chat_id INTEGER NOT NULL, topic_id INTEGER NOT NULL,
            source TEXT, record TEXT NOT NULL, state TEXT NOT NULL,
            local_turn INTEGER, native_thread TEXT, native_turn TEXT,
            terminal TEXT, submitted INTEGER NOT NULL DEFAULT 0, deadline_at TEXT, created_at TEXT NOT NULL, updated_at TEXT NOT NULL
        ); CREATE UNIQUE INDEX IF NOT EXISTS tribu_original_destination
          ON tribu_admissions(source,chat_id,topic_id) WHERE source IS NOT NULL;",
        )?;
        // No worker survived restart. Queued work never reached native submission;
        // keep it visible but require another explicit request instead of replaying it.
        conn.execute("UPDATE tribu_admissions SET state=CASE WHEN terminal IN ('completed','failed','interrupted') THEN terminal WHEN submitted=0 THEN 'withdrawn-restart' ELSE 'undetermined' END,updated_at=?1 WHERE state IN ('queued','reserved','running')", [now_string()])?;
        Ok(())
    }

    pub fn tribu_admission(&self, id: &str) -> Result<Option<TribuAdmission>> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        conn.query_row(
            "SELECT id,state,record FROM tribu_admissions WHERE id=?",
            [id],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            },
        )
        .optional()?
        .map(|(id, state, record)| {
            Ok(TribuAdmission {
                id,
                state,
                record: serde_json::from_str(&record)?,
                fresh: false,
            })
        })
        .transpose()
    }

    pub fn tribu_admit(
        &self,
        key: SessionKey,
        id: &str,
        source: Option<&str>,
        record: &Value,
        max_pending: usize,
    ) -> Result<TribuAdmission> {
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let existing=tx.query_row("SELECT id,state,record,chat_id,topic_id FROM tribu_admissions WHERE id=?1 OR (source=?2 AND chat_id=?3 AND topic_id=?4)", params![id,source,key.chat_id,key.thread_id], |r| Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,String>(2)?,r.get::<_,i64>(3)?,r.get::<_,i64>(4)?))).optional()?;
        if let Some((old_id, state, raw, chat, topic)) = existing {
            if old_id != id || chat != key.chat_id || topic != key.thread_id {
                bail!("Tribu admission destination conflict");
            }
            return Ok(TribuAdmission {
                id: old_id,
                state,
                record: serde_json::from_str(&raw)?,
                fresh: false,
            });
        }
        let paused = tx
            .query_row(
                "SELECT value FROM bot_state WHERE key=?",
                [tribu_pause_key(key)],
                |r| r.get::<_, String>(0),
            )
            .optional()?
            .is_some_and(|v| v == "true");
        let uncertain:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM tribu_admissions WHERE chat_id=? AND topic_id=? AND state='undetermined')",params![key.chat_id,key.thread_id],|r|r.get(0))?;
        let pending:i64=tx.query_row("SELECT COUNT(*) FROM tribu_admissions WHERE chat_id=? AND topic_id=? AND state IN ('queued','reserved','running')",params![key.chat_id,key.thread_id],|r|r.get(0))?;
        let state = if paused {
            "withdrawn-paused"
        } else if uncertain {
            "blocked-uncertain"
        } else if pending >= max_pending as i64 {
            "retained-overflow"
        } else {
            "queued"
        };
        tx.execute("INSERT INTO tribu_admissions(id,chat_id,topic_id,source,record,state,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?)",params![id,key.chat_id,key.thread_id,source,serde_json::to_string(record)?,state,now_string(),now_string()])?;
        tx.commit()?;
        Ok(TribuAdmission {
            id: id.to_string(),
            state: state.to_string(),
            record: record.clone(),
            fresh: true,
        })
    }

    pub fn tribu_reserve(&self, key: SessionKey, id: &str) -> Result<bool> {
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let blocked:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM tribu_admissions WHERE chat_id=?1 AND topic_id=?2 AND state IN ('reserved','running','undetermined')) OR EXISTS(SELECT 1 FROM bot_state WHERE key=?3 AND value='true')",params![key.chat_id,key.thread_id,tribu_pause_key(key)],|r|r.get(0))?;
        if blocked {
            tx.execute("UPDATE tribu_admissions SET state='withdrawn-blocked',updated_at=?1 WHERE id=?2 AND state='queued'",params![now_string(),id])?;
            tx.commit()?;
            return Ok(false);
        }
        let n=tx.execute("UPDATE tribu_admissions SET state='reserved',updated_at=?1 WHERE id=?2 AND state='queued' AND chat_id=?3 AND topic_id=?4",params![now_string(),id,key.chat_id,key.thread_id])?;
        tx.commit()?;
        Ok(n == 1)
    }

    /// Explicit owner recovery of a public input provably never submitted.
    /// Current human source authentication belongs to the ingress adapter;
    /// possible native effects, pause and destination fences remain durable.
    pub fn tribu_retry_unsubmitted(
        &self,
        key: SessionKey,
        id: &str,
        source: &str,
        record: &Value,
        max_pending: usize,
    ) -> Result<TribuAdmission> {
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let (chat, topic, original, state, submitted, local, thread, turn, terminal) = tx.query_row(
            "SELECT chat_id,topic_id,source,state,submitted,local_turn,native_thread,native_turn,terminal FROM tribu_admissions WHERE id=?",
            [id], |r| Ok((r.get::<_,i64>(0)?,r.get::<_,i64>(1)?,r.get::<_,Option<String>>(2)?,
                r.get::<_,String>(3)?,r.get::<_,bool>(4)?,r.get::<_,Option<i64>>(5)?,
                r.get::<_,Option<String>>(6)?,r.get::<_,Option<String>>(7)?,r.get::<_,Option<String>>(8)?)))?;
        if chat != key.chat_id
            || topic != key.thread_id
            || original.as_deref() != Some(source)
            || record["id"].as_str() != Some(source)
            || record["private"] == true
        {
            bail!("Tribu retry destination or source mismatch");
        }
        if matches!(state.as_str(), "queued" | "reserved" | "running") {
            let old = tx.query_row(
                "SELECT record FROM tribu_admissions WHERE id=?",
                [id],
                |r| r.get::<_, String>(0),
            )?;
            return Ok(TribuAdmission {
                id: id.into(),
                state,
                record: serde_json::from_str(&old)?,
                fresh: false,
            });
        }
        if submitted
            || local.is_some()
            || thread.is_some()
            || turn.is_some()
            || terminal.is_some()
            || !matches!(
                state.as_str(),
                "blocked-uncertain"
                    | "withdrawn-blocked"
                    | "withdrawn-restart"
                    | "withdrawn-worker"
                    | "withdrawn-native-writer"
                    | "not-submitted-source-unavailable"
            )
        {
            bail!("Tribu retry requires proven absence of native submission");
        }
        let blocked: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM bot_state WHERE key=?1 AND value='true') OR EXISTS(SELECT 1 FROM tribu_admissions WHERE chat_id=?2 AND topic_id=?3 AND state='undetermined') OR EXISTS(SELECT 1 FROM bot_state WHERE key=?4)",
            params![tribu_pause_key(key),key.chat_id,key.thread_id,format!("native_writer_hold:{}:{}",key.chat_id,key.thread_id)], |r| r.get(0))?;
        let pending: i64 = tx.query_row(
            "SELECT COUNT(*) FROM tribu_admissions WHERE chat_id=? AND topic_id=? AND state IN ('queued','reserved','running')",
            params![key.chat_id,key.thread_id], |r| r.get(0))?;
        if blocked || pending >= max_pending as i64 {
            bail!(
                "Tribu retry blocked by pause, native writer hold, uncertain execution or full queue"
            );
        }
        tx.execute(
            "UPDATE tribu_admissions SET state='queued',record=?,updated_at=? WHERE id=?",
            params![serde_json::to_string(record)?, now_string(), id],
        )?;
        tx.commit()?;
        Ok(TribuAdmission {
            id: id.into(),
            state: "queued".into(),
            record: record.clone(),
            fresh: true,
        })
    }

    pub fn tribu_withdraw(&self, id: &str, reason: &str) -> Result<()> {
        self.conn.lock().expect("store mutex poisoned").execute("UPDATE tribu_admissions SET state=?1,updated_at=?2 WHERE id=?3 AND state IN ('queued','reserved')",params![reason,now_string(),id])?;
        Ok(())
    }

    pub fn tribu_refresh(&self, id: &str, record: &Value) -> Result<()> {
        let n=self.conn.lock().expect("store mutex poisoned").execute("UPDATE tribu_admissions SET record=?,updated_at=? WHERE id=? AND state='reserved' AND local_turn IS NULL",params![serde_json::to_string(record)?,now_string(),id])?;
        if n != 1 {
            bail!("Tribu request was already submitted or withdrawn");
        }
        Ok(())
    }

    pub fn tribu_link_turn(&self, id: &str, local_turn: i64) -> Result<()> {
        self.conn.lock().expect("store mutex poisoned").execute("UPDATE tribu_admissions SET local_turn=?1,state='running',updated_at=?2 WHERE id=?3 AND state='reserved'",params![local_turn,now_string(),id])?;
        Ok(())
    }

    pub fn tribu_native_thread(&self, key: SessionKey, id: &str, thread: &str) -> Result<()> {
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let pin_key = tribu_thread_key(key);
        let pin = tx
            .query_row("SELECT value FROM bot_state WHERE key=?", [&pin_key], |r| {
                r.get::<_, String>(0)
            })
            .optional()?;
        if pin.as_deref().is_some_and(|p| p != thread) {
            bail!("Tribu native thread changed unexpectedly");
        }
        tx.execute(
            "INSERT OR IGNORE INTO bot_state(key,value) VALUES(?,?)",
            params![pin_key, thread],
        )?;
        tx.execute(
            "UPDATE tribu_admissions SET native_thread=?,updated_at=? WHERE id=?",
            params![thread, now_string(), id],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn tribu_check_thread(&self, session: &SessionRecord) -> Result<()> {
        let pin = self.bot_state_value(&tribu_thread_key(session.key))?;
        if let Some(pin) = pin {
            if session.codex_thread_id.as_deref() != Some(pin.as_str()) {
                bail!("Tribu native conversation binding differs from its preparation pin");
            }
        } else if session.codex_thread_id.is_some() || !session.force_fresh_thread {
            bail!("Tribu must begin with a new dedicated native thread");
        }
        Ok(())
    }

    pub fn tribu_native_turn(&self, id: &str, turn: &str, terminal: Option<&str>) -> Result<()> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let old = conn
            .query_row(
                "SELECT native_turn FROM tribu_admissions WHERE id=?",
                [id],
                |r| r.get::<_, Option<String>>(0),
            )
            .optional()?
            .flatten();
        if old.as_deref().is_some_and(|p| p != turn) {
            bail!("Tribu native turn changed unexpectedly");
        }
        conn.execute("UPDATE tribu_admissions SET native_turn=?,terminal=COALESCE(?,terminal),updated_at=? WHERE id=?",params![turn,terminal,now_string(),id])?;
        Ok(())
    }

    pub fn tribu_finish(&self, id: &str) -> Result<()> {
        // Terminal native evidence, not successful Telegram presentation, settles
        // model execution. A stream error or interrupt ack alone does not.
        self.conn.lock().expect("store mutex poisoned").execute("UPDATE tribu_admissions SET state=CASE WHEN submitted=0 THEN 'not-submitted' WHEN terminal IN ('completed','failed','interrupted') THEN terminal ELSE 'undetermined' END,updated_at=? WHERE id=? AND state IN ('reserved','running')",params![now_string(),id])?;
        Ok(())
    }

    pub fn tribu_pause(&self, key: SessionKey, paused: bool) -> Result<()> {
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        tx.execute("INSERT INTO bot_state(key,value) VALUES(?,?) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![tribu_pause_key(key),paused.to_string()])?;
        if paused {
            tx.execute("UPDATE tribu_admissions SET state='withdrawn-paused',updated_at=? WHERE chat_id=? AND topic_id=? AND state='queued'",params![now_string(),key.chat_id,key.thread_id])?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn tribu_status(&self, key: SessionKey) -> Result<Value> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let paused = conn
            .query_row(
                "SELECT value FROM bot_state WHERE key=?",
                [tribu_pause_key(key)],
                |r| r.get::<_, String>(0),
            )
            .optional()?
            .is_some_and(|v| v == "true");
        let mut statement=conn.prepare("SELECT id,state,native_thread,native_turn,terminal FROM tribu_admissions WHERE chat_id=? AND topic_id=? ORDER BY created_at DESC LIMIT 20")?;
        let rows=statement.query_map(params![key.chat_id,key.thread_id],|r|Ok(json!({"id":r.get::<_,String>(0)?,"state":r.get::<_,String>(1)?,"native_thread":r.get::<_,Option<String>>(2)?,"native_turn":r.get::<_,Option<String>>(3)?,"terminal":r.get::<_,Option<String>>(4)?})))?.collect::<rusqlite::Result<Vec<_>>>()?;
        let unknown:i64=conn.query_row("SELECT COUNT(*) FROM tribu_admissions WHERE chat_id=? AND topic_id=? AND state='undetermined'",params![key.chat_id,key.thread_id],|r|r.get(0))?;
        Ok(json!({"paused":paused,"undetermined_count":unknown,"recent":rows}))
    }
}

fn tribu_pause_key(key: SessionKey) -> String {
    format!("tribu_pause:{}:{}", key.chat_id, key.thread_id)
}
fn tribu_thread_key(key: SessionKey) -> String {
    format!("tribu_thread:{}:{}", key.chat_id, key.thread_id)
}
