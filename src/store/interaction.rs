//! Canonical native output identity for the integrating conversation.
//!
//! Native completion records describe observed items, not which instructions a
//! worker incorporated. Request coverage and publication are separate states.
use super::*;
use anyhow::bail;

#[allow(clippy::too_many_arguments)]
pub(super) fn record_native_message_on(
    conn: &Connection,
    local_turn: i64,
    thread: &str,
    turn: &str,
    item: &str,
    phase: Option<&str>,
    text: &str,
) -> Result<bool> {
    if thread.is_empty() || turn.is_empty() || item.is_empty() {
        return Err(anyhow!("native output identity is incomplete"));
    }
    let bound: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM turns WHERE id=?1 AND native_thread_id=?2)",
        params![local_turn, thread],
        |r| r.get(0),
    )?;
    if !bound {
        return Err(anyhow!("native output does not match the owning execution"));
    }
    let inserted = conn.execute(
        "INSERT OR IGNORE INTO interaction_native_messages
             (native_thread,native_turn,item_id,local_turn,phase,text,observed_at,presentation_input)
             VALUES(?1,?2,?3,?4,?5,?6,?7,(SELECT p.source_input FROM conversation_returns p
               JOIN conversation_inputs c ON c.id=p.source_input AND c.human_id=p.human_id
               JOIN turns t ON t.id=?4 WHERE p.native_thread=?1 AND p.human_id=t.from_user_id))",
        params![thread, turn, item, local_turn, phase, text, now_string()],
    )? == 1;
    if !inserted {
        let matches: bool = conn.query_row(
            "SELECT phase IS ?4 AND text=?5 FROM interaction_native_messages
                 WHERE native_thread=?1 AND native_turn=?2 AND item_id=?3",
            params![thread, turn, item, phase, text],
            |r| r.get(0),
        )?;
        if !matches {
            return Err(anyhow!(
                "completed native item identity was reused with different content"
            ));
        }
    }
    super::topic_publications::native_on(conn, local_turn, thread, turn, item, text)?;
    Ok(inserted)
}

