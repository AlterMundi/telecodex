//! Retained bounded context-return legs of the workflow journal, not another
//! execution queue. Existing entry workers and native collection own execution.
use super::*;

mod context;
mod routed;
use context::{ReturnContext, ReturnParent};

const MAX_NATIVE_RETURNS: usize = 16;
const MAX_RETURN_ATTEMPTS: u8 = 3;
fn first_attempt() -> u8 {
    1
}
fn retry_delay(attempts: u8) -> Option<i64> {
    match attempts {
        1 => Some(2),
        2 => Some(5),
        _ => None,
    }
}
const MAX_ENVELOPE_BYTES: usize = 2048;
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct WorkflowReturn {
    pub id: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub workflow: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reported_revision: Option<String>,
    pub envelope: Value,
    pub phase: String,
    pub local: Option<i64>,
    #[serde(default)]
    pub collection_local: Option<i64>,
    pub native_turn: Option<String>,
    pub instance: String,
    #[serde(default = "first_attempt")]
    pub attempts: u8,
    #[serde(default)]
    pub retry_after: Option<i64>,
}
#[derive(Debug, Clone)]
pub(crate) struct ReturnDispatch {
    pub name: &'static str,
    pub id: String,
    pub attempts: u8,
    pub human: i64,
    pub entry: SessionKey,
    pub native: String,
    pub epoch: String,
    pub output: String,
}
pub(crate) struct ReturnObservation {
    pub physical: String,
    pub live: bool,
}
fn bind_on(conn: &Connection, local: i64, id: &str) -> Result<()> {
    let mut ids = bindings_on(conn, local)?;
    if ids.iter().any(|v| v == id) {
        return Ok(());
    }
    if ids.len() >= 32 {
        bail!("Workflow return execution bound exhausted");
    }
    ids.push(id.into());
    conn.execute("INSERT INTO bot_state(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![format!("workflow_return_local:{local}"),serde_json::to_string(&ids)?])?;
    Ok(())
}
fn detached_on(conn: &Connection, w: &mut ReturnContext, index: usize) -> Result<()> {
    if w.native_returns[index].collection_local.is_some() {
        return Ok(());
    }
    let Ok((session, native)) = owner_on(conn, w.human, w.entry) else {
        return Ok(());
    };
    if native != w.native {
        return Ok(());
    }
    // All receipts incorporated into one physical turn share one distinct
    // observation, so its canonical output freezes the entire source set.
    let receipt = &w.native_returns[index];
    if let Some(submission) = receipt.local {
        for id in bindings_on(conn, submission)? {
            if id == receipt.id {
                continue;
            }
            let (other, i) = match w.native_returns.iter().position(|r| r.id == id) {
                Some(i) => (w.clone(), i),
                None => selected_on(conn, &id)?,
            };
            let sibling = &other.native_returns[i];
            if other.human != w.human
                || other.entry != w.entry
                || other.native != w.native
                || sibling.native_turn != receipt.native_turn
            {
                continue;
            }
            if let Some(local) = sibling.collection_local {
                let open: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM turns t JOIN native_collections n ON n.local_turn=t.id WHERE t.id=?1 AND t.status='observing' AND n.phase='open' AND n.anchor=?2)",params![local,receipt.native_turn],|r|r.get(0))?;
                if open {
                    bind_on(conn, local, &receipt.id)?;
                    w.native_returns[index].collection_local = Some(local);
                    return Ok(());
                }
            }
        }
    }
    let request = TurnRequest {
        session_key: w.entry,
        from_user_id: w.human,
        prompt: String::new(),
        runtime_instructions: None,
        attachments: vec![],
        review_mode: None,
        override_search_mode: None,
    };
    let local = crate::store::record_turn_started_on(conn, session, &request, None)?;
    let live = current_on(conn, w, index, false)?;
    conn.execute(
        "UPDATE turns SET status=?3,native_thread_id=?2,completed_at=?4 WHERE id=?1",
        params![
            local,
            w.native,
            if live {
                "observing"
            } else {
                "observation_retired"
            },
            if live { None } else { Some(now_string()) }
        ],
    )?;
    bind_on(conn, local, &w.native_returns[index].id)?;
    let physical = w.native_returns[index]
        .native_turn
        .as_deref()
        .context("Detached ACK identity missing")?;
    conn.execute("INSERT INTO native_collections(local_turn,anchor,first_turn,phase,detail) VALUES(?1,?2,?2,?3,'Exact acknowledged workflow return; no input replay')",params![local,physical,if current_on(conn,w,index,false)? {"open"} else {"retired"}])?;
    w.native_returns[index].collection_local = Some(local);
    Ok(())
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
fn selected_on(conn: &Connection, id: &str) -> Result<(ReturnContext, usize)> {
    if let Some(selected) = routed::load(conn, id)? {
        return Ok(selected);
    }
    let raw:String=conn.query_row("SELECT doc FROM interaction_workflows w WHERE EXISTS(SELECT 1 FROM json_each(w.doc,'$.native_returns') r WHERE json_extract(r.value,'$.id')=?1)",[id],|r|r.get(0))?;
    let w: Workflow = serde_json::from_str(&raw)?;
    let index = w
        .native_returns
        .iter()
        .position(|r| r.id == id)
        .context("Missing workflow return")?;
    Ok((ReturnContext::workflow(w), index))
}
fn current_on(
    conn: &Connection,
    context: &ReturnContext,
    index: usize,
    admission: bool,
) -> Result<bool> {
    let r = &context.native_returns[index];
    match &context.parent {
        ReturnParent::Routed(parent) => routed::current(conn, parent, r, admission),
        ReturnParent::Workflow(w) => {
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
    }
}
fn save_on(conn: &Connection, context: &ReturnContext) -> Result<()> {
    match &context.parent {
        ReturnParent::Routed(parent) => routed::save(conn, parent, &context.native_returns),
        ReturnParent::Workflow(w) => {
            let mut current = w.clone();
            current.native_returns = context.native_returns.clone();
            super::save_on(conn, &current)
        }
    }
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
    Ok(publication_sources_on(conn, local)?.is_none_or(|ids| !ids.is_empty()))
}
fn publication_sources_on(conn: &Connection, local: i64) -> Result<Option<Vec<String>>> {
    let bindings = bindings_on(conn, local)?;
    if bindings.is_empty() || later_human_on(conn, local)? {
        return Ok(None);
    }
    let mut sources = Vec::new();
    for id in &bindings {
        let (w, index) = selected_on(conn, id)?;
        let r = &w.native_returns[index];
        if r.collection_local
            .map_or(r.local != Some(local), |owner| owner != local)
        {
            continue;
        }
        match r.phase.as_str() {
            "reserved" | "retained_unsent" | "rejected" => continue,
            _ => {}
        }
        let superseded = w
            .native_returns
            .iter()
            .enumerate()
            .any(|(other_index, other)| {
                other.id != r.id
                    && other.envelope["slot"] == r.envelope["slot"]
                    && bindings.contains(&other.id)
                    && other.collection_local.is_none_or(|owner| owner == local)
                    && matches!(other.phase.as_str(), "accepted" | "accepted_reconciled")
                    && w.reported(other_index) == other.envelope["revision"].as_str()
                    && current_on(conn, &w, other_index, false).unwrap_or(false)
            });
        if superseded {
            continue;
        }
        match r.phase.as_str() {
            "accepted" | "accepted_reconciled" if current_on(conn, &w, index, false)? => {
                sources.push(id.clone())
            }
            // Native output has no per-input causal attribution. A stale or
            // possibly incorporated sibling cannot borrow another offer's grant.
            _ => return Ok(Some(Vec::new())),
        }
    }
    Ok(Some(sources))
}
/// Freeze whether a new canonical item has the selected workflow's publication
/// authority. Old items keep history; no later binding can widen an internal item.
pub(in crate::store) fn publication_on(conn: &Connection, local: i64) -> Result<Option<String>> {
    Ok(match publication_sources_on(conn, local)? {
        None => None,
        Some(ids) if ids.is_empty() => Some("workflow-internal".into()),
        Some(ids) => {
            let routed = ids
                .iter()
                .any(|id| id.starts_with("result:") || id.starts_with("question:"));
            Some(format!(
                "{}{}",
                if routed {
                    "native-public-set:"
                } else {
                    "workflow-public-set:"
                },
                serde_json::to_string(&ids)?
            ))
        }
    })
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
            let mut w = ReturnContext::workflow(serde_json::from_str(&raw)?);
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
            for r in &mut w.native_returns {
                if r.phase == "retained_unsent" && r.retry_after.is_none() {
                    r.retry_after =
                        retry_delay(r.attempts).map(|seconds| Utc::now().timestamp() + seconds);
                }
            }
            for index in 0..w.native_returns.len() {
                if w.native_returns[index].phase == "accepted_uncollected" {
                    detached_on(&tx, &mut w, index)?;
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
                    reported_revision: None,
                    envelope: envelope.clone(),
                    phase: "reserved".into(),
                    local: None,
                    collection_local: None,
                    native_turn: None,
                    instance: self.instance_id.clone(),
                    attempts: 1,
                    retry_after: None,
                };
                w.native_returns.push(r);
                // Reserve room for remaining required step/checkpoint updates.
                if document(&w)?.len() > MAX_WORKFLOW_DOCUMENT_BYTES - 16 * 1024 {
                    w.native_returns.pop();
                    continue;
                }
                let output=json!({"schema":"telecodex.workflow-native-return/v1","return_id":id,"workflow":w.id,"original_input_ref":w.plan.input_ref,"evidence":envelope,"policy":"Attributed internal workflow evidence, not new human speech or authority. Inspect exact sources on demand. The retained human task and its selected milestone/final/question policy govern any response in this same Conversation. Keep unfinished successor/receiving steps unfinished; do not invent work, helpers, publication or hearing. Acknowledge only an actually incorporated offer with workflow_reported."}).to_string();
                selected.push(ReturnDispatch {
                    name: "workflow_attention",
                    attempts: 1,
                    id,
                    human,
                    entry,
                    native: w.native.clone(),
                    epoch: w.epoch.clone(),
                    output,
                });
            }
            super::save_on(&tx, &w)?;
        }
        tx.commit()?;
        Ok(selected)
    }
    /// Resume only already-retained, current intents with proof of no native input.
    /// The existing heartbeat invokes this; it never discovers new source events.
    #[cfg(test)]
    pub(crate) fn workflow_native_recover_unsent(
        &self,
        human: i64,
        entry: SessionKey,
    ) -> Result<Vec<ReturnDispatch>> {
        self.native_returns_recover_unsent(human, entry, true, false)
    }
    pub(crate) fn native_returns_recover_unsent(
        &self,
        human: i64,
        entry: SessionKey,
        workflows: bool,
        routed: bool,
    ) -> Result<Vec<ReturnDispatch>> {
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction()?;
        let rows = if workflows {
            let mut q = tx.prepare("SELECT doc FROM interaction_workflows WHERE human_id=?1 AND entry_chat=?2 AND entry_topic=?3 AND phase='active' AND julianday(json_extract(doc,'$.plan.expires_at'))>julianday('now') AND EXISTS(SELECT 1 FROM json_each(doc,'$.native_returns') r WHERE json_extract(r.value,'$.phase')='retained_unsent') ORDER BY rowid LIMIT 16")?;
            q.query_map(params![human, entry.chat_id, entry.thread_id], |r| {
                r.get::<_, String>(0)
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?
        } else {
            Vec::new()
        };
        let mut contexts = rows
            .into_iter()
            .map(|raw| serde_json::from_str(&raw).map(ReturnContext::workflow))
            .collect::<serde_json::Result<Vec<_>>>()?;
        if routed {
            contexts.extend(routed::retained(&tx, Some(human), Some(entry), true)?);
        }
        let mut selected = vec![];
        let now = Utc::now().timestamp();
        for mut w in contexts {
            for index in 0..w.native_returns.len() {
                let r = &w.native_returns[index];
                if selected.len() == 4
                    || r.phase != "retained_unsent"
                    || r.attempts == 0
                    || r.attempts >= MAX_RETURN_ATTEMPTS
                    || r.retry_after.is_none_or(|at| at > now)
                    || !current_on(&tx, &w, index, true)?
                {
                    continue;
                }
                let r = &mut w.native_returns[index];
                r.attempts += 1;
                r.phase = "reserved".into();
                r.instance = self.instance_id.clone();
                r.local = None;
                r.collection_local = None;
                r.native_turn = None;
                r.retry_after = None;
                let id = r.id.clone();
                let attempts = r.attempts;
                let output = w.output(index);
                selected.push(ReturnDispatch {
                    name: w.name(),
                    attempts,
                    id,
                    human: w.human,
                    entry: w.entry,
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
        let output = w.output(index);
        Ok(ReturnDispatch {
            name: w.name(),
            attempts: r.attempts,
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
        bind_on(&tx, local, id)?;
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
    #[cfg(test)]
    pub(crate) fn workflow_native_outcome(
        &self,
        id: &str,
        phase: &str,
        native_turn: Option<&str>,
    ) -> Result<()> {
        let attempt = {
            let conn = self.conn.lock().expect("store mutex poisoned");
            let (w, index) = selected_on(&conn, id)?;
            w.native_returns[index].attempts
        };
        self.workflow_native_outcome_attempt(id, attempt, phase, native_turn)
    }
    pub(crate) fn workflow_native_outcome_attempt(
        &self,
        id: &str,
        attempt: u8,
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
        if r.attempts != attempt {
            return Ok(());
        }
        // A later timeout cannot erase an acknowledged receipt.
        if r.phase.starts_with("accepted") || (r.phase == "rejected" && phase != "rejected") {
            return Ok(());
        }
        if phase.starts_with("accepted")
            && native_turn.is_none_or(|v| v.is_empty() || v.len() > 128)
        {
            bail!("Native return acknowledgment identity missing");
        }
        // A local failure before the native boundary is known unsent. The
        // caller need not infer submission from an unrelated execution error.
        r.phase = if phase == "uncertain" && r.phase == "reserved" {
            "retained_unsent"
        } else {
            phase
        }
        .into();
        r.native_turn = native_turn.map(str::to_owned);
        r.retry_after = if r.phase == "retained_unsent" {
            retry_delay(r.attempts).map(|seconds| Utc::now().timestamp() + seconds)
        } else {
            None
        };
        if phase == "accepted_uncollected" {
            detached_on(&tx, &mut w, index)?;
        }
        save_on(&tx, &w)?;
        tx.commit()?;
        Ok(())
    }
    pub(crate) fn workflow_native_visible(&self, local: i64) -> Result<bool> {
        visible_on(&self.conn.lock().expect("store mutex poisoned"), local)
    }
    /// Only a distinct callback-only observation owns a detached ACK. It cannot
    /// subscribe the previous human collector to another physical turn.
    pub(crate) fn workflow_native_collection(
        &self,
        local: i64,
    ) -> Result<Option<ReturnObservation>> {
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction()?;
        for id in bindings_on(&tx, local)? {
            let (w, index) = selected_on(&tx, &id)?;
            let r = &w.native_returns[index];
            if r.collection_local != Some(local) {
                continue;
            }
            let live = current_on(&tx, &w, index, false)?;
            if !live {
                tx.execute("UPDATE native_collections SET phase='retired',detail='Workflow observation scope retired; exact ACK retained' WHERE local_turn=?1",[local])?;
                tx.execute("UPDATE turns SET status='observation_retired',completed_at=?2 WHERE id=?1 AND status='observing'",params![local,now_string()])?;
            }
            let result = ReturnObservation {
                physical: r.native_turn.clone().context("Detached ACK missing")?,
                live,
            };
            tx.commit()?;
            return Ok(Some(result));
        }
        Ok(None)
    }
}

/// Called inside canonical recovery's transaction, before recording its items.
/// Exact ACK and collection ownership are immutable; partial/no-final snapshots
/// cannot obtain publication authority or be reported as reconciled.
pub(in crate::store) fn reconcile_on(conn: &Connection, local: i64, turns: &[Value]) -> Result<()> {
    for id in bindings_on(conn, local)? {
        let (mut w, index) = selected_on(conn, &id)?;
        let r = &w.native_returns[index];
        if r.collection_local != Some(local) {
            continue;
        }
        anyhow::ensure!(
            turns.len() == 1 && turns[0]["id"].as_str() == r.native_turn.as_deref(),
            "Workflow observation escaped exact acknowledged turn"
        );
        anyhow::ensure!(
            current_on(conn, &w, index, false)?,
            "Workflow observation scope retired"
        );
        let turn = &turns[0];
        if turn["status"] == "inProgress" {
            return Ok(());
        }
        anyhow::ensure!(
            matches!(
                turn["status"].as_str(),
                Some("completed" | "failed" | "interrupted")
            ),
            "Workflow terminal observation malformed"
        );
        let canonical = turn["items"]
            .as_array()
            .context("Exact workflow items missing")?
            .iter()
            .filter(|i| i["type"] == "agentMessage")
            .collect::<Vec<_>>();
        anyhow::ensure!(
            canonical.len() <= 16
                && canonical
                    .iter()
                    .map(|i| i["text"].as_str().map_or(0, str::len))
                    .sum::<usize>()
                    <= 64 * 1024,
            "Exact workflow canonical output exceeds recovery bound; source retained"
        );
        let final_item = turn["items"]
            .as_array()
            .and_then(|v| {
                v.iter().rev().find(|i| {
                    i["type"] == "agentMessage"
                        && i["phase"] == "final_answer"
                        && i["text"].as_str().is_some_and(|t| !t.trim().is_empty())
                })
            })
            .context(
                "Exact terminal workflow return has no canonical final; reconciliation pending",
            )?;
        // An unacknowledged sibling may have joined this same physical turn.
        // Its persisted intent precedes native submission; wait for observation,
        // never guess its target or replay it to discover the answer.
        if let Some(submission) = r.local {
            for sibling_id in bindings_on(conn, submission)? {
                let (sibling_workflow, sibling_index) = selected_on(conn, &sibling_id)?;
                let sibling = &sibling_workflow.native_returns[sibling_index];
                anyhow::ensure!(
                    !matches!(sibling.phase.as_str(), "submitting" | "uncertain")
                        || sibling.native_turn.is_some(),
                    "Workflow sibling native target uncertain; reconciliation pending"
                );
            }
        }
        w.native_returns[index].phase = "accepted_reconciled".into();
        save_on(conn, &w)?;
        conn.execute("UPDATE turns SET status=?2,assistant_text=?3,completed_at=?4 WHERE id=?1 AND status IN ('observing','completed','failed','interrupted')",params![local,turn["status"].as_str(),final_item["text"].as_str(),now_string()])?;
    }
    Ok(())
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
