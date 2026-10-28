//! Bounded native-return metadata lives beside its existing canonical source.
//! It does not declare a workflow or create another task/input/queue.
use super::*;

const MAX_ROUTED_METADATA_BYTES: usize = 16 * 1024;
const MAX_QUESTION_VERSIONS: usize = 4;
const RETURN_SECONDS: i64 = 900;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RoutedParent {
    pub human: i64,
    pub entry: SessionKey,
    pub native: String,
    pub binding: String,
    pub epoch: String,
    pub original: InputReference,
    pub channel: String,
    pub original_revision: String,
    pub source: String,
    pub expires_at: String,
    pub returns: Vec<WorkflowReturn>,
}

fn location(source: &str) -> Result<(&'static str, &str)> {
    if let Some(id) = source.strip_prefix("delivery:") {
        Ok(("interaction_deliveries", id))
    } else if let Some(id) = source.strip_prefix("question:") {
        Ok(("human_interactions", id))
    } else {
        bail!("Unknown routed return source")
    }
}
pub(super) fn load_source(conn: &Connection, source: &str) -> Result<Option<ReturnContext>> {
    let (table, id) = location(source)?;
    let raw: Option<String> = conn
        .query_row(
            &format!("SELECT native_return FROM {table} WHERE id=?1"),
            [id],
            |r| r.get(0),
        )
        .optional()?
        .flatten();
    raw.map(|raw| {
        let parent: RoutedParent = serde_json::from_str(&raw)?;
        anyhow::ensure!(
            parent.source == source && parent.returns.len() <= MAX_QUESTION_VERSIONS,
            "Routed return binding changed"
        );
        Ok(ReturnContext::routed(parent))
    })
    .transpose()
}
pub(super) fn load(conn: &Connection, id: &str) -> Result<Option<(ReturnContext, usize)>> {
    let source = if let Some(id) = id.strip_prefix("result:") {
        Some(format!("delivery:{id}"))
    } else {
        id.strip_prefix("question:")
            .and_then(|s| s.rsplit_once(':'))
            .map(|(id, _)| format!("question:{id}"))
    };
    let Some(source) = source else {
        return Ok(None);
    };
    let context = load_source(conn, &source)?.context("Routed return receipt missing")?;
    let index = context
        .native_returns
        .iter()
        .position(|r| r.id == id)
        .context("Routed return identity missing")?;
    Ok(Some((context, index)))
}
pub(super) fn save(
    conn: &Connection,
    parent: &RoutedParent,
    returns: &[WorkflowReturn],
) -> Result<()> {
    let (table, id) = location(&parent.source)?;
    let mut next = parent.clone();
    next.returns = returns.to_vec();
    let raw = serde_json::to_string(&next)?;
    anyhow::ensure!(
        raw.len() <= MAX_ROUTED_METADATA_BYTES && returns.len() <= MAX_QUESTION_VERSIONS,
        "Routed return metadata capacity reached"
    );
    anyhow::ensure!(
        conn.execute(
            &format!("UPDATE {table} SET native_return=?2 WHERE id=?1"),
            params![id, raw]
        )? == 1,
        "Routed source disappeared"
    );
    Ok(())
}

