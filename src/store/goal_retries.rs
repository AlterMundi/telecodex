//! Durable scheduling around the existing conditional goal-control writer.
//! A retry is new continuation of a retained goal, never replay of failed input.
use super::*;
use crate::codex::{NativeGoal, NativeGoalSnapshot};
use anyhow::bail;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GoalRecoveryPolicy {
    pub request_id: String,
    pub target: SessionKey,
    pub expected_revision: Option<String>,
    pub enabled: bool,
}

pub(crate) struct GoalRetry {
    pub id: String,
    pub human: i64,
    pub target: SessionKey,
    pub native: String,
    pub failed_turn: String,
    pub goal: NativeGoal,
    pub context: Value,
    pub phase: String,
    pub control: Option<String>,
}

pub(super) fn policy_on(conn: &Connection, human: i64, key: SessionKey) -> Result<Option<Value>> {
    let raw: Option<String> = conn
        .query_row(
            "SELECT value FROM bot_state WHERE key=?1",
            [format!(
                "goal_recovery_policy:{human}:{}:{}",
                key.chat_id, key.thread_id
            )],
            |r| r.get(0),
        )
        .optional()?;
    raw.map(|s| serde_json::from_str(&s).map_err(Into::into))
        .transpose()
}

/// The native error shape currently contains no Retry-After field. Use explicit
/// fallback timing, with stable jitter surviving duplicate events/restarts.
fn delay_seconds(id: &str, attempt: u32) -> i64 {
    let base = 30i64.saturating_mul(1i64 << attempt.min(6)).min(1800);
    let jitter = id
        .bytes()
        .fold(0u64, |n, b| n.wrapping_mul(31).wrapping_add(b as u64))
        % 21;
    base * (90 + jitter as i64) / 100
}

pub(super) fn enqueue_on(
    conn: &Connection,
    human: i64,
    key: SessionKey,
    session: i64,
    native: &str,
    turn: &str,
) -> Result<()> {
    let row:Option<(String,String,String)>=conn.query_row("SELECT blocked_goal,terminal_error,block_context FROM goal_failure_observations WHERE human_id=?1 AND session_id=?2 AND native_thread=?3 AND native_turn=?4 AND blocked_goal IS NOT NULL AND terminal_error IS NOT NULL AND block_context IS NOT NULL",params![human,session,native,turn],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
    let Some((goal, error, context)) = row else {
        return Ok(());
    };
    let error: Value = serde_json::from_str(&error)?;
    let mut context: Value = serde_json::from_str(&context)?;
    if error["codexErrorInfo"] != "serverOverloaded"
        || context["delegation"]["recovery"] != true
        || context["policy"]["enabled"] == false
    {
        return Ok(());
    }
    let blocked: NativeGoal = serde_json::from_str(&goal)?;
    let granted = context["delegation"]["created_at"]
        .as_str()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .context("Retained recovery delegation has no source time")?;
    if blocked.updated_at <= granted.timestamp() {
        // A delayed/reconciled observation of an older block must not turn a
        // later delegation into permission to revive preexisting paused work.
        // Native timestamps have second precision: equal-second order is unknown.
        return Ok(());
    }
    let id = Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!("goal-recovery:{human}:{session}:{native}:{turn}").as_bytes(),
    )
    .to_string();
    let previous:Option<(u32,Option<String>,String,String)>=conn.query_row("SELECT attempts,control_id,context,phase FROM goal_retries WHERE human_id=?1 AND session_id=?2 AND native_thread=?3 AND json_extract(goal,'$.createdAt')=json_extract(?4,'$.createdAt') AND json_extract(goal,'$.objective')=json_extract(?4,'$.objective') AND json_extract(context,'$.delegation.revision')=json_extract(?5,'$.delegation.revision') ORDER BY rowid DESC LIMIT 1",params![human,session,native,goal,context.to_string()],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?;
    let mut prior = 0;
    let mut chain = json!({"id":id,"first_observed_at":Utc::now().timestamp()});
    if let Some((attempts, Some(control), old, state)) = previous {
        if context["command_generation"] == control
            && matches!(state.as_str(), "acknowledged" | "failed_again")
        {
            let old: Value = serde_json::from_str(&old)?;
            prior = attempts;
            if old["chain"].is_object() {
                chain = old["chain"].clone();
            }
        }
    }
    context["chain"] = chain;
    let due = Utc::now().timestamp() + delay_seconds(&id, prior);
    conn.execute("INSERT OR IGNORE INTO goal_retries(id,human_id,session_id,target_chat,target_topic,native_thread,failed_turn,goal,context,phase,next_at,attempts,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,'waiting',?10,?12,?11)",params![id,human,session,key.chat_id,key.thread_id,native,turn,goal,context.to_string(),due,now_string(),prior])?;
    Ok(())
}

fn retained_on(conn: &Connection, id: &str) -> Result<GoalRetry> {
    let raw:(i64,i64,i64,String,String,String,String,String,Option<String>)=conn.query_row("SELECT human_id,target_chat,target_topic,native_thread,failed_turn,goal,context,phase,control_id FROM goal_retries WHERE id=?1",[id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?,r.get(7)?,r.get(8)?)))?;
    Ok(GoalRetry {
        id: id.into(),
        human: raw.0,
        target: SessionKey::new(raw.1, Some(raw.2)),
        native: raw.3,
        failed_turn: raw.4,
        goal: serde_json::from_str(&raw.5)?,
        context: serde_json::from_str(&raw.6)?,
        phase: raw.7,
        control: raw.8,
    })
}

