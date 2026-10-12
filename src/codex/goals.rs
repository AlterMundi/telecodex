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
    pub fn same_definition(&self, other: &Self) -> bool {
        self.thread_id == other.thread_id
            && self.objective == other.objective
            && self.status == other.status
            && self.token_budget == other.token_budget
            && self.created_at == other.created_at
    }
    pub fn edited_status(&self) -> &str {
        match self.status.as_str() {
            "complete" | "budgetLimited" => "active",
            other => other,
        }
    }
}

#[derive(Debug, Clone)]
pub struct GoalActivation {
    pub intent: String,
    pub native_thread: Option<String>,
    pub expected: Option<NativeGoal>,
    pub objective: Option<String>,
    pub replace: bool,
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
    let id = process
        .send_request("thread/goal/get", json!({"threadId":thread_id}))
        .await?;
    let current = parse_goal(&process.await_response(id).await?, thread_id)?;
    if !super::goal_definitions_match(current.as_ref(), activation.expected.as_ref()) {
        bail!("The goal changed; inspect /goal again.");
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
    Activate(GoalActivation),
    Pause,
    Stop(Arc<std::sync::Mutex<GoalStopSnapshot>>),
    Clear,
}
pub(super) async fn mutate_control(
    process: &mut AppServerProcess,
    thread_id: &str,
    control: &GoalControl,
) -> Result<Option<NativeGoal>> {
    match control {
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