/// Use the same visible canonical result and the same owned question reader.
/// Carrier phase, artifact availability and playback are not source revisions.
fn evidence(
    conn: &Connection,
    human: i64,
    entry: SessionKey,
    source: &str,
) -> Result<Option<Value>> {
    if source.starts_with("delivery:") {
        let Some(value) = super::super::super::presentation::source_on(conn, human, entry, source)?
        else {
            return Ok(None);
        };
        if value["kind"] != "worker_result"
            || value["completed"] != true
            || value["current"] != true
            || value["correction_pending"] != false
            || value["native_binding_current"] != true
        {
            return Ok(None);
        }
        let mut frozen = json!({"kind":"canonical_result","id":source,"result":value["result_id"],"target":value["target"],"requests_revision":digest(&value["request_ids"])?,"text_sha256":value["text_sha256"],"entry_binding":value["entry_binding"],"native":value["result_native"],"completed":true,"current":true});
        frozen["revision"] = json!(digest(&frozen)?);
        frozen["preview"] = json!(
            value["text"]
                .as_str()
                .unwrap_or("")
                .chars()
                .take(180)
                .collect::<String>()
        );
        Ok(Some(frozen))
    } else if let Some(id) = source.strip_prefix("question:") {
        let question =
            match super::super::super::conversation_requests::read_on(conn, human, entry, id) {
                Ok(question) => question,
                Err(error)
                    if error
                        .downcast_ref::<rusqlite::Error>()
                        .is_some_and(|e| matches!(e, rusqlite::Error::QueryReturnedNoRows)) =>
                {
                    return Ok(None);
                }
                Err(error) => return Err(error),
            };
        let running: bool = conn.query_row(
            "SELECT status='running' FROM turns WHERE id=?1",
            [question.item.local_turn],
            |r| r.get(0),
        )?;
        if question.key == entry
            || question.item.state != "pending"
            || !running
            || question.item.source["kind"] != "question"
        {
            return Ok(None);
        }
        Ok(Some(
            json!({"kind":"native_question","id":source,"revision":question.capture_version,"target":question.key,"local_turn":question.item.local_turn,"native":question.item.native_thread,"preview":question.item.title.chars().take(180).collect::<String>(),"current":true}),
        ))
    } else {
        Ok(None)
    }
}
pub(super) fn current(
    conn: &Connection,
    parent: &RoutedParent,
    receipt: &WorkflowReturn,
    admission: bool,
) -> Result<bool> {
    let (session, native) =
        super::super::super::presentation::entry_on(conn, parent.human, parent.entry)?;
    if native != parent.native
        || super::super::super::presentation::entry_binding(
            conn,
            parent.human,
            parent.entry,
            session,
        )? != parent.binding
        || super::super::super::stopping::admission_epoch_on(conn, parent.entry)? != parent.epoch
        || super::super::super::stopping::fenced_on(conn, parent.entry)?
        || super::super::super::maintenance_writer_held_on(conn, parent.entry)?
        || chrono::DateTime::parse_from_rfc3339(&parent.expires_at)? <= Utc::now()
        || admission && receipt.reported_revision.is_some()
    {
        return Ok(false);
    }
    let (original, _) = super::super::super::dialogue::original_owned_on(
        conn,
        parent.entry,
        parent.human,
        &parent.native,
        &parent.original,
        &parent.channel,
        false,
    )?;
    if digest(&original)? != parent.original_revision {
        return Ok(false);
    }
    Ok(evidence(conn, parent.human, parent.entry, &parent.source)?
        .is_some_and(|value| value == receipt.envelope["source"]))
}