fn authority_on(conn: &Connection, job: &GoalRetry) -> Result<()> {
    let binding = super::goal_controls::binding_on(conn, job.human, job.target)?;
    let grant = super::goal_delegations::grant_on(conn, job.human, job.target, true)?;
    if binding != job.context["binding"]
        || grant["revision"] != job.context["delegation"]["revision"]
        || serde_json::to_value(policy_on(conn, job.human, job.target)?)? != job.context["policy"]
        || super::stopping::fenced_on(conn, job.target)?
        || super::maintenance_writer_held_on(conn, job.target)?
    {
        bail!("Recovery binding, delegation, policy or writer fence changed");
    }
    let generation = super::goal_controls::command_generation_on(conn, job.target)?;
    if serde_json::to_value(&generation)? != job.context["command_generation"]
        && generation != job.control
    {
        bail!("Later goal direction superseded capacity recovery");
    }
    let barrier = job.context["original_barrier"]
        .as_i64()
        .context("Recovery capture barrier unavailable")?;
    let newer:bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM conversation_inputs c JOIN original_ingress_order o ON o.channel='conversation' AND o.event_id=c.id WHERE c.human_id=?1 AND c.native_thread=?2 AND o.id>?3) OR EXISTS(SELECT 1 FROM incoming_updates i JOIN original_ingress_order o ON o.channel='telegram' AND o.event_id=CAST(i.update_id AS TEXT) WHERE o.id>?3 AND json_extract(i.payload_json,'$.message.from.id')=?1 AND json_extract(i.payload_json,'$.message.chat.id')=?4 AND COALESCE(json_extract(i.payload_json,'$.message.message_thread_id'),0)=?5 AND (COALESCE(json_extract(i.payload_json,'$.message.text'),'') NOT LIKE '/%' OR (json_extract(i.payload_json,'$.message.text') LIKE '/goal%' AND instr(json_extract(i.payload_json,'$.message.text'),' ')>0) OR trim(json_extract(i.payload_json,'$.message.text'))='/stop'))",params![job.human,job.native,barrier,job.target.chat_id,job.target.thread_id],|r|r.get(0))?;
    if newer {
        bail!("A later captured human input superseded automatic capacity recovery");
    }
    Ok(())
}

pub(super) fn validate_control_on(
    conn: &Connection,
    human: i64,
    target: SessionKey,
    doc: &Value,
) -> Result<()> {
    let id = doc["recovery"]["id"]
        .as_str()
        .context("Recovery association missing")?;
    let job = retained_on(conn, id)?;
    authority_on(conn, &job)?;
    if job.human != human
        || job.target != target
        || job.control.as_deref() != doc["recovery"]["control"].as_str()
        || !matches!(job.phase.as_str(), "claimed" | "submitted")
    {
        bail!("Recovery control no longer owns this attempt");
    }
    Ok(())
}

