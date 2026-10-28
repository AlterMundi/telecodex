//! Retained bounded context-return legs of the workflow journal, not another
//! execution queue. Existing entry workers and native collection own execution.
use super::*;

const MAX_NATIVE_RETURNS: usize = 16;
const MAX_ENVELOPE_BYTES: usize = 2048;
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct WorkflowReturn {
    pub id: String,
    pub workflow: String,
    pub envelope: Value,
    pub phase: String,
    pub local: Option<i64>,
    pub native_turn: Option<String>,
    pub instance: String,
}
#[derive(Debug, Clone)]
pub(crate) struct ReturnDispatch {
    pub id: String,
    pub human: i64,
    pub entry: SessionKey,
    pub native: String,
    pub epoch: String,
    pub output: String,
}
fn bindings_on(conn: &Connection, local: i64) -> Result<Vec<String>> {
    let raw: Option<String> = conn
        .query_row(
            "SELECT value FROM bot_state WHERE key=?",
            [format!("workflow_return_local:{local}")],
            |r| r.get(0),
        )
        .optional()?;
    Ok(raw
        .map(|s| serde_json::from_str(&s))
        .transpose()?
        .unwrap_or_default())
}
fn selected_on(conn: &Connection, id: &str) -> Result<(Workflow, usize)> {
    let raw:String=conn.query_row("SELECT doc FROM interaction_workflows w WHERE EXISTS(SELECT 1 FROM json_each(w.doc,'$.native_returns') r WHERE json_extract(r.value,'$.id')=?1)",[id],|r|r.get(0))?;
    let w: Workflow = serde_json::from_str(&raw)?;
    let index = w
        .native_returns
        .iter()
        .position(|r| r.id == id)
        .context("Missing workflow return")?;
    Ok((w, index))
}
fn current_on(conn: &Connection, w: &Workflow, index: usize, admission: bool) -> Result<bool> {
    let r = &w.native_returns[index];
    if admission
        && (w.phase != "active"
            || w.reported
                .iter()
                .any(|v| v.as_deref() == r.envelope["revision"].as_str()))
    {
        return Ok(false);
    }
    if !matches!(w.phase.as_str(), "active" | "completed")
        || (w.phase == "completed" && r.envelope["reason"] != "integration_required")
    {
        return Ok(false);
    }
    let mut current = w.clone();
    current.phase = "active".into();
    current.reported.fill(None);
    Ok(attention_on(conn, &current)?
        .iter()
        .any(|event| event["revision"] == r.envelope["revision"]))
}
fn later_human_on(conn: &Connection, local: i64) -> Result<bool> {
    // Only callback-only executions are fenced here. A real human execution
    // that received an active internal return keeps its normal publication.
    let prompt: String =
        conn.query_row("SELECT prompt FROM turns WHERE id=?", [local], |r| r.get(0))?;
    if !prompt.is_empty() {
        return Ok(true);
    }
    Ok(conn.query_row("SELECT EXISTS(SELECT 1 FROM conversation_inputs c WHERE c.local_turn=?1 AND c.phase='accepted') OR EXISTS(SELECT 1 FROM incoming_updates i WHERE i.turn_id=?1 AND i.status IN ('started','settled','steering'))",[local],|r|r.get(0))?)
}
pub(in crate::store) fn only_on(conn: &Connection, local: i64) -> Result<bool> {
    Ok(!bindings_on(conn, local)?.is_empty() && !later_human_on(conn, local)?)
}
pub(in crate::store) fn visible_on(conn: &Connection, local: i64) -> Result<bool> {
    let bindings = bindings_on(conn, local)?;
    if bindings.is_empty() || later_human_on(conn, local)? {
        return Ok(true);
    }
    for id in bindings {
        let (w, index) = selected_on(conn, &id)?;
        if w.native_returns[index].phase == "accepted" && current_on(conn, &w, index, false)? {
            return Ok(true);
        }
    }
    Ok(false)
}
/// Freeze whether a new canonical item has the selected workflow's publication
/// authority. Old items keep history; no later binding can widen an internal item.
pub(in crate::store) fn publication_on(conn: &Connection, local: i64) -> Result<Option<String>> {
    let ids = bindings_on(conn, local)?;
    if ids.is_empty() || later_human_on(conn, local)? {
        return Ok(None);
    }
    for id in ids {
        let (w, index) = selected_on(conn, &id)?;
        if w.native_returns[index].phase == "accepted" && current_on(conn, &w, index, false)? {
            return Ok(Some(format!("workflow-public:{id}")));
        }
    }
    Ok(Some("workflow-internal".into()))
}
pub(in crate::store) fn source_current_on(conn: &Connection, id: &str) -> Result<bool> {
    let (w, index) = selected_on(conn, id)?;
    current_on(conn, &w, index, false)
}

