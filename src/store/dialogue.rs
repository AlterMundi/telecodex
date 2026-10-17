//! Original committed entry inputs, not model-authored messages or new authority.
use super::*;
use anyhow::bail;

pub(super) fn entry_inputs_on(
    conn: &Connection,
    key: SessionKey,
    human: i64,
    native: &str,
    selected: Option<i64>,
) -> Result<Vec<crate::telegram::Update>> {
    entry_inputs_query(conn, key, human, native, selected, false)
}

fn entry_inputs_query(
    conn: &Connection,
    key: SessionKey,
    human: i64,
    native: &str,
    selected: Option<i64>,
    committed: bool,
) -> Result<Vec<crate::telegram::Update>> {
    let mut query=conn.prepare("SELECT i.payload_json FROM incoming_updates i JOIN turns t ON t.id=i.turn_id JOIN sessions s ON s.id=t.session_id JOIN users u ON u.tg_user_id=s.creator_user_id JOIN bot_state captured ON captured.key='stop_input_epoch:'||i.update_id LEFT JOIN bot_state current ON current.key='stop_admission_epoch:'||s.creator_user_id WHERE s.chat_id=?1 AND s.thread_id=?2 AND s.creator_user_id=?3 AND u.allowed=1 AND t.from_user_id=?3 AND s.codex_thread_id=?4 AND t.native_thread_id=?4 AND (i.status IN ('started','settled','steering') OR (?6=1 AND i.status='undetermined' AND EXISTS(SELECT 1 FROM interaction_creations c WHERE c.human_id=?3 AND c.entry_chat=?1 AND c.entry_topic=?2 AND c.input_update=i.update_id))) AND json_extract(i.payload_json,'$.message.from.id')=?3 AND json_extract(i.payload_json,'$.message.chat.id')=?1 AND json_extract(i.payload_json,'$.message.message_thread_id')=?2 AND (json_type(i.payload_json,'$.message.text')='text' OR EXISTS(SELECT 1 FROM interaction_voice_inputs v WHERE v.update_id=i.update_id AND v.human_id=?3 AND v.phase='committed')) AND captured.value=COALESCE(current.value,'initial') AND (?5 IS NULL OR i.update_id=?5) ORDER BY i.update_id DESC LIMIT 12")?;
    let rows = query.query_map(
        params![
            key.chat_id,
            key.thread_id,
            human,
            native,
            selected,
            committed
        ],
        |r| r.get::<_, String>(0),
    )?;
    rows.map(|row| Ok(serde_json::from_str(&row?)?)).collect()
}

use crate::interaction::{InputReference, ReturnRoute, SourceInput};

impl rusqlite::ToSql for InputReference {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        match self {
            Self::Telegram(id) => id.to_sql(),
            Self::Conversation(id) => id.to_sql(),
        }
    }
}

pub(super) fn original_on(
    conn: &Connection,
    key: SessionKey,
    human: i64,
    native: &str,
    reference: &InputReference,
    channel: &str,
    committed: bool,
) -> Result<(SourceInput, Option<crate::telegram::Message>)> {
    reference.validate()?;
    match reference {
        InputReference::Telegram(update) => {
            let original = entry_inputs_query(conn, key, human, native, Some(*update), committed)?
                .pop()
                .context("Original Telegram input unavailable")?;
            let (text, transcription) = super::voice::content_on(conn, &original, human)?;
            let message = original.message.context("Original message missing")?;
            let origin = ReturnRoute {
                chat_id: message.chat.id,
                thread_id: message.message_thread_id.unwrap_or(0),
                message_id: Some(message.message_id),
            };
            Ok((
                SourceInput {
                    transcription,
                    channel: channel.into(),
                    event_id: update.to_string(),
                    update_id: Some(*update),
                    human_id: human,
                    target_sessions: vec![],
                    origin,
                    text,
                },
                Some(message),
            ))
        }
        InputReference::Conversation(id) => {
            let input = super::conversation_inputs::read_on(conn, id, human)?;
            super::conversation_inputs::eligible_on(conn, &input)?;
            let linked:bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM turns WHERE id=?1 AND session_id=?2 AND from_user_id=?3 AND native_thread_id=?4)",params![input.local_turn,input.session,human,native],|r|r.get(0))?;
            let retained_creation=committed && input.phase=="uncertain" && conn.query_row("SELECT EXISTS(SELECT 1 FROM interaction_creations WHERE human_id=?1 AND entry_chat=?2 AND entry_topic=?3 AND input_update=?4)",params![human,key.chat_id,key.thread_id,id],|r|r.get::<_,bool>(0))?;
            if input.entry != key
                || input.native != native
                || !linked
                || !(matches!(input.phase.as_str(), "sending" | "steering" | "accepted")
                    || retained_creation)
            {
                bail!("Original conversation input is not linked to this native entry");
            }
            Ok((
                SourceInput {
                    transcription: input.voice.as_ref().and_then(|v| v.transcription.clone()),
                    channel: "conversation".into(),
                    event_id: id.clone(),
                    update_id: None,
                    human_id: human,
                    target_sessions: vec![],
                    origin: ReturnRoute {
                        chat_id: key.chat_id,
                        thread_id: key.thread_id,
                        message_id: None,
                    },
                    text: input.text,
                },
                None,
            ))
        }
    }
}