pub(super) fn pending_on(
    conn: &Connection,
    target: SessionKey,
    session: i64,
    own: Option<i64>,
) -> Result<()> {
    // A failed local collector leaves its original `undetermined`, even after
    // exact native acceptance. A matching retained terminal failure or completed
    // native lifetime settles admission only (never replay). Match every such
    // historical original, since subsequent continuations have different turns.
    let busy:bool=conn.query_row(r#"
      SELECT EXISTS(SELECT 1 FROM turns WHERE session_id=?1 AND status='running'
          AND completed_at IS NULL AND (?2 IS NULL OR id!=?2))
      OR EXISTS(SELECT 1 FROM conversation_inputs WHERE session_id=?1 AND retired=0
          AND phase IN ('retained','retained_unsent','preparing','preparation_unconfirmed','queued','sending','steering','uncertain','awaiting_target'))
      OR EXISTS(SELECT 1 FROM incoming_updates i
          WHERE status IN ('received','processing','queued','steering','uncertain','undetermined')
          AND json_extract(payload_json,'$.message.chat.id')=?3
          AND COALESCE(json_extract(payload_json,'$.message.message_thread_id'),0)=?4
          AND NOT (i.status='undetermined' AND EXISTS(
              SELECT 1 FROM native_input_receipts r JOIN sessions s
                ON s.id=?1 AND s.codex_thread_id=r.native_thread
              WHERE r.update_id=i.update_id
                AND s.creator_user_id=json_extract(i.payload_json,'$.message.from.id')
                AND (EXISTS(SELECT 1 FROM goal_failure_observations f
                      WHERE f.native_thread=r.native_thread AND f.native_turn=r.native_turn
                        AND f.session_id=s.id AND f.human_id=s.creator_user_id AND f.terminal_error IS NOT NULL)
                  OR EXISTS(SELECT 1 FROM native_collections n JOIN turns t ON t.id=n.local_turn
                      WHERE t.id=i.turn_id AND t.session_id=s.id AND t.from_user_id=s.creator_user_id
                        AND t.native_thread_id=s.codex_thread_id AND n.phase='complete'
                        AND r.native_turn IN (n.first_turn,n.anchor))))))
      OR EXISTS(SELECT 1 FROM human_interactions WHERE session_id=?1
          AND state IN ('pending','answering','unknown') AND kind='native')
      "#,params![session,own,target.chat_id,target.thread_id],|r|r.get(0))?;
    if busy {
        bail!("Current/pending work or human interaction takes precedence over capacity recovery");
    }
    Ok(())
}

impl Store {
    pub(super) fn init_goal_retries(&self) -> Result<()> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        conn.execute_batch("CREATE TABLE IF NOT EXISTS goal_retries(id TEXT PRIMARY KEY,human_id INTEGER NOT NULL,session_id INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,target_chat INTEGER NOT NULL,target_topic INTEGER NOT NULL,native_thread TEXT NOT NULL,failed_turn TEXT NOT NULL,goal TEXT NOT NULL,context TEXT NOT NULL,phase TEXT NOT NULL,next_at INTEGER NOT NULL,attempts INTEGER NOT NULL,control_id TEXT,created_at TEXT NOT NULL,detail TEXT,last_settlement TEXT,UNIQUE(human_id,session_id,native_thread,failed_turn));")?;
        Ok(())
    }
    pub(super) fn reconcile_goal_retries(&self) -> Result<()> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        // The goal-control ledger has already reconciled the old writer. Mirror
        // its exact outcome; a submitted/unknown effect is never requeued.
        conn.execute("UPDATE goal_retries SET phase=CASE WHEN (SELECT phase FROM interaction_goal_controls WHERE id=control_id) IN ('known_unsent','retained_unsent') THEN 'waiting' WHEN (SELECT phase FROM interaction_goal_controls WHERE id=control_id) IN ('acknowledged','rejected','partial','uncertain','superseded') THEN (SELECT phase FROM interaction_goal_controls WHERE id=control_id) ELSE 'uncertain' END WHERE phase IN ('claimed','submitted')",[])?;
        Ok(())
    }

    pub fn goal_recovery_policy(&self, human: i64, change: &GoalRecoveryPolicy) -> Result<Value> {
        if Uuid::parse_str(&change.request_id).is_err() {
            bail!("Recovery policy requires a retained request UUID");
        }
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        super::presentation::entry_on(&tx, human, change.target)?;
        let existing = policy_on(&tx, human, change.target)?;
        if existing
            .as_ref()
            .is_some_and(|v| v["request_id"] == change.request_id)
        {
            if existing
                .as_ref()
                .is_none_or(|v| v["enabled"] != change.enabled)
            {
                bail!("Recovery policy request is immutable");
            }
            return Ok(json!({"policy":existing,"new_revision":false,"native_mutation":false}));
        }
        if existing.as_ref().and_then(|v| v["revision"].as_str())
            != change.expected_revision.as_deref()
        {
            bail!("Recovery policy changed after inspection");
        }
        let value = json!({"revision":Uuid::now_v7().to_string(),"request_id":change.request_id,"enabled":change.enabled,"at":now_string(),"principal":"authenticated_owner_control"});
        tx.execute("INSERT INTO bot_state(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![format!("goal_recovery_policy:{human}:{}:{}",change.target.chat_id,change.target.thread_id),value.to_string()])?;
        tx.commit()?;
        Ok(json!({"policy":value,"new_revision":true,"native_mutation":false}))
    }

    pub(crate) fn goal_retries_due(&self, limit: usize) -> Result<Vec<GoalRetry>> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let mut query=conn.prepare("SELECT id FROM goal_retries WHERE phase='waiting' AND next_at<=?1 ORDER BY next_at,rowid LIMIT ?2")?;
        let ids = query
            .query_map(params![Utc::now().timestamp(), limit.min(16)], |r| {
                r.get::<_, String>(0)
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        ids.iter().map(|id| retained_on(&conn, id)).collect()
    }

    pub(crate) fn goal_retry_defer(&self, id: &str, phase: &str, detail: &str) -> Result<()> {
        if !matches!(phase, "waiting" | "superseded" | "disabled") {
            bail!("Invalid unsubmitted recovery state");
        }
        let conn = self.conn.lock().expect("store mutex poisoned");
        conn.execute(
            "UPDATE goal_retries SET phase=?2,next_at=?3,detail=?4 WHERE id=?1 AND phase='waiting'",
            params![id, phase, Utc::now().timestamp() + 60, detail],
        )?;
        Ok(())
    }
}

impl Store {
    /// This entry is backend-owned, never callable by model arguments. Native
    /// observation is an atomic snapshot plus its actual latest terminal turn.
    pub(crate) fn goal_retry_claim(
        &self,
        id: &str,
        snapshot: &NativeGoalSnapshot,
        latest: &Value,
    ) -> Result<Option<Value>> {
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let job = retained_on(&tx, id)?;
        if job.phase != "waiting" {
            return Ok(None);
        }
        let due: bool = tx.query_row(
            "SELECT next_at<=?2 FROM goal_retries WHERE id=?1",
            params![id, Utc::now().timestamp()],
            |r| r.get(0),
        )?;
        if !due {
            return Ok(None);
        }
        if let Err(error) = authority_on(&tx, &job) {
            if observation_unavailable(&error) {
                return Err(error);
            }
            tx.execute("UPDATE goal_retries SET phase='superseded',detail='Binding, delegation, policy, later intent or fence changed' WHERE id=?1",[id])?;
            tx.commit()?;
            return Ok(None);
        }
        let Some(goal) = &snapshot.goal else {
            tx.execute(
                "UPDATE goal_retries SET phase='superseded',detail='Goal is absent' WHERE id=?1",
                [id],
            )?;
            tx.commit()?;
            return Ok(None);
        };
        if !goal.same_definition(&job.goal)
            || goal.budget_exhausted()
            || latest["id"] != job.failed_turn
            || latest["status"] != "failed"
        {
            tx.execute("UPDATE goal_retries SET phase='superseded',detail='Current goal, budget or latest native turn no longer matches the causal failure' WHERE id=?1",[id])?;
            tx.commit()?;
            return Ok(None);
        }
        let session = job.context["binding"][1]
            .as_i64()
            .context("Recovery session missing")?;
        if let Err(error) = pending_on(&tx, job.target, session, None) {
            if observation_unavailable(&error) {
                return Err(error);
            }
            tx.execute("UPDATE goal_retries SET next_at=?2,detail='Waiting for current or pending work/human interaction' WHERE id=?1",params![id,Utc::now().timestamp()+60])?;
            tx.commit()?;
            return Ok(None);
        }
        let control = job.control.clone().unwrap_or_else(|| {
            Uuid::new_v5(
                &Uuid::NAMESPACE_URL,
                format!("goal-recovery-control:{id}").as_bytes(),
            )
            .to_string()
        });
        if job.control.is_none() {
            let plan = GoalPlan {
                action: GoalAction::Resume,
                expected: snapshot.clone(),
                objective: None,
                token_budget: None,
                followup: None,
            };
            plan.validate()?;
            let mut delegation = job.context["delegation"].clone();
            delegation["principal"] = json!("recovery");
            let doc = json!({"entry":job.target,"entry_binding":job.context["binding"],"target":job.target,"target_binding":job.context["binding"],
                "goal_command_generation":job.context["command_generation"],"plan":plan,"reason":"Continue the same native goal after its identified terminal provider-capacity failure, under retained human delegation; never replay the failed input or tools.",
                "original":null,"proposal":false,"confirmation":null,"delegation":delegation,"authorization_order":job.context["original_barrier"],
                "actor":{"kind":"recovery","principal":"telecodex.goal-recovery","receiver_instance":self.instance_id},
                "recovery":{"id":id,"control":control,"failed_turn":job.failed_turn,"cause":"serverOverloaded"},"created_at":now_string()});
            tx.execute("INSERT INTO interaction_goal_controls(id,human_id,entry_chat,entry_topic,target_chat,target_topic,phase,doc,instance_id) VALUES(?1,?2,?3,?4,?3,?4,'prepared',?5,?6)",params![control,job.human,job.target.chat_id,job.target.thread_id,doc.to_string(),self.instance_id])?;
        } else {
            let retained = super::goal_controls::read_on(&tx, job.human, job.target, &control)?;
            let expected: NativeGoalSnapshot =
                serde_json::from_value(retained["intent"]["plan"]["expected"].clone())?;
            if snapshot.revision != expected.revision
                || !crate::codex::goal_definitions_match(
                    snapshot.goal.as_ref(),
                    expected.goal.as_ref(),
                )
            {
                tx.execute("UPDATE goal_retries SET phase='superseded',detail='Prepared native revision changed' WHERE id=?1",[id])?;
                tx.commit()?;
                return Ok(None);
            }
        }
        tx.execute(
            "UPDATE goal_retries SET phase='claimed',control_id=?2,attempts=attempts+1 WHERE id=?1",
            params![id, control],
        )?;
        let (value, fresh) = super::goal_controls::claim_on(
            &tx,
            &self.instance_id,
            job.human,
            job.target,
            &control,
            json!({"kind":"recovery","principal":"telecodex.goal-recovery","receiver_instance":self.instance_id}),
        )?;
        if !fresh {
            tx.execute(
                "UPDATE goal_retries SET phase=?2 WHERE id=?1",
                params![id, value["phase"].as_str()],
            )?;
        }
        tx.commit()?;
        Ok(fresh.then_some(value))
    }
}

fn observation_unavailable(error: &anyhow::Error) -> bool {
    error
        .downcast_ref::<rusqlite::Error>()
        .is_some_and(|e| !matches!(e, rusqlite::Error::QueryReturnedNoRows))
        || error.downcast_ref::<serde_json::Error>().is_some()
}

/// Called in the exact goal receipt transaction, including collector retirement
/// and startup reconciliation. Routine retry outcomes never generate bot spam.
pub(super) fn settle_on(conn: &Connection, control: &str, phase: &str) -> Result<()> {
    let row:Option<(String,u32,Option<String>,Option<String>)>=conn.query_row("SELECT j.id,j.attempts,j.last_settlement,json_extract(c.doc,'$.attempt') FROM goal_retries j JOIN interaction_goal_controls c ON c.id=j.control_id WHERE j.control_id=?1",[control],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?;
    let Some((id, attempts, previous, attempt)) = row else {
        return Ok(());
    };
    let settlement = json!([control, attempt, phase]).to_string();
    if previous.as_deref() == Some(&settlement) {
        return Ok(());
    }
    let state = if matches!(phase, "known_unsent" | "retained_unsent") {
        "waiting"
    } else {
        phase
    };
    conn.execute("UPDATE goal_retries SET phase=?2,next_at=?3,detail=?4,last_settlement=?5 WHERE id=?1",params![id,state,Utc::now().timestamp()+delay_seconds(&id,attempts),format!("Conditional native control: {phase}; acknowledgment is separate from useful recovered work"),settlement])?;
    Ok(())
}

impl Store {
    pub(crate) fn goal_retry_validate(&self, id: &str) -> Result<bool> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        match authority_on(&conn, &retained_on(&conn, id)?) {
            Ok(()) => Ok(true),
            Err(error) if observation_unavailable(&error) => Err(error),
            Err(_) => Ok(false),
        }
    }
}

pub(super) fn view_on(
    conn: &Connection,
    human: i64,
    key: SessionKey,
    session: i64,
    native: &str,
) -> Result<Value> {
    let grant = super::goal_delegations::grant_on(conn, human, key, true).ok();
    let policy = policy_on(conn, human, key)?;
    let backend: Option<Value> = conn
        .query_row(
            "SELECT value FROM bot_state WHERE key='goal_recovery_backend'",
            [],
            |r| r.get::<_, String>(0),
        )
        .optional()?
        .map(|s| serde_json::from_str(&s))
        .transpose()?;
    type RetryStatus = (String, String, i64, u32, Option<String>, Option<String>);
    let row:Option<RetryStatus>=conn.query_row("SELECT id,phase,next_at,attempts,control_id,detail FROM goal_retries WHERE human_id=?1 AND session_id=?2 AND native_thread=?3 ORDER BY rowid DESC LIMIT 1",params![human,session,native],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?))).optional()?;
    let latest=row.map(|(id,phase,due,attempts,control,detail)|json!({"id":id,"state":phase,"next_attempt_at":if phase=="waiting" {json!(due)}else{Value::Null},"attempts":attempts,"control":control,"detail":detail,"principal":"telecodex.goal-recovery"}));
    Ok(
        json!({"schema":"telecodex.goal-recovery-state/v1","authorized":grant.is_some(),"delegation_revision":grant.as_ref().map(|g|&g["revision"]),"policy":policy,"backend":backend,"effective_enabled":backend.as_ref().map(|b|b["enabled"]==true && grant.is_some() && policy.as_ref().is_none_or(|p|p["enabled"]!=false)),"enabled_by_scope":grant.is_some() && policy.as_ref().is_none_or(|p|p["enabled"]!=false),"latest":latest,"meaning":"Authorization and retained scheduler state; acknowledged continuation is separate from useful native recovery."}),
    )
}

