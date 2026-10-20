//! Native goal metadata and explicit Telegram goal activation.
use super::*;

#[derive(Debug, Clone, serde::Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct NativeGoal {
    pub thread_id: String,
    pub objective: String,
    pub status: String,
    pub token_budget: Option<i64>,
    pub tokens_used: i64,
    pub time_used_seconds: i64,
    pub created_at: i64,
    pub updated_at: i64,
}
impl NativeGoal {
    pub fn budget_exhausted(&self) -> bool {
        self.token_budget
            .is_some_and(|budget| self.tokens_used >= budget)
    }
    pub fn ensure_resumable(&self) -> Result<()> {
        if self.budget_exhausted() {
            bail!(
                "This goal's token budget is exhausted. Resume and objective edits preserve usage; increase the token budget through native goal controls before resuming. Inspect /goal for the current usage and budget."
            );
        }
        Ok(())
    }
    pub fn same_definition(&self, other: &Self) -> bool {
        self.thread_id == other.thread_id
            && self.objective == other.objective
            && self.status == other.status
            && self.token_budget == other.token_budget
            && self.created_at == other.created_at
    }
    pub fn edited_status(&self) -> &str {
        match self.status.as_str() {
            "complete" | "budgetLimited" if self.budget_exhausted() => "budgetLimited",
            "complete" | "budgetLimited" => "active",
            other => other,
        }
    }
}

/// Native definition/incarnation precondition; usage increments do not change it.
#[derive(Debug, Clone, serde::Serialize, Deserialize, PartialEq, Eq)]
pub struct NativeGoalSnapshot {
    pub goal: Option<NativeGoal>,
    pub revision: String,
}
pub(super) fn parse_goal_snapshot(response: &Value, thread_id: &str) -> Result<NativeGoalSnapshot> {
    let revision=response.get("revision").and_then(Value::as_str)
        .filter(|value|!value.is_empty() && value.len()<=512 && value.bytes().all(|c|(33..=126).contains(&c)))
        .context("Native goal snapshot has no bounded opaque revision; legacy get is not an atomic precondition")?;
    Ok(NativeGoalSnapshot {
        goal: parse_goal(response, thread_id)?,
        revision: revision.into(),
    })
}

#[derive(Debug, Clone)]
pub struct GoalActivation {
    pub intent: String,
    pub native_thread: Option<String>,
    pub expected: Option<NativeGoal>,
    pub objective: Option<String>,
    pub replace: bool,
    /// Collector-only attachment: never start a turn or mutate goal metadata.
    pub attach_turn: Option<String>,
    /// Frozen integrator intent, admitted through the native context-aware CAS.
    pub conditional: Option<Box<GoalMetadataControl>>,
}

pub(super) fn parse_goal(response: &Value, thread_id: &str) -> Result<Option<NativeGoal>> {
    let value = response
        .get("goal")
        .context("native goal response has no goal field")?;
    if value.is_null() {
        return Ok(None);
    }
    let goal: NativeGoal = serde_json::from_value(value.clone())?;
    if goal.thread_id != thread_id {
        bail!("native goal response belongs to another thread");
    }
    Ok(Some(goal))
}

impl CodexRunner {
    async fn goal_rpc(&self, method: &str, params: Value) -> Result<Value> {
        tokio::time::timeout(Duration::from_secs(10), async {
            let mut process = AppServerProcess::spawn(&self.binary, self.shared_app_server).await?;
            process.initialize().await?;
            let id = process.send_request(method, params).await?;
            let result = process.await_response(id).await?;
            process.shutdown().await?;
            Ok(result)
        })
        .await
        .context("native goal acknowledgment timed out; inspect /goal before retrying")?
    }
    pub async fn get_goal_snapshot(&self, thread_id: &str) -> Result<NativeGoalSnapshot> {
        parse_goal_snapshot(
            &self
                .goal_rpc("thread/goal/snapshot", json!({"threadId":thread_id}))
                .await?,
            thread_id,
        )
    }
    pub async fn get_goal(&self, thread_id: &str) -> Result<Option<NativeGoal>> {
        parse_goal(
            &self
                .goal_rpc("thread/goal/get", json!({"threadId":thread_id}))
                .await?,
            thread_id,
        )
    }
    pub async fn set_goal(&self, thread_id: &str, mut params: Value) -> Result<NativeGoal> {
        params["threadId"] = json!(thread_id);
        params["origin"] = json!("user");
        parse_goal(&self.goal_rpc("thread/goal/set", params).await?, thread_id)?
            .context("native goal mutation returned no goal")
    }
    pub async fn clear_goal(&self, thread_id: &str) -> Result<bool> {
        self.goal_rpc(
            "thread/goal/clear",
            json!({"threadId":thread_id,"origin":"user"}),
        )
        .await?
        .get("cleared")
        .and_then(Value::as_bool)
        .context("native clear acknowledgment has no cleared field")
    }
}

