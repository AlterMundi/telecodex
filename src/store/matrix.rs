//! Retained cross-body correlation. Matrix owns transport and receiving execution;
//! this store neither signs execution approvals nor schedules/activates a peer.
use super::*;
use crate::interaction::{InputReference, MatrixDestination, RequestRelation};
use anyhow::bail;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

fn digest(value: &Value) -> String {
    format!("{:x}", Sha256::digest(value.to_string().as_bytes()))
}
fn canonical_uuid(value: &str) -> bool {
    Uuid::parse_str(value).is_ok_and(|id| id.to_string() == value)
}
fn hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|v| v.is_ascii_digit() || (b'a'..=b'f').contains(&v))
}

/// One selected original cannot also acquire a local/creation destination plan.
/// Called inside the owning admission transaction, never as a preflight alone.
pub(super) fn unclaimed_on(
    conn: &Connection,
    human: i64,
    channel: &str,
    event: &str,
    update: Option<i64>,
) -> Result<()> {
    let held = selected_on(conn, human, channel, event, update)?.is_some();
    if held {
        bail!("Original input has an immutable Matrix destination");
    }
    Ok(())
}

// Telegram update identity is local to this store, independent of mutable bot
// labels or a creation preflight's deliberately absent presentation channel.
fn selected_on(
    conn: &Connection,
    human: i64,
    channel: &str,
    event: &str,
    update: Option<i64>,
) -> Result<Option<String>> {
    Ok(if let Some(update) = update {
        conn.query_row(
            "SELECT id FROM interaction_matrix_requests WHERE human_id=?1 AND telegram_update=?2",
            params![human, update],
            |r| r.get(0),
        )
        .optional()?
    } else {
        conn.query_row("SELECT id FROM interaction_matrix_requests WHERE human_id=?1 AND source_channel=?2 AND source_event=?3", params![human,channel,event], |r|r.get(0)).optional()?
    })
}

fn owned_on(conn: &Connection, human: i64, entry: SessionKey, id: &str) -> Result<Value> {
    if !canonical_uuid(id) {
        bail!("Canonical Matrix request UUID required");
    }
    let (session, _) = super::presentation::entry_on(conn, human, entry)?;
    let (raw,phase,submission):(String,String,Option<String>)=conn.query_row("SELECT doc,phase,submission FROM interaction_matrix_requests WHERE id=?1 AND human_id=?2 AND entry_session=?3",params![id,human,session],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).context("Selected Matrix request unavailable")?;
    let request: Value = serde_json::from_str(&raw)?;
    let current = super::presentation::entry_binding(conn, human, entry, session)?
        == request["entry_generation"]
        && super::stopping::admission_epoch_on(conn, entry)? == request["epoch"]
        && !super::stopping::fenced_on(conn, entry)?
        && !super::maintenance_writer_held_on(conn, entry)?;
    Ok(json!({"request":request,"phase":phase,"current":current,
        "submission":submission.map(|s|serde_json::from_str::<Value>(&s)).transpose()?,
        "execution":"Not authorized by retention or transport; receiving admission remains Matrix-owned"}))
}

