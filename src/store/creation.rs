//! Retained creation intents and enrollment in the existing interaction registry.
use super::*;
use crate::interaction::{InteractionConfig, RequestRelation, ReturnRoute, SourceInput, Target};
use serde_json::{Value, json};

impl Store {
    pub(super) fn init_interaction_creation(&self) -> Result<()> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        conn.execute_batch("CREATE TABLE IF NOT EXISTS interaction_creations(id TEXT PRIMARY KEY,human_id INTEGER NOT NULL,entry_chat INTEGER NOT NULL,entry_topic INTEGER NOT NULL,input_update INTEGER NOT NULL,phase TEXT NOT NULL,doc TEXT NOT NULL,topic INTEGER,instance_id TEXT NOT NULL,UNIQUE(human_id,entry_chat,entry_topic,input_update));
        CREATE TABLE IF NOT EXISTS interaction_enrollments(human_id INTEGER NOT NULL,entry_chat INTEGER NOT NULL,entry_topic INTEGER NOT NULL,target_topic INTEGER NOT NULL,session_id INTEGER NOT NULL,name TEXT NOT NULL,creation_id TEXT NOT NULL UNIQUE REFERENCES interaction_creations(id),PRIMARY KEY(human_id,entry_chat,entry_topic,target_topic));")?;
        conn.execute("UPDATE interaction_creations SET phase=CASE phase WHEN 'prepared' THEN 'retained_unsent' WHEN 'creating_topic' THEN 'uncertain' ELSE 'retained_topic' END WHERE instance_id!=?1 AND phase IN ('prepared','creating_topic','topic_known')",[&self.instance_id])?;
        Ok(())
    }