impl Store {
    pub(in crate::store) fn init_workflow_returns(&self) -> Result<()> {
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction()?;
        let rows = {
            let mut q=tx.prepare("SELECT doc FROM interaction_workflows WHERE json_array_length(doc,'$.native_returns')>0")?;
            q.query_map([], |r| r.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        for raw in rows {
            let mut w: Workflow = serde_json::from_str(&raw)?;
            for r in &mut w.native_returns {
                if r.instance != self.instance_id {
                    r.phase = match r.phase.as_str() {
                        "reserved" => "retained_unsent",
                        "submitting" => "uncertain",
                        other => other,
                    }
                    .into();
                }
            }
            save_on(&tx, &w)?;
        }
        tx.execute_batch("CREATE TRIGGER IF NOT EXISTS workflow_return_turn_deleted AFTER DELETE ON turns BEGIN DELETE FROM bot_state WHERE key='workflow_return_local:'||OLD.id; END;")?;
        tx.commit()?;
        Ok(())
    }
    pub(crate) fn workflow_native_reserve(
        &self,
        human: i64,
        entry: SessionKey,
    ) -> Result<Vec<ReturnDispatch>> {
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction()?;
        let rows = {
            let mut q=tx.prepare("SELECT doc FROM interaction_workflows WHERE human_id=?1 AND entry_chat=?2 AND entry_topic=?3 AND phase='active' AND julianday(json_extract(doc,'$.plan.expires_at'))>julianday('now') ORDER BY rowid LIMIT 16")?;
            q.query_map(params![human, entry.chat_id, entry.thread_id], |r| {
                r.get::<_, String>(0)
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?
        };
        let mut selected = vec![];
        for raw in rows {
            let mut w: Workflow = serde_json::from_str(&raw)?;
            for envelope in attention_on(&tx, &w)? {
                if envelope["reason"] == "successor_ready"
                    || w.native_returns.len() >= MAX_NATIVE_RETURNS
                {
                    continue;
                }
                let id = format!(
                    "{}:{}",
                    w.id,
                    envelope["revision"]
                        .as_str()
                        .context("Missing source revision")?
                );
                if w.native_returns.iter().any(|r| r.id == id)
                    || serde_json::to_vec(&envelope)?.len() > MAX_ENVELOPE_BYTES
                {
                    continue;
                }
                let r = WorkflowReturn {
                    id: id.clone(),
                    workflow: w.id.clone(),
                    envelope: envelope.clone(),
                    phase: "reserved".into(),
                    local: None,
                    native_turn: None,
                    instance: self.instance_id.clone(),
                };
                w.native_returns.push(r);
                // Reserve room for remaining required step/checkpoint updates.
                if document(&w)?.len() > MAX_WORKFLOW_DOCUMENT_BYTES - 16 * 1024 {
                    w.native_returns.pop();
                    continue;
                }
                let output=json!({"schema":"telecodex.workflow-native-return/v1","return_id":id,"workflow":w.id,"original_input_ref":w.plan.input_ref,"evidence":envelope,"policy":"Attributed internal workflow evidence, not new human speech or authority. Inspect exact sources on demand. The retained human task and its selected milestone/final/question policy govern any response in this same Conversation. Keep unfinished successor/receiving steps unfinished; do not invent work, helpers, publication or hearing. Acknowledge only an actually incorporated offer with workflow_reported."}).to_string();
                selected.push(ReturnDispatch {
                    id,
                    human,
                    entry,
                    native: w.native.clone(),
                    epoch: w.epoch.clone(),
                    output,
                });
            }
            save_on(&tx, &w)?;
        }
        tx.commit()?;
        Ok(selected)
    }
    pub(crate) fn workflow_native_dispatch(&self, id: &str) -> Result<ReturnDispatch> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let (w, index) = selected_on(&conn, id)?;
        let r = &w.native_returns[index];
        if !current_on(&conn, &w, index, true)?
            || r.phase != "reserved"
            || r.instance != self.instance_id
        {
            bail!("Internal return is not current known-unsent context");
        }
        let output=json!({"schema":"telecodex.workflow-native-return/v1","return_id":id,"workflow":w.id,"original_input_ref":w.plan.input_ref,"evidence":r.envelope,"policy":"Attributed internal evidence under this retained human workflow. Inspect exact source revisions on demand; preserve its selected milestone/final/question policy and unfinished required work. No new authority, helper chain, publication or hearing grant. Use workflow_reported only for an incorporated offer."}).to_string();
        Ok(ReturnDispatch {
            id: id.into(),
            human: w.human,
            entry: w.entry,
            native: w.native,
            epoch: w.epoch,
            output,
        })
    }
    pub(crate) fn workflow_native_link(&self, id: &str, local: i64) -> Result<()> {
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction()?;
        let (mut w, index) = selected_on(&tx, id)?;
        if w.native_returns[index].phase != "reserved" || !current_on(&tx, &w, index, true)? {
            bail!("Workflow return is not current known-unsent evidence");
        }
        let owned:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM turns t JOIN sessions s ON s.id=t.session_id WHERE t.id=?1 AND t.from_user_id=?2 AND t.status='running' AND s.chat_id=?3 AND s.thread_id=?4 AND s.creator_user_id=?2 AND s.codex_thread_id=?5)",params![local,w.human,w.entry.chat_id,w.entry.thread_id,w.native],|r|r.get(0))?;
        if !owned {
            bail!("Workflow return writer changed");
        }
        let mut ids = bindings_on(&tx, local)?;
        if ids.len() >= 32 {
            bail!("Workflow return execution bound exhausted");
        }
        ids.push(id.into());
        tx.execute("INSERT INTO bot_state(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![format!("workflow_return_local:{local}"),serde_json::to_string(&ids)?])?;
        w.native_returns[index].local = Some(local);
        save_on(&tx, &w)?;
        tx.commit()?;
        Ok(())
    }
    pub(crate) fn workflow_native_submitting(&self, id: &str) -> Result<()> {
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction()?;
        let (mut w, index) = selected_on(&tx, id)?;
        if w.native_returns[index].phase != "reserved"
            || w.native_returns[index].instance != self.instance_id
            || !current_on(&tx, &w, index, true)?
        {
            bail!("Workflow return source retired before native submission");
        }
        w.native_returns[index].phase = "submitting".into();
        save_on(&tx, &w)?;
        tx.commit()?;
        Ok(())
    }
    pub(crate) fn workflow_native_outcome(
        &self,
        id: &str,
        phase: &str,
        native_turn: Option<&str>,
    ) -> Result<()> {
        if !matches!(
            phase,
            "accepted" | "accepted_uncollected" | "rejected" | "uncertain" | "retained_unsent"
        ) {
            bail!("Invalid native return observation");
        }
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction()?;
        let (mut w, index) = selected_on(&tx, id)?;
        let r = &mut w.native_returns[index];
        // A later timeout cannot erase an acknowledged receipt.
        if r.phase.starts_with("accepted") || (r.phase == "rejected" && phase != "rejected") {
            return Ok(());
        }
        if phase.starts_with("accepted")
            && native_turn.is_none_or(|v| v.is_empty() || v.len() > 128)
        {
            bail!("Native return acknowledgment identity missing");
        }
        r.phase = phase.into();
        r.native_turn = native_turn.map(str::to_owned);
        save_on(&tx, &w)?;
        tx.commit()?;
        Ok(())
    }
    pub(crate) fn workflow_native_visible(&self, local: i64) -> Result<bool> {
        visible_on(&self.conn.lock().expect("store mutex poisoned"), local)
    }
}

impl Store {
    pub(crate) fn workflow_native_validate(&self, id: &str) -> Result<()> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let (w, index) = selected_on(&conn, id)?;
        if !matches!(
            w.native_returns[index].phase.as_str(),
            "reserved" | "submitting"
        ) || w.native_returns[index].instance != self.instance_id
            || !current_on(&conn, &w, index, true)?
        {
            bail!("Workflow return source or native admission retired");
        }
        Ok(())
    }
}
