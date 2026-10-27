//! Attributed, bounded local session dependencies, separate from human speech.
use super::*;
use crate::interaction::{Instruction, RequestRelation, SourceInput};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionRequest {
    pub id: String,
    pub human: i64,
    pub caller: SessionKey,
    pub caller_native: String,
    pub caller_turn: String,
    pub parent_instruction: String,
    pub target_instruction: String,
    pub epoch: String,
    pub return_instruction: Option<String>,
    pub phase: String,
}

pub(super) fn author_on(conn: &Connection, input: &str) -> Result<Option<Value>> {
    let raw: Option<String> = conn
        .query_row(
            "SELECT doc FROM interaction_session_authors WHERE input_id=?",
            [input],
            |r| r.get(0),
        )
        .optional()?;
    raw.map(|s| Ok(serde_json::from_str(&s)?)).transpose()
}

fn instruction_on(conn: &Connection, id: &str, human: i64) -> Result<Instruction> {
    let (raw,phase):(String,String)=conn.query_row("SELECT i.doc,i.phase FROM interaction_instructions i JOIN interaction_sources s ON s.id=i.input_id WHERE i.id=?1 AND s.human_id=?2",params![id,human],|r|Ok((r.get(0)?,r.get(1)?)))?;
    let mut i: Instruction = serde_json::from_str(&raw)?;
    i.phase = phase.as_str().try_into()?;
    Ok(i)
}

fn current_epoch(conn: &Connection, human: i64) -> Result<String> {
    Ok(conn
        .query_row(
            "SELECT value FROM bot_state WHERE key=?",
            [format!("stop_admission_epoch:{human}")],
            |r| r.get(0),
        )
        .optional()?
        .unwrap_or_else(|| "initial".into()))
}

fn save_epoch(conn: &Connection, i: &mut Instruction, epoch: &str) -> Result<()> {
    i.admission_epoch = Some(epoch.to_owned());
    conn.execute(
        "UPDATE interaction_instructions SET doc=?2 WHERE id=?1",
        params![i.id, serde_json::to_string(i)?],
    )?;
    Ok(())
}

impl Store {
    pub(super) fn init_session_requests(&self) -> Result<()> {
        self.conn.lock().expect("store mutex poisoned").execute_batch(
            "CREATE TABLE IF NOT EXISTS interaction_session_requests(id TEXT PRIMARY KEY,human_id INTEGER NOT NULL,caller_chat INTEGER NOT NULL,caller_topic INTEGER NOT NULL,parent_instruction TEXT NOT NULL,target_instruction TEXT NOT NULL UNIQUE,phase TEXT NOT NULL,doc TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS interaction_session_authors(input_id TEXT PRIMARY KEY REFERENCES interaction_sources(id),doc TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS interaction_session_withdrawals(parent_instruction TEXT PRIMARY KEY);"
        )?;
        Ok(())
    }

