//! Native failure evidence for the shared goal service, independent of ingress.
//!
//! This ledger observes; it never authorizes a control or retries model work.
use super::*;
use crate::codex::NativeGoal;
use anyhow::bail;
use serde_json::{Value, json};

/// Deliberately narrow: neither message text nor `blocked` establishes capacity.
fn capacity_error(error: &Value) -> bool {
    error.get("codexErrorInfo").and_then(Value::as_str) == Some("serverOverloaded")
}

impl Store {
    pub(super) fn init_goal_recovery(&self) -> Result<()> {
        self.conn
            .lock()
            .expect("store mutex poisoned")
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS goal_failure_observations(
                session_id INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
                native_thread TEXT NOT NULL,
                native_turn TEXT NOT NULL,
                blocked_goal TEXT,
                terminal_error TEXT,
                observed_at TEXT NOT NULL,
                PRIMARY KEY(session_id,native_thread,native_turn));",
            )?;
        Ok(())
    }

    /// Retain the goal notification's actual turn association, never infer it
    /// from a nearby failure or attach a historical block to the latest turn.
    pub fn goal_recovery_block_observed(
        &self,
        key: SessionKey,
        human: i64,
        native: &str,
        turn: &str,
        goal: &NativeGoal,
    ) -> Result<()> {
        if goal.thread_id != native || goal.status != "blocked" {
            bail!("Recovery block evidence must identify this blocked native goal");
        }
        self.goal_recovery_observe(key, human, native, turn, Some(goal), None)
    }

    /// Only a terminal failed turn supplies this half of the evidence. A stream
    /// warning, lost RPC ACK, or provider message is not a terminal observation.
    pub fn goal_recovery_failure_observed(
        &self,
        key: SessionKey,
        human: i64,
        native: &str,
        turn: &str,
        error: &Value,
    ) -> Result<()> {
        if !error.is_object() || error.to_string().len() > 16384 {
            bail!("Native terminal error is not a bounded structured observation");
        }
        self.goal_recovery_observe(key, human, native, turn, None, Some(error))
    }

    fn goal_recovery_observe(
        &self,
        key: SessionKey,
        human: i64,
        native: &str,
        turn: &str,
        goal: Option<&NativeGoal>,
        error: Option<&Value>,
    ) -> Result<()> {
        if native.is_empty() || native.len() > 128 || turn.is_empty() || turn.len() > 128 {
            bail!("Native failure identity is missing or unbounded");
        }
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let (session, bound) = super::presentation::entry_on(&tx, human, key)?;
        if bound != native {
            bail!("Failure evidence belongs to a different native binding");
        }
        let goal = goal.map(serde_json::to_string).transpose()?;
        let error = error.map(Value::to_string);
        let retained: Option<(Option<String>, Option<String>)> = tx
            .query_row(
                "SELECT blocked_goal,terminal_error FROM goal_failure_observations
             WHERE session_id=?1 AND native_thread=?2 AND native_turn=?3",
                params![session, native, turn],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        if let Some((old_goal, old_error)) = &retained {
            if (old_goal.is_some() && goal.is_some() && *old_goal != goal)
                || (old_error.is_some() && error.is_some() && *old_error != error)
            {
                bail!("Conflicting native failure evidence is not a new retry authorization");
            }
            if (goal.is_none() || old_goal.is_some()) && (error.is_none() || old_error.is_some()) {
                return Ok(());
            }
        }
        tx.execute(
            "INSERT INTO goal_failure_observations
             (session_id,native_thread,native_turn,blocked_goal,terminal_error,observed_at)
             VALUES(?1,?2,?3,?4,?5,?6)
             ON CONFLICT(session_id,native_thread,native_turn) DO UPDATE SET
             blocked_goal=COALESCE(goal_failure_observations.blocked_goal,excluded.blocked_goal),
             terminal_error=COALESCE(goal_failure_observations.terminal_error,excluded.terminal_error)",
            params![session,native,turn,goal,error,now_string()],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Same current owner/binding as other presentation reads. No native RPC,
    /// inference, Telegram operation, scheduling or lifecycle control occurs.
    pub fn goal_recovery_evidence(&self, human: i64, key: SessionKey) -> Result<Value> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let (session, native) = super::presentation::entry_on(&conn, human, key)?;
        let mut query = conn.prepare(
            "SELECT native_turn,blocked_goal,terminal_error,observed_at
             FROM goal_failure_observations WHERE session_id=?1 AND native_thread=?2
             ORDER BY rowid DESC LIMIT 17",
        )?;
        let rows = query
            .query_map(params![session, native], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let items = rows
            .iter()
            .take(16)
            .map(|(turn, goal, error, observed)| {
                let goal: Option<NativeGoal> =
                    goal.as_deref().map(serde_json::from_str).transpose()?;
                let error: Option<Value> =
                    error.as_deref().map(serde_json::from_str).transpose()?;
                let linked_capacity = goal.is_some() && error.as_ref().is_some_and(capacity_error);
                let goal_view=goal.as_ref().map(|g|json!({"threadId":g.thread_id,
                    "objective_preview":g.objective.chars().take(240).collect::<String>(),
                    "objective_truncated":g.objective.chars().count()>240,"status":g.status,
                    "tokenBudget":g.token_budget,"tokensUsed":g.tokens_used,
                    "timeUsedSeconds":g.time_used_seconds,"createdAt":g.created_at,"updatedAt":g.updated_at}));
                let error_view=error.as_ref().map(|e| {
                    let message=e["message"].as_str().unwrap_or("");
                    json!({"codexErrorInfo":e.get("codexErrorInfo").filter(|v|v.to_string().len()<=512),
                        "message_preview":message.chars().take(240).collect::<String>(),
                        "message_truncated":message.chars().count()>240,"full_evidence_retained":true})
                });
                Ok(
                    json!({"native_turn":turn,"goal_at_failure":goal_view,"terminal_error":error_view,
                "observed_at":observed,"linked_capacity_failure":linked_capacity,
                "cause":if linked_capacity {"server_overloaded"}else{"unqualified"},
                "current_goal_revision":null,"delegation":null,"next_attempt_at":null,
                "execution_enabled":false}),
                )
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(
            json!({"schema":"telecodex.goal-failure-evidence/v1","items":items,
            "more":rows.len()>16,"read_only":true,"execution_enabled":false,
            "meaning":"Historical native evidence, not current goal state or permission to resume. Matching turn identities qualify the cause only; delegation, fresh revision, budgets, later intent, fences and admission remain required."}),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SearchMode;
    use tempfile::NamedTempFile;

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
    fn goal() -> NativeGoal {
        NativeGoal {
            thread_id: "native".into(),
            objective: "Preserve this task".into(),
            status: "blocked".into(),
            token_budget: Some(100),
            tokens_used: 42,
            time_used_seconds: 3,
            created_at: 1,
            updated_at: 2,
        }
    }
    fn owned(store: &Store) -> SessionKey {
        let key = SessionKey::new(100, Some(7));
        store.ensure_session(key, 100, &defaults()).unwrap();
        store.set_session_codex_thread(key, "native").unwrap();
        key
    }
    #[test]
    fn capacity_requires_structured_code_not_text_or_quota() {
        for code in [
            "rateLimitExceeded",
            "usageLimitExceeded",
            "unauthorized",
            "badRequest",
            "other",
        ] {
            assert!(!capacity_error(
                &json!({"codexErrorInfo":code,"message":"Selected model is at capacity"})
            ));
        }
        assert!(!capacity_error(
            &json!({"message":"Selected model is at capacity"})
        ));
        assert!(capacity_error(
            &json!({"codexErrorInfo":"serverOverloaded"})
        ));
    }
    #[test]
    fn ordered_or_reordered_duplicates_join_only_same_native_turn() {
        let file = NamedTempFile::new().unwrap();
        let store = Store::open(file.path(), &[100], &defaults()).unwrap();
        let key = owned(&store);
        let error = json!({"codexErrorInfo":"serverOverloaded","message":"provider capacity"});
        store
            .goal_recovery_failure_observed(key, 100, "native", "failed-a", &error)
            .unwrap();
        store
            .goal_recovery_block_observed(key, 100, "native", "failed-b", &goal())
            .unwrap();
        let first = store.goal_recovery_evidence(100, key).unwrap();
        assert!(
            first["items"]
                .as_array()
                .unwrap()
                .iter()
                .all(|v| v["linked_capacity_failure"] == false)
        );
        for _ in 0..2 {
            store
                .goal_recovery_block_observed(key, 100, "native", "failed-a", &goal())
                .unwrap();
            store
                .goal_recovery_failure_observed(key, 100, "native", "failed-b", &error)
                .unwrap();
        }
        let joined = store.goal_recovery_evidence(100, key).unwrap();
        assert_eq!(joined["items"].as_array().unwrap().len(), 2);
        assert!(
            joined["items"]
                .as_array()
                .unwrap()
                .iter()
                .all(|v| v["linked_capacity_failure"] == true && v["execution_enabled"] == false)
        );
        assert_eq!(joined["items"][0]["goal_at_failure"]["tokensUsed"], 42);
        drop(store);
        let store = Store::open(file.path(), &[100], &defaults()).unwrap();
        assert_eq!(store.goal_recovery_evidence(100, key).unwrap(), joined);
    }
    #[test]
    fn wrong_owner_rebinding_and_conflicting_evidence_never_authorize() {
        let file = NamedTempFile::new().unwrap();
        let store = Store::open(file.path(), &[100, 200], &defaults()).unwrap();
        let key = owned(&store);
        assert!(
            store
                .goal_recovery_block_observed(key, 200, "native", "failed", &goal())
                .is_err()
        );
        assert!(
            store
                .goal_recovery_block_observed(key, 100, "other", "failed", &goal())
                .is_err()
        );
        store
            .goal_recovery_block_observed(key, 100, "native", "failed", &goal())
            .unwrap();
        let mut changed = goal();
        changed.objective = "A different goal".into();
        assert!(
            store
                .goal_recovery_block_observed(key, 100, "native", "failed", &changed)
                .is_err()
        );
        store.set_session_codex_thread(key, "new-native").unwrap();
        assert!(
            store
                .goal_recovery_failure_observed(
                    key,
                    100,
                    "native",
                    "failed",
                    &json!({"codexErrorInfo":"serverOverloaded"})
                )
                .is_err()
        );
        assert_eq!(
            store.goal_recovery_evidence(100, key).unwrap()["items"],
            json!([])
        );
    }

    #[test]
    fn projection_is_bounded_and_explicit_session_deletion_still_works() {
        let file = NamedTempFile::new().unwrap();
        let store = Store::open(file.path(), &[100], &defaults()).unwrap();
        let key = owned(&store);
        let mut long = goal();
        long.objective = "A long retained objective ".repeat(200);
        let error = json!({"codexErrorInfo":"serverOverloaded","message":"Long provider explanation ".repeat(200),"additionalDetails":"PRIVATE PROVIDER DETAILS"});
        for i in 0..18 {
            let turn = format!("failed-{i}");
            store
                .goal_recovery_block_observed(key, 100, "native", &turn, &long)
                .unwrap();
            store
                .goal_recovery_failure_observed(key, 100, "native", &turn, &error)
                .unwrap();
        }
        let projection = store.goal_recovery_evidence(100, key).unwrap();
        assert_eq!(projection["items"].as_array().unwrap().len(), 16);
        assert_eq!(projection["more"], true);
        assert_eq!(
            projection["items"][0]["goal_at_failure"]["objective_truncated"],
            true
        );
        assert_eq!(
            projection["items"][0]["terminal_error"]["message_truncated"],
            true
        );
        assert!(!projection.to_string().contains("PRIVATE PROVIDER DETAILS"));
        store.delete_session(key).unwrap();
        assert!(store.get_session(key).unwrap().is_none());
        assert_eq!(
            store
                .conn
                .lock()
                .unwrap()
                .query_row("SELECT COUNT(*) FROM goal_failure_observations", [], |r| {
                    r.get::<_, i64>(0)
                })
                .unwrap(),
            0
        );
    }
}