/// Select only an accepted actual-human instruction, not a helper/workflow leg.
fn original_for(
    conn: &Connection,
    human: i64,
    entry: SessionKey,
    source: &str,
) -> Result<Option<Instruction>> {
    let selected = if let Some(delivery) = source.strip_prefix("delivery:") {
        let raw: String = conn.query_row(
            "SELECT doc FROM interaction_deliveries WHERE id=?1 AND human_id=?2",
            params![delivery, human],
            |r| r.get(0),
        )?;
        let d: Delivery = serde_json::from_str(&raw)?;
        let mut selected = None;
        for id in d.covered.iter().take(64) {
            let i = instruction_on(conn, human, id)?;
            let internal: bool = conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM interaction_session_authors WHERE input_id=?1)",
                [&i.input_id],
                |r| r.get(0),
            )?;
            if !internal
                && i.origin.chat_id == entry.chat_id
                && i.origin.thread_id == entry.thread_id
                && i.phase == crate::interaction::TransportPhase::Accepted
            {
                selected = Some(i);
                break;
            }
        }
        selected
    } else if let Some(id) = source.strip_prefix("question:") {
        let question = super::super::super::conversation_requests::read_on(conn, human, entry, id)?;
        let raw: Option<String> = conn.query_row("SELECT i.doc FROM interaction_instructions i JOIN interaction_sources x ON x.id=i.input_id WHERE x.human_id=?1 AND x.origin_chat=?2 AND x.origin_thread=?3 AND i.phase='accepted' AND NOT EXISTS(SELECT 1 FROM interaction_session_authors a WHERE a.input_id=i.input_id) AND json_extract(i.doc,'$.local_turn')=?4 ORDER BY i.sequence DESC LIMIT 1",params![human,entry.chat_id,entry.thread_id,question.item.local_turn],|r|r.get(0)).optional()?;
        raw.map(|raw| serde_json::from_str(&raw)).transpose()?
    } else {
        None
    };
    let Some(i) = selected else {
        return Ok(None);
    };
    let internal: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM interaction_session_authors WHERE input_id=?1)",
        [&i.input_id],
        |r| r.get(0),
    )?;
    Ok((!internal).then_some(i))
}
fn reserve(
    conn: &Connection,
    human: i64,
    entry: SessionKey,
    source: &str,
    instance: &str,
) -> Result<Option<ReturnDispatch>> {
    let Some(source_value) = evidence(conn, human, entry, source)? else {
        return Ok(None);
    };
    let Some(instruction) = original_for(conn, human, entry, source)? else {
        return Ok(None);
    };
    let mut context = if let Some(context) = load_source(conn, source)? {
        context
    } else {
        let original: SourceInput = conn
            .query_row(
                "SELECT doc FROM interaction_sources WHERE id=?1 AND human_id=?2",
                params![instruction.input_id, human],
                |r| r.get::<_, String>(0),
            )
            .map(|raw| serde_json::from_str(&raw))??;
        let reference = original
            .update_id
            .map(InputReference::Telegram)
            .unwrap_or_else(|| InputReference::Conversation(original.event_id.clone()));
        let (session, native) = super::super::super::presentation::entry_on(conn, human, entry)?;
        let (retained, _) = super::super::super::dialogue::original_owned_on(
            conn,
            entry,
            human,
            &native,
            &reference,
            &original.channel,
            false,
        )?;
        ReturnContext::routed(RoutedParent {
            human,
            entry,
            native,
            binding: super::super::super::presentation::entry_binding(conn, human, entry, session)?,
            epoch: super::super::super::stopping::admission_epoch_on(conn, entry)?,
            original: reference,
            channel: original.channel,
            original_revision: digest(&retained)?,
            source: source.into(),
            expires_at: (Utc::now() + chrono::Duration::seconds(RETURN_SECONDS)).to_rfc3339(),
            returns: vec![],
        })
    };
    let return_id = if let Some(id) = source.strip_prefix("delivery:") {
        format!("result:{id}")
    } else {
        format!(
            "{source}:{}",
            source_value["revision"]
                .as_str()
                .context("Question revision missing")?
        )
    };
    if context.native_returns.iter().any(|r| r.id == return_id)
        || context.native_returns.len() >= MAX_QUESTION_VERSIONS
    {
        return Ok(None);
    }
    let reason = if source.starts_with("delivery:") {
        "result_available"
    } else {
        "human_decision_required"
    };
    let expires = match &context.parent {
        ReturnParent::Routed(parent) => parent.expires_at.clone(),
        _ => unreachable!(),
    };
    let mut envelope = json!({"source":source_value,"reason":reason,"slot":0,"expires_at":expires,"internal_evidence":true,"publication_grant":false});
    envelope["revision"] = json!(digest(&envelope)?);
    anyhow::ensure!(
        serde_json::to_vec(&envelope)?.len() <= MAX_ENVELOPE_BYTES,
        "Routed envelope exceeds bound"
    );
    let receipt = WorkflowReturn {
        id: return_id.clone(),
        workflow: String::new(),
        envelope,
        phase: "reserved".into(),
        local: None,
        collection_local: None,
        native_turn: None,
        instance: instance.into(),
        attempts: 1,
        retry_after: None,
        reported_revision: None,
    };
    context.native_returns.push(receipt);
    let index = context.native_returns.len() - 1;
    if !current_on(conn, &context, index, true)? {
        return Ok(None);
    }
    save_on(conn, &context)?;
    Ok(Some(ReturnDispatch {
        name: context.name(),
        id: return_id,
        attempts: 1,
        human,
        entry,
        native: context.native.clone(),
        epoch: context.epoch.clone(),
        output: context.output(index),
    }))
}