    pub fn interaction_profile(&self, base: &InteractionConfig) -> Result<InteractionConfig> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let mut cfg = base.clone();
        let mut q=conn.prepare("SELECT e.target_topic,e.name FROM interaction_enrollments e JOIN sessions s ON s.id=e.session_id JOIN users u ON u.tg_user_id=s.creator_user_id WHERE e.human_id=?1 AND e.entry_chat=?2 AND e.entry_topic=?3 AND s.creator_user_id=?1 AND u.allowed=1 AND s.chat_id=e.entry_chat AND s.thread_id=e.target_topic ORDER BY e.target_topic")?;
        let rows = q.query_map(params![base.human_id, base.chat_id, base.topic_id], |r| {
            Ok(Target {
                topic_id: r.get(0)?,
                name: r.get(1)?,
            })
        })?;
        for target in rows {
            let target = target?;
            if !cfg.targets.iter().any(|t| t.topic_id == target.topic_id) {
                cfg.targets.push(target);
            }
        }
        cfg.validate()?;
        Ok(cfg)
    }

    pub fn interaction_creation(&self, id: &str, human: i64) -> Result<Value> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        creation_on(&conn, id, human)
    }

    pub fn interaction_creation_for_input(
        &self,
        key: SessionKey,
        human: i64,
        update: i64,
    ) -> Result<Value> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let id:String=conn.query_row("SELECT id FROM interaction_creations WHERE human_id=?1 AND entry_chat=?2 AND entry_topic=?3 AND input_update=?4",params![human,key.chat_id,key.thread_id,update],|r|r.get(0))?;
        creation_on(&conn, &id, human)
    }

    /// Freeze one original, attributed context package before a topic RPC.
    #[allow(clippy::too_many_arguments)]
    pub fn interaction_prepare_creation(
        &self,
        key: SessionKey,
        human: i64,
        native: &str,
        update: i64,
        name: &str,
        reason: &str,
        context: &Value,
        capacity: usize,
        closed: &tokio_util::sync::CancellationToken,
    ) -> Result<(Value, bool)> {
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction()?;
        let original = super::dialogue::entry_inputs_on(&tx, key, human, native, Some(update))?
            .pop()
            .context("Original input unavailable")?;
        let (text, transcription) = super::voice::content_on(&tx, &original, human)?;
        let source = original.message.context("Original message missing")?;
        if text.trim().is_empty()
            || text.trim_start().starts_with('/')
            || context.to_string().len() > 48000
        {
            return Err(anyhow!("Unsupported original input or context"));
        }
        let id = Uuid::new_v5(
            &Uuid::NAMESPACE_URL,
            format!(
                "interaction-create:{human}:{}:{}:{update}",
                key.chat_id, key.thread_id
            )
            .as_bytes(),
        )
        .to_string();
        let mut doc = json!({"id":id,"human":human,"entry":key,"entry_native":native,"input_update":update,"name":name,"reason":reason,"context":context,"original_text":text,"origin":{"chat_id":source.chat.id,"thread_id":source.message_thread_id.unwrap_or(0),"message_id":source.message_id}});
        if let Some(transcription) = transcription {
            doc["transcription"] = serde_json::to_value(transcription)?;
        }
        if let Some(existing) = tx
            .query_row(
                "SELECT doc FROM interaction_creations WHERE id=?1",
                [&id],
                |r| r.get::<_, String>(0),
            )
            .optional()?
        {
            if serde_json::from_str::<Value>(&existing)? != doc {
                return Err(anyhow!("Creation intent is immutable"));
            }
            return Ok((creation_on(&tx, &id, human)?, false));
        }
        let reserved:i64=tx.query_row("SELECT count(*) FROM interaction_creations WHERE human_id=?1 AND entry_chat=?2 AND entry_topic=?3",params![human,key.chat_id,key.thread_id],|r|r.get(0))?;
        if reserved >= capacity as i64 {
            return Err(anyhow!("Creation capacity is already reserved"));
        }
        let routed: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM interaction_sources WHERE human_id=?1 AND update_id=?2)",
            params![human, update],
            |r| r.get(0),
        )?;
        if routed {
            return Err(anyhow!("Original input already has a selected plan"));
        }
        tx.execute("INSERT INTO interaction_creations(id,human_id,entry_chat,entry_topic,input_update,phase,doc,instance_id) VALUES(?1,?2,?3,?4,?5,'prepared',?6,?7)",params![id,human,key.chat_id,key.thread_id,update,doc.to_string(),self.instance_id])?;
        if closed.is_cancelled() {
            return Err(anyhow!("Native request closed before creation commitment"));
        }
        tx.commit()?;
        drop(conn);
        Ok((self.interaction_creation(&id, human)?, true))
    }

    /// Claim only a known-unsent mutation or a confirmed topic for explicit
    /// recovery. Concurrent calls, completed plans and unknown effects only inspect.
    pub fn interaction_creation_resume(
        &self,
        key: SessionKey,
        human: i64,
        update: i64,
        closed: &tokio_util::sync::CancellationToken,
    ) -> Result<(Value, bool)> {
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction()?;
        let id:String=tx.query_row("SELECT id FROM interaction_creations WHERE human_id=?1 AND entry_chat=?2 AND entry_topic=?3 AND input_update=?4",params![human,key.chat_id,key.thread_id,update],|r|r.get(0))?;
        let operation = creation_on(&tx, &id, human)?;
        let phase = match operation["phase"].as_str() {
            Some("retained_unsent") => "prepared",
            Some("retained_topic") => "topic_known",
            _ => return Ok((operation, false)),
        };
        eligible(&tx, &operation)?;
        tx.execute(
            "UPDATE interaction_creations SET phase=?1,instance_id=?2 WHERE id=?3",
            params![phase, self.instance_id, id],
        )?;
        if closed.is_cancelled() {
            return Err(anyhow!("Native request closed before recovery commitment"));
        }
        let operation = creation_on(&tx, &id, human)?;
        tx.commit()?;
        Ok((operation, true))
    }

    pub fn interaction_creation_suspend(&self, id: &str, human: i64) -> Result<()> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        conn.execute("UPDATE interaction_creations SET phase=CASE phase WHEN 'prepared' THEN 'retained_unsent' WHEN 'creating_topic' THEN 'uncertain' ELSE 'retained_topic' END WHERE id=?1 AND human_id=?2 AND instance_id=?3 AND phase IN ('prepared','creating_topic','topic_known')",params![id,human,self.instance_id])?;
        Ok(())
    }

    /// Mark the external mutation before sending it. Unknown RPC outcomes never retry.
    pub fn interaction_creation_start(&self, id: &str, human: i64) -> Result<()> {
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction()?;
        let operation = creation_on(&tx, id, human)?;
        eligible(&tx, &operation)?;
        if tx.execute("UPDATE interaction_creations SET phase='creating_topic' WHERE id=?1 AND human_id=?2 AND phase='prepared' AND instance_id=?3",params![id,human,self.instance_id])?!=1 {return Err(anyhow!("Creation cannot be replayed"));}
        tx.commit()?;
        Ok(())
    }

    pub fn interaction_creation_topic(&self, id: &str, human: i64, topic: i64) -> Result<()> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        if topic<=0 || conn.execute("UPDATE interaction_creations SET topic=?1,phase='topic_known' WHERE id=?2 AND human_id=?3 AND phase='creating_topic' AND instance_id=?4",params![topic,id,human,self.instance_id])?!=1 {return Err(anyhow!("Creation topic receipt unavailable"));}
        Ok(())
    }

    /// Enroll the actual owned destination and admit its original input atomically.
    pub fn interaction_creation_finish(
        &self,
        id: &str,
        human: i64,
        channel: &str,
    ) -> Result<crate::interaction::Instruction> {
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction()?;
        let operation = creation_on(&tx, id, human)?;
        eligible(&tx, &operation)?;
        if operation["phase"] != "topic_known" {
            return Err(anyhow!("Creation has no confirmed topic"));
        }
        let d = &operation["intent"];
        let entry: SessionKey = serde_json::from_value(d["entry"].clone())?;
        let topic = operation["topic"].as_i64().context("Missing topic")?;
        let session:i64=tx.query_row("SELECT id FROM sessions WHERE chat_id=?1 AND thread_id=?2 AND creator_user_id=?3 AND codex_thread_id IS NULL AND force_fresh_thread=1",params![entry.chat_id,topic,human],|r|r.get(0))?;
        let source = SourceInput {
            transcription: serde_json::from_value(
                d.get("transcription").cloned().unwrap_or(Value::Null),
            )?,
            channel: channel.into(),
            event_id: d["input_update"].to_string(),
            update_id: d["input_update"].as_i64(),
            human_id: human,
            target_sessions: vec![session],
            origin: serde_json::from_value::<ReturnRoute>(d["origin"].clone())?,
            text: d["original_text"]
                .as_str()
                .context("Missing original text")?
                .into(),
        };
        let (instruction, _) = super::interaction::admit_on(
            &tx,
            &source,
            SessionKey::new(entry.chat_id, Some(topic)),
            RequestRelation::New,
            &self.instance_id,
        )?;
        tx.execute("INSERT INTO interaction_enrollments(human_id,entry_chat,entry_topic,target_topic,session_id,name,creation_id) VALUES(?1,?2,?3,?4,?5,?6,?7)",params![human,entry.chat_id,entry.thread_id,topic,session,d["name"].as_str(),id])?;
        tx.execute(
            "UPDATE interaction_creations SET phase='ready' WHERE id=?1",
            [id],
        )?;
        tx.commit()?;
        Ok(instruction)
    }

    pub fn interaction_creation_context(
        &self,
        key: SessionKey,
        human: i64,
    ) -> Result<Option<Value>> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let doc:Option<String>=conn.query_row("SELECT c.doc FROM interaction_enrollments e JOIN interaction_creations c ON c.id=e.creation_id JOIN sessions s ON s.id=e.session_id WHERE e.human_id=?1 AND s.creator_user_id=?1 AND s.chat_id=?2 AND s.thread_id=?3",params![human,key.chat_id,key.thread_id],|r|r.get(0)).optional()?;
        doc.map(|d| Ok(serde_json::from_str::<Value>(&d)?["context"].clone()))
            .transpose()
    }
}

