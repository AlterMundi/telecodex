//! Immutable output references and logical audiences, independent of delivery.
use super::*;
use serde_json::{Value, json};

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct ArtifactRecord {
    pub id: String,
    pub name: String,
    pub sha256: String,
    pub bytes: i64,
    pub expires_at: String,
    pub source_session: i64,
    pub source_local: i64,
    pub native_thread: String,
    pub native_turn: String,
    pub native_item: String,
}

impl Store {
    pub(super) fn init_artifacts(&self) -> Result<()> {
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        conn.execute_batch("CREATE TABLE IF NOT EXISTS interaction_artifact_blobs(sha256 TEXT PRIMARY KEY,bytes INTEGER NOT NULL,present INTEGER NOT NULL);
            CREATE TABLE IF NOT EXISTS interaction_artifacts(id TEXT PRIMARY KEY,source_local INTEGER NOT NULL REFERENCES turns(id),source_session INTEGER NOT NULL REFERENCES sessions(id),human_id INTEGER NOT NULL,native_thread TEXT NOT NULL,native_turn TEXT NOT NULL,native_item TEXT NOT NULL,name TEXT NOT NULL,sha256 TEXT REFERENCES interaction_artifact_blobs(sha256),bytes INTEGER,expires_at TEXT NOT NULL,phase TEXT NOT NULL,UNIQUE(native_thread,native_turn,native_item,name));
            CREATE TABLE IF NOT EXISTS interaction_artifact_audiences(artifact TEXT NOT NULL REFERENCES interaction_artifacts(id),session_id INTEGER NOT NULL REFERENCES sessions(id),human_id INTEGER NOT NULL,PRIMARY KEY(artifact,session_id,human_id));")?;
        conn.execute_batch("CREATE INDEX IF NOT EXISTS artifact_audience_entry ON interaction_artifact_audiences(session_id,human_id,artifact);
            CREATE INDEX IF NOT EXISTS artifact_blob_refs ON interaction_artifacts(sha256,phase,expires_at);
            CREATE INDEX IF NOT EXISTS artifact_expiry ON interaction_artifacts(phase,expires_at);")?;
        conn.execute_batch("CREATE TABLE IF NOT EXISTS interaction_artifact_jobs(job TEXT PRIMARY KEY,local INTEGER NOT NULL REFERENCES turns(id),human INTEGER NOT NULL,doc TEXT NOT NULL,phase TEXT NOT NULL,instance_id TEXT NOT NULL);")?;
        let has_job: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM pragma_table_info('interaction_artifact_jobs') WHERE name='job')", [], |r|r.get(0))?;
        if !has_job {
            let tx = conn.transaction()?;
            tx.execute_batch("ALTER TABLE interaction_artifact_jobs RENAME TO interaction_artifact_jobs_legacy;
                CREATE TABLE interaction_artifact_jobs(job TEXT PRIMARY KEY,local INTEGER NOT NULL REFERENCES turns(id),human INTEGER NOT NULL,doc TEXT NOT NULL,phase TEXT NOT NULL,instance_id TEXT NOT NULL);
                INSERT INTO interaction_artifact_jobs SELECT 'legacy:'||local,local,human,doc,phase,instance_id FROM interaction_artifact_jobs_legacy;
                DROP TABLE interaction_artifact_jobs_legacy;")?;
            tx.commit()?;
        }
        conn.execute("UPDATE interaction_artifact_jobs SET phase='pending' WHERE phase='running' AND instance_id!=?", [&self.instance_id])?;
        Ok(())
    }

    pub fn artifact_job_stage(&self, local: i64, human: i64, doc: &Value) -> Result<()> {
        use sha2::{Digest, Sha256};
        let output = &doc["output"];
        let job = format!(
            "{local}:{:x}",
            Sha256::digest(serde_json::to_vec(&json!([
                output["native"],
                output["turn"],
                output["item"]
            ]))?)
        );
        let conn = self.conn.lock().expect("store mutex poisoned");
        let mut doc = doc.clone();
        if let Some(files) = doc
            .get_mut("output")
            .and_then(|v| v.get_mut("files"))
            .and_then(Value::as_object_mut)
        {
            let mut previous=conn.prepare("SELECT EXISTS(SELECT 1 FROM interaction_artifact_jobs j,json_each(j.doc,'$.output.files') f WHERE j.local=?1 AND j.human=?2 AND j.job!=?3 AND f.key=?4 AND json(f.value)=json(?5))")?;
            let mut duplicate = Vec::new();
            for (name, metadata) in files.iter() {
                if previous.query_row(
                    params![local, human, job, name, metadata.to_string()],
                    |r| r.get::<_, bool>(0),
                )? {
                    duplicate.push(name.clone());
                }
            }
            for name in duplicate {
                files.remove(&name);
            }
        }
        conn.execute(
            "INSERT OR IGNORE INTO interaction_artifact_jobs VALUES(?1,?2,?3,?4,'pending','')",
            params![job, local, human, doc.to_string()],
        )?;
        Ok(())
    }

    pub fn artifact_job_projects(&self, local: i64) -> Result<bool> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        Ok(conn.query_row("SELECT EXISTS(SELECT 1 FROM interaction_artifact_jobs WHERE local=?1 AND json_extract(doc,'$.publish_files')=1)",[local],|r|r.get(0))?)
    }

    /// Byte copying is retryable; native inputs and publication effects are not.
    pub fn artifact_job_claim(&self, human: i64) -> Result<Option<(String, i64, Value)>> {
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let running: i64 = tx.query_row(
            "SELECT COUNT(*) FROM interaction_artifact_jobs WHERE phase='running'",
            [],
            |r| r.get(0),
        )?;
        if running >= 4 {
            return Ok(None);
        }
        let selected:Option<(String,i64,String)>=tx.query_row("SELECT j.job,j.local,j.doc FROM interaction_artifact_jobs j JOIN turns t ON t.id=j.local JOIN sessions s ON s.id=t.session_id JOIN users u ON u.tg_user_id=s.creator_user_id WHERE j.phase='pending' AND j.human=?1 AND s.creator_user_id=?1 AND t.from_user_id=?1 AND u.allowed=1 ORDER BY j.local,j.rowid LIMIT 1",[human],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
        let Some((job, local, doc)) = selected else {
            return Ok(None);
        };
        let doc = serde_json::from_str(&doc)?;
        tx.execute(
            "UPDATE interaction_artifact_jobs SET phase='running',instance_id=?2 WHERE job=?1",
            params![job, &self.instance_id],
        )?;
        tx.commit()?;
        Ok(Some((job, local, doc)))
    }

    pub fn artifact_job_finish(&self, job: &str, phase: &str) -> Result<()> {
        if !matches!(phase, "done" | "partial" | "pending") {
            return Err(anyhow!("Invalid file job phase"));
        }
        self.conn.lock().expect("store mutex poisoned").execute("UPDATE interaction_artifact_jobs SET phase=?2 WHERE job=?1 AND phase='running' AND instance_id=?3",params![job,phase,&self.instance_id])?;
        Ok(())
    }

    /// The observation must name a retained actual native item, not a path or a
    /// model's upload claim. Coverage can select only already accepted origins.
    #[allow(clippy::too_many_arguments)]
    pub fn artifact_register(
        &self,
        local: i64,
        human: i64,
        native: &str,
        turn: &str,
        item: &str,
        name: &str,
        sha: &str,
        bytes: i64,
        expires: &str,
    ) -> Result<String> {
        if sha.len() != 64
            || !sha
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
            || bytes < 0
        {
            return Err(anyhow!("Invalid retained bytes metadata"));
        }
        self.artifact_observe(
            local,
            human,
            native,
            turn,
            item,
            name,
            Some(sha),
            Some(bytes),
            expires,
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub fn artifact_unavailable(
        &self,
        local: i64,
        human: i64,
        native: &str,
        turn: &str,
        item: &str,
        name: &str,
        expires: &str,
    ) -> Result<String> {
        self.artifact_observe(local, human, native, turn, item, name, None, None, expires)
    }
    #[allow(clippy::too_many_arguments)]
    fn artifact_observe(
        &self,
        local: i64,
        human: i64,
        native: &str,
        turn: &str,
        item: &str,
        name: &str,
        sha: Option<&str>,
        bytes: Option<i64>,
        expires: &str,
    ) -> Result<String> {
        if name.is_empty()
            || name.chars().count() > 128
            || name
                .chars()
                .any(|c| c.is_control() || matches!(c, '/' | '\\'))
            || matches!(name, "." | "..")
            || chrono::DateTime::parse_from_rfc3339(expires)
                .ok()
                .is_none_or(|at| at <= Utc::now())
        {
            return Err(anyhow!("Invalid artifact observation metadata"));
        }
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let(source,body):(i64,String)=tx.query_row("SELECT t.session_id,m.text FROM interaction_native_messages m JOIN turns t ON t.id=m.local_turn JOIN sessions s ON s.id=t.session_id JOIN users u ON u.tg_user_id=s.creator_user_id WHERE m.local_turn=?1 AND m.native_thread=?2 AND m.native_turn=?3 AND m.item_id=?4 AND t.native_thread_id=?2 AND t.from_user_id=?5 AND s.creator_user_id=?5 AND u.allowed=1",params![local,native,turn,item,human],|r|Ok((r.get(0)?,r.get(1)?))).context("Artifact output provenance unavailable")?;
        let old:Option<(String,Option<String>,Option<i64>,String)>=tx.query_row("SELECT id,sha256,bytes,expires_at FROM interaction_artifacts WHERE native_thread=?1 AND native_turn=?2 AND native_item=?3 AND name=?4",params![native,turn,item,name],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).optional()?;
        if let Some((old, digest, size, expiry)) = &old {
            if sha.is_none() {
                return Ok(old.clone());
            }
            if digest.is_some() {
                if digest.as_deref() != sha || *size != bytes {
                    return Err(anyhow!("Artifact identity payload conflict"));
                }
                return Ok(old.clone());
            }
            if chrono::DateTime::parse_from_rfc3339(expiry)?.with_timezone(&Utc) <= Utc::now() {
                return Err(anyhow!(
                    "Expired unavailable observation needs a new output"
                ));
            }
        }
        if let (Some(sha), Some(bytes)) = (sha, bytes) {
            let existing: Option<i64> = tx
                .query_row(
                    "SELECT bytes FROM interaction_artifact_blobs WHERE sha256=?",
                    [sha],
                    |r| r.get(0),
                )
                .optional()?;
            if existing.is_some_and(|size| size != bytes) {
                return Err(anyhow!("Artifact digest reused with different size"));
            }
            tx.execute("INSERT INTO interaction_artifact_blobs VALUES(?1,?2,1) ON CONFLICT(sha256) DO UPDATE SET present=1",params![sha,bytes])?;
        }
        let id = old
            .map(|value| value.0)
            .unwrap_or_else(|| Uuid::now_v7().to_string());
        tx.execute("INSERT INTO interaction_artifacts VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12) ON CONFLICT(id) DO UPDATE SET sha256=excluded.sha256,bytes=excluded.bytes,phase=excluded.phase",params![id,local,source,human,native,turn,item,name,sha,bytes,expires,if sha.is_some(){"ready"}else{"unavailable"}])?;
        tx.execute(
            "INSERT OR IGNORE INTO interaction_artifact_audiences VALUES(?1,?2,?3)",
            params![id, source, human],
        )?;
        let coverage = crate::interaction::worker_result(&body).1;
        if let Some(coverage) = coverage {
            let mut query=tx.prepare("SELECT i.id,i.doc FROM interaction_instructions i JOIN interaction_sources x ON x.id=i.input_id WHERE x.human_id=?1 AND i.phase='accepted' AND json_extract(i.doc,'$.local_turn')=?2 AND json_extract(i.doc,'$.native_thread')=?3 AND json_extract(i.doc,'$.native_turn')=?4")?;
            let inputs = query
                .query_map(params![human, local, native, turn], |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            drop(query);
            for (input, doc) in inputs {
                if !coverage.covered.contains(&input) {
                    continue;
                }
                let input: crate::interaction::Instruction = serde_json::from_str(&doc)?;
                tx.execute("INSERT OR IGNORE INTO interaction_artifact_audiences SELECT ?1,s.id,?2 FROM sessions s JOIN users u ON u.tg_user_id=s.creator_user_id WHERE s.chat_id=?3 AND s.thread_id=?4 AND s.creator_user_id=?2 AND u.allowed=1",params![id,human,input.origin.chat_id,input.origin.thread_id])?;
            }
        }
        if sha.is_some() {
            let publish:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM interaction_artifact_jobs WHERE local=?1 AND json_extract(doc,'$.publish_files')=1)",[local],|r|r.get(0))?;
            if publish {
                super::topic_publications::enqueue_on(&tx, local, "artifact", &id, name)?;
            }
        }
        tx.commit()?;
        Ok(id)
    }

    /// Logical owner/audience stays authoritative after native renewal. The old
    /// physical source remains provenance; it is not a new current-binding gate.
    pub fn artifact_get(&self, human: i64, entry: SessionKey, id: &str) -> Result<ArtifactRecord> {
        if Uuid::parse_str(id).is_err() {
            return Err(anyhow!("Invalid artifact reference"));
        }
        let conn = self.conn.lock().expect("store mutex poisoned");
        goal_return_audiences_on(&conn, human, Some(id))?;
        let row:Option<String>=conn.query_row("SELECT json_object('id',a.id,'name',a.name,'sha256',a.sha256,'bytes',a.bytes,'expires_at',a.expires_at,'source_session',a.source_session,'source_local',a.source_local,'native_thread',a.native_thread,'native_turn',a.native_turn,'native_item',a.native_item) FROM interaction_artifacts a JOIN interaction_artifact_audiences v ON v.artifact=a.id JOIN sessions e ON e.id=v.session_id JOIN sessions s ON s.id=a.source_session JOIN users u ON u.tg_user_id=a.human_id WHERE a.id=?1 AND a.human_id=?2 AND v.human_id=?2 AND e.creator_user_id=?2 AND s.creator_user_id=?2 AND u.allowed=1 AND e.chat_id=?3 AND e.thread_id=?4 AND a.phase='ready' AND julianday(a.expires_at)>julianday('now')",params![id,human,entry.chat_id,entry.thread_id],|r|r.get(0)).optional()?;
        serde_json::from_str(
            &row.context("Artifact unavailable, expired or outside this conversation")?,
        )
        .map_err(Into::into)
    }

    pub fn artifact_list(&self, human: i64, entry: SessionKey) -> Result<Value> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        goal_return_audiences_on(&conn, human, None)?;
        let session: i64 = conn.query_row(
            "SELECT id FROM sessions WHERE chat_id=?1 AND thread_id=?2 AND creator_user_id=?3",
            params![entry.chat_id, entry.thread_id, human],
            |r| r.get(0),
        )?;
        let mut q=conn.prepare("SELECT j.phase,m.rowid,r.id FROM interaction_artifact_jobs j JOIN turns t ON t.id=j.local JOIN sessions s ON s.id=t.session_id JOIN users u ON u.tg_user_id=s.creator_user_id JOIN interaction_native_messages m ON m.local_turn=j.local AND m.native_thread=json_extract(j.doc,'$.output.native') AND m.native_turn=json_extract(j.doc,'$.output.turn') AND m.item_id=json_extract(j.doc,'$.output.item') LEFT JOIN interaction_results r ON r.native_thread=m.native_thread AND r.native_turn=m.native_turn AND r.item_id=m.item_id WHERE j.human=?1 AND t.from_user_id=?1 AND s.creator_user_id=?1 AND u.allowed=1 AND (t.session_id=?2 OR EXISTS(SELECT 1 FROM interaction_deliveries d WHERE d.result_id=r.id AND d.human_id=?1 AND json_extract(d.doc,'$.origin.chat_id')=?3 AND json_extract(d.doc,'$.origin.thread_id')=?4) OR ((m.phase='final_answer' OR m.phase IS NULL) AND r.id IS NULL AND EXISTS(SELECT 1 FROM conversation_inputs c JOIN interaction_goal_controls g ON g.human_id=c.human_id AND g.entry_chat=?3 AND g.entry_topic=?4 AND g.phase='acknowledged' AND json_extract(g.doc,'$.worker_execution.local')=m.local_turn AND json_extract(g.doc,'$.worker_execution.native')=m.native_thread WHERE c.id=m.presentation_input AND c.human_id=?1 AND c.session_id=?2))) ORDER BY j.local DESC LIMIT 128")?;
        let jobs=q.query_map(params![human,session,entry.chat_id,entry.thread_id],|r|Ok(json!({"phase":r.get::<_,String>(0)?,"native_source":format!("native:{}",r.get::<_,i64>(1)?),"result_id":r.get::<_,Option<String>>(2)?})))?.collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(
            json!({"schema":"telecodex.artifacts/v1","items":views_on(&conn,human,session,None,None,true)?,"jobs":jobs}),
        )
    }

    pub fn artifact_storage_bytes(&self) -> Result<i64> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        Ok(conn.query_row(
            "SELECT COALESCE(SUM(bytes),0) FROM interaction_artifact_blobs WHERE present=1",
            [],
            |r| r.get(0),
        )?)
    }

    /// No filesystem authority is returned beyond content-addressed owned names.
    pub fn artifact_expired_blobs(&self) -> Result<Vec<String>> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        conn.execute("UPDATE interaction_artifacts SET phase='expired' WHERE phase='ready' AND julianday(expires_at)<=julianday('now')",[])?;
        let mut q=conn.prepare("SELECT b.sha256 FROM interaction_artifact_blobs b WHERE b.present=1 AND NOT EXISTS(SELECT 1 FROM interaction_artifacts a WHERE a.sha256=b.sha256 AND a.phase='ready')")?;
        Ok(q.query_map([], |r| r.get(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?)
    }
    pub fn artifact_blob_removed(&self, sha: &str) -> Result<()> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        conn.execute("UPDATE interaction_artifact_blobs SET present=0 WHERE sha256=?1 AND NOT EXISTS(SELECT 1 FROM interaction_artifacts WHERE sha256=?1 AND phase='ready')",[sha])?;
        Ok(())
    }
    pub fn artifact_already_retained(
        &self,
        local: i64,
        human: i64,
        name: &str,
        sha: &str,
    ) -> Result<bool> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        Ok(conn.query_row("SELECT EXISTS(SELECT 1 FROM interaction_artifacts a JOIN interaction_artifact_blobs b ON b.sha256=a.sha256 WHERE a.source_local=?1 AND a.human_id=?2 AND a.name=?3 AND a.sha256=?4 AND a.phase='ready' AND b.present=1 AND julianday(a.expires_at)>julianday('now'))",params![local,human,name,sha],|r|r.get(0))?)
    }

    pub fn artifact_output(&self, local: i64, human: i64) -> Result<Option<Value>> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        Ok(conn.query_row("SELECT m.native_thread,m.native_turn,m.item_id FROM interaction_native_messages m JOIN turns t ON t.id=m.local_turn JOIN sessions s ON s.id=t.session_id JOIN users u ON u.tg_user_id=s.creator_user_id WHERE m.local_turn=?1 AND t.from_user_id=?2 AND s.creator_user_id=?2 AND u.allowed=1 AND (m.phase='final_answer' OR m.phase IS NULL) ORDER BY m.rowid DESC LIMIT 1",params![local,human],|r|Ok(json!({"native":r.get::<_,String>(0)?,"turn":r.get::<_,String>(1)?,"item":r.get::<_,String>(2)?}))).optional()?)
    }
    pub fn artifact_entry_binding(&self, human: i64, entry: SessionKey) -> Result<Value> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let(session,native):(i64,String)=conn.query_row("SELECT s.id,COALESCE(s.codex_thread_id,'') FROM sessions s JOIN users u ON u.tg_user_id=s.creator_user_id WHERE s.chat_id=?1 AND s.thread_id=?2 AND s.creator_user_id=?3 AND u.allowed=1",params![entry.chat_id,entry.thread_id,human],|r|Ok((r.get(0)?,r.get(1)?))).context("Artifact conversation owner unavailable")?;
        Ok(json!([human, entry, session, native]))
    }
}

