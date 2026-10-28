//! Bounded human-selected workflows extend the existing instruction/result journal.
//! This module never runs a scheduler, publishes an answer or creates human speech.
use super::*;
use crate::interaction::{Delivery, InputReference, Instruction, RequestRelation, SourceInput};
use anyhow::bail;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

mod native_returns;
pub(crate) use native_returns::{ReturnDispatch, WorkflowReturn};
pub(super) use native_returns::{
    only_on as native_only_on, publication_on as native_publication_on,
    reconcile_on as native_reconcile_on, source_current_on as native_source_current_on,
    transfer_collection_on as native_transfer_collection_on, visible_on as native_visible_on,
};

const MAX_RETAINED_WORKFLOWS: i64 = 256;
const MAX_WORKFLOW_DOCUMENT_BYTES: usize = 256 * 1024;
pub(crate) const WORKFLOW_CAPACITY_ERROR: &str = "Workflow journal capacity reached; retain existing history and finish or inspect current workflows";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkflowStep {
    pub id: String,
    pub owner: SessionKey,
    pub after: Option<String>,
    pub scope: String,
    pub checkpoint: String,
    pub milestone: Option<String>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkflowPlan {
    pub request_id: String,
    pub input_ref: InputReference,
    pub expires_at: String,
    pub steps: Vec<WorkflowStep>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Admission {
    instruction: String,
    predecessor: Option<Value>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Workflow {
    id: String,
    human: i64,
    entry: SessionKey,
    native: String,
    binding: String,
    epoch: String,
    source: SourceInput,
    plan: WorkflowPlan,
    phase: String,
    admissions: Vec<Option<Admission>>,
    // One current acknowledgment per step/source kind, never an event history.
    reported: Vec<Option<String>>,
    final_checkpoint: Option<String>,
    supersession: Option<Value>,
    #[serde(default)]
    native_returns: Vec<WorkflowReturn>,
}
fn digest(v: &impl Serialize) -> Result<String> {
    Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(v)?)))
}
fn token(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
fn load_on(conn: &Connection, human: i64, entry: SessionKey, id: &str) -> Result<Workflow> {
    let raw:String=conn.query_row("SELECT doc FROM interaction_workflows WHERE id=?1 AND human_id=?2 AND entry_chat=?3 AND entry_topic=?4",params![id,human,entry.chat_id,entry.thread_id],|r|r.get(0))?;
    Ok(serde_json::from_str(&raw)?)
}
fn document(w: &Workflow) -> Result<String> {
    let raw = serde_json::to_string(w)?;
    if raw.len() > MAX_WORKFLOW_DOCUMENT_BYTES {
        bail!("Workflow document bound exceeded; no state was changed");
    }
    Ok(raw)
}
fn save_on(conn: &Connection, w: &Workflow) -> Result<()> {
    let raw = document(w)?;
    conn.execute(
        "UPDATE interaction_workflows SET phase=?2,doc=?3 WHERE id=?1",
        params![w.id, w.phase, raw],
    )?;
    Ok(())
}
fn live_on(conn: &Connection, w: &Workflow) -> Result<bool> {
    let (session, native) = super::presentation::entry_on(conn, w.human, w.entry)?;
    if w.phase != "active"
        || native != w.native
        || super::presentation::entry_binding(conn, w.human, w.entry, session)? != w.binding
        || super::stopping::admission_epoch_on(conn, w.entry)? != w.epoch
        || super::stopping::fenced_on(conn, w.entry)?
        || super::maintenance_writer_held_on(conn, w.entry)?
        || chrono::DateTime::parse_from_rfc3339(&w.plan.expires_at)? <= Utc::now()
    {
        return Ok(false);
    }
    // Re-read the retained original instead of promoting a callback to authority.
    let (original, _) = super::dialogue::original_owned_on(
        conn,
        w.entry,
        w.human,
        &w.native,
        &w.plan.input_ref,
        &w.source.channel,
        false,
    )?;
    Ok(original == w.source)
}
fn owner_on(conn: &Connection, human: i64, key: SessionKey) -> Result<(i64, String)> {
    Ok(conn.query_row("SELECT s.id,s.codex_thread_id FROM sessions s JOIN users u ON u.tg_user_id=s.creator_user_id WHERE s.chat_id=?1 AND s.thread_id=?2 AND s.creator_user_id=?3 AND u.allowed=1 AND s.force_fresh_thread=0 AND s.codex_thread_id IS NOT NULL",params![key.chat_id,key.thread_id,human],|r|Ok((r.get(0)?,r.get(1)?)))?)
}
fn instruction_on(conn: &Connection, human: i64, id: &str) -> Result<Instruction> {
    let (raw,phase):(String,String)=conn.query_row("SELECT i.doc,i.phase FROM interaction_instructions i JOIN interaction_sources s ON s.id=i.input_id WHERE i.id=?1 AND s.human_id=?2",params![id,human],|r|Ok((r.get(0)?,r.get(1)?)))?;
    let mut i: Instruction = serde_json::from_str(&raw)?;
    i.phase = phase.as_str().try_into()?;
    Ok(i)
}
fn result_on(conn: &Connection, w: &Workflow, index: usize) -> Result<Option<(Value, Delivery)>> {
    let Some(a) = &w.admissions[index] else {
        return Ok(None);
    };
    let i = instruction_on(conn, w.human, &a.instruction)?;
    // Join every covered request, not only the representative delivery request.
    let found:Option<(String,String,String,String,String)>=conn.query_row(
        "SELECT d.doc,d.result_id,z.native_thread,z.native_turn,z.item_id FROM interaction_deliveries d JOIN interaction_delivery_requests m ON m.delivery_id=d.id JOIN interaction_request_links l ON l.request_id=m.request_id JOIN interaction_results z ON z.id=d.result_id WHERE l.instruction_id=?1 AND d.human_id=?2 ORDER BY d.rowid DESC LIMIT 1",
        params![i.id,w.human],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).optional()?;
    let Some((raw, result, native, turn, item)) = found else {
        return Ok(None);
    };
    let d: Delivery = serde_json::from_str(&raw)?;
    let (current, correction) = super::routing::delivery_freshness(conn, &d)?;
    let owner_current =
        owner_on(conn, w.human, w.plan.steps[index].owner).is_ok_and(|(_, n)| n == native);
    let mut source = json!({"id":result,"delivery":d.id,"native_thread":native,"native_turn":turn,"item":item,"source_session":i.target_session,"instruction":i.id,"completed":d.completed,"current":current && owner_current,"correction_pending":correction});
    // The revision fences the immutable canonical body and current source/coverage.
    source["revision"] = json!(digest(&json!([source, d.covered, d.body]))?);
    Ok(Some((source, d)))
}
fn completed(source: &Value) -> bool {
    source["completed"] == true
        && source["current"] == true
        && source["correction_pending"] == false
}
fn view_on(conn: &Connection, w: &Workflow) -> Result<Value> {
    let live = live_on(conn, w).unwrap_or(false);
    let mut steps = vec![];
    let mut previous: Option<Value> = None;
    let mut all_completed = true;
    for (index, step) in w.plan.steps.iter().enumerate() {
        let result = result_on(conn, w, index)?.map(|(source, _)| source);
        let predecessor_current = index == 0 || previous.as_ref().is_some_and(completed);
        let mut phase = if predecessor_current {
            "ready"
        } else {
            "waiting"
        }
        .to_owned();
        let mut instruction = None;
        if let Some(a) = &w.admissions[index] {
            let i = instruction_on(conn, w.human, &a.instruction)?;
            phase = i.phase.as_str().to_owned();
            // An already started successor cannot be replayed after a correction.
            // Retain it and expose the mismatch for integrator reconciliation.
            if a.predecessor != previous {
                phase = "predecessor_changed".into();
            } else if result.as_ref().is_some_and(completed) {
                phase = "completed".into();
            } else if result
                .as_ref()
                .is_some_and(|r| r["correction_pending"] == true || r["current"] == false)
            {
                phase = "correction_pending".into();
            }
            instruction = Some(i.id);
        } else if !live {
            phase = "withdrawn".into();
        }
        all_completed &= phase == "completed";
        steps.push(json!({"id":step.id,"owner":step.owner,"after":step.after,"checkpoint":step.checkpoint,"milestone":step.milestone,"phase":phase,"instruction":instruction,"result":result}));
        // Successor readiness requires the whole prefix, including the source
        // revision actually consumed by every earlier step.
        previous = if phase == "completed" { result } else { None };
    }
    let phase = if w.phase != "active" {
        w.phase.as_str()
    } else if !live {
        "retired"
    } else if all_completed {
        "awaiting_integration"
    } else {
        "active"
    };
    let mut view = json!({"schema":"telecodex.workflow/v1","id":w.id,"input_ref":w.plan.input_ref,"request_id":w.plan.request_id,"entry":w.entry,"expires_at":w.plan.expires_at,"phase":phase,"steps":steps,"final_checkpoint":w.final_checkpoint,"supersession":w.supersession,"authority":"retained original human input; worker results are internal evidence","publication_grant":false});
    view["revision"] = json!(digest(&view)?);
    let mut returns = Vec::new();
    for r in &w.native_returns {
        let observation = if let Some(local) = r.collection_local {
            conn.query_row(
                "SELECT phase,detail FROM native_collections WHERE local_turn=?1",
                [local],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?
            .map(|(phase, detail)| json!({"phase":phase,"detail":detail}))
        } else {
            None
        };
        returns.push(json!({"id":r.id,"source_revision":r.envelope["revision"],"phase":r.phase,"local_turn":r.local,"collection_local_turn":r.collection_local,"native_turn":r.native_turn,"attempts":r.attempts,"max_attempts":3,"retry_after":r.retry_after,"observation":observation}));
    }
    view["native_returns"] = json!({"limit":16,"retained":returns});
    Ok(view)
}
fn attention_on(conn: &Connection, w: &Workflow) -> Result<Vec<Value>> {
    let view = view_on(conn, w)?;
    if !matches!(
        view["phase"].as_str(),
        Some("active" | "awaiting_integration")
    ) {
        return Ok(vec![]);
    }
    let mut envelopes = vec![];
    for (index, step) in w.plan.steps.iter().enumerate() {
        let source = &view["steps"][index]["result"];
        let bounded_source = if source.is_object() {
            json!({"kind":"canonical_result","id":source["id"],"delivery":source["delivery"],"revision":source["revision"],"source_session":source["source_session"],"instruction":source["instruction"],"completed":source["completed"],"current":source["current"],"correction_pending":source["correction_pending"]})
        } else {
            Value::Null
        };
        let phase = view["steps"][index]["phase"].as_str().unwrap_or("");
        let reason = if matches!(phase, "correction_pending" | "predecessor_changed") {
            Some("reconciliation_required")
        } else if phase == "completed" && index + 1 == w.plan.steps.len() {
            Some("integration_required")
        } else if phase == "completed"
            && (step.milestone.is_some()
                || w.admissions.get(index + 1).is_some_and(Option::is_none))
        {
            Some(if step.milestone.is_some() {
                "reporting_milestone"
            } else {
                "successor_ready"
            })
        } else if matches!(phase, "retained_unsent" | "uncertain" | "rejected") {
            Some("admission_attention_required")
        } else {
            None
        };
        if let Some(reason) = reason {
            let mut envelope = json!({"workflow":w.id,"step":step.id,"source":bounded_source,"reason":reason,"milestone":step.milestone,"workflow_revision":view["revision"],"expires_at":w.plan.expires_at,"internal_evidence":true,"publication_grant":false});
            envelope
                .as_object_mut()
                .unwrap()
                .remove("workflow_revision");
            envelope["slot"] = json!(index * 3);
            let revision = digest(&envelope)?;
            envelope["revision"] = json!(revision);
            envelope["workflow_revision"] = view["revision"].clone();
            if w.reported[index * 3].as_deref() != Some(&revision) {
                envelopes.push(envelope);
            }
        }
        // Only pending native questions from this exact admitted step execution.
        // These source versions are not approval/answer revisions or responders.
        if let Some(admission) = &w.admissions[index] {
            let instruction = instruction_on(conn, w.human, &admission.instruction)?;
            let mut q=conn.prepare("SELECT h.id FROM human_interactions h JOIN turns t ON t.id=h.local_turn WHERE h.requester=?1 AND h.kind='native' AND h.state='pending' AND h.local_turn=?2 AND t.status='running' AND h.native_thread=?3 AND json_extract(h.payload_json,'$.source.kind')='question' ORDER BY h.rowid LIMIT 2")?;
            let ids = q
                .query_map(
                    params![w.human, instruction.local_turn, instruction.native_thread],
                    |r| r.get::<_, String>(0),
                )?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            for (position, id) in ids.into_iter().enumerate() {
                let question = super::conversation_requests::read_scoped_on(
                    conn, w.human, w.entry, &id, true,
                )?;
                let source = json!({"kind":"native_question","id":id,"revision":question.capture_version,"revision_kind":"capture_version","source_session":instruction.target_session,"native_thread":question.item.native_thread,"local_turn":question.item.local_turn,"current":true});
                let slot = index * 3 + position + 1;
                let mut envelope = json!({"workflow":w.id,"step":step.id,"source":source,"reason":"human_decision_required","slot":slot,"expires_at":w.plan.expires_at,"internal_evidence":true,"publication_grant":false});
                let revision = digest(&envelope)?;
                envelope["revision"] = json!(revision);
                envelope["workflow_revision"] = view["revision"].clone();
                if w.reported[slot].as_deref() != Some(&revision) {
                    envelopes.push(envelope);
                }
            }
        }
    }
    Ok(envelopes)
}

impl Store {
    pub(super) fn init_workflows(&self) -> Result<()> {
        self.conn.lock().expect("store mutex poisoned").execute_batch(
            "CREATE TABLE IF NOT EXISTS interaction_workflows(id TEXT PRIMARY KEY,human_id INTEGER NOT NULL,entry_chat INTEGER NOT NULL,entry_topic INTEGER NOT NULL,source_key TEXT NOT NULL,phase TEXT NOT NULL,doc TEXT NOT NULL,UNIQUE(human_id,entry_chat,entry_topic,source_key));")?;
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    pub fn interaction_workflow_declare(
        &self,
        human: i64,
        entry: SessionKey,
        native: &str,
        channel: &str,
        plan: WorkflowPlan,
        closed: &tokio_util::sync::CancellationToken,
    ) -> Result<Value> {
        if !token(&plan.request_id) || plan.steps.is_empty() || plan.steps.len() > 4 {
            bail!("A workflow requires one to four predeclared steps");
        }
        let mut ids = std::collections::HashSet::new();
        for (index, step) in plan.steps.iter().enumerate() {
            if !token(&step.id)
                || !ids.insert(&step.id)
                || step.owner == entry
                || step.scope.trim().is_empty()
                || step.scope.len() > 4000
                || step.checkpoint.trim().is_empty()
                || step.checkpoint.len() > 512
                || step
                    .milestone
                    .as_ref()
                    .is_some_and(|m| m.trim().is_empty() || m.len() > 80)
                || step.after.as_ref() != index.checked_sub(1).map(|i| &plan.steps[i].id)
            {
                bail!("Invalid ordered workflow step");
            }
        }
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction()?;
        let (session, current) = super::presentation::entry_on(&tx, human, entry)?;
        if current != native {
            bail!("Workflow entry binding changed");
        }
        let (source, _) = super::dialogue::original_owned_on(
            &tx,
            entry,
            human,
            native,
            &plan.input_ref,
            channel,
            false,
        )?;
        let source_key = digest(&json!([source.channel, source.event_id]))?;
        let id = Uuid::new_v5(
            &Uuid::NAMESPACE_URL,
            format!("telecodex:workflow:{human}:{session}:{source_key}").as_bytes(),
        )
        .to_string();
        let existing: Option<String> = tx
            .query_row(
                "SELECT doc FROM interaction_workflows WHERE id=?",
                [&id],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(raw) = existing {
            let w: Workflow = serde_json::from_str(&raw)?;
            if w.plan != plan || w.source != source || w.native != native {
                bail!("Workflow identity is immutable");
            }
            return view_on(&tx, &w);
        }
        super::matrix::unclaimed_on(
            &tx,
            human,
            &source.channel,
            &source.event_id,
            source.update_id,
        )?;
        let selected:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM interaction_sources WHERE human_id=?1 AND channel=?2 AND event_id=?3) OR EXISTS(SELECT 1 FROM interaction_creations WHERE human_id=?1 AND input_update=?4)",params![human,source.channel,source.event_id,plan.input_ref],|r|r.get(0))?;
        if selected {
            bail!("Original already has a dispatch or creation plan");
        }
        let retained: i64 =
            tx.query_row("SELECT count(*) FROM interaction_workflows", [], |r| {
                r.get(0)
            })?;
        if retained >= MAX_RETAINED_WORKFLOWS {
            bail!(WORKFLOW_CAPACITY_ERROR);
        }
        let expiry = chrono::DateTime::parse_from_rfc3339(&plan.expires_at)?;
        if expiry <= Utc::now() || expiry > Utc::now() + Duration::days(7) {
            bail!("Workflow lifetime must be finite and at most seven days");
        }
        let count:i64=tx.query_row("SELECT count(*) FROM interaction_workflows WHERE human_id=?1 AND entry_chat=?2 AND entry_topic=?3 AND phase='active' AND julianday(json_extract(doc,'$.plan.expires_at'))>julianday('now')",params![human,entry.chat_id,entry.thread_id],|r|r.get(0))?;
        if count >= 16 {
            bail!("Active workflow bound reached");
        }
        for step in &plan.steps {
            owner_on(&tx, human, step.owner)?;
        }
        let w = Workflow {
            id: id.clone(),
            human,
            entry,
            native: native.into(),
            binding: super::presentation::entry_binding(&tx, human, entry, session)?,
            epoch: super::stopping::admission_epoch_on(&tx, entry)?,
            source,
            admissions: vec![None; plan.steps.len()],
            reported: vec![None; plan.steps.len() * 3],
            plan,
            phase: "active".into(),
            final_checkpoint: None,
            supersession: None,
            native_returns: vec![],
        };
        if !live_on(&tx, &w)? || closed.is_cancelled() {
            bail!("Workflow authority is not current");
        }
        let raw = document(&w)?;
        // Reserve room for the four bounded admission references, offer slots,
        // later-original supersession reference and final integration checkpoint.
        if raw.len() > MAX_WORKFLOW_DOCUMENT_BYTES - 32 * 1024 {
            bail!("Workflow original exceeds the reserved journal document bound");
        }
        tx.execute(
            "INSERT INTO interaction_workflows VALUES(?1,?2,?3,?4,?5,'active',?6)",
            params![id, human, entry.chat_id, entry.thread_id, source_key, raw],
        )?;
        let view = view_on(&tx, &w)?;
        tx.commit()?;
        Ok(view)
    }
    pub fn interaction_workflow_status(
        &self,
        human: i64,
        entry: SessionKey,
        native: &str,
        id: &str,
    ) -> Result<Value> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        if owner_on(&conn, human, entry)?.1 != native {
            bail!("Entry binding changed");
        }
        view_on(&conn, &load_on(&conn, human, entry, id)?)
    }
    /// Exactly one explicit progression under the original grant. No scheduler
    /// and no retry of an existing queued/uncertain/native-accepted instruction.
    #[allow(clippy::too_many_arguments)]
    pub fn interaction_workflow_advance(
        &self,
        human: i64,
        entry: SessionKey,
        native: &str,
        id: &str,
        revision: &str,
        closed: &tokio_util::sync::CancellationToken,
    ) -> Result<Option<Instruction>> {
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction()?;
        let mut w = load_on(&tx, human, entry, id)?;
        if native != w.native || !live_on(&tx, &w)? || closed.is_cancelled() {
            bail!("Workflow grant retired");
        }
        let view = view_on(&tx, &w)?;
        if view["revision"].as_str() != Some(revision) {
            bail!("Workflow revision changed; inspect without replay");
        }
        let Some(index) = view["steps"]
            .as_array()
            .unwrap()
            .iter()
            .position(|s| s["phase"] == "ready")
        else {
            return Ok(None);
        };
        let step = &w.plan.steps[index];
        let (session, _) = owner_on(&tx, human, step.owner)?;
        if super::stopping::fenced_on(&tx, step.owner)?
            || super::maintenance_writer_held_on(&tx, step.owner)?
        {
            bail!("Workflow owner admission held");
        }
        let predecessor = index
            .checked_sub(1)
            .map(|i| view["steps"][i]["result"].clone());
        let text = format!(
            "Preauthorized workflow step, authored by the integrating session. This is internal session context under the retained human task, never a new human instruction or permission to transfer ownership/create helpers. Workflow: {}. Step: {}. Original input: {}. Required checkpoint: {}. Scope:\n{}\nPredecessor source (read its exact revision with workflow_result before relying on it): {}",
            w.id,
            step.id,
            w.plan.input_ref,
            step.checkpoint,
            step.scope,
            serde_json::to_string(&predecessor)?
        );
        let source = SourceInput {
            transcription: None,
            channel: "local-session".into(),
            event_id: format!("workflow:{}:{}", w.id, step.id),
            update_id: None,
            human_id: human,
            target_sessions: vec![session],
            origin: w.source.origin.clone(),
            text,
        };
        let (mut i, fresh) = super::interaction::admit_on(
            &tx,
            &source,
            step.owner,
            RequestRelation::New,
            &self.instance_id,
        )?;
        if !fresh {
            bail!("Unattributed workflow instruction exists");
        }
        i.admission_epoch = Some(w.epoch.clone());
        tx.execute(
            "UPDATE interaction_instructions SET doc=?2 WHERE id=?1",
            params![i.id, serde_json::to_string(&i)?],
        )?;
        let author = json!({"kind":"workflow_step","session":entry,"native_thread":native,"workflow":w.id,"step":step.id,"input_ref":w.plan.input_ref,"authorized_human":human,"predecessor":predecessor});
        tx.execute(
            "INSERT INTO interaction_session_authors VALUES(?1,?2)",
            params![i.input_id, author.to_string()],
        )?;
        w.admissions[index] = Some(Admission {
            instruction: i.id.clone(),
            predecessor,
        });
        save_on(&tx, &w)?;
        if closed.is_cancelled() {
            bail!("Workflow advance closed before commitment");
        }
        tx.commit()?;
        Ok(Some(i))
    }
    /// A committed canonical result is evidence for the integrator's retained
    /// finite plan, never a new worker-authored grant. Select exactly that source
    /// before reusing the same transactional progression/admission primitive.
    pub fn interaction_workflow_return(
        &self,
        human: i64,
        delivery: &str,
    ) -> Result<Option<Instruction>> {
        let selected = {
            let conn = self.conn.lock().expect("store mutex poisoned");
            let mut q=conn.prepare("SELECT DISTINCT w.doc FROM interaction_workflows w JOIN interaction_session_authors a ON json_extract(a.doc,'$.workflow')=w.id JOIN interaction_instructions i ON i.input_id=a.input_id JOIN interaction_request_links l ON l.instruction_id=i.id JOIN interaction_delivery_requests m ON m.request_id=l.request_id JOIN interaction_deliveries d ON d.id=m.delivery_id WHERE d.id=?1 AND d.human_id=?2 AND w.human_id=?2 AND w.phase='active' AND json_extract(a.doc,'$.kind')='workflow_step' LIMIT 2")?;
            let candidates = q
                .query_map(params![delivery, human], |r| r.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            if candidates.len() > 1 {
                bail!("Canonical return has ambiguous workflow ownership");
            }
            let Some(raw) = candidates.into_iter().next() else {
                return Ok(None);
            };
            let w: Workflow = serde_json::from_str(&raw)?;
            let view = view_on(&conn, &w)?;
            if view["phase"] != "active" {
                return Ok(None);
            }
            let steps = view["steps"].as_array().unwrap();
            let ready = steps.iter().enumerate().any(|(index, step)| {
                index > 0
                    && step["phase"] == "ready"
                    && steps[index - 1]["phase"] == "completed"
                    && steps[index - 1]["result"]["delivery"] == delivery
            });
            if !ready {
                return Ok(None);
            }
            (
                w.entry,
                w.native,
                w.id,
                view["revision"].as_str().unwrap().to_owned(),
            )
        };
        // Revalidation is inside the committing transaction. A concurrent human
        // correction or duplicate result callback invalidates this exact view.
        self.interaction_workflow_advance(
            human,
            selected.0,
            &selected.1,
            &selected.2,
            &selected.3,
            &tokio_util::sync::CancellationToken::new(),
        )
    }

    pub fn interaction_workflow_attention(&self, human: i64, entry: SessionKey) -> Result<Value> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        super::presentation::entry_on(&conn, human, entry)?;
        let mut q=conn.prepare("SELECT doc FROM interaction_workflows WHERE human_id=?1 AND entry_chat=?2 AND entry_topic=?3 AND phase='active' AND julianday(json_extract(doc,'$.plan.expires_at'))>julianday('now') ORDER BY rowid LIMIT 16")?;
        let mut envelopes = vec![];
        for row in q.query_map(params![human, entry.chat_id, entry.thread_id], |r| {
            r.get::<_, String>(0)
        })? {
            let workflow: Workflow = serde_json::from_str(&row?)?;
            for mut envelope in attention_on(&conn, &workflow)? {
                // Delivery metadata never changes the source revision, reserves
                // another return, or converts an ACK into human incorporation.
                if let Some(receipt) = workflow
                    .native_returns
                    .iter()
                    .rev()
                    .find(|r| r.envelope["revision"] == envelope["revision"])
                {
                    envelope["native_return"] = json!({"id":receipt.id,"phase":receipt.phase,"native_turn":receipt.native_turn,"attempts":receipt.attempts,"max_attempts":3,"retry_after":receipt.retry_after});
                }
                envelopes.push(envelope);
            }
        }
        Ok(
            json!({"schema":"telecodex.workflow-attention/v1","envelopes":envelopes,"bounded":true,"max_workflows":16,"max_steps":4,"max_envelopes":192,"automatic_native_return":false,"publication_grant":false}),
        )
    }
    /// An exact offer acknowledgment is not a hearing or completion receipt.
    pub fn interaction_workflow_reported(
        &self,
        human: i64,
        entry: SessionKey,
        native: &str,
        id: &str,
        revision: &str,
    ) -> Result<()> {
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction()?;
        let mut w = load_on(&tx, human, entry, id)?;
        if native != w.native || !live_on(&tx, &w)? {
            bail!("Workflow grant retired");
        }
        if w.reported.iter().any(|r| r.as_deref() == Some(revision)) {
            return Ok(());
        }
        let envelope = attention_on(&tx, &w)?
            .into_iter()
            .find(|e| e["revision"].as_str() == Some(revision))
            .context("Attention source changed or expired")?;
        let slot = envelope["slot"].as_u64().context("Missing offer slot")? as usize;
        w.reported[slot] = Some(revision.into());
        save_on(&tx, &w)?;
        tx.commit()?;
        Ok(())
    }
    pub fn interaction_workflow_finish(
        &self,
        human: i64,
        entry: SessionKey,
        native: &str,
        id: &str,
        revision: &str,
        checkpoint: &str,
    ) -> Result<Value> {
        if checkpoint.trim().is_empty() || checkpoint.len() > 2000 {
            bail!("Integration checkpoint required");
        }
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction()?;
        let mut w = load_on(&tx, human, entry, id)?;
        if native != w.native || owner_on(&tx, human, entry)?.1 != native {
            bail!("Workflow entry binding changed");
        }
        if w.phase == "completed" {
            if w.final_checkpoint.as_deref() != Some(checkpoint) {
                bail!("Final integration checkpoint is immutable");
            }
            return view_on(&tx, &w);
        }
        if !live_on(&tx, &w)? {
            bail!("Workflow grant retired");
        }
        let view = view_on(&tx, &w)?;
        if view["revision"].as_str() != Some(revision) || view["phase"] != "awaiting_integration" {
            bail!("Required workflow steps remain unfinished or changed");
        }
        w.phase = "completed".into();
        w.final_checkpoint = Some(checkpoint.into());
        save_on(&tx, &w)?;
        let view = view_on(&tx, &w)?;
        tx.commit()?;
        Ok(view)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn interaction_workflow_supersede(
        &self,
        human: i64,
        entry: SessionKey,
        native: &str,
        channel: &str,
        id: &str,
        revision: &str,
        input_ref: InputReference,
    ) -> Result<Value> {
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction()?;
        let mut w = load_on(&tx, human, entry, id)?;
        if native != w.native || owner_on(&tx, human, entry)?.1 != native {
            bail!("Entry binding changed");
        }
        if w.phase == "superseded"
            && w.supersession
                .as_ref()
                .is_some_and(|s| s["input_ref"] == json!(input_ref))
        {
            return view_on(&tx, &w);
        }
        if w.phase != "active" || view_on(&tx, &w)?["revision"].as_str() != Some(revision) {
            bail!("Workflow changed before supersession");
        }
        if input_ref == w.plan.input_ref {
            bail!("Supersession requires a later human original");
        }
        let (source, _) =
            super::dialogue::original_on(&tx, entry, human, native, &input_ref, channel, false)?;
        let old_order = super::dialogue::original_order_on(&tx, &w.source)?;
        let new_order = super::dialogue::original_order_on(&tx, &source)?;
        if old_order.zip(new_order).is_none_or(|(a, b)| b <= a) {
            bail!("Supersession requires observed later human input");
        }
        w.phase = "superseded".into();
        w.supersession = Some(
            json!({"input_ref":input_ref,"source_revision":digest(&source)?,"source_channel":source.channel,"source_event":source.event_id}),
        );
        save_on(&tx, &w)?;
        // Only queued, definitely unsent inputs are held. Native accepted and
        // uncertain operations remain untouched and visible in the same journal.
        tx.execute("UPDATE interaction_instructions SET phase='retained_unsent' WHERE phase='queued' AND input_id IN (SELECT input_id FROM interaction_session_authors WHERE json_extract(doc,'$.workflow')=?1)",[id])?;
        let view = view_on(&tx, &w)?;
        tx.commit()?;
        Ok(view)
    }

    pub fn interaction_workflow_question(
        &self,
        human: i64,
        entry: SessionKey,
        native: &str,
        id: &str,
        revision: &str,
        offset: usize,
    ) -> Result<Value> {
        if offset > 1_000_000 {
            bail!("Question offset outside bound");
        }
        let conn = self.conn.lock().expect("store mutex poisoned");
        if owner_on(&conn, human, entry)?.1 != native {
            bail!("Entry binding changed");
        }
        let mut q=conn.prepare("SELECT doc FROM interaction_workflows WHERE human_id=?1 AND entry_chat=?2 AND entry_topic=?3 AND phase='active' AND julianday(json_extract(doc,'$.plan.expires_at'))>julianday('now') ORDER BY rowid DESC LIMIT 16")?;
        let mut selected = None;
        for raw in q.query_map(params![human, entry.chat_id, entry.thread_id], |r| {
            r.get::<_, String>(0)
        })? {
            let mut w: Workflow = serde_json::from_str(&raw?)?;
            w.reported.fill(None);
            for envelope in attention_on(&conn, &w)? {
                if envelope["source"]["kind"] == "native_question"
                    && envelope["source"]["id"] == id
                    && envelope["source"]["revision"] == revision
                {
                    selected = Some(envelope["source"].clone());
                }
            }
        }
        let source =
            selected.context("Question version expired, reported or outside selected workflow")?;
        let question = super::conversation_requests::read_scoped_on(&conn, human, entry, id, true)?;
        if question.item.state != "pending" || question.capture_version != revision {
            bail!("Question version changed");
        }
        let index = question.item.source["conversationIndex"]
            .as_u64()
            .unwrap_or(0) as usize;
        let text = serde_json::to_string(&question.item.source["request"]["questions"][index])?;
        let length = text.chars().count();
        if offset > length {
            bail!("Question offset beyond source");
        }
        let text: String = text.chars().skip(offset).take(4000).collect();
        let end = offset + text.chars().count();
        Ok(
            json!({"schema":"telecodex.workflow-question-part/v1","source":source,"text":text,"offset":offset,"next_offset":if end<length{Some(end)}else{None},"answer_allowed":false,"meaning":"Current native question evidence. Use the existing human-selected question controls; this operation cannot answer or approve."}),
        )
    }

    /// Inspect predecessor source text on demand; helper content stays internal.
    #[allow(clippy::too_many_arguments)]
    pub fn interaction_workflow_result(
        &self,
        human: i64,
        actor: SessionKey,
        native: &str,
        id: &str,
        step: &str,
        revision: &str,
        offset: usize,
    ) -> Result<Value> {
        if offset > 1_000_000 {
            bail!("Result offset outside bound");
        }
        let conn = self.conn.lock().expect("store mutex poisoned");
        if owner_on(&conn, human, actor)?.1 != native {
            bail!("Actor binding changed");
        }
        let raw: String = conn.query_row(
            "SELECT doc FROM interaction_workflows WHERE id=?1 AND human_id=?2",
            params![id, human],
            |r| r.get(0),
        )?;
        let w: Workflow = serde_json::from_str(&raw)?;
        let index = w
            .plan
            .steps
            .iter()
            .position(|s| s.id == step)
            .context("Unknown workflow step")?;
        let eligible = actor == w.entry
            || w.plan
                .steps
                .get(index + 1)
                .is_some_and(|s| s.owner == actor && w.admissions[index + 1].is_some());
        if !eligible {
            bail!("Result outside this step's selected dependency");
        }
        let (source, d) = result_on(&conn, &w, index)?.context("Canonical result unavailable")?;
        if source["revision"].as_str() != Some(revision) || !completed(&source) {
            bail!("Canonical result changed or incomplete");
        }
        let length = d.body.chars().count();
        if offset > length {
            bail!("Result offset beyond source");
        }
        let text: String = d.body.chars().skip(offset).take(4000).collect();
        let end = offset + text.chars().count();
        Ok(
            json!({"schema":"telecodex.workflow-result-part/v1","source":source,"covered":d.covered,"text":text,"offset":offset,"next_offset":if end<length{Some(end)}else{None},"internal_evidence":true,"publication_grant":false}),
        )
    }
}

pub(super) fn input_allowed_on(conn: &Connection, author: &Value, i: &Instruction) -> Result<bool> {
    let id = author["workflow"]
        .as_str()
        .context("Missing workflow provenance")?;
    let raw: String = conn.query_row(
        "SELECT doc FROM interaction_workflows WHERE id=?1 AND human_id=?2",
        params![id, i.human_id],
        |r| r.get(0),
    )?;
    let w: Workflow = serde_json::from_str(&raw)?;
    if !live_on(conn, &w)? {
        return Ok(false);
    }
    let view = view_on(conn, &w)?;
    Ok(w.admissions.iter().enumerate().any(|(index, a)| {
        a.as_ref().is_some_and(|a| a.instruction == i.id)
            && !matches!(
                view["steps"][index]["phase"].as_str(),
                Some("predecessor_changed" | "correction_pending" | "withdrawn")
            )
    }))
}
pub(super) fn withdraw_on(conn: &Connection, caller: SessionKey) -> Result<()> {
    conn.execute("UPDATE interaction_workflows SET phase='withdrawn',doc=json_set(doc,'$.phase','withdrawn') WHERE entry_chat=?1 AND entry_topic=?2 AND phase='active'",params![caller.chat_id,caller.thread_id])?;
    Ok(())
}

// Share the existing original-selection exclusion with dispatch/creation/Matrix.
pub(super) fn unclaimed_on(
    conn: &Connection,
    human: i64,
    channel: &str,
    event: &str,
    update: Option<i64>,
) -> Result<()> {
    let held:bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM interaction_workflows WHERE human_id=?1 AND (json_extract(doc,'$.source.update_id')=?4 OR (json_extract(doc,'$.source.channel')=?2 AND json_extract(doc,'$.source.event_id')=?3)))",params![human,channel,event,update],|r|r.get(0))?;
    if held {
        bail!("Original input has an immutable workflow selection");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interaction::{ReturnRoute, TransportPhase as P};
    use crate::store::ConversationInputEvent as E;
    use tokio_util::sync::CancellationToken;

    fn session(s: &Store, topic: i64) -> SessionRecord {
        let key = SessionKey::new(100, Some(topic));
        s.ensure_session(key, 100, &super::super::tests::defaults())
            .unwrap();
        s.set_session_codex_thread(key, &format!("native-{topic}"))
            .unwrap();
        s.get_session(key).unwrap().unwrap()
    }
    fn turn(s: &Store, session: &SessionRecord, text: &str) -> i64 {
        let local = s
            .record_turn_started(
                session.id,
                &TurnRequest {
                    session_key: session.key,
                    from_user_id: 100,
                    prompt: text.into(),
                    runtime_instructions: None,
                    attachments: vec![],
                    review_mode: None,
                    override_search_mode: None,
                },
            )
            .unwrap();
        s.record_turn_native_thread(local, session.codex_thread_id.as_deref().unwrap())
            .unwrap();
        local
    }
    fn original(s: &Store, entry: &SessionRecord, text: &str) -> InputReference {
        let id = Uuid::now_v7().to_string();
        let binding = s.conversation_request_binding(100, entry.key).unwrap()["binding"]
            .as_str()
            .unwrap()
            .to_owned();
        s.conversation_input_capture(100, entry.key, &binding, &id, text)
            .unwrap();
        s.conversation_input_event(100, &id, E::Queue).unwrap();
        let local = turn(s, entry, text);
        s.conversation_input_event(
            100,
            &id,
            E::Begin {
                local_turn: local,
                steer: false,
            },
        )
        .unwrap();
        s.conversation_input_event(100, &id, E::Accepted { local_turn: local })
            .unwrap();
        InputReference::Conversation(id)
    }
    fn setup() -> (tempfile::NamedTempFile, Store, SessionRecord, WorkflowPlan) {
        let f = tempfile::NamedTempFile::new().unwrap();
        let s = Store::open(f.path(), &[100], &super::super::tests::defaults()).unwrap();
        let entry = session(&s, 1);
        let a = session(&s, 7);
        let b = session(&s, 8);
        let plan = WorkflowPlan {
            request_id: "selected-work".into(),
            input_ref: original(
                &s,
                &entry,
                "A produces a checkpoint, B consumes it; return here after verification and report the first checkpoint.",
            ),
            expires_at: (Utc::now() + Duration::hours(2)).to_rfc3339(),
            steps: vec![
                WorkflowStep {
                    id: "produce".into(),
                    owner: a.key,
                    after: None,
                    scope: "Produce the selected checkpoint".into(),
                    checkpoint: "Current canonical checkpoint".into(),
                    milestone: Some("Checkpoint ready".into()),
                },
                WorkflowStep {
                    id: "consume".into(),
                    owner: b.key,
                    after: Some("produce".into()),
                    scope: "Consume the exact first checkpoint and verify the selected outcome"
                        .into(),
                    checkpoint: "Receiving verification".into(),
                    milestone: None,
                },
            ],
        };
        (f, s, entry, plan)
    }
    fn declare(s: &Store, e: &SessionRecord, p: WorkflowPlan) -> Value {
        s.interaction_workflow_declare(
            100,
            e.key,
            "native-1",
            "conversation",
            p,
            &CancellationToken::new(),
        )
        .unwrap()
    }
    fn status(s: &Store, e: &SessionRecord, id: &str) -> Value {
        s.interaction_workflow_status(100, e.key, "native-1", id)
            .unwrap()
    }
    fn advance(s: &Store, e: &SessionRecord, v: &Value) -> Option<Instruction> {
        s.interaction_workflow_advance(
            100,
            e.key,
            "native-1",
            v["id"].as_str().unwrap(),
            v["revision"].as_str().unwrap(),
            &CancellationToken::new(),
        )
        .unwrap()
    }
    fn accept(s: &Store, i: &Instruction) -> i64 {
        let target = s.get_session(i.target).unwrap().unwrap();
        let local = turn(s, &target, &i.text);
        s.interaction_transport(&i.id, 100, local, P::Submitting, None)
            .unwrap();
        s.interaction_transport(
            &i.id,
            100,
            local,
            P::Accepted,
            Some(&format!("turn-{local}")),
        )
        .unwrap();
        local
    }
    fn answer(
        s: &Store,
        i: &Instruction,
        local: i64,
        item: &str,
        covered: Vec<String>,
        body: &str,
    ) -> Delivery {
        let text = format!(
            "{body}\n<daimon-result>{}</daimon-result>",
            json!({"requests":i.requests,"covered":covered,"completed":i.requests})
        );
        s.interaction_record_result(
            &i.id,
            100,
            local,
            i.native_thread.as_deref().unwrap(),
            &format!("turn-{local}"),
            item,
            &text,
            true,
        )
        .unwrap()
        .remove(0)
    }
    fn routed_result(
        s: &Store,
        entry: &SessionRecord,
        reference: &InputReference,
    ) -> (Instruction, Delivery) {
        let (mut source, _) = s
            .interaction_entry_original(entry.key, 100, "native-1", reference, "conversation")
            .unwrap();
        source.target_sessions = vec![
            s.get_session(SessionKey::new(100, Some(7)))
                .unwrap()
                .unwrap()
                .id,
        ];
        let (instruction, _) = s
            .interaction_admit(&source, SessionKey::new(100, Some(7)), RequestRelation::New)
            .unwrap();
        let local = accept(s, &instruction);
        let instruction = s
            .interaction_instruction(&instruction.id, 100)
            .unwrap()
            .unwrap();
        let delivery = answer(
            s,
            &instruction,
            local,
            "ordinary-source",
            vec![instruction.id.clone()],
            &"Result α\n".repeat(400),
        );
        (instruction, delivery)
    }
    #[test]
    fn ordinary_routed_result_returns_exact_source_without_declaring_workflow() {
        let (_f, s, entry, plan) = setup();
        let (_, delivery) = routed_result(&s, &entry, &plan.input_ref);
        let selected = s
            .native_routed_result_reserve(100, entry.key, &delivery.id)
            .unwrap()
            .unwrap();
        assert_eq!(selected.name, "conversation_attention");
        let packet: Value = serde_json::from_str(&selected.output).unwrap();
        assert_eq!(packet["schema"], "telecodex.native-attention/v1");
        assert_eq!(packet["original_input_ref"], json!(plan.input_ref));
        assert!(packet.get("workflow").is_none());
        assert!(packet["evidence"]["source"].get("text").is_none());
        assert_eq!(packet["source_text_complete"], false);
        let revision = packet["evidence"]["revision"].as_str().unwrap();
        let part = s
            .native_routed_source(100, entry.key, "native-1", &selected.id, revision, 0)
            .unwrap();
        assert_eq!(part["text"].as_str().unwrap().chars().count(), 1600);
        assert_eq!(part["next_offset"], 1600);
        assert!(
            s.native_routed_source(100, entry.key, "wrong-native", &selected.id, revision, 0)
                .is_err()
        );
        assert!(
            s.native_routed_source(
                100,
                entry.key,
                "native-1",
                &selected.id,
                "wrong-revision",
                0
            )
            .is_err()
        );
        assert!(
            s.native_routed_result_reserve(100, entry.key, &delivery.id)
                .unwrap()
                .is_none()
        );
        s.native_routed_reported(100, entry.key, "native-1", &selected.id, revision)
            .unwrap();
        assert!(s.workflow_native_dispatch(&selected.id).is_err());
        let conn = s.conn.lock().unwrap();
        assert_eq!(
            conn.query_row("SELECT count(*) FROM interaction_workflows", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert_eq!(
            conn.query_row("SELECT count(*) FROM interaction_sources", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            1
        );
    }
    #[test]
    fn ordinary_return_reuses_exact_attempt_and_source_publication_fences() {
        let (_f, s, entry, plan) = setup();
        let (_, delivery) = routed_result(&s, &entry, &plan.input_ref);
        let selected = s
            .native_routed_result_reserve(100, entry.key, &delivery.id)
            .unwrap()
            .unwrap();
        let local = turn(&s, &entry, "");
        s.workflow_native_link(&selected.id, local).unwrap();
        s.workflow_native_submitting(&selected.id).unwrap();
        s.workflow_native_outcome_attempt(
            &selected.id,
            1,
            "accepted",
            Some("ordinary-attention-physical"),
        )
        .unwrap();
        s.record_native_message(
            local,
            "native-1",
            "ordinary-attention-physical",
            "offer",
            Some("final_answer"),
            "The result is ready; shall we review it?",
        )
        .unwrap();
        let list = s
            .interaction_presentation_list(100, entry.key, 0, 0)
            .unwrap();
        let item = list["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["preview"] == "The result is ready; shall we review it?")
            .unwrap();
        assert_eq!(item["native_returns"], json!([selected.id]));
        assert!(item.get("workflow_returns").is_none());
        assert_eq!(item["current"], true);
        assert!(
            s.native_returns_recover_unsent(100, entry.key, false, true)
                .unwrap()
                .is_empty()
        );
        // Expiry invalidates callback publication without inventing another input.
        let conn = s.conn.lock().unwrap();
        conn.execute("UPDATE interaction_deliveries SET native_return=json_set(native_return,'$.expires_at','2000-01-01T00:00:00Z') WHERE id=?1", [&delivery.id]).unwrap();
        drop(conn);
        assert!(s.workflow_native_dispatch(&selected.id).is_err());
        let list = s
            .interaction_presentation_list(100, entry.key, 0, 0)
            .unwrap();
        let item = list["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["preview"] == "The result is ready; shall we review it?")
            .unwrap();
        assert_eq!(item["current"], false);
    }
    #[test]
    fn ordinary_question_return_reads_exact_question_and_never_answers_it() {
        let (_f, s, entry, plan) = setup();
        let (mut source, _) = s
            .interaction_entry_original(entry.key, 100, "native-1", &plan.input_ref, "conversation")
            .unwrap();
        source.target_sessions = vec![
            s.get_session(SessionKey::new(100, Some(7)))
                .unwrap()
                .unwrap()
                .id,
        ];
        let (instruction, _) = s
            .interaction_admit(&source, SessionKey::new(100, Some(7)), RequestRelation::New)
            .unwrap();
        let local = accept(&s, &instruction);
        let question = HumanInteraction {
            id: "c".repeat(32),
            kind: "native".into(),
            state: "pending".into(),
            requester: 100,
            native_thread: "native-7".into(),
            local_turn: local,
            title: "Receiving choice".into(),
            action: "Answer".into(),
            blocking: true,
            source: json!({"kind":"question","conversationIndex":0,"request":{"questions":[{"id":"choice","question":"Which receiving target?","options":[]}]}}),
        };
        s.save_interaction(instruction.target, &question, Some("ordinary-question"))
            .unwrap();
        let selected = s
            .native_routed_questions_reserve(100, entry.key, local)
            .unwrap()
            .remove(0);
        let packet: Value = serde_json::from_str(&selected.output).unwrap();
        assert_eq!(packet["source_text_complete"], false);
        let revision = packet["evidence"]["revision"].as_str().unwrap();
        let part = s
            .native_routed_source(100, entry.key, "native-1", &selected.id, revision, 0)
            .unwrap();
        assert!(
            part["text"]
                .as_str()
                .unwrap()
                .contains("Which receiving target?")
        );
        assert_eq!(part["answer_allowed"], false);
        s.native_routed_reported(100, entry.key, "native-1", &selected.id, revision)
            .unwrap();
        let conn = s.conn.lock().unwrap();
        assert_eq!(
            conn.query_row(
                "SELECT state FROM human_interactions WHERE id=?1",
                [&question.id],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
            "pending"
        );
        drop(conn);
        assert!(
            s.native_routed_questions_reserve(100, entry.key, local)
                .unwrap()
                .is_empty()
        );
        s.transition_interaction(instruction.target, 100, &question.id, "pending", "expired")
            .unwrap();
        assert!(
            s.native_routed_source(100, entry.key, "native-1", &selected.id, revision, 0)
                .is_err()
        );
        let direct = session(&s, 8);
        let direct_local = turn(&s, &direct, "Direct topic work");
        let mut direct_question = question.clone();
        direct_question.id = "d".repeat(32);
        direct_question.native_thread = "native-8".into();
        direct_question.local_turn = direct_local;
        s.save_interaction(direct.key, &direct_question, Some("direct-only-question"))
            .unwrap();
        assert!(
            s.native_routed_questions_reserve(100, entry.key, direct_local)
                .unwrap()
                .is_empty()
        );
    }
    #[test]
    fn ordinary_return_restart_preserves_unsent_uncertainty_and_finite_retry() {
        let (file, s, entry, plan) = setup();
        let (instruction, first) = routed_result(&s, &entry, &plan.input_ref);
        let unsent = s
            .native_routed_result_reserve(100, entry.key, &first.id)
            .unwrap()
            .unwrap();
        let second = answer(
            &s,
            &instruction,
            instruction.local_turn.unwrap(),
            "second-ordinary-source",
            vec![instruction.id.clone()],
            "Another exact result",
        );
        let unknown = s
            .native_routed_result_reserve(100, entry.key, &second.id)
            .unwrap()
            .unwrap();
        let local = turn(&s, &entry, "");
        s.workflow_native_link(&unknown.id, local).unwrap();
        s.workflow_native_submitting(&unknown.id).unwrap();
        drop(s);
        let s = Store::open(file.path(), &[100], &super::super::tests::defaults()).unwrap();
        let conn = s.conn.lock().unwrap();
        let unknown_doc: String = conn
            .query_row(
                "SELECT native_return FROM interaction_deliveries WHERE id=?1",
                [&second.id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&unknown_doc).unwrap()["returns"][0]["phase"],
            "uncertain"
        );
        conn.execute("UPDATE interaction_deliveries SET native_return=json_set(native_return,'$.returns[0].retry_after',?2) WHERE id=?1", params![first.id,Utc::now().timestamp()-1]).unwrap();
        drop(conn);
        assert!(
            s.native_returns_recover_unsent(100, entry.key, false, false)
                .unwrap()
                .is_empty()
        );
        let retry = s
            .native_returns_recover_unsent(100, entry.key, false, true)
            .unwrap()
            .remove(0);
        assert_eq!(retry.id, unsent.id);
        assert_eq!(retry.attempts, 2);
        assert_eq!(
            serde_json::from_str::<Value>(&retry.output).unwrap()["evidence"],
            serde_json::from_str::<Value>(&unsent.output).unwrap()["evidence"]
        );
        s.workflow_native_outcome_attempt(&retry.id, 1, "accepted", Some("stale-attempt"))
            .unwrap();
        assert_eq!(s.workflow_native_dispatch(&retry.id).unwrap().attempts, 2);
        assert!(s.workflow_native_dispatch(&unknown.id).is_err());
        s.workflow_native_outcome_attempt(&retry.id, 2, "retained_unsent", None)
            .unwrap();
        let conn = s.conn.lock().unwrap();
        conn.execute("UPDATE interaction_deliveries SET native_return=json_set(native_return,'$.returns[0].retry_after',?2) WHERE id=?1", params![first.id,Utc::now().timestamp()-1]).unwrap();
        drop(conn);
        let last = s
            .native_returns_recover_unsent(100, entry.key, false, true)
            .unwrap()
            .remove(0);
        assert_eq!(last.id, unsent.id);
        assert_eq!(last.attempts, 3);
        s.workflow_native_outcome_attempt(&last.id, 3, "retained_unsent", None)
            .unwrap();
        assert!(
            s.native_returns_recover_unsent(100, entry.key, false, true)
                .unwrap()
                .is_empty()
        );
        assert!(s.workflow_native_dispatch(&unknown.id).is_err());
    }
    #[test]
    fn ordinary_return_correction_retires_old_source_without_replaying_work() {
        let (_file, s, entry, plan) = setup();
        let (instruction, delivery) = routed_result(&s, &entry, &plan.input_ref);
        let selected = s
            .native_routed_result_reserve(100, entry.key, &delivery.id)
            .unwrap()
            .unwrap();
        let correction = original(
            &s,
            &entry,
            "Change the selected requirement before announcing the result.",
        );
        let (mut source, _) = s
            .interaction_entry_original(entry.key, 100, "native-1", &correction, "conversation")
            .unwrap();
        source.target_sessions = vec![instruction.target_session];
        let (amended, _) = s
            .interaction_admit(
                &source,
                instruction.target,
                RequestRelation::Amends(instruction.requests.clone()),
            )
            .unwrap();
        accept(&s, &amended);
        assert!(s.workflow_native_dispatch(&selected.id).is_err());
        assert!(
            s.native_routed_result_reserve(100, entry.key, &delivery.id)
                .unwrap()
                .is_none()
        );
        let packet: Value = serde_json::from_str(&selected.output).unwrap();
        assert!(
            s.native_routed_source(
                100,
                entry.key,
                "native-1",
                &selected.id,
                packet["evidence"]["revision"].as_str().unwrap(),
                0
            )
            .is_err()
        );
        let conn = s.conn.lock().unwrap();
        assert_eq!(
            conn.query_row("SELECT count(*) FROM interaction_instructions", [], |r| r
                .get::<_, i64>(
                0
            ))
            .unwrap(),
            2
        );
        assert_eq!(
            conn.query_row("SELECT count(*) FROM interaction_workflows", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
    #[test]
    fn ordinary_callback_replacement_keeps_expired_source_fenced_and_human_input_usable() {
        let (file, s, entry, plan) = setup();
        let (_, delivery) = routed_result(&s, &entry, &plan.input_ref);
        let returned = s
            .native_routed_result_reserve(100, entry.key, &delivery.id)
            .unwrap()
            .unwrap();
        let old = turn(&s, &entry, "");
        s.workflow_native_link(&returned.id, old).unwrap();
        s.native_collection_submitting(old, None).unwrap();
        s.native_collection_started(old, "callback-physical")
            .unwrap();
        s.workflow_native_submitting(&returned.id).unwrap();
        s.workflow_native_outcome(&returned.id, "accepted", Some("callback-physical"))
            .unwrap();
        assert!(s.workflow_native_visible(old).unwrap());
        {
            let conn = s.conn.lock().unwrap();
            conn.execute("UPDATE interaction_deliveries SET native_return=json_set(native_return,'$.expires_at','2000-01-01T00:00:00Z') WHERE id=?1",[&delivery.id]).unwrap();
        }
        assert!(!s.workflow_native_visible(old).unwrap());
        drop(s);
        let s = Store::open(file.path(), &[100], &super::super::tests::defaults()).unwrap();
        let successor = turn(
            &s,
            &entry,
            "Owner-local collector attachment; no new native input.",
        );
        s.topic_direct_register(successor, 100, entry.key).unwrap();
        s.record_collector_attachment(successor, entry.key, 100, "native-1", "callback-physical")
            .unwrap();
        s.native_collection_started(successor, "callback-physical")
            .unwrap();
        s.record_native_message(
            successor,
            "native-1",
            "callback-physical",
            "late-expired-offer",
            Some("final_answer"),
            "Stale callback must stay internal after collector replacement.",
        )
        .unwrap();
        let page = s
            .interaction_presentation_list(100, entry.key, 0, 0)
            .unwrap();
        assert!(
            !page["items"]
                .as_array()
                .unwrap()
                .iter()
                .any(|i| i["native_item"] == "late-expired-offer"),
            "Replacing a collector must not erase the exact callback source fence"
        );
        // A real later human input in the same execution keeps ordinary rights.
        let binding = s.conversation_request_binding(100, entry.key).unwrap()["binding"]
            .as_str()
            .unwrap()
            .to_owned();
        let id = Uuid::now_v7().to_string();
        s.conversation_input_capture(
            100,
            entry.key,
            &binding,
            &id,
            "My later actual human question",
        )
        .unwrap();
        s.conversation_input_event(100, &id, E::Queue).unwrap();
        s.conversation_input_event(
            100,
            &id,
            E::Begin {
                local_turn: successor,
                steer: true,
            },
        )
        .unwrap();
        s.conversation_input_event(
            100,
            &id,
            E::Accepted {
                local_turn: successor,
            },
        )
        .unwrap();
        s.record_native_message(
            successor,
            "native-1",
            "callback-physical",
            "real-human-answer",
            Some("final_answer"),
            "The later actual human answer remains visible.",
        )
        .unwrap();
        let page = s
            .interaction_presentation_list(100, entry.key, 0, 0)
            .unwrap();
        assert!(
            page["items"]
                .as_array()
                .unwrap()
                .iter()
                .any(|i| i["native_item"] == "real-human-answer")
        );
    }
    fn replacement_collector(s: &Store, entry: &SessionRecord, physical: &str) -> i64 {
        let local = turn(
            s,
            entry,
            "Owner-local collector attachment; no new native input.",
        );
        s.topic_direct_register(local, 100, entry.key).unwrap();
        s.record_collector_attachment(local, entry.key, 100, "native-1", physical)
            .unwrap();
        s.native_collection_started(local, physical).unwrap();
        local
    }
    #[test]
    fn replacement_collector_retains_unknown_sibling_and_the_exact_source_set() {
        let (_file, s, entry, plan) = setup();
        let (instruction, first) = routed_result(&s, &entry, &plan.input_ref);
        let second = answer(
            &s,
            &instruction,
            instruction.local_turn.unwrap(),
            "another-source",
            vec![instruction.id.clone()],
            "Another current result",
        );
        let a = s
            .native_routed_result_reserve(100, entry.key, &first.id)
            .unwrap()
            .unwrap();
        let b = s
            .native_routed_result_reserve(100, entry.key, &second.id)
            .unwrap()
            .unwrap();
        let old = turn(&s, &entry, "");
        s.native_collection_started(old, "mixed-physical").unwrap();
        for id in [&a.id, &b.id] {
            s.workflow_native_link(id, old).unwrap();
            s.workflow_native_submitting(id).unwrap();
        }
        s.workflow_native_outcome(&a.id, "accepted", Some("mixed-physical"))
            .unwrap();
        s.workflow_native_outcome(&b.id, "uncertain", None).unwrap();
        let next = replacement_collector(&s, &entry, "mixed-physical");
        assert!(
            !s.workflow_native_visible(next).unwrap(),
            "An unknown sibling cannot borrow the accepted source's publication"
        );
        s.workflow_native_outcome(&b.id, "accepted", Some("mixed-physical"))
            .unwrap();
        s.record_native_message(
            next,
            "native-1",
            "mixed-physical",
            "recovered-offer",
            Some("final_answer"),
            "Both exact sources are now confirmed.",
        )
        .unwrap();
        let page = s
            .interaction_presentation_list(100, entry.key, 0, 0)
            .unwrap();
        let item = page["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["native_item"] == "recovered-offer")
            .unwrap();
        let ids = item["native_returns"].as_array().unwrap();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&json!(a.id)) && ids.contains(&json!(b.id)));
        assert!(
            !s.workflow_native_visible(old).unwrap(),
            "The earlier local cannot borrow the successor's grant"
        );
        let newer = replacement_collector(&s, &entry, "mixed-physical");
        assert!(s.workflow_native_visible(newer).unwrap());
        assert!(!s.workflow_native_visible(next).unwrap());
        let other = replacement_collector(&s, &entry, "different-physical");
        let conn = s.conn.lock().unwrap();
        assert!(
            !conn
                .query_row(
                    "SELECT EXISTS(SELECT 1 FROM bot_state WHERE key=?1)",
                    [format!("workflow_return_local:{other}")],
                    |r| r.get::<_, bool>(0)
                )
                .unwrap(),
            "Another physical turn cannot inherit these source bindings"
        );
    }
    #[test]
    fn replacement_collector_preserves_real_human_execution_publication() {
        let (_file, s, entry, plan) = setup();
        let (_, delivery) = routed_result(&s, &entry, &plan.input_ref);
        let selected = s
            .native_routed_result_reserve(100, entry.key, &delivery.id)
            .unwrap()
            .unwrap();
        let InputReference::Conversation(id) = &plan.input_ref else {
            unreachable!()
        };
        let old = {
            let conn = s.conn.lock().unwrap();
            conn.query_row(
                "SELECT local_turn FROM conversation_inputs WHERE id=?1",
                [id],
                |r| r.get::<_, i64>(0),
            )
            .unwrap()
        };
        s.native_collection_started(old, "human-physical").unwrap();
        s.workflow_native_link(&selected.id, old).unwrap();
        s.workflow_native_submitting(&selected.id).unwrap();
        s.workflow_native_outcome(&selected.id, "accepted", Some("human-physical"))
            .unwrap();
        {
            let conn = s.conn.lock().unwrap();
            conn.execute("UPDATE interaction_deliveries SET native_return=json_set(native_return,'$.expires_at','2000-01-01T00:00:00Z') WHERE id=?1",[delivery.id]).unwrap();
        }
        let next = replacement_collector(&s, &entry, "human-physical");
        assert!(s.workflow_native_visible(next).unwrap());
        s.record_native_message(
            next,
            "native-1",
            "human-physical",
            "real-original-answer",
            Some("final_answer"),
            "The actual human execution keeps its publication.",
        )
        .unwrap();
        let page = s
            .interaction_presentation_list(100, entry.key, 0, 0)
            .unwrap();
        assert!(
            page["items"]
                .as_array()
                .unwrap()
                .iter()
                .any(|i| i["native_item"] == "real-original-answer")
        );
    }
    #[test]
    fn declared_callback_replacement_cannot_publish_an_expired_workflow() {
        let (_file, s, entry, plan) = setup();
        let workflow = declare(&s, &entry, plan);
        let instruction = advance(&s, &entry, &workflow).unwrap();
        let worker = accept(&s, &instruction);
        answer(
            &s,
            &instruction,
            worker,
            "workflow-source",
            vec![instruction.id.clone()],
            "Declared evidence",
        );
        let selected = s.workflow_native_reserve(100, entry.key).unwrap().remove(0);
        let old = turn(&s, &entry, "");
        s.native_collection_started(old, "declared-physical")
            .unwrap();
        s.workflow_native_link(&selected.id, old).unwrap();
        s.workflow_native_submitting(&selected.id).unwrap();
        s.workflow_native_outcome(&selected.id, "accepted", Some("declared-physical"))
            .unwrap();
        {
            let conn = s.conn.lock().unwrap();
            conn.execute("UPDATE interaction_workflows SET doc=json_set(doc,'$.plan.expires_at','2000-01-01T00:00:00Z') WHERE id=?1",[workflow["id"].as_str().unwrap()]).unwrap();
        }
        let next = replacement_collector(&s, &entry, "declared-physical");
        assert!(!s.workflow_native_visible(next).unwrap());
    }
    #[test]
    fn workflow_retains_ownership_exact_scope_milestones_and_final_integration() {
        let (_f, s, e, plan) = setup();
        let initial = declare(&s, &e, plan.clone());
        let id = initial["id"].as_str().unwrap();
        assert_eq!(declare(&s, &e, plan.clone()), initial);
        let mut changed = plan.clone();
        changed.steps[0].scope = "Different work".into();
        assert!(
            s.interaction_workflow_declare(
                100,
                e.key,
                "native-1",
                "conversation",
                changed,
                &CancellationToken::new()
            )
            .is_err()
        );
        // A selected workflow cannot also dispatch the whole original separately.
        assert!(
            s.interaction_entry_original(e.key, 100, "native-1", &plan.input_ref, "conversation")
                .is_err()
        );
        let first = advance(&s, &e, &initial).unwrap();
        assert!(s.interaction_session_input_allowed(&first.id, 100).unwrap());
        let author = s.interaction_author(&first.id, 100).unwrap().unwrap();
        assert_eq!(author["kind"], "workflow_step");
        assert_eq!(author["input_ref"], json!(plan.input_ref));
        assert!(
            s.interaction_workflow_advance(
                100,
                e.key,
                "native-1",
                id,
                initial["revision"].as_str().unwrap(),
                &CancellationToken::new()
            )
            .is_err()
        );
        assert!(advance(&s, &e, &status(&s, &e, id)).is_none());
        let local = accept(&s, &first);
        assert!(
            s.interaction_session_request(
                first.target,
                100,
                "native-7",
                &format!("turn-{local}"),
                &first.id,
                "recursive",
                plan.steps[1].owner,
                "Unplanned helper chain",
                &CancellationToken::new()
            )
            .is_err()
        );
        let d = answer(
            &s,
            &first,
            local,
            "first",
            vec![first.id.clone()],
            &"Canonical checkpoint α\n".repeat(250),
        );
        assert_eq!(d.internal, Some(true));
        let ready = status(&s, &e, id);
        assert_eq!(ready["steps"][0]["phase"], "completed");
        assert_eq!(ready["steps"][1]["phase"], "ready");
        assert_eq!(ready["phase"], "active");
        assert!(
            s.interaction_workflow_finish(
                100,
                e.key,
                "native-1",
                id,
                ready["revision"].as_str().unwrap(),
                "Too early"
            )
            .is_err()
        );
        let attention = s.interaction_workflow_attention(100, e.key).unwrap();
        assert_eq!(attention["envelopes"].as_array().unwrap().len(), 1);
        let event = &attention["envelopes"][0];
        assert_eq!(event["reason"], "reporting_milestone");
        assert!(event["source"]["text"].is_null());
        s.interaction_workflow_reported(
            100,
            e.key,
            "native-1",
            id,
            event["revision"].as_str().unwrap(),
        )
        .unwrap();
        let second = advance(&s, &e, &ready).unwrap();
        // Advancing another step cannot replay a previously reported milestone.
        assert!(
            s.interaction_workflow_attention(100, e.key).unwrap()["envelopes"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        let source = &ready["steps"][0]["result"];
        let part = s
            .interaction_workflow_result(
                100,
                second.target,
                "native-8",
                id,
                "produce",
                source["revision"].as_str().unwrap(),
                0,
            )
            .unwrap();
        assert_eq!(part["text"].as_str().unwrap().chars().count(), 4000);
        assert_eq!(part["next_offset"], 4000);
        assert!(
            s.interaction_workflow_result(
                100,
                first.target,
                "native-7",
                id,
                "produce",
                source["revision"].as_str().unwrap(),
                0
            )
            .is_err()
        );
        let local = accept(&s, &second);
        answer(
            &s,
            &second,
            local,
            "verified",
            vec![second.id.clone()],
            "Receiving verified",
        );
        let final_view = status(&s, &e, id);
        assert_eq!(final_view["phase"], "awaiting_integration");
        let final_event = s.interaction_workflow_attention(100, e.key).unwrap();
        assert_eq!(
            final_event["envelopes"][0]["reason"],
            "integration_required"
        );
        let done = s
            .interaction_workflow_finish(
                100,
                e.key,
                "native-1",
                id,
                final_view["revision"].as_str().unwrap(),
                "Integrated receiving verification",
            )
            .unwrap();
        assert_eq!(done["phase"], "completed");
        assert_eq!(
            s.interaction_workflow_finish(
                100,
                e.key,
                "native-1",
                id,
                final_view["revision"].as_str().unwrap(),
                "Integrated receiving verification"
            )
            .unwrap(),
            done
        );
        assert!(
            s.interaction_workflow_attention(100, e.key).unwrap()["envelopes"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        let conn = s.conn.lock().unwrap();
        assert_eq!(
            conn.query_row(
                "SELECT count(*) FROM interaction_public_deliveries",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
    }
    #[test]
    fn native_return_publication_uses_original_reporting_scope_and_preserves_history() {
        let (_f, s, e, plan) = setup();
        let initial = declare(&s, &e, plan);
        assert!(s.workflow_native_reserve(100, e.key).unwrap().is_empty());
        let first = advance(&s, &e, &initial).unwrap();
        let worker = accept(&s, &first);
        answer(
            &s,
            &first,
            worker,
            "checkpoint",
            vec![first.id.clone()],
            "Selected reporting checkpoint",
        );
        let before = status(&s, &e, initial["id"].as_str().unwrap());
        let selected = s.workflow_native_reserve(100, e.key).unwrap().remove(0);
        assert!(selected.output.contains("Attributed internal"));
        let reserved_attention = s.interaction_workflow_attention(100, e.key).unwrap();
        let reserved_event = &reserved_attention["envelopes"][0];
        assert_eq!(reserved_event["native_return"]["id"], selected.id);
        assert_eq!(reserved_event["native_return"]["phase"], "reserved");
        let source_revision = reserved_event["revision"].clone();
        assert_eq!(
            status(&s, &e, initial["id"].as_str().unwrap())["revision"],
            before["revision"]
        );
        assert!(s.workflow_native_reserve(100, e.key).unwrap().is_empty());
        let local = turn(&s, &e, "");
        s.workflow_native_link(&selected.id, local).unwrap();
        s.native_collection_submitting(local, None).unwrap();
        s.native_collection_started(local, "return-turn").unwrap();
        s.workflow_native_submitting(&selected.id).unwrap();
        assert!(!s.workflow_native_visible(local).unwrap());
        s.workflow_native_outcome(&selected.id, "accepted", Some("return-turn"))
            .unwrap();
        s.workflow_native_outcome(&selected.id, "uncertain", None)
            .unwrap();
        let accepted_attention = s.interaction_workflow_attention(100, e.key).unwrap();
        let accepted_event = &accepted_attention["envelopes"][0];
        assert_eq!(accepted_event["native_return"]["phase"], "accepted");
        assert_eq!(
            accepted_event["native_return"]["native_turn"],
            "return-turn"
        );
        assert_eq!(accepted_event["revision"], source_revision);
        assert!(s.workflow_native_visible(local).unwrap());
        assert!(s.conversation_turn_return(local).unwrap());
        assert!(
            s.start_response_draft(100, e.key, local, "native-1", "return-turn")
                .unwrap()
        );
        assert!(
            s.interaction_response_draft(
                100,
                e.key,
                local,
                "native-1",
                "return-turn",
                "draft",
                "",
                true,
                false
            )
            .unwrap()
        );
        assert!(
            s.interaction_response_draft(
                100,
                e.key,
                local,
                "native-1",
                "return-turn",
                "draft",
                "Selected milestone. More current draft text",
                false,
                false
            )
            .unwrap()
        );
        let prefix = "Selected milestone. ";
        let live = format!(
            "live:return-turn:draft:0:{}:{:x}",
            prefix.chars().count(),
            Sha256::digest(prefix.as_bytes())
        );
        let binding = s.interaction_presentation_list(100, e.key, 0, 0).unwrap()["binding"]
            .as_str()
            .unwrap()
            .to_owned();
        let revision = format!(
            "{:x}",
            Sha256::digest(format!("{binding}:{live}").as_bytes())
        );
        s.interaction_live_speech_part(100, e.key, &live, &revision, 0)
            .expect("Current accepted workflow draft prefix is readable");
        s.record_native_message(
            local,
            "native-1",
            "return-turn",
            "reported",
            Some("final_answer"),
            "The selected milestone is ready.",
        )
        .unwrap();
        let page = s.interaction_presentation_list(100, e.key, 0, 0).unwrap();
        let reported = page["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|item| item["preview"] == "The selected milestone is ready.")
            .unwrap();
        assert_eq!(reported["current"], true);
        s.interaction_withdraw_session_requests(e.key).unwrap();
        assert!(!s.workflow_native_visible(local).unwrap());
        assert!(
            s.interaction_live_speech_part(100, e.key, &live, &revision, 0)
                .is_err()
        );
        assert!(
            s.interaction_presentation_list(100, e.key, 0, 0).unwrap()["response_draft"].is_null()
        );
        assert!(
            !s.start_response_draft(100, e.key, local, "native-1", "return-turn")
                .unwrap()
        );
        s.record_native_message(
            local,
            "native-1",
            "return-turn",
            "stale",
            Some("final_answer"),
            "A stale callback cannot obtain publication.",
        )
        .unwrap();
        let page = s.interaction_presentation_list(100, e.key, 0, 0).unwrap();
        let items = page["items"].as_array().unwrap();
        assert!(
            items
                .iter()
                .any(|item| item["preview"] == "The selected milestone is ready."
                    && item["current"] == false)
        );
        assert!(
            !items
                .iter()
                .any(|item| item["preview"] == "A stale callback cannot obtain publication.")
        );
        // A genuinely later human input on the same writer retains ordinary
        // publication even though this earlier internal return is now retired.
        let id = Uuid::now_v7().to_string();
        let binding = s.conversation_request_binding(100, e.key).unwrap()["binding"]
            .as_str()
            .unwrap()
            .to_owned();
        s.conversation_input_capture(100, e.key, &binding, &id, "My later actual human question")
            .unwrap();
        s.conversation_input_event(100, &id, E::Queue).unwrap();
        s.conversation_input_event(
            100,
            &id,
            E::Begin {
                local_turn: local,
                steer: true,
            },
        )
        .unwrap();
        s.conversation_input_event(100, &id, E::Accepted { local_turn: local })
            .unwrap();
        assert!(s.workflow_native_visible(local).unwrap());
        s.record_native_message(
            local,
            "native-1",
            "return-turn",
            "human",
            Some("final_answer"),
            "The ordinary human answer remains visible.",
        )
        .unwrap();
        assert!(
            s.interaction_presentation_list(100, e.key, 0, 0).unwrap()["items"]
                .as_array()
                .unwrap()
                .iter()
                .any(|i| i["preview"] == "The ordinary human answer remains visible.")
        );
    }
    fn make_return_due(s: &Store, e: &SessionRecord, workflow: &str) {
        let conn = s.conn.lock().unwrap();
        let mut w = load_on(&conn, 100, e.key, workflow).unwrap();
        for r in &mut w.native_returns {
            r.retry_after = Some(Utc::now().timestamp() - 1);
        }
        save_on(&conn, &w).unwrap();
    }
    #[test]
    fn known_unsent_return_recovers_same_source_with_finite_attempts() {
        let (f, s, e, plan) = setup();
        let v = declare(&s, &e, plan);
        let workflow = v["id"].as_str().unwrap();
        let first = advance(&s, &e, &v).unwrap();
        let worker = accept(&s, &first);
        answer(
            &s,
            &first,
            worker,
            "source",
            vec![first.id.clone()],
            "Ready",
        );
        let original = s.workflow_native_reserve(100, e.key).unwrap().remove(0);
        let old = turn(&s, &e, "");
        s.workflow_native_link(&original.id, old).unwrap();
        s.workflow_native_outcome_attempt(&original.id, 1, "retained_unsent", None)
            .unwrap();
        assert!(
            s.workflow_native_recover_unsent(100, e.key)
                .unwrap()
                .is_empty()
        );
        for attempt in 2..=3 {
            make_return_due(&s, &e, workflow);
            let retry = s
                .workflow_native_recover_unsent(100, e.key)
                .unwrap()
                .remove(0);
            assert_eq!(retry.id, original.id);
            assert_eq!(retry.attempts, attempt);
            assert_eq!(
                serde_json::from_str::<Value>(&retry.output).unwrap()["evidence"],
                serde_json::from_str::<Value>(&original.output).unwrap()["evidence"]
            );
            assert!(
                s.workflow_native_recover_unsent(100, e.key)
                    .unwrap()
                    .is_empty()
            );
            // A late ACK/error for the earlier attempt cannot mutate this claim.
            s.workflow_native_outcome_attempt(
                &retry.id,
                attempt - 1,
                "accepted",
                Some("stale-ack"),
            )
            .unwrap();
            s.workflow_native_outcome_attempt(&retry.id, attempt - 1, "uncertain", None)
                .unwrap();
            assert_eq!(
                s.workflow_native_dispatch(&retry.id).unwrap().attempts,
                attempt
            );
            let current = turn(&s, &e, "");
            s.workflow_native_link(&retry.id, current).unwrap();
            s.workflow_native_submitting(&retry.id).unwrap();
            assert!(!s.workflow_native_visible(old).unwrap());
            s.workflow_native_outcome_attempt(&retry.id, attempt, "retained_unsent", None)
                .unwrap();
        }
        make_return_due(&s, &e, workflow);
        assert!(
            s.workflow_native_recover_unsent(100, e.key)
                .unwrap()
                .is_empty()
        );
        let workflow = workflow.to_owned();
        drop(s);
        let s = Store::open(f.path(), &[100], &super::super::tests::defaults()).unwrap();
        assert!(
            s.workflow_native_recover_unsent(100, e.key)
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            status(&s, &e, &workflow)["native_returns"]["retained"][0]["attempts"],
            3
        );
    }
    #[test]
    fn return_recovery_excludes_uncertain_rejected_accepted_and_changed_sources() {
        for outcome in ["uncertain", "rejected", "accepted", "changed"] {
            let (_f, s, e, plan) = setup();
            let v = declare(&s, &e, plan);
            let workflow = v["id"].as_str().unwrap();
            let first = advance(&s, &e, &v).unwrap();
            let worker = accept(&s, &first);
            answer(
                &s,
                &first,
                worker,
                "source",
                vec![first.id.clone()],
                "Ready",
            );
            let r = s.workflow_native_reserve(100, e.key).unwrap().remove(0);
            let local = turn(&s, &e, "");
            s.workflow_native_link(&r.id, local).unwrap();
            s.workflow_native_submitting(&r.id).unwrap();
            if outcome == "changed" {
                s.workflow_native_outcome_attempt(&r.id, 1, "retained_unsent", None)
                    .unwrap();
                answer(
                    &s,
                    &first,
                    worker,
                    "new-source",
                    vec![first.id.clone()],
                    "Corrected",
                );
            } else {
                s.workflow_native_outcome_attempt(
                    &r.id,
                    1,
                    outcome,
                    (outcome == "accepted").then_some("actual-ack"),
                )
                .unwrap();
            }
            make_return_due(&s, &e, workflow);
            assert!(
                s.workflow_native_recover_unsent(100, e.key)
                    .unwrap()
                    .is_empty(),
                "{outcome}"
            );
        }
    }
    #[test]
    fn native_return_restart_keeps_known_unsent_uncertainty_and_ack_separate() {
        let (f, s, e, plan) = setup();
        let v = declare(&s, &e, plan);
        let id = v["id"].as_str().unwrap();
        let first = advance(&s, &e, &v).unwrap();
        let worker = accept(&s, &first);
        answer(
            &s,
            &first,
            worker,
            "a",
            vec![first.id.clone()],
            "First checkpoint",
        );
        let a = s.workflow_native_reserve(100, e.key).unwrap().remove(0);
        // Failure before a native submitting observation remains known unsent.
        s.workflow_native_outcome(&a.id, "uncertain", None).unwrap();
        answer(
            &s,
            &first,
            worker,
            "b",
            vec![first.id.clone()],
            "Revised checkpoint",
        );
        let b = s.workflow_native_reserve(100, e.key).unwrap().remove(0);
        let local = turn(&s, &e, "");
        s.workflow_native_link(&b.id, local).unwrap();
        s.workflow_native_submitting(&b.id).unwrap();
        answer(
            &s,
            &first,
            worker,
            "c",
            vec![first.id.clone()],
            "Current checkpoint",
        );
        let c = s.workflow_native_reserve(100, e.key).unwrap().remove(0);
        let local = turn(&s, &e, "");
        s.workflow_native_link(&c.id, local).unwrap();
        s.workflow_native_submitting(&c.id).unwrap();
        s.workflow_native_outcome(&c.id, "accepted", Some("accepted-native"))
            .unwrap();
        let id = id.to_owned();
        drop(s);
        let s = Store::open(f.path(), &[100], &super::super::tests::defaults()).unwrap();
        let view = status(&s, &e, &id);
        let rows = view["native_returns"]["retained"].as_array().unwrap();
        assert_eq!(
            rows.iter().find(|r| r["id"] == a.id).unwrap()["phase"],
            "retained_unsent"
        );
        assert_eq!(
            rows.iter().find(|r| r["id"] == b.id).unwrap()["phase"],
            "uncertain"
        );
        assert_eq!(
            rows.iter().find(|r| r["id"] == c.id).unwrap()["phase"],
            "accepted"
        );
        assert!(s.workflow_native_reserve(100, e.key).unwrap().is_empty());
        assert!(s.workflow_native_validate(&b.id).is_err());
    }
    #[test]
    fn detached_ack_reconciles_only_a_distinct_exact_collection_and_late_final() {
        let (_f, s, e, plan) = setup();
        let v = declare(&s, &e, plan);
        let first = advance(&s, &e, &v).unwrap();
        let worker = accept(&s, &first);
        answer(
            &s,
            &first,
            worker,
            "source",
            vec![first.id.clone()],
            "Selected checkpoint",
        );
        let dispatch = s.workflow_native_reserve(100, e.key).unwrap().remove(0);
        let old = turn(&s, &e, "Actual human execution");
        s.native_collection_started(old, "old-physical").unwrap();
        s.workflow_native_link(&dispatch.id, old).unwrap();
        s.workflow_native_submitting(&dispatch.id).unwrap();
        s.workflow_native_outcome(&dispatch.id, "accepted_uncollected", Some("new-physical"))
            .unwrap();
        s.workflow_native_outcome(
            &dispatch.id,
            "accepted_uncollected",
            Some("another-physical"),
        )
        .unwrap();
        let selected = s.native_collection_next().unwrap().unwrap();
        assert_ne!(selected.local, old);
        assert_eq!(selected.anchor, "new-physical");
        let observation = s
            .workflow_native_collection(selected.local)
            .unwrap()
            .unwrap();
        assert!(observation.live);
        assert_eq!(observation.physical, "new-physical");
        assert!(!s.workflow_native_visible(selected.local).unwrap());
        let active = json!({"id":"new-physical","status":"inProgress","items":[]});
        s.native_collection_observed(&selected, &[active], false)
            .unwrap();
        assert!(
            s.native_collection_observed(
                &selected,
                &[json!({"id":"old-physical","status":"completed","items":[]})],
                false
            )
            .is_err()
        );
        assert!(
            s.native_collection_observed(
                &selected,
                &[json!({"id":"new-physical","status":"completed","items":[]})],
                false
            )
            .is_err()
        );
        let final_turn = json!({"id":"new-physical","status":"completed","items":[{"id":"actual-native-item","type":"agentMessage","phase":"final_answer","text":"Late selected workflow report"}]});
        s.native_collection_observed(&selected, &[final_turn], false)
            .unwrap();
        let view = status(&s, &e, v["id"].as_str().unwrap());
        let receipt = &view["native_returns"]["retained"][0];
        assert_eq!(receipt["phase"], "accepted_reconciled");
        assert_eq!(receipt["native_turn"], "new-physical");
        assert_eq!(receipt["local_turn"], old);
        assert_eq!(receipt["collection_local_turn"], selected.local);
        let conn = s.conn.lock().unwrap();
        let old_state:(String,String)=conn.query_row("SELECT t.status,n.anchor FROM turns t JOIN native_collections n ON n.local_turn=t.id WHERE t.id=?1",[old],|r|Ok((r.get(0)?,r.get(1)?))).unwrap();
        assert_eq!(old_state, ("running".into(), "old-physical".into()));
        drop(conn);
        let page = s.interaction_presentation_list(100, e.key, 0, 0).unwrap();
        assert!(
            page["items"]
                .as_array()
                .unwrap()
                .iter()
                .any(|item| item["preview"] == "Late selected workflow report"
                    && item["local_turn"] == selected.local
                    && item["current"] == true)
        );
        assert_ne!(
            view["phase"], "completed",
            "A collected helper report cannot finish integration"
        );
    }
    #[test]
    fn detached_observation_retires_at_original_scope_without_erasing_ack() {
        let (_f, s, e, plan) = setup();
        let v = declare(&s, &e, plan);
        let first = advance(&s, &e, &v).unwrap();
        let worker = accept(&s, &first);
        answer(
            &s,
            &first,
            worker,
            "source",
            vec![first.id.clone()],
            "Selected checkpoint",
        );
        let dispatch = s.workflow_native_reserve(100, e.key).unwrap().remove(0);
        let old = turn(&s, &e, "Actual human execution");
        s.workflow_native_link(&dispatch.id, old).unwrap();
        s.workflow_native_submitting(&dispatch.id).unwrap();
        s.workflow_native_outcome(&dispatch.id, "accepted_uncollected", Some("new-physical"))
            .unwrap();
        let selected = s.native_collection_next().unwrap().unwrap();
        s.interaction_withdraw_session_requests(e.key).unwrap();
        assert!(
            !s.workflow_native_collection(selected.local)
                .unwrap()
                .unwrap()
                .live
        );
        assert!(s.native_collection_next().unwrap().is_none());
        let view = status(&s, &e, v["id"].as_str().unwrap());
        assert_eq!(
            view["native_returns"]["retained"][0]["phase"],
            "accepted_uncollected"
        );
        assert_eq!(
            view["native_returns"]["retained"][0]["native_turn"],
            "new-physical"
        );
    }
    #[test]
    fn detached_siblings_share_exact_collection_and_wait_for_unknown_target() {
        let (_f, s, e, plan) = setup();
        let v = declare(&s, &e, plan.clone());
        let first = advance(&s, &e, &v).unwrap();
        let worker = accept(&s, &first);
        answer(
            &s,
            &first,
            worker,
            "first",
            vec![first.id.clone()],
            "First selected checkpoint",
        );
        let a = s.workflow_native_reserve(100, e.key).unwrap().remove(0);
        let mut next = plan;
        next.request_id = "second-workflow".into();
        next.input_ref = original(&s, &e, "Another finite selected human workflow");
        let second_view = declare(&s, &e, next);
        let second = advance(&s, &e, &second_view).unwrap();
        let second_worker = accept(&s, &second);
        answer(
            &s,
            &second,
            second_worker,
            "second",
            vec![second.id.clone()],
            "Second selected checkpoint",
        );
        let b = s.workflow_native_reserve(100, e.key).unwrap().remove(0);
        let old = turn(&s, &e, "Actual human owner");
        s.native_collection_started(old, "old-physical").unwrap();
        for id in [&a.id, &b.id] {
            s.workflow_native_link(id, old).unwrap();
            s.workflow_native_submitting(id).unwrap();
        }
        s.workflow_native_outcome(&a.id, "accepted_uncollected", Some("shared-physical"))
            .unwrap();
        let selected = s.native_collection_next().unwrap().unwrap();
        let final_turn = json!({"id":"shared-physical","status":"completed","items":[{"id":"shared-item","type":"agentMessage","phase":"final_answer","text":"Both current checkpoints"}]});
        assert!(
            s.native_collection_observed(&selected, std::slice::from_ref(&final_turn), false)
                .is_err(),
            "Unknown sibling target must not borrow the first source grant"
        );
        assert_eq!(
            status(&s, &e, v["id"].as_str().unwrap())["native_returns"]["retained"][0]["phase"],
            "accepted_uncollected"
        );
        s.workflow_native_outcome(&b.id, "accepted_uncollected", Some("shared-physical"))
            .unwrap();
        let second_receipt = status(&s, &e, second_view["id"].as_str().unwrap());
        assert_eq!(
            second_receipt["native_returns"]["retained"][0]["collection_local_turn"],
            selected.local
        );
        s.native_collection_observed(&selected, &[final_turn], false)
            .unwrap();
        for view in [&v, &second_view] {
            assert_eq!(
                status(&s, &e, view["id"].as_str().unwrap())["native_returns"]["retained"][0]["phase"],
                "accepted_reconciled"
            );
        }
        let page = s.interaction_presentation_list(100, e.key, 0, 0).unwrap();
        let report = page["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["preview"] == "Both current checkpoints")
            .unwrap();
        assert_eq!(report["workflow_returns"].as_array().unwrap().len(), 2);
        assert_eq!(report["local_turn"], selected.local);
        let conn = s.conn.lock().unwrap();
        let anchor: String = conn
            .query_row(
                "SELECT anchor FROM native_collections WHERE local_turn=?1",
                [old],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(anchor, "old-physical");
    }
    #[test]
    fn callback_publication_fences_every_accepted_source_in_its_frozen_set() {
        let (_f, s, e, plan) = setup();
        let v = declare(&s, &e, plan.clone());
        let first = advance(&s, &e, &v).unwrap();
        let worker = accept(&s, &first);
        answer(
            &s,
            &first,
            worker,
            "first",
            vec![first.id.clone()],
            "First selected checkpoint",
        );
        let a = s.workflow_native_reserve(100, e.key).unwrap().remove(0);
        let mut next = plan;
        next.request_id = "second-workflow".into();
        next.input_ref = original(&s, &e, "Another finite selected human workflow");
        let second_view = declare(&s, &e, next);
        let second = advance(&s, &e, &second_view).unwrap();
        let second_worker = accept(&s, &second);
        answer(
            &s,
            &second,
            second_worker,
            "second",
            vec![second.id.clone()],
            "Second selected checkpoint",
        );
        let b = s.workflow_native_reserve(100, e.key).unwrap().remove(0);
        let local = turn(&s, &e, "");
        s.workflow_native_link(&a.id, local).unwrap();
        s.workflow_native_submitting(&a.id).unwrap();
        s.workflow_native_outcome(&a.id, "accepted", Some("callback"))
            .unwrap();
        s.workflow_native_link(&b.id, local).unwrap();
        assert!(
            s.workflow_native_visible(local).unwrap(),
            "Known unsent sibling cannot poison accepted context"
        );
        s.workflow_native_submitting(&b.id).unwrap();
        assert!(
            !s.workflow_native_visible(local).unwrap(),
            "An unresolved possible input has no causal publication proof"
        );
        s.workflow_native_outcome(&b.id, "accepted", Some("callback"))
            .unwrap();
        assert!(s.workflow_native_visible(local).unwrap());
        s.record_native_message(
            local,
            "native-1",
            "callback",
            "mixed",
            Some("final_answer"),
            "Two current selected checkpoints",
        )
        .unwrap();
        let page = s.interaction_presentation_list(100, e.key, 0, 0).unwrap();
        let mixed = page["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["preview"] == "Two current selected checkpoints")
            .unwrap();
        assert_eq!(mixed["workflow_returns"].as_array().unwrap().len(), 2);
        answer(
            &s,
            &first,
            worker,
            "corrected",
            vec![first.id.clone()],
            "Corrected first source",
        );
        assert!(
            !s.workflow_native_visible(local).unwrap(),
            "Current second source cannot publish a stale first source"
        );
        s.record_native_message(
            local,
            "native-1",
            "callback",
            "stale-mixed",
            Some("final_answer"),
            "Stale mixed output cannot publish",
        )
        .unwrap();
        let page = s.interaction_presentation_list(100, e.key, 0, 0).unwrap();
        assert!(
            page["items"]
                .as_array()
                .unwrap()
                .iter()
                .any(
                    |i| i["preview"] == "Two current selected checkpoints" && i["current"] == false
                )
        );
        assert!(
            !page["items"]
                .as_array()
                .unwrap()
                .iter()
                .any(|i| i["preview"] == "Stale mixed output cannot publish")
        );
        let corrected = s.workflow_native_reserve(100, e.key).unwrap().remove(0);
        s.workflow_native_link(&corrected.id, local).unwrap();
        s.workflow_native_submitting(&corrected.id).unwrap();
        s.workflow_native_outcome(&corrected.id, "accepted", Some("callback"))
            .unwrap();
        assert!(
            !s.workflow_native_visible(local).unwrap(),
            "ACK does not declare integration of the corrected source"
        );
        let offer: Value = serde_json::from_str(&corrected.output).unwrap();
        s.interaction_workflow_reported(
            100,
            e.key,
            "native-1",
            v["id"].as_str().unwrap(),
            offer["evidence"]["revision"].as_str().unwrap(),
        )
        .unwrap();
        assert!(
            s.workflow_native_visible(local).unwrap(),
            "Explicit current same-slot incorporation supersedes the old source"
        );
        s.record_native_message(
            local,
            "native-1",
            "callback",
            "current-mixed",
            Some("final_answer"),
            "Current explicitly incorporated checkpoints",
        )
        .unwrap();
        let page = s.interaction_presentation_list(100, e.key, 0, 0).unwrap();
        let current = page["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|i| i["preview"] == "Current explicitly incorporated checkpoints")
            .unwrap();
        assert_eq!(current["workflow_returns"].as_array().unwrap().len(), 2);
        assert!(
            !current["workflow_returns"]
                .as_array()
                .unwrap()
                .contains(&json!(a.id))
        );
    }
    #[test]
    fn native_return_count_is_bounded_and_source_corrections_fence_submission() {
        let (_f, s, e, plan) = setup();
        let v = declare(&s, &e, plan);
        let id = v["id"].as_str().unwrap();
        let first = advance(&s, &e, &v).unwrap();
        let worker = accept(&s, &first);
        answer(
            &s,
            &first,
            worker,
            "initial",
            vec![first.id.clone()],
            "Checkpoint before correction",
        );
        let stale = s.workflow_native_reserve(100, e.key).unwrap().remove(0);
        answer(
            &s,
            &first,
            worker,
            "changed",
            vec![first.id.clone()],
            "Current corrected checkpoint",
        );
        assert!(s.workflow_native_validate(&stale.id).is_err());
        for index in 0..16 {
            answer(
                &s,
                &first,
                worker,
                &format!("revision-{index}"),
                vec![first.id.clone()],
                &format!("Named milestone revision{index}"),
            );
            let selected = s.workflow_native_reserve(100, e.key).unwrap();
            assert_eq!(selected.len(), usize::from(index < 15));
        }
        let view = status(&s, &e, id);
        assert_eq!(
            view["native_returns"]["retained"].as_array().unwrap().len(),
            16
        );
        // At the model-return limit, evidence remains visible and ordinary
        // preauthorized successor progression is still usable without inference.
        assert!(
            !s.interaction_workflow_attention(100, e.key).unwrap()["envelopes"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert!(advance(&s, &e, &view).is_some());
    }

    #[test]
    fn historical_rows_and_documents_are_bounded_without_erasing_identity_or_effects() {
        let (_f, s, e, plan) = setup();
        let v = declare(&s, &e, plan.clone());
        let id = v["id"].as_str().unwrap();
        {
            let mut conn = s.conn.lock().unwrap();
            let tx = conn.transaction().unwrap();
            let w = load_on(&tx, 100, e.key, id).unwrap();
            let mut oversized = w.clone();
            oversized.source.text = "x".repeat(MAX_WORKFLOW_DOCUMENT_BYTES);
            assert!(save_on(&tx, &oversized).is_err());
            assert_eq!(load_on(&tx, 100, e.key, id).unwrap().source, w.source);
            // Retained historical rows, not active work, exhaust the bound.
            for index in 1..MAX_RETAINED_WORKFLOWS {
                let mut old = w.clone();
                old.id = format!("historical-{index}");
                old.phase = "completed".into();
                tx.execute(
                    "INSERT INTO interaction_workflows VALUES(?1,100,100,1,?2,'completed',?3)",
                    params![
                        old.id,
                        format!("historical-source-{index}"),
                        document(&old).unwrap()
                    ],
                )
                .unwrap();
            }
            tx.commit().unwrap();
        }
        let later = original(&s, &e, "Another independently selected workflow");
        let mut next = plan.clone();
        next.input_ref = later;
        let error = s
            .interaction_workflow_declare(
                100,
                e.key,
                "native-1",
                "conversation",
                next,
                &CancellationToken::new(),
            )
            .unwrap_err();
        assert_eq!(error.to_string(), WORKFLOW_CAPACITY_ERROR);
        // Capacity does not disable an exact retained retry or current progress.
        assert_eq!(declare(&s, &e, plan), v);
        assert!(advance(&s, &e, &v).is_some());
        let conn = s.conn.lock().unwrap();
        assert_eq!(
            conn.query_row("SELECT count(*) FROM interaction_workflows", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            MAX_RETAINED_WORKFLOWS
        );
        assert!(
            conn.query_row(
                "SELECT sum(length(CAST(doc AS BLOB))) FROM interaction_workflows",
                [],
                |r| r.get::<_, usize>(0)
            )
            .unwrap()
                <= MAX_RETAINED_WORKFLOWS as usize * MAX_WORKFLOW_DOCUMENT_BYTES
        );
    }

    #[test]
    fn committed_result_advances_only_the_current_predeclared_successor_once() {
        let (_f, s, e, plan) = setup();
        let v = declare(&s, &e, plan);
        let id = v["id"].as_str().unwrap();
        let first = advance(&s, &e, &v).unwrap();
        let local = accept(&s, &first);
        let partial = answer(
            &s,
            &first,
            local,
            "partial",
            vec![],
            "Coverage is incomplete",
        );
        assert!(
            s.interaction_workflow_return(100, &partial.id)
                .unwrap()
                .is_none()
        );
        let d = answer(
            &s,
            &first,
            local,
            "complete",
            vec![first.id.clone()],
            "Canonical predecessor",
        );
        let second = s.interaction_workflow_return(100, &d.id).unwrap().unwrap();
        assert_eq!(second.target, SessionKey::new(100, Some(8)));
        let author = s.interaction_author(&second.id, 100).unwrap().unwrap();
        assert_eq!(
            author["predecessor"]["id"],
            status(&s, &e, id)["steps"][0]["result"]["id"]
        );
        assert!(s.interaction_workflow_return(100, &d.id).unwrap().is_none());
        let before = status(&s, &e, id);
        assert_eq!(before["steps"][1]["phase"], "queued");
        // A reserved successor cannot be recreated to repair a lost dispatch.
        assert!(s.interaction_workflow_return(100, &d.id).unwrap().is_none());
        assert_eq!(status(&s, &e, id), before);
        let local = accept(&s, &second);
        let done = answer(
            &s,
            &second,
            local,
            "verified",
            vec![second.id.clone()],
            "Receiving checkpoint",
        );
        assert!(
            s.interaction_workflow_return(100, &done.id)
                .unwrap()
                .is_none()
        );
        assert_eq!(status(&s, &e, id)["phase"], "awaiting_integration");
    }

    #[test]
    fn corrections_invalidate_consumed_predecessor_without_replaying_successor() {
        let (_f, s, e, plan) = setup();
        let v = declare(&s, &e, plan);
        let id = v["id"].as_str().unwrap();
        let first = advance(&s, &e, &v).unwrap();
        let local = accept(&s, &first);
        answer(&s, &first, local, "partial", vec![], "Uncovered result");
        assert_ne!(status(&s, &e, id)["steps"][1]["phase"], "ready");
        answer(
            &s,
            &first,
            local,
            "current",
            vec![first.id.clone()],
            "Covered checkpoint",
        );
        let ready = status(&s, &e, id);
        let second = advance(&s, &e, &ready).unwrap();
        let second_local = accept(&s, &second);
        let amendment = s
            .interaction_admit(
                &SourceInput {
                    transcription: None,
                    channel: "fixture-correction".into(),
                    event_id: "later-correction".into(),
                    update_id: None,
                    human_id: 100,
                    target_sessions: vec![first.target_session],
                    origin: ReturnRoute {
                        chat_id: 100,
                        thread_id: 1,
                        message_id: Some(12),
                    },
                    text: "Correct the checkpoint".into(),
                },
                first.target,
                RequestRelation::Amends(first.requests.clone()),
            )
            .unwrap()
            .0;
        s.interaction_bind_epoch(&amendment.id, 100, "initial")
            .unwrap();
        let amended = accept(&s, &amendment);
        let v = status(&s, &e, id);
        assert_eq!(v["steps"][0]["phase"], "correction_pending");
        assert_eq!(v["steps"][1]["phase"], "predecessor_changed");
        assert!(
            !s.interaction_session_input_allowed(&second.id, 100)
                .unwrap()
        );
        assert!(advance(&s, &e, &v).is_none());
        answer(
            &s,
            &amendment,
            amended,
            "amended",
            vec![first.id.clone(), amendment.id.clone()],
            "Corrected checkpoint",
        );
        let v = status(&s, &e, id);
        assert_eq!(v["steps"][0]["phase"], "completed");
        assert_eq!(v["steps"][1]["phase"], "predecessor_changed");
        answer(
            &s,
            &second,
            second_local,
            "old-consumption",
            vec![second.id.clone()],
            "Consumed earlier checkpoint",
        );
        assert_ne!(status(&s, &e, id)["phase"], "awaiting_integration");
        let a = s.interaction_workflow_attention(100, e.key).unwrap();
        assert!(
            a["envelopes"]
                .as_array()
                .unwrap()
                .iter()
                .any(|v| v["reason"] == "reconciliation_required")
        );
    }
    #[test]
    fn restart_atomic_admission_and_uncertain_native_input_are_never_replayed() {
        let (f, s, e, plan) = setup();
        let v = declare(&s, &e, plan);
        let id = v["id"].as_str().unwrap().to_owned();
        s.conn.lock().unwrap().execute_batch("CREATE TRIGGER synthetic_author_failure BEFORE INSERT ON interaction_session_authors BEGIN SELECT RAISE(ABORT,'synthetic author failure'); END;").unwrap();
        assert!(
            s.interaction_workflow_advance(
                100,
                e.key,
                "native-1",
                &id,
                v["revision"].as_str().unwrap(),
                &CancellationToken::new()
            )
            .is_err()
        );
        assert_eq!(status(&s, &e, &id), v);
        s.conn
            .lock()
            .unwrap()
            .execute_batch("DROP TRIGGER synthetic_author_failure;")
            .unwrap();
        let first = advance(&s, &e, &v).unwrap();
        let target = s.get_session(first.target).unwrap().unwrap();
        let local = turn(&s, &target, &first.text);
        s.interaction_transport(&first.id, 100, local, P::Submitting, None)
            .unwrap();
        drop(s);
        let s = Store::open(f.path(), &[100], &super::super::tests::defaults()).unwrap();
        let v = status(&s, &e, &id);
        assert_eq!(v["steps"][0]["phase"], "uncertain");
        assert!(advance(&s, &e, &v).is_none());
        let conn = s.conn.lock().unwrap();
        assert_eq!(
            conn.query_row("SELECT count(*) FROM interaction_instructions", [], |r| r
                .get::<_, i64>(
                0
            ))
            .unwrap(),
            1
        );
    }
    #[test]
    fn pending_question_offers_are_scoped_versioned_and_expire_without_deciding() {
        let (_f, s, e, plan) = setup();
        let v = declare(&s, &e, plan);
        let first = advance(&s, &e, &v).unwrap();
        let local = accept(&s, &first);
        let question = HumanInteraction {
            id: "a".repeat(32),
            kind: "native".into(),
            state: "pending".into(),
            requester: 100,
            native_thread: "native-7".into(),
            local_turn: local,
            title: "Selected missing decision".into(),
            action: "Answer".into(),
            blocking: true,
            source: json!({"kind":"question","conversationIndex":0,"request":{"questions":[{"id":"decision","question":"Which receiving target?","options":[]}]}}),
        };
        s.save_interaction(first.target, &question, Some("selected-question"))
            .unwrap();
        let mut unrelated = question.clone();
        unrelated.id = "b".repeat(32);
        unrelated.native_thread = "native-8".into();
        unrelated.local_turn = turn(
            &s,
            &s.get_session(SessionKey::new(100, Some(8)))
                .unwrap()
                .unwrap(),
            "Unrelated direct work",
        );
        s.save_interaction(
            SessionKey::new(100, Some(8)),
            &unrelated,
            Some("direct-question"),
        )
        .unwrap();
        let a = s.interaction_workflow_attention(100, e.key).unwrap();
        let events = a["envelopes"].as_array().unwrap();
        assert_eq!(events.len(), 1);
        let event = &events[0];
        assert_eq!(event["source"]["id"], question.id);
        assert_eq!(event["reason"], "human_decision_required");
        let revision = event["source"]["revision"].as_str().unwrap();
        let native_return = s.workflow_native_reserve(100, e.key).unwrap().remove(0);
        let part = s
            .interaction_workflow_question(100, e.key, "native-1", &question.id, revision, 0)
            .unwrap();
        assert!(
            part["text"]
                .as_str()
                .unwrap()
                .contains("Which receiving target?")
        );
        assert_eq!(part["answer_allowed"], false);
        s.transition_interaction(first.target, 100, &question.id, "pending", "expired")
            .unwrap();
        assert!(
            s.interaction_workflow_attention(100, e.key).unwrap()["envelopes"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert!(
            s.interaction_workflow_question(100, e.key, "native-1", &question.id, revision, 0)
                .is_err()
        );
        assert!(s.workflow_native_validate(&native_return.id).is_err());
        assert!(
            s.interaction_workflow_reported(
                100,
                e.key,
                "native-1",
                v["id"].as_str().unwrap(),
                event["revision"].as_str().unwrap()
            )
            .is_err()
        );
    }
    #[test]
    fn bounded_grants_supersession_withdrawal_and_binding_retirement_preserve_active_effects() {
        let (_f, s, e, plan) = setup();
        let mut oversized = plan.clone();
        oversized.steps.resize(5, plan.steps[0].clone());
        assert!(
            s.interaction_workflow_declare(
                100,
                e.key,
                "native-1",
                "conversation",
                oversized,
                &CancellationToken::new()
            )
            .is_err()
        );
        let mut expired = plan.clone();
        expired.expires_at = (Utc::now() - Duration::seconds(1)).to_rfc3339();
        assert!(
            s.interaction_workflow_declare(
                100,
                e.key,
                "native-1",
                "conversation",
                expired,
                &CancellationToken::new()
            )
            .is_err()
        );
        let mut excessive = plan.clone();
        excessive.expires_at = (Utc::now() + Duration::days(8)).to_rfc3339();
        assert!(
            s.interaction_workflow_declare(
                100,
                e.key,
                "native-1",
                "conversation",
                excessive,
                &CancellationToken::new()
            )
            .is_err()
        );
        let v = declare(&s, &e, plan);
        let id = v["id"].as_str().unwrap();
        let first = advance(&s, &e, &v).unwrap();
        let local = accept(&s, &first);
        let correction = original(
            &s,
            &e,
            "Supersede this plan; preserve the active checkpoint",
        );
        let current = status(&s, &e, id);
        let superseded = s
            .interaction_workflow_supersede(
                100,
                e.key,
                "native-1",
                "conversation",
                id,
                current["revision"].as_str().unwrap(),
                correction.clone(),
            )
            .unwrap();
        assert_eq!(superseded["phase"], "superseded");
        assert_eq!(superseded["steps"][0]["phase"], "accepted");
        assert_eq!(superseded["steps"][1]["phase"], "withdrawn");
        assert!(!s.interaction_session_input_allowed(&first.id, 100).unwrap());
        answer(
            &s,
            &first,
            local,
            "retained-after-supersession",
            vec![first.id.clone()],
            "Completed effect retained",
        );
        assert_eq!(
            s.interaction_instruction(&first.id, 100)
                .unwrap()
                .unwrap()
                .phase,
            P::Accepted
        );
        assert_eq!(
            s.interaction_workflow_supersede(
                100,
                e.key,
                "native-1",
                "conversation",
                id,
                current["revision"].as_str().unwrap(),
                correction
            )
            .unwrap()["phase"],
            "superseded"
        );
        let (_f, s, e, plan) = setup();
        let v = declare(&s, &e, plan);
        let id = v["id"].as_str().unwrap();
        s.set_session_codex_thread(e.key, "replacement").unwrap();
        s.set_session_codex_thread(e.key, "native-1").unwrap();
        assert_eq!(status(&s, &e, id)["phase"], "retired");
        assert!(
            s.interaction_workflow_advance(
                100,
                e.key,
                "native-1",
                id,
                v["revision"].as_str().unwrap(),
                &CancellationToken::new()
            )
            .is_err()
        );
        let (_f, s, e, plan) = setup();
        let v = declare(&s, &e, plan);
        let first = advance(&s, &e, &v).unwrap();
        s.interaction_withdraw_session_requests(e.key).unwrap();
        assert_eq!(
            status(&s, &e, v["id"].as_str().unwrap())["phase"],
            "withdrawn"
        );
        assert!(!s.interaction_session_input_allowed(&first.id, 100).unwrap());
    }
}
