//! Attributed goal-control intents. Retention is not native execution or approval.
use super::*;
use crate::{codex::NativeGoalSnapshot, interaction::InputReference};
use anyhow::bail;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GoalAction {
    Start,
    Edit,
    Pause,
    Resume,
    Clear,
    Replace,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GoalPlan {
    pub action: GoalAction,
    pub expected: NativeGoalSnapshot,
    pub objective: Option<String>,
    pub token_budget: Option<i64>,
    /// Exact selected clause from the retained human input, not rewritten speech.
    pub followup: Option<String>,
}
impl GoalPlan {
    pub(crate) fn delegatable(&self) -> bool {
        matches!(
            self.action,
            GoalAction::Edit | GoalAction::Pause | GoalAction::Resume
        ) && self.token_budget.is_none()
            && self.followup.is_none()
    }
    pub fn validate(&self) -> Result<()> {
        use GoalAction::*;
        if self.expected.revision.is_empty()
            || self.expected.revision.len() > 512
            || !self
                .expected
                .revision
                .bytes()
                .all(|v| (33..=126).contains(&v))
            || self.token_budget.is_some_and(|v| v <= 0)
            || self
                .objective
                .as_ref()
                .is_some_and(|v| v.trim().is_empty() || v.len() > 12000)
            || self
                .followup
                .as_ref()
                .is_some_and(|v| v.trim().is_empty() || v.len() > 12000)
        {
            bail!("Invalid bounded goal-control plan");
        }
        match self.action {
            Start if self.expected.goal.is_some() || self.objective.is_none() => {
                bail!("Start requires an absent goal and objective")
            }
            Replace if self.expected.goal.is_none() || self.objective.is_none() => {
                bail!("Replace requires an existing goal and new objective")
            }
            Edit if self.expected.goal.is_none()
                || (self.objective.is_none() && self.token_budget.is_none()) =>
            {
                bail!("Edit requires a goal and changed objective or budget")
            }
            Pause | Resume | Clear
                if self.expected.goal.is_none()
                    || self.objective.is_some()
                    || self.token_budget.is_some() =>
            {
                bail!("Status/clear controls require an existing goal and no replacement payload")
            }
            _ => (),
        }
        if matches!(self.action, Resume) {
            self.expected
                .goal
                .as_ref()
                .expect("validated goal")
                .ensure_resumable()?;
        }
        Ok(())
    }
}

pub(super) fn unclaimed_on(conn: &Connection, human: i64, kind: &str, event: &str) -> Result<()> {
    let held:bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM interaction_goal_inputs WHERE human_id=?1 AND source_kind=?2 AND source_event=?3)",params![human,kind,event],|r|r.get(0))?;
    if held {
        bail!("Original input has a retained goal-control selection");
    }
    Ok(())
}

