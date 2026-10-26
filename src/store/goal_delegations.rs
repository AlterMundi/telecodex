//! Owner-installed human delegation, distinct from captured human speech.
//! Grant arguments never come from a model tool. Existing owner authentication
//! supplies the human scope; every revision and idempotent outcome is retained.
use super::*;
use anyhow::bail;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct GoalDelegationUpdate {
    pub request_id: String,
    pub expected_revision: Option<String>,
    pub source: String,
    pub agents: bool,
    pub recovery: bool,
    pub scope: String,
}

pub(super) fn current_on(conn: &Connection, human: i64) -> Result<Option<Value>> {
    let raw: Option<String> = conn
        .query_row(
            "SELECT doc FROM goal_delegations WHERE human_id=?1 ORDER BY rowid DESC LIMIT 1",
            [human],
            |r| r.get(0),
        )
        .optional()?;
    raw.map(|s| serde_json::from_str(&s).map_err(Into::into))
        .transpose()
}

pub(super) fn grant_on(
    conn: &Connection,
    human: i64,
    target: SessionKey,
    recovery: bool,
) -> Result<Value> {
    super::presentation::entry_on(conn, human, target)?;
    let value = current_on(conn, human)?.context("No retained human goal delegation")?;
    let flag = if recovery { "recovery" } else { "agents" };
    if value["scope"] != "owned_sessions" || value[flag] != true {
        bail!("This goal principal has no effective delegation");
    }
    Ok(value)
}

pub(super) fn validate_on(
    conn: &Connection,
    human: i64,
    target: SessionKey,
    delegation: &Value,
) -> Result<()> {
    let current = grant_on(conn, human, target, delegation["principal"] == "recovery")?;
    if current["revision"] != delegation["revision"] {
        bail!("Goal delegation changed after preparation");
    }
    Ok(())
}

