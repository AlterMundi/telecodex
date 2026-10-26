//! Safe incident projections and immutable delivery-to-service outbox. Neither
//! acceptance nor notification acknowledgment grants native recovery authority.
use super::*;
use crate::config::GoalRecoveryAlerts;
use serde_json::{Value, json};

impl Store {
    pub(super) fn init_goal_alerts(&self) -> Result<()> {
        self.conn
            .lock()
            .expect("store mutex poisoned")
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS goal_recovery_alert_outbox(
             producer TEXT NOT NULL,incident TEXT NOT NULL,revision INTEGER NOT NULL,
             payload TEXT NOT NULL,receipt TEXT,PRIMARY KEY(producer,incident,revision));",
            )?;
        Ok(())
    }

    pub(crate) fn goal_recovery_alerts_prepare(&self, settings: &GoalRecoveryAlerts) -> Result<()> {
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction()?;
        let now = Utc::now().timestamp();
        let rows = {
            let mut q=tx.prepare("SELECT context,phase FROM goal_retries WHERE human_id=?1 AND rowid IN (SELECT MAX(rowid) FROM goal_retries WHERE human_id=?1 GROUP BY json_extract(context,'$.chain.id'))")?;
            q.query_map([settings.human_id], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?
        };
        for (raw, phase) in rows {
            let context: Value = serde_json::from_str(&raw)?;
            let Some(chain) = context["chain"]["id"].as_str() else {
                continue;
            };
            let Some(source) = context["chain"]["first_observed_at"].as_i64() else {
                continue;
            };
            let prior:Option<String>=tx.query_row("SELECT payload FROM goal_recovery_alert_outbox WHERE producer=?1 AND incident=?2 AND revision=1",params![settings.producer_id,chain],|r|r.get(0)).optional()?;
            let resolved = matches!(
                phase.as_str(),
                "returned"
                    | "native_completed"
                    | "superseded"
                    | "disabled"
                    | "interrupted"
                    | "rejected"
            );
            if prior.is_none()
                && (resolved
                    || now - source < settings.after_seconds
                    || !matches!(
                        phase.as_str(),
                        "waiting" | "uncertain" | "partial" | "failed_again"
                    ))
            {
                continue;
            }
            let revision = if resolved { 2 } else { 1 };
            if prior.is_some() && !resolved {
                continue;
            }
            let mut payload = if let Some(raw) = prior {
                serde_json::from_str::<Value>(&raw)?
            } else {
                json!({"schema":"daimon-alerts/v1","producer_id":settings.producer_id,"incident_id":chain,"operation_id":chain,
                    "revision":1,"receiving_scope":settings.receiving_scope,"phase":"goal_recovery","resource_ref":settings.resource_ref,
                    "severity":"warning","problem_code":"provider_error","summary":"A native goal remains blocked or its capacity-recovery outcome remains uncertain.",
                    "action":"Inspect the retained goal recovery state. Preserve uncertain effects and deliberate pauses; notification acknowledgment does not resume work.",
                    "source_at":source,"updated_at":now,"processing_state":"open","work_preserved":true})
            };
            if resolved {
                // Alerts phase is immutable provenance, not recovery progress.
                // Retain it (including older projections) across resolution.
                payload["revision"] = json!(2);
                payload["processing_state"] = json!("resolved");
                payload["updated_at"] = json!(now);
                if matches!(phase.as_str(), "returned" | "native_completed") {
                    payload["summary"] = json!(
                        "The matched native continuation completed. Goal completion and human receipt remain separate."
                    );
                    payload["action"] = json!(
                        "Inspect the retained native result and current goal. No additional work was started by this notice."
                    );
                } else {
                    payload["summary"] = json!(
                        "The recovery attempt was retired or interrupted; this does not declare provider recovery."
                    );
                    payload["action"] = json!(
                        "Inspect the current goal and retained direction before any explicit further action."
                    );
                }
            }
            tx.execute(
                "INSERT OR IGNORE INTO goal_recovery_alert_outbox VALUES(?1,?2,?3,?4,NULL)",
                params![settings.producer_id, chain, revision, payload.to_string()],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub(crate) fn goal_recovery_alerts_due(
        &self,
        settings: &GoalRecoveryAlerts,
    ) -> Result<Vec<Value>> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        // A resolution cannot overtake an unconfirmed open acceptance. Exact
        // content replay is safe under the established Alerts upsert contract.
        let mut q=conn.prepare("SELECT payload FROM goal_recovery_alert_outbox a WHERE producer=?1 AND receipt IS NULL AND json_extract(payload,'$.receiving_scope')=?2 AND json_extract(payload,'$.resource_ref')=?3 AND NOT EXISTS(SELECT 1 FROM goal_recovery_alert_outbox old WHERE old.producer=a.producer AND old.incident=a.incident AND old.revision<a.revision AND old.receipt IS NULL) ORDER BY rowid LIMIT 4")?;
        let raw = q
            .query_map(
                params![
                    settings.producer_id,
                    settings.receiving_scope,
                    settings.resource_ref
                ],
                |r| r.get::<_, String>(0),
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        raw.iter()
            .map(|s| serde_json::from_str(s).map_err(Into::into))
            .collect()
    }

    pub(crate) fn goal_recovery_alert_accepted(
        &self,
        payload: &Value,
        receipt: &Value,
    ) -> Result<()> {
        if receipt["incident_id"] != payload["incident_id"]
            || receipt["producer_id"] != payload["producer_id"]
            || receipt["revision"] != payload["revision"]
            || receipt["schema"] != "daimon-alerts/v1"
        {
            anyhow::bail!("Alerts acceptance does not match retained projection");
        }
        let conn = self.conn.lock().expect("store mutex poisoned");
        conn.execute("UPDATE goal_recovery_alert_outbox SET receipt=?4 WHERE producer=?1 AND incident=?2 AND revision=?3 AND payload=?5 AND receipt IS NULL",params![payload["producer_id"].as_str(),payload["incident_id"].as_str(),payload["revision"].as_i64(),receipt.to_string(),payload.to_string()])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn alerts_retain_exact_content_and_order_across_restart_and_resolution() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let store = Store::open(file.path(), &[100], &super::super::tests::defaults()).unwrap();
        let key = SessionKey::new(100, Some(7));
        let session = store
            .ensure_session(key, 100, &super::super::tests::defaults())
            .unwrap();
        let settings = GoalRecoveryAlerts {
            command: vec!["synthetic-adapter".into()],
            human_id: 100,
            producer_id: "test-goal-recovery".into(),
            receiving_scope: "fictional-human".into(),
            resource_ref: "fictional-native-goals".into(),
            after_seconds: 600,
        };
        let chain = Uuid::now_v7().to_string();
        let context = json!({"chain":{"id":chain,"first_observed_at":Utc::now().timestamp()-1000}});
        store.conn.lock().unwrap().execute("INSERT INTO goal_retries(id,human_id,session_id,target_chat,target_topic,native_thread,failed_turn,goal,context,phase,next_at,attempts,created_at) VALUES('test',100,?1,100,7,'native','failed','{}',?2,'waiting',0,3,?3)",params![session.id,context.to_string(),now_string()]).unwrap();
        store.goal_recovery_alerts_prepare(&settings).unwrap();
        let open = store.goal_recovery_alerts_due(&settings).unwrap().remove(0);
        assert_eq!(open["revision"], 1);
        drop(store);
        let store = Store::open(file.path(), &[100], &super::super::tests::defaults()).unwrap();
        store.goal_recovery_alerts_prepare(&settings).unwrap();
        assert_eq!(
            store.goal_recovery_alerts_due(&settings).unwrap(),
            vec![open.clone()],
            "Lost upsert ACK must retry identical safe content"
        );
        store
            .conn
            .lock()
            .unwrap()
            .execute("UPDATE goal_retries SET phase='returned'", [])
            .unwrap();
        store.goal_recovery_alerts_prepare(&settings).unwrap();
        assert_eq!(
            store.goal_recovery_alerts_due(&settings).unwrap(),
            vec![open.clone()],
            "Unconfirmed open acceptance must precede resolution"
        );
        let receipt = json!({"schema":"daimon-alerts/v1","producer_id":settings.producer_id,"incident_id":chain,"revision":1,"accepted_at":Utc::now().timestamp(),"idempotent":false});
        store.goal_recovery_alert_accepted(&open, &receipt).unwrap();
        let resolved = store.goal_recovery_alerts_due(&settings).unwrap().remove(0);
        assert_eq!(resolved["revision"], 2);
        assert_eq!(resolved["processing_state"], "resolved");
        assert_eq!(resolved["source_at"], open["source_at"]);
        for field in [
            "producer_id",
            "incident_id",
            "operation_id",
            "receiving_scope",
            "source_at",
            "phase",
            "resource_ref",
        ] {
            assert_eq!(
                resolved[field], open[field],
                "Immutable Alerts provenance: {field}"
            );
        }
        let mut wrong = receipt.clone();
        wrong["revision"] = json!(3);
        assert!(
            store
                .goal_recovery_alert_accepted(&resolved, &wrong)
                .is_err()
        );
        let mut acknowledged = receipt;
        acknowledged["revision"] = json!(2);
        store
            .goal_recovery_alert_accepted(&resolved, &acknowledged)
            .unwrap();
        assert!(
            store
                .goal_recovery_alerts_due(&settings)
                .unwrap()
                .is_empty()
        );
        let conn = store.conn.lock().unwrap();
        for table in [
            "turns",
            "incoming_updates",
            "conversation_inputs",
            "interaction_goal_controls",
        ] {
            assert_eq!(
                conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r
                    .get::<_, i64>(0))
                    .unwrap(),
                0,
                "Alerts must not create work or speech"
            );
        }
    }
}