fn binding_on(conn: &Connection, human: i64, key: SessionKey) -> Result<Value> {
    let (session, native) = super::presentation::entry_on(conn, human, key)?;
    Ok(json!([
        super::presentation::entry_binding(conn, human, key, session)?,
        session,
        native,
        super::stopping::admission_epoch_on(conn, key)?
    ]))
}
fn read_on(conn: &Connection, human: i64, entry: SessionKey, id: &str) -> Result<Value> {
    super::presentation::entry_on(conn, human, entry)?;
    let (raw,phase,result):(String,String,Option<String>)=conn.query_row(
        "SELECT doc,phase,result FROM interaction_goal_controls WHERE id=?1 AND human_id=?2 AND entry_chat=?3 AND entry_topic=?4",
        params![id,human,entry.chat_id,entry.thread_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).context("Selected goal control is outside this owned entry")?;
    Ok(
        json!({"id":id,"phase":phase,"retryable_outcome":matches!(phase.as_str(),"known_unsent"|"retained_unsent"),"intent":serde_json::from_str::<Value>(&raw)?,
        "result":result.map(|s|serde_json::from_str::<Value>(&s)).transpose()?}),
    )
}
fn control_view_on(conn: &Connection, human: i64, entry: SessionKey, id: &str) -> Result<Value> {
    let value = read_on(conn, human, entry, id)?;
    let doc = &value["intent"];
    let (session, native) = super::presentation::entry_on(conn, human, entry)?;
    let current = eligible_on(conn, human, entry, &value).is_ok();
    let target: SessionKey = serde_json::from_value(doc["target"].clone())?;
    let label:Option<String>=conn.query_row("SELECT session_title FROM sessions WHERE chat_id=?1 AND thread_id=?2 AND creator_user_id=?3",params![target.chat_id,target.thread_id,human],|r|r.get(0)).optional()?.flatten();
    Ok(json!({"schema":"telecodex.goal-control-view/v1","id":id,
        "binding":super::presentation::entry_binding(conn,human,entry,session)?,"entry_binding":[human,entry,session,native],
        "phase":value["phase"],"current":current,"can_confirm":current && value["phase"]=="awaiting_confirmation",
        "label":label.unwrap_or_else(||format!("Topic {}",target.thread_id)),"action":doc["plan"]["action"],
        "proposal":doc["proposal"],"plan":doc["plan"],"reason":doc["reason"],"result":value["result"]}))
}

fn command_generation_on(conn: &Connection, target: SessionKey) -> Result<Option<String>> {
    conn.query_row(
        "SELECT value FROM bot_state WHERE key=?",
        [format!(
            "goal_intent:{}:{}",
            target.chat_id, target.thread_id
        )],
        |r| r.get(0),
    )
    .optional()
    .map_err(Into::into)
}
fn eligible_on(conn: &Connection, human: i64, entry: SessionKey, value: &Value) -> Result<()> {
    let d = &value["intent"];
    let target: SessionKey = serde_json::from_value(d["target"].clone())?;
    if d["delegation"].is_object() {
        super::goal_delegations::validate_on(conn, human, target, &d["delegation"])?;
    }
    let generation = serde_json::to_value(command_generation_on(conn, target)?)?;
    let expected_generation = if d["ever_claimed"] == true
        || matches!(value["phase"].as_str(), Some("claimed" | "submitted"))
    {
        &value["id"]
    } else {
        &d["goal_command_generation"]
    };
    if &generation != expected_generation {
        bail!("A later explicit goal command replaced this selected control generation");
    }
    if binding_on(conn, human, entry)? != d["entry_binding"]
        || binding_on(conn, human, target)? != d["target_binding"]
        || d["expires_at"]
            .as_i64()
            .is_none_or(|t| t <= Utc::now().timestamp())
        || super::stopping::fenced_on(conn, entry)?
        || super::stopping::fenced_on(conn, target)?
        || super::maintenance_writer_held_on(conn, entry)?
        || super::maintenance_writer_held_on(conn, target)?
    {
        bail!("Goal-control binding, permission, deadline or writer availability changed");
    }
    Ok(())
}
fn source_on(
    conn: &Connection,
    actor: &LiveTaskActor<'_>,
    reference: &InputReference,
) -> Result<Value> {
    let (source, message) = super::dialogue::original_on(
        conn,
        actor.key,
        actor.human,
        actor.native,
        reference,
        "",
        false,
    )?;
    if message
        .as_ref()
        .is_some_and(|m| m.from.as_ref().is_none_or(|u| u.is_bot))
    {
        bail!("Actual human input required");
    }
    if source.text.trim().is_empty() {
        bail!("Original input is empty");
    }
    let order = super::dialogue::original_order_on(conn, &source)?
        .context("Original capture chronology unavailable")?;
    let routed:bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM interaction_sources WHERE human_id=?1 AND ((update_id=?2 AND ?2 IS NOT NULL) OR (channel='conversation' AND event_id=?3 AND ?2 IS NULL))) OR EXISTS(SELECT 1 FROM interaction_creations WHERE human_id=?1 AND input_update=?4) OR EXISTS(SELECT 1 FROM conversation_decisions WHERE human_id=?1 AND json_extract(doc,'$.source_input')=?3)",params![actor.human,reference.update(),reference.event(),reference],|r|r.get(0))?;
    if routed {
        bail!("Original input already has a selected route, creation or answer");
    }
    Ok(json!({"input_ref":reference,"capture_order":order,"source":source}))
}
fn claim_source_on(conn: &Connection, human: i64, source: &Value, id: &str) -> Result<()> {
    let reference: InputReference = serde_json::from_value(source["input_ref"].clone())?;
    conn.execute("INSERT INTO interaction_goal_inputs(human_id,source_kind,source_event,control_id) VALUES(?1,?2,?3,?4)",
        params![human,if reference.update().is_some(){"telegram"}else{"conversation"},reference.event(),id])?;
    Ok(())
}