pub(super) async fn mutate_live_goal(
    process: &mut AppServerProcess,
    thread_id: &str,
    activation: &GoalActivation,
) -> Result<NativeGoal> {
    if activation.native_thread.as_deref() != Some(thread_id) {
        bail!("goal control belongs to another native thread");
    }
    if let Some(control) = &activation.conditional {
        let (goal, _) = conditional_activation(
            process,
            thread_id,
            control,
            control
                .context
                .clone()
                .context("Current writer invocation context missing")?,
        )
        .await?;
        return Ok(goal);
    }
    let id = process
        .send_request("thread/goal/get", json!({"threadId":thread_id}))
        .await?;
    let current = parse_goal(&process.await_response(id).await?, thread_id)?;
    if !super::goal_definitions_match(current.as_ref(), activation.expected.as_ref()) {
        bail!("The goal changed; inspect /goal again.");
    }
    if !activation.replace
        && let Some(goal) = &current
    {
        goal.ensure_resumable()?;
    }
    if activation.replace {
        let id = process
            .send_request(
                "thread/goal/clear",
                json!({"threadId":thread_id,"origin":"user"}),
            )
            .await?;
        let result = process.await_response(id).await?;
        if result.get("cleared").and_then(Value::as_bool) != Some(true) {
            bail!("goal replacement clear was not confirmed");
        }
    }
    let mut params = json!({"threadId":thread_id,"status":"active","origin":"user"});
    if let Some(objective) = &activation.objective {
        params["objective"] = json!(objective);
    }
    let id = process.send_request("thread/goal/set", params).await?;
    parse_goal(&process.await_response(id).await?, thread_id)?
        .context("native goal update returned no goal")
}

#[derive(Debug, Clone, Default)]
pub struct GoalStopSnapshot {
    pub before: Option<NativeGoal>,
    pub after: Option<NativeGoal>,
    pub was_staged: bool,
}
#[derive(Debug, Clone)]
pub enum GoalControl {
    Activate(Box<GoalActivation>),
    Pause,
    Stop(Arc<std::sync::Mutex<GoalStopSnapshot>>),
    Clear,
    ConditionalMetadata(Box<GoalMetadataControl>),
}
pub(super) async fn mutate_control(
    process: &mut AppServerProcess,
    thread_id: &str,
    control: &GoalControl,
) -> Result<Option<NativeGoal>> {
    match control {
        GoalControl::ConditionalMetadata(_) => {
            bail!("Conditional metadata is handled by the subscribed writer")
        }
        GoalControl::Activate(activation) => Ok(Some(
            mutate_live_goal(process, thread_id, activation).await?,
        )),
        GoalControl::Stop(receipt) => {
            let id = process
                .send_request("thread/goal/get", json!({"threadId":thread_id}))
                .await?;
            let before = parse_goal(&process.await_response(id).await?, thread_id)?;
            let after = if before.as_ref().is_some_and(|g| g.status == "active") {
                let id = process
                    .send_request(
                        "thread/goal/set",
                        json!({"threadId":thread_id,"status":"paused","origin":"user"}),
                    )
                    .await?;
                parse_goal(&process.await_response(id).await?, thread_id)?
            } else {
                before.clone()
            };
            let mut snapshot = receipt.lock().expect("goal stop snapshot mutex poisoned");
            snapshot.before = before;
            snapshot.after = after.clone();
            Ok(after)
        }
        GoalControl::Pause => {
            let id = process
                .send_request(
                    "thread/goal/set",
                    json!({"threadId":thread_id,"status":"paused","origin":"user"}),
                )
                .await?;
            parse_goal(&process.await_response(id).await?, thread_id)
        }
        GoalControl::Clear => {
            let id = process
                .send_request(
                    "thread/goal/clear",
                    json!({"threadId":thread_id,"origin":"user"}),
                )
                .await?;
            let result = process.await_response(id).await?;
            result
                .get("cleared")
                .and_then(Value::as_bool)
                .context("clear response unconfirmed")?;
            Ok(None)
        }
    }
}

