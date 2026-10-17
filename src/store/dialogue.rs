//! Original committed entry inputs, not model-authored messages or new authority.
use super::*;

fn entry_inputs_on(
    conn: &Connection,
    key: SessionKey,
    human: i64,
    native: &str,
    selected: Option<i64>,
) -> Result<Vec<crate::telegram::Update>> {
    let mut query=conn.prepare("SELECT i.payload_json FROM incoming_updates i JOIN turns t ON t.id=i.turn_id JOIN sessions s ON s.id=t.session_id JOIN users u ON u.tg_user_id=s.creator_user_id JOIN bot_state captured ON captured.key='stop_input_epoch:'||i.update_id LEFT JOIN bot_state current ON current.key='stop_admission_epoch:'||s.creator_user_id WHERE s.chat_id=?1 AND s.thread_id=?2 AND s.creator_user_id=?3 AND u.allowed=1 AND t.from_user_id=?3 AND s.codex_thread_id=?4 AND t.native_thread_id=?4 AND i.status IN ('started','settled','steering') AND json_extract(i.payload_json,'$.message.from.id')=?3 AND json_extract(i.payload_json,'$.message.chat.id')=?1 AND json_extract(i.payload_json,'$.message.message_thread_id')=?2 AND json_type(i.payload_json,'$.message.text')='text' AND captured.value=COALESCE(current.value,'initial') AND (?5 IS NULL OR i.update_id=?5) ORDER BY i.update_id DESC LIMIT 12")?;
    let rows = query.query_map(
        params![key.chat_id, key.thread_id, human, native, selected],
        |r| r.get::<_, String>(0),
    )?;
    rows.map(|row| Ok(serde_json::from_str(&row?)?)).collect()
}