fn validate_claimed_on(
    conn: &Connection,
    instance: &str,
    human: i64,
    entry: SessionKey,
    id: &str,
    attempt: &str,
) -> Result<()> {
    let value = read_on(conn, human, entry, id)?;
    if value["intent"]["attempt"] != attempt {
        bail!("Goal submission attempt was replaced");
    }
    let owned:bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM interaction_goal_controls WHERE id=?1 AND phase IN ('claimed','submitted') AND instance_id=?2)",params![id,instance],|r|r.get(0))?;
    if !owned {
        bail!("Goal attempt is no longer owned by this submission");
    }
    let d = &value["intent"];
    let newer:bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM interaction_goal_controls WHERE human_id=?1 AND target_chat=?2 AND target_topic=?3 AND id!=?4 AND json_extract(doc,'$.authorization_order')>?5)",params![human,d["target"]["chat_id"].as_i64(),d["target"]["thread_id"].as_i64(),id,d["authorization_order"].as_i64()],|r|r.get(0))?;
    if newer {
        bail!("Later captured human goal direction superseded this submission");
    }
    eligible_on(conn, human, entry, &value)
}

impl Store {
    pub(super) fn init_goal_controls(&self) -> Result<()> {
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        conn.execute_batch("CREATE TABLE IF NOT EXISTS interaction_goal_controls(id TEXT PRIMARY KEY,human_id INTEGER NOT NULL,entry_chat INTEGER NOT NULL,entry_topic INTEGER NOT NULL,target_chat INTEGER NOT NULL,target_topic INTEGER NOT NULL,phase TEXT NOT NULL,doc TEXT NOT NULL,result TEXT,instance_id TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS interaction_goal_inputs(human_id INTEGER NOT NULL,source_kind TEXT NOT NULL,source_event TEXT NOT NULL,control_id TEXT NOT NULL REFERENCES interaction_goal_controls(id),PRIMARY KEY(human_id,source_kind,source_event));")?;
        let tx = conn.transaction()?;
        let interrupted = {
            let mut query=tx.prepare("SELECT id,CASE WHEN phase='submitted' THEN 'uncertain' ELSE 'retained_unsent' END FROM interaction_goal_controls WHERE instance_id!=?1 AND phase IN ('claimed','submitted') AND json_extract(doc,'$.attempt') IS NOT NULL")?;
            query
                .query_map([&self.instance_id], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        tx.execute("UPDATE interaction_goal_controls SET phase=CASE WHEN phase='submitted' THEN 'uncertain' ELSE 'retained_unsent' END WHERE instance_id!=?1 AND phase IN ('prepared','claimed','submitted')",[&self.instance_id])?;
        for (id, phase) in interrupted {
            super::topic_publications::goal_control_on(&tx, &id, &phase)?;
        }
        tx.commit()?;
        Ok(())
    }
    pub fn goal_control_read(&self, human: i64, entry: SessionKey, id: &str) -> Result<Value> {
        read_on(
            &self.conn.lock().expect("store mutex poisoned"),
            human,
            entry,
            id,
        )
    }
    /// Human-readable views of this entry's retained controls, without native I/O
    /// or new decision authority. Actual execution still checks native revision.
    pub fn goal_control_view(&self, human: i64, entry: SessionKey, id: &str) -> Result<Value> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        control_view_on(&conn, human, entry, id)
    }
    pub fn goal_control_views(&self, human: i64, entry: SessionKey, before: i64) -> Result<Value> {
        if before < 0 {
            bail!("Invalid goal-control cursor");
        }
        let conn = self.conn.lock().expect("store mutex poisoned");
        let (session, native) = super::presentation::entry_on(&conn, human, entry)?;
        let mut query=conn.prepare("SELECT rowid,id FROM interaction_goal_controls WHERE human_id=?1 AND entry_chat=?2 AND entry_topic=?3 AND phase NOT IN ('acknowledged','cancelled','superseded','rejected') AND (?4=0 OR rowid<?4) ORDER BY rowid DESC LIMIT 33")?;
        let rows = query
            .query_map(
                params![human, entry.chat_id, entry.thread_id, before],
                |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)),
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let items = rows
            .iter()
            .take(32)
            .map(|(_, id)| {
                let mut view = control_view_on(&conn, human, entry, id)?;
                // Full objective/reason/follow-up are fetched only when reviewed.
                view.as_object_mut().expect("control view").remove("plan");
                view.as_object_mut().expect("control view").remove("reason");
                view.as_object_mut().expect("control view").remove("result");
                Ok(view)
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(
            json!({"schema":"telecodex.goal-control-list/v1","binding":super::presentation::entry_binding(&conn,human,entry,session)?,
            "entry_binding":[human,entry,session,native],"items":items,"more":rows.len()>32,"cursor":rows.iter().take(32).next_back().map(|r|r.0).unwrap_or(before)}),
        )
    }
    /// A missing original is an assistant proposal, never an implicit human command.
    #[allow(clippy::too_many_arguments)]
    pub fn goal_control_prepare(
        &self,
        actor: &LiveTaskActor<'_>,
        target: SessionKey,
        target_binding: &Value,
        reference: Option<&InputReference>,
        plan: &GoalPlan,
        reason: &str,
        closed: &CancellationToken,
    ) -> Result<(Value, bool)> {
        plan.validate()?;
        if reason.trim().is_empty() || reason.len() > 2000 {
            bail!("Bounded goal-control reason required");
        }
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        super::live_tasks::actor_on(&tx, actor, closed)?;
        let entry_binding = binding_on(&tx, actor.human, actor.key)?;
        let actual_binding = binding_on(&tx, actor.human, target)?;
        let delegation = if reference.is_none() && plan.delegatable() {
            super::goal_delegations::grant_on(&tx, actor.human, target, false)
                .ok()
                .map(|mut grant| {
                    grant["principal"] = json!("daimon");
                    grant
                })
        } else {
            None
        };
        if !actor.integrator && (delegation.is_none() || reference.is_some()) {
            bail!(
                "This execution requires an effective goal delegation for the selected operation"
            );
        }
        // Match the inspected public binding format, including incarnation.
        if json!({"binding":actual_binding[0],"entry_binding":[actor.human,target,actual_binding[1],actual_binding[2]]})
            != *target_binding
        {
            bail!("Selected target binding changed after inspection");
        }
        let native = actual_binding[2]
            .as_str()
            .filter(|s| !s.is_empty())
            .context("A selected existing native context is required")?;
        if plan
            .expected
            .goal
            .as_ref()
            .is_some_and(|g| g.thread_id != native)
        {
            bail!("Goal snapshot belongs to another target");
        }
        let identity = if let Some(reference) = reference {
            json!([actor.human, actor.key, reference])
        } else {
            json!([
                actor.human,
                actor.key,
                actor.local,
                actor.physical,
                target,
                plan,
                reason
            ])
        };
        let id = Uuid::new_v5(
            &Uuid::NAMESPACE_URL,
            format!("goal-control:{identity}").as_bytes(),
        )
        .to_string();
        let existing: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM interaction_goal_controls WHERE id=?1)",
            [&id],
            |r| r.get(0),
        )?;
        if existing {
            let value = read_on(&tx, actor.human, actor.key, &id)?;
            let d = &value["intent"];
            if d["plan"] != serde_json::to_value(plan)?
                || d["target"] != serde_json::to_value(target)?
                || d["reason"] != reason
                || d["entry_binding"] != entry_binding
                || d["target_binding"] != actual_binding
            {
                bail!("Goal intent is immutable");
            }
            return Ok((value, false));
        }
        let source = reference.map(|r| source_on(&tx, actor, r)).transpose()?;
        if let (Some(followup), Some(source)) = (&plan.followup, &source) {
            if !source["source"]["text"]
                .as_str()
                .is_some_and(|s| s.contains(followup))
            {
                bail!("Follow-up must be an exact selected clause of this human input");
            }
        }
        let barrier: i64 = tx.query_row(
            "SELECT COALESCE(MAX(id),0) FROM original_ingress_order",
            [],
            |r| r.get(0),
        )?;
        let phase = if source.is_some() || delegation.is_some() {
            "prepared"
        } else {
            "awaiting_confirmation"
        };
        let doc = json!({"entry":actor.key,"entry_binding":entry_binding,"target":target,"target_binding":actual_binding,
            "goal_command_generation":command_generation_on(&tx,target)?,"plan":plan,"reason":reason,"original":source,"proposal":source.is_none() && delegation.is_none(),"confirmation":null,
            "proposal_capture_barrier":barrier,"authorization_order":source.as_ref().map(|s|s["capture_order"].clone()).or_else(||delegation.as_ref().map(|_|json!(barrier))),"expires_at":Utc::now().timestamp()+900,
            "delegation":delegation,
            "actor":{"kind":if delegation.is_some(){"daimon"}else{"human_instruction_interpreter"},"native":actor.native,"physical":actor.physical,"local":actor.local},"created_at":now_string()});
        let value = json!({"id":id,"phase":phase,"intent":doc});
        eligible_on(&tx, actor.human, actor.key, &value)?;
        tx.execute("INSERT INTO interaction_goal_controls(id,human_id,entry_chat,entry_topic,target_chat,target_topic,phase,doc,instance_id) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",params![id,actor.human,actor.key.chat_id,actor.key.thread_id,target.chat_id,target.thread_id,phase,doc.to_string(),self.instance_id])?;
        if let Some(source) = source {
            claim_source_on(&tx, actor.human, &source, &id)?;
        }
        if closed.is_cancelled() {
            bail!("Goal intent controller expired before retention");
        }
        let value = read_on(&tx, actor.human, actor.key, &id)?;
        tx.commit()?;
        Ok((value, true))
    }
    /// Later captured human speech confirms or cancels exactly this proposal once.
    pub fn goal_control_decide(
        &self,
        actor: &LiveTaskActor<'_>,
        id: &str,
        reference: &InputReference,
        execute: bool,
        closed: &CancellationToken,
    ) -> Result<(Value, bool)> {
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        super::live_tasks::actor_on(&tx, actor, closed)?;
        let value = read_on(&tx, actor.human, actor.key, id)?;
        let mut d = value["intent"].clone();
        if d["confirmation"]["input_ref"] == serde_json::to_value(reference)?
            && d["confirmation"]["execute"] == execute
        {
            return Ok((value, false));
        }
        if value["phase"] != "awaiting_confirmation" || d["proposal"] != true {
            bail!("Proposal is no longer awaiting a decision");
        }
        eligible_on(&tx, actor.human, actor.key, &value)?;
        let source = source_on(&tx, actor, reference)?;
        if source["capture_order"].as_i64() <= d["proposal_capture_barrier"].as_i64() {
            bail!("Confirmation was captured before this proposal existed");
        }
        if execute
            && let Some(followup) = d["plan"]["followup"].as_str()
            && !source["source"]["text"]
                .as_str()
                .is_some_and(|s| s.contains(followup))
        {
            bail!("Proposed follow-up must be expressly present in the confirming human input");
        }
        if execute {
            d["authorization_order"] = source["capture_order"].clone();
        }
        d["confirmation"] =
            json!({"input_ref":reference,"execute":execute,"original":source,"at":now_string()});
        claim_source_on(&tx, actor.human, &source, id)?;
        tx.execute(
            "UPDATE interaction_goal_controls SET phase=?2,doc=?3,instance_id=?4 WHERE id=?1",
            params![
                id,
                if execute { "prepared" } else { "cancelled" },
                d.to_string(),
                self.instance_id
            ],
        )?;
        if closed.is_cancelled() {
            bail!("Confirmation controller expired before retention");
        }
        let value = read_on(&tx, actor.human, actor.key, id)?;
        tx.commit()?;
        Ok((value, true))
    }
    /// One active attempt. A proven unsent outcome can retry the same frozen intent;
    /// crash/lost acknowledgment remains uncertain and is never replayable.
    pub fn goal_control_claim(
        &self,
        actor: &LiveTaskActor<'_>,
        id: &str,
        closed: &CancellationToken,
    ) -> Result<(Value, bool)> {
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        super::live_tasks::actor_on(&tx, actor, closed)?;
        let value = read_on(&tx, actor.human, actor.key, id)?;
        if !matches!(
            value["phase"].as_str(),
            Some("prepared" | "known_unsent" | "retained_unsent")
        ) {
            return Ok((value, false));
        }
        eligible_on(&tx, actor.human, actor.key, &value)?;
        let d = &value["intent"];
        let newer:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM interaction_goal_controls WHERE human_id=?1 AND target_chat=?2 AND target_topic=?3 AND id!=?4 AND json_extract(doc,'$.authorization_order')>?5)",params![actor.human,d["target"]["chat_id"].as_i64(),d["target"]["thread_id"].as_i64(),id,d["authorization_order"].as_i64()],|r|r.get(0))?;
        if newer {
            tx.execute(
                "UPDATE interaction_goal_controls SET phase='superseded' WHERE id=?1",
                [id],
            )?;
            let value = read_on(&tx, actor.human, actor.key, id)?;
            tx.commit()?;
            return Ok((value, false));
        }
        let conflict:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM interaction_goal_controls WHERE human_id=?1 AND target_chat=?2 AND target_topic=?3 AND phase IN ('submitted','uncertain','partial'))",params![actor.human,d["target"]["chat_id"].as_i64(),d["target"]["thread_id"].as_i64()],|r|r.get(0))?;
        if conflict {
            bail!(
                "Target has an unconfirmed goal-control outcome; inspect it before another mutation"
            );
        }
        let mut claimed = d.clone();
        if value["result"].is_object() {
            let previous = json!({"attempt":d["attempt"],"phase":value["phase"],"result":value["result"],"worker_execution":d["worker_execution"]});
            if !claimed["previous_outcomes"].is_array() {
                claimed["previous_outcomes"] = json!([]);
            }
            claimed["previous_outcomes"]
                .as_array_mut()
                .expect("array")
                .push(previous);
        }
        claimed["worker_execution"] = Value::Null;
        claimed["execution_actor"] =
            json!({"native":actor.native,"physical":actor.physical,"local":actor.local});
        claimed["ever_claimed"] = json!(true);
        claimed["attempt"] = json!(Uuid::now_v7().to_string());
        let target: SessionKey = serde_json::from_value(d["target"].clone())?;
        tx.execute("INSERT INTO bot_state(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![format!("goal_intent:{}:{}",target.chat_id,target.thread_id),id])?;
        tx.execute(
            "UPDATE interaction_goal_controls SET phase='claimed',instance_id=?2,doc=?3,result=NULL WHERE id=?1",
            params![id, self.instance_id,claimed.to_string()],
        )?;
        if closed.is_cancelled() {
            bail!("Goal controller expired before submission claim");
        }
        let value = read_on(&tx, actor.human, actor.key, id)?;
        tx.commit()?;
        Ok((value, true))
    }
    /// Revalidate the already claimed attempt immediately before the native write.
    pub fn goal_control_validate_submission(
        &self,
        actor: &LiveTaskActor<'_>,
        id: &str,
        attempt: &str,
        closed: &CancellationToken,
    ) -> Result<()> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        super::live_tasks::actor_on(&conn, actor, closed)?;
        drop(conn);
        self.goal_control_begin_native_submission(actor.human, actor.key, id, attempt)
    }
    /// A claimed human command survives its interpreting model turn. Ownership,
    /// generation, holds and newer human direction still constrain the one attempt.
    pub(crate) fn goal_control_validate_retained_submission(
        &self,
        human: i64,
        entry: SessionKey,
        id: &str,
        attempt: &str,
    ) -> Result<()> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        validate_claimed_on(&conn, &self.instance_id, human, entry, id, attempt)
    }
    /// Mark the write boundary atomically, separately from a queued/claimed job.
    pub(crate) fn goal_control_begin_native_submission(
        &self,
        human: i64,
        entry: SessionKey,
        id: &str,
        attempt: &str,
    ) -> Result<()> {
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        validate_claimed_on(&tx, &self.instance_id, human, entry, id, attempt)?;
        tx.execute("UPDATE interaction_goal_controls SET phase='submitted' WHERE id=?1 AND phase='claimed' AND instance_id=?2",params![id,self.instance_id])?;
        tx.commit()?;
        Ok(())
    }

    /// Link the actual selected collector, without making another human ingress.
    pub(crate) fn goal_control_link_execution(
        &self,
        human: i64,
        entry: SessionKey,
        id: &str,
        attempt: &str,
        local: i64,
        native: &str,
    ) -> Result<()> {
        self.goal_control_validate_retained_submission(human, entry, id, attempt)?;
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let value = read_on(&tx, human, entry, id)?;
        eligible_on(&tx, human, entry, &value)?;
        let mut doc = value["intent"].clone();
        let matched: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM turns WHERE id=?1 AND session_id=?2 AND from_user_id=?3)",
            params![local, doc["target_binding"][1].as_i64(), human],
            |r| r.get(0),
        )?;
        if !matched || doc["target_binding"][2] != native {
            bail!("Goal collector does not match the retained target");
        }
        let execution = json!({"local":local,"native":native});
        if !doc["worker_execution"].is_null() && doc["worker_execution"] != execution {
            bail!("Goal control already belongs to another collector");
        }
        doc["worker_execution"] = execution;
        let selected = if doc["confirmation"].is_null() {
            &doc["original"]
        } else {
            &doc["confirmation"]["original"]
        };
        if doc["delegation"].is_null() {
            let reference: InputReference = serde_json::from_value(selected["input_ref"].clone())?;
            let source = match reference {
                InputReference::Conversation(id) => id,
                InputReference::Telegram(update) => format!("telegram:{update}"),
            };
            super::conversation_inputs::select_return_on(&tx, human, native, &source)?;
        }
        // Delegated control is an application action. It preserves the latest
        // actual human-selected return, without inventing another ingress.
        let changed=tx.execute("UPDATE interaction_goal_controls SET doc=?2 WHERE id=?1 AND phase IN ('claimed','submitted') AND instance_id=?3",params![id,doc.to_string(),self.instance_id])?;
        if changed != 1 {
            bail!("Goal submission no longer owned");
        }
        tx.commit()?;
        Ok(())
    }
    pub(crate) fn goal_control_retire_execution(
        &self,
        human: i64,
        entry: SessionKey,
        id: &str,
        attempt: &str,
    ) -> Result<()> {
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction()?;
        let changed = tx.execute("UPDATE interaction_goal_controls SET phase=CASE WHEN phase='claimed' THEN 'known_unsent' ELSE 'uncertain' END,result=json_patch(COALESCE(result,'{}'),?5) WHERE id=?1 AND human_id=?2 AND entry_chat=?3 AND entry_topic=?4 AND phase IN ('claimed','submitted') AND instance_id=?6 AND json_extract(doc,'$.attempt')=?7",params![id,human,entry.chat_id,entry.thread_id,json!({"error":"Selected collector ended before retaining a goal-control receipt","automatic_retry":false}).to_string(),self.instance_id,attempt])?;
        if changed == 1 {
            let phase: String = tx.query_row(
                "SELECT phase FROM interaction_goal_controls WHERE id=?",
                [id],
                |r| r.get(0),
            )?;
            super::topic_publications::goal_control_on(&tx, id, &phase)?;
        }
        tx.commit()?;
        Ok(())
    }
    pub fn goal_control_finish(
        &self,
        human: i64,
        entry: SessionKey,
        id: &str,
        attempt: &str,
        phase: &str,
        result: &Value,
    ) -> Result<bool> {
        if !matches!(
            phase,
            "submitted" | "acknowledged" | "rejected" | "uncertain" | "known_unsent" | "partial"
        ) || result.to_string().len() > 32000
        {
            bail!("Invalid goal-control receipt");
        }
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction()?;
        let current_attempt:Option<String>=tx.query_row("SELECT json_extract(doc,'$.attempt') FROM interaction_goal_controls WHERE id=?1 AND human_id=?2 AND entry_chat=?3 AND entry_topic=?4",params![id,human,entry.chat_id,entry.thread_id],|r|r.get(0))?;
        if current_attempt.as_deref() != Some(attempt) {
            return Ok(false);
        }
        // Preserve receipts after a binding/grant changes in flight; do not restore permission or resubmit.
        let changed=tx.execute("UPDATE interaction_goal_controls SET phase=?5,result=?6 WHERE id=?1 AND human_id=?2 AND entry_chat=?3 AND entry_topic=?4 AND phase IN ('claimed','submitted') AND instance_id=?7",params![id,human,entry.chat_id,entry.thread_id,phase,result.to_string(),self.instance_id])?;
        if changed == 0 {
            let retained:(String,Option<String>)=tx.query_row("SELECT phase,result FROM interaction_goal_controls WHERE id=?1 AND human_id=?2 AND entry_chat=?3 AND entry_topic=?4",params![id,human,entry.chat_id,entry.thread_id],|r|Ok((r.get(0)?,r.get(1)?)))?;
            if retained != (phase.to_owned(), Some(result.to_string())) {
                bail!("Goal receipt is immutable or belongs to another instance");
            }
        }
        super::topic_publications::goal_control_on(&tx, id, phase)?;
        tx.commit()?;
        Ok(changed == 1)
    }
}