#[cfg(test)]
mod snapshot_tests {
    use super::*;
    #[test]
    fn snapshot_requires_real_revision_and_matching_native_goal() {
        let empty = parse_goal_snapshot(
            &json!({"goal":null,"revision":"goal-v1:database:thread:0"}),
            "thread",
        )
        .unwrap();
        assert!(empty.goal.is_none());
        for bad in [
            json!({"goal":null}),
            json!({"goal":null,"revision":""}),
            json!({"goal":null,"revision":"forged\nline"}),
        ] {
            assert!(parse_goal_snapshot(&bad, "thread").is_err());
        }
        let mut response = json!({"goal":{"threadId":"other","objective":"selected goal","status":"paused","tokenBudget":1000,"tokensUsed":80,"timeUsedSeconds":4,"createdAt":1,"updatedAt":2},"revision":"opaque-definition"});
        assert!(parse_goal_snapshot(&response, "thread").is_err());
        response["goal"]["threadId"] = json!("thread");
        let selected = parse_goal_snapshot(&response, "thread").unwrap();
        assert_eq!(selected.goal.unwrap().tokens_used, 80);
    }
}

#[derive(Debug, Clone)]
pub struct GoalControlScope {
    pub human: i64,
    pub entry: crate::models::SessionKey,
    pub id: String,
    pub attempt: String,
}

pub type GoalSettlement = Arc<dyn Fn(&str, &Value) -> Result<()> + Send + Sync>;

/// A current worker processes metadata controls on its own subscribed connection.
#[derive(Clone)]
pub struct GoalMetadataControl {
    pub plan: crate::store::GoalPlan,
    pub admission: SteerAdmissionCheck,
    pub receipt: Arc<std::sync::Mutex<Option<(String, Value)>>>,
    pub context: Option<Value>,
    pub scope: Option<GoalControlScope>,
    pub settled: Option<GoalSettlement>,
}
impl std::fmt::Debug for GoalMetadataControl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GoalMetadataControl")
            .field("action", &self.plan.action)
            .finish_non_exhaustive()
    }
}