pub(super) fn admit_on(
    tx: &Connection,
    source: &crate::interaction::SourceInput,
    target: SessionKey,
    relation: crate::interaction::RequestRelation,
    instance: &str,
) -> Result<(crate::interaction::Instruction, bool)> {
    use crate::interaction::{Instruction, RequestRelation};
    super::matrix::unclaimed_on(
        tx,
        source.human_id,
        &source.channel,
        &source.event_id,
        source.update_id,
    )?;
    if source.text.trim().is_empty()
        || source.text.len() > 256 * 1024
        || source.channel.is_empty()
        || source.channel.len() > 64
        || source.event_id.is_empty()
        || source.event_id.len() > 256
        || source.human_id <= 0
        || source.origin.chat_id == 0
        || source.origin.thread_id < 0
        || source.origin.message_id.is_some_and(|id| id <= 0)
        || (source.origin.message_id.is_none()
            && !matches!(
                source.channel.as_str(),
                "conversation" | "local-session" | "local-session-result"
            ))
        || source.update_id.is_some_and(|id| id < 0)
        || relation.requests().len() > 64
        || source.target_sessions.is_empty()
        || source.target_sessions.len() > 64
        || source
            .target_sessions
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len()
            != source.target_sessions.len()
    {
        return Err(anyhow!("Invalid interaction input"));
    }
    {
        let reference = source
            .update_id
            .map(crate::interaction::InputReference::Telegram)
            .unwrap_or_else(|| {
                crate::interaction::InputReference::Conversation(source.event_id.clone())
            });
        let reserved:Option<(String,Option<i64>,i64)>=tx.query_row("SELECT phase,topic,entry_chat FROM interaction_creations WHERE human_id=?1 AND input_update=?2",params![source.human_id,reference],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
        if let Some((phase, topic, chat)) = reserved {
            if !matches!(phase.as_str(), "topic_known" | "ready")
                || topic != Some(target.thread_id)
                || chat != target.chat_id
                || source.target_sessions.len() != 1
            {
                return Err(anyhow!("Original input has an immutable creation plan"));
            }
        }
    }
    if let Some(transcription) = &source.transcription
        && source.channel != "conversation"
    {
        let update = source
            .update_id
            .context("Voice source lacks original ingress")?;
        let raw: String = tx.query_row(
            "SELECT payload_json FROM incoming_updates WHERE update_id=?1",
            [update],
            |r| r.get(0),
        )?;
        let original: crate::telegram::Update = serde_json::from_str(&raw)?;
        let (content, proof) = super::voice::content_on(tx, &original, source.human_id)?;
        let body = crate::interaction::explicit_selector(&content).map(|(_, body)| body);
        if proof.as_ref() != Some(transcription)
            || (source.text != content && body != Some(source.text.as_str()))
        {
            bail!("Voice input does not match committed interpretation");
        }
    }
    let mut source_epoch = None;
    if source.channel == "conversation" {
        let original = super::conversation_inputs::read_on(tx, &source.event_id, source.human_id)?;
        super::conversation_inputs::eligible_on(tx, &original)?;
        source_epoch = Some(original.epoch.clone());
        if source.update_id.is_some()
            || source.transcription
                != original
                    .voice
                    .as_ref()
                    .and_then(|v| v.transcription.clone())
            || source.origin.message_id.is_some()
            || original.entry.chat_id != source.origin.chat_id
            || original.entry.thread_id != source.origin.thread_id
            || original.text != source.text
        {
            bail!("Canonical routing source does not match its original");
        }
    }
    let id = Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!(
            "telecodex:interaction:{}:{}",
            source.channel, source.event_id
        )
        .as_bytes(),
    )
    .to_string();
    let raw_source = serde_json::to_string(source)?;
    let target_row: Option<(i64, Option<String>)> = tx.query_row(
        "SELECT s.id,s.codex_thread_id FROM sessions s JOIN users u ON u.tg_user_id=s.creator_user_id
         WHERE s.chat_id=?1 AND s.thread_id=?2 AND s.creator_user_id=?3 AND u.allowed=1
         AND (s.codex_thread_id IS NOT NULL OR s.force_fresh_thread=1)",
        params![target.chat_id,target.thread_id,source.human_id], |r| Ok((r.get(0)?,r.get(1)?)),
    ).optional()?;
    let (session, native) =
        target_row.context("Interaction target is not an owned bound session")?;
    if source.origin.message_id.is_none()
        && let Some(native) = native.as_deref()
    {
        super::conversation_inputs::select_return_on(
            tx,
            source.human_id,
            native,
            &source.event_id,
        )?;
    }
    if !source.target_sessions.contains(&session) {
        return Err(anyhow!(
            "Destination is outside the immutable selected target plan"
        ));
    }
    let existing: Option<(String, String, String)> = tx.query_row(
        "SELECT s.doc,i.doc,i.phase FROM interaction_sources s JOIN interaction_instructions i ON i.input_id=s.id
         WHERE s.id=?1 AND i.session_id=?2", params![id,session],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)),
    ).optional()?;
    if let Some((old_source, old_instruction, phase)) = existing {
        let mut previous: Instruction = serde_json::from_str(&old_instruction)?;
        if raw_source != old_source || previous.relation != relation {
            return Err(anyhow!("Interaction source identity is immutable"));
        }
        previous.phase = phase.as_str().try_into()?;
        return Ok((previous, false));
    }
    let old_source: Option<String> = tx
        .query_row(
            "SELECT doc FROM interaction_sources WHERE id=?",
            [&id],
            |r| r.get(0),
        )
        .optional()?;
    if old_source.as_ref().is_some_and(|old| old != &raw_source) {
        return Err(anyhow!("Interaction source identity is immutable"));
    }
    let instruction_id =
        Uuid::new_v5(&Uuid::NAMESPACE_URL, format!("{id}:{session}").as_bytes()).to_string();
    let requests = match &relation {
        RequestRelation::New => vec![instruction_id.clone()],
        RequestRelation::Amends(ids) | RequestRelation::Context(ids) => {
            if ids.is_empty()
                || ids.iter().collect::<std::collections::HashSet<_>>().len() != ids.len()
            {
                return Err(anyhow!(
                    "An amendment/context must identify distinct existing requests"
                ));
            }
            for request in ids {
                let valid:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM interaction_requests WHERE id=?1 AND session_id=?2 AND human_id=?3)",params![request,session,source.human_id],|r|r.get(0))?;
                if !valid {
                    return Err(anyhow!(
                        "Interaction relation crosses request ownership or destination"
                    ));
                }
            }
            ids.clone()
        }
    };
    let sequence: i64 = tx.query_row(
        "SELECT COALESCE(MAX(sequence),0)+1 FROM interaction_instructions WHERE session_id=?",
        [session],
        |r| r.get(0),
    )?;
    let instruction = Instruction {
        id: instruction_id,
        input_id: id.clone(),
        human_id: source.human_id,
        target_session: session,
        target,
        sequence,
        relation,
        requests,
        text: source.text.clone(),
        origin: source.origin.clone(),
        phase: crate::interaction::TransportPhase::Queued,
        admission_epoch: source_epoch,
        local_turn: None,
        native_thread: native,
        native_turn: None,
    };
    tx.execute("INSERT OR IGNORE INTO interaction_sources(id,human_id,update_id,channel,event_id,origin_chat,origin_thread,origin_message,doc,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",params![id,source.human_id,source.update_id,source.channel,source.event_id,source.origin.chat_id,source.origin.thread_id,source.origin.message_id,raw_source,now_string()])?;
    tx.execute("INSERT INTO interaction_instructions(id,input_id,session_id,sequence,phase,doc,instance_id) VALUES(?1,?2,?3,?4,'queued',?5,?6)",params![instruction.id,instruction.input_id,session,sequence,serde_json::to_string(&instruction)?,instance])?;
    if matches!(instruction.relation, RequestRelation::New) {
        tx.execute(
            "INSERT INTO interaction_requests(id,input_id,session_id,human_id) VALUES(?1,?2,?3,?4)",
            params![
                instruction.id,
                instruction.input_id,
                session,
                source.human_id
            ],
        )?;
    }
    for request in &instruction.requests {
        tx.execute(
            "INSERT INTO interaction_request_links(instruction_id,request_id) VALUES(?1,?2)",
            params![instruction.id, request],
        )?;
    }
    Ok((instruction, true))
}

