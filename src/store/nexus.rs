//! Durable selected Nexus input and observed native receipts; never replay input.
use super::*;
use serde_json::{Value, json};

impl Store {
    pub(super) fn init_nexus(&self) -> Result<()> {
        self.conn.lock().expect("store mutex poisoned").execute_batch(
            "CREATE TABLE IF NOT EXISTS nexus_input_context(update_id INTEGER PRIMARY KEY REFERENCES incoming_updates(update_id), target TEXT NOT NULL, capture TEXT, captured_at TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS nexus_bindings(table_id TEXT NOT NULL,author_ref TEXT NOT NULL,doc TEXT NOT NULL,PRIMARY KEY(table_id,author_ref));
             CREATE TABLE IF NOT EXISTS nexus_requests(id TEXT PRIMARY KEY,doc TEXT NOT NULL,phase TEXT NOT NULL,dispatch_id TEXT,local_turn INTEGER,native_turn TEXT,instance_id TEXT NOT NULL,reason TEXT,echo_attempted INTEGER NOT NULL DEFAULT 0);
             CREATE TABLE IF NOT EXISTS nexus_executions(local_turn INTEGER NOT NULL,native_turn TEXT NOT NULL,terminal TEXT,reply TEXT NOT NULL DEFAULT '',PRIMARY KEY(local_turn,native_turn));
             CREATE TABLE IF NOT EXISTS nexus_receipts(id INTEGER PRIMARY KEY,request_id TEXT NOT NULL,status TEXT NOT NULL,doc TEXT NOT NULL,sent INTEGER NOT NULL DEFAULT 0,UNIQUE(request_id,status));"
        )?;
        self.nexus_reconcile_observations()?;
        self.conn.lock().expect("store mutex poisoned").execute("UPDATE nexus_requests SET phase=CASE WHEN phase='queued' THEN 'retained_queue' ELSE 'undetermined' END,reason='Listener restarted; inspect retained state before any explicit recovery' WHERE instance_id!=? AND phase IN ('queued','claiming','claimed','submitting','steering')",[&self.instance_id])?;
        Ok(())
    }
    /// Reserve capture before operator I/O. A crash never authorizes later recapture.
    pub fn nexus_begin_input_capture(&self, id: i64, target: &Value) -> Result<bool> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        Ok(conn.execute("INSERT OR IGNORE INTO nexus_input_context(update_id,target,captured_at) SELECT ?,?,? FROM incoming_updates WHERE update_id=? AND status='received' AND instance_id=?",params![id,serde_json::to_string(target)?,now_string(),id,self.instance_id])? == 1)
    }
    pub fn nexus_finish_input_capture(&self, id: i64, capture: &Value) -> Result<()> {
        let raw = serde_json::to_string(capture)?;
        if raw.len() > 512 * 1024 {
            return Err(anyhow!("Nexus input capture exceeds bound"));
        }
        let conn = self.conn.lock().expect("store mutex poisoned");
        if conn.execute(
            "UPDATE nexus_input_context SET capture=? WHERE update_id=? AND capture IS NULL",
            params![raw, id],
        )? != 1
        {
            let previous: String = conn.query_row(
                "SELECT capture FROM nexus_input_context WHERE update_id=?",
                [id],
                |r| r.get(0),
            )?;
            if previous != raw {
                return Err(anyhow!("Nexus input capture is immutable"));
            }
        }
        Ok(())
    }
    pub fn nexus_input_context(&self, id: i64) -> Result<Option<Value>> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let row: Option<(String, Option<String>, String)> = conn
            .query_row(
                "SELECT target,capture,captured_at FROM nexus_input_context WHERE update_id=?",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        row.map(|(target,capture,at)| Ok(json!({"target":serde_json::from_str::<Value>(&target)?,"capture":capture.map(|s|serde_json::from_str::<Value>(&s)).transpose()?.unwrap_or(json!({"status":"capture_interrupted","perspective":null})),"ingress_at":at}))).transpose()
    }
    pub fn nexus_reconcile_observations(&self) -> Result<()> {
        let retained: Vec<(String, i64, String)> = {
            let conn = self.conn.lock().expect("store mutex poisoned");
            let mut stmt=conn.prepare("SELECT id,local_turn,native_turn FROM nexus_requests WHERE phase IN ('accepted','undetermined') AND native_turn IS NOT NULL AND local_turn IS NOT NULL")?;
            stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
                .collect::<std::result::Result<Vec<_>, _>>()?
        };
        for (id, turn, native) in retained {
            self.nexus_receipt(&id, "accepted", Some(&native), None, None)?;
            self.nexus_sync_terminal(&id, turn, &native)?;
        }
        Ok(())
    }
    /// Completed local collection only selects a native read; it cannot prove
    /// native completion, supply a reply or authorize resubmitting the input.
    pub fn nexus_missing_terminals(&self) -> Result<Vec<(i64, String, String)>> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let mut stmt = conn.prepare(
            "SELECT DISTINCT r.local_turn,r.native_turn,json_extract(r.doc,'$.target.native_thread_id')
             FROM nexus_requests r JOIN turns t ON t.id=r.local_turn
             LEFT JOIN nexus_executions e ON e.local_turn=r.local_turn AND e.native_turn=r.native_turn
             WHERE r.phase IN ('accepted','undetermined') AND r.native_turn IS NOT NULL
               AND t.status IN ('completed','failed','interrupted')
               AND t.native_thread_id=json_extract(r.doc,'$.target.native_thread_id')
               AND e.terminal IS NULL ORDER BY r.local_turn LIMIT 20",
        )?;
        Ok(stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<std::result::Result<Vec<_>, _>>()?)
    }
    pub fn nexus_recover_queue(&self, id: &str) -> Result<bool> {
        Ok(self.conn.lock().expect("store mutex poisoned").execute("UPDATE nexus_requests SET phase='queued',instance_id=? WHERE id=? AND phase='retained_queue' AND dispatch_id IS NULL AND native_turn IS NULL",params![self.instance_id,id])?==1)
    }
    /// One recovery only when a fresh owner inspection proves dispatch never
    /// committed. Missing native acknowledgement is explicitly excluded.
    pub fn nexus_recover_undispatched(&self, id: &str, observed: &Value) -> Result<bool> {
        if observed["request"]["request_id"] != id
            || observed["request"]["status"] != "queued"
            || !observed["dispatch_id"].is_null()
        {
            return Ok(false);
        }
        let Some(previous) = self.nexus_request(id)? else {
            return Ok(false);
        };
        if previous["phase"] != "undetermined" || !previous["_known_unsent_recovery"].is_null() {
            return Ok(false);
        }
        // The whole frozen request and route must match, including response mode
        // and return context. Fresh queue metadata cannot redirect old intent.
        if previous["request"] != observed["request"] || previous["target"] != observed["target"] {
            return Err(anyhow!(
                "Nexus request identity changed before undispatched recovery"
            ));
        }
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction()?;
        let raw: String = tx.query_row("SELECT doc FROM nexus_requests WHERE id=?", [id], |r| {
            r.get(0)
        })?;
        let mut doc: Value = serde_json::from_str(&raw)?;
        doc["_known_unsent_recovery"] = json!({"observed_at":now_string(),"evidence":"fresh owner inspection: queued, no dispatch"});
        let changed = tx.execute(
            "UPDATE nexus_requests SET phase='queued',doc=?,instance_id=?,reason='One recovery after observed non-dispatch'
             WHERE id=? AND phase='undetermined' AND dispatch_id IS NULL AND local_turn IS NULL
             AND native_turn IS NULL AND echo_attempted=0
             AND reason='Dispatch acknowledgement missing; no automatic native replay'
             AND json_type(doc,'$._known_unsent_recovery') IS NULL",
            params![serde_json::to_string(&doc)?,self.instance_id,id],
        )?;
        tx.commit()?;
        Ok(changed == 1)
    }
    pub fn nexus_finish(&self, id: &str) -> Result<()> {
        if let Some(doc) = self.nexus_request(id)? {
            match doc["phase"].as_str() {
                Some("queued") => {
                    self.conn.lock().expect("store mutex poisoned").execute(
                        "UPDATE nexus_requests SET phase='retained_queue',reason='Worker ended before claim; explicit recovery required' WHERE id=? AND phase='queued' AND dispatch_id IS NULL AND local_turn IS NULL AND native_turn IS NULL AND instance_id=?",
                        params![id,self.instance_id],
                    )?;
                }
                Some("claimed") => self.nexus_receipt(
                    id,
                    "rejected",
                    None,
                    None,
                    Some("Worker ended before native submission"),
                )?,
                Some("submitting" | "steering" | "accepted") => self.nexus_phase(
                    id,
                    "undetermined",
                    "Native outcome unconfirmed; inspect retained output without replay",
                )?,
                _ => {}
            }
        }
        Ok(())
    }
    pub fn nexus_binding(&self, table: &str, author: &str) -> Result<Option<Value>> {
        let raw: Option<String> = self
            .conn
            .lock()
            .expect("store mutex poisoned")
            .query_row(
                "SELECT doc FROM nexus_bindings WHERE table_id=? AND author_ref=?",
                params![table, author],
                |r| r.get(0),
            )
            .optional()?;
        raw.map(|r| Ok(serde_json::from_str(&r)?)).transpose()
    }
    pub fn nexus_save_binding(&self, table: &str, author: &str, doc: &Value) -> Result<()> {
        self.conn.lock().expect("store mutex poisoned").execute(
            "INSERT INTO nexus_bindings VALUES(?,?,?) ON CONFLICT(table_id,author_ref) DO UPDATE SET doc=excluded.doc",params![table,author,serde_json::to_string(doc)?])?;
        Ok(())
    }
    pub fn nexus_receive(&self, id: &str, doc: &Value) -> Result<bool> {
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction()?;
        let old: Option<String> = tx
            .query_row("SELECT doc FROM nexus_requests WHERE id=?", [id], |r| {
                r.get(0)
            })
            .optional()?;
        if let Some(old) = old {
            let old: Value = serde_json::from_str(&old)?;
            if old["target"] != doc["target"]
                || old["request"]["contribution"] != doc["request"]["contribution"]
                || old["request"]["request_kind"] != doc["request"]["request_kind"]
                || old["request"]["iteration"] != doc["request"]["iteration"]
                || old["request"]["iteration_id"] != doc["request"]["iteration_id"]
                || old["request"]["iteration_revision"] != doc["request"]["iteration_revision"]
                || old["request"]["binding_id"] != doc["request"]["binding_id"]
                || old["request"]["binding_revision"] != doc["request"]["binding_revision"]
            {
                return Err(anyhow!("Nexus request identity changed"));
            }
            return Ok(false);
        }
        tx.execute(
            "INSERT INTO nexus_requests(id,doc,phase,instance_id) VALUES(?,?,'queued',?)",
            params![id, serde_json::to_string(doc)?, self.instance_id],
        )?;
        tx.commit()?;
        Ok(true)
    }
    pub fn nexus_request(&self, id: &str) -> Result<Option<Value>> {
        self.conn.lock().expect("store mutex poisoned").query_row(
            "SELECT doc,phase,dispatch_id,local_turn,native_turn,reason FROM nexus_requests WHERE id=?",[id],|r|{
                let raw:String=r.get(0)?;let mut value:Value=serde_json::from_str(&raw).map_err(|e|rusqlite::Error::FromSqlConversionFailure(0,rusqlite::types::Type::Text,Box::new(e)))?;
                value["phase"]=json!(r.get::<_,String>(1)?);value["dispatch_id"]=json!(r.get::<_,Option<String>>(2)?);value["local_turn"]=json!(r.get::<_,Option<i64>>(3)?);value["native_turn"]=json!(r.get::<_,Option<String>>(4)?);value["reason"]=json!(r.get::<_,Option<String>>(5)?);Ok(value)
            }).optional().map_err(Into::into)
    }
    pub fn nexus_begin_claim(&self, id: &str) -> Result<bool> {
        Ok(self.conn.lock().expect("store mutex poisoned").execute("UPDATE nexus_requests SET phase='claiming' WHERE id=? AND phase='queued' AND instance_id=?",params![id,self.instance_id])?==1)
    }
    pub fn nexus_claimed(&self, id: &str, doc: &Value) -> Result<()> {
        let original = self
            .nexus_request(id)?
            .ok_or_else(|| anyhow!("Nexus claim source missing"))?;
        if original["target"] != doc["target"]
            || [
                "request_id",
                "table_id",
                "author_ref",
                "binding_id",
                "binding_revision",
                "contribution",
                "request_kind",
                "iteration",
                "iteration_id",
                "iteration_revision",
                "return_context",
                "responder_ref",
            ]
            .iter()
            .any(|key| original["request"][key] != doc["request"][key])
        {
            return Err(anyhow!("Nexus dispatch changed the retained request"));
        }
        let dispatch = doc["dispatch_id"]
            .as_str()
            .ok_or_else(|| anyhow!("missing Nexus dispatch identity"))?;
        let n=self.conn.lock().expect("store mutex poisoned").execute("UPDATE nexus_requests SET phase='claimed',dispatch_id=?,doc=? WHERE id=? AND phase='claiming'",params![dispatch,serde_json::to_string(doc)?,id])?;
        if n != 1 {
            return Err(anyhow!("Nexus request was already claimed"));
        }
        Ok(())
    }
    pub fn nexus_phase(&self, id: &str, phase: &str, reason: &str) -> Result<()> {
        self.conn.lock().expect("store mutex poisoned").execute("UPDATE nexus_requests SET phase=?,reason=? WHERE id=? AND phase NOT IN ('completed','failed','interrupted','rejected')",params![phase,reason,id])?;
        Ok(())
    }
    pub fn nexus_link(&self, id: &str, turn: i64) -> Result<()> {
        self.conn.lock().expect("store mutex poisoned").execute(
            "UPDATE nexus_requests SET local_turn=? WHERE id=? AND phase='claimed'",
            params![turn, id],
        )?;
        Ok(())
    }
    pub fn nexus_echo_once(&self, id: &str) -> Result<bool> {
        Ok(self.conn.lock().expect("store mutex poisoned").execute(
            "UPDATE nexus_requests SET echo_attempted=1 WHERE id=? AND echo_attempted=0",
            [id],
        )? == 1)
    }
    pub fn nexus_local_requests(&self, turn: i64) -> Result<Vec<String>> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let mut stmt=conn.prepare("SELECT id FROM nexus_requests WHERE local_turn=? AND phase IN ('claimed','submitting','steering','accepted','undetermined')")?;
        Ok(stmt
            .query_map([turn], |r| r.get(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?)
    }
    pub fn nexus_native_started(&self, turn: i64, native: &str) -> Result<()> {
        self.conn.lock().expect("store mutex poisoned").execute(
            "INSERT OR IGNORE INTO nexus_executions(local_turn,native_turn) VALUES(?,?)",
            params![turn, native],
        )?;
        Ok(())
    }
    pub fn nexus_input_accepted(&self, turn: i64, native: &str) -> Result<()> {
        for id in self.nexus_local_requests(turn)? {
            if self
                .nexus_request(&id)?
                .is_some_and(|r| r["phase"] == "submitting" && r["native_turn"].is_null())
            {
                self.nexus_accept(&id, turn, native)?;
            }
        }
        Ok(())
    }
    pub fn nexus_accept(&self, id: &str, turn: i64, native: &str) -> Result<()> {
        let record = self
            .nexus_request(id)?
            .ok_or_else(|| anyhow!("Nexus request missing"))?;
        if !record["native_turn"].is_null() && record["native_turn"] != native {
            return Err(anyhow!("Nexus acceptance belongs to another turn"));
        }
        let receipt = json!({"request_id":id,"dispatch_id":record["dispatch_id"],"status":"accepted","native_thread_id":record["target"]["native_thread_id"],"native_turn_id":native});
        {
            let mut conn = self.conn.lock().expect("store mutex poisoned");
            let tx = conn.transaction()?;
            let changed=tx.execute("UPDATE nexus_requests SET phase='accepted',local_turn=?,native_turn=? WHERE id=? AND phase IN ('submitting','steering','accepted') AND (native_turn IS NULL OR native_turn=?)",params![turn,native,id,native])?;
            if changed != 1 {
                return Err(anyhow!(
                    "Nexus acceptance state does not authorize a receipt"
                ));
            }
            let raw = serde_json::to_string(&receipt)?;
            let old: Option<String> = tx
                .query_row(
                    "SELECT doc FROM nexus_receipts WHERE request_id=? AND status='accepted'",
                    [id],
                    |r| r.get(0),
                )
                .optional()?;
            if old.as_ref().is_some_and(|old| old != &raw) {
                return Err(anyhow!("Nexus acceptance receipt changed"));
            }
            tx.execute("INSERT OR IGNORE INTO nexus_receipts(request_id,status,doc) VALUES(?,'accepted',?)",params![id,raw])?;
            tx.commit()?;
        }
        self.nexus_native_started(turn, native)?;
        self.nexus_sync_terminal(id, turn, native)
    }
    pub fn nexus_final(&self, turn: i64, native: &str, text: &str) -> Result<()> {
        if text.trim().is_empty() {
            return Ok(());
        }
        self.nexus_native_started(turn, native)?;
        self.conn.lock().expect("store mutex poisoned").execute("UPDATE nexus_executions SET reply=CASE WHEN reply='' THEN ? ELSE reply||char(10)||char(10)||? END WHERE local_turn=? AND native_turn=? AND terminal IS NULL",params![text,text,turn,native])?;
        Ok(())
    }
    pub fn nexus_terminal(&self, turn: i64, native: &str, status: &str) -> Result<()> {
        self.nexus_native_started(turn, native)?;
        self.conn.lock().expect("store mutex poisoned").execute(
            "UPDATE nexus_executions SET terminal=? WHERE local_turn=? AND native_turn=?",
            params![status, turn, native],
        )?;
        for id in self.nexus_local_requests(turn)? {
            if self
                .nexus_request(&id)?
                .is_some_and(|r| r["native_turn"] == native)
            {
                self.nexus_sync_terminal(&id, turn, native)?;
            }
        }
        Ok(())
    }
    pub fn nexus_observed_reply(
        &self,
        turn: i64,
        native: &str,
        reply: &str,
        status: &str,
    ) -> Result<()> {
        self.nexus_native_started(turn, native)?;
        // Metadata inspection returns the entire selected final, not a delta.
        self.conn.lock().expect("store mutex poisoned").execute("UPDATE nexus_executions SET reply=? WHERE local_turn=? AND native_turn=? AND NOT EXISTS(SELECT 1 FROM nexus_requests r JOIN nexus_receipts c ON c.request_id=r.id WHERE r.local_turn=? AND r.native_turn=? AND c.status='completed')",params![reply,turn,native,turn,native])?;
        self.nexus_terminal(turn, native, status)
    }
    fn nexus_sync_terminal(&self, id: &str, turn: i64, native: &str) -> Result<()> {
        let terminal: Option<(Option<String>, String)> = self
            .conn
            .lock()
            .expect("store mutex poisoned")
            .query_row(
                "SELECT terminal,reply FROM nexus_executions WHERE local_turn=? AND native_turn=?",
                params![turn, native],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        if let Some((Some(status), reply)) = terminal {
            match status.as_str() {
                "completed" if !reply.trim().is_empty() && reply.chars().count()<=64000 =>self.nexus_receipt(id,"completed",Some(native),Some(&reply),None)?,
                "failed"|"interrupted"=>self.nexus_receipt(id,&status,Some(native),None,Some(&format!("Native turn reported {status}")))?,
                "completed"=>self.nexus_phase(id,"undetermined","Native completed without a representable final reply; inspect its retained output")?,
                _=>{}
            }
        }
        Ok(())
    }
    pub fn nexus_receipt(
        &self,
        id: &str,
        status: &str,
        native: Option<&str>,
        reply: Option<&str>,
        reason: Option<&str>,
    ) -> Result<()> {
        let record = self
            .nexus_request(id)?
            .ok_or_else(|| anyhow!("Nexus receipt source missing"))?;
        let target: &Value = &record["target"];
        let mut doc = json!({"request_id":id,"dispatch_id":record["dispatch_id"],"status":status,"native_thread_id":target["native_thread_id"]});
        if let Some(n) = native {
            doc["native_turn_id"] = json!(n);
        }
        if let Some(r) = reply {
            doc["reply"] = json!(r);
        }
        if let Some(r) = reason {
            doc["reason"] = json!(r);
        }
        let raw = serde_json::to_string(&doc)?;
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction()?;
        let old: Option<String> = tx
            .query_row(
                "SELECT doc FROM nexus_receipts WHERE request_id=? AND status=?",
                params![id, status],
                |r| r.get(0),
            )
            .optional()?;
        if old.as_ref().is_some_and(|old| old != &raw) {
            return Err(anyhow!("Nexus receipt content changed"));
        }
        tx.execute(
            "INSERT OR IGNORE INTO nexus_receipts(request_id,status,doc) VALUES(?,?,?)",
            params![id, status, raw],
        )?;
        if status != "accepted" {
            tx.execute(
                "UPDATE nexus_requests SET phase=? WHERE id=?",
                params![status, id],
            )?;
        }
        tx.commit()?;
        Ok(())
    }
    pub fn nexus_outbox(&self) -> Result<Vec<(i64, Value)>> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let mut stmt =
            conn.prepare("SELECT id,doc FROM nexus_receipts WHERE sent=0 ORDER BY id LIMIT 100")?;
        let raw = stmt
            .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        raw.into_iter()
            .map(|(id, raw)| Ok((id, serde_json::from_str(&raw)?)))
            .collect()
    }
    pub fn nexus_receipt_sent(&self, id: i64) -> Result<()> {
        self.conn
            .lock()
            .expect("store mutex poisoned")
            .execute("UPDATE nexus_receipts SET sent=1 WHERE id=?", [id])?;
        Ok(())
    }
    pub fn nexus_status(&self, key: SessionKey, user: i64) -> Result<Vec<(String, String)>> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let mut stmt=conn.prepare("SELECT id,phase FROM nexus_requests WHERE json_extract(doc,'$.target.chat_id')=? AND json_extract(doc,'$.target.topic_id')=? AND json_extract(doc,'$.target.human_user_id')=? ORDER BY rowid DESC LIMIT 20")?;
        Ok(stmt
            .query_map(params![key.chat_id, key.thread_id, user], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    fn open(path: &Path) -> Store {
        Store::open(
            path,
            &[100],
            &SessionDefaults {
                cwd: path.parent().unwrap().into(),
                model: None,
                reasoning_effort: None,
                session_prompt: None,
                sandbox_mode: "read-only".into(),
                approval_policy: "never".into(),
                search_mode: crate::config::SearchMode::Disabled,
                add_dirs: vec![],
            },
        )
        .unwrap()
    }
    #[test]
    fn ingress_capture_is_immutable_across_restart_and_failed_capture_is_not_retried() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("db");
        let store = open(&path);
        let update = |id| {
            serde_json::from_value::<crate::telegram::Update>(json!({"update_id":id,"message":{"message_id":id,"from":{"id":100,"is_bot":false,"first_name":"Human"},"chat":{"id":100,"type":"private"},"message_thread_id":7,"text":"this node"}})).unwrap()
        };
        let target = json!({"human_user_id":100,"native_thread_id":"native-a"});
        assert!(store.admit_update(&update(1)).unwrap());
        assert!(store.nexus_begin_input_capture(1, &target).unwrap());
        let a = json!({"status":"captured","perspective":{"selection":{"object_id":"A"}}});
        store.nexus_finish_input_capture(1, &a).unwrap();
        store.nexus_finish_input_capture(1, &a).unwrap();
        assert!(
            store
                .nexus_finish_input_capture(
                    1,
                    &json!({"status":"captured","perspective":{"selection":{"object_id":"B"}}})
                )
                .is_err()
        );
        assert!(store.admit_update(&update(2)).unwrap());
        assert!(store.nexus_begin_input_capture(2, &target).unwrap());
        drop(store);
        let store = open(&path);
        assert_eq!(store.nexus_input_context(1).unwrap().unwrap()["capture"], a);
        assert_eq!(
            store.nexus_input_context(2).unwrap().unwrap()["capture"]["status"],
            "capture_interrupted"
        );
        assert!(!store.admit_update(&update(1)).unwrap());
        assert!(!store.nexus_begin_input_capture(1, &target).unwrap());
        assert!(!store.nexus_begin_input_capture(2, &target).unwrap());
        assert!(!store.nexus_begin_input_capture(3, &target).unwrap());
        assert!(store.nexus_input_context(3).unwrap().is_none());
    }
    fn doc(id: &str) -> Value {
        json!({"dispatch_id":"dispatch-one","target":{"chat_id":100,"topic_id":7,"human_user_id":100,"native_thread_id":"existing"},"request":{"request_id":id,"binding_id":"binding","binding_revision":1,"contribution":{"text":"Human question","anchors":[]}}})
    }
    fn claimed(store: &Store, id: &str, turn: i64) {
        assert!(store.nexus_receive(id, &doc(id)).unwrap());
        assert!(store.nexus_begin_claim(id).unwrap());
        store.nexus_claimed(id, &doc(id)).unwrap();
        store.nexus_link(id, turn).unwrap();
    }
    #[test]
    fn unstarted_worker_retains_queue_for_explicit_recovery() {
        let root = tempfile::tempdir().unwrap();
        let store = open(&root.path().join("db"));
        assert!(store.nexus_receive("r", &doc("r")).unwrap());
        store.nexus_finish("r").unwrap();
        store.nexus_finish("r").unwrap();
        assert_eq!(
            store.nexus_request("r").unwrap().unwrap()["phase"],
            "retained_queue"
        );
        assert!(!store.nexus_begin_claim("r").unwrap());
        assert!(store.nexus_recover_queue("r").unwrap());
        assert!(store.nexus_begin_claim("r").unwrap());
        store.nexus_finish("r").unwrap();
        assert_eq!(
            store.nexus_request("r").unwrap().unwrap()["phase"],
            "claiming"
        );
    }
    #[test]
    fn duplicate_race_has_one_journal_and_claim() {
        let root = tempfile::tempdir().unwrap();
        let store = Arc::new(open(&root.path().join("db")));
        let handles: Vec<_> = (0..8)
            .map(|_| {
                let store = store.clone();
                std::thread::spawn(move || store.nexus_receive("r", &doc("r")).unwrap())
            })
            .collect();
        assert_eq!(
            handles
                .into_iter()
                .filter_map(|h| h.join().ok())
                .filter(|v| *v)
                .count(),
            1
        );
        assert!(store.nexus_begin_claim("r").unwrap());
        assert!(!store.nexus_begin_claim("r").unwrap());
        let mut changed = doc("r");
        changed["target"]["native_thread_id"] = json!("other");
        assert!(store.nexus_receive("r", &changed).is_err());
    }
    #[test]
    fn reviewed_iteration_cannot_change_during_claim_or_restart() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("db");
        let store = open(&path);
        let mut selected = doc("iteration-request");
        selected["request"]
            .as_object_mut()
            .unwrap()
            .remove("contribution");
        selected["request"]["request_kind"] = json!("iteration");
        selected["request"]["iteration_id"] = json!("research");
        selected["request"]["iteration_revision"] = json!(1);
        selected["request"]["iteration"] = json!({"revision":1,"spec":{"iteration_id":"research","tasks":[{"task_id":"map","instruction":"Preserve evidence boundaries"}]}});
        assert!(store.nexus_receive("iteration-request", &selected).unwrap());
        assert!(store.nexus_begin_claim("iteration-request").unwrap());
        let mut changed = selected.clone();
        changed["request"]["iteration"]["spec"]["tasks"][0]["instruction"] =
            json!("Different work");
        assert!(store.nexus_receive("iteration-request", &changed).is_err());
        assert!(store.nexus_claimed("iteration-request", &changed).is_err());
        store.nexus_claimed("iteration-request", &selected).unwrap();
        drop(store);
        let store = open(&path);
        assert!(!store.nexus_receive("iteration-request", &selected).unwrap());
        assert!(store.nexus_receive("iteration-request", &changed).is_err());
    }
    #[test]
    fn notifications_do_not_accept_input_and_steer_ack_owns_its_turn() {
        let root = tempfile::tempdir().unwrap();
        let store = open(&root.path().join("db"));
        claimed(&store, "r", 4);
        store.nexus_phase("r", "steering", "").unwrap();
        store.nexus_native_started(4, "old").unwrap();
        store
            .nexus_input_accepted(4, "unrelated-start-ack")
            .unwrap();
        assert!(store.nexus_outbox().unwrap().is_empty());
        store.nexus_accept("r", 4, "old").unwrap();
        assert_eq!(store.nexus_outbox().unwrap()[0].1["native_turn_id"], "old");
        assert!(store.nexus_accept("r", 4, "other").is_err());
        store.nexus_terminal(4, "other", "failed").unwrap();
        assert_eq!(
            store.nexus_request("r").unwrap().unwrap()["phase"],
            "accepted"
        );
    }
    #[test]
    fn terminal_before_ack_is_correlated_and_nonempty_reply_required() {
        let root = tempfile::tempdir().unwrap();
        let store = open(&root.path().join("db"));
        claimed(&store, "r", 4);
        store.nexus_phase("r", "submitting", "").unwrap();
        store
            .nexus_final(4, "actual", "Actual native final")
            .unwrap();
        store.nexus_terminal(4, "actual", "completed").unwrap();
        assert!(store.nexus_outbox().unwrap().is_empty());
        store.nexus_input_accepted(4, "actual").unwrap();
        let receipts = store.nexus_outbox().unwrap();
        assert_eq!(receipts.len(), 2);
        assert_eq!(receipts[0].1["status"], "accepted");
        assert_eq!(receipts[1].1["reply"], "Actual native final");
        claimed(&store, "empty", 5);
        store.nexus_phase("empty", "submitting", "").unwrap();
        store.nexus_accept("empty", 5, "empty-turn").unwrap();
        store.nexus_terminal(5, "empty-turn", "completed").unwrap();
        assert_eq!(
            store.nexus_request("empty").unwrap().unwrap()["phase"],
            "undetermined"
        );
        assert!(
            !store
                .nexus_outbox()
                .unwrap()
                .iter()
                .any(|(_, v)| v["request_id"] == "empty" && v["status"] == "completed")
        );
    }
    #[test]
    fn undispatched_recovery_requires_fresh_proof_identity_and_is_bounded() {
        let root = tempfile::tempdir().unwrap();
        let store = open(&root.path().join("db"));
        let mut fresh = doc("unsent");
        fresh["dispatch_id"] = Value::Null;
        fresh["request"]["status"] = json!("queued");
        store.nexus_receive("unsent", &fresh).unwrap();
        store.nexus_begin_claim("unsent").unwrap();
        store
            .nexus_phase(
                "unsent",
                "undetermined",
                "Dispatch acknowledgement missing; no automatic native replay",
            )
            .unwrap();
        let mut dispatched = fresh.clone();
        dispatched["request"]["status"] = json!("handed_off");
        dispatched["dispatch_id"] = json!("remote-committed");
        assert!(
            !store
                .nexus_recover_undispatched("unsent", &dispatched)
                .unwrap()
        );
        let mut changed = fresh.clone();
        changed["request"]["contribution"]["text"] = json!("Changed source");
        assert!(
            store
                .nexus_recover_undispatched("unsent", &changed)
                .is_err()
        );
        let mut changed_mode = fresh.clone();
        changed_mode["request"]["response_mode"] = json!("another_mode");
        assert!(
            store
                .nexus_recover_undispatched("unsent", &changed_mode)
                .is_err()
        );
        let mut changed_context = fresh.clone();
        changed_context["request"]["return_context"] = json!({"playback_ms":999});
        assert!(
            store
                .nexus_recover_undispatched("unsent", &changed_context)
                .is_err()
        );
        assert!(store.nexus_recover_undispatched("unsent", &fresh).unwrap());
        assert_eq!(
            store.nexus_request("unsent").unwrap().unwrap()["request"]["contribution"],
            fresh["request"]["contribution"]
        );
        assert!(store.nexus_begin_claim("unsent").unwrap());
        store
            .nexus_phase(
                "unsent",
                "undetermined",
                "Dispatch acknowledgement missing; no automatic native replay",
            )
            .unwrap();
        assert!(!store.nexus_recover_undispatched("unsent", &fresh).unwrap());
        assert!(store.nexus_outbox().unwrap().is_empty());
    }
    #[test]
    fn missing_terminal_requires_exact_admission_and_native_observation() {
        let root = tempfile::tempdir().unwrap();
        let store = open(&root.path().join("db"));
        let defaults = SessionDefaults {
            cwd: root.path().into(),
            model: None,
            reasoning_effort: None,
            session_prompt: None,
            sandbox_mode: "read-only".into(),
            approval_policy: "never".into(),
            search_mode: crate::config::SearchMode::Disabled,
            add_dirs: vec![],
        };
        let key = crate::models::SessionKey::new(100, Some(7));
        let session = store.ensure_session(key, 100, &defaults).unwrap();
        let turn = store
            .record_turn_started(
                session.id,
                &crate::models::TurnRequest {
                    session_key: key,
                    from_user_id: 100,
                    prompt: "Question".into(),
                    runtime_instructions: None,
                    attachments: vec![],
                    review_mode: None,
                    override_search_mode: None,
                },
            )
            .unwrap();
        store.record_turn_native_thread(turn, "existing").unwrap();
        claimed(&store, "recover", turn);
        store.nexus_phase("recover", "submitting", "").unwrap();
        assert!(store.nexus_missing_terminals().unwrap().is_empty());
        store.nexus_accept("recover", turn, "exact-native").unwrap();
        store
            .nexus_final(turn, "exact-native", "Retained final")
            .unwrap();
        // A live collector must not trigger recovery; a local completion still
        // does not manufacture a native completion receipt.
        assert!(store.nexus_missing_terminals().unwrap().is_empty());
        store
            .record_turn_finished(turn, "completed", Some("Retained final"))
            .unwrap();
        assert_eq!(
            store.nexus_missing_terminals().unwrap(),
            vec![(turn, "exact-native".into(), "existing".into())]
        );
        assert!(
            !store
                .nexus_outbox()
                .unwrap()
                .iter()
                .any(|(_, r)| r["status"] == "completed")
        );
        store
            .nexus_observed_reply(turn, "exact-native", "Retained final", "completed")
            .unwrap();
        assert!(store.nexus_missing_terminals().unwrap().is_empty());
        assert_eq!(
            store
                .nexus_outbox()
                .unwrap()
                .iter()
                .filter(|(_, r)| r["status"] == "completed")
                .count(),
            1
        );
        store.nexus_reconcile_observations().unwrap();
        assert_eq!(
            store
                .nexus_outbox()
                .unwrap()
                .iter()
                .filter(|(_, r)| r["status"] == "completed")
                .count(),
            1
        );
    }
    #[test]
    fn restart_reconstructs_observed_receipts_without_replaying_native() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("db");
        let store = open(&path);
        claimed(&store, "r", 4);
        store.nexus_phase("r", "submitting", "").unwrap();
        store.nexus_accept("r", 4, "actual").unwrap();
        store.nexus_final(4, "actual", "Retained reply").unwrap();
        // Simulate crashes after acceptance/terminal observation, before outbox.
        store
            .conn
            .lock()
            .unwrap()
            .execute("DELETE FROM nexus_receipts", [])
            .unwrap();
        store
            .conn
            .lock()
            .unwrap()
            .execute("UPDATE nexus_executions SET terminal='completed'", [])
            .unwrap();
        drop(store);
        let reopened = open(&path);
        assert_eq!(reopened.nexus_outbox().unwrap().len(), 2);
        assert_eq!(
            reopened.nexus_request("r").unwrap().unwrap()["phase"],
            "completed"
        );
        assert!(!reopened.nexus_begin_claim("r").unwrap());
        assert!(!reopened.nexus_recover_queue("r").unwrap());
    }
    #[test]
    fn only_never_claimed_queue_can_be_explicitly_recovered() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("db");
        let store = open(&path);
        store.nexus_receive("queued", &doc("queued")).unwrap();
        claimed(&store, "unknown", 4);
        store.nexus_phase("unknown", "submitting", "").unwrap();
        drop(store);
        let store = open(&path);
        assert_eq!(
            store.nexus_request("queued").unwrap().unwrap()["phase"],
            "retained_queue"
        );
        assert!(!store.nexus_begin_claim("queued").unwrap());
        assert!(store.nexus_recover_queue("queued").unwrap());
        assert!(store.nexus_begin_claim("queued").unwrap());
        assert!(!store.nexus_recover_queue("unknown").unwrap());
        assert_eq!(
            store.nexus_request("unknown").unwrap().unwrap()["phase"],
            "undetermined"
        );
    }
}