pub(super) fn views_on(
    conn: &Connection,
    human: i64,
    entry: i64,
    result: Option<&str>,
    native: Option<i64>,
    all: bool,
) -> Result<Vec<Value>> {
    goal_return_audiences_on(conn, human, None)?;
    let mut q=conn.prepare("SELECT a.id,a.name,a.sha256,a.bytes,a.expires_at,a.source_session,a.source_local,(a.phase='ready' AND julianday(a.expires_at)>julianday('now') AND b.present=1),r.id,m.rowid FROM interaction_artifacts a JOIN interaction_artifact_audiences v ON v.artifact=a.id JOIN sessions e ON e.id=v.session_id JOIN sessions s ON s.id=a.source_session JOIN users u ON u.tg_user_id=a.human_id LEFT JOIN interaction_artifact_blobs b ON b.sha256=a.sha256 JOIN interaction_native_messages m ON m.native_thread=a.native_thread AND m.native_turn=a.native_turn AND m.item_id=a.native_item LEFT JOIN interaction_results r ON r.native_thread=a.native_thread AND r.native_turn=a.native_turn AND r.item_id=a.native_item WHERE a.human_id=?1 AND v.human_id=?1 AND e.creator_user_id=?1 AND s.creator_user_id=?1 AND u.allowed=1 AND e.id=?2 AND (?3 OR r.id=?4 OR m.rowid=?5) ORDER BY a.rowid DESC LIMIT ?6")?;
    Ok(q.query_map(params![human,entry,all,result,native,if all{128}else{20}],|r|Ok(json!({"id":r.get::<_,String>(0)?,"name":r.get::<_,String>(1)?,"sha256":r.get::<_,Option<String>>(2)?,"bytes":r.get::<_,Option<i64>>(3)?,"expires_at":r.get::<_,String>(4)?,"source_session":r.get::<_,i64>(5)?,"source_local":r.get::<_,i64>(6)?,"available":r.get::<_,bool>(7)?,"result_id":r.get::<_,Option<String>>(8)?,"native_source":r.get::<_,Option<i64>>(9)?.map(|id|format!("native:{id}"))})))?.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Goal collectors return canonical native items, without routed coverage
/// envelopes. Reuse their acknowledged execution and frozen presentation input,
/// never a later direct return selection. Read-time materialization also fixes
/// already copied files and acknowledgment/copy races without replaying work.
fn goal_return_audiences_on(conn: &Connection, human: i64, id: Option<&str>) -> Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO interaction_artifact_audiences(artifact,session_id,human_id)
         SELECT a.id,c.session_id,a.human_id FROM interaction_artifacts a
         JOIN interaction_native_messages m ON m.local_turn=a.source_local
           AND m.native_thread=a.native_thread AND m.native_turn=a.native_turn AND m.item_id=a.native_item
         JOIN turns t ON t.id=m.local_turn AND t.native_thread_id=m.native_thread AND t.session_id=a.source_session
         JOIN conversation_inputs c ON c.id=m.presentation_input AND c.human_id=a.human_id
         JOIN sessions e ON e.id=c.session_id AND e.creator_user_id=a.human_id
         JOIN sessions s ON s.id=a.source_session AND s.creator_user_id=a.human_id
         JOIN users u ON u.tg_user_id=a.human_id AND u.allowed=1
         WHERE a.human_id=?1 AND t.from_user_id=?1 AND (m.phase='final_answer' OR m.phase IS NULL)
           AND (?2 IS NULL OR a.id=?2)
           AND EXISTS(SELECT 1 FROM interaction_goal_controls g WHERE g.human_id=?1
             AND g.entry_chat=e.chat_id AND g.entry_topic=e.thread_id AND g.phase='acknowledged'
             AND json_extract(g.doc,'$.worker_execution.local')=m.local_turn
             AND json_extract(g.doc,'$.worker_execution.native')=m.native_thread)
           AND NOT EXISTS(SELECT 1 FROM interaction_results r WHERE r.native_thread=m.native_thread
             AND r.native_turn=m.native_turn AND r.item_id=m.item_id)",
        params![human, id],
    )?;
    Ok(())
}