pub(super) fn source_reference(source: &SourceInput) -> Option<InputReference> {
    source.update_id.map(InputReference::Telegram).or_else(|| {
        (source.channel == "conversation")
            .then(|| InputReference::Conversation(source.event_id.clone()))
    })
}
pub(super) fn original_order_on(conn: &Connection, source: &SourceInput) -> Result<Option<i64>> {
    let Some(reference) = source_reference(source) else {
        return Ok(None);
    };
    let channel = if reference.update().is_some() {
        "telegram"
    } else {
        "conversation"
    };
    Ok(conn
        .query_row(
            "SELECT id FROM original_ingress_order WHERE channel=?1 AND event_id=?2",
            params![channel, reference.event()],
            |r| r.get(0),
        )
        .optional()?)
}

impl Store {
    /// Return only inputs actually linked to this owned native integrating entry.
    /// Retired generations and uncertain inputs cannot become automatic dispatch.
    #[cfg(test)]
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

    pub fn interaction_entry_original(
        &self,
        key: SessionKey,
        human: i64,
        native: &str,
        reference: &InputReference,
        channel: &str,
    ) -> Result<(SourceInput, Option<crate::telegram::Message>)> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        original_on(&conn, key, human, native, reference, channel, false)
    }

    pub fn interaction_entry_references(
        &self,
        key: SessionKey,
        human: i64,
        native: &str,
    ) -> Result<Vec<InputReference>> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let mut refs = entry_inputs_on(&conn, key, human, native, None)?
            .into_iter()
            .map(|u| InputReference::Telegram(u.update_id))
            .collect::<Vec<_>>();
        let mut query=conn.prepare("SELECT id FROM conversation_inputs WHERE human_id=?1 AND native_thread=?2 AND retired=0 AND phase IN ('sending','steering','accepted') ORDER BY created_at DESC LIMIT 12")?;
        let ids = query
            .query_map(params![human, native], |r| r.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        for id in ids {
            let reference = InputReference::Conversation(id);
            if original_on(&conn, key, human, native, &reference, "", false).is_ok() {
                refs.push(reference);
            }
        }
        Ok(refs)
    }

    pub fn interaction_original_allowed(&self, id: &str, human: i64) -> Result<bool> {
        let source = self.interaction_source(id, human)?;
        if source.channel != "conversation" {
            return Ok(true);
        }
        let conn = self.conn.lock().expect("store mutex poisoned");
        let input = super::conversation_inputs::read_on(&conn, &source.event_id, human)?;
        Ok(super::conversation_inputs::eligible_on(&conn, &input).is_ok())
    }

    pub fn interaction_source_order(&self, source: &SourceInput) -> Result<Option<i64>> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        original_order_on(&conn, source)
    }

    pub fn interaction_original_provenance(
        &self,
        id: &str,
        human: i64,
    ) -> Result<serde_json::Value> {
        let source = self.interaction_source(id, human)?;
        let conn = self.conn.lock().expect("store mutex poisoned");
        Ok(
            serde_json::json!({"input_ref":source_reference(&source),"capture_order":original_order_on(&conn,&source)?}),
        )
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
        let reference = source
            .update_id
            .map(InputReference::Telegram)
            .unwrap_or_else(|| InputReference::Conversation(source.event_id.clone()));
        let (mut original, _) =
            original_on(&tx, key, human, native, &reference, &source.channel, false)?;
        original.target_sessions = source.target_sessions.clone();
        if original != *source
            || plan.len() != source.target_sessions.len()
            || plan.is_empty()
            || plan
                .iter()
                .map(|(key, _)| *key)
                .collect::<std::collections::HashSet<_>>()
                .len()
                != plan.len()
        {
            bail!("Routing plan does not match original entry input");
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
        rows.map(|row|{let(doc,phase,source)=row?;let i:crate::interaction::Instruction=serde_json::from_str(&doc)?;let source:crate::interaction::SourceInput=serde_json::from_str(&source)?;let capture=original_order_on(&conn,&source)?;let reference=source_reference(&source);let author=super::session_requests::author_on(&conn,&i.input_id)?;Ok(json!({"author":author,"instruction":i.id,"requests":i.requests,"relation":i.relation,"transport_phase":phase,"input_ref":reference,"capture_order":capture,"source_channel":source.channel,"transcription":source.transcription,"original_ingress_update":source.update_id,"source_message":source.origin,"human_text_excerpt":if author.is_none() && source.transcription.is_none(){Some(i.text.chars().take(4000).collect::<String>())}else{None},"attributed_text_excerpt":i.text.chars().take(4000).collect::<String>(),"excerpt_bounded":true}))}).collect::<Result<Vec<Value>>>()
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
            transcription: None,
            channel: "telegram:fixture".into(),
            event_id: "20".into(),
            update_id: Some(20),
            human_id: 100,
            target_sessions: targets.iter().map(|(_, id)| *id).collect(),
            origin: ReturnRoute {
                chat_id: 100,
                thread_id: 1,
                message_id: Some(20),
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
