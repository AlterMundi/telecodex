//! Integrator interpretations retain actual human originals and native preconditions.
use super::*;
use crate::{
    codex::NativeGoalSnapshot,
    interaction::InputReference,
    store::{GoalAction, GoalPlan, LiveTaskActor},
};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Prepare {
    selector: String,
    target_binding: Value,
    #[serde(default)]
    input_ref: Option<InputReference>,
    plan: GoalPlan,
    reason: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Selection {
    id: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Decision {
    id: String,
    input_ref: InputReference,
    execute: bool,
}

pub(super) fn capabilities(value: &mut Value) {
    value["operations"]["goal_prepare"] = json!({"arguments":{"selector":"one enrolled target","target_binding":"exact goal_inspect target_binding","input_ref":"actual retained explicit human command reference, omit for a daimon action only with effective delegation; otherwise an assistant proposal","plan":{"action":"start|edit|pause|resume|clear|replace","expected":"exact native {goal,revision} from goal_inspect","objective":"optional selected objective","token_budget":"optional expressly requested budget","followup":"optional exact follow-up clause of the human instruction"},"reason":"bounded explanation"},"description":"Retain one immutable goal-control interpretation. A supplied original must actually request this control. Omitted original uses an effective retained delegation for its supported actions, otherwise it is an assistant proposal requiring later human confirmation. Retention never mutates the goal or starts work. Do not dispatch the same original separately; ordered follow-up belongs to this intent."});
    value["operations"]["goal_decide"] = json!({"arguments":{"id":"retained proposal","input_ref":"actual later captured human confirmation/cancellation","execute":"true only when this human input explicitly accepts the exact proposal"},"description":"Single-use proposal decision; actual capture must postdate proposal creation. No native mutation. Stale goal definitions are rejected at execution, never silently refreshed."});
    value["operations"]["goal_status"] = json!({"arguments":{"id":"retained goal-control ID"},"description":"Inspect immutable interpretation and outcome; never replay uncertain work."});
    value["operations"]["goal_execute"] = json!({"arguments":{"id":"prepared explicit or confirmed goal-control ID"},"description":"Execute one frozen explicit or confirmed control. Pause/clear/inactive edits mutate conditional metadata on the owning writer; an exact human follow-up then uses ordinary start/steer on its collector without reactivating the goal. Start/resume/replace/active edits use its native continuation collector with fresh invocation context; there is no artificial kickoff. No implicit interrupt. Inspect retained status after queued admission. A known-unsent outcome may retry this same frozen intent after checking readiness, without another human confirmation. Native acknowledgment is separate from work incorporation. Lost ACK and partial effects are never replayed."});
}

/// The existing owner socket supplies authentication and the human scope.
/// Client JSON cannot name another human or act as a live daimon execution.
pub(super) async fn owner_command(app: &App, human: i64, args: &str) -> Result<Value> {
    if args == "delegation" {
        return app.shared.store.goal_delegation_read(human);
    }
    if let Some(json) = args.strip_prefix("delegation set ") {
        let update: crate::store::GoalDelegationUpdate = serde_json::from_str(json)?;
        return app.shared.store.goal_delegation_update(human, &update);
    }
    if let Some(selected) = args.strip_prefix("inspect ") {
        let fields: Vec<_> = selected.split_whitespace().collect();
        if fields.len() != 2 {
            bail!("Expected goal inspect CHAT TOPIC");
        }
        return super::goals::inspect_target(
            app,
            human,
            SessionKey::new(fields[0].parse()?, Some(fields[1].parse()?)),
        )
        .await;
    }
    bail!("Expected goal delegation | delegation set JSON | inspect CHAT TOPIC");
}

pub(super) async fn tool(
    app: &App,
    key: SessionKey,
    human: i64,
    request: &crate::codex::DynamicToolRequest,
    operation: &str,
    args: Value,
    closed: &CancellationToken,
) -> Result<Value> {
    let integrator = app
        .shared
        .config
        .interaction
        .as_ref()
        .is_some_and(|cfg| cfg.human_id == human && cfg.entry() == key);
    if !integrator
        && !app
            .shared
            .store
            .goal_delegation_allows_actor(human, key, &request.thread_id)?
    {
        bail!("This native execution has no effective goal delegation");
    }
    let local = app
        .workers
        .lock()
        .await
        .get(&key)
        .and_then(|w| {
            w.steer
                .lock()
                .expect("steer mutex poisoned")
                .as_ref()
                .map(|s| s.turn_id)
        })
        .context("No current integrating controller execution")?;
    let actor = LiveTaskActor {
        key,
        human,
        local,
        native: &request.thread_id,
        physical: &request.turn_id,
        integrator,
    };
    match operation {
        "goal_prepare" => {
            let args: Prepare = serde_json::from_value(args)?;
            let selector = args
                .selector
                .strip_prefix("/self#")
                .or_else(|| args.selector.strip_prefix('#'))
                .unwrap_or(&args.selector);
            let targets = if integrator {
                super::routing::profile(&app.shared)?.resolve(selector)
            } else {
                app.shared
                    .store
                    .goal_delegated_targets(human, key, selector)?
            };
            if targets.len() != 1 {
                return Ok(json!({"kind":"clarification_required","candidates":targets}));
            }
            let target = targets[0];
            let native = app
                .shared
                .store
                .get_session(target)?
                .and_then(|s| s.codex_thread_id)
                .context("Selected native context unavailable")?;
            app.shared
                .store
                .validate_owned_binding(target, human, &native)?;
            // Refuse a stale interpretation; this read is not the atomic write
            // guard. The native conditional RPC enforces the same revision later.
            let observed: NativeGoalSnapshot = app.shared.codex.get_goal_snapshot(&native).await?;
            if observed.revision != args.plan.expected.revision
                || !crate::codex::goal_definitions_match(
                    observed.goal.as_ref(),
                    args.plan.expected.goal.as_ref(),
                )
            {
                bail!(
                    "Goal changed after inspection; discuss the current definition before a new intent"
                );
            }
            let (value, fresh) = app.shared.store.goal_control_prepare(
                &actor,
                target,
                &args.target_binding,
                args.input_ref.as_ref(),
                &args.plan,
                &args.reason,
                closed,
            )?;
            Ok(
                json!({"kind":"goal_control","new_intent":fresh,"control":value,"native_mutation":false}),
            )
        }
        "goal_decide" => {
            let args: Decision = serde_json::from_value(args)?;
            let (value, fresh) = app.shared.store.goal_control_decide(
                &actor,
                &args.id,
                &args.input_ref,
                args.execute,
                closed,
            )?;
            Ok(
                json!({"kind":"goal_control","new_decision":fresh,"control":value,"native_mutation":false}),
            )
        }
        "goal_status" => {
            let args: Selection = serde_json::from_value(args)?;
            app.shared.store.goal_control_read(human, key, &args.id)
        }
        "goal_execute" => {
            let args: Selection = serde_json::from_value(args)?;
            let retained = app.shared.store.goal_control_read(human, key, &args.id)?;
            if !matches!(
                retained["phase"].as_str(),
                Some("prepared" | "known_unsent" | "retained_unsent")
            ) {
                return Ok(retained);
            }
            let plan: GoalPlan = serde_json::from_value(retained["intent"]["plan"].clone())?;
            let activating = matches!(
                plan.action,
                GoalAction::Start | GoalAction::Resume | GoalAction::Replace
            ) || (matches!(plan.action, GoalAction::Edit)
                && plan
                    .expected
                    .goal
                    .as_ref()
                    .is_some_and(|g| g.status == "active"));
            let target: SessionKey = serde_json::from_value(retained["intent"]["target"].clone())?;
            let target_session = app
                .shared
                .store
                .get_session(target)?
                .context("Selected target unavailable before submission")?;
            let (value, fresh) = app
                .shared
                .store
                .goal_control_claim(&actor, &args.id, closed)?;
            if !fresh {
                return Ok(value);
            }
            let attempt = value["intent"]["attempt"]
                .as_str()
                .context("Retained goal attempt missing")?
                .to_owned();
            if activating || plan.followup.is_some() {
                let result =
                    activate_control(app, human, key, &args.id, &value, &plan, &target_session)
                        .await;
                super::topic_publications::wake(app.shared.clone(), target);
                return result;
            }
            let native = value["intent"]["target_binding"][2]
                .as_str()
                .context("Retained native target missing")?;
            let owned_app = app.clone();
            let control_id = args.id.clone();
            let admission_attempt = attempt.clone();
            let actor_native = actor.native.to_owned();
            let actor_physical = actor.physical.to_owned();
            let deadline = closed.clone();
            let admission: crate::codex::SteerAdmissionCheck = std::sync::Arc::new(move || {
                let actor = LiveTaskActor {
                    key,
                    human,
                    local,
                    native: &actor_native,
                    physical: &actor_physical,
                    integrator,
                };
                owned_app.shared.store.goal_control_validate_submission(
                    &actor,
                    &control_id,
                    &admission_attempt,
                    &deadline,
                )
            });
            let receipt_slot = std::sync::Arc::new(std::sync::Mutex::new(None));
            let control = crate::codex::GoalControl::ConditionalMetadata(Box::new(
                crate::codex::GoalMetadataControl {
                    origin: mutation_origin(&value),
                    plan: plan.clone(),
                    admission: admission.clone(),
                    receipt: receipt_slot.clone(),
                    context: None,
                    scope: None,
                    settled: None,
                },
            ));
            let (phase,receipt)=match super::goals::control_live(app,&target_session,control).await {
                Ok(true)=>receipt_slot.lock().expect("goal control receipt mutex poisoned").clone().unwrap_or_else(||("uncertain".into(),json!({"error":"Subscriber returned without a matched goal receipt","automatic_retry":false}))),
                Ok(false)=>{
                    if app.workers.lock().await.contains_key(&target) {
                        ("known_unsent".into(),json!({"error":"Selected writer is preparing; no independent goal mutation submitted","automatic_retry":false}))
                    }else{app.shared.codex.conditional_goal_metadata(native,&plan,&admission,mutation_origin(&value)).await}
                },
                Err(_)=>("uncertain".into(),json!({"error":"Selected writer goal-control acknowledgment unavailable","automatic_retry":false})),
            };
            app.shared
                .store
                .goal_control_finish(human, key, &args.id, &attempt, &phase, &receipt)?;
            super::topic_publications::wake(app.shared.clone(), target);
            app.shared.store.goal_control_read(human, key, &args.id)
        }
        _ => bail!("Unsupported goal-control operation"),
    }
}

fn mutation_origin(value: &Value) -> crate::codex::GoalMutationOrigin {
    if value["intent"]["delegation"].is_object() {
        crate::codex::GoalMutationOrigin::Automatic
    } else {
        crate::codex::GoalMutationOrigin::User
    }
}

async fn activate_control(
    app: &App,
    human: i64,
    entry: SessionKey,
    id: &str,
    value: &Value,
    plan: &GoalPlan,
    target: &crate::models::SessionRecord,
) -> Result<Value> {
    let native = value["intent"]["target_binding"][2]
        .as_str()
        .context("Retained native target unavailable")?
        .to_owned();
    let attempt = value["intent"]["attempt"]
        .as_str()
        .context("Retained goal attempt unavailable")?
        .to_owned();
    let admission_attempt = attempt.clone();
    let shared = app.shared.clone();
    let selected = id.to_owned();
    let admission: crate::codex::SteerAdmissionCheck = std::sync::Arc::new(move || {
        shared.store.goal_control_begin_native_submission(
            human,
            entry,
            &selected,
            &admission_attempt,
        )
    });
    let shared = app.shared.clone();
    let selected = id.to_owned();
    let settlement_attempt = attempt.clone();
    let settled_target = target.key;
    let settled = std::sync::Arc::new(move |phase: &str, receipt: &Value| {
        shared.store.goal_control_finish(
            human,
            entry,
            &selected,
            &settlement_attempt,
            phase,
            receipt,
        )?;
        super::topic_publications::wake(shared.clone(), settled_target);
        Ok(())
    });
    let slot = std::sync::Arc::new(std::sync::Mutex::new(None));
    let mut control = crate::codex::GoalMetadataControl {
        origin: mutation_origin(value),
        plan: plan.clone(),
        admission,
        receipt: slot.clone(),
        context: None,
        scope: Some(crate::codex::GoalControlScope {
            human,
            entry,
            id: id.to_owned(),
            attempt: attempt.clone(),
        }),
        settled: Some(settled),
    };
    let attached = app
        .workers
        .lock()
        .await
        .get(&target.key)
        .and_then(|w| w.steer.lock().expect("steer mutex poisoned").clone());
    let activation = |control: crate::codex::GoalMetadataControl| crate::codex::GoalActivation {
        intent: id.to_owned(),
        native_thread: Some(native.clone()),
        expected: plan.expected.goal.clone(),
        objective: plan.objective.clone(),
        replace: matches!(plan.action, GoalAction::Replace),
        attach_turn: None,
        conditional: Some(Box::new(control)),
    };
    if let Some(attached) = attached {
        let Some(hint) = attached.controller_hint else {
            app.shared.store.goal_control_finish(human,entry,id,&attempt,"known_unsent",&json!({"error":"Selected native writer has no current invocation lease","automatic_retry":false}))?;
            return app.shared.store.goal_control_read(human, entry, id);
        };
        app.shared.store.goal_control_link_execution(
            human,
            entry,
            id,
            &attempt,
            attached.turn_id,
            &native,
        )?;
        control.context = Some(json!({"telecodex.controller":{"kind":"application","value":hint}}));
        // Send through the exact collector whose invocation lease was selected.
        // Re-resolving the worker here could change its local turn in flight.
        let (response, ack) = oneshot::channel();
        let sent = attached.sender.send(crate::codex::CodexSteerRequest {
            observation: None,
            images: vec![],
            admission_check: None,
            client_message_id: None,
            text: String::new(),
            goal: Some(crate::codex::GoalControl::Activate(Box::new(activation(
                control,
            )))),
            response,
        });
        if sent.is_err() {
            app.shared.store.goal_control_finish(human,entry,id,&attempt,"known_unsent",&json!({"error":"Selected collector closed before goal control admission","automatic_retry":false}))?;
        } else {
            // The collector owns settlement even after this bounded observation
            // ends. A timeout cannot authorize another goal submission.
            if let Ok(result) = tokio::time::timeout(Duration::from_secs(18), ack).await
                && !matches!(result, Ok(Ok(_)))
            {
                app.shared
                    .store
                    .goal_control_retire_execution(human, entry, id, &attempt)?;
            }
        }
    } else {
        if app
            .workers
            .lock()
            .await
            .get(&target.key)
            .is_some_and(|w| w.cancel.lock().expect("cancel mutex poisoned").is_some())
        {
            app.shared.store.goal_control_finish(human,entry,id,&attempt,"known_unsent",&json!({"error":"Selected writer is still preparing; no goal mutation submitted","automatic_retry":false}))?;
            return app.shared.store.goal_control_read(human, entry, id);
        }
        let epoch = super::stopping::input_epoch(&app.shared, target.key)?;
        let handle = app.worker_for(target.key).await?;
        // This is a retained goal control, not a second human message/update.
        if handle.sender.send(QueuedTurn{
            request:TurnRequest{session_key:target.key,from_user_id:human,prompt:"Continue the explicitly authorized native goal. Use its native state and accounting.".into(),runtime_instructions:None,attachments:vec![],review_mode:None,override_search_mode:None},
            chat_kind:if target.key.chat_id<0{"supergroup"}else{"private"}.into(),update_id:None,conversation_input:None,tribu_admission:None,goal:Some(activation(control)),nexus:None,merge:None,stop_resume:None,interaction:None,admission_epoch:epoch,
        }).is_err() {
            app.shared.store.goal_control_finish(human,entry,id,&attempt,"known_unsent",&json!({"error":"Selected collector admission closed before queuing","automatic_retry":false}))?;
        }
    }
    app.shared.store.goal_control_read(human, entry, id)
}

/// Retirement follows the actual collector lifetime, never an observation timeout.
pub(super) struct GoalExecutionGuard {
    shared: Arc<AppShared>,
    scope: Option<crate::codex::GoalControlScope>,
}
impl GoalExecutionGuard {
    pub(super) fn new(shared: Arc<AppShared>, goal: Option<&crate::codex::GoalActivation>) -> Self {
        Self {
            shared,
            scope: goal
                .and_then(|g| g.conditional.as_ref())
                .and_then(|c| c.scope.clone()),
        }
    }
}
impl Drop for GoalExecutionGuard {
    fn drop(&mut self) {
        let Some(scope) = &self.scope else {
            return;
        };
        if self
            .shared
            .store
            .goal_control_retire_execution(scope.human, scope.entry, &scope.id, &scope.attempt)
            .is_err()
        {
            tracing::warn!("Goal collector retirement outcome could not be retained");
            return;
        }
        if tokio::runtime::Handle::try_current().is_ok()
            && super::topic_publications::recover(self.shared.clone()).is_err()
        {
            tracing::warn!("Goal retirement notice retained without a publication wake");
        }
    }
}

/// A narrow facade for an actual native work execution. It does not grant the
/// routing, Matrix, original-input or global controls of an integrating entry.
pub(super) fn delegated_call(
    app: App,
    key: SessionKey,
    human: i64,
    request: crate::codex::DynamicToolRequest,
    operation: String,
    args: Value,
    closed: CancellationToken,
) -> oneshot::Receiver<Value> {
    let (send, recv) = oneshot::channel();
    tokio::spawn(async move {
        let result:Result<Value>=async {
            if closed.is_cancelled() || request.namespace.is_some() || args.to_string().len()>16000 ||
                !app.shared.store.goal_delegation_allows_actor(human,key,&request.thread_id)? {
                bail!("Goal delegation is unavailable for this execution");
            }
            match operation.as_str() {
                "capabilities" if args==json!({}) => {
                    let mut view=json!({"schema":"telecodex.delegated-goals/v1","operations":{},"scope":"retained human delegation; current owned native sessions only","native_origin":"automatic"});
                    delegated_capabilities(&mut view);
                    Ok(view)
                }
                "goal_inspect" => {
                    #[derive(Deserialize)]
                    #[serde(deny_unknown_fields)]
                    struct Inspect { selector:String }
                    let selected:Inspect=serde_json::from_value(args)?;
                    let selector=selected.selector.strip_prefix('#').unwrap_or(&selected.selector);
                    let targets=app.shared.store.goal_delegated_targets(human,key,selector)?;
                    if targets.len()!=1 {return Ok(json!({"kind":"clarification_required","candidates":targets,"read_only":true}));}
                    super::goals::inspect_target(&app,human,targets[0]).await
                }
                "goal_prepare"|"goal_status"|"goal_execute" => tool(&app,key,human,&request,&operation,args,&closed).await,
                _=>bail!("Unsupported delegated goal operation"),
            }
        }.await;
        let result = match result {
            Ok(value) => crate::codex::dynamic_tool_result(true, value.to_string()),
            Err(error) => crate::codex::dynamic_tool_result(
                false,
                format!(
                    "Goal control was not confirmed: {error}. Inspect retained status before retrying; uncertain work is not replayable."
                ),
            ),
        };
        let _ = send.send(result);
    });
    recv
}

pub(super) fn delegated_capabilities(view: &mut Value) {
    capabilities(view);
    view["operations"]
        .as_object_mut()
        .expect("operations")
        .remove("goal_decide");
    view["operations"]["goal_inspect"] = json!({"arguments":{"selector":"self, exact owned session title or stable tID; ambiguous matches require clarification"},"description":"Inspect current native goal snapshot/revision and actual target binding without mutation."});
    view["operations"]["goal_prepare"]["description"] = json!(
        "Prepare edit objective, pause or resume under the current retained human delegation. Omit input_ref for a daimon action, retain the exact inspected target_binding and native snapshot, and give a bounded reason. Budget changes, destructive actions and invented human follow-up are outside this delegation. The receiver records actual native actor and native origin=automatic. Preparation does not execute; use goal_execute, then inspect status after uncertain admission."
    );
}