pub(super) fn status_on(
    conn: &Connection,
    result: Option<&str>,
    native: Option<i64>,
) -> Result<Option<String>> {
    Ok(conn.query_row("SELECT j.phase FROM interaction_artifact_jobs j JOIN interaction_native_messages m ON m.local_turn=j.local AND m.native_thread=json_extract(j.doc,'$.output.native') AND m.native_turn=json_extract(j.doc,'$.output.turn') AND m.item_id=json_extract(j.doc,'$.output.item') LEFT JOIN interaction_results r ON r.native_thread=m.native_thread AND r.native_turn=m.native_turn AND r.item_id=m.item_id WHERE r.id=?1 OR m.rowid=?2 LIMIT 1",params![result,native],|r|r.get(0)).optional()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interaction::{RequestRelation, ReturnRoute, SourceInput, TransportPhase as P};
    fn setup() -> (tempfile::NamedTempFile, Store, i64, SessionKey, SessionKey) {
        setup_with_coverage(true)
    }
    fn setup_with_coverage(
        covered: bool,
    ) -> (tempfile::NamedTempFile, Store, i64, SessionKey, SessionKey) {
        let file = tempfile::NamedTempFile::new().unwrap();
        let defaults = super::super::tests::defaults();
        let store = Store::open(file.path(), &[100, 101], &defaults).unwrap();
        let entry = SessionKey::new(100, Some(1));
        let worker = SessionKey::new(100, Some(7));
        for key in [entry, worker, SessionKey::new(100, Some(8))] {
            store.ensure_session(key, 100, &defaults).unwrap();
            store
                .set_session_codex_thread(
                    key,
                    if key == worker {
                        "old-native"
                    } else {
                        "other-native"
                    },
                )
                .unwrap();
        }
        let target = store.get_session(worker).unwrap().unwrap();
        let local = store
            .record_turn_started(
                target.id,
                &TurnRequest {
                    session_key: worker,
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
            .record_turn_native_thread(local, "old-native")
            .unwrap();
        let (input, _) = store
            .interaction_admit(
                &SourceInput {
                    transcription: None,
                    channel: "telegram:fixture".into(),
                    event_id: "10".into(),
                    update_id: Some(10),
                    human_id: 100,
                    target_sessions: vec![target.id],
                    origin: ReturnRoute {
                        chat_id: 100,
                        thread_id: 1,
                        message_id: Some(10),
                    },
                    text: "Make a file".into(),
                },
                worker,
                RequestRelation::New,
            )
            .unwrap();
        store
            .interaction_transport(&input.id, 100, local, P::Submitting, None)
            .unwrap();
        store
            .interaction_transport(&input.id, 100, local, P::Accepted, Some("physical"))
            .unwrap();
        let body = if covered {
            format!(
                "Output\n<daimon-result>{}</daimon-result>",
                json!({"requests":input.requests,"covered":[input.id]})
            )
        } else {
            "Goal output without a routed envelope".into()
        };
        store
            .record_native_message(
                local,
                "old-native",
                "physical",
                "final",
                Some("final_answer"),
                &body,
            )
            .unwrap();
        (file, store, local, entry, worker)
    }
    fn register(store: &Store, local: i64, name: &str) -> String {
        store
            .artifact_register(
                local,
                100,
                "old-native",
                "physical",
                "final",
                name,
                &"ab".repeat(32),
                13,
                &(Utc::now() + Duration::hours(24)).to_rfc3339(),
            )
            .unwrap()
    }
    #[test]
    fn goal_files_follow_acknowledged_execution_and_frozen_origin_without_replay() {
        let (file, s, local, entry, worker) = setup_with_coverage(false);
        let original = Uuid::now_v7().to_string();
        let later = Uuid::now_v7().to_string();
        let other = SessionKey::new(100, Some(8));
        let entry_id = s.get_session(entry).unwrap().unwrap().id;
        let other_id = s.get_session(other).unwrap().unwrap().id;
        {
            let conn = s.conn.lock().unwrap();
            for (id, session) in [(&original, entry_id), (&later, other_id)] {
                conn.execute("INSERT INTO conversation_inputs(id,human_id,session_id,native_thread,phase,doc,instance_id,created_at) VALUES(?1,100,?2,'other-native','accepted','{}','fixture',?3)",params![id,session,now_string()]).unwrap();
            }
            conn.execute(
                "UPDATE interaction_native_messages SET presentation_input=?1 WHERE local_turn=?2",
                params![original, local],
            )
            .unwrap();
            // A later surface selection must not grant this completed file.
            conn.execute(
                "INSERT INTO conversation_returns VALUES(100,'old-native',?1,?2)",
                params![later, now_string()],
            )
            .unwrap();
            conn.execute("INSERT INTO interaction_goal_controls VALUES('goal',100,100,1,100,7,'submitted',?1,NULL,'fixture')",[json!({"worker_execution":{"local":local,"native":"wrong-native"}}).to_string()]).unwrap();
        }
        s.artifact_job_stage(
            local,
            100,
            &json!({"output":{"native":"old-native","turn":"physical","item":"final"}}),
        )
        .unwrap();
        let id = register(&s, local, "goal.json");
        assert!(s.artifact_get(100, entry, &id).is_err());
        assert!(
            s.artifact_list(100, entry).unwrap()["jobs"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert!(s.artifact_get(100, worker, &id).is_ok());
        s.conn
            .lock()
            .unwrap()
            .execute(
                "UPDATE interaction_goal_controls SET phase='acknowledged'",
                [],
            )
            .unwrap();
        assert!(
            s.artifact_get(100, entry, &id).is_err(),
            "Wrong execution cannot grant file access"
        );
        s.conn
            .lock()
            .unwrap()
            .execute(
                "UPDATE interaction_goal_controls SET doc=?1",
                [json!({"worker_execution":{"local":local,"native":"old-native"}}).to_string()],
            )
            .unwrap();
        assert_eq!(s.artifact_get(100, entry, &id).unwrap().id, id);
        assert_eq!(
            s.artifact_list(100, entry).unwrap()["jobs"][0]["phase"],
            "pending"
        );
        assert!(s.artifact_get(100, other, &id).is_err());
        assert!(s.artifact_get(101, entry, &id).is_err());
        assert_eq!(
            s.artifact_list(100, entry).unwrap()["items"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        // Simulate a file registered by the previous version, then recover it
        // through its retained proof after restart rather than running the goal.
        s.conn
            .lock()
            .unwrap()
            .execute(
                "DELETE FROM interaction_artifact_audiences WHERE artifact=?1 AND session_id=?2",
                params![id, entry_id],
            )
            .unwrap();
        drop(s);
        let reopened =
            Store::open(file.path(), &[100, 101], &super::super::tests::defaults()).unwrap();
        assert_eq!(
            reopened.artifact_get(100, entry, &id).unwrap().sha256,
            "ab".repeat(32)
        );
        assert_eq!(
            reopened.artifact_list(100, entry).unwrap()["items"][0]["id"],
            id
        );
        let conn = reopened.conn.lock().unwrap();
        assert_eq!(
            conn.query_row("SELECT count(*) FROM turns", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            conn.query_row(
                "SELECT count(*) FROM interaction_artifact_audiences WHERE artifact=?1",
                [id],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            2
        );
    }
    #[test]
    fn file_publications_keep_logical_ownership_and_never_retry_unknown_receipts() {
        let (_file, s, local, _, worker) = setup();
        s.topic_projection_register(local, 100, worker).unwrap();
        s.artifact_job_stage(local, 100, &json!({"publish_files":true}))
            .unwrap();
        let id = register(&s, local, "output.txt");
        let publication = s
            .topic_publication_claim_files(worker, 100)
            .unwrap()
            .unwrap();
        assert_eq!(publication.kind, "artifact");
        assert_eq!(publication.source, id);
        s.record_native_message(
            local,
            "old-native",
            "physical",
            "progress",
            Some("commentary"),
            "Independent text",
        )
        .unwrap();
        let text = s.topic_publication_claim(worker, 100).unwrap().unwrap();
        assert_eq!(text.kind, "native");
        s.topic_publication_parts(text.id, &["Independent text".into()])
            .unwrap();
        assert!(s.topic_publication_reserve(text.id, 0).unwrap());
        s.topic_publication_receipt(text.id, 0, Some(800), false)
            .unwrap();
        s.topic_publication_finish(text.id).unwrap();
        s.set_session_codex_thread(worker, "renewed").unwrap();
        assert_eq!(
            s.topic_file_caption(publication.id).unwrap(),
            "File · output.txt\nOutput"
        );
        s.topic_publication_parts(publication.id, &["output.txt".into()])
            .unwrap();
        assert!(s.topic_publication_reserve(publication.id, 0).unwrap());
        s.topic_publication_receipt(publication.id, 0, None, false)
            .unwrap();
        s.topic_publication_finish(publication.id).unwrap();
        assert_eq!(id, register(&s, local, "output.txt"));
        assert!(
            s.topic_publication_claim_files(worker, 100)
                .unwrap()
                .is_none()
        );
        register(&s, local, "denied.txt");
        s.conn
            .lock()
            .unwrap()
            .execute("UPDATE users SET allowed=0 WHERE tg_user_id=100", [])
            .unwrap();
        assert!(
            s.topic_publication_claim_files(worker, 100)
                .unwrap()
                .is_none()
        );
        let conn = s.conn.lock().unwrap();
        assert_eq!(
            conn.query_row(
                "SELECT COUNT(*) FROM topic_publications WHERE kind='artifact'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            2
        );
        assert_eq!(
            conn.query_row(
                "SELECT phase FROM topic_publications WHERE id=?1",
                [publication.id],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
            "uncertain"
        );
    }
    #[test]
    fn artifact_checkpoints_are_distinct_and_legacy_jobs_survive_migration() {
        let (file, s, local, _, _) = setup();
        let first = json!({"out":"owned","output":{"native":"old-native","turn":"physical","item":"first"}});
        {
            let conn = s.conn.lock().unwrap();
            conn.execute_batch("DROP TABLE interaction_artifact_jobs; CREATE TABLE interaction_artifact_jobs(local INTEGER PRIMARY KEY REFERENCES turns(id),human INTEGER NOT NULL,doc TEXT NOT NULL,phase TEXT NOT NULL,instance_id TEXT NOT NULL);").unwrap();
            conn.execute(
                "INSERT INTO interaction_artifact_jobs VALUES(?1,100,?2,'pending','')",
                params![local, first.to_string()],
            )
            .unwrap();
        }
        s.release_instance_lock().unwrap();
        drop(s);
        let s = Store::open(file.path(), &[100, 101], &super::super::tests::defaults()).unwrap();
        let (job, actual, doc) = s.artifact_job_claim(100).unwrap().unwrap();
        assert_eq!((actual, doc), (local, first));
        s.artifact_job_finish(&job, "done").unwrap();
        let second = json!({"out":"owned","output":{"native":"old-native","turn":"physical","item":"second"}});
        s.artifact_job_stage(local, 100, &second).unwrap();
        s.artifact_job_stage(local, 100, &second).unwrap();
        let (next, actual, doc) = s.artifact_job_claim(100).unwrap().unwrap();
        assert_ne!(next, job);
        assert_eq!((actual, doc), (local, second));
        assert!(s.artifact_job_claim(100).unwrap().is_none());
    }

    #[test]
    fn concurrent_checkpoint_jobs_keep_the_first_file_owner_before_copying() {
        let (_file, s, local, _, _) = setup();
        let first = json!({"output":{"native":"old-native","turn":"physical","item":"one","files":{"first.txt":[1,2,3]}}});
        let second = json!({"output":{"native":"old-native","turn":"physical","item":"two","files":{"first.txt":[1,2,3],"second.txt":[4,5,6]}}});
        s.artifact_job_stage(local, 100, &first).unwrap();
        let (_, _, claimed) = s.artifact_job_claim(100).unwrap().unwrap();
        assert_eq!(claimed, first);
        // The first copy is still running when the next canonical final arrives.
        s.artifact_job_stage(local, 100, &second).unwrap();
        let (_, _, claimed) = s.artifact_job_claim(100).unwrap().unwrap();
        assert!(claimed["output"]["files"].get("first.txt").is_none());
        assert_eq!(claimed["output"]["files"]["second.txt"], json!([4, 5, 6]));
    }

    #[test]
    fn artifact_jobs_resume_only_file_work_after_restart() {
        let (file, s, local, _, _) = setup();
        let doc = json!({"out":"owned-output","device":1,"inode":2,"output":{"native":"old-native","turn":"physical","item":"final"}});
        s.artifact_job_stage(local, 100, &doc).unwrap();
        assert!(s.artifact_job_claim(101).unwrap().is_none());
        let (job, claimed_local, claimed_doc) = s.artifact_job_claim(100).unwrap().unwrap();
        assert_eq!((claimed_local, claimed_doc), (local, doc.clone()));
        assert!(s.artifact_job_claim(100).unwrap().is_none());
        s.release_instance_lock().unwrap();
        drop(s);
        let s = Store::open(file.path(), &[100, 101], &super::super::tests::defaults()).unwrap();
        assert_eq!(
            s.artifact_job_claim(100).unwrap().unwrap(),
            (job.clone(), local, doc)
        );
        s.artifact_job_finish(&job, "done").unwrap();
        assert!(s.artifact_job_claim(100).unwrap().is_none());
        assert_eq!(
            s.conn
                .lock()
                .unwrap()
                .query_row("SELECT COUNT(*) FROM turns", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            1
        );
    }
    #[test]
    fn artifacts_preserve_actual_provenance_and_logical_audience_after_native_renewal() {
        let (_file, s, local, entry, worker) = setup();
        let id = register(&s, local, "demo.txt");
        assert_eq!(
            s.artifact_get(100, entry, &id).unwrap().native_thread,
            "old-native"
        );
        assert!(s.artifact_get(100, worker, &id).is_ok());
        assert!(
            s.artifact_get(100, SessionKey::new(100, Some(8)), &id)
                .is_err()
        );
        assert!(s.artifact_get(101, entry, &id).is_err());
        s.set_session_codex_thread(worker, "renewed-worker")
            .unwrap();
        s.set_session_codex_thread(entry, "renewed-entry").unwrap();
        let retained = s.artifact_get(100, entry, &id).unwrap();
        assert_eq!(retained.native_thread, "old-native");
        assert_eq!(retained.native_turn, "physical");
        assert_eq!(retained.native_item, "final");
        assert_eq!(retained.source_local, local);
        assert_eq!(
            s.artifact_list(100, entry).unwrap()["items"][0]["available"],
            true
        );
    }
    #[test]
    fn artifacts_deduplicate_identity_and_bytes_and_reject_unobserved_source() {
        let (_file, s, local, entry, _) = setup();
        let one = register(&s, local, "one.txt");
        assert_eq!(one, register(&s, local, "one.txt"));
        let two = register(&s, local, "two.txt");
        assert_ne!(one, two);
        assert_eq!(s.artifact_storage_bytes().unwrap(), 13);
        for (name, item, size) in [
            ("../private", "final", 13),
            ("valid", "invented", 13),
            ("one.txt", "final", 14),
        ] {
            assert!(
                s.artifact_register(
                    local,
                    100,
                    "old-native",
                    "physical",
                    item,
                    name,
                    &"ab".repeat(32),
                    size,
                    &(Utc::now() + Duration::hours(24)).to_rfc3339()
                )
                .is_err()
            );
        }
        assert_eq!(
            s.artifact_list(100, entry).unwrap()["items"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
    }
    #[test]
    fn artifacts_expire_without_deleting_a_shared_live_blob_and_revoke_with_owner() {
        let (_file, s, local, entry, _) = setup();
        let one = register(&s, local, "one.txt");
        let two = register(&s, local, "two.txt");
        s.conn
            .lock()
            .unwrap()
            .execute(
                "UPDATE interaction_artifacts SET expires_at='2000-01-01T00:00:00Z' WHERE id=?",
                [&one],
            )
            .unwrap();
        assert!(s.artifact_get(100, entry, &one).is_err());
        assert!(s.artifact_get(100, entry, &two).is_ok());
        assert!(s.artifact_expired_blobs().unwrap().is_empty());
        s.conn
            .lock()
            .unwrap()
            .execute(
                "UPDATE interaction_artifacts SET expires_at='2000-01-01T00:00:00Z'",
                [],
            )
            .unwrap();
        assert_eq!(s.artifact_expired_blobs().unwrap(), vec!["ab".repeat(32)]);
        s.artifact_blob_removed(&"ab".repeat(32)).unwrap();
        assert_eq!(s.artifact_storage_bytes().unwrap(), 0);
        assert!(
            s.artifact_list(100, entry).unwrap()["items"]
                .as_array()
                .unwrap()
                .iter()
                .all(|item| item["available"] == false)
        );
        s.conn
            .lock()
            .unwrap()
            .execute("UPDATE users SET allowed=0 WHERE tg_user_id=100", [])
            .unwrap();
        assert!(s.artifact_get(100, entry, &two).is_err());
    }
}
