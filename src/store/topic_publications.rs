//! A secondary topic projection never admits input or controls native execution.
use super::*;
use serde_json::json;

#[derive(Debug)]
pub struct TopicPublication {
    pub id: i64,
    pub local_turn: i64,
    pub direct: bool,
    pub target: SessionKey,
    pub body: String,
    pub kind: String,
    pub source: String,
}

pub(super) fn enqueue_on(
    conn: &Connection,
    local: i64,
    kind: &str,
    source: &str,
    body: &str,
) -> Result<()> {
    if body.trim().is_empty() {
        return Ok(());
    }
    conn.execute("INSERT OR IGNORE INTO topic_publications(local_turn,session_id,human_id,kind,source,body,phase,instance_id)
        SELECT p.local_turn,t.session_id,p.human_id,?2,?3,?4,'ready',?5
        FROM topic_projection_turns p JOIN turns t ON t.id=p.local_turn WHERE p.local_turn=?1",
        params![local,kind,source,body,""])?;
    Ok(())
}

pub(super) fn native_on(
    conn: &Connection,
    local: i64,
    thread: &str,
    turn: &str,
    item: &str,
    text: &str,
) -> Result<()> {
    let legacy:bool=conn.query_row("SELECT EXISTS(SELECT 1 FROM native_recovery_baseline WHERE native_thread=?1 AND native_turn=?2 AND item_id=?3)",params![thread,turn,item],|r|r.get(0))?;
    if legacy {
        return Ok(());
    }
    enqueue_on(
        conn,
        local,
        "native",
        &json!([thread, turn, item]).to_string(),
        crate::interaction::visible_answer(text),
    )
}

pub(super) fn accepted_on(
    conn: &Connection,
    input: &crate::interaction::Instruction,
) -> Result<()> {
    if input.origin.chat_id == input.target.chat_id
        && input.origin.thread_id == input.target.thread_id
    {
        return Ok(());
    }
    let Some(local) = input.local_turn else {
        return Ok(());
    };
    let source: String = conn.query_row(
        "SELECT doc FROM interaction_sources WHERE id=?",
        [&input.input_id],
        |r| r.get(0),
    )?;
    let source: crate::interaction::SourceInput = serde_json::from_str(&source)?;
    let author = if matches!(
        source.channel.as_str(),
        "local-session" | "local-session-result"
    ) {
        "Session message"
    } else if source.transcription.is_some() {
        "Human voice transcript"
    } else {
        "Human message"
    };
    // Original input is source data, including any protocol-looking quotation.
    let body = format!("↪ {author} · via integrator\n\n{}", input.text);
    enqueue_on(conn, local, "input", &input.id, &body)
}

impl Store {
    pub(super) fn init_topic_publications(&self) -> Result<()> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        conn.execute_batch("CREATE TABLE IF NOT EXISTS topic_projection_turns(local_turn INTEGER PRIMARY KEY REFERENCES turns(id),human_id INTEGER NOT NULL);
            CREATE TABLE IF NOT EXISTS topic_publications(id INTEGER PRIMARY KEY AUTOINCREMENT,local_turn INTEGER NOT NULL REFERENCES turns(id),session_id INTEGER NOT NULL REFERENCES sessions(id),human_id INTEGER NOT NULL,kind TEXT NOT NULL,source TEXT NOT NULL,body TEXT NOT NULL,phase TEXT NOT NULL,instance_id TEXT NOT NULL,UNIQUE(kind,source,session_id));
            CREATE INDEX IF NOT EXISTS topic_publications_pending ON topic_publications(session_id,phase,id);
            CREATE TABLE IF NOT EXISTS topic_publication_parts(publication INTEGER NOT NULL REFERENCES topic_publications(id),part INTEGER NOT NULL,body TEXT NOT NULL,phase TEXT NOT NULL,message_id INTEGER,PRIMARY KEY(publication,part));")?;
        conn.execute_batch("CREATE TABLE IF NOT EXISTS direct_topic_turns(local_turn INTEGER PRIMARY KEY REFERENCES topic_projection_turns(local_turn));")?;
        conn.execute("UPDATE topic_publications SET phase=CASE WHEN EXISTS(SELECT 1 FROM topic_publication_parts p WHERE p.publication=topic_publications.id AND p.phase IN ('sending','uncertain')) THEN 'uncertain' ELSE 'ready' END WHERE phase='publishing' AND instance_id!=?",[&self.instance_id])?;
        conn.execute("UPDATE topic_publication_parts SET phase='uncertain' WHERE phase='sending' AND publication IN (SELECT id FROM topic_publications WHERE phase='uncertain')",[])?;
        Ok(())
    }