impl Store {
    pub(super) fn init_matrix(&self) -> Result<()> {
        self.conn.lock().expect("store mutex poisoned").execute_batch(
            "CREATE TABLE IF NOT EXISTS interaction_matrix_requests(
                id TEXT PRIMARY KEY,human_id INTEGER NOT NULL,entry_session INTEGER NOT NULL,
                source_channel TEXT NOT NULL,source_event TEXT NOT NULL,telegram_update INTEGER,doc TEXT NOT NULL,
                phase TEXT NOT NULL,submission TEXT,created_at TEXT NOT NULL,
                UNIQUE(human_id,source_channel,source_event),UNIQUE(human_id,telegram_update));
             CREATE TABLE IF NOT EXISTS interaction_matrix_results(
                message_id TEXT PRIMARY KEY,request_id TEXT NOT NULL REFERENCES interaction_matrix_requests(id),
                human_id INTEGER NOT NULL,doc TEXT NOT NULL,created_at TEXT NOT NULL);
             CREATE INDEX IF NOT EXISTS interaction_matrix_result_request ON interaction_matrix_results(request_id);"
        )?;
        Ok(())
    }

    /// Freeze selected original content, destination and return scope atomically.
    /// There is no native write or receiving model call at this boundary.
    #[allow(clippy::too_many_arguments)]
    pub fn interaction_matrix_prepare(
        &self,
        entry: SessionKey,
        human: i64,
        native: &str,
        reference: &InputReference,
        channel: &str,
        label: &str,
        destination: &MatrixDestination,
        relation: RequestRelation,
        closed: &tokio_util::sync::CancellationToken,
    ) -> Result<Value> {
        reference.validate()?;
        destination.validate()?;
        if label.is_empty()
            || label.len() > 128
            || relation.requests().len() > 4
            || relation
                .requests()
                .iter()
                .collect::<std::collections::HashSet<_>>()
                .len()
                != relation.requests().len()
            || !matches!(&relation, RequestRelation::New) && relation.requests().is_empty()
        {
            bail!("Invalid selected Matrix request relation");
        }
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction()?;
        let (session, current_native) = super::presentation::entry_on(&tx, human, entry)?;
        if current_native != native || closed.is_cancelled() {
            bail!("Integrating execution changed or closed");
        }
        let (source_channel, event) = match reference {
            InputReference::Telegram(id) => (channel.to_owned(), id.to_string()),
            InputReference::Conversation(id) => ("conversation".into(), id.clone()),
        };
        let id = Uuid::new_v5(
            &Uuid::NAMESPACE_URL,
            format!("matrix-input/v1:{human}:{source_channel}:{event}").as_bytes(),
        )
        .to_string();
        if let Some(id) = selected_on(&tx, human, &source_channel, &event, reference.update())? {
            let mut result = owned_on(&tx, human, entry, &id)?;
            if result["request"]["destination"] != serde_json::to_value(destination)?
                || result["request"]["relation"] != serde_json::to_value(&relation)?
            {
                bail!("Original Matrix destination and relation are immutable");
            }
            result["new_request"] = json!(false);
            return Ok(result);
        }
        let (source, _) =
            super::dialogue::original_on(&tx, entry, human, native, reference, channel, false)?;
        if let InputReference::Conversation(input) = reference
            && super::conversation_inputs::read_on(&tx, input, human)?
                .visual
                .is_some()
        {
            bail!("This selected Matrix adapter does not carry a visual attachment");
        }
        let local: bool = if let Some(update) = reference.update() {
            tx.query_row("SELECT EXISTS(SELECT 1 FROM interaction_sources WHERE human_id=?1 AND update_id=?2)",params![human,update],|r|r.get(0))?
        } else {
            tx.query_row("SELECT EXISTS(SELECT 1 FROM interaction_sources WHERE human_id=?1 AND channel=?2 AND event_id=?3)",params![human,source_channel,event],|r|r.get(0))?
        };
        let creating:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM interaction_creations WHERE human_id=?1 AND input_update=?2)",params![human,reference],|r|r.get(0))?;
        if local
            || creating
            || source.text.trim().is_empty()
            || source.text.len() > 16000
            || source.text.starts_with('/')
            || !source.target_sessions.is_empty()
        {
            bail!("Original is already selected or unsupported for this Matrix text route");
        }
        // Source text remains exact. Only explicitly selected same-destination
        // parents are included, never entire private session histories.
        let mut parents = vec![];
        let mut context_bytes = source.text.len();
        for parent in relation.requests() {
            let retained = owned_on(&tx, human, entry, parent)?;
            if retained["request"]["destination"] != serde_json::to_value(destination)? {
                bail!("Matrix relation cannot change recipient");
            }
            let text = retained["request"]["original"]["text"]
                .as_str()
                .context("Original Matrix parent unavailable")?;
            context_bytes += text.len();
            parents.push(
                json!({"request":parent,"binding":retained["request"]["binding"],"text":text,"input_kind":if retained["request"]["original"]["transcription"].is_null(){"human_text"}else{"committed_machine_transcript"}}),
            );
        }
        // Reserve 1024 bytes of the SDK's 16384-byte relay for fixed schema,
        // UUID, and bounded execution reference (200-byte name, 16-digit revision).
        // Check serialized UTF-8, including escaping, before consuming the input.
        let content = json!({"text":source.text,"context":parents,"input_kind":if source.transcription.is_none(){"human_text"}else{"committed_machine_transcript"}});
        if context_bytes > 24000 || content.to_string().len() > 15360 {
            bail!("Selected Matrix context exceeds the bounded package");
        }
        let generation = super::presentation::entry_binding(&tx, human, entry, session)?;
        let epoch = super::stopping::admission_epoch_on(&tx, entry)?;
        if super::stopping::fenced_on(&tx, entry)? || super::maintenance_writer_held_on(&tx, entry)?
        {
            bail!("Integrating entry is fenced");
        }
        let mut request = json!({"schema":"telecodex.matrix-request/v1","id":id,"human":human,
            "entry":entry,"entry_session":session,"entry_generation":generation,"entry_native":native,
            "input_ref":reference,"epoch":epoch,"original":source,"destination":destination,
            "label":label,"relation":relation,"parents":parents});
        request["binding"] = json!(digest(&request));
        tx.execute("INSERT INTO interaction_matrix_requests(id,human_id,entry_session,source_channel,source_event,telegram_update,doc,phase,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,'awaiting_execution',?8)",params![id,human,session,source_channel,event,reference.update(),request.to_string(),now_string()])?;
        if let InputReference::Conversation(input) = reference {
            // Consuming the original is atomic with the whole external plan.
            tx.execute("UPDATE conversation_inputs SET phase='matrix_requested' WHERE id=?1 AND human_id=?2",params![input,human])?;
        }
        if closed.is_cancelled() {
            bail!("Matrix selection closed before commitment");
        }
        let mut result = owned_on(&tx, human, entry, &id)?;
        result["new_request"] = json!(true);
        tx.commit()?;
        Ok(result)
    }

    /// Claim one selected technical send before any external effect. A lost host
    /// outcome remains submitting/uncertain and is never automatically replayed.
    pub fn interaction_matrix_claim(
        &self,
        human: i64,
        entry: SessionKey,
        id: &str,
        binding: &str,
        closed: &tokio_util::sync::CancellationToken,
    ) -> Result<bool> {
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction()?;
        let selected = owned_on(&tx, human, entry, id)?;
        if selected["request"]["binding"] != binding
            || selected["current"] != true
            || closed.is_cancelled()
        {
            bail!("Selected Matrix send changed, closed or fenced before admission");
        }
        if selected["phase"] != "awaiting_execution" {
            return Ok(false);
        }
        let changed = tx.execute("UPDATE interaction_matrix_requests SET phase='submitting' WHERE id=?1 AND phase='awaiting_execution'", [id])?;
        tx.commit()?;
        Ok(changed == 1)
    }

    pub fn interaction_matrix_uncertain(
        &self,
        human: i64,
        entry: SessionKey,
        id: &str,
    ) -> Result<()> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        owned_on(&conn, human, entry, id)?;
        // A late failed host observation must not erase a verified reply.
        conn.execute("UPDATE interaction_matrix_requests SET phase='submission_uncertain' WHERE id=?1 AND phase='submitting'", [id])?;
        Ok(())
    }

    pub fn interaction_matrix_submitted(
        &self,
        human: i64,
        entry: SessionKey,
        id: &str,
        binding: &str,
        source: &MatrixAuthor,
        value: &MatrixSubmission,
    ) -> Result<Value> {
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction()?;
        let selected = owned_on(&tx, human, entry, id)?;
        let expected_send = Uuid::new_v5(
            &Uuid::NAMESPACE_URL,
            format!("telecodex.matrix/request:{id}").as_bytes(),
        )
        .to_string();
        if selected["request"]["binding"] != binding
            || !matches!(
                selected["phase"].as_str(),
                Some("submitting" | "submission_uncertain" | "submitted" | "reply_received")
            )
            || serde_json::to_value(&value.destination)? != selected["request"]["destination"]
            || value.author != *source
            || value.send_id != expected_send
            || value.thread_id != id
            || !canonical_uuid(&value.message_id)
            || !hash(&value.message_hash)
            || !selected["submission"].is_null()
                && selected["submission"] != serde_json::to_value(value)?
        {
            bail!("Matrix host outcome differs from the claimed original and fixed source");
        }
        tx.execute("UPDATE interaction_matrix_requests SET submission=?2,phase=CASE WHEN phase='reply_received' THEN phase ELSE 'submitted' END WHERE id=?1", params![id,serde_json::to_string(value)?])?;
        tx.commit()?;
        drop(conn);
        self.interaction_matrix_status(human, entry, id)
    }

    pub fn interaction_matrix_message_status(
        &self,
        human: i64,
        entry: SessionKey,
        message: &str,
    ) -> Result<Value> {
        if !canonical_uuid(message) {
            bail!("Canonical Matrix event UUID required");
        }
        let conn = self.conn.lock().expect("store mutex poisoned");
        let id: String = conn.query_row(
            "SELECT request_id FROM interaction_matrix_results WHERE message_id=?1 AND human_id=?2",
            params![message, human],
            |r| r.get(0),
        )?;
        drop(conn);
        self.interaction_matrix_status(human, entry, &id)
    }

    pub fn interaction_matrix_status(
        &self,
        human: i64,
        entry: SessionKey,
        id: &str,
    ) -> Result<Value> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let mut result = owned_on(&conn, human, entry, id)?;
        let mut query=conn.prepare("SELECT rowid,doc FROM interaction_matrix_results WHERE request_id=?1 AND human_id=?2 ORDER BY rowid DESC LIMIT 8")?;
        let rows = query
            .query_map(params![id, human], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        result["results"] = json!(
            rows.into_iter()
                .map(|(sequence, doc)| Ok(
                    json!({"sequence":sequence,"result":serde_json::from_str::<Value>(&doc)?})
                ))
                .collect::<Result<Vec<_>>>()?
        );
        Ok(result)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MatrixAuthor {
    pub being_ref: String,
    pub embodiment_id: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MatrixSubmission {
    pub send_id: String,
    pub message_id: String,
    pub message_hash: String,
    pub thread_id: String,
    pub author: MatrixAuthor,
    pub destination: MatrixDestination,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MatrixReturn {
    pub request: String,
    pub binding: String,
    pub submission: MatrixSubmission,
    pub message_id: String,
    pub message_hash: String,
    pub author: MatrixAuthor,
    pub text: String,
}
impl Store {
    /// Trusted receiving host only, after SDK signature/parent verification.
    /// This records evidence and never admits a peer or calls a native writer.
    pub fn interaction_matrix_return(
        &self,
        human: i64,
        entry: SessionKey,
        value: &MatrixReturn,
    ) -> Result<Value> {
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction()?;
        let selected = owned_on(&tx, human, entry, &value.request)?;
        let destination: MatrixDestination =
            serde_json::from_value(selected["request"]["destination"].clone())?;
        let actual_peer = MatrixDestination::Body {
            being_ref: value.author.being_ref.clone(),
            embodiment_id: value.author.embodiment_id.clone(),
        };
        actual_peer.validate()?;
        let source = MatrixDestination::Body {
            being_ref: value.submission.author.being_ref.clone(),
            embodiment_id: value.submission.author.embodiment_id.clone(),
        };
        source.validate()?;
        if value.binding != selected["request"]["binding"]
            || value.submission.destination != destination
            || value.author.being_ref != destination.being()
            || matches!(&destination,MatrixDestination::Body{embodiment_id,..} if embodiment_id!=&value.author.embodiment_id)
            || [
                &value.message_id,
                &value.submission.send_id,
                &value.submission.message_id,
                &value.submission.thread_id,
            ]
            .iter()
            .any(|id| !canonical_uuid(id))
            || !hash(&value.message_hash)
            || !hash(&value.submission.message_hash)
            || value.text.trim().is_empty()
            || value.text.len() > 16000
        {
            bail!("Matrix return differs from the immutable selected origin and recipient");
        }
        let submission = serde_json::to_string(&value.submission)?;
        if !selected["submission"].is_null()
            && selected["submission"] != serde_json::to_value(&value.submission)?
        {
            bail!("Matrix request submission identity is immutable");
        }
        let doc = serde_json::to_string(value)?;
        if tx.execute("INSERT OR IGNORE INTO interaction_matrix_results(message_id,request_id,human_id,doc,created_at) VALUES(?1,?2,?3,?4,?5)",params![value.message_id,value.request,human,doc,now_string()])?==0 {
            let previous:String=tx.query_row("SELECT doc FROM interaction_matrix_results WHERE message_id=?",[&value.message_id],|r|r.get(0))?;
            if previous!=doc {bail!("Canonical Matrix reply identity was reused with different content");}
        }
        tx.execute(
            "UPDATE interaction_matrix_requests SET submission=?2,phase='reply_received' WHERE id=?1",
            params![value.request, submission],
        )?;
        tx.commit()?;
        drop(conn);
        self.interaction_matrix_status(human, entry, &value.request)
    }
}

/// SDK-originated canonical text, not a fabricated native turn or human input.
pub(super) fn result_on(
    conn: &Connection,
    human: i64,
    entry: SessionKey,
    id: &str,
) -> Result<Option<Value>> {
    let raw:Option<(String,String)>=conn.query_row("SELECT request_id,doc FROM interaction_matrix_results WHERE message_id=?1 AND human_id=?2",params![id,human],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
    let Some((request_id, raw)) = raw else {
        return Ok(None);
    };
    let selected = owned_on(conn, human, entry, &request_id)?;
    let result: MatrixReturn = serde_json::from_str(&raw)?;
    let session = selected["request"]["entry_session"]
        .as_i64()
        .context("Matrix source logical entry unavailable")?;
    let correction:bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM interaction_matrix_requests m JOIN json_each(m.doc,'$.relation.requests') parent WHERE m.human_id=?1 AND m.entry_session=?2 AND json_extract(m.doc,'$.relation.kind')='amends' AND parent.value=?3)",params![human,session,request_id],|r|r.get(0))?;
    Ok(Some(
        json!({"id":format!("matrix:{id}"),"kind":"matrix_result","text":result.text,
        "label":format!("#{}",selected["request"]["label"].as_str().unwrap_or("Matrix")),
        "request_id":request_id,"current":selected["current"]==true && !correction,
        "correction_pending":correction,"completed":Value::Null,
        "native_binding_current":selected["current"],"delivery_phase":"retained",
        "source_author":result.author,"matrix_message_hash":result.message_hash,
        "verification":"Receiving host reports SDK-verified event and exact reply parent; no native incorporation or task completion claimed"}),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::TurnRequest;
    use tokio_util::sync::CancellationToken;

    fn setup() -> (tempfile::NamedTempFile, Store, SessionKey) {
        let file = tempfile::NamedTempFile::new().unwrap();
        let store =
            Store::open(file.path(), &[100, 101], &super::super::tests::defaults()).unwrap();
        let entry = SessionKey::new(100, Some(1));
        store
            .ensure_session(entry, 100, &super::super::tests::defaults())
            .unwrap();
        store
            .set_session_codex_thread(entry, "entry-native")
            .unwrap();
        (file, store, entry)
    }
    fn original(store: &Store, entry: SessionKey, id: i64) -> InputReference {
        let update:crate::telegram::Update=serde_json::from_value(json!({"update_id":id,"message":{"message_id":id,"message_thread_id":entry.thread_id,"from":{"id":100,"is_bot":false,"first_name":"Human"},"chat":{"id":entry.chat_id,"type":"private"},"text":format!("The exact retained question {id}")}})).unwrap();
        store.admit_update(&update).unwrap();
        store.begin_update(id).unwrap();
        store.mark_update_queued(id).unwrap();
        let session = store.get_session(entry).unwrap().unwrap();
        let turn = store
            .record_turn_started(
                session.id,
                &TurnRequest {
                    session_key: entry,
                    from_user_id: 100,
                    prompt: "Owning entry context".into(),
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
        store.link_update_turn(id, turn).unwrap();
        InputReference::Telegram(id)
    }
    fn destination() -> MatrixDestination {
        MatrixDestination::Body {
            being_ref: format!("dm:being:v1:{}", "A".repeat(43)),
            embodiment_id: "embodiment:receiver".into(),
        }
    }
    fn prepare(store: &Store, entry: SessionKey, reference: &InputReference) -> Value {
        store
            .interaction_matrix_prepare(
                entry,
                100,
                "entry-native",
                reference,
                "telegram:fixture",
                "SelectedPeer",
                &destination(),
                RequestRelation::New,
                &CancellationToken::new(),
            )
            .unwrap()
    }
    fn reply(request: &Value) -> MatrixReturn {
        MatrixReturn {
            request: request["id"].as_str().unwrap().into(),
            binding: request["binding"].as_str().unwrap().into(),
            submission: MatrixSubmission {
                send_id: Uuid::now_v7().to_string(),
                message_id: Uuid::now_v7().to_string(),
                message_hash: "a".repeat(64),
                thread_id: Uuid::now_v7().to_string(),
                author: MatrixAuthor {
                    being_ref: format!("dm:being:v1:{}", "B".repeat(43)),
                    embodiment_id: "embodiment:source".into(),
                },
                destination: destination(),
            },
            message_id: Uuid::now_v7().to_string(),
            message_hash: "b".repeat(64),
            author: MatrixAuthor {
                being_ref: destination().being().into(),
                embodiment_id: "embodiment:receiver".into(),
            },
            text: "The actual canonical external answer".into(),
        }
    }
    #[test]
    fn technical_claim_survives_duplicates_restart_and_uncertain_outcome() {
        let (file, store, entry) = setup();
        let reference = original(&store, entry, 81);
        let selected = prepare(&store, entry, &reference);
        let id = selected["request"]["id"].as_str().unwrap();
        let binding = selected["request"]["binding"].as_str().unwrap();
        let closed = CancellationToken::new();
        closed.cancel();
        assert!(
            store
                .interaction_matrix_claim(100, entry, id, binding, &closed)
                .is_err()
        );
        let open = CancellationToken::new();
        assert!(
            store
                .interaction_matrix_claim(100, entry, id, binding, &open)
                .unwrap()
        );
        assert!(
            !store
                .interaction_matrix_claim(100, entry, id, binding, &open)
                .unwrap()
        );
        assert_eq!(
            store.interaction_matrix_status(100, entry, id).unwrap()["phase"],
            "submitting"
        );
        store.interaction_matrix_uncertain(100, entry, id).unwrap();
        drop(store);
        let reopened =
            Store::open(file.path(), &[100, 101], &super::super::tests::defaults()).unwrap();
        assert_eq!(
            reopened.interaction_matrix_status(100, entry, id).unwrap()["phase"],
            "submission_uncertain"
        );
        assert!(
            !reopened
                .interaction_matrix_claim(100, entry, id, binding, &open)
                .unwrap()
        );
    }

    #[test]
    fn technical_outcome_binds_source_and_cannot_erase_an_early_reply() {
        let (_file, store, entry) = setup();
        let selected = prepare(&store, entry, &original(&store, entry, 82));
        let id = selected["request"]["id"].as_str().unwrap();
        let binding = selected["request"]["binding"].as_str().unwrap();
        store
            .interaction_matrix_claim(100, entry, id, binding, &CancellationToken::new())
            .unwrap();
        let mut response = reply(&selected["request"]);
        response.submission.send_id = Uuid::new_v5(
            &Uuid::NAMESPACE_URL,
            format!("telecodex.matrix/request:{id}").as_bytes(),
        )
        .to_string();
        response.submission.thread_id = id.to_owned();
        let source = response.submission.author.clone();
        let mut wrong = response.submission.clone();
        wrong.author.embodiment_id = "another-body".into();
        assert!(
            store
                .interaction_matrix_submitted(100, entry, id, binding, &source, &wrong)
                .is_err()
        );
        assert!(
            store
                .interaction_matrix_submitted(
                    100,
                    entry,
                    id,
                    "changed",
                    &source,
                    &response.submission
                )
                .is_err()
        );
        let submitted = store
            .interaction_matrix_submitted(100, entry, id, binding, &source, &response.submission)
            .unwrap();
        assert_eq!(submitted["phase"], "submitted");
        store
            .interaction_matrix_return(100, entry, &response)
            .unwrap();
        store.interaction_matrix_uncertain(100, entry, id).unwrap();
        let late = store
            .interaction_matrix_submitted(100, entry, id, binding, &source, &response.submission)
            .unwrap();
        assert_eq!(late["phase"], "reply_received");
        assert_eq!(late["results"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn selected_original_return_binding_and_native_identity_are_not_model_rewrites() {
        let (_file, store, entry) = setup();
        let reference = original(&store, entry, 20);
        let result = prepare(&store, entry, &reference);
        let request = &result["request"];
        assert_eq!(
            request["original"]["text"],
            "The exact retained question 20"
        );
        assert_eq!(
            request["original"]["origin"],
            json!({"chat_id":100,"thread_id":1,"message_id":20})
        );
        assert_eq!(result["phase"], "awaiting_execution");
        assert_eq!(result["current"], true);
        assert_eq!(prepare(&store, entry, &reference)["new_request"], false);
        assert!(
            store
                .interaction_matrix_prepare(
                    entry,
                    100,
                    "entry-native",
                    &reference,
                    "telegram:fixture",
                    "Other",
                    &MatrixDestination::Being {
                        being_ref: format!("dm:being:v1:{}", "C".repeat(43))
                    },
                    RequestRelation::New,
                    &CancellationToken::new()
                )
                .is_err()
        );
        assert!(
            store
                .interaction_entry_original(
                    entry,
                    100,
                    "entry-native",
                    &reference,
                    "telegram:fixture"
                )
                .is_err()
        );
        let callback = reply(request);
        let received = store
            .interaction_matrix_return(100, entry, &callback)
            .unwrap();
        assert_eq!(received["results"][0]["result"]["text"], callback.text);
        store
            .interaction_matrix_return(100, entry, &callback)
            .unwrap();
        let mut changed = callback.clone();
        changed.text = "Different bytes under the same SDK event".into();
        assert!(
            store
                .interaction_matrix_return(100, entry, &changed)
                .is_err()
        );
        assert_eq!(
            store
                .interaction_matrix_status(100, entry, &callback.request)
                .unwrap()["results"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }
    #[test]
    fn raced_telegram_creation_and_renamed_channel_cannot_reselect_original() {
        let (_file, store, entry) = setup();
        let reference = original(&store, entry, 26);
        // Creation's preliminary read succeeds before its awaited context work.
        store
            .interaction_entry_original(entry, 100, "entry-native", &reference, "")
            .unwrap();
        let selected = prepare(&store, entry, &reference);
        // Its actual transaction must see the intervening Matrix reservation.
        assert!(
            store
                .interaction_prepare_creation(
                    entry,
                    100,
                    "entry-native",
                    reference.clone(),
                    "Other",
                    "Independent work",
                    &json!({}),
                    20,
                    &CancellationToken::new()
                )
                .is_err()
        );
        assert!(
            store
                .interaction_creation_for_input(entry, 100, reference.clone())
                .is_err()
        );
        let retained = store
            .interaction_matrix_prepare(
                entry,
                100,
                "entry-native",
                &reference,
                "telegram:renamed",
                "SelectedPeer",
                &destination(),
                RequestRelation::New,
                &CancellationToken::new(),
            )
            .unwrap();
        assert_eq!(retained["request"], selected["request"]);
        assert_eq!(retained["new_request"], false);
    }

    #[test]
    fn selection_before_native_ack_preserves_disposition_and_external_cursor() {
        use super::super::conversation_inputs::ConversationInputEvent as E;
        let (_file, store, entry) = setup();
        original(&store, entry, 27);
        let local: i64 = store
            .conn
            .lock()
            .unwrap()
            .query_row("SELECT max(id) FROM turns", [], |r| r.get(0))
            .unwrap();
        let binding = store.conversation_input_page(100, entry, 0, false).unwrap()["binding"]
            .as_str()
            .unwrap()
            .to_owned();
        let id = Uuid::now_v7().to_string();
        store
            .conversation_input_capture(100, entry, &binding, &id, "Exact browser original")
            .unwrap();
        store.conversation_input_event(100, &id, E::Queue).unwrap();
        store
            .conversation_input_event(
                100,
                &id,
                E::Begin {
                    local_turn: local,
                    steer: false,
                },
            )
            .unwrap();
        let selected = prepare(&store, entry, &InputReference::Conversation(id.clone()));
        assert_eq!(
            store
                .conversation_input_event(100, &id, E::Accepted { local_turn: local })
                .unwrap()
                .phase,
            "matrix_requested"
        );
        let callback = reply(&selected["request"]);
        store
            .interaction_matrix_return(100, entry, &callback)
            .unwrap();
        let page = store
            .interaction_presentation_page_matrix(100, entry, 0, 0, Some(0), false)
            .unwrap();
        assert_eq!(page["items"][0]["kind"], "matrix_result");
        let cursor = page["cursor"]["matrix"].as_i64().unwrap();
        assert!(cursor > 0);
        assert_eq!(
            store
                .interaction_presentation_page_matrix(100, entry, 0, 0, Some(cursor), false)
                .unwrap()["items"],
            json!([])
        );
        assert_eq!(
            store
                .interaction_presentation_page_matrix(100, entry, 0, 0, None, false)
                .unwrap()["items"],
            json!([])
        );
    }

    #[test]
    fn oversized_serialized_context_leaves_original_available() {
        use super::super::conversation_inputs::ConversationInputEvent as E;
        let (_file, store, entry) = setup();
        original(&store, entry, 28);
        let local: i64 = store
            .conn
            .lock()
            .unwrap()
            .query_row("SELECT max(id) FROM turns", [], |r| r.get(0))
            .unwrap();
        let binding = store
            .interaction_presentation_list(100, entry, 0, 0)
            .unwrap()["binding"]
            .as_str()
            .unwrap()
            .to_owned();
        let capture = |text: &str| {
            let id = Uuid::now_v7().to_string();
            store
                .conversation_input_capture(100, entry, &binding, &id, text)
                .unwrap();
            store.conversation_input_event(100, &id, E::Queue).unwrap();
            store
                .conversation_input_event(
                    100,
                    &id,
                    E::Begin {
                        local_turn: local,
                        steer: false,
                    },
                )
                .unwrap();
            store
                .conversation_input_event(100, &id, E::Accepted { local_turn: local })
                .unwrap();
            id
        };
        let first = capture(&"a".repeat(10000));
        let selected = prepare(&store, entry, &InputReference::Conversation(first));
        let second = capture(&"b".repeat(6000));
        assert!(
            store
                .interaction_matrix_prepare(
                    entry,
                    100,
                    "entry-native",
                    &InputReference::Conversation(second.clone()),
                    "telegram:fixture",
                    "SelectedPeer",
                    &destination(),
                    RequestRelation::Amends(vec![
                        selected["request"]["id"].as_str().unwrap().into()
                    ]),
                    &CancellationToken::new()
                )
                .is_err()
        );
        assert_eq!(
            store
                .conversation_input_read(100, entry, &second)
                .unwrap()
                .phase,
            "accepted"
        );
        let escaped = capture(&format!("x{}", "\t".repeat(8000)));
        assert!(
            store
                .interaction_matrix_prepare(
                    entry,
                    100,
                    "entry-native",
                    &InputReference::Conversation(escaped.clone()),
                    "telegram:fixture",
                    "SelectedPeer",
                    &destination(),
                    RequestRelation::New,
                    &CancellationToken::new()
                )
                .is_err()
        );
        assert_eq!(
            store
                .conversation_input_read(100, entry, &escaped)
                .unwrap()
                .phase,
            "accepted"
        );
        let count: i64 = store
            .conn
            .lock()
            .unwrap()
            .query_row(
                "SELECT count(*) FROM interaction_matrix_requests",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn ownership_closed_selection_and_renewal_keep_history_without_readdressing() {
        let (_file, store, entry) = setup();
        let reference = original(&store, entry, 21);
        let closed = CancellationToken::new();
        closed.cancel();
        assert!(
            store
                .interaction_matrix_prepare(
                    entry,
                    100,
                    "entry-native",
                    &reference,
                    "telegram:fixture",
                    "SelectedPeer",
                    &destination(),
                    RequestRelation::New,
                    &closed
                )
                .is_err()
        );
        let selected = prepare(&store, entry, &reference);
        let id = selected["request"]["id"].as_str().unwrap();
        assert!(store.interaction_matrix_status(101, entry, id).is_err());
        store
            .set_session_codex_thread(entry, "renewed-native")
            .unwrap();
        let retained = store
            .interaction_matrix_prepare(
                entry,
                100,
                "renewed-native",
                &reference,
                "telegram:fixture",
                "SelectedPeer",
                &destination(),
                RequestRelation::New,
                &CancellationToken::new(),
            )
            .unwrap();
        assert_eq!(retained["request"], selected["request"]);
        assert_eq!(retained["current"], false);
        assert_eq!(retained["new_request"], false);
        // A late genuine reply is retained as historical evidence, not sent to
        // a replacement native turn or declared current by the changed binding.
        let callback = reply(&selected["request"]);
        assert_eq!(
            store
                .interaction_matrix_return(100, entry, &callback)
                .unwrap()["current"],
            false
        );
    }
    #[test]
    fn local_native_admission_and_crossbody_selection_are_mutually_exclusive() {
        let (_file, store, entry) = setup();
        let reference = original(&store, entry, 22);
        let worker = SessionKey::new(100, Some(7));
        let target = store
            .ensure_session(worker, 100, &super::super::tests::defaults())
            .unwrap();
        store
            .set_session_codex_thread(worker, "worker-native")
            .unwrap();
        let (mut source, _) = store
            .interaction_entry_original(entry, 100, "entry-native", &reference, "telegram:fixture")
            .unwrap();
        source.target_sessions = vec![target.id];
        let selected = prepare(&store, entry, &reference);
        assert!(
            store
                .interaction_admit(&source, worker, RequestRelation::New)
                .is_err()
        );
        assert_eq!(
            store
                .interaction_matrix_status(100, entry, selected["request"]["id"].as_str().unwrap())
                .unwrap()["phase"],
            "awaiting_execution"
        );
        let second = original(&store, entry, 23);
        let (mut local, _) = store
            .interaction_entry_original(entry, 100, "entry-native", &second, "telegram:fixture")
            .unwrap();
        local.target_sessions = vec![target.id];
        store
            .interaction_admit(&local, worker, RequestRelation::New)
            .unwrap();
        assert!(
            store
                .interaction_matrix_prepare(
                    entry,
                    100,
                    "entry-native",
                    &second,
                    "telegram:fixture",
                    "SelectedPeer",
                    &destination(),
                    RequestRelation::New,
                    &CancellationToken::new()
                )
                .is_err()
        );
    }
    #[test]
    fn explicit_corrections_keep_selected_parent_and_recipient_without_refilling_execution() {
        let (_file, store, entry) = setup();
        let reference = original(&store, entry, 24);
        let first = prepare(&store, entry, &reference);
        let correction = original(&store, entry, 25);
        let id = first["request"]["id"].as_str().unwrap().to_owned();
        let second = store
            .interaction_matrix_prepare(
                entry,
                100,
                "entry-native",
                &correction,
                "telegram:fixture",
                "SelectedPeer",
                &destination(),
                RequestRelation::Amends(vec![id.clone()]),
                &CancellationToken::new(),
            )
            .unwrap();
        assert_ne!(second["request"]["id"], first["request"]["id"]);
        assert_eq!(second["request"]["parents"][0]["request"], id);
        assert_eq!(
            second["request"]["parents"][0]["text"],
            "The exact retained question 24"
        );
        let mut bad = reply(&second["request"]);
        bad.author.embodiment_id = "embodiment:unselected".into();
        assert!(store.interaction_matrix_return(100, entry, &bad).is_err());
        assert_eq!(
            store.interaction_matrix_status(100, entry, &id).unwrap()["phase"],
            "awaiting_execution"
        );
    }
}