impl Store {
    pub fn goal_retry_native_terminal(
        &self,
        human: i64,
        key: SessionKey,
        native: &str,
        turn: &str,
        status: &str,
    ) -> Result<()> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let (session, bound) = super::presentation::entry_on(&conn, human, key)?;
        if bound != native {
            bail!("Recovery terminal observation belongs to an old binding");
        }
        let output:bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM interaction_native_messages WHERE native_thread=?1 AND native_turn=?2 AND phase='final_answer' AND length(trim(text))>0)",params![native,turn],|r|r.get(0))?;
        let phase = match status {
            "completed" if output => "returned",
            "completed" => "native_completed",
            "failed" => "failed_again",
            "interrupted" => "interrupted",
            _ => return Ok(()),
        };
        conn.execute("UPDATE goal_retries SET phase=?5,detail=?6 WHERE human_id=?1 AND session_id=?2 AND native_thread=?3 AND phase='acknowledged' AND EXISTS(SELECT 1 FROM interaction_goal_controls c WHERE c.id=goal_retries.control_id AND json_extract(c.result,'$.native_receipt.activation.turnId')=?4)",params![human,session,native,turn,phase,format!("Observed native turn {status}; final answer retained: {output}. This does not declare the goal complete or prove human receipt.")])?;
        Ok(())
    }
}