pub(super) fn recover_on(conn: &Connection, id: &str, human: i64, instance: &str) -> Result<bool> {
    use crate::interaction::{Instruction, TransportPhase};
    let doc:Option<String>=conn.query_row("SELECT i.doc FROM interaction_instructions i JOIN interaction_sources s ON s.id=i.input_id JOIN sessions t ON t.id=i.session_id JOIN users u ON u.tg_user_id=t.creator_user_id WHERE i.id=?1 AND s.human_id=?2 AND u.tg_user_id=?2 AND u.allowed=1 AND i.phase='retained_unsent'",params![id,human],|r|r.get(0)).optional()?;
    let Some(doc) = doc else {
        return Ok(false);
    };
    let mut instruction: Instruction = serde_json::from_str(&doc)?;
    instruction.phase = TransportPhase::Queued;
    instruction.local_turn = None;
    instruction.native_turn = None;
    instruction.admission_epoch = None;
    conn.execute(
        "UPDATE interaction_instructions SET phase='queued',doc=?2,instance_id=?3 WHERE id=?1",
        params![id, serde_json::to_string(&instruction)?, instance],
    )?;
    Ok(true)
}

impl Store {
    pub(super) fn init_interaction(&self) -> Result<()> {
        self.conn
            .lock()
            .expect("store mutex poisoned")
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS interaction_native_messages(
                native_thread TEXT NOT NULL,
                native_turn TEXT NOT NULL,
                item_id TEXT NOT NULL,
                local_turn INTEGER NOT NULL REFERENCES turns(id) ON DELETE CASCADE,
                phase TEXT,
                text TEXT NOT NULL,
                observed_at TEXT NOT NULL,
                PRIMARY KEY(native_thread,native_turn,item_id)
             );
             CREATE INDEX IF NOT EXISTS interaction_native_messages_local
                ON interaction_native_messages(local_turn);
             CREATE TABLE IF NOT EXISTS interaction_sources(
                id TEXT PRIMARY KEY,human_id INTEGER NOT NULL,update_id INTEGER,
                channel TEXT NOT NULL,event_id TEXT NOT NULL,
                origin_chat INTEGER NOT NULL,origin_thread INTEGER NOT NULL,origin_message INTEGER,
                doc TEXT NOT NULL,created_at TEXT NOT NULL,UNIQUE(channel,event_id)
             );
             CREATE INDEX IF NOT EXISTS interaction_sources_reply ON interaction_sources(human_id,origin_chat,origin_thread,origin_message);
             CREATE TABLE IF NOT EXISTS interaction_instructions(
                id TEXT PRIMARY KEY,input_id TEXT NOT NULL REFERENCES interaction_sources(id),
                session_id INTEGER NOT NULL,sequence INTEGER NOT NULL,
                phase TEXT NOT NULL,doc TEXT NOT NULL,instance_id TEXT NOT NULL,
                UNIQUE(input_id,session_id),UNIQUE(session_id,sequence)
             );
             CREATE TABLE IF NOT EXISTS interaction_requests(
                id TEXT PRIMARY KEY,input_id TEXT NOT NULL REFERENCES interaction_sources(id),
                session_id INTEGER NOT NULL,human_id INTEGER NOT NULL
             );
             CREATE TABLE IF NOT EXISTS interaction_request_links(
                instruction_id TEXT NOT NULL REFERENCES interaction_instructions(id),
                request_id TEXT NOT NULL REFERENCES interaction_requests(id),
                PRIMARY KEY(instruction_id,request_id)
             );",
            )?;
        {
            let conn = self.conn.lock().expect("store mutex poisoned");
            let presentation:bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM pragma_table_info('interaction_native_messages') WHERE name='presentation_input')",[],|r|r.get(0))?;
            if !presentation {
                conn.execute_batch(
                    "ALTER TABLE interaction_native_messages ADD COLUMN presentation_input TEXT;",
                )?;
            }
            let old:bool=conn.query_row("SELECT [notnull] FROM pragma_table_info('interaction_sources') WHERE name='origin_message'",[],|r|r.get(0))?;
            if old {
                // Rebuild only this constraint; keep every source, dependency and
                // receipt identity. Startup holds the sole store connection.
                conn.pragma_update(None, "foreign_keys", false)?;
                let migration=conn.execute_batch("BEGIN IMMEDIATE;
                    CREATE TABLE interaction_sources_nullable(id TEXT PRIMARY KEY,human_id INTEGER NOT NULL,update_id INTEGER,channel TEXT NOT NULL,event_id TEXT NOT NULL,origin_chat INTEGER NOT NULL,origin_thread INTEGER NOT NULL,origin_message INTEGER,doc TEXT NOT NULL,created_at TEXT NOT NULL,UNIQUE(channel,event_id));
                    INSERT INTO interaction_sources_nullable SELECT * FROM interaction_sources;
                    DROP TABLE interaction_sources;
                    ALTER TABLE interaction_sources_nullable RENAME TO interaction_sources;
                    CREATE INDEX interaction_sources_reply ON interaction_sources(human_id,origin_chat,origin_thread,origin_message);
                    COMMIT;");
                if migration.is_err() {
                    let _ = conn.execute_batch("ROLLBACK");
                }
                conn.pragma_update(None, "foreign_keys", true)?;
                migration?;
            }
        }
        self.conn.lock().expect("store mutex poisoned").execute(
            "UPDATE interaction_instructions SET phase=CASE WHEN phase='queued' THEN 'retained_unsent' ELSE 'uncertain' END WHERE instance_id!=?1 AND phase IN ('queued','submitting','steering')",[&self.instance_id],
        )?;
        Ok(())
    }

    /// Preserve one immutable canonical item before any text-only presentation.
    /// Exact repeated observations are harmless; conflicting reuse is an error.
    #[allow(clippy::too_many_arguments)]
    pub fn record_native_message(
        &self,
        local_turn: i64,
        thread: &str,
        turn: &str,
        item: &str,
        phase: Option<&str>,
        text: &str,
    ) -> Result<bool> {
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction()?;
        let inserted = record_native_message_on(&tx, local_turn, thread, turn, item, phase, text)?;
        tx.commit()?;
        Ok(inserted)
    }
    /// Admit one source once and retain its immutable authorized return route.
    /// This does not submit native input or claim that a worker received it.
    pub fn interaction_admit(
        &self,
        source: &crate::interaction::SourceInput,
        target: SessionKey,
        relation: crate::interaction::RequestRelation,
    ) -> Result<(crate::interaction::Instruction, bool)> {
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction()?;
        let result = admit_on(&tx, source, target, relation, &self.instance_id)?;
        tx.commit()?;
        Ok(result)
    }

    pub fn interaction_instruction(
        &self,
        id: &str,
        human: i64,
    ) -> Result<Option<crate::interaction::Instruction>> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let raw:Option<(String,String)>=conn.query_row("SELECT i.doc,i.phase FROM interaction_instructions i JOIN interaction_sources s ON s.id=i.input_id WHERE i.id=?1 AND s.human_id=?2",params![id,human],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
        raw.map(|(doc, phase)| {
            let mut instruction: crate::interaction::Instruction = serde_json::from_str(&doc)?;
            instruction.phase = phase.as_str().try_into()?;
            Ok(instruction)
        })
        .transpose()
    }

    /// Reuse an immutable accepted routed execution when its exact physical
    /// writer is collected by a successor. Never migrate or re-accept input.
    pub fn interaction_attachment_execution(
        &self,
        key: SessionKey,
        human: i64,
        native: &str,
        physical: &str,
    ) -> Result<Option<(i64, Option<String>)>> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let mut query=conn.prepare("SELECT DISTINCT t.id FROM interaction_instructions i
            JOIN interaction_sources o ON o.id=i.input_id
            JOIN sessions s ON s.id=i.session_id
            JOIN turns t ON t.id=json_extract(i.doc,'$.local_turn') AND t.session_id=s.id
            JOIN users u ON u.tg_user_id=s.creator_user_id
            WHERE s.chat_id=?1 AND s.thread_id=?2 AND s.creator_user_id=?3 AND u.allowed=1
            AND o.human_id=?3 AND t.from_user_id=?3 AND t.native_thread_id=?4
            AND s.codex_thread_id=?4 AND s.force_fresh_thread=0 AND i.phase='accepted'
            AND json_extract(i.doc,'$.native_thread')=?4 AND json_extract(i.doc,'$.native_turn')=?5")?;
        let ids = query
            .query_map(
                params![key.chat_id, key.thread_id, human, native, physical],
                |r| r.get::<_, i64>(0),
            )?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        if ids.len() > 1 {
            return Err(anyhow!(
                "Native writer has ambiguous retained local executions"
            ));
        }
        let Some(local) = ids.first().copied() else {
            return Ok(None);
        };
        let raw: Option<String> = conn
            .query_row(
                "SELECT value FROM bot_state WHERE key=?1",
                [format!("execution_root:{local}")],
                |r| r.get(0),
            )
            .optional()?;
        let root:Option<String>=serde_json::from_str(&raw.context("Original execution presentation mode unavailable; inspect retained result without replay")?)?;
        if let Some(root) = &root {
            let valid:bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM interaction_instructions i JOIN interaction_sources o ON o.id=i.input_id WHERE i.id=?1 AND i.phase='accepted' AND o.human_id=?2 AND json_extract(i.doc,'$.local_turn')=?3)",params![root,human,local],|r|r.get(0))?;
            if !valid {
                return Err(anyhow!("Original routed execution root changed"));
            }
        }
        Ok(Some((local, root)))
    }

    /// Instructions associated with this actual owner execution, including
    /// routed input attached after a native goal/merge/Nexus launch.
    pub fn interaction_turn_inputs(
        &self,
        local: i64,
        human: i64,
    ) -> Result<Vec<crate::interaction::Instruction>> {
        let mut ids = {
            let conn = self.conn.lock().expect("store mutex poisoned");
            let mut stmt = conn.prepare("SELECT i.id FROM interaction_instructions i
                JOIN interaction_sources s ON s.id=i.input_id JOIN turns t ON t.session_id=i.session_id
                WHERE t.id=?1 AND t.from_user_id=?2 AND s.human_id=?2
                AND json_extract(i.doc,'$.local_turn')=?1 ORDER BY i.sequence DESC LIMIT 256")?;
            stmt.query_map(params![local, human], |r| r.get::<_, String>(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?
        };
        ids.reverse();
        ids.into_iter()
            .map(|id| {
                self.interaction_instruction(&id, human)?
                    .context("Execution instruction missing")
            })
            .collect()
    }

    #[cfg(test)]
    pub fn interaction_for_update(
        &self,
        update: i64,
        target: SessionKey,
        human: i64,
    ) -> Result<Option<crate::interaction::Instruction>> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let id:Option<String>=conn.query_row("SELECT i.id FROM interaction_instructions i JOIN interaction_sources s ON s.id=i.input_id JOIN sessions t ON t.id=i.session_id WHERE s.update_id=?1 AND t.chat_id=?2 AND t.thread_id=?3 AND s.human_id=?4",params![update,target.chat_id,target.thread_id,human],|r|r.get(0)).optional()?;
        drop(conn);
        id.map(|id| self.interaction_instruction(&id, human))
            .transpose()
            .map(Option::flatten)
    }

    /// Reply lookup is scoped to an actual source message and its original human.
    pub fn interaction_reply(
        &self,
        route: &crate::interaction::ReturnRoute,
        human: i64,
    ) -> Result<Vec<crate::interaction::Instruction>> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let mut stmt=conn.prepare("SELECT i.id FROM interaction_sources s JOIN interaction_instructions i ON i.input_id=s.id WHERE s.human_id=?1 AND s.origin_chat=?2 AND s.origin_thread=?3 AND s.origin_message=?4 ORDER BY s.created_at DESC,i.sequence DESC LIMIT 64")?;
        let ids = stmt
            .query_map(
                params![human, route.chat_id, route.thread_id, route.message_id],
                |r| r.get::<_, String>(0),
            )?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        drop(stmt);
        drop(conn);
        ids.iter()
            .map(|id| {
                self.interaction_instruction(id, human)?
                    .context("Interaction reply ownership changed")
            })
            .collect()
    }

    /// Link transport evidence to the actual existing owner execution. A terminal
    /// native turn never converts outstanding application requests into answers.
    pub fn interaction_transport(
        &self,
        id: &str,
        human: i64,
        local_turn: i64,
        phase: crate::interaction::TransportPhase,
        native_turn: Option<&str>,
    ) -> Result<crate::interaction::Instruction> {
        use crate::interaction::{Instruction, TransportPhase as P};
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction()?;
        let (doc,old):(String,String)=tx.query_row("SELECT i.doc,i.phase FROM interaction_instructions i JOIN interaction_sources s ON s.id=i.input_id WHERE i.id=?1 AND s.human_id=?2",params![id,human],|r|Ok((r.get(0)?,r.get(1)?)))?;
        let mut instruction: Instruction = serde_json::from_str(&doc)?;
        let old = P::try_from(old.as_str())?;
        let valid = old == phase
            || matches!(
                (old, phase),
                (
                    P::Queued,
                    P::Submitting | P::Steering | P::Cancelled | P::RetainedUnsent
                ) | (
                    P::Submitting | P::Steering,
                    P::Accepted | P::Rejected | P::Uncertain
                ) | (P::Uncertain, P::Accepted | P::Rejected)
            );
        if !valid {
            return Err(anyhow!(
                "Interaction transport cannot skip admission or replay uncertainty"
            ));
        }
        let binding: Option<(i64, i64, Option<String>)> = tx
            .query_row(
                "SELECT session_id,from_user_id,native_thread_id FROM turns WHERE id=?",
                [local_turn],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        let (session, owner, native) = binding.context("Interaction execution missing")?;
        if session != instruction.target_session
            || owner != human
            || instruction
                .local_turn
                .is_some_and(|previous| previous != local_turn)
        {
            return Err(anyhow!("Interaction execution ownership changed"));
        }
        if matches!(phase, P::Submitting | P::Steering) {
            let allowed:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM sessions s JOIN users u ON u.tg_user_id=s.creator_user_id WHERE s.id=?1 AND u.tg_user_id=?2 AND u.allowed=1)",params![session,human],|r|r.get(0))?;
            if !allowed {
                return Err(anyhow!("Interaction execution is no longer admitted"));
            }
        }
        if phase == P::Accepted && (native.is_none() || native_turn.is_none_or(str::is_empty)) {
            return Err(anyhow!(
                "Native acceptance requires exact thread and turn evidence"
            ));
        }
        if old == P::Accepted
            && (instruction.native_thread != native
                || instruction.native_turn.as_deref() != native_turn)
        {
            return Err(anyhow!("Accepted interaction execution is immutable"));
        }
        instruction.phase = phase;
        instruction.local_turn = Some(local_turn);
        if native.is_some() {
            instruction.native_thread = native;
        }
        if let Some(turn) = native_turn {
            instruction.native_turn = Some(turn.to_owned());
        }
        tx.execute(
            "UPDATE interaction_instructions SET phase=?2,doc=?3 WHERE id=?1",
            params![id, phase.as_str(), serde_json::to_string(&instruction)?],
        )?;
        if phase == P::Accepted {
            super::topic_publications::accepted_on(&tx, &instruction)?;
        }
        tx.commit()?;
        Ok(instruction)
    }

    /// Withdraw only input that cannot have crossed the native submission fence.
    pub fn interaction_cancel_unsent(&self, id: &str, human: i64) -> Result<bool> {
        use crate::interaction::{Instruction, TransportPhase};
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction()?;
        let doc: Option<String> = tx.query_row(
            "SELECT i.doc FROM interaction_instructions i JOIN interaction_sources s ON s.id=i.input_id JOIN users u ON u.tg_user_id=s.human_id WHERE i.id=?1 AND s.human_id=?2 AND u.allowed=1 AND i.phase IN ('queued','retained_unsent')",
            params![id,human], |r| r.get(0),
        ).optional()?;
        let Some(doc) = doc else {
            return Ok(false);
        };
        let mut instruction: Instruction = serde_json::from_str(&doc)?;
        instruction.phase = TransportPhase::Cancelled;
        tx.execute(
            "UPDATE interaction_instructions SET phase='cancelled',doc=?2 WHERE id=?1",
            params![id, serde_json::to_string(&instruction)?],
        )?;
        tx.commit()?;
        Ok(true)
    }

    pub fn interaction_recover(&self, id: &str, human: i64) -> Result<bool> {
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction()?;
        let changed = recover_on(&tx, id, human, &self.instance_id)?;
        tx.commit()?;
        Ok(changed)
    }

    /// Bind a queued dispatch to its captured ingress generation. Automatic
    /// continuations keep this binding; explicit known-unsent recovery clears it.
    pub fn interaction_bind_epoch(&self, id: &str, human: i64, epoch: &str) -> Result<()> {
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction()?;
        let raw: String = tx.query_row(
            "SELECT i.doc FROM interaction_instructions i JOIN interaction_sources s ON s.id=i.input_id
             WHERE i.id=?1 AND s.human_id=?2 AND i.phase='queued'",
            params![id, human], |r| r.get(0),
        )?;
        let mut i: crate::interaction::Instruction = serde_json::from_str(&raw)?;
        if i.admission_epoch.as_deref().is_some_and(|old| old != epoch) {
            return Err(anyhow!(
                "Queued input cannot adopt a later admission generation"
            ));
        }
        i.admission_epoch = Some(epoch.to_owned());
        tx.execute(
            "UPDATE interaction_instructions SET doc=?2 WHERE id=?1",
            params![id, serde_json::to_string(&i)?],
        )?;
        tx.commit()?;
        Ok(())
    }

    #[cfg(test)]
    pub fn interaction_recent(
        &self,
        human: i64,
        target: SessionKey,
    ) -> Result<Vec<crate::interaction::Instruction>> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let mut stmt=conn.prepare("SELECT i.id FROM interaction_instructions i JOIN interaction_sources s ON s.id=i.input_id JOIN sessions t ON t.id=i.session_id WHERE s.human_id=?1 AND t.chat_id=?2 AND t.thread_id=?3 ORDER BY i.sequence DESC LIMIT 128")?;
        let ids = stmt
            .query_map(params![human, target.chat_id, target.thread_id], |r| {
                r.get::<_, String>(0)
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        drop(stmt);
        drop(conn);
        ids.iter()
            .map(|id| {
                self.interaction_instruction(id, human)?
                    .context("Interaction ownership changed")
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn execution(store: &Store, thread: &str) -> i64 {
        let key = SessionKey::new(100, Some(7));
        let session = store
            .ensure_session(key, 100, &super::super::tests::defaults())
            .unwrap();
        let id = store
            .record_turn_started(
                session.id,
                &TurnRequest {
                    session_key: key,
                    from_user_id: 100,
                    prompt: "A request".into(),
                    runtime_instructions: None,
                    attachments: vec![],
                    review_mode: None,
                    override_search_mode: None,
                },
            )
            .unwrap();
        store.record_turn_native_thread(id, thread).unwrap();
        id
    }

    #[test]
    fn distinct_items_survive_one_turn_and_repeated_observations() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let store = Store::open(file.path(), &[100], &super::super::tests::defaults()).unwrap();
        let local = execution(&store, "thread");
        assert!(
            store
                .record_native_message(
                    local,
                    "thread",
                    "turn",
                    "A",
                    Some("final_answer"),
                    "First answer"
                )
                .unwrap()
        );
        assert!(
            !store
                .record_native_message(
                    local,
                    "thread",
                    "turn",
                    "A",
                    Some("final_answer"),
                    "First answer"
                )
                .unwrap()
        );
        assert!(
            store
                .record_native_message(
                    local,
                    "thread",
                    "turn",
                    "B",
                    Some("final_answer"),
                    "Second answer"
                )
                .unwrap()
        );
        assert!(
            store
                .record_native_message(
                    local,
                    "thread",
                    "turn",
                    "A",
                    Some("final_answer"),
                    "Conflicting answer"
                )
                .is_err()
        );
        drop(store);
        let store = Store::open(file.path(), &[100], &super::super::tests::defaults()).unwrap();
        assert!(
            !store
                .record_native_message(
                    local,
                    "thread",
                    "turn",
                    "A",
                    Some("final_answer"),
                    "First answer"
                )
                .unwrap()
        );
        let count: i64 = store
            .conn
            .lock()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM interaction_native_messages",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn unrelated_thread_and_incomplete_identity_cannot_be_recorded() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let store = Store::open(file.path(), &[100], &super::super::tests::defaults()).unwrap();
        let local = execution(&store, "thread");
        assert!(
            store
                .record_native_message(local, "other", "turn", "A", None, "answer")
                .is_err()
        );
        assert!(
            store
                .record_native_message(local, "thread", "turn", "", None, "answer")
                .is_err()
        );
    }
    fn target(store: &Store, topic: i64, human: i64) -> SessionRecord {
        let key = SessionKey::new(100, Some(topic));
        store
            .ensure_session(key, human, &super::super::tests::defaults())
            .unwrap();
        store
            .set_session_codex_thread(key, &format!("native-{topic}"))
            .unwrap();
        store.get_session(key).unwrap().unwrap()
    }
    fn input(event: i64, targets: &[i64]) -> crate::interaction::SourceInput {
        crate::interaction::SourceInput {
            transcription: None,
            channel: "telegram:test-bot".into(),
            event_id: event.to_string(),
            update_id: Some(event),
            human_id: 100,
            target_sessions: targets.to_vec(),
            origin: crate::interaction::ReturnRoute {
                chat_id: 100,
                thread_id: 1,
                message_id: Some(event),
            },
            text: format!("Instruction {event}"),
        }
    }
    fn local(store: &Store, target: &SessionRecord) -> i64 {
        let turn = store
            .record_turn_started(
                target.id,
                &TurnRequest {
                    session_key: target.key,
                    from_user_id: 100,
                    prompt: "Work".into(),
                    runtime_instructions: None,
                    attachments: vec![],
                    review_mode: None,
                    override_search_mode: None,
                },
            )
            .unwrap();
        store
            .record_turn_native_thread(turn, target.codex_thread_id.as_deref().unwrap())
            .unwrap();
        turn
    }
    #[test]
    fn application_requests_remain_distinct_inside_one_execution() {
        use crate::interaction::{RequestRelation as R, TransportPhase as P};
        let file = tempfile::NamedTempFile::new().unwrap();
        let store = Store::open(file.path(), &[100], &super::super::tests::defaults()).unwrap();
        let session = target(&store, 7, 100);
        let source = input(1, &[session.id]);
        let (a, fresh) = store
            .interaction_admit(&source, session.key, R::New)
            .unwrap();
        assert!(fresh);
        let (b, _) = store
            .interaction_admit(&input(2, &[session.id]), session.key, R::New)
            .unwrap();
        let (correction, _) = store
            .interaction_admit(
                &input(3, &[session.id]),
                session.key,
                R::Amends(a.requests.clone()),
            )
            .unwrap();
        assert_eq!((a.sequence, b.sequence, correction.sequence), (1, 2, 3));
        assert_eq!(correction.requests, a.requests);
        assert_ne!(a.requests, b.requests);
        let execution = local(&store, &session);
        for instruction in [&a, &b] {
            store
                .interaction_transport(&instruction.id, 100, execution, P::Submitting, None)
                .unwrap();
            store
                .interaction_transport(
                    &instruction.id,
                    100,
                    execution,
                    P::Accepted,
                    Some("physical-turn"),
                )
                .unwrap();
        }
        store
            .record_turn_finished(execution, "completed", Some("An answer for A only"))
            .unwrap();
        assert_eq!(
            store
                .interaction_instruction(&b.id, 100)
                .unwrap()
                .unwrap()
                .phase,
            P::Accepted
        );
        let (again, fresh) = store
            .interaction_admit(&source, session.key, R::New)
            .unwrap();
        assert!(!fresh);
        assert_eq!(again.phase, P::Accepted);
        assert_eq!(
            store
                .interaction_for_update(3, session.key, 100)
                .unwrap()
                .unwrap()
                .id,
            correction.id
        );
        assert_eq!(
            store.interaction_reply(&source.origin, 100).unwrap()[0].id,
            a.id
        );
        assert!(
            store
                .interaction_reply(&source.origin, 200)
                .unwrap()
                .is_empty()
        );
        let before = store.interaction_recent(100, session.key).unwrap();
        let after = store.interaction_recent(100, session.key).unwrap();
        assert_eq!(before.len(), after.len());
        assert_eq!(after.len(), 3); // Status reads create no instruction/revision.
    }

    #[test]
    fn source_return_plan_and_request_ownership_are_immutable() {
        use crate::interaction::RequestRelation as R;
        let file = tempfile::NamedTempFile::new().unwrap();
        let store =
            Store::open(file.path(), &[100, 200], &super::super::tests::defaults()).unwrap();
        let a = target(&store, 7, 100);
        let b = target(&store, 8, 100);
        let other = target(&store, 9, 200);
        let source = input(1, &[a.id]);
        let (instruction, _) = store.interaction_admit(&source, a.key, R::New).unwrap();
        let mut changed = source.clone();
        changed.origin.message_id = Some(999);
        assert!(store.interaction_admit(&changed, a.key, R::New).is_err());
        assert!(store.interaction_admit(&source, b.key, R::New).is_err());
        changed = source.clone();
        changed.target_sessions.push(b.id);
        assert!(store.interaction_admit(&changed, b.key, R::New).is_err());
        assert!(
            store
                .interaction_admit(&input(2, &[other.id]), other.key, R::New)
                .is_err()
        );
        assert!(
            store
                .interaction_admit(&input(3, &[b.id]), b.key, R::Amends(instruction.requests))
                .is_err()
        );
        let explicit = input(4, &[a.id, b.id]);
        assert!(store.interaction_admit(&explicit, a.key, R::New).unwrap().1);
        assert!(store.interaction_admit(&explicit, b.key, R::New).unwrap().1);
    }

    #[test]
    fn restart_preserves_uncertainty_and_only_unsent_work_can_be_recovered() {
        use crate::interaction::{RequestRelation as R, TransportPhase as P};
        let file = tempfile::NamedTempFile::new().unwrap();
        let store = Store::open(file.path(), &[100], &super::super::tests::defaults()).unwrap();
        let session = target(&store, 7, 100);
        let source = input(1, &[session.id]);
        let (unsent, _) = store
            .interaction_admit(&source, session.key, R::New)
            .unwrap();
        let (unknown, _) = store
            .interaction_admit(&input(2, &[session.id]), session.key, R::New)
            .unwrap();
        let execution = local(&store, &session);
        store
            .interaction_transport(&unknown.id, 100, execution, P::Steering, None)
            .unwrap();
        drop(store);
        let store = Store::open(file.path(), &[100], &super::super::tests::defaults()).unwrap();
        let (again, fresh) = store
            .interaction_admit(&source, session.key, R::New)
            .unwrap();
        assert!(!fresh);
        assert_eq!(again.phase, P::RetainedUnsent);
        assert_eq!(
            store
                .interaction_instruction(&unknown.id, 100)
                .unwrap()
                .unwrap()
                .phase,
            P::Uncertain
        );
        assert!(!store.interaction_recover(&unknown.id, 100).unwrap());
        assert!(store.interaction_recover(&unsent.id, 100).unwrap());
        assert!(!store.interaction_recover(&unsent.id, 100).unwrap());
        let accepted = store
            .interaction_transport(
                &unknown.id,
                100,
                execution,
                P::Accepted,
                Some("actual-turn"),
            )
            .unwrap();
        assert_eq!(accepted.phase, P::Accepted);
        assert!(
            store
                .interaction_transport(
                    &unknown.id,
                    100,
                    execution,
                    P::Accepted,
                    Some("different-turn")
                )
                .is_err()
        );
    }
    #[test]
    fn cancellation_is_before_submission_and_does_not_affect_other_requests() {
        use crate::interaction::{RequestRelation as R, TransportPhase as P};
        let file = tempfile::NamedTempFile::new().unwrap();
        let store = Store::open(file.path(), &[100], &super::super::tests::defaults()).unwrap();
        let session = target(&store, 7, 100);
        let (a, _) = store
            .interaction_admit(&input(1, &[session.id]), session.key, R::New)
            .unwrap();
        let (b, _) = store
            .interaction_admit(&input(2, &[session.id]), session.key, R::New)
            .unwrap();
        assert!(!store.interaction_cancel_unsent(&a.id, 200).unwrap());
        assert!(store.interaction_cancel_unsent(&a.id, 100).unwrap());
        let execution = local(&store, &session);
        assert!(
            store
                .interaction_transport(&a.id, 100, execution, P::Submitting, None)
                .is_err()
        );
        store
            .interaction_transport(&b.id, 100, execution, P::Submitting, None)
            .unwrap();
        assert!(!store.interaction_cancel_unsent(&b.id, 100).unwrap());
        assert_eq!(
            store
                .interaction_instruction(&a.id, 100)
                .unwrap()
                .unwrap()
                .phase,
            P::Cancelled
        );
        assert_eq!(
            store
                .interaction_instruction(&b.id, 100)
                .unwrap()
                .unwrap()
                .phase,
            P::Submitting
        );
    }
    #[test]
    fn nullable_origin_migration_preserves_existing_sources_and_dependencies() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let defaults = super::super::tests::defaults();
        let store = Store::open(file.path(), &[100], &defaults).unwrap();
        let worker = target(&store, 7, 100);
        let source = input(20, &[worker.id]);
        let (instruction, _) = store
            .interaction_admit(
                &source,
                worker.key,
                crate::interaction::RequestRelation::New,
            )
            .unwrap();
        let original = serde_json::to_string(&source).unwrap();
        {
            let conn = store.conn.lock().unwrap();
            let schema:String=conn.query_row("SELECT sql FROM sqlite_master WHERE type='table' AND name='interaction_sources'",[],|r|r.get(0)).unwrap();
            let old = schema
                .replacen("interaction_sources(", "interaction_sources_old(", 1)
                .replace(
                    "origin_message INTEGER,",
                    "origin_message INTEGER NOT NULL,",
                );
            // This fixture constructs the historical schema, before native
            // request retirement triggers referenced interaction_sources.
            conn.execute_batch("DROP TRIGGER IF EXISTS native_request_binding_retired; DROP TRIGGER IF EXISTS native_request_entry_deleted; DROP TRIGGER IF EXISTS native_request_grant_retired;").unwrap();
            conn.pragma_update(None, "foreign_keys", false).unwrap();
            conn.execute_batch(&format!("BEGIN IMMEDIATE; {old}; INSERT INTO interaction_sources_old SELECT * FROM interaction_sources; DROP TABLE interaction_sources; ALTER TABLE interaction_sources_old RENAME TO interaction_sources; CREATE INDEX interaction_sources_reply ON interaction_sources(human_id,origin_chat,origin_thread,origin_message); COMMIT;")).unwrap();
            conn.pragma_update(None, "foreign_keys", true).unwrap();
            assert!(conn.query_row("SELECT [notnull] FROM pragma_table_info('interaction_sources') WHERE name='origin_message'",[],|r|r.get::<_,bool>(0)).unwrap());
        }
        drop(store);
        let store = Store::open(file.path(), &[100], &defaults).unwrap();
        assert_eq!(
            store.interaction_source(&instruction.id, 100).unwrap(),
            source
        );
        let conn = store.conn.lock().unwrap();
        assert_eq!(
            conn.query_row("SELECT doc FROM interaction_sources", [], |r| r
                .get::<_, String>(0))
                .unwrap(),
            original
        );
        assert_eq!(
            conn.query_row(
                "SELECT count(*) FROM interaction_requests WHERE input_id=?1",
                [instruction.input_id],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
        assert_eq!(
            conn.query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |r| r
                .get::<_, i64>(
                0
            ))
            .unwrap(),
            0
        );
        assert!(!conn.query_row("SELECT [notnull] FROM pragma_table_info('interaction_sources') WHERE name='origin_message'",[],|r|r.get::<_,bool>(0)).unwrap());
        assert!(
            conn.query_row("PRAGMA foreign_keys", [], |r| r.get::<_, bool>(0))
                .unwrap()
        );
    }
}