    pub fn topic_projection_register(&self, local: i64, human: i64, key: SessionKey) -> Result<()> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let valid: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM turns t JOIN sessions s ON s.id=t.session_id JOIN users u ON u.tg_user_id=s.creator_user_id WHERE t.id=?1 AND t.from_user_id=?2 AND s.creator_user_id=?2 AND u.allowed=1 AND s.chat_id=?3 AND s.thread_id=?4)",params![local,human,key.chat_id,key.thread_id],|r|r.get(0))?;
        if !valid {
            return Err(anyhow!("Topic projection ownership changed"));
        }
        conn.execute(
            "INSERT OR IGNORE INTO topic_projection_turns(local_turn,human_id) VALUES(?1,?2)",
            params![local, human],
        )?;
        Ok(())
    }

    pub fn topic_direct_register(&self, local: i64, human: i64, key: SessionKey) -> Result<()> {
        self.topic_projection_register(local, human, key)?;
        self.conn.lock().expect("store mutex poisoned").execute(
            "INSERT OR IGNORE INTO direct_topic_turns(local_turn) VALUES(?)",
            [local],
        )?;
        Ok(())
    }

    pub fn topic_publication_owner(&self, key: SessionKey) -> Result<Option<i64>> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        Ok(conn.query_row("SELECT s.creator_user_id FROM sessions s JOIN users u ON u.tg_user_id=s.creator_user_id WHERE s.chat_id=?1 AND s.thread_id=?2 AND u.allowed=1",params![key.chat_id,key.thread_id],|r|r.get(0)).optional()?)
    }

    pub fn topic_publication_notice(&self, local: i64, text: &str) -> Result<()> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        enqueue_on(&conn, local, "notice", &format!("collector:{local}"), text)
    }

    pub fn topic_projection_active(&self, local: i64) -> Result<bool> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        Ok(conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM topic_projection_turns WHERE local_turn=?)",
            [local],
            |r| r.get(0),
        )?)
    }

    pub fn topic_publication_targets(&self) -> Result<Vec<SessionKey>> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let mut query=conn.prepare("SELECT DISTINCT s.chat_id,s.thread_id FROM topic_publications p JOIN sessions s ON s.id=p.session_id WHERE p.phase='ready' LIMIT 512")?;
        Ok(query
            .query_map([], |r| Ok(SessionKey::new(r.get(0)?, Some(r.get(1)?))))?
            .collect::<rusqlite::Result<_>>()?)
    }

    /// SQLite serializes the queue per destination, independent of its producer.
    pub fn topic_publication_claim(
        &self,
        key: SessionKey,
        human: i64,
    ) -> Result<Option<TopicPublication>> {
        self.topic_publication_claim_lane(key, human, false)
    }
    pub fn topic_publication_claim_files(
        &self,
        key: SessionKey,
        human: i64,
    ) -> Result<Option<TopicPublication>> {
        self.topic_publication_claim_lane(key, human, true)
    }
    fn topic_publication_claim_lane(
        &self,
        key: SessionKey,
        human: i64,
        files: bool,
    ) -> Result<Option<TopicPublication>> {
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let busy: bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM topic_publications p JOIN sessions s ON s.id=p.session_id WHERE s.chat_id=?1 AND s.thread_id=?2 AND p.phase='publishing' AND (p.kind='artifact')=?3)",params![key.chat_id,key.thread_id,files],|r|r.get(0))?;
        if busy {
            return Ok(None);
        }
        loop {
            let row:Option<(i64,String,bool,String,String,i64,bool)>=tx.query_row("SELECT p.id,p.body,COALESCE((u.allowed=1 AND s.creator_user_id=p.human_id AND t.from_user_id=p.human_id AND (((p.kind!='artifact' AND s.codex_thread_id=t.native_thread_id AND s.force_fresh_thread=0) OR (p.kind='notice' AND t.native_thread_id IS NULL)) OR (p.kind='artifact' AND EXISTS(SELECT 1 FROM interaction_artifacts a JOIN interaction_artifact_blobs b ON b.sha256=a.sha256 WHERE a.id=p.source AND a.source_local=p.local_turn AND a.source_session=p.session_id AND a.human_id=p.human_id AND a.phase='ready' AND b.present=1 AND julianday(a.expires_at)>julianday('now'))))),0),p.kind,p.source,p.local_turn,EXISTS(SELECT 1 FROM direct_topic_turns pt WHERE pt.local_turn=p.local_turn) FROM topic_publications p JOIN sessions s ON s.id=p.session_id JOIN turns t ON t.id=p.local_turn JOIN users u ON u.tg_user_id=p.human_id WHERE s.chat_id=?1 AND s.thread_id=?2 AND p.human_id=?3 AND p.phase='ready' AND (p.kind='artifact')=?4 ORDER BY p.id LIMIT 1",params![key.chat_id,key.thread_id,human,files],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?))).optional()?;
            let Some((id, body, valid, kind, source, local_turn, direct)) = row else {
                tx.commit()?;
                return Ok(None);
            };
            tx.execute(
                "UPDATE topic_publications SET phase=?2,instance_id=?3 WHERE id=?1",
                params![
                    id,
                    if valid { "publishing" } else { "retired" },
                    &self.instance_id
                ],
            )?;
            if valid {
                tx.commit()?;
                return Ok(Some(TopicPublication {
                    id,
                    local_turn,
                    direct,
                    target: key,
                    body,
                    kind,
                    source,
                }));
            }
        }
    }

    pub fn topic_file_caption(&self, id: i64) -> Result<String> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let (name,text):(String,String)=conn.query_row("SELECT a.name,m.text FROM topic_publications p JOIN interaction_artifacts a ON a.id=p.source JOIN interaction_native_messages m ON m.local_turn=a.source_local AND m.native_thread=a.native_thread AND m.native_turn=a.native_turn AND m.item_id=a.native_item WHERE p.id=?1 AND p.kind='artifact'",[id],|r|Ok((r.get(0)?,r.get(1)?)))?;
        Ok(format!(
            "File · {}\n{}",
            name,
            crate::interaction::visible_answer(&text)
                .chars()
                .take(240)
                .collect::<String>()
        ))
    }

    pub fn topic_publication_parts(
        &self,
        id: i64,
        chunks: &[String],
    ) -> Result<Vec<(i64, String)>> {
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction()?;
        let own:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM topic_publications WHERE id=?1 AND phase='publishing' AND instance_id=?2)",params![id,&self.instance_id],|r|r.get(0))?;
        if !own {
            return Err(anyhow!("Topic publication lease changed"));
        }
        let count: i64 = tx.query_row(
            "SELECT COUNT(*) FROM topic_publication_parts WHERE publication=?",
            [id],
            |r| r.get(0),
        )?;
        if count == 0 {
            if chunks.is_empty() {
                return Err(anyhow!("Invalid topic publication parts"));
            }
            for (part, body) in chunks.iter().enumerate() {
                tx.execute(
                    "INSERT INTO topic_publication_parts VALUES(?1,?2,?3,'ready',NULL)",
                    params![id, part as i64, body],
                )?;
            }
        }
        let parts = {
            let mut q=tx.prepare("SELECT part,body FROM topic_publication_parts WHERE publication=? AND phase='ready' ORDER BY part")?;
            q.query_map([id], |r| Ok((r.get(0)?, r.get(1)?)))?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        tx.commit()?;
        Ok(parts)
    }

    /// Called after Telegram cooldown, immediately before one network attempt.
    pub fn topic_publication_reserve(&self, id: i64, part: i64) -> Result<bool> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        Ok(conn.execute("UPDATE topic_publication_parts SET phase='sending' WHERE publication=?1 AND part=?2 AND phase='ready' AND EXISTS(SELECT 1 FROM topic_publications p JOIN turns t ON t.id=p.local_turn JOIN sessions s ON s.id=p.session_id JOIN users u ON u.tg_user_id=p.human_id WHERE p.id=?1 AND p.phase='publishing' AND p.instance_id=?3 AND u.allowed=1 AND s.creator_user_id=p.human_id AND t.from_user_id=p.human_id AND (((p.kind!='artifact' AND s.codex_thread_id=t.native_thread_id AND s.force_fresh_thread=0) OR (p.kind='notice' AND t.native_thread_id IS NULL)) OR (p.kind='artifact' AND EXISTS(SELECT 1 FROM interaction_artifacts a JOIN interaction_artifact_blobs b ON b.sha256=a.sha256 WHERE a.id=p.source AND a.source_local=p.local_turn AND a.source_session=p.session_id AND a.human_id=p.human_id AND a.phase='ready' AND b.present=1 AND julianday(a.expires_at)>julianday('now')))))",params![id,part,&self.instance_id])?==1)
    }

    pub fn topic_publication_receipt(
        &self,
        id: i64,
        part: i64,
        message: Option<i64>,
        rejected: bool,
    ) -> Result<()> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let changed=conn.execute("UPDATE topic_publication_parts SET phase=?3,message_id=?4 WHERE publication=?1 AND part=?2 AND phase='sending' AND EXISTS(SELECT 1 FROM topic_publications p WHERE p.id=?1 AND p.phase='publishing' AND p.instance_id=?5)",params![id,part,if message.is_some(){"sent"}else if rejected{"rejected"}else{"uncertain"},message,&self.instance_id])?;
        if changed != 1 {
            return Err(anyhow!("Topic publication receipt ownership changed"));
        }
        if let Some(message) = message {
            conn.execute("INSERT OR IGNORE INTO handoff_messages(chat_id,thread_id,message_id,turn_id,text) SELECT s.chat_id,s.thread_id,?3,p.local_turn,x.body FROM topic_publications p JOIN sessions s ON s.id=p.session_id JOIN topic_publication_parts x ON x.publication=p.id WHERE p.id=?1 AND x.part=?2",params![id,part,message])?;
        }
        Ok(())
    }

    pub fn topic_publication_finish(&self, id: i64) -> Result<()> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        conn.execute("UPDATE topic_publications SET phase=CASE WHEN EXISTS(SELECT 1 FROM topic_publication_parts p WHERE p.publication=?1 AND p.phase IN ('sending','uncertain')) THEN 'uncertain' WHEN EXISTS(SELECT 1 FROM topic_publication_parts p WHERE p.publication=?1 AND p.phase='rejected') THEN 'rejected' WHEN NOT EXISTS(SELECT 1 FROM topic_publication_parts p WHERE p.publication=?1 AND p.phase!='sent') THEN 'sent' ELSE 'retired' END WHERE id=?1 AND phase='publishing' AND instance_id=?2",params![id,&self.instance_id])?;
        Ok(())
    }

    pub fn topic_publication_release(&self, id: i64) -> Result<()> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        conn.execute("UPDATE topic_publications SET phase=CASE WHEN EXISTS(SELECT 1 FROM topic_publication_parts p WHERE p.publication=?1 AND p.phase IN ('sending','uncertain')) THEN 'uncertain' ELSE 'ready' END WHERE id=?1 AND phase='publishing' AND instance_id=?2",params![id,&self.instance_id])?;
        Ok(())
    }

    pub fn topic_publication_retry(&self, id: i64, part: i64) -> Result<()> {
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction()?;
        let changed=tx.execute("UPDATE topic_publication_parts SET phase='ready' WHERE publication=?1 AND part=?2 AND phase='sending' AND EXISTS(SELECT 1 FROM topic_publications p WHERE p.id=?1 AND p.phase='publishing' AND p.instance_id=?3)",params![id,part,&self.instance_id])?;
        if changed != 1 {
            return Err(anyhow!("Topic retry lease changed"));
        }
        tx.execute(
            "UPDATE topic_publications SET phase='ready' WHERE id=?1 AND instance_id=?2",
            params![id, &self.instance_id],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn topic_publication_inventory(&self, human: i64) -> Result<serde_json::Value> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let mut query=conn.prepare("SELECT p.id,s.chat_id,s.thread_id,p.kind,p.phase,p.body,(SELECT COUNT(*) FROM topic_publication_parts x WHERE x.publication=p.id AND x.phase='sent') FROM topic_publications p JOIN sessions s ON s.id=p.session_id JOIN users u ON u.tg_user_id=p.human_id WHERE p.human_id=? AND s.creator_user_id=p.human_id AND u.allowed=1 ORDER BY p.id DESC LIMIT 256")?;
        let items=query.query_map([human],|r|{let body:String=r.get(5)?;Ok(json!({"id":r.get::<_,i64>(0)?,"chat":r.get::<_,i64>(1)?,"topic":r.get::<_,i64>(2)?,"kind":r.get::<_,String>(3)?,"phase":r.get::<_,String>(4)?,"preview":body.chars().take(300).collect::<String>(),"acknowledged_parts":r.get::<_,i64>(6)?}))})?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(json!({"schema":"telecodex.topic-publications/v1","items":items,"bounded":256}))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn execution(store: &Store) -> (i64, SessionKey) {
        let key = SessionKey::new(100, Some(7));
        let session = store
            .ensure_session(key, 100, &super::super::tests::defaults())
            .unwrap();
        store.set_session_codex_thread(key, "native").unwrap();
        let local = store
            .record_turn_started(
                session.id,
                &TurnRequest {
                    session_key: key,
                    from_user_id: 100,
                    prompt: "Work".into(),
                    runtime_instructions: None,
                    attachments: vec![],
                    review_mode: None,
                    override_search_mode: None,
                },
            )
            .unwrap();
        store.record_turn_native_thread(local, "native").unwrap();
        store.topic_projection_register(local, 100, key).unwrap();
        (local, key)
    }
    fn store(path: &Path) -> Store {
        Store::open(path, &[100, 101], &super::super::tests::defaults()).unwrap()
    }
    #[test]
    fn pre_native_failure_notice_needs_owner_scope_but_no_native_receipt() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let s = store(file.path());
        let (local, key) = execution(&s);
        // Preparation can fail before ThreadStarted even for an already-bound
        // session. A notice does not claim native output or input acceptance.
        s.conn
            .lock()
            .unwrap()
            .execute("UPDATE turns SET native_thread_id=NULL WHERE id=?", [local])
            .unwrap();
        s.topic_direct_register(local, 100, key).unwrap();
        s.topic_publication_notice(local, "Native preparation unavailable")
            .unwrap();
        assert!(s.topic_publication_claim(key, 101).unwrap().is_none());
        let p = s.topic_publication_claim(key, 100).unwrap().unwrap();
        s.topic_publication_parts(p.id, &[p.body]).unwrap();
        s.conn
            .lock()
            .unwrap()
            .execute("UPDATE users SET allowed=0 WHERE tg_user_id=100", [])
            .unwrap();
        assert!(!s.topic_publication_reserve(p.id, 0).unwrap());
        s.conn
            .lock()
            .unwrap()
            .execute("UPDATE users SET allowed=1 WHERE tg_user_id=100", [])
            .unwrap();
        assert!(s.topic_publication_reserve(p.id, 0).unwrap());
        s.topic_publication_receipt(p.id, 0, Some(900), false)
            .unwrap();
        s.topic_publication_finish(p.id).unwrap();
    }
    #[test]
    fn topic_publication_identity_order_parts_and_unknown_ack_survive_restart() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let s = store(file.path());
        let (local, key) = execution(&s);
        let text = "Answer\n<daimon-result>{\"requests\":[],\"covered\":[]}</daimon-result>";
        for _ in 0..2 {
            s.record_native_message(local, "native", "turn", "A", Some("final_answer"), text)
                .unwrap();
        }
        s.record_native_message(
            local,
            "native",
            "turn",
            "B",
            Some("commentary"),
            "Another item",
        )
        .unwrap();
        let a = s.topic_publication_claim(key, 100).unwrap().unwrap();
        assert_eq!(a.body, "Answer");
        assert!(s.topic_publication_claim(key, 100).unwrap().is_none());
        let parts = s
            .topic_publication_parts(a.id, &["One".into(), "Two".into()])
            .unwrap();
        assert_eq!(parts.len(), 2);
        assert!(s.topic_publication_reserve(a.id, 0).unwrap());
        s.topic_publication_receipt(a.id, 0, Some(901), false)
            .unwrap();
        assert!(s.topic_publication_reserve(a.id, 1).unwrap());
        drop(s); // Accepted send with no receipt: never replay its second part.
        let s = store(file.path());
        let b = s.topic_publication_claim(key, 100).unwrap().unwrap();
        assert_ne!(a.id, b.id);
        let inventory = s.topic_publication_inventory(100).unwrap();
        let rows = inventory["items"].as_array().unwrap();
        let old = rows.iter().find(|row| row["id"] == a.id).unwrap();
        assert_eq!(old["phase"], "uncertain");
        assert_eq!(old["acknowledged_parts"], 1);
        assert!(
            s.topic_publication_inventory(101).unwrap()["items"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert!(!s.topic_publication_reserve(a.id, 1).unwrap());
    }
    #[test]
    fn topic_publication_known_unsent_and_acknowledged_prefix_resume_without_reformat() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let s = store(file.path());
        let (local, key) = execution(&s);
        s.record_native_message(local, "native", "turn", "A", None, "Whole answer")
            .unwrap();
        let a = s.topic_publication_claim(key, 100).unwrap().unwrap();
        s.topic_publication_parts(a.id, &["First".into(), "Second".into()])
            .unwrap();
        assert!(s.topic_publication_reserve(a.id, 0).unwrap());
        s.topic_publication_receipt(a.id, 0, Some(900), false)
            .unwrap();
        drop(s); // No attempt for part 1; continuing publication is not work replay.
        let s = store(file.path());
        let restored = s.topic_publication_claim(key, 100).unwrap().unwrap();
        assert_eq!(restored.id, a.id);
        assert_eq!(
            s.topic_publication_parts(a.id, &["Changed renderer".into()])
                .unwrap(),
            vec![(1, "Second".into())]
        );
        assert!(s.topic_publication_reserve(a.id, 1).unwrap());
        s.topic_publication_retry(a.id, 1).unwrap(); // An explicit rejection can retry.
        let retry = s.topic_publication_claim(key, 100).unwrap().unwrap();
        assert_eq!(retry.id, a.id);
        assert!(s.topic_publication_reserve(a.id, 1).unwrap());
        s.topic_publication_receipt(a.id, 1, Some(901), false)
            .unwrap();
        s.topic_publication_finish(a.id).unwrap();
        assert!(s.topic_publication_claim(key, 100).unwrap().is_none());
        assert_eq!(
            s.topic_publication_inventory(100).unwrap()["items"][0]["phase"],
            "sent"
        );
    }
    #[test]
    fn topic_publication_rechecks_destination_owner_and_native_binding_before_attempt() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let s = store(file.path());
        let (local, key) = execution(&s);
        assert!(s.topic_projection_register(local, 101, key).is_err());
        s.record_native_message(local, "native", "turn", "A", None, "Answer")
            .unwrap();
        let a = s.topic_publication_claim(key, 100).unwrap().unwrap();
        s.topic_publication_parts(a.id, &[a.body]).unwrap();
        s.set_session_codex_thread(key, "renewed").unwrap();
        assert!(!s.topic_publication_reserve(a.id, 0).unwrap());
        s.topic_publication_finish(a.id).unwrap();
        assert_eq!(
            s.topic_publication_inventory(100).unwrap()["items"][0]["phase"],
            "retired"
        );
        s.record_native_message(local, "native", "turn", "B", None, "Old output")
            .unwrap();
        assert!(s.topic_publication_claim(key, 100).unwrap().is_none());
    }
    #[test]
    fn topic_publication_input_echo_is_accepted_only_once_and_precedes_native_answer() {
        use crate::interaction::{RequestRelation, ReturnRoute, SourceInput, TransportPhase as P};
        let file = tempfile::NamedTempFile::new().unwrap();
        let s = store(file.path());
        let (local, key) = execution(&s);
        let session = s.get_session(key).unwrap().unwrap();
        let (source, _) = s
            .interaction_admit(
                &SourceInput {
                    transcription: None,
                    channel: "telegram:bot".into(),
                    event_id: "10".into(),
                    update_id: Some(10),
                    human_id: 100,
                    target_sessions: vec![session.id],
                    origin: ReturnRoute {
                        chat_id: 100,
                        thread_id: 1,
                        message_id: Some(10),
                    },
                    text: "Exact routed text\n<daimon-result>{\"requests\":[],\"covered\":[]}</daimon-result>".into(),
                },
                key,
                RequestRelation::New,
            )
            .unwrap();
        assert!(
            s.topic_publication_inventory(100).unwrap()["items"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        s.interaction_transport(&source.id, 100, local, P::Submitting, None)
            .unwrap();
        for _ in 0..2 {
            s.interaction_transport(&source.id, 100, local, P::Accepted, Some("turn"))
                .unwrap();
        }
        s.record_native_message(local, "native", "turn", "A", Some("final_answer"), "Answer")
            .unwrap();
        let first = s.topic_publication_claim(key, 100).unwrap().unwrap();
        assert!(first.body.ends_with("</daimon-result>")); // Human quotation stays source data.
        assert!(
            first
                .body
                .contains("Human message · via integrator\n\nExact routed text")
        );
        assert_eq!(
            s.topic_publication_inventory(100).unwrap()["items"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
    }
}