impl Store {
    pub fn goal_delegated_targets(
        &self,
        human: i64,
        caller: SessionKey,
        selector: &str,
    ) -> Result<Vec<SessionKey>> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        grant_on(&conn, human, caller, false)?;
        if selector == "self" {
            return Ok(vec![caller]);
        }
        let mut query=conn.prepare("SELECT chat_id,thread_id FROM sessions WHERE creator_user_id=?1 AND force_fresh_thread=0 AND codex_thread_id IS NOT NULL AND (session_title=?2 OR 't'||thread_id=?2) ORDER BY id LIMIT 3")?;
        let rows = query
            .query_map(params![human, selector], |r| {
                Ok(SessionKey::new(r.get(0)?, Some(r.get(1)?)))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }
    pub(super) fn init_goal_delegations(&self) -> Result<()> {
        self.conn.lock().expect("store mutex poisoned").execute_batch(
            "CREATE TABLE IF NOT EXISTS goal_delegations(id TEXT PRIMARY KEY,human_id INTEGER NOT NULL,request_id TEXT NOT NULL,request TEXT NOT NULL,doc TEXT NOT NULL,UNIQUE(human_id,request_id));"
        )?;
        Ok(())
    }

    pub fn goal_delegation_read(&self, human: i64) -> Result<Value> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let allowed: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM users WHERE tg_user_id=?1 AND allowed=1)",
            [human],
            |r| r.get(0),
        )?;
        if !allowed {
            bail!("Human is not allowed on this receiver");
        }
        Ok(
            json!({"schema":"telecodex.goal-delegation/v1","delegation":current_on(&conn,human)?,"native_mutation":false}),
        )
    }

    /// Called only by the authenticated owner gateway. The request references
    /// the real human delegation; it is not a fabricated Telegram/app original.
    pub fn goal_delegation_update(
        &self,
        human: i64,
        change: &GoalDelegationUpdate,
    ) -> Result<Value> {
        if Uuid::parse_str(&change.request_id).is_err()
            || change.source.trim().is_empty()
            || change.source.len() > 2000
            || change.scope != "owned_sessions"
        {
            bail!(
                "Expected request UUID, bounded delegation source and explicit owned_sessions scope"
            );
        }
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let allowed: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM users WHERE tg_user_id=?1 AND allowed=1)",
            [human],
            |r| r.get(0),
        )?;
        if !allowed {
            bail!("Human is not allowed on this receiver");
        }
        let request = serde_json::to_string(change)?;
        let prior: Option<(String, String)> = tx
            .query_row(
                "SELECT request,doc FROM goal_delegations WHERE human_id=?1 AND request_id=?2",
                params![human, change.request_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        if let Some((old, doc)) = prior {
            if old != request {
                bail!("Delegation request is immutable");
            }
            // A retry of an older grant returns its historical outcome; it
            // never makes that old revision effective again after revocation.
            return Ok(
                json!({"delegation":serde_json::from_str::<Value>(&doc)?,"current":current_on(&tx,human)?,"new_revision":false,"native_mutation":false}),
            );
        }
        let current = current_on(&tx, human)?;
        if current.as_ref().and_then(|v| v["revision"].as_str())
            != change.expected_revision.as_deref()
        {
            bail!("Goal delegation revision changed; inspect before updating");
        }
        let revision = Uuid::now_v7().to_string();
        let value = json!({"revision":revision,"human":human,"source":change.source,"scope":change.scope,
            "agents":change.agents,"recovery":change.recovery,"created_at":now_string(),
            "installed_by":"authenticated_owner_control","request_id":change.request_id,
            "agent_principal":"receiver_verified_owned_native_execution","recovery_principal":"telecodex.goal-recovery",
            "agent_actions":["edit_objective","pause","resume"],"budget_changes":false,"destructive_actions":false});
        tx.execute("INSERT INTO goal_delegations(id,human_id,request_id,request,doc) VALUES(?1,?2,?3,?4,?5)",params![revision,human,change.request_id,request,value.to_string()])?;
        tx.commit()?;
        Ok(json!({"delegation":value,"current":value,"new_revision":true,"native_mutation":false}))
    }

    pub fn goal_delegation_allows_actor(
        &self,
        human: i64,
        key: SessionKey,
        native: &str,
    ) -> Result<bool> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let Ok((_, bound)) = super::presentation::entry_on(&conn, human, key) else {
            return Ok(false);
        };
        Ok(bound == native && grant_on(&conn, human, key, false).is_ok())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        codex::{NativeGoal, NativeGoalSnapshot},
        config::SearchMode,
    };
    use tokio_util::sync::CancellationToken;

    fn defaults() -> SessionDefaults {
        SessionDefaults {
            cwd: "/tmp".into(),
            model: None,
            reasoning_effort: None,
            session_prompt: None,
            sandbox_mode: "read-only".into(),
            approval_policy: "never".into(),
            search_mode: SearchMode::Disabled,
            add_dirs: vec![],
        }
    }
    fn grant(expected: Option<&str>, agents: bool) -> GoalDelegationUpdate {
        GoalDelegationUpdate {
            request_id: Uuid::now_v7().to_string(),
            expected_revision: expected.map(str::to_owned),
            source: "https://example.test/human-delegation".into(),
            agents,
            recovery: true,
            scope: "owned_sessions".into(),
        }
    }
    fn setup(store: &Store) -> (SessionKey, i64) {
        let key = SessionKey::new(100, Some(7));
        let session = store.ensure_session(key, 100, &defaults()).unwrap();
        store.set_session_codex_thread(key, "native").unwrap();
        let request = TurnRequest {
            session_key: key,
            from_user_id: 100,
            prompt: "Synthetic execution context".into(),
            runtime_instructions: None,
            attachments: vec![],
            review_mode: None,
            override_search_mode: None,
        };
        let local = store.record_turn_started(session.id, &request).unwrap();
        store.record_turn_native_thread(local, "native").unwrap();
        (key, local)
    }
    fn plan() -> GoalPlan {
        GoalPlan {
            action: GoalAction::Pause,
            expected: NativeGoalSnapshot {
                revision: "native-opaque-revision".into(),
                goal: Some(NativeGoal {
                    thread_id: "native".into(),
                    objective: "Keep this objective".into(),
                    status: "active".into(),
                    token_budget: Some(100),
                    tokens_used: 42,
                    time_used_seconds: 3,
                    created_at: 1,
                    updated_at: 2,
                }),
            },
            objective: None,
            token_budget: None,
            followup: None,
        }
    }

    #[test]
    fn delegation_cas_idempotency_revocation_and_restart_do_not_restore_old_grants() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let store = Store::open(file.path(), &[100, 200], &defaults()).unwrap();
        let (key, _) = setup(&store);
        assert!(
            !store
                .goal_delegation_allows_actor(100, key, "native")
                .unwrap()
        );
        let first = grant(None, true);
        let receipt = store.goal_delegation_update(100, &first).unwrap();
        let revision = receipt["delegation"]["revision"].as_str().unwrap();
        assert!(
            store
                .goal_delegation_allows_actor(100, key, "native")
                .unwrap()
        );
        assert!(
            !store
                .goal_delegation_allows_actor(200, key, "native")
                .unwrap()
        );
        assert!(
            !store
                .goal_delegation_allows_actor(100, key, "other")
                .unwrap()
        );
        assert!(
            store
                .goal_delegation_update(100, &grant(None, false))
                .is_err()
        );
        let revoke = grant(Some(revision), false);
        let revoked = store.goal_delegation_update(100, &revoke).unwrap();
        let retried = store.goal_delegation_update(100, &first).unwrap();
        assert_eq!(retried["new_revision"], false);
        assert_eq!(retried["current"], revoked["delegation"]);
        assert!(
            !store
                .goal_delegation_allows_actor(100, key, "native")
                .unwrap()
        );
        drop(store);
        let store = Store::open(file.path(), &[100, 200], &defaults()).unwrap();
        assert!(
            !store
                .goal_delegation_allows_actor(100, key, "native")
                .unwrap()
        );
        assert_eq!(
            store.goal_delegation_read(100).unwrap()["delegation"],
            revoked["delegation"]
        );
    }

    #[test]
    fn delegated_native_actor_controls_without_fabricating_human_input_and_revocation_wins() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let store = Store::open(file.path(), &[100], &defaults()).unwrap();
        let (key, local) = setup(&store);
        let actor = LiveTaskActor {
            key,
            human: 100,
            local,
            native: "native",
            physical: "actual-physical",
            integrator: false,
        };
        let binding = store.conversation_request_binding(100, key).unwrap();
        let closed = CancellationToken::new();
        assert!(
            store
                .goal_control_prepare(
                    &actor,
                    key,
                    &binding,
                    None,
                    &plan(),
                    "Pause for the delegated task",
                    &closed
                )
                .is_err()
        );
        let receipt = store
            .goal_delegation_update(100, &grant(None, true))
            .unwrap();
        let (intent, fresh) = store
            .goal_control_prepare(
                &actor,
                key,
                &binding,
                None,
                &plan(),
                "Pause for the delegated task",
                &closed,
            )
            .unwrap();
        assert!(fresh);
        assert_eq!(intent["phase"], "prepared");
        assert_eq!(intent["intent"]["proposal"], false);
        assert_eq!(intent["intent"]["original"], Value::Null);
        assert_eq!(intent["intent"]["actor"]["kind"], "daimon");
        assert_eq!(intent["intent"]["actor"]["physical"], "actual-physical");
        let id = intent["id"].as_str().unwrap();
        let (claimed, fresh) = store.goal_control_claim(&actor, id, &closed).unwrap();
        assert!(fresh);
        let attempt = claimed["intent"]["attempt"].as_str().unwrap();
        store
            .goal_control_link_execution(100, key, id, attempt, local, "native")
            .unwrap();
        let mut budget = plan();
        budget.action = GoalAction::Edit;
        budget.token_budget = Some(1000);
        assert!(
            store
                .goal_control_prepare(
                    &actor,
                    key,
                    &binding,
                    None,
                    &budget,
                    "Increase budget",
                    &closed
                )
                .is_err()
        );
        let mut destructive = plan();
        destructive.action = GoalAction::Clear;
        assert!(
            store
                .goal_control_prepare(
                    &actor,
                    key,
                    &binding,
                    None,
                    &destructive,
                    "Clear goal",
                    &closed
                )
                .is_err()
        );
        let conn = store.conn.lock().unwrap();
        for table in [
            "incoming_updates",
            "conversation_inputs",
            "original_ingress_order",
            "interaction_goal_inputs",
            "conversation_returns",
        ] {
            let n: i64 = conn
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
                .unwrap();
            assert_eq!(n, 0, "Created fabricated source/return in {table}");
        }
        drop(conn);
        store
            .goal_delegation_update(
                100,
                &grant(receipt["delegation"]["revision"].as_str(), false),
            )
            .unwrap();
        assert!(
            store
                .goal_control_begin_native_submission(100, key, id, attempt)
                .is_err()
        );
    }
}