fn creation_on(conn: &Connection, id: &str, human: i64) -> Result<Value> {
    let (doc, phase, topic): (String, String, Option<i64>) = conn.query_row(
        "SELECT doc,phase,topic FROM interaction_creations WHERE id=?1 AND human_id=?2",
        params![id, human],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    let intent: Value = serde_json::from_str(&doc)?;
    let mut q=conn.prepare("SELECT i.id,i.phase FROM interaction_instructions i JOIN interaction_sources s ON s.id=i.input_id JOIN sessions destination ON destination.id=i.session_id WHERE s.human_id=?1 AND s.update_id=?2 AND destination.chat_id=?3 AND destination.thread_id=?4 ORDER BY i.sequence LIMIT 1")?;
    let rows = q.query_map(
        params![
            human,
            intent["input_update"].as_i64(),
            intent["entry"]["chat_id"].as_i64(),
            topic
        ],
        |r| Ok(json!({"id":r.get::<_,String>(0)?,"phase":r.get::<_,String>(1)?})),
    )?;
    let instructions = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(
        json!({"id":id,"intent":intent,"phase":phase,"topic":topic,"instructions":instructions,"uncertainty":"A recorded intent or topic is not proof of worker incorporation; never replay an unconfirmed topic creation. Inspect retained instruction IDs for downstream delivery recovery."}),
    )
}
fn eligible(conn: &Connection, operation: &Value) -> Result<()> {
    let d = &operation["intent"];
    let key: SessionKey = serde_json::from_value(d["entry"].clone())?;
    let original = super::dialogue::committed_entry_input_on(
        conn,
        key,
        d["human"].as_i64().context("Missing human")?,
        d["entry_native"].as_str().context("Missing native")?,
        d["input_update"]
            .as_i64()
            .context("Missing original update")?,
    )?;
    if original.len() != 1 {
        return Err(anyhow!("Creation original retired or rebound"));
    }
    let message = original[0]
        .message
        .as_ref()
        .context("Original message missing")?;
    let origin: ReturnRoute = serde_json::from_value(d["origin"].clone())?;
    let (content, transcription) = super::voice::content_on(
        conn,
        &original[0],
        d["human"].as_i64().context("Missing human")?,
    )?;
    if Some(content.as_str()) != d["original_text"].as_str()
        || serde_json::to_value(transcription)?
            != d.get("transcription").cloned().unwrap_or(Value::Null)
        || message.chat.id != origin.chat_id
        || message.message_thread_id.unwrap_or(0) != origin.thread_id
        || message.message_id != origin.message_id
    {
        return Err(anyhow!("Retained creation original changed"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn creation_reserves_one_plan_and_atomically_enrolls_original() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let defaults = super::super::tests::defaults();
        let store = Store::open(file.path(), &[100], &defaults).unwrap();
        let entry = SessionKey::new(100, Some(1));
        let source = store.ensure_session(entry, 100, &defaults).unwrap();
        store
            .set_session_codex_thread(entry, "entry-native")
            .unwrap();
        let update:crate::telegram::Update=serde_json::from_value(json!({"update_id":20,"message":{"message_id":20,"message_thread_id":1,"from":{"id":100,"is_bot":false,"first_name":"Human"},"chat":{"id":100,"type":"private"},"text":"Start a distinct new task"}})).unwrap();
        store.admit_update(&update).unwrap();
        store.begin_update(20).unwrap();
        store.mark_update_queued(20).unwrap();
        let turn = store
            .record_turn_started(
                source.id,
                &TurnRequest {
                    session_key: entry,
                    from_user_id: 100,
                    prompt: "Runtime envelope differs from original".into(),
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
        let token = tokio_util::sync::CancellationToken::new();
        token.cancel();
        let prepare = |token: &tokio_util::sync::CancellationToken| {
            store.interaction_prepare_creation(
                entry,
                100,
                "entry-native",
                20,
                "Gamma",
                "This is a distinct task",
                &json!({"sources":[]}),
                2,
                token,
            )
        };
        assert!(prepare(&token).is_err());
        let token = tokio_util::sync::CancellationToken::new();
        let (intent, fresh) = prepare(&token).unwrap();
        assert!(fresh);
        let id = intent["id"].as_str().unwrap();
        assert!(!prepare(&token).unwrap().1);
        let old = SessionKey::new(100, Some(7));
        let oldrow = store.ensure_session(old, 100, &defaults).unwrap();
        store.set_session_codex_thread(old, "old-native").unwrap();
        let input = SourceInput {
            transcription: None,
            channel: "telegram:fixture".into(),
            event_id: "20".into(),
            update_id: Some(20),
            human_id: 100,
            target_sessions: vec![oldrow.id],
            origin: ReturnRoute {
                chat_id: 100,
                thread_id: 1,
                message_id: 20,
            },
            text: "Start a distinct new task".into(),
        };
        assert!(
            store
                .interaction_admit(&input, old, RequestRelation::New)
                .is_err(),
            "Original creation input became a different dispatch plan"
        );
        store.interaction_creation_suspend(id, 100).unwrap();
        store
            .conn
            .lock()
            .unwrap()
            .execute(
                "UPDATE incoming_updates SET status='undetermined' WHERE update_id=20",
                [],
            )
            .unwrap();
        assert!(
            store
                .interaction_entry_inputs(entry, 100, "entry-native", Some(20))
                .unwrap()
                .is_empty(),
            "Ordinary new dispatch accepted uncertain ingress"
        );
        let cancelled = tokio_util::sync::CancellationToken::new();
        cancelled.cancel();
        assert!(
            store
                .interaction_creation_resume(entry, 100, 20, &cancelled)
                .is_err()
        );
        assert_eq!(
            store
                .interaction_creation_for_input(entry, 100, 20)
                .unwrap()["phase"],
            "retained_unsent"
        );
        assert!(
            store
                .interaction_creation_resume(entry, 100, 20, &token)
                .unwrap()
                .1
        );
        assert!(
            !store
                .interaction_creation_resume(entry, 100, 20, &token)
                .unwrap()
                .1
        );
        store.interaction_creation_start(id, 100).unwrap();
        assert!(
            store.interaction_creation_start(id, 100).is_err(),
            "External topic mutation was replayed"
        );
        store.interaction_creation_topic(id, 100, 77).unwrap();
        store.interaction_creation_suspend(id, 100).unwrap();
        assert_eq!(
            store
                .interaction_creation_for_input(entry, 100, 20)
                .unwrap()["phase"],
            "retained_topic"
        );
        assert!(
            store
                .interaction_creation_resume(entry, 100, 20, &token)
                .unwrap()
                .1
        );
        assert!(
            !store
                .interaction_creation_resume(entry, 100, 20, &token)
                .unwrap()
                .1
        );

        let key = SessionKey::new(100, Some(77));
        store.ensure_session(key, 100, &defaults).unwrap();
        let mut template = source;
        template.codex_thread_id = None;
        template.force_fresh_thread = true;
        template.session_title = Some("Gamma".into());
        store
            .apply_empty_session_template(key, &template, 100)
            .unwrap();
        let instruction = store
            .interaction_creation_finish(id, 100, "telegram:fixture")
            .unwrap();
        assert_eq!(instruction.text, "Start a distinct new task");
        assert_eq!(instruction.target, key);
        assert!(
            !store
                .interaction_creation_resume(entry, 100, 20, &token)
                .unwrap()
                .1
        );
        let status = store
            .interaction_creation_for_input(entry, 100, 20)
            .unwrap();
        assert_eq!(status["instructions"][0]["id"], instruction.id);

        assert!(
            store
                .interaction_creation_finish(id, 100, "telegram:fixture")
                .is_err()
        );
        let cfg = InteractionConfig {
            human_id: 100,
            chat_id: 100,
            topic_id: 1,
            max_active_workers: 2,
            targets: vec![Target {
                name: "Alpha".into(),
                topic_id: 7,
            }],
        };
        assert_eq!(
            store.interaction_profile(&cfg).unwrap().resolve("Gamma"),
            vec![key]
        );
        let empty = InteractionConfig {
            targets: vec![],
            ..cfg.clone()
        };
        empty.validate().unwrap();
        assert_eq!(
            store.interaction_profile(&empty).unwrap().resolve("Gamma"),
            vec![key]
        );
        assert_eq!(
            store
                .interaction_creation_for_input(entry, 100, 20)
                .unwrap()["phase"],
            "ready"
        );
        assert!(
            store
                .interaction_creation_for_input(entry, 101, 20)
                .is_err()
        );
    }
}