impl Store {
    /// Return only inputs actually linked to this owned native integrating entry.
    /// Retired generations and uncertain inputs cannot become automatic dispatch.
    pub fn interaction_entry_inputs(
        &self,
        key: SessionKey,
        human: i64,
        native: &str,
        selected: Option<i64>,
    ) -> Result<Vec<crate::telegram::Update>> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        entry_inputs_on(&conn, key, human, native, selected)
    }

    /// Atomically retain the whole routing plan under the original entry owner,
    /// binding and ingress generation before any per-target transport begins.
    pub fn interaction_admit_entry_plan(
        &self,
        key: SessionKey,
        human: i64,
        native: &str,
        source: &crate::interaction::SourceInput,
        plan: &[(SessionKey, crate::interaction::RequestRelation)],
        closed: &tokio_util::sync::CancellationToken,
    ) -> Result<Vec<(crate::interaction::Instruction, bool)>> {
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction()?;
        let update = source
            .update_id
            .context("Missing original ingress reference")?;
        let original = entry_inputs_on(&tx, key, human, native, Some(update))?
            .pop()
            .context("Original entry input no longer eligible")?;
        let message = original.message.context("Missing original message")?;
        if source.human_id != human
            || source.event_id != update.to_string()
            || message.text.as_deref() != Some(source.text.as_str())
            || source.origin.chat_id != message.chat.id
            || source.origin.thread_id != message.message_thread_id.unwrap_or(0)
            || source.origin.message_id != message.message_id
            || plan.len() != source.target_sessions.len()
            || plan.is_empty()
            || plan
                .iter()
                .map(|(k, _)| *k)
                .collect::<std::collections::HashSet<_>>()
                .len()
                != plan.len()
        {
            return Err(anyhow!("Routing plan does not match original entry input"));
        }
        let mut admitted = vec![];
        for (target, relation) in plan {
            admitted.push(super::interaction::admit_on(
                &tx,
                source,
                *target,
                relation.clone(),
                &self.instance_id,
            )?);
        }
        if closed.is_cancelled() {
            return Err(anyhow!("Native tool closed before plan commitment"));
        }
        tx.commit()?;
        Ok(admitted)
    }

    /// Input source order is distinct from asynchronous target admission order.
    pub fn interaction_recent_target_inputs(
        &self,
        key: SessionKey,
        human: i64,
    ) -> Result<Vec<serde_json::Value>> {
        use serde_json::{Value, json};
        let conn = self.conn.lock().expect("store mutex poisoned");
        let mut query=conn.prepare("SELECT i.doc,i.phase,o.doc FROM interaction_instructions i JOIN interaction_sources o ON o.id=i.input_id JOIN sessions s ON s.id=i.session_id JOIN users u ON u.tg_user_id=s.creator_user_id WHERE s.chat_id=?1 AND s.thread_id=?2 AND s.creator_user_id=?3 AND u.allowed=1 AND o.human_id=?3 AND (json_extract(i.doc,'$.native_thread')=s.codex_thread_id OR json_extract(i.doc,'$.native_thread') IS NULL) ORDER BY i.sequence DESC LIMIT 16")?;
        let rows = query.query_map(params![key.chat_id, key.thread_id, human], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?;
        rows.map(|row|{let(doc,phase,source)=row?;let i:crate::interaction::Instruction=serde_json::from_str(&doc)?;let source:crate::interaction::SourceInput=serde_json::from_str(&source)?;Ok(json!({"instruction":i.id,"requests":i.requests,"relation":i.relation,"transport_phase":phase,"source_channel":source.channel,"original_ingress_update":source.update_id,"source_message":source.origin,"human_text_excerpt":i.text.chars().take(4000).collect::<String>(),"excerpt_bounded":true}))}).collect::<Result<Vec<Value>>>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interaction::{RequestRelation, ReturnRoute, SourceInput};
    use serde_json::json;

    #[test]
    fn original_entry_plan_is_atomic_idempotent_and_generation_scoped() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let defaults = super::super::tests::defaults();
        let store = Store::open(file.path(), &[100, 101], &defaults).unwrap();
        let entry = SessionKey::new(100, Some(1));
        let entry_record = store.ensure_session(entry, 100, &defaults).unwrap();
        store
            .set_session_codex_thread(entry, "entry-native")
            .unwrap();
        let mut targets = vec![];
        for topic in [7, 8] {
            let key = SessionKey::new(100, Some(topic));
            let s = store.ensure_session(key, 100, &defaults).unwrap();
            store
                .set_session_codex_thread(key, &format!("worker-{topic}"))
                .unwrap();
            targets.push((key, s.id));
        }
        let update:crate::telegram::Update=serde_json::from_value(json!({"update_id":20,"message":{"message_id":20,"message_thread_id":1,"from":{"id":100,"is_bot":false,"first_name":"Human"},"chat":{"id":100,"type":"private"},"text":"The exact original human input"}})).unwrap();
        store.admit_update(&update).unwrap();
        store.begin_update(20).unwrap();
        store.mark_update_queued(20).unwrap();
        let turn = store
            .record_turn_started(
                entry_record.id,
                &TurnRequest {
                    session_key: entry,
                    from_user_id: 100,
                    prompt: "Native prompt may include controller context".into(),
                    runtime_instructions: None,
                    attachments: vec![],
                    review_mode: None,
                    override_search_mode: None,
                },
            )
            .unwrap();
        store
            .record_turn_native_thread(turn, "entry-native")
            .unwrap();
        store.link_update_turn(20, turn).unwrap();
        assert_eq!(
            store
                .interaction_entry_inputs(entry, 100, "entry-native", None)
                .unwrap()
                .len(),
            1
        );
        assert!(
            store
                .interaction_entry_inputs(entry, 101, "entry-native", None)
                .unwrap()
                .is_empty()
        );
        assert!(
            store
                .interaction_entry_inputs(entry, 100, "another-native", None)
                .unwrap()
                .is_empty()
        );
        let closed = tokio_util::sync::CancellationToken::new();
        let mut source = SourceInput {
            channel: "telegram:fixture".into(),
            event_id: "20".into(),
            update_id: Some(20),
            human_id: 100,
            target_sessions: targets.iter().map(|(_, id)| *id).collect(),
            origin: ReturnRoute {
                chat_id: 100,
                thread_id: 1,
                message_id: 20,
            },
            text: "The exact original human input".into(),
        };
        let invalid = [
            (targets[0].0, RequestRelation::New),
            (
                targets[1].0,
                RequestRelation::Amends(vec!["unknown-request".into()]),
            ),
        ];
        assert!(
            store
                .interaction_admit_entry_plan(
                    entry,
                    100,
                    "entry-native",
                    &source,
                    &invalid,
                    &closed
                )
                .is_err()
        );
        let count = || {
            store
                .conn
                .lock()
                .unwrap()
                .query_row("SELECT count(*) FROM interaction_instructions", [], |r| {
                    r.get::<_, i64>(0)
                })
                .unwrap()
        };
        assert_eq!(count(), 0, "A partial multi-target plan survived rollback");
        source.text = "Model authored replacement".into();
        let valid = targets
            .iter()
            .map(|(key, _)| (*key, RequestRelation::New))
            .collect::<Vec<_>>();
        assert!(
            store
                .interaction_admit_entry_plan(entry, 100, "entry-native", &source, &valid, &closed)
                .is_err()
        );
        source.text = "The exact original human input".into();
        closed.cancel();
        assert!(
            store
                .interaction_admit_entry_plan(entry, 100, "entry-native", &source, &valid, &closed)
                .is_err()
        );
        assert_eq!(count(), 0, "A closed native tool committed new work");
        let closed = tokio_util::sync::CancellationToken::new();
        let result = store
            .interaction_admit_entry_plan(entry, 100, "entry-native", &source, &valid, &closed)
            .unwrap();
        assert!(
            result
                .iter()
                .all(|(i, fresh)| *fresh && i.text == source.text)
        );
        assert_eq!(count(), 2);
        assert!(
            store
                .interaction_admit_entry_plan(entry, 100, "entry-native", &source, &valid, &closed)
                .unwrap()
                .iter()
                .all(|(_, fresh)| !*fresh)
        );
        assert_eq!(count(), 2);
        store
            .set_session_codex_thread(entry, "replacement-native")
            .unwrap();
        assert!(
            store
                .interaction_entry_inputs(entry, 100, "entry-native", None)
                .unwrap()
                .is_empty()
        );
        assert!(
            store
                .interaction_admit_entry_plan(entry, 100, "entry-native", &source, &valid, &closed)
                .is_err()
        );
        store
            .set_session_codex_thread(entry, "entry-native")
            .unwrap();
        store
            .save_bot_state("stop_admission_epoch:100", "retired-fixture-generation")
            .unwrap();
        assert!(
            store
                .interaction_entry_inputs(entry, 100, "entry-native", None)
                .unwrap()
                .is_empty()
        );
        assert!(
            store
                .interaction_admit_entry_plan(entry, 100, "entry-native", &source, &valid, &closed)
                .is_err()
        );
        assert_eq!(count(), 2);
    }
}