    pub fn interaction_author(&self, instruction: &str, human: i64) -> Result<Option<Value>> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let i = instruction_on(&conn, instruction, human)?;
        author_on(&conn, &i.input_id)
    }

    /// The caller's accepted human instruction is the causal grant. One level,
    /// at most four dependencies per original instruction, and no self-addressing.
    #[allow(clippy::too_many_arguments)]
    pub fn interaction_session_request(
        &self,
        caller: SessionKey,
        human: i64,
        native: &str,
        turn: &str,
        parent: &str,
        client: &str,
        target: SessionKey,
        text: &str,
        closed: &tokio_util::sync::CancellationToken,
    ) -> Result<(SessionRequest, Instruction, bool)> {
        if caller == target
            || text.trim().is_empty()
            || text.len() > 16000
            || client.is_empty()
            || client.len() > 64
            || !client
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        {
            return Err(anyhow!("Invalid local session request"));
        }
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction()?;
        let p = instruction_on(&tx, parent, human)?;
        let caller_owned:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM sessions s JOIN users u ON u.tg_user_id=s.creator_user_id JOIN turns t ON t.session_id=s.id WHERE s.id=?1 AND s.chat_id=?2 AND s.thread_id=?3 AND s.creator_user_id=?4 AND u.allowed=1 AND s.codex_thread_id=?5 AND t.id=?6 AND t.native_thread_id=?5 AND t.status='running')",params![p.target_session,caller.chat_id,caller.thread_id,human,native,p.local_turn],|r|r.get(0))?;
        let withdrawn:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM interaction_session_withdrawals WHERE parent_instruction=?)",[parent],|r|r.get(0))?;
        if !caller_owned
            || withdrawn
            || p.target != caller
            || p.phase != crate::interaction::TransportPhase::Accepted
            || p.native_thread.as_deref() != Some(native)
            || p.native_turn.as_deref() != Some(turn)
            || author_on(&tx, &p.input_id)?.is_some()
        {
            return Err(anyhow!(
                "Local request has no current accepted human parent"
            ));
        }
        // An interview receives historical evidence, not authority to resume or
        // steer the work it is explaining. Enforce this at durable admission,
        // independently of the interview model's output or role prompt.
        let source_protected:bool=tx.query_row("SELECT EXISTS(
            SELECT 1 FROM interaction_enrollments e JOIN interaction_creations c ON c.id=e.creation_id
            JOIN json_each(c.doc,'$.context.sources') source
            WHERE e.session_id=?1 AND e.human_id=?2 AND json_extract(c.doc,'$.context.purpose')='interview'
            AND json_extract(source.value,'$.source_session')=(SELECT id FROM sessions WHERE chat_id=?3 AND thread_id=?4))",
            params![p.target_session,human,target.chat_id,target.thread_id],|r|r.get(0))?;
        if source_protected {
            return Err(anyhow!(
                "An interview cannot activate its pinned source work"
            ));
        }
        let epoch = p
            .admission_epoch
            .as_deref()
            .context("Parent admission generation unavailable")?;
        if current_epoch(&tx, human)? != epoch || closed.is_cancelled() {
            return Err(anyhow!("Parent admission retired or closed"));
        }
        let target_id:i64=tx.query_row("SELECT s.id FROM sessions s JOIN users u ON u.tg_user_id=s.creator_user_id WHERE s.chat_id=?1 AND s.thread_id=?2 AND s.creator_user_id=?3 AND u.allowed=1 AND s.codex_thread_id IS NOT NULL",params![target.chat_id,target.thread_id,human],|r|r.get(0))?;
        let raw: String = tx.query_row(
            "SELECT doc FROM interaction_sources WHERE id=?",
            [&p.input_id],
            |r| r.get(0),
        )?;
        let original: SourceInput = serde_json::from_str(&raw)?;
        let id = Uuid::new_v5(
            &Uuid::NAMESPACE_URL,
            format!(
                "local-session-request:{}:{parent}:{client}",
                p.target_session
            )
            .as_bytes(),
        )
        .to_string();
        let source = SourceInput {
            transcription: None,
            channel: "local-session".into(),
            event_id: id.clone(),
            update_id: None,
            human_id: human,
            target_sessions: vec![target_id],
            origin: original.origin,
            text: text.to_owned(),
        };
        let existing: Option<(String, String)> = tx
            .query_row(
                "SELECT doc,phase FROM interaction_session_requests WHERE id=? AND human_id=?",
                params![id, human],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        // The integrator has already handed this exact original to the target.
        // A second worker must not manufacture a parallel task and callback.
        let already_routed: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM interaction_instructions WHERE input_id=?1 AND session_id=?2)",
            params![p.input_id, target_id], |r| r.get(0),
        )?;
        if existing.is_none() && already_routed {
            return Err(anyhow!(
                "Destination already received this original input; no duplicate dependency was created"
            ));
        }
        let count: i64 = tx.query_row(
            "SELECT count(*) FROM interaction_session_requests WHERE parent_instruction=?",
            [parent],
            |r| r.get(0),
        )?;
        if existing.is_none() && count >= 4 {
            return Err(anyhow!("Human parent dependency budget exhausted"));
        }
        let (mut i, fresh) = super::interaction::admit_on(
            &tx,
            &source,
            target,
            RequestRelation::New,
            &self.instance_id,
        )?;
        if let Some((raw, phase)) = existing {
            let mut call: SessionRequest = serde_json::from_str(&raw)?;
            call.phase = phase;
            if call.caller_native != native
                || call.caller_turn != turn
                || call.target_instruction != i.id
            {
                return Err(anyhow!("Local request identity is immutable"));
            }
            tx.commit()?;
            return Ok((call, i, false));
        }
        if !fresh {
            return Err(anyhow!(
                "Unattributed local source cannot become a session request"
            ));
        }
        save_epoch(&tx, &mut i, epoch)?;
        let call = SessionRequest {
            id: id.clone(),
            human,
            caller,
            caller_native: native.into(),
            caller_turn: turn.into(),
            parent_instruction: parent.into(),
            target_instruction: i.id.clone(),
            epoch: epoch.into(),
            return_instruction: None,
            phase: "requested".into(),
        };
        let author = json!({"kind":"session_request","session":caller,"native_thread":native,"native_turn":turn,"parent_instruction":parent,"local_request":id,"authorized_human":human});
        tx.execute(
            "INSERT INTO interaction_session_authors VALUES(?1,?2)",
            params![i.input_id, author.to_string()],
        )?;
        tx.execute(
            "INSERT INTO interaction_session_requests VALUES(?1,?2,?3,?4,?5,?6,'requested',?7)",
            params![
                id,
                human,
                caller.chat_id,
                caller.thread_id,
                parent,
                i.id,
                serde_json::to_string(&call)?
            ],
        )?;
        if closed.is_cancelled() {
            return Err(anyhow!("Local request closed before commit"));
        }
        tx.commit()?;
        Ok((call, i, true))
    }

    pub fn interaction_session_requests(
        &self,
        caller: SessionKey,
        human: i64,
        native: &str,
    ) -> Result<Vec<SessionRequest>> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let owned:bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM sessions s JOIN users u ON u.tg_user_id=s.creator_user_id WHERE s.chat_id=?1 AND s.thread_id=?2 AND s.creator_user_id=?3 AND u.allowed=1 AND s.codex_thread_id=?4)",params![caller.chat_id,caller.thread_id,human,native],|r|r.get(0))?;
        if !owned {
            return Err(anyhow!("Local caller binding changed"));
        }
        let mut q=conn.prepare("SELECT doc,phase FROM interaction_session_requests WHERE human_id=?1 AND caller_chat=?2 AND caller_topic=?3 ORDER BY rowid DESC LIMIT 16")?;
        q.query_map(params![human, caller.chat_id, caller.thread_id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })?
        .map(|r| {
            let (raw, phase) = r?;
            let mut call: SessionRequest = serde_json::from_str(&raw)?;
            call.phase = phase;
            Ok(call)
        })
        .collect()
    }

    pub fn interaction_session_input_allowed(&self, id: &str, human: i64) -> Result<bool> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let i = instruction_on(&conn, id, human)?;
        let Some(author) = author_on(&conn, &i.input_id)? else {
            return Ok(true);
        };
        let call = author["local_request"]
            .as_str()
            .context("Missing local request provenance")?;
        let phase: Option<String> = conn
            .query_row(
                "SELECT phase FROM interaction_session_requests WHERE id=?1 AND human_id=?2",
                params![call, human],
                |r| r.get(0),
            )
            .optional()?;
        Ok(matches!(
            (author["kind"].as_str(), phase.as_deref()),
            (Some("session_request"), Some("requested"))
                | (Some("session_result"), Some("returned"))
        ))
    }

    /// Withdraw future dependency continuations synchronously with scoped controls.
    pub fn interaction_withdraw_session_requests(&self, caller: SessionKey) -> Result<()> {
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction()?;
        tx.execute("INSERT OR IGNORE INTO interaction_session_withdrawals SELECT i.id FROM interaction_instructions i JOIN sessions s ON s.id=i.session_id WHERE s.chat_id=?1 AND s.thread_id=?2 AND i.phase='accepted'",params![caller.chat_id,caller.thread_id])?;
        tx.execute("UPDATE interaction_session_requests SET phase='withdrawn' WHERE caller_chat=?1 AND caller_topic=?2 AND (phase='requested' OR (phase='returned' AND EXISTS(SELECT 1 FROM interaction_instructions i WHERE i.id=json_extract(interaction_session_requests.doc,'$.return_instruction') AND i.phase IN ('queued','submitting','steering'))))",params![caller.chat_id,caller.thread_id])?;
        tx.execute("UPDATE interaction_instructions SET phase='retained_unsent' WHERE phase='queued' AND input_id IN (SELECT a.input_id FROM interaction_session_authors a JOIN interaction_session_requests c ON json_extract(a.doc,'$.local_request')=c.id WHERE c.caller_chat=?1 AND c.caller_topic=?2 AND c.phase='withdrawn' AND json_extract(a.doc,'$.kind')='session_result')",params![caller.chat_id,caller.thread_id])?;
        tx.commit()?;
        Ok(())
    }

    pub fn interaction_session_selected(
        &self,
        caller: SessionKey,
        human: i64,
        native: &str,
        id: &str,
    ) -> Result<SessionRequest> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let (raw,phase):(String,String)=conn.query_row("SELECT c.doc,c.phase FROM interaction_session_requests c JOIN sessions s ON s.chat_id=c.caller_chat AND s.thread_id=c.caller_topic JOIN users u ON u.tg_user_id=s.creator_user_id WHERE c.id=?1 AND c.human_id=?2 AND s.creator_user_id=?2 AND u.allowed=1 AND s.chat_id=?3 AND s.thread_id=?4 AND s.codex_thread_id=?5",params![id,human,caller.chat_id,caller.thread_id,native],|r|Ok((r.get(0)?,r.get(1)?)))?;
        let mut call: SessionRequest = serde_json::from_str(&raw)?;
        call.phase = phase;
        Ok(call)
    }

    /// Explicit recovery selects retained evidence, never another helper turn.
    pub fn interaction_session_recover(
        &self,
        caller: SessionKey,
        human: i64,
        native: &str,
        id: &str,
        closed: &tokio_util::sync::CancellationToken,
    ) -> Result<Option<Instruction>> {
        self.interaction_session_selected(caller, human, native, id)?;
        if closed.is_cancelled() {
            return Err(anyhow!("Explicit local return request closed"));
        }
        let deliveries = {
            let conn = self.conn.lock().expect("store mutex poisoned");
            let mut q=conn.prepare("SELECT d.id FROM interaction_session_requests c JOIN interaction_deliveries d ON d.request_id=c.target_instruction WHERE c.id=?1 AND c.human_id=?2 AND c.phase='requested' ORDER BY d.rowid DESC LIMIT 16")?;
            q.query_map(params![id, human], |r| r.get::<_, String>(0))?
                .collect::<std::result::Result<Vec<_>, _>>()?
        };
        for delivery in deliveries {
            if let Some(i) = self.session_return(&delivery, human, Some(closed))? {
                return Ok(Some(i));
            }
        }
        Ok(None)
    }

    /// A current worker-declared completed canonical result can become one
    /// attributed context input to the original logical caller, never human speech.
    pub fn interaction_session_return(
        &self,
        delivery: &str,
        human: i64,
    ) -> Result<Option<Instruction>> {
        self.session_return(delivery, human, None)
    }

    fn session_return(
        &self,
        delivery: &str,
        human: i64,
        closed: Option<&tokio_util::sync::CancellationToken>,
    ) -> Result<Option<Instruction>> {
        if closed.is_some_and(|c| c.is_cancelled()) {
            return Err(anyhow!("Explicit local return request closed"));
        }
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction()?;
        let found:Option<(String,String,String,String)>=tx.query_row("SELECT c.doc,c.phase,d.doc,d.result_id FROM interaction_session_requests c JOIN interaction_deliveries d ON d.request_id=c.target_instruction WHERE d.id=?1 AND c.human_id=?2 AND d.human_id=?2",params![delivery,human],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?;
        let Some((raw, phase, raw_delivery, result_id)) = found else {
            return Ok(None);
        };
        if phase != "requested" {
            return Ok(None);
        }
        let mut call: SessionRequest = serde_json::from_str(&raw)?;
        let d: crate::interaction::Delivery = serde_json::from_str(&raw_delivery)?;
        let (current, correction) = super::routing::delivery_freshness(&tx, &d)?;
        if !d.completed || !current || correction || current_epoch(&tx, human)? != call.epoch {
            return Ok(None);
        }
        let parent = instruction_on(&tx, &call.parent_instruction, human)?;
        let viable: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM turns WHERE id=?1 AND status IN ('running','completed'))",
            [parent.local_turn],
            |r| r.get(0),
        )?;
        if !viable || parent.phase != crate::interaction::TransportPhase::Accepted {
            return Ok(None);
        }
        let helper = instruction_on(&tx, &call.target_instruction, human)?;
        let text = format!(
            "Selected local session dependency returned. This is session-authored evidence, not a new human instruction or independent proof of completion. Local request: {}. Helper instruction: {}. Canonical result: {}. Worker-declared current completed coverage.\n\n{}",
            call.id,
            helper.id,
            result_id,
            d.body.chars().take(16000).collect::<String>()
        );
        let source = SourceInput {
            transcription: None,
            channel: "local-session-result".into(),
            event_id: call.id.clone(),
            update_id: None,
            human_id: human,
            target_sessions: vec![parent.target_session],
            origin: parent.origin.clone(),
            text,
        };
        let (mut i, fresh) = super::interaction::admit_on(
            &tx,
            &source,
            call.caller,
            RequestRelation::Context(parent.requests),
            &self.instance_id,
        )?;
        if !fresh {
            return Err(anyhow!("Unattributed local result source"));
        }
        save_epoch(&tx, &mut i, &call.epoch)?;
        let (result_native, result_turn): (String, String) = tx.query_row(
            "SELECT native_thread,native_turn FROM interaction_results WHERE id=?",
            [&result_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let author = json!({"kind":"session_result","session":helper.target,"native_thread":result_native,"native_turn":result_turn,"parent_instruction":call.parent_instruction,"local_request":call.id,"canonical_result":result_id,"authorized_human":human});
        tx.execute(
            "INSERT INTO interaction_session_authors VALUES(?1,?2)",
            params![i.input_id, author.to_string()],
        )?;
        call.return_instruction = Some(i.id.clone());
        call.phase = "returned".into();
        tx.execute(
            "UPDATE interaction_session_requests SET phase='returned',doc=?2 WHERE id=?1",
            params![call.id, serde_json::to_string(&call)?],
        )?;
        if closed.is_some_and(|c| c.is_cancelled()) {
            return Err(anyhow!(
                "Explicit local return request closed before commit"
            ));
        }
        tx.commit()?;
        Ok(Some(i))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interaction::{ReturnRoute, TransportPhase as P};
    fn setup(s: &Store, topic: i64) -> SessionRecord {
        let key = SessionKey::new(100, Some(topic));
        s.ensure_session(key, 100, &super::super::tests::defaults())
            .unwrap();
        s.set_session_codex_thread(key, &format!("native-{topic}"))
            .unwrap();
        s.get_session(key).unwrap().unwrap()
    }
    fn accept(s: &Store, target: &SessionRecord, i: &Instruction, turn: &str) -> i64 {
        let local = s
            .record_turn_started(
                target.id,
                &TurnRequest {
                    session_key: target.key,
                    from_user_id: 100,
                    prompt: i.text.clone(),
                    runtime_instructions: None,
                    attachments: vec![],
                    review_mode: None,
                    override_search_mode: None,
                },
            )
            .unwrap();
        s.record_turn_native_thread(local, target.codex_thread_id.as_deref().unwrap())
            .unwrap();
        s.interaction_transport(&i.id, 100, local, P::Submitting, None)
            .unwrap();
        s.interaction_transport(&i.id, 100, local, P::Accepted, Some(turn))
            .unwrap();
        local
    }
    fn parent(s: &Store, a: &SessionRecord) -> Instruction {
        parent_targets(s, a, vec![a.id])
    }
    fn parent_targets(s: &Store, a: &SessionRecord, targets: Vec<i64>) -> Instruction {
        let i = s
            .interaction_admit(
                &SourceInput {
                    transcription: None,
                    channel: "fixture".into(),
                    event_id: "human-parent".into(),
                    update_id: None,
                    human_id: 100,
                    target_sessions: targets,
                    origin: ReturnRoute {
                        chat_id: 100,
                        thread_id: 1,
                        message_id: Some(10),
                    },
                    text: "Human task".into(),
                },
                a.key,
                RequestRelation::New,
            )
            .unwrap()
            .0;
        s.interaction_bind_epoch(&i.id, 100, "initial").unwrap();
        accept(s, a, &i, "parent-turn");
        s.interaction_instruction(&i.id, 100).unwrap().unwrap()
    }
    fn result(s: &Store, b: &SessionRecord, i: &Instruction) -> crate::interaction::Delivery {
        let helper_turn = format!("helper-turn-{}", i.id);
        let local = accept(s, b, i, &helper_turn);
        let text = format!(
            "Selected dependency answer\n<daimon-result>{}</daimon-result>",
            json!({"requests":i.requests,"covered":[i.id],"completed":i.requests})
        );
        s.interaction_record_result(
            &i.id,
            100,
            local,
            b.codex_thread_id.as_deref().unwrap(),
            &helper_turn,
            "answer",
            &text,
            true,
        )
        .unwrap()
        .remove(0)
    }
    #[test]
    fn local_dependency_is_attributed_idempotent_bounded_and_returns_to_logical_caller() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let s = Store::open(file.path(), &[100], &super::super::tests::defaults()).unwrap();
        let a = setup(&s, 7);
        let b = setup(&s, 8);
        let p = parent(&s, &a);
        let closed = tokio_util::sync::CancellationToken::new();
        let send = |client: &str, text: &str| {
            s.interaction_session_request(
                a.key,
                100,
                "native-7",
                "parent-turn",
                &p.id,
                client,
                b.key,
                text,
                &closed,
            )
        };
        let (call, i, fresh) =
            send("selected-question", "A bounded session-authored question").unwrap();
        assert!(fresh);
        assert_eq!(
            s.interaction_author(&i.id, 100).unwrap().unwrap()["kind"],
            "session_request"
        );
        assert!(
            !send("selected-question", "A bounded session-authored question")
                .unwrap()
                .2
        );
        assert!(send("selected-question", "Changed question").is_err());
        assert!(
            s.interaction_session_request(
                a.key,
                100,
                "native-7",
                "foreign-turn",
                &p.id,
                "foreign",
                b.key,
                "Text",
                &closed
            )
            .is_err()
        );
        assert!(
            s.interaction_session_request(
                a.key,
                100,
                "native-7",
                "parent-turn",
                &p.id,
                "self",
                a.key,
                "Text",
                &closed
            )
            .is_err()
        );
        for client in ["second", "third", "fourth"] {
            send(client, "Another selected dependency").unwrap();
        }
        assert!(send("fifth", "No unbounded fanout").is_err());
        let d = result(&s, &b, &i);
        assert_eq!(d.phase, "internal");
        assert!(s.interaction_begin_delivery(&d.id, 100).unwrap().is_none());
        // An older writer may have reserved the same internal result for public
        // delivery. The claim boundary, not just its new phase, prevents replay.
        s.conn
            .lock()
            .unwrap()
            .execute(
                "UPDATE interaction_deliveries SET phase='reserved' WHERE id=?",
                [&d.id],
            )
            .unwrap();
        assert!(s.interaction_begin_delivery(&d.id, 100).unwrap().is_none());
        let entry = setup(&s, 1);
        let page = s
            .interaction_presentation_list(100, entry.key, 0, 0)
            .unwrap();
        assert!(page["items"].as_array().unwrap().is_empty());
        assert!(
            s.interaction_presentation_part(
                100,
                entry.key,
                &format!("delivery:{}", d.id),
                "unused",
                0
            )
            .is_err()
        );
        assert!(
            s.interaction_session_request(
                b.key,
                100,
                "native-8",
                &format!("helper-turn-{}", i.id),
                &i.id,
                "recursive",
                a.key,
                "Must not recurse",
                &closed
            )
            .is_err()
        );
        s.record_turn_finished(p.local_turn.unwrap(), "completed", None)
            .unwrap();
        s.set_session_codex_thread(a.key, "renewed-native-7")
            .unwrap();
        let returned = s.interaction_session_return(&d.id, 100).unwrap().unwrap();
        assert_eq!(returned.target, a.key);
        assert_eq!(returned.native_thread.as_deref(), Some("renewed-native-7"));
        assert_eq!(returned.requests, p.requests);
        assert_eq!(returned.origin, p.origin);
        assert_eq!(returned.admission_epoch.as_deref(), Some("initial"));
        let author = s.interaction_author(&returned.id, 100).unwrap().unwrap();
        assert_eq!(author["kind"], "session_result");
        assert_eq!(author["native_turn"], format!("helper-turn-{}", i.id));
        assert_eq!(author["local_request"], call.id);
        assert!(s.interaction_session_return(&d.id, 100).unwrap().is_none());
        s.interaction_withdraw_session_requests(a.key).unwrap();
        assert_eq!(
            s.interaction_instruction(&returned.id, 100)
                .unwrap()
                .unwrap()
                .phase,
            P::RetainedUnsent
        );
        assert!(
            s.interaction_session_requests(a.key, 101, "renewed-native-7")
                .is_err()
        );
    }
    #[test]
    fn direct_original_is_not_forwarded_again_but_existing_dependency_remains_inspectable() {
        for helper_first in [true, false] {
            let file = tempfile::NamedTempFile::new().unwrap();
            let s = Store::open(file.path(), &[100], &super::super::tests::defaults()).unwrap();
            let a = setup(&s, 7);
            let b = setup(&s, 8);
            let p = parent_targets(&s, &a, vec![a.id, b.id]);
            let source: SourceInput = {
                let conn = s.conn.lock().unwrap();
                let raw: String = conn
                    .query_row(
                        "SELECT doc FROM interaction_sources WHERE id=?",
                        [&p.input_id],
                        |r| r.get(0),
                    )
                    .unwrap();
                serde_json::from_str(&raw).unwrap()
            };
            let closed = tokio_util::sync::CancellationToken::new();
            let send = |id| {
                s.interaction_session_request(
                    a.key,
                    100,
                    "native-7",
                    "parent-turn",
                    &p.id,
                    id,
                    b.key,
                    "Selected work",
                    &closed,
                )
            };
            // A genuinely new dependency retains its exact identity even if a
            // direct route for the original is admitted afterwards.
            let (call, _, _) = send("before-route").unwrap();
            s.interaction_admit(&source, b.key, RequestRelation::New)
                .unwrap();
            assert_eq!(send("before-route").unwrap().0.id, call.id);
            let error = send("duplicate-route").unwrap_err().to_string();
            assert!(error.contains("Destination already received this original input"));
            assert_eq!(
                s.interaction_session_requests(a.key, 100, "native-7")
                    .unwrap()
                    .len(),
                1
            );
            let helper = s
                .interaction_instruction(&call.target_instruction, 100)
                .unwrap()
                .unwrap();
            let direct = s
                .interaction_admit(&source, b.key, RequestRelation::New)
                .unwrap()
                .0;
            // A retired or known-unsent direct route is not permission to
            // recreate the same work under another worker's dependency ID.
            for phase in ["rejected", "cancelled", "retained_unsent"] {
                s.conn
                    .lock()
                    .unwrap()
                    .execute(
                        "UPDATE interaction_instructions SET phase=?2 WHERE id=?1",
                        params![direct.id, phase],
                    )
                    .unwrap();
                assert!(
                    send("not-a-recovery")
                        .unwrap_err()
                        .to_string()
                        .contains("Destination already received this original input")
                );
            }
            s.conn
                .lock()
                .unwrap()
                .execute(
                    "UPDATE interaction_instructions SET phase='queued' WHERE id=?",
                    [&direct.id],
                )
                .unwrap();
            let physical = "mixed-result-turn";
            let (root, steered) = if helper_first {
                (&helper, &direct)
            } else {
                (&direct, &helper)
            };
            let local = accept(&s, &b, root, physical);
            s.interaction_transport(&steered.id, 100, local, P::Submitting, None)
                .unwrap();
            s.interaction_transport(&steered.id, 100, local, P::Accepted, Some(physical))
                .unwrap();
            let text = format!(
                "One human answer\n<daimon-result>{}</daimon-result>",
                json!({"requests":[direct.id, helper.id], "covered":[direct.id, helper.id], "completed":[direct.id, helper.id]})
            );
            let deliveries = s
                .interaction_record_result(
                    &root.id,
                    100,
                    local,
                    "native-8",
                    physical,
                    "mixed-answer",
                    &text,
                    true,
                )
                .unwrap();
            assert_eq!(deliveries.len(), 2);
            assert_eq!(
                deliveries.iter().filter(|d| d.phase == "reserved").count(),
                1
            );
            assert_eq!(
                deliveries.iter().filter(|d| d.phase == "internal").count(),
                1
            );
            let entry = setup(&s, 1);
            let page = s
                .interaction_presentation_list(100, entry.key, 0, 0)
                .unwrap();
            let items = page["items"].as_array().unwrap();
            assert_eq!(items.len(), 1);
            assert_eq!(items[0]["request_id"], direct.id);
            let internal = deliveries.iter().find(|d| d.phase == "internal").unwrap();
            assert!(
                s.interaction_session_return(&internal.id, 100)
                    .unwrap()
                    .is_some()
            );
            assert!(
                s.interaction_session_return(&internal.id, 100)
                    .unwrap()
                    .is_none()
            );
        }
    }

    #[test]
    fn dependency_author_and_admission_commit_atomically_and_retired_or_withdrawn_calls_do_not_return()
     {
        let file = tempfile::NamedTempFile::new().unwrap();
        let s = Store::open(file.path(), &[100], &super::super::tests::defaults()).unwrap();
        let a = setup(&s, 7);
        let b = setup(&s, 8);
        let p = parent(&s, &a);
        let closed = tokio_util::sync::CancellationToken::new();
        let send = |client: &str| {
            s.interaction_session_request(
                a.key,
                100,
                "native-7",
                "parent-turn",
                &p.id,
                client,
                b.key,
                "Selected question",
                &closed,
            )
        };
        s.conn.lock().unwrap().execute_batch("CREATE TRIGGER fail_author BEFORE INSERT ON interaction_session_authors BEGIN SELECT RAISE(ABORT,'synthetic author write failure'); END;").unwrap();
        assert!(send("atomic").is_err());
        assert_eq!(s.interaction_recent(100, b.key).unwrap().len(), 0);
        s.conn
            .lock()
            .unwrap()
            .execute_batch("DROP TRIGGER fail_author;")
            .unwrap();
        let (_, i, _) = send("withdraw").unwrap();
        let d = result(&s, &b, &i);
        s.interaction_withdraw_session_requests(a.key).unwrap();
        assert!(s.interaction_session_return(&d.id, 100).unwrap().is_none());
        assert!(send("new-id-after-withdrawal").is_err());
    }
    #[test]
    fn retired_dependency_cannot_return_or_create_a_new_grant() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let s = Store::open(file.path(), &[100], &super::super::tests::defaults()).unwrap();
        let a = setup(&s, 7);
        let b = setup(&s, 8);
        let p = parent(&s, &a);
        let closed = tokio_util::sync::CancellationToken::new();
        let send = |client: &str| {
            s.interaction_session_request(
                a.key,
                100,
                "native-7",
                "parent-turn",
                &p.id,
                client,
                b.key,
                "Selected question",
                &closed,
            )
        };
        let (_, i, _) = send("retire").unwrap();
        let d = result(&s, &b, &i);
        s.save_bot_state("stop_admission_epoch:100", "synthetic-retired-generation")
            .unwrap();
        assert!(s.interaction_session_return(&d.id, 100).unwrap().is_none());
        assert!(send("new-after-retire").is_err());
    }
    #[test]
    fn completed_canonical_dependency_recovers_after_restart_without_replaying_helper() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let defaults = super::super::tests::defaults();
        let s = Store::open(file.path(), &[100], &defaults).unwrap();
        let a = setup(&s, 7);
        let b = setup(&s, 8);
        let p = parent(&s, &a);
        let closed = tokio_util::sync::CancellationToken::new();
        let (call, i, _) = s
            .interaction_session_request(
                a.key,
                100,
                "native-7",
                "parent-turn",
                &p.id,
                "recover",
                b.key,
                "Selected question",
                &closed,
            )
            .unwrap();
        let _d = result(&s, &b, &i);
        let helper = s.interaction_instruction(&i.id, 100).unwrap().unwrap();
        s.record_turn_finished(p.local_turn.unwrap(), "completed", None)
            .unwrap();
        s.record_turn_finished(helper.local_turn.unwrap(), "completed", None)
            .unwrap();
        drop(s);
        // Canonical item+delivery committed, return admission had not run.
        let s = Store::open(file.path(), &[100], &defaults).unwrap();
        assert_eq!(
            s.interaction_session_requests(a.key, 100, "native-7")
                .unwrap()[0]
                .phase,
            "requested"
        );
        {
            let conn = s.conn.lock().unwrap();
            for n in 0..17 {
                let mut later = call.clone();
                later.id = format!("retained-later-{n}");
                later.target_instruction = format!("historical-helper-{n}");
                conn.execute("INSERT INTO interaction_session_requests VALUES(?1,100,100,7,?2,?3,'withdrawn',?4)",params![later.id,p.id,later.target_instruction,serde_json::to_string(&later).unwrap()]).unwrap();
            }
        }
        assert!(
            !s.interaction_session_requests(a.key, 100, "native-7")
                .unwrap()
                .iter()
                .any(|c| c.id == call.id)
        );
        let cancelled = tokio_util::sync::CancellationToken::new();
        cancelled.cancel();
        assert!(
            s.interaction_session_recover(a.key, 100, "native-7", &call.id, &cancelled)
                .is_err()
        );
        assert_eq!(
            s.interaction_session_selected(a.key, 100, "native-7", &call.id)
                .unwrap()
                .phase,
            "requested"
        );
        let returned = s
            .interaction_session_recover(a.key, 100, "native-7", &call.id, &closed)
            .unwrap()
            .unwrap();
        assert_eq!(returned.requests, p.requests);
        assert!(
            s.interaction_session_recover(a.key, 100, "native-7", &call.id, &closed)
                .unwrap()
                .is_none()
        );
        let conn = s.conn.lock().unwrap();
        let count: i64 = conn
            .query_row("SELECT count(*) FROM interaction_results", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
        let turns: i64 = conn
            .query_row("SELECT count(*) FROM turns", [], |r| r.get(0))
            .unwrap();
        assert_eq!(turns, 2);
    }
}