pub(super) fn retained(
    conn: &Connection,
    human: Option<i64>,
    entry: Option<SessionKey>,
    pending_only: bool,
) -> Result<Vec<ReturnContext>> {
    let mut contexts = vec![];
    for (table, owner) in [
        ("interaction_deliveries", "human_id"),
        ("human_interactions", "requester"),
    ] {
        let mut query=conn.prepare(&format!("SELECT native_return FROM {table} WHERE native_return IS NOT NULL AND EXISTS(SELECT 1 FROM json_each(native_return,'$.returns') r WHERE json_extract(r.value,'$.phase') IN ('reserved','submitting','retained_unsent','accepted_uncollected')) AND (?1 IS NULL OR {owner}=?1) AND (?2 IS NULL OR (json_extract(native_return,'$.entry.chat_id')=?2 AND json_extract(native_return,'$.entry.thread_id')=?3)) AND (?4=0 OR (julianday(json_extract(native_return,'$.expires_at'))>julianday('now') AND EXISTS(SELECT 1 FROM json_each(native_return,'$.returns') r WHERE json_extract(r.value,'$.phase')='retained_unsent'))) ORDER BY rowid LIMIT CASE WHEN ?4 THEN 16 ELSE 256 END"))?;
        for raw in query.query_map(
            params![
                human,
                entry.map(|e| e.chat_id),
                entry.map(|e| e.thread_id),
                pending_only
            ],
            |r| r.get::<_, String>(0),
        )? {
            contexts.push(ReturnContext::routed(serde_json::from_str(&raw?)?));
        }
    }
    Ok(contexts)
}