/// One conditional write, with source/binding validation at the write boundary.
pub(super) async fn conditional_metadata(
    process: &mut AppServerProcess,
    thread: &str,
    plan: &crate::store::GoalPlan,
    admission: &SteerAdmissionCheck,
) -> (String, Value) {
    use crate::store::GoalAction;
    let mut sent = false;
    let outcome = tokio::time::timeout(Duration::from_secs(12), async {
        plan.validate()?;
        if matches!(
            plan.action,
            GoalAction::Start | GoalAction::Resume | GoalAction::Replace
        ) || (matches!(plan.action, GoalAction::Edit)
            && plan
                .expected
                .goal
                .as_ref()
                .is_some_and(|g| g.status == "active"))
        {
            bail!("Active goal changes require the owning continuation collector");
        }
        let mut args =
            json!({"threadId":thread,"expectedRevision":plan.expected.revision,"origin":"user"});
        let method = if matches!(plan.action, GoalAction::Clear) {
            "thread/goal/conditionalClear"
        } else {
            if matches!(plan.action, GoalAction::Pause) {
                args["status"] = json!("paused");
            }
            if let Some(objective) = &plan.objective {
                args["objective"] = json!(objective);
            }
            if let Some(budget) = plan.token_budget {
                args["tokenBudget"] = json!(budget);
            }
            "thread/goal/conditionalSet"
        };
        admission()?;
        sent = true;
        let id = process.send_request(method, args).await?;
        let receipt = process.await_response(id).await?;
        if matches!(plan.action, GoalAction::Clear) {
            if receipt["cleared"].as_bool().is_none()
                || receipt["revision"].as_str().is_none_or(|s| s.is_empty())
            {
                bail!("Malformed conditional clear receipt");
            }
            Ok::<Value, anyhow::Error>(receipt)
        } else {
            Ok(serde_json::to_value(parse_goal_snapshot(
                &receipt, thread,
            )?)?)
        }
    })
    .await;
    match outcome {
        Ok(Ok(receipt)) => (
            "acknowledged".into(),
            json!({"native_receipt":receipt,"meaning":"Native goal metadata acknowledged; this does not interrupt an active turn or prove incorporation of later work."}),
        ),
        Ok(Err(error)) => {
            let code = error.downcast_ref::<RpcError>().and_then(|e| e.code);
            let phase = if !sent {
                "known_unsent"
            } else if matches!(code, Some(-32602..=-32600)) {
                "rejected"
            } else {
                "uncertain"
            };
            (
                phase.into(),
                json!({"error":error.to_string(),"native_error_code":code,"automatic_retry":false}),
            )
        }
        Err(_) => (
            if sent { "uncertain" } else { "known_unsent" }.into(),
            json!({"error":"Conditional goal acknowledgment timed out","automatic_retry":false}),
        ),
    }
}
impl CodexRunner {
    pub async fn conditional_goal_metadata(
        &self,
        thread: &str,
        plan: &crate::store::GoalPlan,
        admission: &SteerAdmissionCheck,
    ) -> (String, Value) {
        let connection = tokio::time::timeout(Duration::from_secs(8), async {
            let mut process = AppServerProcess::spawn(&self.binary, self.shared_app_server).await?;
            process.initialize().await?;
            Ok::<_, anyhow::Error>(process)
        })
        .await;
        let mut process = match connection {
            Ok(Ok(process)) => process,
            _ => {
                return (
                    "known_unsent".into(),
                    json!({"error":"Native goal connection unavailable before mutation submission","automatic_retry":false}),
                );
            }
        };
        let result = conditional_metadata(&mut process, thread, plan, admission).await;
        // Never lose the matched receipt while closing an owned proxy.
        let _ = tokio::time::timeout(Duration::from_secs(2), process.shutdown()).await;
        result
    }
}

