//! Original authenticated conversation inputs, independent of Telegram updates.
//! Capturing text is not native acceptance; uncertain transport is never replayed.
use super::*;
use anyhow::bail;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationInput {
    pub id: String,
    pub human: i64,
    pub entry: SessionKey,
    pub session: i64,
    pub native: String,
    pub epoch: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visual: Option<crate::visual::VisualSelection>,
    /// Bounded owned question versions present before capture/ASR, not device hints.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub question_versions: Option<std::collections::BTreeMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voice: Option<ConversationVoice>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply: Option<ConversationReply>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub focus: Option<ConversationFocus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub routing: Option<ConversationRouting>,
    pub phase: String,
    pub local_turn: Option<i64>,
    pub retired: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConversationVoice {
    pub audio_sha256: String,
    pub mime: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transcription: Option<crate::voice::TranscriptSource>,
}
impl ConversationVoice {
    pub fn validate(&self) -> Result<()> {
        if self.audio_sha256.len() != 64
            || !self
                .audio_sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            || !matches!(
                self.mime.as_str(),
                "audio/webm"
                    | "audio/webm;codecs=opus"
                    | "audio/ogg"
                    | "audio/ogg;codecs=opus"
                    | "audio/mp4"
                    | "audio/wav"
                    | "audio/mpeg"
            )
        {
            bail!("Invalid committed voice source");
        }
        Ok(())
    }
}
impl ConversationInput {
    pub fn rendered(&self) -> String {
        let text = format!(
            "{}{}",
            self.visual
                .as_ref()
                .map(|v| v.rendered())
                .unwrap_or_default(),
            self.answer_text()
        );
        let questions = self.question_versions.as_ref().map(|v| v.len());
        if self.reply.as_ref().is_some_and(|r| r.context_only) {
            let guidance = if self
                .reply
                .as_ref()
                .is_some_and(|r| r.source.starts_with("task:"))
            {
                "This metadata references a selected durable human task, not a live native question. Inspect interaction_controller human_task using its exact ID and current revision before interpreting feedback or retiring anything. The device hint does not prove task ownership, human completion, approval, or a destination choice. Task text is session-authored context. Discuss or route only the actual human input; if the action is obsolete, use the existing scoped withdraw_task with its inspected revision and observed reason."
            } else if self
                .reply
                .as_ref()
                .is_some_and(|r| r.source.starts_with("matrix:"))
            {
                "This metadata references a selected Matrix reply. Inspect interaction_controller matrix_status using its message_id before deciding whether this new original amends or contextualizes that request. Use the retained request ID, configured recipient and this original input_ref; never infer receiving execution approval from the reply or replay the prior question. It may instead be a new topic."
            } else if self
                .reply
                .as_ref()
                .is_some_and(|r| r.source.starts_with("input:"))
            {
                "The reported input:UUID identifies a device-reported destination question for that retained original, which may be stale. If this utterance clearly clarifies its destination, inspect pending_routes and use resolve_route with that exact pending UUID and this new input_ref. Never guess a newer pending original. If the human changes topic or the intent is unclear, discuss or route normally; context alone selects nothing."
            } else {
                "Use interaction_controller capabilities to inspect questions or answer the selected question from this original input only when its intent is clear; otherwise discuss or route normally. A stale source must not prevent a new topic."
            };
            format!(
                "[Device-reported conversational context]\n{}\nThis is contextual source metadata, not a human destination or answer selection and not proof of hearing. Interpret the original input below. {}\n\n{}",
                serde_json::json!({"input_ref":self.id,"reply":self.reply,"focus":self.focus,"questions_at_capture":questions}),
                guidance,
                text
            )
        } else {
            let guidance = if questions.is_some_and(|count| count > 0) {
                " Owned native questions existed at capture. If this could answer work, inspect current interaction_controller capabilities/questions before selecting an answer. It may instead discuss or start another topic."
            } else {
                ""
            };
            format!(
                "[Original conversation input]\n{}\nController reference for this retained original; metadata is not human speech. Use this exact input_ref for routing, creation or question interpretation, even outside the recent-input window.{}\n\n{}",
                serde_json::json!({"input_ref": self.id,"questions_at_capture":questions}),
                guidance,
                text
            )
        }
    }
    pub fn question_answer_text(&self, db: &std::path::Path) -> String {
        let Some(visual) = self.visual.as_ref() else {
            return self.answer_text();
        };
        format!(
            "{}Retained image files in attachment order: {}\nInspect these exact bytes with the image-reading tool before interpreting this answer; if unavailable, ask rather than substituting another view. This path is controller metadata, not human speech.\n\n{}",
            visual.rendered(),
            serde_json::to_string(
                &visual
                    .images()
                    .iter()
                    .map(|image| crate::visual::image_path(db, &image.image_sha256))
                    .collect::<Vec<_>>()
            )
            .expect("image path"),
            self.answer_text()
        )
    }

    /// Preserve original speech attribution without copying device focus into
    /// the native question's answer as if it were human speech.
    pub fn answer_text(&self) -> String {
        match self.voice.as_ref().and_then(|v| v.transcription.as_ref()) {
            Some(proof) => format!(
                "[Committed voice input]\nCapture: {}\nAutomatic interpretation of the original human clip, not verified written speech. Compare receiver capture order; a slow earlier transcript does not supersede later corrections.\nProvenance: {}\n\n{}",
                self.id,
                serde_json::to_string(proof).expect("typed voice proof"),
                self.text
            ),
            None => self.text.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum ConversationInputEvent {
    Queue,
    Begin { local_turn: i64, steer: bool },
    Accepted { local_turn: i64 },
    Rejected { local_turn: i64 },
    Uncertain { local_turn: i64 },
}

pub(super) fn read_on(conn: &Connection, id: &str, human: i64) -> Result<ConversationInput> {
    let (doc, phase, local_turn, retired): (String, String, Option<i64>, bool) = conn
        .query_row(
            "SELECT doc,phase,local_turn,retired FROM conversation_inputs WHERE id=?1 AND human_id=?2",
            params![id, human],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .context("Original conversation input unavailable")?;
    let mut input: ConversationInput = serde_json::from_str(&doc)?;
    input.phase = phase;
    input.local_turn = local_turn;
    input.retired = retired;
    if let Some(route) = input.routing.as_mut()
        && let Some(instruction) = &route.instruction
    {
        route.phase = conn
            .query_row(
                "SELECT phase FROM interaction_instructions WHERE id=?1",
                [instruction],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or_else(|| "unavailable".into());
    }
    Ok(input)
}

/// Historical inspection never grants admission or refreshes a voice transcript.
fn trace_original_on(
    conn: &Connection,
    human: i64,
    entry: SessionKey,
    id: &str,
) -> Result<serde_json::Value> {
    use serde_json::json;
    let (session, _) = super::presentation::entry_on(conn, human, entry)?;
    if let Some(value) = id.strip_prefix("telegram:") {
        let update_id: i64 = value.parse()?;
        if update_id < 0 || update_id.to_string() != value {
            bail!("Invalid Telegram input identity");
        }
        let (raw, phase): (String, String) = conn.query_row(
            "SELECT payload_json,status FROM incoming_updates WHERE update_id=?1",
            [update_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let update: crate::telegram::Update = serde_json::from_str(&raw)?;
        let message = update.message.context("No original message")?;
        if message
            .from
            .as_ref()
            .is_none_or(|u| u.id != human || u.is_bot)
            || message.chat.id != entry.chat_id
            || message.message_thread_id.unwrap_or(0) != entry.thread_id
        {
            bail!("Telegram original is outside this owned entry");
        }
        let (text, voice, retired) = if let Some(text) = message.text {
            (text, None, false)
        } else {
            let voice = message.voice.context("No original text or voice")?;
            let retained:Option<(String,Option<String>,Option<String>)>=conn.query_row("SELECT phase,text,source FROM interaction_voice_inputs WHERE update_id=?1 AND human_id=?2 AND session_id=?3",params![update_id,human,session],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
            let (state, text, source) = retained.unwrap_or(("not_prepared".into(), None, None));
            if let Some(raw) = source {
                let source: crate::voice::TranscriptSource = serde_json::from_str(&raw)?;
                if source.telegram_file_id.as_deref() != Some(voice.file_id.as_str()) {
                    bail!("Original voice changed");
                }
            } else if text.is_some() {
                bail!("Transcript support unavailable");
            }
            (
                text.unwrap_or_default(),
                Some(json!({"phase":state})),
                state == "retired",
            )
        };
        return Ok(
            json!({"id":id,"text":text,"voice":voice,"phase":phase,"retired":retired,"routing":null,"channel_kind":"telegram","source_event":value,"update_id":update_id}),
        );
    }
    let input = read_on(conn, id, human)?;
    if input.entry != entry || input.session != session {
        bail!("Input belongs to another conversation");
    }
    Ok(
        json!({"id":input.id,"text":input.text,"voice":input.voice.as_ref().map(|v|json!({"audio_sha256":v.audio_sha256,"mime":v.mime})),"phase":input.phase,"retired":input.retired,"routing":input.routing,"channel_kind":"conversation","source_event":input.id,"update_id":null}),
    )
}

pub(super) fn eligible_on(conn: &Connection, input: &ConversationInput) -> Result<()> {
    let (session, native) = super::presentation::entry_on(conn, input.human, input.entry)?;
    if input.retired
        || session != input.session
        || native != input.native
        || super::stopping::admission_epoch_on(conn, input.entry)? != input.epoch
        || super::stopping::fenced_on(conn, input.entry)?
        || super::maintenance_writer_held_on(conn, input.entry)?
    {
        bail!("Original conversation input binding or admission generation retired");
    }
    if let Some(visual) = input.visual.as_ref() {
        crate::visual::attachments(
            std::path::Path::new(conn.path().context("Image storage unavailable")?),
            Some(visual),
        )?;
    }
    Ok(())
}

/// Select presentation by receiver capture order, without admitting direct input
/// to the routing controller. A delayed transcript cannot replace a newer choice.
pub(super) fn select_return_on(
    conn: &Connection,
    human: i64,
    native: &str,
    source: &str,
) -> Result<()> {
    let order = |id: &str| -> Result<Option<i64>> {
        let (channel, event) = id
            .strip_prefix("telegram:")
            .map_or(("conversation", id), |v| ("telegram", v));
        Ok(conn
            .query_row(
                "SELECT id FROM original_ingress_order WHERE channel=?1 AND event_id=?2",
                params![channel, event],
                |r| r.get(0),
            )
            .optional()?)
    };
    let previous: Option<String> = conn
        .query_row(
            "SELECT source_input FROM conversation_returns WHERE human_id=?1 AND native_thread=?2",
            params![human, native],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(previous) = previous {
        if previous == source {
            return Ok(());
        }
        if let Some(old) = order(&previous)? {
            if order(source)?.is_none_or(|new| new <= old) {
                return Ok(());
            }
        }
    }
    conn.execute("INSERT INTO conversation_returns(human_id,native_thread,source_input,selected_at) VALUES(?1,?2,?3,?4) ON CONFLICT(human_id,native_thread) DO UPDATE SET source_input=excluded.source_input,selected_at=excluded.selected_at", params![human,native,source,now_string()])?;
    Ok(())
}

impl Store {
    pub(super) fn init_conversation_inputs(&self) -> Result<()> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        conn.execute_batch("CREATE TABLE IF NOT EXISTS conversation_inputs(
            id TEXT PRIMARY KEY,human_id INTEGER NOT NULL,session_id INTEGER NOT NULL,
            native_thread TEXT NOT NULL,phase TEXT NOT NULL,local_turn INTEGER,retired INTEGER NOT NULL DEFAULT 0,
            doc TEXT NOT NULL,instance_id TEXT NOT NULL,created_at TEXT NOT NULL);
            CREATE INDEX IF NOT EXISTS conversation_inputs_entry ON conversation_inputs(session_id,human_id,native_thread);")?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS conversation_returns(
            human_id INTEGER NOT NULL,native_thread TEXT NOT NULL,source_input TEXT NOT NULL,
            selected_at TEXT NOT NULL,PRIMARY KEY(human_id,native_thread));
            CREATE INDEX IF NOT EXISTS conversation_returns_native ON conversation_returns(native_thread);",
        )?;
        // These are retained source identities, not live foreign keys. Retire
        // monotonically even when a binding or grant is subsequently restored.
        conn.execute_batch("CREATE TRIGGER IF NOT EXISTS conversation_input_binding_retired AFTER UPDATE OF codex_thread_id,force_fresh_thread,creator_user_id ON sessions BEGIN
            UPDATE conversation_inputs SET retired=1 WHERE session_id=NEW.id AND (native_thread IS NOT NEW.codex_thread_id OR NEW.force_fresh_thread=1 OR human_id!=NEW.creator_user_id); END;
            CREATE TRIGGER IF NOT EXISTS conversation_input_session_deleted AFTER DELETE ON sessions BEGIN UPDATE conversation_inputs SET retired=1 WHERE session_id=OLD.id; END;
            CREATE TRIGGER IF NOT EXISTS conversation_input_owner_revoked AFTER UPDATE OF allowed ON users WHEN NEW.allowed=0 BEGIN UPDATE conversation_inputs SET retired=1 WHERE human_id=NEW.tg_user_id; END;")?;
        // A held reply may not silently adopt an unrelated native replacement or
        // a restored ownership grant. Initial native establishment is compatible;
        // fresh human inputs can select the current logical session after renewal.
        conn.execute_batch("CREATE TRIGGER IF NOT EXISTS conversation_focus_binding_retired AFTER UPDATE OF codex_thread_id,force_fresh_thread,creator_user_id ON sessions
            WHEN (OLD.codex_thread_id IS NOT NULL AND OLD.codex_thread_id IS NOT NEW.codex_thread_id) OR NEW.force_fresh_thread=1 OR OLD.creator_user_id!=NEW.creator_user_id BEGIN
            UPDATE conversation_inputs SET doc=json_set(doc,'$.focus.retired',json('true')) WHERE json_extract(doc,'$.focus.session')=NEW.id AND phase IN ('preparing','retained','queued','awaiting_target'); END;
            CREATE TRIGGER IF NOT EXISTS conversation_focus_session_deleted AFTER DELETE ON sessions BEGIN
            UPDATE conversation_inputs SET doc=json_set(doc,'$.focus.retired',json('true')) WHERE json_extract(doc,'$.focus.session')=OLD.id AND phase IN ('preparing','retained','queued','awaiting_target'); END;")?;
        conn.execute("UPDATE conversation_inputs SET phase=CASE WHEN phase IN ('sending','steering') THEN 'uncertain' WHEN phase='preparing' THEN 'preparation_unconfirmed' ELSE 'retained_unsent' END WHERE instance_id!=?1 AND phase IN ('retained','queued','sending','steering','preparing')",[&self.instance_id])?;
        Ok(())
    }

    /// Retain exact owner-authenticated text before any native write. UUID reuse
    /// inspects the original outcome; it never authorizes another submission.
    #[cfg(test)]
    pub fn conversation_input_capture(
        &self,
        human: i64,
        entry: SessionKey,
        binding: &str,
        id: &str,
        text: &str,
    ) -> Result<(ConversationInput, bool)> {
        self.conversation_input_capture_reply(human, entry, binding, id, text, None)
    }

    #[cfg(test)]
    pub fn conversation_input_capture_reply(
        &self,
        human: i64,
        entry: SessionKey,
        binding: &str,
        id: &str,
        text: &str,
        reply: Option<ConversationReply>,
    ) -> Result<(ConversationInput, bool)> {
        self.capture_original(human, entry, binding, id, text, None, true, reply, None)
    }

    #[cfg(test)]
    pub fn conversation_voice_capture(
        &self,
        human: i64,
        entry: SessionKey,
        binding: &str,
        id: &str,
        voice: ConversationVoice,
        can_prepare: bool,
    ) -> Result<(ConversationInput, bool)> {
        voice.validate()?;
        if voice.transcription.is_some() {
            bail!("Transcription is produced only by the receiving adapter");
        }
        self.capture_original(
            human,
            entry,
            binding,
            id,
            "",
            Some(voice),
            can_prepare,
            None,
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn conversation_voice_capture_reply(
        &self,
        human: i64,
        entry: SessionKey,
        binding: &str,
        id: &str,
        voice: ConversationVoice,
        can_prepare: bool,
        reply: Option<ConversationReply>,
        visual: Option<crate::visual::VisualSelection>,
    ) -> Result<(ConversationInput, bool)> {
        voice.validate()?;
        if voice.transcription.is_some() {
            bail!("Transcription is receiving-owned");
        }
        self.capture_original(
            human,
            entry,
            binding,
            id,
            "",
            Some(voice),
            can_prepare,
            reply,
            visual,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn conversation_input_capture_visual(
        &self,
        human: i64,
        entry: SessionKey,
        binding: &str,
        id: &str,
        text: &str,
        reply: Option<ConversationReply>,
        visual: Option<crate::visual::VisualSelection>,
    ) -> Result<(ConversationInput, bool)> {
        self.capture_original(human, entry, binding, id, text, None, true, reply, visual)
    }

    #[allow(clippy::too_many_arguments)]
    fn capture_original(
        &self,
        human: i64,
        entry: SessionKey,
        binding: &str,
        id: &str,
        text: &str,
        voice: Option<ConversationVoice>,
        can_prepare: bool,
        reply: Option<ConversationReply>,
        visual: Option<crate::visual::VisualSelection>,
    ) -> Result<(ConversationInput, bool)> {
        if Uuid::parse_str(id)
            .ok()
            .is_none_or(|uuid| uuid.to_string() != id)
            || voice.is_none() && text.trim().is_empty()
            || text.len() > 32 * 1024
            || text.contains('\0')
        {
            bail!("Invalid bounded conversation input");
        }
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction()?;
        let (session, native) = super::presentation::entry_on(&tx, human, entry)?;
        if binding != super::presentation::entry_binding(&tx, human, entry, session)? {
            bail!("Selected conversation changed before input capture");
        }
        let existing: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM conversation_inputs WHERE id=?1)",
            [id],
            |r| r.get(0),
        )?;
        if existing {
            let input = read_on(&tx, id, human)?;
            if input.entry != entry
                || input.visual != visual
                || input.reply != reply
                || match (&input.voice, &voice) {
                    (None, None) => input.text != text,
                    (Some(a), Some(b)) => a.audio_sha256 != b.audio_sha256 || a.mime != b.mime,
                    _ => true,
                }
            {
                bail!("Original conversation input identity is immutable");
            }
            return Ok((input, false));
        }
        if let Some(reference) = visual.as_ref() {
            reference.validate(true)?;
            crate::visual::attachments(
                std::path::Path::new(tx.path().context("Image storage unavailable")?),
                Some(reference),
            )?;
        }
        let question_override = voice.is_none()
            && crate::interaction::explicit_selector(text).is_some()
            && reply
                .as_ref()
                .is_some_and(|r| r.source.starts_with("request:"));
        let focus = if let Some(context) = reply.as_ref().filter(|r| r.context_only) {
            context.validate_context()?;
            // Device metadata is not authority. Preserve a well-formed stale
            // hint without blocking a new human message or selecting a target.
            super::conversation_routing::reply_on(&tx, human, entry, context).ok()
        } else if question_override {
            None
        } else {
            reply
                .as_ref()
                .map(|r| super::conversation_routing::reply_on(&tx, human, entry, r))
                .transpose()?
        };
        let input = ConversationInput {
            id: id.into(),
            human,
            entry,
            session,
            native,
            epoch: super::stopping::admission_epoch_on(&tx, entry)?,
            text: text.into(),
            visual,
            question_versions: Some(super::conversation_requests::capture_questions_on(
                &tx, human, entry,
            )?),
            voice: voice.clone(),
            reply,
            focus,
            routing: None,
            phase: if !can_prepare
                || super::stopping::fenced_on(&tx, entry)?
                || super::maintenance_writer_held_on(&tx, entry)?
            {
                "retained_unsent"
            } else if voice.is_some() {
                "preparing"
            } else {
                "retained"
            }
            .into(),
            local_turn: None,
            retired: false,
        };
        tx.execute(
            "INSERT INTO original_ingress_order(channel,event_id) VALUES('conversation',?1)",
            [&input.id],
        )?;
        tx.execute("INSERT INTO conversation_inputs(id,human_id,session_id,native_thread,phase,doc,instance_id,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",params![input.id,human,session,input.native,input.phase,serde_json::to_string(&input)?,self.instance_id,now_string()])?;
        tx.commit()?;
        Ok((input, true))
    }

    /// Explicit recovery claims only a failed original, preserving ingress order.
    pub fn conversation_voice_resume(
        &self,
        human: i64,
        entry: SessionKey,
        binding: &str,
        id: &str,
    ) -> Result<(ConversationInput, bool)> {
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction()?;
        let (session, _) = super::presentation::entry_on(&tx, human, entry)?;
        if binding != super::presentation::entry_binding(&tx, human, entry, session)? {
            bail!("Entry changed before voice recovery");
        }
        let mut input = read_on(&tx, id, human)?;
        eligible_on(&tx, &input)?;
        if input.entry != entry
            || input.session != session
            || input
                .voice
                .as_ref()
                .is_none_or(|v| v.transcription.is_some())
        {
            bail!("Recovery requires the same untranscribed voice original");
        }
        if input.phase != "preparation_unconfirmed" {
            return Ok((input, false));
        }
        input.phase = "preparing".into();
        let changed=tx.execute("UPDATE conversation_inputs SET doc=?1,phase='preparing',instance_id=?2 WHERE id=?3 AND human_id=?4 AND phase='preparation_unconfirmed'",params![serde_json::to_string(&input)?,self.instance_id,id,human])?;
        tx.commit()?;
        Ok((input, changed == 1))
    }

    pub fn conversation_voice_validate(&self, human: i64, id: &str) -> Result<()> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let input = read_on(&conn, id, human)?;
        eligible_on(&conn, &input)?;
        if input.phase != "preparing" || input.voice.is_none() {
            bail!("Voice capture is not pending preparation");
        }
        Ok(())
    }
    pub fn conversation_voice_commit(
        &self,
        human: i64,
        id: &str,
        text: &str,
        proof: crate::voice::TranscriptSource,
    ) -> Result<ConversationInput> {
        if text.trim().is_empty() || text.len() > 16000 || text.contains('\0') {
            bail!("Invalid final voice interpretation");
        }
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction()?;
        let mut input = read_on(&tx, id, human)?;
        eligible_on(&tx, &input)?;
        let voice = input.voice.as_mut().context("Missing voice source")?;
        if input.phase != "preparing"
            || proof.capture_id.as_deref() != Some(id)
            || proof.telegram_file_id.is_some()
            || voice.audio_sha256 != proof.audio_sha256
            || voice.transcription.is_some()
        {
            bail!("Voice interpretation no longer matches this pending capture");
        }
        voice.transcription = Some(proof);
        input.text = text.into();
        input.phase = "retained".into();
        let changed=tx.execute("UPDATE conversation_inputs SET doc=?1,phase='retained' WHERE id=?2 AND human_id=?3 AND phase='preparing' AND instance_id=?4",params![serde_json::to_string(&input)?,id,human,self.instance_id])?;
        if changed != 1 {
            bail!("Voice preparation instance changed");
        }
        tx.commit()?;
        Ok(input)
    }
    pub fn conversation_voice_fail(&self, human: i64, id: &str) -> Result<()> {
        self.conn.lock().expect("store mutex poisoned").execute("UPDATE conversation_inputs SET phase=CASE WHEN phase='preparing' THEN 'preparation_unconfirmed' ELSE 'retained_unsent' END WHERE id=?1 AND human_id=?2 AND phase IN ('preparing','retained','queued') AND instance_id=?3",params![id,human,self.instance_id])?;
        Ok(())
    }

    pub fn conversation_input_read(
        &self,
        human: i64,
        entry: SessionKey,
        id: &str,
    ) -> Result<ConversationInput> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        // Owners may inspect a retained old binding, but cannot dispatch it.
        let (session, _) = super::presentation::entry_on(&conn, human, entry)?;
        let input = read_on(&conn, id, human)?;
        if input.entry != entry || input.session != session {
            bail!("Input belongs to another conversation");
        }
        Ok(input)
    }

    /// Trace an owned original through immutable controller instructions/results.
    /// This read does not prepare, transmit, retry or synthesize anything.
    pub fn conversation_input_trace(
        &self,
        human: i64,
        entry: SessionKey,
        id: &str,
    ) -> Result<serde_json::Value> {
        use crate::interaction::{Delivery, Instruction};
        use serde_json::json;
        let answer = self.conversation_answer_receipt(human, entry, id)?;
        let conn = self.conn.lock().expect("store mutex poisoned");
        let (session, native) = super::presentation::entry_on(&conn, human, entry)?;
        let original = trace_original_on(&conn, human, entry, id)?;
        let mut query = conn.prepare(
            "SELECT i.doc,i.phase,coalesce(s.creator_user_id=?1 AND u.allowed=1,0)
             FROM interaction_sources o JOIN interaction_instructions i ON i.input_id=o.id
             LEFT JOIN sessions s ON s.id=i.session_id
             LEFT JOIN users u ON u.tg_user_id=s.creator_user_id
             WHERE o.human_id=?1 AND o.event_id=?2
               AND ((?3='conversation' AND o.channel='conversation') OR
                    (?3='telegram' AND o.channel LIKE 'telegram:%' AND o.update_id=?4 AND o.origin_chat=?5 AND o.origin_thread=?6))
             ORDER BY i.sequence LIMIT 6",
        )?;
        let rows = query
            .query_map(
                params![
                    human,
                    original["source_event"].as_str(),
                    original["channel_kind"].as_str(),
                    original["update_id"].as_i64(),
                    entry.chat_id,
                    entry.thread_id
                ],
                |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, bool>(2)?,
                    ))
                },
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        if rows.len() > 5 {
            bail!("Trace exceeds selected destination bound");
        }
        let mut routes = Vec::new();
        let mut result_budget = 24_000;
        for (raw, phase, available) in rows {
            let instruction: Instruction = serde_json::from_str(&raw)?;
            if instruction.human_id != human {
                bail!("Trace instruction owner changed");
            }
            let mut results = Vec::new();
            let mut more_results = false;
            if available {
                let mut deliveries = conn.prepare(
                    "SELECT DISTINCT d.doc,d.phase FROM interaction_request_links l
                     JOIN interaction_delivery_requests links ON links.request_id=l.request_id
                     JOIN interaction_deliveries d ON d.id=links.delivery_id
                     WHERE l.instruction_id=?1 ORDER BY d.rowid DESC LIMIT 9",
                )?;
                let selected = deliveries
                    .query_map([&instruction.id], |r| {
                        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
                    })?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                more_results = selected.len() > 8;
                for (raw, delivery_phase) in selected.into_iter().take(8) {
                    let delivery: Delivery = serde_json::from_str(&raw)?;
                    let (current, pending) = super::routing::delivery_freshness(&conn, &delivery)?;
                    let result = json!({"source":format!("delivery:{}",delivery.id),
                        "phase":delivery_phase,"text_excerpt":delivery.body.chars().take(500).collect::<String>(),
                        "text_bounded":delivery.body.chars().count()>500,
                        "instruction_covered":delivery.covered.contains(&instruction.id),
                        "declared":delivery.declared,"completed":delivery.completed,
                        "current":current,"correction_pending":pending});
                    let size = serde_json::to_vec(&result)?.len();
                    if size > result_budget {
                        more_results = true;
                        break;
                    }
                    result_budget -= size;
                    results.push(result);
                }
            }
            routes.push(
                json!({"instruction":instruction.id,"target":instruction.target,
                "relation":instruction.relation,"requests":instruction.requests,
                "forwarded_original":instruction.text.chars().take(2048).collect::<String>(),
                "forwarded_next_offset":(instruction.text.chars().count()>2048).then_some(2048),"transport_phase":phase,
                "native_acceptance":phase=="accepted","target_available":available,
                "results":results,"more_results":more_results}),
            );
        }
        Ok(json!({"schema":"telecodex.conversation-trace/v1","id":id,
            "entry_binding":[human,entry,session,native],"text":original["text"].as_str().unwrap_or_default().chars().take(2048).collect::<String>(),
            "text_next_offset":(original["text"].as_str().unwrap_or_default().chars().count()>2048).then_some(2048),
            "voice":original["voice"],"phase":original["phase"],"retired":original["retired"],"routing":original["routing"],
            "answer":answer.map(|a|json!({"id":a["id"],"revision":a["revision"],"recorded":true})),
            "routes":routes,"read_only":true,
            "evidence":"Forwarded originals are exact retained controller text, not reconstructed native relay envelopes. Native acceptance and worker coverage declarations are separate evidence."}))
    }

    pub fn conversation_trace_text(
        &self,
        human: i64,
        entry: SessionKey,
        id: &str,
        source: &str,
        offset: usize,
    ) -> Result<serde_json::Value> {
        use crate::interaction::Instruction;
        use serde_json::json;
        let conn = self.conn.lock().expect("store mutex poisoned");
        let (session, native) = super::presentation::entry_on(&conn, human, entry)?;
        let original = trace_original_on(&conn, human, entry, id)?;
        let text = if source == "original" {
            original["text"].as_str().unwrap_or_default().to_owned()
        } else {
            let raw:String=conn.query_row("SELECT i.doc FROM interaction_instructions i JOIN interaction_sources o ON o.id=i.input_id WHERE i.id=?1 AND o.event_id=?2 AND o.human_id=?3 AND ((?4='conversation' AND o.channel='conversation') OR (?4='telegram' AND o.channel LIKE 'telegram:%' AND o.update_id=?5 AND o.origin_chat=?6 AND o.origin_thread=?7))",params![source,original["source_event"].as_str(),human,original["channel_kind"].as_str(),original["update_id"].as_i64(),entry.chat_id,entry.thread_id],|r|r.get(0))?;
            let instruction: Instruction = serde_json::from_str(&raw)?;
            if instruction.human_id != human {
                bail!("Trace instruction owner changed");
            }
            instruction.text
        };
        let length = text.chars().count();
        if offset > length {
            bail!("Invalid trace text offset");
        }
        let part = text.chars().skip(offset).take(2048).collect::<String>();
        Ok(
            json!({"schema":"telecodex.conversation-trace-text/v1","id":id,"source":source,
            "entry_binding":[human,entry,session,native],"offset":offset,"text":part,
            "next_offset":(offset+2048<length).then_some(offset+2048),"read_only":true}),
        )
    }

    /// Telegram observations use retained ingress, including unrouted messages.
    pub fn conversation_telegram_page(
        &self,
        human: i64,
        entry: SessionKey,
        cursor: i64,
        before: bool,
    ) -> Result<serde_json::Value> {
        use serde_json::json;
        if cursor < 0 {
            bail!("Invalid Telegram input cursor");
        }
        let conn = self.conn.lock().expect("store mutex poisoned");
        let (session, native) = super::presentation::entry_on(&conn, human, entry)?;
        let comparison = if before { "<" } else { ">" };
        let order = if before { "DESC" } else { "ASC" };
        let mut query=conn.prepare(&format!("SELECT i.update_id,g.id,g.captured_at FROM incoming_updates i LEFT JOIN original_ingress_order g ON g.channel='telegram' AND g.event_id=CAST(i.update_id AS TEXT) WHERE json_extract(i.payload_json,'$.message.from.id')=?1 AND json_extract(i.payload_json,'$.message.from.is_bot')=0 AND json_extract(i.payload_json,'$.message.chat.id')=?2 AND COALESCE(json_extract(i.payload_json,'$.message.message_thread_id'),0)=?3 AND (json_type(i.payload_json,'$.message.text')='text' OR json_type(i.payload_json,'$.message.voice')='object') AND i.update_id{comparison}?4 ORDER BY i.update_id {order} LIMIT 9"))?;
        let rows = query
            .query_map(
                params![
                    human,
                    entry.chat_id,
                    entry.thread_id,
                    if before && cursor == 0 {
                        i64::MAX
                    } else {
                        cursor
                    }
                ],
                |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, Option<i64>>(1)?,
                        r.get::<_, Option<String>>(2)?,
                    ))
                },
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut items = Vec::new();
        for (update, capture, received_at) in rows.iter().take(8) {
            let id = format!("telegram:{update}");
            let original = trace_original_on(&conn, human, entry, &id)?;
            items.push(json!({"id":id,"sequence":update,"received_at":received_at,"capture_order":capture,"ordering_unknown":capture.is_none(),"origin_kind":"telegram","read_only":true,"preview":original["text"].as_str().unwrap_or_default().chars().take(1000).collect::<String>(),"voice":original["voice"],"phase":original["phase"],"retired":original["retired"]}));
        }
        Ok(
            json!({"schema":"telecodex.telegram-input-list/v1","entry_binding":[human,entry,session,native],"binding":super::presentation::entry_binding(&conn,human,entry,session)?,"items":items,"cursor":rows.iter().take(8).next_back().map(|r|r.0),"newest":rows.iter().take(8).map(|r|r.0).max().unwrap_or(0),"more":rows.len()>8,"read_only":true}),
        )
    }

    /// Recent retained originals for this owned logical entry, newest first.
    /// Pagination never dispatches an input or invents an acceptance receipt.
    pub fn conversation_input_page(
        &self,
        human: i64,
        entry: SessionKey,
        cursor: i64,
        before: bool,
    ) -> Result<serde_json::Value> {
        if cursor < 0 {
            bail!("Invalid input history cursor");
        }
        let conn = self.conn.lock().expect("store mutex poisoned");
        let (session, native) = super::presentation::entry_on(&conn, human, entry)?;
        let comparison = if before { "<" } else { ">" };
        let order = if before { "DESC" } else { "ASC" };
        let mut statement = conn.prepare(&format!(
            "SELECT rowid,id FROM conversation_inputs WHERE human_id=?1 AND session_id=?2
             AND rowid{comparison}?3 ORDER BY rowid {order} LIMIT 9"
        ))?;
        let rows = statement
            .query_map(
                params![
                    human,
                    session,
                    if before && cursor == 0 {
                        i64::MAX
                    } else {
                        cursor
                    }
                ],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut items = Vec::new();
        for (sequence, id) in rows.iter().take(8) {
            let input = read_on(&conn, id, human)?;
            let capture:Option<i64>=conn.query_row("SELECT id FROM original_ingress_order WHERE channel='conversation' AND event_id=?1",[id],|r|r.get(0)).optional()?;
            let received_at: String = conn.query_row(
                "SELECT created_at FROM conversation_inputs WHERE id=?1",
                [id],
                |r| r.get(0),
            )?;
            items.push(serde_json::json!({
                "received_at":received_at,"id":id,"sequence":sequence,"capture_order":capture,"ordering_unknown":capture.is_none(),"preview":input.text.chars().take(1000).collect::<String>(),
                "phase":input.phase,"retired":input.retired,"visual":input.visual,"voice":input.voice,"routing":input.routing,"continuation":super::conversation_routing::continuation_on(&conn,human,entry,&input)?,
                "clarifiable":input.phase=="awaiting_target" && eligible_on(&conn,&input).is_ok(),
                "native_binding_current":input.native==native,
            }));
        }
        Ok(
            serde_json::json!({"schema":"telecodex.conversation-input-list/v1",
            "entry_binding":[human,entry,session,native],
            "binding":super::presentation::entry_binding(&conn,human,entry,session)?,
            "cursor":rows.iter().take(8).next_back().map(|row|row.0),
            "newest":rows.iter().take(8).map(|row|row.0).max().unwrap_or(0),
            "more":rows.len()>8,"items":items}),
        )
    }

    pub fn conversation_input_validate(&self, human: i64, id: &str) -> Result<()> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let input = read_on(&conn, id, human)?;
        eligible_on(&conn, &input)?;
        if !matches!(input.phase.as_str(), "queued" | "sending" | "steering") {
            bail!("Conversation input is not eligible for native submission");
        }
        Ok(())
    }

    pub fn conversation_input_can_clarify(&self, human: i64, id: &str) -> Result<bool> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let input = read_on(&conn, id, human)?;
        Ok(input.phase == "awaiting_target" && eligible_on(&conn, &input).is_ok())
    }

    pub fn conversation_input_finish(&self, human: i64, id: &str) -> Result<()> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        conn.execute("UPDATE conversation_inputs SET phase=CASE WHEN phase IN ('retained','queued') THEN 'retained_unsent' ELSE 'uncertain' END WHERE id=?1 AND human_id=?2 AND instance_id=?3 AND phase IN ('retained','queued','sending','steering')",params![id,human,self.instance_id])?;
        Ok(())
    }

    /// The native provider proved that the initial packet was never written.
    /// This also covers corrections admitted into that still-preparing packet.
    /// It retains a known-unsent outcome and grants no automatic resubmission.
    pub fn conversation_inputs_not_submitted(&self, human: i64, local_turn: i64) -> Result<()> {
        self.conn.lock().expect("store mutex poisoned").execute(
            "UPDATE conversation_inputs SET phase='retained_unsent' WHERE human_id=?1 AND local_turn=?2 AND instance_id=?3 AND phase IN ('sending','steering','uncertain')",
            params![human,local_turn,self.instance_id])?;
        Ok(())
    }

    /// Begin events validate the current owner/binding/epoch in the same SQLite
    /// transaction. Observed acknowledgments remain recordable after revocation
    /// or a fence: recording an effect does not authorize future execution.
    pub fn conversation_input_event(
        &self,
        human: i64,
        id: &str,
        event: ConversationInputEvent,
    ) -> Result<ConversationInput> {
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction()?;
        let input = read_on(&tx, id, human)?;
        if let ConversationInputEvent::Accepted { local_turn } = event
            && matches!(
                input.phase.as_str(),
                "accepted" | "answered" | "matrix_requested"
            )
            && input.local_turn == Some(local_turn)
        {
            return Ok(input);
        }
        let (phase, local) = match event {
            ConversationInputEvent::Queue
                if matches!(input.phase.as_str(), "retained" | "rejected") =>
            {
                eligible_on(&tx, &input)?;
                // Select the return surface before any possible native uptake.
                // The context may retain this input after the local turn ends,
                // so the selection survives collectors and logical rebindings.
                if !input.native.is_empty() {
                    select_return_on(&tx, human, &input.native, &input.id)?;
                }
                ("queued", None)
            }
            ConversationInputEvent::Begin { local_turn, steer }
                if input.phase == "queued"
                    || (input.phase == "rejected" && input.local_turn == Some(local_turn)) =>
            {
                eligible_on(&tx, &input)?;
                let matches:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM turns WHERE id=?1 AND session_id=?2 AND from_user_id=?3 AND (native_thread_id IS NULL OR native_thread_id=?4))",params![local_turn,input.session,human,input.native],|r|r.get(0))?;
                if !matches {
                    bail!("Conversation input execution mismatch");
                }
                (if steer { "steering" } else { "sending" }, Some(local_turn))
            }
            ConversationInputEvent::Accepted { local_turn }
                if input.local_turn == Some(local_turn)
                    && matches!(input.phase.as_str(), "sending" | "steering" | "uncertain") =>
            {
                ("accepted", Some(local_turn))
            }
            ConversationInputEvent::Rejected { local_turn }
                if input.local_turn == Some(local_turn)
                    && matches!(input.phase.as_str(), "sending" | "steering") =>
            {
                ("rejected", Some(local_turn))
            }
            ConversationInputEvent::Uncertain { local_turn }
                if input.local_turn == Some(local_turn)
                    && matches!(input.phase.as_str(), "sending" | "steering") =>
            {
                ("uncertain", Some(local_turn))
            }
            _ => bail!("Conversation input cannot be replayed or settled by another execution"),
        };
        let changed=tx.execute("UPDATE conversation_inputs SET phase=?1,local_turn=?2 WHERE id=?3 AND human_id=?4 AND phase=?5 AND instance_id=?6",params![phase,local,id,human,input.phase,self.instance_id])?;
        if changed != 1 {
            bail!("Conversation input belongs to a retired listener");
        }
        let updated = read_on(&tx, id, human)?;
        if phase == "accepted" {
            super::topic_publications::conversation_accepted_on(&tx, &updated)?;
        }
        tx.commit()?;
        Ok(updated)
    }

    pub fn interaction_select_native_return(
        &self,
        id: &str,
        human: i64,
        native: &str,
    ) -> Result<()> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let source: String=conn.query_row("SELECT x.doc FROM interaction_sources x JOIN interaction_instructions i ON i.input_id=x.id WHERE i.id=?1 AND x.human_id=?2",params![id,human],|r|r.get(0))?;
        let source: crate::interaction::SourceInput = serde_json::from_str(&source)?;
        if source.origin.message_id.is_none() {
            select_return_on(&conn, human, native, &source.event_id)?;
        }
        Ok(())
    }

    pub fn conversation_select_direct_return(
        &self,
        update: Option<i64>,
        key: SessionKey,
        human: i64,
    ) -> Result<()> {
        let Some(update) = update else {
            return Ok(());
        };
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction()?;
        let native: Option<String> = tx.query_row("SELECT codex_thread_id FROM sessions s WHERE chat_id=?1 AND thread_id=?2 AND creator_user_id=?3 AND force_fresh_thread=0 AND EXISTS(SELECT 1 FROM conversation_returns p WHERE p.native_thread=s.codex_thread_id AND p.human_id=?3)",params![key.chat_id,key.thread_id,human],|r|r.get(0)).optional()?.flatten();
        if let Some(native) = native {
            let owned:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM incoming_updates WHERE update_id=?1 AND json_extract(payload_json,'$.message.from.id')=?2 AND json_extract(payload_json,'$.message.chat.id')=?3 AND COALESCE(json_extract(payload_json,'$.message.message_thread_id'),0)=?4)",params![update,human,key.chat_id,key.thread_id],|r|r.get(0))?;
            if !owned {
                bail!("Direct presentation input owner mismatch");
            }
            select_return_on(&tx, human, &native, &format!("telegram:{update}"))?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn conversation_turn_direct_return(&self, local: i64) -> Result<bool> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        Ok(conn.query_row("SELECT EXISTS(SELECT 1 FROM turns t LEFT JOIN sessions s ON s.id=t.session_id JOIN conversation_returns p ON p.native_thread=COALESCE(t.native_thread_id,s.codex_thread_id) AND p.human_id=t.from_user_id WHERE t.id=?1 AND p.source_input LIKE 'telegram:%')",[local],|r|r.get(0))?)
    }

    pub fn conversation_turn_return(&self, local_turn: i64) -> Result<bool> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let selected:Option<bool>=conn.query_row("SELECT EXISTS(SELECT 1 FROM conversation_returns p WHERE p.native_thread=COALESCE(t.native_thread_id,s.codex_thread_id) AND p.human_id=t.from_user_id AND p.source_input NOT LIKE 'telegram:%') FROM turns t LEFT JOIN sessions s ON s.id=t.session_id WHERE t.id=?1",[local_turn],|r|r.get(0)).optional()?;
        // A deleted/unknown execution is not authority to publish its output.
        Ok(selected.unwrap_or(true))
    }

    pub fn native_interaction_source(
        &self,
        local_turn: i64,
        human: i64,
    ) -> Result<(SessionKey, String)> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        conn.query_row("SELECT s.chat_id,s.thread_id,t.native_thread_id FROM turns t JOIN sessions s ON s.id=t.session_id WHERE t.id=?1 AND t.from_user_id=?2",params![local_turn,human],|r|Ok((SessionKey::new(r.get(0)?,Some(r.get(1)?)),r.get(2)?))).context("Native interaction source unavailable")
    }

    pub fn settle_native_projection(&self, id: &str, local_turn: i64, state: &str) -> Result<()> {
        if !matches!(state, "answering" | "expired") {
            bail!("Invalid native projection transition");
        }
        self.conn.lock().expect("store mutex poisoned").execute("UPDATE human_interactions SET state=?3,updated_at=?4 WHERE id=?1 AND local_turn=?2 AND kind='native' AND state='pending' AND instance_id=?5",params![id,local_turn,state,now_string(),self.instance_id])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ConversationInputEvent as E;

    #[test]
    fn native_presentation_selection_follows_capture_order_across_surfaces() {
        let (_file, store, _, _) = setup();
        let conn = store.conn.lock().unwrap();
        for (channel, event) in [
            ("conversation", "old-voice"),
            ("telegram", "10"),
            ("conversation", "new-text"),
        ] {
            conn.execute(
                "INSERT INTO original_ingress_order(channel,event_id) VALUES(?1,?2)",
                params![channel, event],
            )
            .unwrap();
        }
        select_return_on(&conn, 100, "entry-native", "old-voice").unwrap();
        select_return_on(&conn, 100, "entry-native", "telegram:10").unwrap();
        select_return_on(&conn, 100, "entry-native", "old-voice").unwrap();
        let selected = || {
            conn.query_row("SELECT source_input FROM conversation_returns WHERE human_id=100 AND native_thread='entry-native'",[],|r|r.get::<_,String>(0)).unwrap()
        };
        assert_eq!(selected(), "telegram:10");
        select_return_on(&conn, 100, "entry-native", "new-text").unwrap();
        select_return_on(&conn, 100, "entry-native", "telegram:10").unwrap();
        assert_eq!(selected(), "new-text");
    }

    fn setup() -> (tempfile::NamedTempFile, Store, SessionKey, String) {
        let file = tempfile::NamedTempFile::new().unwrap();
        let store = Store::open(file.path(), &[100], &super::super::tests::defaults()).unwrap();
        let key = SessionKey::new(100, Some(1));
        store
            .ensure_session(key, 100, &super::super::tests::defaults())
            .unwrap();
        store.set_session_codex_thread(key, "entry-native").unwrap();
        let binding = store.interaction_presentation_list(100, key, 0, 0).unwrap()["binding"]
            .as_str()
            .unwrap()
            .to_string();
        (file, store, key, binding)
    }

    fn clip() -> ConversationVoice {
        ConversationVoice {
            audio_sha256: "a".repeat(64),
            mime: "audio/wav".into(),
            transcription: None,
        }
    }
    fn voice_proof(id: &str) -> crate::voice::TranscriptSource {
        crate::voice::TranscriptSource {
            audio_sha256: "a".repeat(64),
            telegram_file_id: None,
            capture_id: Some(id.into()),
            configured_transcriber: "synthetic/asr".into(),
            committed_at: now_string(),
            audio_millis: 100,
            preparation_millis: 1,
            transcription_millis: 1,
            interpretation: "machine, not exact written speech".into(),
        }
    }
    #[test]
    fn completed_voice_recovery_claim_is_owned_idempotent_and_preserves_original_order() {
        let (_file, store, key, binding) = setup();
        let id = Uuid::now_v7().to_string();
        store
            .conversation_voice_capture_reply(100, key, &binding, &id, clip(), true, None, None)
            .unwrap();
        assert!(
            !store
                .conversation_voice_resume(100, key, &binding, &id)
                .unwrap()
                .1
        );
        store.conversation_voice_fail(100, &id).unwrap();
        assert!(
            store
                .conversation_voice_resume(100, key, "wrong", &id)
                .is_err()
        );
        assert!(
            store
                .conversation_voice_resume(101, key, &binding, &id)
                .is_err()
        );
        assert!(
            store
                .conversation_voice_resume(100, key, &binding, &id)
                .unwrap()
                .1
        );
        assert!(
            !store
                .conversation_voice_resume(100, key, &binding, &id)
                .unwrap()
                .1
        );
        store
            .conversation_voice_commit(100, &id, "Recovered original", voice_proof(&id))
            .unwrap();
        assert!(
            store
                .conversation_voice_resume(100, key, &binding, &id)
                .is_err()
        );
        let conn = store.conn.lock().unwrap();
        assert_eq!(conn.query_row("SELECT count(*) FROM original_ingress_order WHERE channel='conversation' AND event_id=?1",[&id],|r|r.get::<_,i64>(0)).unwrap(),1);
    }

    #[test]
    fn durable_task_reply_is_untrusted_context_not_a_question_answer_or_completion() {
        let (_file, store, key, binding) = setup();
        let hint = ConversationReply {
            source: format!("task:{}", "a".repeat(32)),
            revision: "b".repeat(64),
            context_only: true,
        };
        let (input, _) = store
            .conversation_input_capture_reply(
                100,
                key,
                &binding,
                &Uuid::now_v7().to_string(),
                "Here is what I observed",
                Some(hint.clone()),
            )
            .unwrap();
        assert_eq!(input.text, "Here is what I observed");
        assert_eq!(input.reply, Some(hint.clone()));
        assert!(input.focus.is_none());
        assert!(input.rendered().contains("human_task"));
        assert!(
            input
                .rendered()
                .contains("not prove task ownership, human completion")
        );
        assert!(
            store
                .conversation_input_capture_reply(
                    100,
                    key,
                    &binding,
                    &Uuid::now_v7().to_string(),
                    "An explicit native answer",
                    Some(ConversationReply {
                        context_only: false,
                        ..hint.clone()
                    })
                )
                .is_err()
        );
        assert!(
            store
                .conversation_input_capture_reply(
                    100,
                    key,
                    &binding,
                    &Uuid::now_v7().to_string(),
                    "Bad hint",
                    Some(ConversationReply {
                        source: "task:not-an-id".into(),
                        ..hint
                    })
                )
                .is_err()
        );
    }

    #[test]
    fn voice_capture_precedes_later_text_and_cannot_admit_partial_or_replaced_source() {
        let (_file, store, key, binding) = setup();
        let id = Uuid::now_v7().to_string();
        let (pending, fresh) = store
            .conversation_voice_capture(100, key, &binding, &id, clip(), true)
            .unwrap();
        assert!(fresh);
        assert_eq!(pending.phase, "preparing");
        assert!(pending.text.is_empty());
        assert!(store.conversation_input_event(100, &id, E::Queue).is_err());
        assert!(store.conversation_input_validate(100, &id).is_err());
        store.conversation_voice_validate(100, &id).unwrap();
        let later = Uuid::now_v7().to_string();
        store
            .conversation_input_capture(100, key, &binding, &later, "Use the later correction")
            .unwrap();
        let mut wrong = voice_proof(&id);
        wrong.audio_sha256 = "b".repeat(64);
        assert!(
            store
                .conversation_voice_commit(100, &id, "Wrong source", wrong)
                .is_err()
        );
        let ready = store
            .conversation_voice_commit(
                100,
                &id,
                "Earlier automatic interpretation",
                voice_proof(&id),
            )
            .unwrap();
        assert_eq!(ready.phase, "retained");
        assert!(ready.rendered().contains("not verified written speech"));
        store.conversation_input_event(100, &id, E::Queue).unwrap();
        let original = store
            .conversation_voice_capture(100, key, &binding, &id, clip(), true)
            .unwrap();
        assert!(!original.1);
        assert_eq!(original.0.phase, "queued");
        assert!(
            store
                .conversation_input_capture(
                    100,
                    key,
                    &binding,
                    &id,
                    "Earlier automatic interpretation"
                )
                .is_err()
        );
        let mut changed = clip();
        changed.audio_sha256 = "b".repeat(64);
        assert!(
            store
                .conversation_voice_capture(100, key, &binding, &id, changed, true)
                .is_err()
        );
        let order:Vec<String>=store.conn.lock().unwrap().prepare("SELECT event_id FROM original_ingress_order WHERE channel='conversation' ORDER BY id").unwrap().query_map([],|r|r.get(0)).unwrap().collect::<rusqlite::Result<_>>().unwrap();
        assert_eq!(order, vec![id, later]);
    }
    #[test]
    fn voice_authority_retires_during_preparation_and_restart_never_replays_asr() {
        let (file, store, key, binding) = setup();
        let id = Uuid::now_v7().to_string();
        store
            .conversation_voice_capture(100, key, &binding, &id, clip(), true)
            .unwrap();
        store.conn.lock().unwrap().execute_batch("UPDATE users SET allowed=0 WHERE tg_user_id=100; UPDATE users SET allowed=1 WHERE tg_user_id=100;").unwrap();
        assert!(store.conversation_voice_validate(100, &id).is_err());
        assert!(
            store
                .conversation_voice_commit(100, &id, "Late result", voice_proof(&id))
                .is_err()
        );
        let pending = Uuid::now_v7().to_string();
        assert!(
            store
                .conversation_voice_capture(100, key, &binding, &pending, clip(), true)
                .is_err()
        );
        let binding = store.conversation_request_binding(100, key).unwrap()["binding"]
            .as_str()
            .unwrap()
            .to_owned();
        store
            .conversation_voice_capture(100, key, &binding, &pending, clip(), true)
            .unwrap();
        let held = Uuid::now_v7().to_string();
        assert_eq!(
            store
                .conversation_voice_capture(100, key, &binding, &held, clip(), false)
                .unwrap()
                .0
                .phase,
            "retained_unsent"
        );
        assert!(store.conversation_voice_validate(100, &held).is_err());
        drop(store);
        let reopened = Store::open(file.path(), &[100], &super::super::tests::defaults()).unwrap();
        assert_eq!(
            reopened
                .conversation_input_read(100, key, &pending)
                .unwrap()
                .phase,
            "preparation_unconfirmed"
        );
        assert!(reopened.conversation_voice_validate(100, &pending).is_err());
        assert!(
            reopened
                .conversation_voice_commit(100, &pending, "No replay", voice_proof(&pending))
                .is_err()
        );
    }

    #[test]
    fn voice_cleanup_settles_post_commit_and_queued_fences_without_reversing_acceptance() {
        let (_file, store, key, binding) = setup();
        let ids: Vec<_> = (0..3).map(|_| Uuid::now_v7().to_string()).collect();
        for id in &ids {
            store
                .conversation_voice_capture(100, key, &binding, id, clip(), true)
                .unwrap();
            store
                .conversation_voice_commit(100, id, "Final interpretation", voice_proof(id))
                .unwrap();
        }
        for id in &ids[1..] {
            store.conversation_input_event(100, id, E::Queue).unwrap();
        }
        let local = turn(&store, key);
        store
            .conversation_input_event(
                100,
                &ids[2],
                E::Begin {
                    local_turn: local,
                    steer: true,
                },
            )
            .unwrap();
        store
            .conversation_input_event(100, &ids[2], E::Accepted { local_turn: local })
            .unwrap();
        store
            .save_bot_state("native_writer_hold:100:1", "synthetic-hold")
            .unwrap();
        assert!(
            store
                .conversation_input_event(100, &ids[0], E::Queue)
                .is_err()
        );
        for id in &ids {
            store.conversation_voice_fail(100, id).unwrap();
        }
        for id in &ids[..2] {
            assert_eq!(
                store.conversation_input_read(100, key, id).unwrap().phase,
                "retained_unsent"
            );
        }
        assert_eq!(
            store
                .conversation_input_read(100, key, &ids[2])
                .unwrap()
                .phase,
            "accepted"
        );
    }

    #[test]
    fn input_history_is_bounded_owned_and_survives_rebinding() {
        let (_file, store, key, binding) = setup();
        let mut ids = Vec::new();
        for n in 0..10 {
            let id = Uuid::now_v7().to_string();
            store
                .conversation_input_capture(100, key, &binding, &id, &format!("Message {n}"))
                .unwrap();
            ids.push(id);
        }
        let page = store.conversation_input_page(100, key, 0, true).unwrap();
        assert_eq!(page["items"].as_array().unwrap().len(), 8);
        assert_eq!(page["items"][0]["id"], ids[9]);
        assert_eq!(page["more"], true);
        let prior = store
            .conversation_input_page(100, key, page["cursor"].as_i64().unwrap(), true)
            .unwrap();
        assert_eq!(prior["items"].as_array().unwrap().len(), 2);
        assert_eq!(prior["items"][1]["id"], ids[0]);
        assert_eq!(prior["more"], false);
        let forward = store
            .conversation_input_page(
                100,
                key,
                prior["items"][1]["sequence"].as_i64().unwrap(),
                false,
            )
            .unwrap();
        assert_eq!(forward["items"][0]["id"], ids[1]);
        assert_eq!(forward["items"].as_array().unwrap().len(), 8);
        assert_eq!(forward["more"], true);
        assert!(forward["newest"].as_i64().unwrap() < page["newest"].as_i64().unwrap());
        assert!(store.conversation_input_page(101, key, 0, true).is_err());
        assert!(store.conversation_input_page(100, key, -1, true).is_err());
        store
            .set_session_codex_thread(key, "replacement-native")
            .unwrap();
        let changed = store.conversation_input_page(100, key, 0, true).unwrap();
        assert_ne!(changed["binding"], page["binding"]);
        assert_eq!(changed["items"][0]["retired"], true);
        assert_eq!(changed["items"][0]["native_binding_current"], false);
    }

    fn turn(store: &Store, key: SessionKey) -> i64 {
        let session = store.get_session(key).unwrap().unwrap();
        let local = store
            .record_turn_started(
                session.id,
                &TurnRequest {
                    session_key: key,
                    from_user_id: 100,
                    prompt: "Original".into(),
                    runtime_instructions: None,
                    attachments: vec![],
                    review_mode: None,
                    override_search_mode: None,
                },
            )
            .unwrap();
        store
            .record_turn_native_thread(local, "entry-native")
            .unwrap();
        local
    }

    #[test]
    fn accepted_web_original_has_one_secondary_echo_without_another_submission() {
        use ConversationInputEvent as E;
        let (_file, store, key, binding) = setup();
        store
            .save_bot_state(
                "telegram_entry_projection",
                &serde_json::json!([100, key.chat_id, key.thread_id]).to_string(),
            )
            .unwrap();
        let id = Uuid::now_v7().to_string();
        store
            .conversation_input_capture(100, key, &binding, &id, "Exact app original")
            .unwrap();
        store.conversation_input_event(100, &id, E::Queue).unwrap();
        let local = turn(&store, key);
        store.topic_projection_register(local, 100, key).unwrap();
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
        assert!(store.topic_publication_claim(key, 100).unwrap().is_none());
        for _ in 0..2 {
            store
                .conversation_input_event(100, &id, E::Accepted { local_turn: local })
                .unwrap();
        }
        let echo = store.topic_publication_claim(key, 100).unwrap().unwrap();
        assert_eq!(echo.body, "You · app\n\nExact app original");
        assert_eq!(echo.local_turn, local);
        assert_eq!(
            store.topic_publication_inventory(100).unwrap()["items"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        store.topic_publication_release(echo.id).unwrap();
        store.set_session_codex_thread(key, "replacement").unwrap();
        assert!(store.topic_publication_claim(key, 100).unwrap().is_none());
    }

    #[test]
    fn exact_original_survives_reconnect_without_a_telegram_update_or_second_submission() {
        let (_file, store, key, binding) = setup();
        let id = Uuid::now_v7().to_string();
        let original = "  Keep this exact message.\n¿Y la corrección? 🦉 ";
        let (input, fresh) = store
            .conversation_input_capture(100, key, &binding, &id, original)
            .unwrap();
        assert!(fresh);
        assert_eq!(input.text, original);
        assert!(
            !store
                .conversation_input_capture(100, key, &binding, &id, original)
                .unwrap()
                .1
        );
        assert!(
            store
                .conversation_input_capture(100, key, &binding, &id, "Edited")
                .is_err()
        );
        assert!(
            store
                .conversation_input_capture(101, key, &binding, &id, original)
                .is_err()
        );
        assert!(
            store
                .conversation_input_capture(100, key, "stale", &id, original)
                .is_err()
        );
        store.conversation_input_event(100, &id, E::Queue).unwrap();
        assert!(store.conversation_input_event(100, &id, E::Queue).is_err());
        let local = turn(&store, key);
        store
            .conversation_input_event(
                100,
                &id,
                E::Begin {
                    local_turn: local,
                    steer: true,
                },
            )
            .unwrap();
        assert!(
            store
                .conversation_input_event(
                    100,
                    &id,
                    E::Accepted {
                        local_turn: local + 1
                    }
                )
                .is_err()
        );
        store
            .conversation_input_event(100, &id, E::Accepted { local_turn: local })
            .unwrap();
        let (again, fresh) = store
            .conversation_input_capture(100, key, &binding, &id, original)
            .unwrap();
        assert!(!fresh);
        assert_eq!(again.phase, "accepted");
        assert_eq!(again.local_turn, Some(local));
        assert!(store.conversation_input_event(100, &id, E::Queue).is_err());
        let conn = store.conn.lock().unwrap();
        assert_eq!(
            conn.query_row("SELECT count(*) FROM incoming_updates", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }

    #[test]
    fn restart_retains_known_unsent_and_uncertain_inputs_without_rearming_them() {
        let (file, store, key, binding) = setup();
        let queued = Uuid::now_v7().to_string();
        let sent = Uuid::now_v7().to_string();
        for id in [&queued, &sent] {
            store
                .conversation_input_capture(100, key, &binding, id, "Retain original")
                .unwrap();
            store.conversation_input_event(100, id, E::Queue).unwrap();
        }
        let local = turn(&store, key);
        store
            .conversation_input_event(
                100,
                &sent,
                E::Begin {
                    local_turn: local,
                    steer: false,
                },
            )
            .unwrap();
        drop(store);
        let store = Store::open(file.path(), &[100], &super::super::tests::defaults()).unwrap();
        assert_eq!(
            store
                .conversation_input_read(100, key, &queued)
                .unwrap()
                .phase,
            "retained_unsent"
        );
        assert_eq!(
            store
                .conversation_input_read(100, key, &sent)
                .unwrap()
                .phase,
            "uncertain"
        );
        assert!(
            store
                .conversation_input_event(100, &queued, E::Queue)
                .is_err()
        );
        assert!(
            store
                .conversation_input_event(100, &sent, E::Queue)
                .is_err()
        );
        assert!(
            !store
                .conversation_input_capture(100, key, &binding, &sent, "Retain original")
                .unwrap()
                .1
        );
    }

    #[test]
    fn retired_epochs_and_bindings_never_revive_but_observed_receipts_are_retained() {
        let (_file, store, key, binding) = setup();
        let id = Uuid::now_v7().to_string();
        store
            .conversation_input_capture(100, key, &binding, &id, "Correct the work")
            .unwrap();
        store.conversation_input_event(100, &id, E::Queue).unwrap();
        let local = turn(&store, key);
        store
            .conversation_input_event(
                100,
                &id,
                E::Begin {
                    local_turn: local,
                    steer: true,
                },
            )
            .unwrap();
        store
            .save_bot_state("stop_admission_epoch:100", "new-human-admission-generation")
            .unwrap();
        store
            .conn
            .lock()
            .unwrap()
            .execute("UPDATE users SET allowed=0 WHERE tg_user_id=100", [])
            .unwrap();
        // An actual native acknowledgment remains evidence after grant revocation.
        let observed = store
            .conversation_input_event(100, &id, E::Accepted { local_turn: local })
            .unwrap();
        assert_eq!(observed.phase, "accepted");
        assert!(store.conversation_input_read(100, key, &id).is_err());
        store
            .conn
            .lock()
            .unwrap()
            .execute("UPDATE users SET allowed=1 WHERE tg_user_id=100", [])
            .unwrap();
        assert!(eligible_on(&store.conn.lock().unwrap(), &observed).is_err());
        store
            .set_session_codex_thread(key, "different-native")
            .unwrap();
        assert_eq!(
            store.conversation_input_read(100, key, &id).unwrap().native,
            "entry-native"
        );
        assert!(
            store
                .conversation_input_capture(
                    100,
                    key,
                    &binding,
                    &Uuid::now_v7().to_string(),
                    "New message"
                )
                .is_err()
        );
    }

    #[test]
    fn restored_grant_or_binding_cannot_revive_an_original_input() {
        for change in ["clear", "revoke"] {
            let (_file, store, key, binding) = setup();
            let id = Uuid::now_v7().to_string();
            store
                .conversation_input_capture(100, key, &binding, &id, "Keep original authority")
                .unwrap();
            store.conversation_input_event(100, &id, E::Queue).unwrap();
            if change == "clear" {
                store.clear_session_conversation(key).unwrap();
                store.set_session_codex_thread(key, "entry-native").unwrap();
            } else {
                let conn = store.conn.lock().unwrap();
                conn.execute("UPDATE users SET allowed=0 WHERE tg_user_id=100", [])
                    .unwrap();
                conn.execute("UPDATE users SET allowed=1 WHERE tg_user_id=100", [])
                    .unwrap();
            }
            assert!(
                store
                    .conversation_input_read(100, key, &id)
                    .unwrap()
                    .retired
            );
            assert!(store.conversation_input_validate(100, &id).is_err());
        }
    }

    #[test]
    fn deletion_preserves_originals_and_maintenance_hold_prevents_submission() {
        let (_file, store, key, binding) = setup();
        let id = Uuid::now_v7().to_string();
        let (original, _) = store
            .conversation_input_capture(100, key, &binding, &id, "Preserve despite deletion")
            .unwrap();
        store.conversation_input_event(100, &id, E::Queue).unwrap();
        store
            .save_bot_state("native_writer_hold:100:1", "fixture-hold")
            .unwrap();
        assert!(store.conversation_input_validate(100, &id).is_err());
        let local = turn(&store, key);
        assert!(
            store
                .conversation_input_event(
                    100,
                    &id,
                    E::Begin {
                        local_turn: local,
                        steer: true
                    }
                )
                .is_err()
        );
        store.conversation_input_finish(100, &id).unwrap();
        assert_eq!(
            store.conversation_input_read(100, key, &id).unwrap().phase,
            "retained_unsent"
        );
        store.take_bot_state("native_writer_hold:100:1").unwrap();
        let accepted_id = Uuid::now_v7().to_string();
        store
            .conversation_input_capture(100, key, &binding, &accepted_id, "Accepted original")
            .unwrap();
        store
            .conversation_input_event(100, &accepted_id, E::Queue)
            .unwrap();
        store
            .conversation_input_event(
                100,
                &accepted_id,
                E::Begin {
                    local_turn: local,
                    steer: true,
                },
            )
            .unwrap();
        store
            .conversation_input_event(100, &accepted_id, E::Uncertain { local_turn: local })
            .unwrap();
        store
            .conversation_input_event(100, &accepted_id, E::Accepted { local_turn: local })
            .unwrap();
        store.delete_session(key).unwrap();
        let retained = read_on(&store.conn.lock().unwrap(), &id, 100).unwrap();
        assert_eq!(retained.text, original.text);
        assert!(retained.retired);
        let accepted = read_on(&store.conn.lock().unwrap(), &accepted_id, 100).unwrap();
        assert!(accepted.retired);
        assert_eq!(accepted.phase, "accepted");
        assert_eq!(accepted.local_turn, Some(local));
        let new = store
            .ensure_session(key, 100, &super::super::tests::defaults())
            .unwrap();
        store.set_session_codex_thread(key, "entry-native").unwrap();
        assert_ne!(new.id, original.session);
        assert!(store.conversation_input_validate(100, &id).is_err());
    }

    #[test]
    fn proven_unsent_initial_packet_retains_its_corrections_without_replay() {
        let (_file, store, key, binding) = setup();
        let local = turn(&store, key);
        let mut ids = vec![];
        for steer in [false, true] {
            let id = Uuid::now_v7().to_string();
            store
                .conversation_input_capture(100, key, &binding, &id, "Original awaiting submission")
                .unwrap();
            store.conversation_input_event(100, &id, E::Queue).unwrap();
            store
                .conversation_input_event(
                    100,
                    &id,
                    E::Begin {
                        local_turn: local,
                        steer,
                    },
                )
                .unwrap();
            ids.push(id);
        }
        store
            .conversation_input_event(100, &ids[1], E::Uncertain { local_turn: local })
            .unwrap();
        store.conversation_inputs_not_submitted(100, local).unwrap();
        for id in ids {
            assert_eq!(
                store.conversation_input_read(100, key, &id).unwrap().phase,
                "retained_unsent"
            );
            assert!(store.conversation_input_event(100, &id, E::Queue).is_err());
        }
    }

    #[test]
    fn return_selection_follows_native_context_across_turns_restart_and_recreation() {
        let (file, store, key, binding) = setup();
        let first = turn(&store, key);
        assert!(!store.conversation_turn_return(first).unwrap());
        let id = Uuid::now_v7().to_string();
        store
            .conversation_input_capture(
                100,
                key,
                &binding,
                &id,
                "Continue through this conversation",
            )
            .unwrap();
        store.conversation_input_event(100, &id, E::Queue).unwrap();
        assert!(store.conversation_turn_return(first).unwrap());
        drop(store);
        let store = Store::open(file.path(), &[100], &super::super::tests::defaults()).unwrap();
        let second = turn(&store, key);
        assert!(store.conversation_turn_return(second).unwrap());
        store.delete_session(key).unwrap();
        store
            .ensure_session(key, 100, &super::super::tests::defaults())
            .unwrap();
        store.set_session_codex_thread(key, "entry-native").unwrap();
        assert!(store.conversation_turn_return(turn(&store, key)).unwrap());
        assert!(store.conversation_turn_return(first).unwrap());
        store
            .set_session_codex_thread(key, "a-new-context")
            .unwrap();
        let new = store.get_session(key).unwrap().unwrap();
        let local = store
            .record_turn_started(
                new.id,
                &TurnRequest {
                    session_key: key,
                    from_user_id: 100,
                    prompt: "A new context".into(),
                    runtime_instructions: None,
                    attachments: vec![],
                    review_mode: None,
                    override_search_mode: None,
                },
            )
            .unwrap();
        store
            .record_turn_native_thread(local, "a-new-context")
            .unwrap();
        assert!(!store.conversation_turn_return(local).unwrap());
    }
    #[test]
    fn canonical_original_routes_and_creates_without_telegram_identity() {
        use crate::interaction::{InputReference, RequestRelation};
        let (_file, store, entry, binding) = setup();
        let local = turn(&store, entry);
        let closed = tokio_util::sync::CancellationToken::new();
        let worker = SessionKey::new(100, Some(7));
        let row = store
            .ensure_session(worker, 100, &super::super::tests::defaults())
            .unwrap();
        store
            .set_session_codex_thread(worker, "worker-native")
            .unwrap();
        let mut refs = vec![];
        for text in [
            "  Route this exact canonical original.  ",
            "Create this independent canonical task.",
        ] {
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
            refs.push(InputReference::Conversation(id));
        }
        let (mut source, message) = store
            .interaction_entry_original(entry, 100, "entry-native", &refs[0], "telegram:fixture")
            .unwrap();
        assert!(message.is_none());
        assert_eq!(source.channel, "conversation");
        assert!(source.update_id.is_none());
        assert!(source.origin.message_id.is_none());
        source.target_sessions = vec![row.id];
        let admitted = store
            .interaction_admit_entry_plan(
                entry,
                100,
                "entry-native",
                &source,
                &[(worker, RequestRelation::New)],
                &closed,
            )
            .unwrap();
        assert!(admitted[0].1);
        assert_eq!(admitted[0].0.admission_epoch.as_deref(), Some("initial"));
        assert!(
            store
                .interaction_original_allowed(&admitted[0].0.id, 100)
                .unwrap()
        );
        let evidence = store
            .interaction_original_provenance(&admitted[0].0.id, 100)
            .unwrap();
        assert_eq!(evidence["input_ref"], refs[0].event());
        assert!(evidence["capture_order"].as_i64().unwrap() > 0);

        assert!(
            !store
                .interaction_admit_entry_plan(
                    entry,
                    100,
                    "entry-native",
                    &source,
                    &[(worker, RequestRelation::New)],
                    &closed
                )
                .unwrap()[0]
                .1
        );
        source.text = "A forged replacement".into();
        assert!(
            store
                .interaction_admit_entry_plan(
                    entry,
                    100,
                    "entry-native",
                    &source,
                    &[(worker, RequestRelation::New)],
                    &closed
                )
                .is_err()
        );
        let (operation, fresh) = store
            .interaction_prepare_creation(
                entry,
                100,
                "entry-native",
                refs[1].clone(),
                "Gamma",
                "A distinct canonical task",
                &serde_json::json!({"sources":[]}),
                2,
                &closed,
            )
            .unwrap();
        assert!(fresh);
        let id = operation["id"].as_str().unwrap();
        store.interaction_creation_start(id, 100).unwrap();
        store.interaction_creation_topic(id, 100, 77).unwrap();
        let target = SessionKey::new(100, Some(77));
        store
            .ensure_session(target, 100, &super::super::tests::defaults())
            .unwrap();
        store.set_session_codex_thread(target, "temporary").unwrap();
        store.clear_session_conversation(target).unwrap();
        let created = store
            .interaction_creation_finish(id, 100, "telegram:fixture")
            .unwrap();
        let original = store.interaction_source(&created.id, 100).unwrap();
        assert_eq!(original.channel, "conversation");
        assert!(original.update_id.is_none());
        assert!(original.origin.message_id.is_none());
        assert_eq!(original.event_id, refs[1].event());
        assert_eq!(
            store
                .interaction_creation_for_input(entry, 100, refs[1].clone())
                .unwrap()["phase"],
            "ready"
        );
        store
            .save_bot_state("stop_admission_epoch:100", "later-epoch")
            .unwrap();
        assert!(
            !store
                .interaction_original_allowed(&admitted[0].0.id, 100)
                .unwrap()
        );
        assert!(
            store
                .interaction_bind_epoch(&admitted[0].0.id, 100, "later-epoch")
                .is_err()
        );
        store
            .save_bot_state("stop_admission_epoch:100", "initial")
            .unwrap();
        store.clear_session_conversation(entry).unwrap();
        store
            .set_session_codex_thread(entry, "entry-native")
            .unwrap();
        assert!(
            !store
                .interaction_original_allowed(&admitted[0].0.id, 100)
                .unwrap()
        );
        let conn = store.conn.lock().unwrap();
        assert_eq!(
            conn.query_row("SELECT count(*) FROM incoming_updates", [], |r| r
                .get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert_eq!(
            conn.query_row(
                "SELECT typeof(input_update) FROM interaction_creations",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
            "text"
        );
        assert_eq!(
            conn.query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |r| r
                .get::<_, i64>(
                0
            ))
            .unwrap(),
            0
        );
    }
    #[test]
    fn receiver_capture_order_spans_channels_and_never_infers_legacy_order() {
        let (file, store, entry, binding) = setup();
        let first = "ffffffff-ffff-4fff-bfff-ffffffffffff";
        let second = "00000000-0000-4000-8000-000000000001";
        store
            .conversation_input_capture(100, entry, &binding, first, "Captured first")
            .unwrap();
        let update:crate::telegram::Update=serde_json::from_value(serde_json::json!({"update_id":500,"message":{"message_id":500,"message_thread_id":1,"from":{"id":100,"is_bot":false,"first_name":"Fixture"},"chat":{"id":100,"type":"private"},"text":"Captured between canonical originals"}})).unwrap();
        assert!(store.admit_update(&update).unwrap());
        store
            .conversation_input_capture(
                100,
                entry,
                &binding,
                second,
                "Captured last despite smaller client UUID",
            )
            .unwrap();
        assert!(!store.admit_update(&update).unwrap());
        let first_page = store
            .conversation_telegram_page(100, entry, 0, true)
            .unwrap();
        let captured_at = first_page["items"][0]["received_at"]
            .as_str()
            .unwrap()
            .to_owned();
        assert!(chrono::DateTime::parse_from_rfc3339(&captured_at).is_ok());
        assert_eq!(first_page["items"][0]["id"], "telegram:500");
        assert_eq!(
            store
                .conversation_telegram_page(100, entry, 0, true)
                .unwrap()["items"][0]["received_at"],
            captured_at
        );

        assert!(
            !store
                .conversation_input_capture(100, entry, &binding, first, "Captured first")
                .unwrap()
                .1
        );
        {
            let conn = store.conn.lock().unwrap();
            let mut q = conn
                .prepare("SELECT channel,event_id FROM original_ingress_order ORDER BY id")
                .unwrap();
            let order = q
                .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
                .unwrap()
                .collect::<rusqlite::Result<Vec<_>>>()
                .unwrap();
            assert_eq!(
                order,
                vec![
                    ("conversation".into(), first.into()),
                    ("telegram".into(), "500".into()),
                    ("conversation".into(), second.into())
                ]
            );
            conn.execute(
                "DELETE FROM original_ingress_order WHERE channel='telegram'",
                [],
            )
            .unwrap();
        }
        drop(store);
        let store = Store::open(file.path(), &[100], &super::super::tests::defaults()).unwrap();
        let conn = store.conn.lock().unwrap();
        assert_eq!(
            conn.query_row(
                "SELECT count(*) FROM incoming_updates WHERE update_id=500",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
        assert_eq!(
            conn.query_row(
                "SELECT count(*) FROM original_ingress_order WHERE channel='telegram'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
    }
}