impl Store {
    pub(in crate::store) fn init_native_routed_returns(&self) -> Result<()> {
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        for (table, owner) in [
            ("interaction_deliveries", "human_id"),
            ("human_interactions", "requester"),
        ] {
            add_column_if_missing(&conn, table, "native_return", "TEXT")?;
            conn.execute_batch(&format!("CREATE INDEX IF NOT EXISTS native_return_{table}_scope ON {table}({owner},json_extract(native_return,'$.entry.chat_id'),json_extract(native_return,'$.entry.thread_id'),json_extract(native_return,'$.expires_at')) WHERE native_return IS NOT NULL;"))?;
        }
        let tx = conn.transaction()?;
        for mut context in retained(&tx, None, None, false)? {
            for r in &mut context.native_returns {
                if r.instance != self.instance_id {
                    r.phase = match r.phase.as_str() {
                        "reserved" => "retained_unsent",
                        "submitting" => "uncertain",
                        other => other,
                    }
                    .into();
                }
                if r.phase == "retained_unsent" && r.retry_after.is_none() {
                    r.retry_after =
                        retry_delay(r.attempts).map(|delay| Utc::now().timestamp() + delay);
                }
            }
            for index in 0..context.native_returns.len() {
                if context.native_returns[index].phase == "accepted_uncollected" {
                    detached_on(&tx, &mut context, index)?;
                }
            }
            save_on(&tx, &context)?;
        }
        tx.commit()?;
        Ok(())
    }
    pub(crate) fn native_routed_result_reserve(
        &self,
        human: i64,
        entry: SessionKey,
        delivery: &str,
    ) -> Result<Option<ReturnDispatch>> {
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction()?;
        let result = reserve(
            &tx,
            human,
            entry,
            &format!("delivery:{delivery}"),
            &self.instance_id,
        )?;
        tx.commit()?;
        Ok(result)
    }
    pub(crate) fn native_routed_questions_reserve(
        &self,
        human: i64,
        entry: SessionKey,
        local: i64,
    ) -> Result<Vec<ReturnDispatch>> {
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction()?;
        let ids = {
            let mut q=tx.prepare("SELECT id FROM human_interactions WHERE requester=?1 AND kind='native' AND state='pending' AND local_turn=?2 AND json_extract(payload_json,'$.source.kind')='question' ORDER BY rowid LIMIT 8")?;
            q.query_map(params![human, local], |r| r.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        let mut selected = vec![];
        for id in ids {
            if let Some(value) = reserve(
                &tx,
                human,
                entry,
                &format!("question:{id}"),
                &self.instance_id,
            )? {
                selected.push(value);
            }
        }
        tx.commit()?;
        Ok(selected)
    }
    pub(crate) fn native_routed_source(
        &self,
        human: i64,
        entry: SessionKey,
        native: &str,
        id: &str,
        revision: &str,
        offset: usize,
    ) -> Result<Value> {
        anyhow::ensure!(offset <= 1_000_000, "Source offset exceeds bound");
        let conn = self.conn.lock().expect("store mutex poisoned");
        let (context, index) = selected_on(&conn, id)?;
        anyhow::ensure!(
            context.human == human
                && context.entry == entry
                && context.native == native
                && current_on(&conn, &context, index, false)?,
            "Routed source no longer current for this entry"
        );
        let r = &context.native_returns[index];
        anyhow::ensure!(
            r.envelope["revision"].as_str() == Some(revision),
            "Routed source revision changed"
        );
        let ReturnParent::Routed(parent) = &context.parent else {
            bail!("Use workflow_result for this retained workflow source");
        };
        let text = if parent.source.starts_with("delivery:") {
            super::super::super::presentation::source_on(&conn, human, entry, &parent.source)?
                .context("Source unavailable")?["text"]
                .as_str()
                .context("Source text unavailable")?
                .to_owned()
        } else {
            let question = super::super::super::conversation_requests::read_on(
                &conn,
                human,
                entry,
                parent.source.trim_start_matches("question:"),
            )?;
            let index = question.item.source["conversationIndex"]
                .as_u64()
                .unwrap_or(0) as usize;
            serde_json::to_string(&question.item.source["request"]["questions"][index])?
        };
        let chars = text.chars().count();
        anyhow::ensure!(
            offset < chars || offset == 0 && chars == 0,
            "Source offset outside body"
        );
        let part: String = text.chars().skip(offset).take(1600).collect();
        let end = offset + part.chars().count();
        Ok(
            json!({"return_id":id,"revision":revision,"source":r.envelope["source"],"text":part,"offset":offset,"next_offset":if end<chars {Some(end)}else{None},"internal_evidence":true,"publication_grant":false,"answer_allowed":false}),
        )
    }
    pub(crate) fn native_routed_reported(
        &self,
        human: i64,
        entry: SessionKey,
        native: &str,
        id: &str,
        revision: &str,
    ) -> Result<()> {
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction()?;
        let (mut context, index) = selected_on(&tx, id)?;
        anyhow::ensure!(
            matches!(&context.parent, ReturnParent::Routed(_))
                && context.human == human
                && context.entry == entry
                && context.native == native
                && current_on(&tx, &context, index, false)?
                && context.native_returns[index].envelope["revision"].as_str() == Some(revision),
            "Routed incorporation source changed"
        );
        context.native_returns[index].reported_revision = Some(revision.into());
        save_on(&tx, &context)?;
        tx.commit()?;
        Ok(())
    }
}
