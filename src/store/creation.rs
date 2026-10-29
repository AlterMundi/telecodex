//! Retained creation intents and enrollment in the existing interaction registry.
use super::*;
use crate::interaction::{
    InputReference, InteractionConfig, RequestRelation, ReturnRoute, SourceInput, Target,
};
use anyhow::bail;
use serde_json::{Value, json};

impl Store {
    pub const CREATION_CONTEXT_MAX_BYTES: usize = 48_000;

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
        if base.discover_owned_sessions {
            // Discovery is local metadata only: never enumerate unrelated native
            // history, start workers, or grant a client access to new targets.
            let mut q = conn.prepare("SELECT s.thread_id,s.session_title FROM sessions s JOIN users u ON u.tg_user_id=s.creator_user_id WHERE s.creator_user_id=?1 AND u.allowed=1 AND s.chat_id=?2 AND s.thread_id>0 AND s.thread_id!=?3 AND s.codex_thread_id IS NOT NULL AND s.codex_thread_id!='' AND s.force_fresh_thread=0 ORDER BY s.thread_id")?;
            let rows = q.query_map(params![base.human_id, base.chat_id, base.topic_id], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, Option<String>>(1)?))
            })?;
            for row in rows {
                let (topic_id, title) = row?;
                if cfg.targets.iter().any(|t| t.topic_id == topic_id) {
                    continue;
                }
                let fallback = format!("t{topic_id}");
                let name = title
                    .filter(|name| {
                        !name.is_empty()
                            && name.len() <= 128
                            && !name.chars().any(|c| {
                                c.is_whitespace()
                                    || c.is_control()
                                    || matches!(c, '#' | '`' | '"' | '\'' | '<' | '>')
                            })
                            && name
                                .strip_prefix('t')
                                .or_else(|| name.strip_prefix('T'))
                                .is_none_or(|v| v.parse::<i64>().is_err())
                            && !cfg
                                .matrix_targets
                                .iter()
                                .any(|t| t.name.eq_ignore_ascii_case(name))
                    })
                    .unwrap_or(fallback);
                cfg.targets.push(Target { topic_id, name });
            }
        }
        cfg.validate()?;
        Ok(cfg)
    }

    pub fn interaction_creation(&self, id: &str, human: i64) -> Result<Value> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        creation_on(&conn, id, human)
    }

    #[cfg(test)]
    pub fn interaction_creation_for_input(
        &self,
        key: SessionKey,
        human: i64,
        update: impl Into<InputReference>,
    ) -> Result<Value> {
        self.interaction_find_creation_for_input(key, human, update)?
            .context("No retained creation for this original input")
    }

    /// Absence is distinct from an unreadable or corrupt retained intent.
    pub fn interaction_find_creation_for_input(
        &self,
        key: SessionKey,
        human: i64,
        update: impl Into<InputReference>,
    ) -> Result<Option<Value>> {
        let update = update.into();
        update.validate()?;
        let conn = self.conn.lock().expect("store mutex poisoned");
        let id:Option<String>=conn.query_row("SELECT id FROM interaction_creations WHERE human_id=?1 AND entry_chat=?2 AND entry_topic=?3 AND input_update=?4",params![human,key.chat_id,key.thread_id,update],|r|r.get(0)).optional()?;
        id.map(|id| creation_on(&conn, &id, human)).transpose()
    }

    /// Freeze one original, attributed context package before a topic RPC.
    #[allow(clippy::too_many_arguments)]
    pub fn interaction_prepare_creation(
        &self,
        key: SessionKey,
        human: i64,
        native: &str,
        update: impl Into<InputReference>,
        name: &str,
        reason: &str,
        context: &Value,
        capacity: usize,
        closed: &tokio_util::sync::CancellationToken,
    ) -> Result<(Value, bool)> {
        let update = update.into();
        update.validate()?;
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction()?;
        let (source, _) =
            super::dialogue::original_on(&tx, key, human, native, &update, "", false)?;
        let text = source.text;
        let transcription = source.transcription;
        if text.trim().is_empty()
            || text.trim_start().starts_with('/')
            || context.to_string().len() > Self::CREATION_CONTEXT_MAX_BYTES
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
        let mut doc = json!({"id":id,"human":human,"entry":key,"entry_native":native,"input_update":update,"name":name,"reason":reason,"context":context,"original_text":text,"origin":source.origin});
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
        let duplicate: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM interaction_creations WHERE human_id=?1 AND entry_chat=?2 AND entry_topic=?3 AND lower(json_extract(doc,'$.name'))=lower(?4))",
            params![human,key.chat_id,key.thread_id,name], |r|r.get(0),
        )?;
        if duplicate {
            return Err(anyhow!(
                "A retained creation already reserves this title; inspect it instead of duplicating it"
            ));
        }
        let reserved:i64=tx.query_row("SELECT count(*) FROM interaction_creations WHERE human_id=?1 AND entry_chat=?2 AND entry_topic=?3",params![human,key.chat_id,key.thread_id],|r|r.get(0))?;
        if reserved >= capacity as i64 {
            return Err(anyhow!("Creation capacity is already reserved"));
        }
        let routed: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM interaction_sources WHERE human_id=?1 AND ((update_id=?2 AND ?2 IS NOT NULL) OR (channel='conversation' AND event_id=?3 AND ?2 IS NULL)))",
            params![human, update.update(),update.event()],
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
        update: impl Into<InputReference>,
        closed: &tokio_util::sync::CancellationToken,
    ) -> Result<(Value, bool)> {
        let update = update.into();
        update.validate()?;
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

    /// Allocate a destination in this standalone receiver, without a Telegram
    /// effect. Reservation and receipt share one transaction, so a lost return
    /// resumes the same destination rather than allocating another one.
    pub fn interaction_creation_local_topic(
        &self,
        id: &str,
        human: i64,
        reserved: &[i64],
    ) -> Result<i64> {
        if reserved.len() > 64 || reserved.iter().any(|id| *id <= 0) {
            bail!("Invalid configured destination reservations");
        }
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction()?;
        let operation = creation_on(&tx, id, human)?;
        eligible(&tx, &operation)?;
        let current: bool = tx.query_row(
            "SELECT instance_id=?1 FROM interaction_creations WHERE id=?2 AND human_id=?3",
            params![self.instance_id, id, human],
            |r| r.get(0),
        )?;
        if !current {
            bail!("Creation is not claimed by this receiver");
        }
        if operation["phase"] == "topic_known" {
            return operation["topic"]
                .as_i64()
                .context("Local destination receipt missing");
        }
        if operation["phase"] != "prepared" {
            bail!("Local creation is not claimed for execution");
        }
        let chat = operation["intent"]["entry"]["chat_id"]
            .as_i64()
            .context("Missing entry")?;
        let previous: i64 = tx.query_row(
            "SELECT MAX(value) FROM (SELECT 0 AS value UNION ALL SELECT thread_id FROM sessions WHERE chat_id=?1 UNION ALL SELECT topic FROM interaction_creations WHERE entry_chat=?1 AND topic IS NOT NULL)",
            [chat], |r| r.get(0),
        )?;
        let previous = previous.max(reserved.iter().copied().max().unwrap_or(0));
        let topic = previous
            .checked_add(1)
            .context("Local destination namespace exhausted")?;
        if tx.execute("UPDATE interaction_creations SET topic=?1,phase='topic_known' WHERE id=?2 AND human_id=?3 AND phase='prepared' AND instance_id=?4",params![topic,id,human,self.instance_id])? != 1 {
            bail!("Local destination reservation changed");
        }
        tx.commit()?;
        Ok(topic)
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
            channel: if d["input_update"].is_string() {
                "conversation".into()
            } else {
                channel.into()
            },
            event_id: serde_json::from_value::<InputReference>(d["input_update"].clone())?.event(),
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
        tx.execute("INSERT INTO interaction_enrollments(human_id,entry_chat,entry_topic,target_topic,session_id,name,creation_id) VALUES(?1,?2,?3,?4,?5,?6,?7)",params![human,entry.chat_id,entry.thread_id,topic,session,if d["name"].as_str().is_some_and(|name|name.contains(' ')) {format!("t{topic}")} else {d["name"].as_str().context("Missing retained name")?.to_owned()},id])?;
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
    let reference: InputReference = serde_json::from_value(intent["input_update"].clone())?;
    let mut q=conn.prepare("SELECT i.id,i.phase FROM interaction_instructions i JOIN interaction_sources s ON s.id=i.input_id JOIN sessions destination ON destination.id=i.session_id WHERE s.human_id=?1 AND ((s.update_id=?2 AND ?2 IS NOT NULL) OR (s.channel='conversation' AND s.event_id=?5 AND ?2 IS NULL)) AND destination.chat_id=?3 AND destination.thread_id=?4 ORDER BY i.sequence LIMIT 1")?;
    let rows = q.query_map(
        params![
            human,
            reference.update(),
            intent["entry"]["chat_id"].as_i64(),
            topic,
            reference.event()
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
    let reference: InputReference = serde_json::from_value(d["input_update"].clone())?;
    let (original, _) = super::dialogue::original_on(
        conn,
        key,
        d["human"].as_i64().context("Missing human")?,
        d["entry_native"].as_str().context("Missing native")?,
        &reference,
        "",
        true,
    )?;
    let origin: ReturnRoute = serde_json::from_value(d["origin"].clone())?;
    if Some(original.text.as_str()) != d["original_text"].as_str()
        || serde_json::to_value(original.transcription)?
            != d.get("transcription").cloned().unwrap_or(Value::Null)
        || original.origin != origin
    {
        bail!("Retained creation original changed");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn discovery_is_opt_in_owned_bound_current_and_preserves_explicit_names() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let defaults = super::super::tests::defaults();
        let store = Store::open(file.path(), &[100, 101], &defaults).unwrap();
        for (chat, topic, owner, title) in [
            (100, 1, 100, "Entry"),
            (100, 2, 100, "Duplicate"),
            (100, 3, 100, "Duplicate"),
            (100, 4, 101, "OtherHuman"),
            (101, 5, 100, "OtherChat"),
            (100, 6, 100, "Unbound"),
            (100, 7, 100, "t999"),
            (100, 8, 100, "Two Words"),
            (100, 9, 100, "Retired"),
            (100, 10, 100, "OriginalTitle"),
        ] {
            let key = SessionKey::new(chat, Some(topic));
            store.ensure_session(key, owner, &defaults).unwrap();
            store.set_session_title(key, Some(title)).unwrap();
            if topic != 6 {
                store
                    .set_session_codex_thread(key, &format!("native-{topic}"))
                    .unwrap();
            }
        }
        store
            .conn
            .lock()
            .unwrap()
            .execute(
                "UPDATE sessions SET force_fresh_thread=1 WHERE thread_id=9",
                [],
            )
            .unwrap();
        let mut cfg = InteractionConfig {
            human_id: 100,
            chat_id: 100,
            topic_id: 1,
            telegram_ingress: false,
            telegram_projection: false,
            telegram_entry_projection: false,
            native_workflow_returns: false,
            native_routed_returns: false,
            artifact_store: None,
            max_active_workers: 2,
            discover_owned_sessions: false,
            observer_model: None,
            expression_pack: None,
            observer_reasoning_effort: None,
            matrix_targets: vec![],
            matrix_transport: None,
            targets: vec![Target {
                name: "Explicit".into(),
                topic_id: 10,
            }],
        };
        assert_eq!(store.interaction_profile(&cfg).unwrap().targets.len(), 1);
        cfg.discover_owned_sessions = true;
        let discovered = store.interaction_profile(&cfg).unwrap();
        assert_eq!(
            discovered
                .targets
                .iter()
                .map(|t| t.topic_id)
                .collect::<Vec<_>>(),
            vec![10, 2, 3, 7, 8]
        );
        assert_eq!(discovered.resolve("Duplicate").len(), 2);
        assert_eq!(
            discovered.resolve("t3"),
            vec![SessionKey::new(100, Some(3))]
        );
        assert_eq!(discovered.targets[0].name, "Explicit");
        assert_eq!(discovered.targets[3].name, "t7");
        assert_eq!(discovered.targets[4].name, "t8");
        assert!(discovered.resolve("t999").is_empty());
        store
            .conn
            .lock()
            .unwrap()
            .execute(
                "UPDATE sessions SET creator_user_id=101 WHERE thread_id=2",
                [],
            )
            .unwrap();
        assert!(
            store
                .interaction_profile(&cfg)
                .unwrap()
                .resolve("t2")
                .is_empty()
        );
        store
            .conn
            .lock()
            .unwrap()
            .execute("UPDATE users SET allowed=0 WHERE tg_user_id=100", [])
            .unwrap();
        assert_eq!(store.interaction_profile(&cfg).unwrap().targets.len(), 1);
    }

    #[test]
    fn standalone_creation_retains_one_local_destination_across_restart() {
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

        // Reserve against both current sessions and other retained creations.
        store
            .ensure_session(SessionKey::new(100, Some(90)), 100, &defaults)
            .unwrap();
        assert!(
            store
                .interaction_creation_local_topic(id, 101, &[100])
                .is_err()
        );
        let topic = store
            .interaction_creation_local_topic(id, 100, &[100])
            .unwrap();
        assert_eq!(topic, 101);
        assert!(
            store
                .get_session(SessionKey::new(100, Some(100)))
                .unwrap()
                .is_none()
        );
        assert_eq!(
            store
                .interaction_creation_local_topic(id, 100, &[100])
                .unwrap(),
            topic
        );
        store.interaction_creation_suspend(id, 100).unwrap();
        assert_eq!(
            store.interaction_creation(id, 100).unwrap()["phase"],
            "retained_topic"
        );
        assert!(
            store
                .interaction_creation_local_topic(id, 100, &[100])
                .is_err()
        );
        let id = id.to_owned();
        drop(store);
        let store = Store::open(file.path(), &[100], &defaults).unwrap();
        let (retained, claimed) = store
            .interaction_creation_resume(entry, 100, 20, &token)
            .unwrap();
        assert!(claimed);
        assert_eq!(retained["topic"], topic);
        assert_eq!(
            store
                .interaction_creation_local_topic(&id, 100, &[100])
                .unwrap(),
            topic
        );
        let key = SessionKey::new(100, Some(topic));
        store.ensure_session(key, 100, &defaults).unwrap();
        let mut template = source;
        template.codex_thread_id = None;
        template.force_fresh_thread = true;
        template.session_title = Some("Gamma".into());
        store
            .apply_empty_session_template(key, &template, 100)
            .unwrap();
        let instruction = store
            .interaction_creation_finish(&id, 100, "unused-disabled-telegram")
            .unwrap();
        assert_eq!(instruction.text, "Start a distinct new task");
        assert_eq!(instruction.target, key);
        assert_eq!(
            store.interaction_creation(&id, 100).unwrap()["phase"],
            "ready"
        );
        assert!(
            store
                .interaction_creation_local_topic(&id, 100, &[100])
                .is_err()
        );
        let cfg: InteractionConfig = serde_json::from_value(json!({
            "human_id":100,"chat_id":100,"topic_id":1,
            "targets":[{"topic_id":100,"name":"Existing"}]
        }))
        .unwrap();
        let profile = store.interaction_profile(&cfg).unwrap();
        assert_eq!(
            profile.resolve("Existing"),
            vec![SessionKey::new(100, Some(100))]
        );
        assert_eq!(profile.resolve("Gamma"), vec![key]);
    }

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
                message_id: Some(20),
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
            telegram_ingress: false,
            telegram_projection: false,
            telegram_entry_projection: false,
            native_workflow_returns: false,
            native_routed_returns: false,
            artifact_store: None,
            max_active_workers: 2,
            discover_owned_sessions: false,
            observer_model: None,
            expression_pack: None,
            observer_reasoning_effort: None,
            matrix_targets: vec![],
            matrix_transport: None,
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