/// Admit the useful continuation on the selected collector. Metadata-only
/// controls do not activate a goal: their actual human clause is ordinary input
/// after the matched mutation, with its own retained admission outcome.
pub(super) async fn conditional_continuation(
    process: &mut AppServerProcess,
    thread: &str,
    control: &GoalMetadataControl,
    mut params: Value,
    running: Option<&str>,
) -> Result<(bool, String)> {
    use crate::store::GoalAction;
    let activating = matches!(
        control.plan.action,
        GoalAction::Start | GoalAction::Resume | GoalAction::Replace
    ) || (matches!(control.plan.action, GoalAction::Edit)
        && control
            .plan
            .expected
            .goal
            .as_ref()
            .is_some_and(|g| g.status == "active"));
    if activating {
        let (goal, turn) = conditional_activation(
            process,
            thread,
            control,
            params
                .get("additionalContext")
                .cloned()
                .unwrap_or(Value::Null),
        )
        .await?;
        return Ok((goal.status == "active", turn));
    }
    let followup = control
        .plan
        .followup
        .as_deref()
        .context("Metadata continuation has no human clause")?;
    let scope = control
        .scope
        .as_ref()
        .context("Attributed continuation scope missing")?;
    if params["additionalContext"]
        .as_object()
        .is_none_or(|v| v.is_empty())
    {
        bail!("Current invocation context required before metadata continuation");
    }
    let (metadata_phase, metadata) =
        conditional_metadata(process, thread, &control.plan, &control.admission).await;
    if metadata_phase != "acknowledged" {
        *control
            .receipt
            .lock()
            .expect("goal control receipt mutex poisoned") =
            Some((metadata_phase.clone(), metadata.clone()));
        if let Some(settled) = &control.settled {
            settled(&metadata_phase, &metadata)?;
        }
        bail!("Goal metadata was not acknowledged; continuation was not submitted");
    }
    // Preserve the committed first effect before attempting the second. Restart
    // may not know whether the following write happened, but never loses the CAS.
    let checkpoint = json!({"native_receipt":metadata["native_receipt"],
        "followup_admitted":null,"followup_outcome":"unconfirmed","automatic_retry":false});
    *control
        .receipt
        .lock()
        .expect("goal control receipt mutex poisoned") =
        Some(("submitted".into(), checkpoint.clone()));
    if let Some(settled) = &control.settled {
        settled("submitted", &checkpoint)?;
    }
    let text = format!(
        "[Integrator continuation of retained human control {} after its acknowledged {:?} operation. Human: {}; source conversation: {}:{}. This is the exact selected human clause, not another proposal.]\n{followup}",
        scope.id, control.plan.action, scope.human, scope.entry.chat_id, scope.entry.thread_id
    );
    params["input"] = json!([{"type":"text","text":text,"text_elements":[]}]);
    params["clientUserMessageId"] = json!(format!("goal-followup:{}:{}", scope.id, scope.attempt));
    let mut submitted = false;
    let result = tokio::time::timeout(Duration::from_secs(12), async {
        (control.admission)()?;
        let method = if let Some(turn) = running {
            params = json!({"threadId":thread,"expectedTurnId":turn,
                "input":params["input"],"additionalContext":params["additionalContext"],
                "clientUserMessageId":params["clientUserMessageId"]});
            "turn/steer"
        } else {
            "turn/start"
        };
        submitted = true;
        let id = process.send_request(method, params).await?;
        let response = process.await_response(id).await?;
        let turn = if let Some(expected) = running {
            let observed = response["turnId"]
                .as_str()
                .context("Follow-up steering acknowledgment has no turn")?;
            if observed != expected {
                bail!("Follow-up acknowledgment changed native turn");
            }
            observed
        } else {
            response
                .pointer("/turn/id")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .context("Follow-up start acknowledgment has no turn")?
        };
        Ok::<_, anyhow::Error>(turn.to_owned())
    })
    .await;
    let (phase, receipt) = match &result {
        Ok(Ok(turn)) => (
            "acknowledged",
            json!({"native_receipt":metadata["native_receipt"],"followup_admitted":true,"followup_turn":turn,
            "meaning":"Metadata acknowledged before actual human follow-up admission; goal remains inactive/absent. Incorporation is separate."}),
        ),
        _ => {
            let error = match &result {
                Ok(Err(e)) => e.to_string(),
                _ => "Ordered follow-up acknowledgment timed out".into(),
            };
            let rejected = match &result {
                Ok(Err(e)) => matches!(
                    e.downcast_ref::<RpcError>().and_then(|e| e.code),
                    Some(-32602..=-32600)
                ),
                _ => false,
            };
            (
                "partial",
                json!({"native_receipt":metadata["native_receipt"],"followup_admitted":if !submitted || rejected {json!(false)} else {Value::Null},
                "followup_outcome":if !submitted || rejected {"known_unsent"} else {"uncertain"},"error":error,"automatic_retry":false}),
            )
        }
    };
    *control
        .receipt
        .lock()
        .expect("goal control receipt mutex poisoned") = Some((phase.into(), receipt.clone()));
    if let Some(settled) = &control.settled {
        settled(phase, &receipt)?;
    }
    let turn = result.context(
        "Ordered follow-up timed out after acknowledged metadata; never replay the control",
    )??;
    Ok((false, turn))
}