impl Store {
    pub fn goal_retry_quiet_capacity(&self, local: i64) -> Result<bool> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let id:Option<String>=conn.query_row("SELECT j.id FROM goal_retries j JOIN interaction_goal_controls c ON c.id=j.control_id WHERE json_extract(c.doc,'$.worker_execution.local')=?1 ORDER BY j.rowid DESC LIMIT 1",[local],|r|r.get(0)).optional()?;
        let Some(id) = id else {
            return Ok(false);
        };
        Ok(authority_on(&conn, &retained_on(&conn, &id)?).is_ok())
    }
}

pub(super) fn compact_view_on(
    conn: &Connection,
    human: i64,
    key: SessionKey,
    session: i64,
    native: &str,
) -> Result<Value> {
    let view = view_on(conn, human, key, session, native)?;
    Ok(
        json!({"enabled":view["effective_enabled"],"state":view["latest"]["state"],"next_attempt_at":view["latest"]["next_attempt_at"],"attempts":view["latest"]["attempts"]}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::goal_delegations::GoalDelegationUpdate;
    thread_local! {
        // Model a native event in the second after grant installation, without
        // sleeps or unstable timestamps across duplicate observations.
        static GOAL_TIME: std::cell::Cell<i64> = const { std::cell::Cell::new(0) };
    }
    fn goal() -> NativeGoal {
        NativeGoal {
            thread_id: "native".into(),
            objective: "Retain the work and accounting".into(),
            status: "blocked".into(),
            token_budget: Some(100),
            tokens_used: 42,
            time_used_seconds: 3,
            created_at: 1,
            updated_at: GOAL_TIME.get(),
        }
    }
    fn fixture(file: &std::path::Path) -> (Store, SessionKey) {
        GOAL_TIME.set(Utc::now().timestamp());
        let store = Store::open(file, &[100], &super::super::tests::defaults()).unwrap();
        let key = SessionKey::new(100, Some(7));
        store
            .ensure_session(key, 100, &super::super::tests::defaults())
            .unwrap();
        store.set_session_codex_thread(key, "native").unwrap();
        (store, key)
    }
    fn grant(store: &Store) {
        store
            .goal_delegation_update(
                100,
                &GoalDelegationUpdate {
                    request_id: Uuid::now_v7().to_string(),
                    expected_revision: None,
                    source: "https://example.test/human-delegation".into(),
                    agents: false,
                    recovery: true,
                    scope: "owned_sessions".into(),
                },
            )
            .unwrap();
        GOAL_TIME.set(Utc::now().timestamp() + 1);
    }
    fn failure(store: &Store, key: SessionKey, turn: &str) {
        store
            .goal_recovery_block_observed(key, 100, "native", turn, &goal())
            .unwrap();
        store
            .goal_recovery_failure_observed(
                key,
                100,
                "native",
                turn,
                &json!({"codexErrorInfo":"serverOverloaded"}),
            )
            .unwrap();
    }
    fn due(store: &Store) -> Vec<GoalRetry> {
        store
            .conn
            .lock()
            .unwrap()
            .execute(
                "UPDATE goal_retries SET next_at=0 WHERE phase='waiting'",
                [],
            )
            .unwrap();
        store.goal_retries_due(16).unwrap()
    }
    fn claim(store: &Store, job: &GoalRetry) -> Value {
        store
            .goal_retry_claim(
                &job.id,
                &NativeGoalSnapshot {
                    goal: Some(goal()),
                    revision: "opaque-revision".into(),
                },
                &json!({"id":job.failed_turn,"status":"failed"}),
            )
            .unwrap()
            .unwrap()
    }
    fn local(store: &Store, key: SessionKey) -> i64 {
        let session = store.get_session(key).unwrap().unwrap();
        let request = TurnRequest {
            session_key: key,
            from_user_id: 100,
            prompt: "Application-owned synthetic qualification".into(),
            runtime_instructions: None,
            attachments: vec![],
            review_mode: None,
            override_search_mode: None,
        };
        let id = store.record_turn_started(session.id, &request).unwrap();
        store.record_turn_native_thread(id, "native").unwrap();
        id
    }
    #[test]
    fn only_causal_failures_observed_under_a_retained_recovery_grant_schedule() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let (store, key) = fixture(file.path());
        failure(&store, key, "old");
        grant(&store);
        assert!(
            due(&store).is_empty(),
            "A new grant must not revive old blocks"
        );
        for _ in 0..2 {
            failure(&store, key, "current");
        }
        let jobs = due(&store);
        assert_eq!(jobs.len(), 1);
        let control = claim(&store, &jobs[0]);
        assert_eq!(control["phase"], "claimed");
        assert_eq!(control["intent"]["delegation"]["principal"], "recovery");
        assert_eq!(control["intent"]["actor"]["kind"], "recovery");
        assert_eq!(control["intent"]["original"], Value::Null);
        assert!(
            store
                .goal_retry_claim(
                    &jobs[0].id,
                    &NativeGoalSnapshot {
                        goal: Some(goal()),
                        revision: "opaque-revision".into()
                    },
                    &json!({"id":"current","status":"failed"})
                )
                .unwrap()
                .is_none()
        );
        let own = local(&store, key);
        let id = control["id"].as_str().unwrap();
        let attempt = control["intent"]["attempt"].as_str().unwrap();
        store
            .goal_control_link_execution(100, key, id, attempt, own, "native")
            .unwrap();
        store
            .goal_control_begin_native_submission(100, key, id, attempt)
            .unwrap();
        let receipt =
            json!({"native_receipt":{"activation":{"kind":"started","turnId":"resumed"}}});
        store
            .goal_control_finish(100, key, id, attempt, "acknowledged", &receipt)
            .unwrap();
        let conn = store.conn.lock().unwrap();
        assert_eq!(
            conn.query_row("SELECT phase FROM goal_retries", [], |r| r
                .get::<_, String>(0))
                .unwrap(),
            "acknowledged"
        );
        for table in [
            "incoming_updates",
            "conversation_inputs",
            "original_ingress_order",
            "interaction_goal_inputs",
            "topic_publications",
        ] {
            assert_eq!(
                conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r
                    .get::<_, i64>(0))
                    .unwrap(),
                0,
                "Fabricated ingress or retry spam in {table}"
            );
        }
    }
    #[test]
    fn later_pause_policy_stop_rebind_budget_and_native_turn_win_before_admission() {
        for change in [
            "intent",
            "policy",
            "stop",
            "binding",
            "budget",
            "native_turn",
        ] {
            let file = tempfile::NamedTempFile::new().unwrap();
            let (store, key) = fixture(file.path());
            grant(&store);
            failure(&store, key, "failed");
            let jobs = due(&store);
            let mut snapshot = NativeGoalSnapshot {
                goal: Some(goal()),
                revision: "opaque-revision".into(),
            };
            let mut latest = json!({"id":"failed","status":"failed"});
            match change {
                "intent" => store
                    .save_bot_state("goal_intent:100:7", "later-pause")
                    .unwrap(),
                "policy" => {
                    store
                        .goal_recovery_policy(
                            100,
                            &GoalRecoveryPolicy {
                                request_id: Uuid::now_v7().to_string(),
                                target: key,
                                expected_revision: None,
                                enabled: false,
                            },
                        )
                        .unwrap();
                }
                "stop" => {
                    store.stop_begin(100, &Uuid::now_v7().to_string()).unwrap();
                }
                "binding" => store.set_session_codex_thread(key, "another").unwrap(),
                "budget" => snapshot.goal.as_mut().unwrap().tokens_used = 100,
                "native_turn" => latest["id"] = json!("later-turn"),
                _ => unreachable!(),
            }
            assert!(
                store
                    .goal_retry_claim(&jobs[0].id, &snapshot, &latest)
                    .unwrap()
                    .is_none(),
                "{change}"
            );
            assert_eq!(
                store
                    .conn
                    .lock()
                    .unwrap()
                    .query_row("SELECT COUNT(*) FROM interaction_goal_controls", [], |r| {
                        r.get::<_, i64>(0)
                    })
                    .unwrap(),
                0,
                "{change}"
            );
        }
    }
    #[test]
    fn restart_retries_only_proven_unsent_and_duplicate_receipts_do_not_move_due_time() {
        for submitted in [false, true] {
            let file = tempfile::NamedTempFile::new().unwrap();
            let (store, key) = fixture(file.path());
            grant(&store);
            failure(&store, key, "failed");
            let job = due(&store).remove(0);
            let control = claim(&store, &job);
            let id = control["id"].as_str().unwrap();
            let attempt = control["intent"]["attempt"].as_str().unwrap();
            if submitted {
                store
                    .goal_control_begin_native_submission(100, key, id, attempt)
                    .unwrap();
            } else {
                store
                    .goal_control_finish(
                        100,
                        key,
                        id,
                        attempt,
                        "known_unsent",
                        &json!({"before_write":true}),
                    )
                    .unwrap();
                let before: i64 = store
                    .conn
                    .lock()
                    .unwrap()
                    .query_row("SELECT next_at FROM goal_retries", [], |r| r.get(0))
                    .unwrap();
                store
                    .conn
                    .lock()
                    .unwrap()
                    .execute("UPDATE goal_retries SET next_at=next_at-10", [])
                    .unwrap();
                store
                    .goal_control_finish(
                        100,
                        key,
                        id,
                        attempt,
                        "known_unsent",
                        &json!({"before_write":true}),
                    )
                    .unwrap();
                assert_eq!(
                    store
                        .conn
                        .lock()
                        .unwrap()
                        .query_row("SELECT next_at FROM goal_retries", [], |r| r
                            .get::<_, i64>(0))
                        .unwrap(),
                    before - 10
                );
            }
            drop(store);
            let store = Store::open(file.path(), &[100], &super::super::tests::defaults()).unwrap();
            let state: String = store
                .conn
                .lock()
                .unwrap()
                .query_row("SELECT phase FROM goal_retries", [], |r| r.get(0))
                .unwrap();
            assert_eq!(state, if submitted { "uncertain" } else { "waiting" });
            if submitted {
                assert!(due(&store).is_empty());
            }
        }
        for attempt in [0, 1, 6, 1000, u32::MAX] {
            let delay = delay_seconds("stable-retry", attempt);
            assert!((27..=1980).contains(&delay));
            assert_eq!(delay, delay_seconds("stable-retry", attempt));
        }
    }

    #[test]
    fn delayed_first_block_in_the_grant_second_has_no_proven_prior_authority() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let (store, key) = fixture(file.path());
        grant(&store);
        let created = store.goal_delegation_read(100).unwrap()["delegation"]["created_at"]
            .as_str()
            .unwrap()
            .to_owned();
        GOAL_TIME.set(
            chrono::DateTime::parse_from_rfc3339(&created)
                .unwrap()
                .timestamp(),
        );
        failure(&store, key, "delayed-first-observation");
        assert!(
            due(&store).is_empty(),
            "Equal-second ordering cannot establish a pre-fault grant"
        );
    }

    #[test]
    fn only_exact_accepted_terminal_originals_stop_fencing_a_continuation() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let (store, key) = fixture(file.path());
        grant(&store);
        let local = local(&store, key);
        let update: crate::telegram::Update = serde_json::from_value(json!({"update_id":1,"message":{"message_id":1,"chat":{"id":100,"type":"private"},"from":{"id":100,"is_bot":false,"first_name":"Human"},"message_thread_id":7,"text":"Continue the retained work"}})).unwrap();
        store.admit_update(&update).unwrap();
        store.begin_update(1).unwrap();
        store.mark_update_queued(1).unwrap();
        store.link_update_turn(1, local).unwrap();
        store
            .native_input_receipt(local, None, "original-failed")
            .unwrap();
        store.record_turn_finished(local, "failed", None).unwrap();
        let session = store.get_session(key).unwrap().unwrap().id;
        {
            let conn = store.conn.lock().unwrap();
            assert_eq!(
                conn.query_row(
                    "SELECT status FROM incoming_updates WHERE update_id=1",
                    [],
                    |r| r.get::<_, String>(0)
                )
                .unwrap(),
                "undetermined"
            );
            assert!(
                pending_on(&conn, key, session, None).is_err(),
                "Acceptance without a retained terminal outcome is still uncertain"
            );
        }
        failure(&store, key, "original-failed");
        // A later retry still recognizes the original terminal receipt; it must
        // not require that old receipt to name the latest failed continuation.
        failure(&store, key, "subsequent-failed");
        let conn = store.conn.lock().unwrap();
        pending_on(&conn, key, session, None).unwrap();
        conn.execute("DELETE FROM native_input_receipts WHERE update_id=1", [])
            .unwrap();
        assert!(
            pending_on(&conn, key, session, None).is_err(),
            "Terminal outcome without exact acceptance must stay fenced"
        );
        conn.execute(
            "INSERT INTO native_input_receipts VALUES(1,'native','original-failed',?1)",
            [now_string()],
        )
        .unwrap();
        conn.execute(
            "UPDATE incoming_updates SET status='uncertain' WHERE update_id=1",
            [],
        )
        .unwrap();
        assert!(
            pending_on(&conn, key, session, None).is_err(),
            "An actually uncertain ingress must not be waived"
        );
        conn.execute(
            "UPDATE incoming_updates SET status='undetermined' WHERE update_id=1",
            [],
        )
        .unwrap();
        conn.execute("INSERT INTO incoming_updates SELECT 2,payload_json,'queued',instance_id,NULL,updated_at FROM incoming_updates WHERE update_id=1",[]).unwrap();
        assert!(
            pending_on(&conn, key, session, None).is_err(),
            "A separate pending original must win"
        );
        conn.execute("DELETE FROM incoming_updates WHERE update_id=2", [])
            .unwrap();
        conn.execute("DELETE FROM goal_failure_observations", [])
            .unwrap();
        assert!(
            pending_on(&conn, key, session, None).is_err(),
            "Acceptance alone is not terminal proof"
        );
        conn.execute("INSERT INTO native_collections(local_turn,anchor,first_turn,phase) VALUES(?1,'original-failed','original-failed','complete')",[local]).unwrap();
        pending_on(&conn, key, session, None).unwrap();
        for phase in ["open", "transferred", "boundary"] {
            conn.execute("UPDATE native_collections SET phase=?1", [phase])
                .unwrap();
            assert!(
                pending_on(&conn, key, session, None).is_err(),
                "{phase} is not proof of a terminal native lifetime"
            );
        }
    }

    #[test]
    fn interrupted_prewrite_claim_retains_prior_attempt_and_actor_after_retry() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let (store, key) = fixture(file.path());
        grant(&store);
        failure(&store, key, "failed");
        let job = due(&store).remove(0);
        let first = claim(&store, &job);
        let id = first["id"].as_str().unwrap().to_owned();
        let attempt = first["intent"]["attempt"].as_str().unwrap().to_owned();
        let actor = first["intent"]["execution_actor"].clone();
        drop(store);
        let store = Store::open(file.path(), &[100], &super::super::tests::defaults()).unwrap();
        let second = claim(&store, &due(&store).remove(0));
        assert_ne!(second["intent"]["attempt"], attempt);
        assert_eq!(second["intent"]["previous_outcomes"][0]["attempt"], attempt);
        let conn = store.conn.lock().unwrap();
        let (result,executor):(String,String)=conn.query_row("SELECT result,json_extract(actor,'$.executor') FROM goal_control_attempt_receipts WHERE control_id=?1 AND attempt=?2 AND phase='retained_unsent'",params![id,attempt],|r|Ok((r.get(0)?,r.get(1)?))).unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&result).unwrap()["native_write_started"],
            false
        );
        assert_eq!(serde_json::from_str::<Value>(&executor).unwrap(), actor);
    }
}
