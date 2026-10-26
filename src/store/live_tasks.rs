//! Scoped durable tasks: owner reads and live agent retirement; never native approval.
use super::*;
use anyhow::bail;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio_util::sync::CancellationToken;

pub struct LiveTaskActor<'a> {
    pub key: SessionKey,
    pub human: i64,
    pub local: i64,
    pub native: &'a str,
    pub physical: &'a str,
    /// Set by the receiver only for its actual integrating entry.
    pub integrator: bool,
}
pub(super) fn actor_on(
    conn: &Connection,
    actor: &LiveTaskActor<'_>,
    closed: &CancellationToken,
) -> Result<i64> {
    if closed.is_cancelled() || actor.physical.is_empty() || actor.physical.len() > 128 {
        bail!("Task controller execution expired");
    }
    let session:Option<i64>=conn.query_row("SELECT s.id FROM sessions s JOIN turns t ON t.session_id=s.id JOIN users u ON u.tg_user_id=s.creator_user_id
      WHERE s.chat_id=?1 AND s.thread_id=?2 AND s.creator_user_id=?3 AND u.allowed=1 AND s.force_fresh_thread=0
      AND s.codex_thread_id=?4 AND t.id=?5 AND t.from_user_id=?3 AND t.native_thread_id=?4
      AND t.status='running' AND t.completed_at IS NULL
      AND NOT EXISTS(SELECT 1 FROM interaction_native_messages m WHERE m.native_thread=?4 AND m.native_turn=?6 AND (m.phase='final_answer' OR m.phase IS NULL))
      AND NOT EXISTS(SELECT 1 FROM interaction_results r WHERE r.native_thread=?4 AND r.native_turn=?6)",
      params![actor.key.chat_id,actor.key.thread_id,actor.human,actor.native,actor.local,actor.physical],|r|r.get(0)).optional()?;
    session.context("Task controller has no current owned execution")
}
fn scoped_query(selection: &str) -> String {
    format!("SELECT h.payload_json,h.state,s.id,s.chat_id,s.thread_id FROM human_interactions h
      JOIN sessions s ON s.id=h.session_id JOIN turns t ON t.id=h.local_turn JOIN users u ON u.tg_user_id=h.requester
      WHERE {selection} AND h.kind='task' AND h.requester=?2 AND s.creator_user_id=?2 AND u.allowed=1
      AND h.native_thread=s.codex_thread_id AND s.force_fresh_thread=0 AND t.session_id=s.id
      AND t.from_user_id=?2 AND t.native_thread_id=h.native_thread
      AND (s.id=?6 OR (?5=1 AND EXISTS(SELECT 1 FROM interaction_instructions i JOIN interaction_sources x ON x.id=i.input_id
        WHERE i.session_id=s.id AND x.human_id=?2 AND x.origin_chat=?3 AND x.origin_thread=?4
        AND json_extract(i.doc,'$.local_turn')=h.local_turn)))")
}
fn revision(actor: &LiveTaskActor<'_>, item: &HumanInteraction, key: SessionKey) -> String {
    format!(
        "{:x}",
        Sha256::digest(
            json!([actor.key, actor.native, key, item])
                .to_string()
                .as_bytes()
        )
    )
}
fn owner_task_view(
    entry: SessionKey,
    native: &str,
    key: SessionKey,
    item: &HumanInteraction,
    full: bool,
) -> Value {
    let revision = format!(
        "{:x}",
        Sha256::digest(json!([entry, native, key, item]).to_string().as_bytes())
    );
    json!({"schema":"telecodex.conversation-task/v1","id":item.id,"revision":revision,
        "kind":"task","state":item.state,"target":key,"title":item.title,
        "preview":item.action.chars().take(240).collect::<String>(),
        "body":if full {json!(item.action)}else{Value::Null},"links":item.source["links"],
        "withdrawal":item.source["agentWithdrawal"],"dismissal":item.source["humanDismissal"],"can_dismiss":item.state=="pending","can_withdraw":false,"read_only":true})
}
fn actor_identity(actor: &LiveTaskActor<'_>, selected_revision: &str) -> Value {
    json!({"session":actor.key,"native_thread":actor.native,"task_revision":selected_revision,"physical_turn":actor.physical})
}
fn view(actor: &LiveTaskActor<'_>, key: SessionKey, item: &HumanInteraction, full: bool) -> Value {
    json!({"id":item.id,"revision":revision(actor,item,key),"kind":"task","state":item.state,
       "target":key,"title":item.title,"preview":item.action.chars().take(240).collect::<String>(),
       "action":if full {json!(item.action)} else {Value::Null},"withdrawal":item.source["agentWithdrawal"],
       "can_withdraw":item.state=="pending","meaning":"Agent-issued human action. Retirement is not human completion or acceptance."})
}
fn selected_on(
    conn: &Connection,
    actor: &LiveTaskActor<'_>,
    session: i64,
    id: &str,
) -> Result<(i64, SessionKey, HumanInteraction)> {
    let (raw, state, owner, chat, topic): (String, String, i64, i64, i64) = conn
        .query_row(
            &scoped_query("h.id=?1"),
            params![
                id,
                actor.human,
                actor.key.chat_id,
                actor.key.thread_id,
                actor.integrator,
                session
            ],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .context("Selected task is outside this current owned conversation")?;
    let mut item: HumanInteraction = serde_json::from_str(&raw)?;
    item.state = if state == "cancelled" && item.source["agentWithdrawal"].is_object() {
        "withdrawn".into()
    } else {
        state
    };
    Ok((owner, SessionKey::new(chat, Some(topic)), item))
}
impl Store {
    /// Owner read path for the same exact entry/routed-task scope as the controller.
    /// It does not impersonate a running controller or authorize task retirement.
    pub fn conversation_human_tasks(
        &self,
        human: i64,
        entry: SessionKey,
        after: i64,
    ) -> Result<Value> {
        if after < 0 {
            bail!("Invalid task cursor");
        }
        let conn = self.conn.lock().expect("store mutex poisoned");
        let (session, native) = super::presentation::entry_on(&conn, human, entry)?;
        let binding = super::presentation::entry_binding(&conn, human, entry, session)?;
        let query = (scoped_query("h.rowid>?1 AND h.state IN ('pending','answering','unknown')")
            + " ORDER BY h.rowid LIMIT 11")
            .replacen("SELECT h.payload_json", "SELECT h.rowid,h.payload_json", 1);
        let mut stmt = conn.prepare(&query)?;
        let rows = stmt
            .query_map(
                params![after, human, entry.chat_id, entry.thread_id, true, session],
                |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, i64>(4)?,
                        r.get::<_, i64>(5)?,
                    ))
                },
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let more = rows.len() > 10;
        let mut cursor = after;
        let mut items = Vec::new();
        for (sequence, raw, state, chat, topic) in rows.into_iter().take(10) {
            let mut item: HumanInteraction = serde_json::from_str(&raw)?;
            item.state = state;
            let mut result = owner_task_view(
                entry,
                &native,
                SessionKey::new(chat, Some(topic)),
                &item,
                false,
            );
            result["entry_binding"] = json!([human, entry, session, native]);
            result["binding"] = json!(binding);
            items.push(result);
            cursor = sequence;
        }
        Ok(
            json!({"schema":"telecodex.conversation-task-list/v1","entry_binding":[human,entry,session,native],
            "binding":binding,"items":items,"cursor":cursor,"more":more,"read_only":true}),
        )
    }
    pub fn conversation_human_task(
        &self,
        human: i64,
        entry: SessionKey,
        id: &str,
    ) -> Result<Value> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let (session, native) = super::presentation::entry_on(&conn, human, entry)?;
        let (raw, state, _, chat, topic): (String, String, i64, i64, i64) = conn
            .query_row(
                &scoped_query("h.id=?1"),
                params![id, human, entry.chat_id, entry.thread_id, true, session],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
            .context("Task is outside this current owned conversation")?;
        let mut item: HumanInteraction = serde_json::from_str(&raw)?;
        item.state = if state == "cancelled" && item.source["agentWithdrawal"].is_object() {
            "withdrawn".into()
        } else {
            state
        };
        let mut result = owner_task_view(
            entry,
            &native,
            SessionKey::new(chat, Some(topic)),
            &item,
            true,
        );
        result["entry_binding"] = json!([human, entry, session, native]);
        result["binding"] = json!(super::presentation::entry_binding(
            &conn, human, entry, session
        )?);
        Ok(result)
    }
    /// Explicit human dismissal reuses the task ledger; never native approval or work cancellation.
    pub fn conversation_dismiss_human_task(
        &self,
        human: i64,
        entry: SessionKey,
        binding: &str,
        id: &str,
        selected_revision: &str,
    ) -> Result<Value> {
        if selected_revision.len() != 64
            || !selected_revision.bytes().all(|c| c.is_ascii_hexdigit())
        {
            bail!("Exact selected task revision required");
        }
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let (session, native) = super::presentation::entry_on(&tx, human, entry)?;
        if super::presentation::entry_binding(&tx, human, entry, session)? != binding {
            bail!("Conversation changed; select its current task");
        }
        let (raw, state, chat, topic): (String, String, i64, i64) = tx
            .query_row(
                &scoped_query("h.id=?1"),
                params![id, human, entry.chat_id, entry.thread_id, true, session],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(3)?, row.get(4)?)),
            )
            .context("Task is outside this current owned conversation")?;
        let mut item: HumanInteraction = serde_json::from_str(&raw)?;
        item.state = state;
        let key = SessionKey::new(chat, Some(topic));
        let previous = &item.source["humanDismissal"];
        let repeated = item.state == "cancelled"
            && previous["revision"] == selected_revision
            && previous["binding"] == binding;
        if !repeated {
            if item.state != "pending"
                || owner_task_view(entry, &native, key, &item, false)["revision"]
                    != selected_revision
            {
                bail!("Task changed, is closed or has an uncertain human receipt");
            }
            let disposition = json!({"revision":selected_revision,"binding":binding,
                "at":now_string(),"human_completion":false,"work_cancelled":false});
            tx.execute(
                "UPDATE human_interactions SET state='cancelled',updated_at=?2,
                payload_json=json_set(payload_json,'$.source.humanDismissal',json(?3))
                WHERE id=?1 AND state='pending'",
                params![id, now_string(), disposition.to_string()],
            )?;
            tx.execute(
                "INSERT INTO audit_log(actor_user_id,action,details_json,created_at)
                VALUES(?1,'human_task_dismissed',?2,?3)",
                params![
                    human,
                    json!({"id":id,"entry":entry,"target":key,"revision":selected_revision,
                    "human_completion":false,"work_cancelled":false})
                    .to_string(),
                    now_string()
                ],
            )?;
        }
        tx.commit()?;
        Ok(
            json!({"schema":"telecodex.conversation-task-dismissal/v1","id":id,
            "revision":selected_revision,"binding":binding,"entry_binding":[human,entry,session,native],
            "target":key,"state":"cancelled","dismissed":true,"new_dismissal":!repeated,
            "human_completion":false,"work_cancelled":false}),
        )
    }

    pub fn live_human_tasks(
        &self,
        actor: &LiveTaskActor<'_>,
        after: i64,
        closed: &CancellationToken,
    ) -> Result<Value> {
        if after < 0 {
            bail!("Invalid task cursor");
        }
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction()?;
        let session = actor_on(&tx, actor, closed)?;
        let query = scoped_query("h.rowid>?1 AND h.state IN ('pending','answering','unknown')")
            + " ORDER BY h.rowid LIMIT 11";
        // Read rowids independently so the public cursor is not a task identifier.
        let query = query.replacen("SELECT h.payload_json", "SELECT h.rowid,h.payload_json", 1);
        let mut stmt = tx.prepare(&query)?;
        let rows = stmt
            .query_map(
                params![
                    after,
                    actor.human,
                    actor.key.chat_id,
                    actor.key.thread_id,
                    actor.integrator,
                    session
                ],
                |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, String>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, i64>(4)?,
                        r.get::<_, i64>(5)?,
                    ))
                },
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let more = rows.len() > 10;
        let mut items = Vec::new();
        let mut cursor = after;
        for (sequence, raw, state, chat, topic) in rows.into_iter().take(10) {
            let mut item: HumanInteraction = serde_json::from_str(&raw)?;
            item.state = state;
            items.push(view(
                actor,
                SessionKey::new(chat, Some(topic)),
                &item,
                false,
            ));
            cursor = sequence;
        }
        drop(stmt);
        tx.commit()?;
        Ok(json!({"kind":"human_tasks","items":items,"cursor":cursor,"more":more,"read_only":true}))
    }
    pub fn live_human_task(
        &self,
        actor: &LiveTaskActor<'_>,
        id: &str,
        closed: &CancellationToken,
    ) -> Result<Value> {
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction()?;
        let session = actor_on(&tx, actor, closed)?;
        let (_, key, item) = selected_on(&tx, actor, session, id)?;
        let result = view(actor, key, &item, true);
        tx.commit()?;
        Ok(result)
    }
    pub fn live_withdraw_task(
        &self,
        actor: &LiveTaskActor<'_>,
        selected_revision: &str,
        withdrawal: &TaskWithdrawal,
        closed: &CancellationToken,
    ) -> Result<Value> {
        if selected_revision.len() != 64
            || !selected_revision.bytes().all(|c| c.is_ascii_hexdigit())
        {
            bail!("Exact inspected task revision required");
        }
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let session = actor_on(&tx, actor, closed)?;
        let (owner, key, item) = selected_on(&tx, actor, session, &withdrawal.id)?;
        let identity = actor_identity(actor, selected_revision);
        // Exact known retry observes the existing withdrawal, not a fresh action.
        if item.state != "withdrawn"
            && (item.state != "pending" || revision(actor, &item, key) != selected_revision)
        {
            bail!("Task changed, resolved or has an uncertain human receipt");
        }
        let fresh = super::interactions::withdraw_task_on(
            &tx,
            owner,
            actor.human,
            &item.native_thread,
            actor.local,
            withdrawal,
            &identity,
        )?;
        if closed.is_cancelled() {
            bail!("Task controller closed before retirement");
        }
        tx.commit()?;
        Ok(
            json!({"kind":"task_withdrawal","id":item.id,"new_withdrawal":fresh,"state":"withdrawn","target":key,
            "reason":withdrawal.reason,"actor":identity,"human_completion":false}),
        )
    }
}
