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
    value["operations"]["goal_prepare"] = json!({"arguments":{"selector":"one enrolled target","target_binding":"exact goal_inspect target_binding","input_ref":"actual retained explicit human command reference, omitted ONLY for an assistant proposal","plan":{"action":"start|edit|pause|resume|clear|replace","expected":"exact native {goal,revision} from goal_inspect","objective":"optional selected objective","token_budget":"optional expressly requested budget","followup":"optional exact follow-up clause of the human instruction"},"reason":"bounded explanation"},"description":"Retain one immutable goal-control interpretation. A supplied original must actually request this control. Omitted original means assistant proposal requiring later human confirmation. Retention never mutates the goal or starts work. Do not dispatch the same original separately; ordered follow-up belongs to this intent."});
    value["operations"]["goal_decide"] = json!({"arguments":{"id":"retained proposal","input_ref":"actual later captured human confirmation/cancellation","execute":"true only when this human input explicitly accepts the exact proposal"},"description":"Single-use proposal decision; actual capture must postdate proposal creation. No native mutation. Stale goal definitions are rejected at execution, never silently refreshed."});
    value["operations"]["goal_status"] = json!({"arguments":{"id":"retained goal-control ID"},"description":"Inspect immutable interpretation and outcome; never replay uncertain work."});
    value["operations"]["goal_execute"] = json!({"arguments":{"id":"prepared explicit or confirmed goal-control ID"},"description":"One conditional native metadata write for pause, clear or edit of an inactive goal. No interrupt. Active goal changes require the owning continuation collector; currently refused without a native mutation. Native acknowledgment is separate from work incorporation. Lost ACK stays uncertain and is never retried automatically."});
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
    let cfg = super::routing::profile(&app.shared)?;
    if cfg.human_id != human || cfg.entry() != key {
        bail!("Goal controls require the owned integrating entry");
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
        integrator: true,
    };
    match operation {
        "goal_prepare" => {
            let args: Prepare = serde_json::from_value(args)?;
            let selector = args
                .selector
                .strip_prefix("/self#")
                .or_else(|| args.selector.strip_prefix('#'))
                .unwrap_or(&args.selector);
            let targets = cfg.resolve(selector);
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
            let plan: GoalPlan = serde_json::from_value(retained["intent"]["plan"].clone())?;
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
                bail!(
                    "Active goal change requires the owning continuation collector; no mutation submitted"
                );
            }
            if plan.followup.is_some() {
                bail!(
                    "Ordered follow-up admission must be connected before this intent executes; no mutation submitted"
                );
            }
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
            let native = value["intent"]["target_binding"][2]
                .as_str()
                .context("Retained native target missing")?;
            let owned_app = app.clone();
            let control_id = args.id.clone();
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
                    integrator: true,
                };
                owned_app.shared.store.goal_control_validate_submission(
                    &actor,
                    &control_id,
                    &deadline,
                )
            });
            let receipt_slot = std::sync::Arc::new(std::sync::Mutex::new(None));
            let control = crate::codex::GoalControl::ConditionalMetadata(Box::new(
                crate::codex::GoalMetadataControl {
                    plan: plan.clone(),
                    admission: admission.clone(),
                    receipt: receipt_slot.clone(),
                },
            ));
            let (phase,receipt)=match super::goals::control_live(app,&target_session,control).await {
                Ok(true)=>receipt_slot.lock().expect("goal control receipt mutex poisoned").clone().unwrap_or_else(||("uncertain".into(),json!({"error":"Subscriber returned without a matched goal receipt","automatic_retry":false}))),
                Ok(false)=>{
                    if app.workers.lock().await.contains_key(&target) {
                        ("known_unsent".into(),json!({"error":"Selected writer is preparing; no independent goal mutation submitted","automatic_retry":false}))
                    }else{app.shared.codex.conditional_goal_metadata(native,&plan,&admission).await}
                },
                Err(_)=>("uncertain".into(),json!({"error":"Selected writer goal-control acknowledgment unavailable","automatic_retry":false})),
            };
            app.shared
                .store
                .goal_control_finish(human, key, &args.id, &phase, &receipt)?;
            app.shared.store.goal_control_read(human, key, &args.id)
        }
        _ => bail!("Unsupported goal-control operation"),
    }
}