/// Use the existing subscribed collector for a context-aware native activation.
/// The receipt is retained before returning an error or collecting model work.
pub(super) async fn conditional_activation(
    process: &mut AppServerProcess,
    thread: &str,
    control: &GoalMetadataControl,
    context: Value,
) -> Result<(NativeGoal, String)> {
    use crate::store::GoalAction;
    let mut context = context;
    let mut sent = false;
    let mut cleared: Option<Value> = None;
    let mut acknowledged: Option<Value> = None;
    let result = tokio::time::timeout(Duration::from_secs(12), async {
        control.plan.validate()?;
        if !matches!(control.plan.action, GoalAction::Start | GoalAction::Resume | GoalAction::Replace | GoalAction::Edit) {
            bail!("Selected goal control is not an activation");
        }
        if context.as_object().is_none_or(|v| v.is_empty()) {
            bail!("Fresh invocation context required before native activation");
        }
        if let Some(followup)=&control.plan.followup {
            let scope=control.scope.as_ref().context("Attributed follow-up control scope required")?;
            let (human,entry,id)=(&scope.human,&scope.entry,&scope.id);
            // The native CAS must succeed before this context is admitted to useful
            // work. Carry the frozen clause in that same admission, never a later
            // priming/steering turn which can lose a race with the first answer.
            context["telecodex.goal.followup"]=json!({"kind":"application","value":format!("This is an attributed continuation of the actual human goal-control instruction retained by this receiver, not another input or assistant proposal. Authorized human: {human}; originating conversation: {}:{}; control: {id}. The following exact human-selected clause belongs to the same accepted control. Incorporate it while carrying out the selected goal. Native admission is not a declaration that you incorporated or completed it.\nCurrent attributed human follow-up:\n{followup}",entry.chat_id,entry.thread_id)});
        }
        let mut revision = control.plan.expected.revision.clone();
        if matches!(control.plan.action, GoalAction::Replace) {
            (control.admission)()?;
            sent = true;
            let id = process.send_request("thread/goal/conditionalClear", json!({"threadId":thread,"expectedRevision":revision,"origin":"user"})).await?;
            let response = process.await_response(id).await?;
            // A matched clear is already an effect even if its revision is malformed.
            cleared = Some(response.clone());
            if response["cleared"] != true { bail!("Conditional replacement clear was not confirmed"); }
            revision = response["revision"].as_str().filter(|v|!v.is_empty()).context("Conditional replacement clear has no revision")?.to_owned();
        }
        let mut args = json!({"threadId":thread,"expectedRevision":revision,"origin":"user","additionalContext":context});
        if let Some(objective) = &control.plan.objective { args["objective"] = json!(objective); }
        if let Some(budget) = control.plan.token_budget { args["tokenBudget"] = json!(budget); }
        (control.admission)()?;
        sent = true;
        let id = process.send_request("thread/goal/conditionalActivate", args).await?;
        let response = process.await_response(id).await?;
        acknowledged = Some(response.clone());
        let goal = parse_goal_snapshot(&response, thread)?.goal.context("Activation returned no goal")?;
        match response["activation"]["kind"].as_str() {
            Some("started" | "steered") => {
                let turn = response["activation"]["turnId"].as_str().filter(|v|!v.is_empty()).context("Native activation has no physical turn")?;
                Ok::<_,anyhow::Error>((goal, turn.to_owned()))
            }
            _ => bail!("Native goal metadata committed without confirmed work admission: {}", response["activation"]),
        }
    }).await;
    let (phase, receipt) = match &result {
        Ok(Ok(_)) => (
            "acknowledged",
            json!({"native_receipt":acknowledged,"replacement_clear":cleared,"followup_admitted":control.plan.followup.is_some(),"meaning":"Native goal metadata and work admission acknowledged; incorporation, completion and hearing are separate."}),
        ),
        _ => {
            let error = match &result {
                Ok(Err(error)) => error.to_string(),
                _ => "Context-aware goal activation acknowledgment timed out".into(),
            };
            let code = match &result {
                Ok(Err(error)) => error.downcast_ref::<RpcError>().and_then(|e| e.code),
                _ => None,
            };
            let phase = if cleared.is_some()
                || acknowledged
                    .as_ref()
                    .is_some_and(|r| r["activation"]["kind"] == "notStarted")
            {
                "partial"
            } else if acknowledged.is_some() {
                "uncertain"
            } else if !sent {
                "known_unsent"
            } else if matches!(code, Some(-32602..=-32600)) {
                "rejected"
            } else {
                "uncertain"
            };
            (
                phase,
                json!({"error":error,"native_error_code":code,"native_receipt":acknowledged,"replacement_clear":cleared,"automatic_retry":false}),
            )
        }
    };
    *control
        .receipt
        .lock()
        .expect("goal control receipt mutex poisoned") = Some((phase.into(), receipt.clone()));
    if let Some(settled) = &control.settled {
        settled(phase, &receipt)?;
    }
    result.context("Context-aware goal activation timed out; inspect retained control, never replay automatically")?
}
