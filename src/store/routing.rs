//! Publication reservations and reply bindings for the interaction controller.
use super::*;
use crate::interaction::{Delivery, Instruction, ReturnRoute, worker_result};

pub(super) fn freshness(
    conn: &Connection,
    request: &str,
    covered: &[String],
    declared: bool,
) -> Result<(bool, bool)> {
    let mut stmt=conn.prepare("SELECT i.id,i.phase,i.doc FROM interaction_request_links l JOIN interaction_instructions i ON i.id=l.instruction_id WHERE l.request_id=? AND i.phase NOT IN ('rejected','cancelled') ORDER BY i.sequence")?;
    let rows = stmt
        .query_map([request], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let mut current = declared && !rows.is_empty();
    let mut correction = false;
    for (id, phase, doc) in rows {
        let i: Instruction = serde_json::from_str(&doc)?;
        let included = phase == "accepted" && covered.contains(&id);
        current &= included;
        correction |=
            matches!(i.relation, crate::interaction::RequestRelation::Amends(_)) && !included;
    }
    Ok((current, correction))
}

fn bind_message(
    conn: &Connection,
    route: &ReturnRoute,
    human: i64,
    session: i64,
    requests: &[String],
) -> Result<()> {
    let doc = serde_json::to_string(requests)?;
    if conn.execute(
        "INSERT OR IGNORE INTO interaction_message_bindings VALUES(?1,?2,?3,?4,?5,?6)",
        params![
            route.chat_id,
            route.thread_id,
            route.message_id,
            human,
            session,
            doc
        ],
    )? == 0
    {
        let original:(i64,i64,String)=conn.query_row("SELECT human_id,session_id,requests FROM interaction_message_bindings WHERE chat_id=?1 AND thread_id=?2 AND message_id=?3",params![route.chat_id,route.thread_id,route.message_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
        if original != (human, session, doc) {
            return Err(anyhow!("Interaction reply binding is immutable"));
        }
    }
    Ok(())
}

impl Store {
    pub(super) fn init_routing(&self) -> Result<()> {
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        conn.execute_batch("CREATE TABLE IF NOT EXISTS interaction_results(id TEXT PRIMARY KEY,local_turn INTEGER NOT NULL,native_thread TEXT NOT NULL,native_turn TEXT NOT NULL,item_id TEXT NOT NULL,body TEXT NOT NULL,claim TEXT,UNIQUE(native_thread,native_turn,item_id));
          CREATE TABLE IF NOT EXISTS interaction_deliveries(id TEXT PRIMARY KEY,result_id TEXT NOT NULL REFERENCES interaction_results(id),request_id TEXT NOT NULL REFERENCES interaction_requests(id),human_id INTEGER NOT NULL,phase TEXT NOT NULL,doc TEXT NOT NULL,message_id INTEGER,instance_id TEXT NOT NULL,UNIQUE(result_id,request_id));
          CREATE TABLE IF NOT EXISTS interaction_delivery_parts(delivery_id TEXT NOT NULL REFERENCES interaction_deliveries(id),part INTEGER NOT NULL,text TEXT NOT NULL,message_id INTEGER,PRIMARY KEY(delivery_id,part));
          CREATE TABLE IF NOT EXISTS interaction_message_bindings(chat_id INTEGER NOT NULL,thread_id INTEGER NOT NULL,message_id INTEGER NOT NULL,human_id INTEGER NOT NULL,session_id INTEGER NOT NULL,requests TEXT NOT NULL,PRIMARY KEY(chat_id,thread_id,message_id));
          CREATE TABLE IF NOT EXISTS interaction_clarifications(id TEXT PRIMARY KEY,human_id INTEGER NOT NULL,doc TEXT NOT NULL,phase TEXT NOT NULL,question_chat INTEGER,question_thread INTEGER,question_message INTEGER);")?;
        let tx = conn.transaction()?;
        tx.execute("UPDATE interaction_deliveries SET phase=CASE WHEN phase='sending' THEN 'uncertain' ELSE 'reserved' END WHERE phase IN ('preparing','sending') AND instance_id!=?",[&self.instance_id])?;
        tx.commit()?;
        Ok(())
    }

    /// Reserve a canonical result before any external publication. The worker can
    /// report request coverage, but cannot supply a different recipient or scope.
    #[cfg(test)]
    #[allow(clippy::too_many_arguments)]
    pub fn interaction_reserve_result(
        &self,
        root_id: &str,
        human: i64,
        local_turn: i64,
        thread: &str,
        turn: &str,
        item: &str,
        text: &str,
    ) -> Result<Vec<Delivery>> {
        self.interaction_result(
            root_id, human, local_turn, thread, turn, item, text, false, true,
        )
    }

    /// Commit the canonical final and all return reservations together.
    #[allow(clippy::too_many_arguments)]
    pub fn interaction_record_result(
        &self,
        root_id: &str,
        human: i64,
        local_turn: i64,
        thread: &str,
        turn: &str,
        item: &str,
        text: &str,
        allow_unreported: bool,
    ) -> Result<Vec<Delivery>> {
        self.interaction_result(
            root_id,
            human,
            local_turn,
            thread,
            turn,
            item,
            text,
            true,
            allow_unreported,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn interaction_result(
        &self,
        root_id: &str,
        human: i64,
        local_turn: i64,
        thread: &str,
        turn: &str,
        item: &str,
        text: &str,
        capture: bool,
        allow_unreported: bool,
    ) -> Result<Vec<Delivery>> {
        let root = self
            .interaction_instruction(root_id, human)?
            .context("Interaction root missing")?;
        let (body, claim) = worker_result(text);
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction()?;
        if capture {
            super::interaction::record_native_message_on(
                &tx,
                local_turn,
                thread,
                turn,
                item,
                Some("final_answer"),
                text,
            )?;
        }
        let bound:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM interaction_native_messages m JOIN turns t ON t.id=m.local_turn WHERE m.native_thread=?1 AND m.native_turn=?2 AND m.item_id=?3 AND m.local_turn=?4 AND m.text=?5 AND t.session_id=?6 AND t.from_user_id=?7)",params![thread,turn,item,local_turn,text,root.target_session,human],|r|r.get(0))?;
        if !bound {
            return Err(anyhow!(
                "Interaction result is not a recorded native item of this execution"
            ));
        }
        let mut valid_claim = claim.is_some();
        if let Some(claim) = &claim {
            for request in &claim.requests {
                let allowed:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM interaction_requests WHERE id=?1 AND human_id=?2 AND session_id=?3)",params![request,human,root.target_session],|r|r.get(0))?;
                valid_claim &= allowed;
            }
            for id in claim.covered.iter().chain(&claim.deferred) {
                let allowed:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM interaction_instructions WHERE id=?1 AND session_id=?2)",params![id,root.target_session],|r|r.get(0))?;
                valid_claim &= allowed;
            }
        }
        let claim = claim.filter(|_| valid_claim);
        if claim.is_none() && !allow_unreported {
            // Preserve the actual native item without pretending a legacy
            // answer addressed an instruction attached while it was running.
            tx.commit()?;
            return Ok(vec![]);
        }
        let requests = claim
            .as_ref()
            .map(|c| c.requests.clone())
            .unwrap_or_else(|| root.requests.clone());
        let covered = claim
            .as_ref()
            .map(|c| c.covered.clone())
            .unwrap_or_default();
        let result_id = Uuid::new_v5(
            &Uuid::NAMESPACE_URL,
            serde_json::to_string(&(thread, turn, item))?.as_bytes(),
        )
        .to_string();
        tx.execute("INSERT OR IGNORE INTO interaction_results(id,local_turn,native_thread,native_turn,item_id,body,claim) VALUES(?1,?2,?3,?4,?5,?6,?7)",params![result_id,local_turn,thread,turn,item,body,claim.as_ref().map(serde_json::to_string).transpose()?])?;
        let mut deliveries = Vec::new();
        for request in requests {
            let source:String=tx.query_row("SELECT s.doc FROM interaction_requests r JOIN interaction_sources s ON s.id=r.input_id WHERE r.id=?1 AND r.human_id=?2 AND r.session_id=?3",params![request,human,root.target_session],|r|r.get(0))?;
            let source: crate::interaction::SourceInput = serde_json::from_str(&source)?;
            let id = Uuid::new_v5(
                &Uuid::NAMESPACE_URL,
                format!("{result_id}:{request}").as_bytes(),
            )
            .to_string();
            let (current, correction_pending) =
                freshness(&tx, &request, &covered, claim.is_some())?;
            let delivery = Delivery {
                id: id.clone(),
                request_id: request,
                target: root.target,
                origin: source.origin,
                body: body.to_owned(),
                covered: covered.clone(),
                declared: claim.is_some(),
                completed: false,
                current,
                correction_pending,
                phase: "reserved".into(),
            };
            // Completion is explicitly per request, not a property of the turn.
            let mut delivery = delivery;
            delivery.completed = claim
                .as_ref()
                .is_some_and(|c| c.completed.contains(&delivery.request_id));
            if tx.execute("INSERT OR IGNORE INTO interaction_deliveries(id,result_id,request_id,human_id,phase,doc,instance_id) VALUES(?1,?2,?3,?4,'reserved',?5,?6)",params![id,result_id,delivery.request_id,human,serde_json::to_string(&delivery)?,self.instance_id])?==1 {
                deliveries.push(delivery);
            }
        }
        tx.commit()?;
        Ok(deliveries)
    }

    /// This transaction is the last check before transmission may begin.
    pub fn interaction_begin_delivery(&self, id: &str, human: i64) -> Result<Option<Delivery>> {
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction()?;
        let raw:Option<String>=tx.query_row("SELECT d.doc FROM interaction_deliveries d JOIN users u ON u.tg_user_id=d.human_id WHERE d.id=?1 AND d.human_id=?2 AND u.allowed=1 AND d.phase='reserved' AND NOT EXISTS(SELECT 1 FROM interaction_delivery_parts p WHERE p.delivery_id=d.id)",params![id,human],|r|r.get(0)).optional()?;
        let Some(raw) = raw else {
            return Ok(None);
        };
        let mut d: Delivery = serde_json::from_str(&raw)?;
        (d.current, d.correction_pending) = freshness(&tx, &d.request_id, &d.covered, d.declared)?;
        d.phase = "preparing".into();
        tx.execute(
            "UPDATE interaction_deliveries SET phase='preparing',doc=?2,instance_id=?3 WHERE id=?1",
            params![id, serde_json::to_string(&d)?, self.instance_id],
        )?;
        tx.commit()?;
        Ok(Some(d))
    }

    /// Read-only result inspection refreshes relevance without changing its
    /// original recipient, transmission history or worker declaration.
    pub fn interaction_delivery(&self, id: &str, human: i64) -> Result<Option<Delivery>> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let raw:Option<(String,String)>=conn.query_row("SELECT d.doc,d.phase FROM interaction_deliveries d JOIN interaction_requests r ON r.id=d.request_id JOIN sessions s ON s.id=r.session_id JOIN users u ON u.tg_user_id=s.creator_user_id WHERE d.id=?1 AND d.human_id=?2 AND u.tg_user_id=?2 AND u.allowed=1",params![id,human],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
        raw.map(|(doc, phase)| {
            let mut d: Delivery = serde_json::from_str(&doc)?;
            d.phase = phase;
            (d.current, d.correction_pending) =
                freshness(&conn, &d.request_id, &d.covered, d.declared)?;
            Ok(d)
        })
        .transpose()
    }

    #[cfg(test)]
    pub fn interaction_recent_deliveries(
        &self,
        human: i64,
        target: SessionKey,
    ) -> Result<Vec<Delivery>> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let mut stmt=conn.prepare("SELECT d.id FROM interaction_deliveries d JOIN interaction_requests r ON r.id=d.request_id JOIN sessions s ON s.id=r.session_id WHERE d.human_id=?1 AND s.chat_id=?2 AND s.thread_id=?3 ORDER BY d.rowid DESC LIMIT 8")?;
        let ids = stmt
            .query_map(params![human, target.chat_id, target.thread_id], |r| {
                r.get::<_, String>(0)
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        drop(stmt);
        drop(conn);
        ids.into_iter()
            .map(|id| self.interaction_delivery(&id, human))
            .collect::<Result<Vec<_>>>()
            .map(|rows| rows.into_iter().flatten().collect())
    }

    pub fn interaction_finish_delivery(
        &self,
        id: &str,
        human: i64,
        message: Option<i64>,
    ) -> Result<()> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let phase: String = conn.query_row(
            "SELECT phase FROM interaction_deliveries WHERE id=?1 AND human_id=?2",
            params![id, human],
            |r| r.get(0),
        )?;
        if matches!(phase.as_str(), "delivered" | "withheld") {
            return Ok(());
        }
        if phase == "preparing" && message.is_none() {
            conn.execute("UPDATE interaction_deliveries SET phase='reserved' WHERE id=?1 AND NOT EXISTS(SELECT 1 FROM interaction_delivery_parts WHERE delivery_id=?1)",[id])?;
            return Ok(());
        }
        if !matches!(phase.as_str(), "sending" | "uncertain") {
            return Err(anyhow!(
                "Interaction publication is not an active transmission"
            ));
        }
        let confirmed_phase = if let Some(message) = message {
            let received:bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM interaction_delivery_parts WHERE delivery_id=?1 AND message_id=?2)",params![id,message],|r|r.get(0))?;
            if !received {
                return Err(anyhow!(
                    "Result completion requires a retained part receipt"
                ));
            }
            let raw: String = conn.query_row(
                "SELECT doc FROM interaction_deliveries WHERE id=?",
                [id],
                |r| r.get(0),
            )?;
            let frozen: Delivery = serde_json::from_str(&raw)?;
            if frozen.correction_pending {
                "withheld"
            } else {
                "delivered"
            }
        } else {
            "uncertain"
        };
        conn.execute("UPDATE interaction_deliveries SET phase=?2,message_id=COALESCE(?3,message_id) WHERE id=?1",params![id,confirmed_phase,message])?;
        Ok(())
    }

    /// Retain each part before sending it; known receipts bind replies even when
    /// a later part's transmission is uncertain. No part is automatically replayed.
    pub fn interaction_reserve_part(
        &self,
        id: &str,
        human: i64,
        part: i64,
        text: &str,
        pending_text: &str,
    ) -> Result<Option<(Delivery, String)>> {
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction()?;
        let raw:Option<String>=tx.query_row("SELECT doc FROM interaction_deliveries WHERE id=?1 AND human_id=?2 AND phase IN ('preparing','sending') AND EXISTS(SELECT 1 FROM users WHERE tg_user_id=?2 AND allowed=1)",params![id,human],|r|r.get(0)).optional()?;
        let Some(raw) = raw else {
            return Ok(None);
        };
        let mut d: Delivery = serde_json::from_str(&raw)?;
        (d.current, d.correction_pending) = freshness(&tx, &d.request_id, &d.covered, d.declared)?;
        let selected = if d.correction_pending {
            pending_text
        } else {
            text
        }
        .to_owned();
        if tx.execute("INSERT OR IGNORE INTO interaction_delivery_parts(delivery_id,part,text) VALUES(?1,?2,?3)",params![id,part,selected])?!=1 {return Ok(None);}
        d.phase = "sending".into();
        tx.execute(
            "UPDATE interaction_deliveries SET phase='sending',doc=?2,instance_id=?3 WHERE id=?1",
            params![id, serde_json::to_string(&d)?, self.instance_id],
        )?;
        tx.commit()?;
        Ok(Some((d, selected)))
    }

    pub fn interaction_part_receipt(
        &self,
        id: &str,
        human: i64,
        part: i64,
        message: i64,
    ) -> Result<()> {
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction()?;
        let (raw, phase): (String, String) = tx.query_row(
            "SELECT doc,phase FROM interaction_deliveries WHERE id=?1 AND human_id=?2",
            params![id, human],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        if !matches!(phase.as_str(), "sending" | "uncertain") {
            return Err(anyhow!("Publication receipt has no corresponding attempt"));
        }
        let d: Delivery = serde_json::from_str(&raw)?;
        if tx.execute("UPDATE interaction_delivery_parts SET message_id=?3 WHERE delivery_id=?1 AND part=?2 AND (message_id IS NULL OR message_id=?3)",params![id,part,message])?!=1 {return Err(anyhow!("Publication part receipt conflicts"));}
        let session: i64 = tx.query_row(
            "SELECT session_id FROM interaction_requests WHERE id=?",
            [&d.request_id],
            |r| r.get(0),
        )?;
        bind_message(
            &tx,
            &ReturnRoute {
                message_id: Some(message),
                ..d.origin
            },
            human,
            session,
            &[d.request_id],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn interaction_message_target(
        &self,
        route: &ReturnRoute,
        human: i64,
    ) -> Result<Option<(SessionKey, Vec<String>)>> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let raw:Option<(i64,i64,String)>=conn.query_row("SELECT s.chat_id,s.thread_id,b.requests FROM interaction_message_bindings b JOIN sessions s ON s.id=b.session_id JOIN users u ON u.tg_user_id=s.creator_user_id WHERE b.chat_id=?1 AND b.thread_id=?2 AND b.message_id=?3 AND b.human_id=?4 AND u.tg_user_id=?4 AND u.allowed=1",params![route.chat_id,route.thread_id,route.message_id,human],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
        raw.map(|(chat, thread, requests)| {
            Ok((
                SessionKey::new(chat, Some(thread)),
                serde_json::from_str(&requests)?,
            ))
        })
        .transpose()
    }
    pub fn interaction_bind_message(
        &self,
        human: i64,
        route: &ReturnRoute,
        target: SessionKey,
        requests: &[String],
    ) -> Result<()> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let session:i64=conn.query_row("SELECT s.id FROM sessions s JOIN users u ON u.tg_user_id=s.creator_user_id WHERE s.chat_id=?1 AND s.thread_id=?2 AND u.tg_user_id=?3 AND u.allowed=1",params![target.chat_id,target.thread_id,human],|r|r.get(0))?;
        for request in requests {
            let allowed:bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM interaction_requests WHERE id=?1 AND session_id=?2 AND human_id=?3)",params![request,session,human],|r|r.get(0))?;
            if !allowed {
                return Err(anyhow!("Reply binding crosses request scope"));
            }
        }
        bind_message(&conn, route, human, session, requests)?;
        Ok(())
    }

    pub fn interaction_hold(
        &self,
        source: &crate::interaction::SourceInput,
        inherited: &[(SessionKey, Vec<String>)],
    ) -> Result<String> {
        let id = Uuid::new_v5(
            &Uuid::NAMESPACE_URL,
            format!("pending:{}:{}", source.channel, source.event_id).as_bytes(),
        )
        .to_string();
        let doc = serde_json::json!({"source":source,"inherited":inherited}).to_string();
        if doc.len() > 512 * 1024 {
            return Err(anyhow!("Routing clarification exceeds input bound"));
        }
        let conn = self.conn.lock().expect("store mutex poisoned");
        let allowed: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM users WHERE tg_user_id=? AND allowed=1)",
            [source.human_id],
            |r| r.get(0),
        )?;
        if !allowed {
            return Err(anyhow!("Routing input is no longer authorized"));
        }
        if conn.execute("INSERT OR IGNORE INTO interaction_clarifications(id,human_id,doc,phase) VALUES(?1,?2,?3,'awaiting_target')",params![id,source.human_id,doc])?==0 {
            let original:String=conn.query_row("SELECT doc FROM interaction_clarifications WHERE id=?",[&id],|r|r.get(0))?;
            if original!=doc {return Err(anyhow!("Pending routing source is immutable"));}
        }
        Ok(id)
    }

    pub fn interaction_hold_state(&self, id: &str, human: i64) -> Result<Option<String>> {
        Ok(self
            .conn
            .lock()
            .expect("store mutex poisoned")
            .query_row(
                "SELECT phase FROM interaction_clarifications WHERE id=?1 AND human_id=?2",
                params![id, human],
                |r| r.get(0),
            )
            .optional()?)
    }

    pub fn interaction_hold_message(
        &self,
        id: &str,
        human: i64,
        route: &ReturnRoute,
    ) -> Result<()> {
        self.conn.lock().expect("store mutex poisoned").execute("UPDATE interaction_clarifications SET question_chat=?3,question_thread=?4,question_message=?5 WHERE id=?1 AND human_id=?2 AND question_message IS NULL",params![id,human,route.chat_id,route.thread_id,route.message_id])?;
        Ok(())
    }

    pub fn interaction_hold_reply(
        &self,
        route: &ReturnRoute,
        human: i64,
    ) -> Result<Option<serde_json::Value>> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let raw:Option<(String,String,String)>=conn.query_row("SELECT id,doc,phase FROM interaction_clarifications WHERE human_id=?1 AND ((question_chat=?2 AND question_thread=?3 AND question_message=?4) OR (json_extract(doc,'$.source.origin.chat_id')=?2 AND json_extract(doc,'$.source.origin.thread_id')=?3 AND json_extract(doc,'$.source.origin.message_id')=?4)) ORDER BY rowid DESC LIMIT 1",params![human,route.chat_id,route.thread_id,route.message_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
        raw.map(|(id, doc, phase)| {
            let mut doc: serde_json::Value = serde_json::from_str(&doc)?;
            doc["id"] = id.into();
            doc["phase"] = phase.into();
            Ok(doc)
        })
        .transpose()
    }

    pub fn interaction_close_hold(&self, id: &str, human: i64, resolved: bool) -> Result<bool> {
        Ok(self.conn.lock().expect("store mutex poisoned").execute("UPDATE interaction_clarifications SET phase=?3 WHERE id=?1 AND human_id=?2 AND phase='awaiting_target'",params![id,human,if resolved{"resolved"}else{"cancelled"}])?==1)
    }

    /// Definite native rejection permits a continuation of the same retained
    /// instruction. It never authorizes another destination or uncertain replay.
    pub fn interaction_requeue_rejected(&self, id: &str, human: i64) -> Result<bool> {
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction()?;
        let raw:Option<String>=tx.query_row("SELECT i.doc FROM interaction_instructions i JOIN interaction_sources s ON s.id=i.input_id WHERE i.id=?1 AND s.human_id=?2 AND i.phase='rejected'",params![id,human],|r|r.get(0)).optional()?;
        let Some(raw) = raw else {
            return Ok(false);
        };
        let mut i: Instruction = serde_json::from_str(&raw)?;
        i.phase = crate::interaction::TransportPhase::Queued;
        i.local_turn = None;
        i.native_turn = None;
        tx.execute(
            "UPDATE interaction_instructions SET phase='queued',doc=?2,instance_id=?3 WHERE id=?1",
            params![id, serde_json::to_string(&i)?, self.instance_id],
        )?;
        tx.commit()?;
        Ok(true)
    }

    pub fn interaction_unfinished(&self, id: &str, human: i64) -> Result<()> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        conn.execute("UPDATE interaction_instructions SET phase=CASE WHEN phase='queued' THEN 'retained_unsent' ELSE 'uncertain' END WHERE id=?1 AND input_id IN (SELECT id FROM interaction_sources WHERE human_id=?2) AND phase IN ('queued','submitting','steering')",params![id,human])?;
        Ok(())
    }

    pub fn interaction_source(
        &self,
        id: &str,
        human: i64,
    ) -> Result<crate::interaction::SourceInput> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let raw:String=conn.query_row("SELECT s.doc FROM interaction_sources s JOIN interaction_instructions i ON i.input_id=s.id WHERE i.id=?1 AND s.human_id=?2",params![id,human],|r|r.get(0))?;
        Ok(serde_json::from_str(&raw)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interaction::{RequestRelation as R, SourceInput, TransportPhase as P};
    use crate::models::{SessionRecord, TurnRequest};

    fn target(s: &Store, topic: i64) -> SessionRecord {
        let key = SessionKey::new(100, Some(topic));
        s.ensure_session(key, 100, &super::super::tests::defaults())
            .unwrap();
        s.set_session_codex_thread(key, &format!("native-{topic}"))
            .unwrap();
        s.get_session(key).unwrap().unwrap()
    }
    fn admit(s: &Store, target: &SessionRecord, event: i64, relation: R) -> Instruction {
        s.interaction_admit(
            &SourceInput {
                transcription: None,
                channel: "fixture".into(),
                event_id: event.to_string(),
                update_id: Some(event),
                human_id: 100,
                target_sessions: vec![target.id],
                origin: ReturnRoute {
                    chat_id: 100,
                    thread_id: 1,
                    message_id: Some(event),
                },
                text: format!("Input {event}"),
            },
            target.key,
            relation,
        )
        .unwrap()
        .0
    }
    fn accept(s: &Store, target: &SessionRecord, i: &Instruction) -> i64 {
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
        s.interaction_transport(&i.id, 100, local, P::Accepted, Some("physical-turn"))
            .unwrap();
        local
    }
    fn reserve(
        s: &Store,
        target: &SessionRecord,
        i: &Instruction,
        local: i64,
        item: &str,
    ) -> Delivery {
        let text = format!(
            "Answer\n<daimon-result>{}</daimon-result>",
            serde_json::json!({"requests":i.requests,"covered":[i.id],"completed":i.requests})
        );
        let thread = target.codex_thread_id.as_deref().unwrap();
        s.record_native_message(
            local,
            thread,
            "physical-turn",
            item,
            Some("final_answer"),
            &text,
        )
        .unwrap();
        let deliveries = s
            .interaction_reserve_result(&i.id, 100, local, thread, "physical-turn", item, &text)
            .unwrap();
        assert!(
            s.interaction_reserve_result(&i.id, 100, local, thread, "physical-turn", item, &text)
                .unwrap()
                .is_empty()
        );
        deliveries.into_iter().next().unwrap()
    }

    #[test]
    fn interaction_dispatch_epoch_changes_only_after_explicit_unsent_recovery() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let s = Store::open(file.path(), &[100], &super::super::tests::defaults()).unwrap();
        let t = target(&s, 7);
        let i = admit(&s, &t, 1, R::New);
        s.interaction_bind_epoch(&i.id, 100, "original").unwrap();
        assert!(s.interaction_bind_epoch(&i.id, 100, "later").is_err());
        s.interaction_unfinished(&i.id, 100).unwrap();
        assert_eq!(
            s.interaction_instruction(&i.id, 100)
                .unwrap()
                .unwrap()
                .admission_epoch
                .as_deref(),
            Some("original")
        );
        assert!(s.interaction_recover(&i.id, 100).unwrap());
        s.interaction_bind_epoch(&i.id, 100, "later").unwrap();
        assert_eq!(
            s.interaction_instruction(&i.id, 100)
                .unwrap()
                .unwrap()
                .admission_epoch
                .as_deref(),
            Some("later")
        );
        assert_eq!(s.interaction_recent(100, t.key).unwrap().len(), 1);
    }

    #[test]
    fn interaction_attached_input_requires_worker_coverage_before_routing_a_legacy_answer() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let s = Store::open(file.path(), &[100], &super::super::tests::defaults()).unwrap();
        let t = target(&s, 7);
        let i = admit(&s, &t, 1, R::New);
        let local = accept(&s, &t, &i);
        let thread = t.codex_thread_id.as_deref().unwrap();
        assert_eq!(s.interaction_turn_inputs(local, 100).unwrap().len(), 1);
        assert!(s.interaction_turn_inputs(local, 101).unwrap().is_empty());
        assert!(
            s.interaction_record_result(
                &i.id,
                100,
                local,
                thread,
                "physical-turn",
                "legacy",
                "Earlier unrelated work",
                false
            )
            .unwrap()
            .is_empty()
        );
        assert!(
            s.interaction_recent_deliveries(100, t.key)
                .unwrap()
                .is_empty()
        );
        let retained: i64 = s
            .conn
            .lock()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM interaction_native_messages WHERE item_id='legacy'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(retained, 1);
        let text = format!(
            "Corrected work\n<daimon-result>{}</daimon-result>",
            serde_json::json!({"requests":i.requests,"covered":[i.id],"completed":i.requests})
        );
        let deliveries = s
            .interaction_record_result(
                &i.id,
                100,
                local,
                thread,
                "physical-turn",
                "covered",
                &text,
                false,
            )
            .unwrap();
        assert_eq!(deliveries.len(), 1);
        assert!(deliveries[0].declared && deliveries[0].current);
        assert_eq!(deliveries[0].origin, i.origin);
    }

    #[test]
    fn interaction_native_capture_and_return_reservation_commit_together() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let s = Store::open(file.path(), &[100], &super::super::tests::defaults()).unwrap();
        let t = target(&s, 7);
        let i = admit(&s, &t, 1, R::New);
        let local = accept(&s, &t, &i);
        let thread = t.codex_thread_id.as_deref().unwrap();
        s.conn.lock().unwrap().execute_batch("CREATE TRIGGER fail_reservation BEFORE INSERT ON interaction_deliveries BEGIN SELECT RAISE(ABORT,'fixture reservation failure'); END;").unwrap();
        assert!(
            s.interaction_record_result(
                &i.id,
                100,
                local,
                thread,
                "physical-turn",
                "atomic",
                "Actual native answer",
                true
            )
            .is_err()
        );
        let count: i64 = s
            .conn
            .lock()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM interaction_native_messages WHERE item_id='atomic'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            count, 0,
            "A failed reservation must not commit an orphan canonical final"
        );
        s.conn
            .lock()
            .unwrap()
            .execute_batch("DROP TRIGGER fail_reservation")
            .unwrap();
        let deliveries = s
            .interaction_record_result(
                &i.id,
                100,
                local,
                thread,
                "physical-turn",
                "atomic",
                "Actual native answer",
                true,
            )
            .unwrap();
        assert_eq!(deliveries.len(), 1);
        assert!(
            s.interaction_record_result(
                &i.id,
                100,
                local,
                thread,
                "physical-turn",
                "atomic",
                "Actual native answer",
                true
            )
            .unwrap()
            .is_empty()
        );
        let id = deliveries[0].id.clone();
        drop(s);
        let s = Store::open(file.path(), &[100], &super::super::tests::defaults()).unwrap();
        assert_eq!(
            s.interaction_delivery(&id, 100).unwrap().unwrap().phase,
            "reserved"
        );
        assert!(s.interaction_begin_delivery(&id, 100).unwrap().is_some());
    }

    #[test]
    fn interaction_publication_rechecks_corrections_after_preparing_and_keeps_other_requests_live()
    {
        let file = tempfile::NamedTempFile::new().unwrap();
        let s = Store::open(file.path(), &[100], &super::super::tests::defaults()).unwrap();
        let t = target(&s, 7);
        let a = admit(&s, &t, 1, R::New);
        let b = admit(&s, &t, 2, R::New);
        let local_a = accept(&s, &t, &a);
        let local_b = accept(&s, &t, &b);
        let da = reserve(&s, &t, &a, local_a, "a");
        let db = reserve(&s, &t, &b, local_b, "b");
        assert!(da.current && da.completed);
        assert!(
            s.interaction_begin_delivery(&da.id, 100)
                .unwrap()
                .unwrap()
                .current
        );
        assert!(s.interaction_begin_delivery(&da.id, 100).unwrap().is_none());
        let _correction = admit(&s, &t, 3, R::Amends(a.requests.clone()));
        let (fresh, text) = s
            .interaction_reserve_part(&da.id, 100, 0, "obsolete answer", "correction pending")
            .unwrap()
            .unwrap();
        assert!(fresh.correction_pending && !fresh.current);
        assert_eq!(text, "correction pending");
        assert!(
            s.interaction_reserve_part(&da.id, 100, 0, "retry", "retry")
                .unwrap()
                .is_none()
        );
        s.interaction_part_receipt(&da.id, 100, 0, 501).unwrap();
        let reply = ReturnRoute {
            message_id: Some(501),
            ..a.origin.clone()
        };
        assert_eq!(
            s.interaction_message_target(&reply, 100).unwrap(),
            Some((t.key, a.requests.clone()))
        );
        assert!(s.interaction_message_target(&reply, 101).unwrap().is_none());
        let other = target(&s, 8);
        assert!(
            s.interaction_bind_message(100, &reply, other.key, &[])
                .is_err()
        );
        assert!(s.interaction_part_receipt(&da.id, 100, 0, 502).is_err());
        s.interaction_finish_delivery(&da.id, 100, Some(501))
            .unwrap();
        assert!(s.interaction_begin_delivery(&db.id, 101).unwrap().is_none());
        assert!(
            s.interaction_begin_delivery(&db.id, 100)
                .unwrap()
                .unwrap()
                .current
        );
        let (fresh, text) = s
            .interaction_reserve_part(&db.id, 100, 0, "B answer", "pending")
            .unwrap()
            .unwrap();
        assert!(fresh.current && !fresh.correction_pending);
        assert_eq!(text, "B answer");
    }

    #[test]
    fn interaction_late_correction_preserves_uncertain_then_delivered_history_and_new_items() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let s = Store::open(file.path(), &[100], &super::super::tests::defaults()).unwrap();
        let t = target(&s, 7);
        let a = admit(&s, &t, 1, R::New);
        let local = accept(&s, &t, &a);
        let old = reserve(&s, &t, &a, local, "earlier");
        s.interaction_begin_delivery(&old.id, 100).unwrap();
        s.interaction_reserve_part(
            &old.id,
            100,
            0,
            "Earlier answer already attempting transmission",
            "pending",
        )
        .unwrap();
        let amendment = admit(&s, &t, 2, R::Amends(a.requests.clone()));
        s.interaction_finish_delivery(&old.id, 100, None).unwrap();
        let d = s.interaction_delivery(&old.id, 100).unwrap().unwrap();
        assert_eq!(d.phase, "uncertain");
        assert!(!d.current && d.correction_pending);
        assert_eq!(d.body, "Answer");
        assert!(s.interaction_begin_delivery(&d.id, 100).unwrap().is_none());
        assert!(s.interaction_delivery(&d.id, 101).unwrap().is_none());
        // Inspection is read-only and does not turn uncertainty into a retry.
        for _ in 0..2 {
            assert_eq!(
                s.interaction_recent_deliveries(100, t.key).unwrap()[0].phase,
                "uncertain"
            );
        }
        assert!(
            s.interaction_reserve_part(&d.id, 100, 0, "retry", "pending")
                .unwrap()
                .is_none()
        );
        // Exact late evidence confirms the earlier attempt, not the correction.
        s.interaction_part_receipt(&d.id, 100, 0, 701).unwrap();
        s.interaction_finish_delivery(&d.id, 100, Some(701))
            .unwrap();
        let prior = s.interaction_delivery(&d.id, 100).unwrap().unwrap();
        assert_eq!(prior.phase, "delivered");
        assert!(prior.correction_pending);
        s.interaction_transport(&amendment.id, 100, local, P::Steering, None)
            .unwrap();
        s.interaction_transport(
            &amendment.id,
            100,
            local,
            P::Accepted,
            Some("physical-turn"),
        )
        .unwrap();
        let text = format!(
            "Updated answer\n<daimon-result>{}</daimon-result>",
            serde_json::json!({"requests":a.requests,"covered":[a.id,amendment.id],"completed":a.requests})
        );
        let thread = t.codex_thread_id.as_deref().unwrap();
        s.record_native_message(
            local,
            thread,
            "physical-turn",
            "updated",
            Some("final_answer"),
            &text,
        )
        .unwrap();
        let updated = s
            .interaction_reserve_result(
                &a.id,
                100,
                local,
                thread,
                "physical-turn",
                "updated",
                &text,
            )
            .unwrap();
        assert_eq!(updated.len(), 1);
        assert!(updated[0].current && !updated[0].correction_pending);
        assert_eq!(
            s.interaction_recent_deliveries(100, t.key).unwrap()[0].id,
            updated[0].id
        );
        let unchanged = s.interaction_delivery(&d.id, 100).unwrap().unwrap();
        assert_eq!(unchanged.body, "Answer");
        assert!(unchanged.correction_pending);
    }

    #[test]
    fn interaction_restart_distinguishes_preparation_from_uncertain_transmission() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let s = Store::open(file.path(), &[100], &super::super::tests::defaults()).unwrap();
        let t = target(&s, 7);
        let a = admit(&s, &t, 1, R::New);
        let b = admit(&s, &t, 2, R::New);
        let local_a = accept(&s, &t, &a);
        let local_b = accept(&s, &t, &b);
        let da = reserve(&s, &t, &a, local_a, "a");
        let db = reserve(&s, &t, &b, local_b, "b");
        s.interaction_begin_delivery(&da.id, 100).unwrap();
        s.interaction_begin_delivery(&db.id, 100).unwrap();
        s.interaction_reserve_part(&db.id, 100, 0, "B", "pending")
            .unwrap();
        drop(s);
        let s = Store::open(file.path(), &[100], &super::super::tests::defaults()).unwrap();
        assert_eq!(
            s.interaction_delivery(&da.id, 100).unwrap().unwrap().phase,
            "reserved"
        );
        assert!(s.interaction_begin_delivery(&da.id, 100).unwrap().is_some());
        assert!(s.interaction_begin_delivery(&db.id, 100).unwrap().is_none());
        s.interaction_finish_delivery(&da.id, 100, None).unwrap();
        assert_eq!(
            s.interaction_delivery(&da.id, 100).unwrap().unwrap().phase,
            "reserved"
        );
        assert!(s.interaction_begin_delivery(&da.id, 100).unwrap().is_some());
        assert!(
            s.interaction_reserve_part(&db.id, 100, 0, "B retry", "pending")
                .unwrap()
                .is_none()
        );
        // A late exact receipt reconciles the original attempt, without replay.
        s.interaction_part_receipt(&db.id, 100, 0, 601).unwrap();
        s.interaction_finish_delivery(&db.id, 100, Some(601))
            .unwrap();
        assert!(
            s.interaction_message_target(
                &ReturnRoute {
                    message_id: Some(601),
                    ..b.origin
                },
                100
            )
            .unwrap()
            .is_some()
        );
    }
}
