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
        self.interactions_page(key, user, 0)
    }

    pub fn interactions_page(
        &self,
        key: SessionKey,
        user: i64,
        offset: usize,
    ) -> Result<Vec<HumanInteraction>> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let mut stmt = conn.prepare("SELECT h.payload_json,h.state,s.codex_thread_id FROM human_interactions h
            JOIN sessions s ON s.id=h.session_id JOIN users u ON u.tg_user_id=h.requester
            WHERE s.chat_id=?1 AND s.thread_id=?2 AND h.requester=?3 AND u.allowed=1
            ORDER BY CASE WHEN h.state IN ('pending','answering') THEN 0 ELSE 1 END,h.created_at DESC,h.id DESC LIMIT 51 OFFSET ?4")?;
        stmt.query_map(
            params![key.chat_id, key.thread_id, user, offset as i64],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, Option<String>>(2)?,
                ))
            },
        )?
        .map(|row| {
            let (payload, state, current) = row?;
            let mut item: HumanInteraction = serde_json::from_str(&payload)?;
            item.state = if matches!(state.as_str(), "pending" | "answering" | "unknown")
                && current.as_deref() != Some(item.native_thread.as_str())
            {
                "source_changed".into()
            } else {
                state
            };
            Ok(item)
        })
        .collect()
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
            let mut item: HumanInteraction = serde_json::from_str(&payload)?;
            item.state = if matches!(state.as_str(), "pending" | "answering" | "unknown")
                && current.as_deref() != Some(item.native_thread.as_str())
            {
                "source_changed".into()
            } else {
                state
            };
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

    pub fn save_human_tasks(
        &self,
        key: SessionKey,
        tasks: &[(String, HumanInteraction)],
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
        tx.commit()?;
        Ok(())
    }

    pub fn update_native_interaction(&self, id: &str, title: &str, action: &str) -> Result<()> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        conn.execute("UPDATE human_interactions SET payload_json=json_set(payload_json,'$.title',?2,'$.action',?3),updated_at=?4
            WHERE id=?1 AND instance_id=?5 AND kind='native' AND state='pending'",
            params![id,title,action,now_string(),self.instance_id])?;
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
