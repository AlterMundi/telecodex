//! Read-only canonical sources for receiving text/audio presentation.
use super::*;
use anyhow::bail;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use tokio_util::sync::CancellationToken;

// Worker progress is visible only through the return selection retained when
// this native item was first observed. Later direct input never widens its scope.
const NATIVE_VISIBILITY: &str = "t.from_user_id=?3 AND s.creator_user_id=?3 AND
    ((t.session_id=?2 AND m.native_thread=?4 AND (m.phase IN ('commentary','final_answer') OR m.phase IS NULL))
      OR (EXISTS(SELECT 1 FROM conversation_inputs c
          WHERE c.id=m.presentation_input AND c.human_id=?3 AND c.session_id=?2)
          AND (m.phase='commentary' OR (m.phase='final_answer' AND EXISTS(
              SELECT 1 FROM interaction_goal_controls g WHERE g.human_id=?3
              AND g.phase='acknowledged' AND json_extract(g.doc,'$.worker_execution.local')=m.local_turn
              AND json_extract(g.doc,'$.worker_execution.native')=m.native_thread)
              AND NOT EXISTS(SELECT 1 FROM interaction_results r WHERE r.native_thread=m.native_thread
                  AND r.native_turn=m.native_turn AND r.item_id=m.item_id)))))";

fn companion_hint_key(native: &str, physical: &str) -> String {
    format!(
        "interaction-companion:{}",
        digest(&json!([native, physical]))
    )
}

fn digest(value: &Value) -> String {
    format!("{:x}", Sha256::digest(value.to_string().as_bytes()))
}

pub(super) fn entry_on(conn: &Connection, human: i64, entry: SessionKey) -> Result<(i64, String)> {
    conn.query_row(
        "SELECT s.id,COALESCE(s.codex_thread_id,'') FROM sessions s JOIN users u ON u.tg_user_id=s.creator_user_id
         WHERE s.chat_id=?1 AND s.thread_id=?2 AND s.creator_user_id=?3 AND u.allowed=1
         AND (s.codex_thread_id IS NULL OR s.force_fresh_thread=0)",
        params![entry.chat_id, entry.thread_id, human],
        |r| Ok((r.get(0)?, r.get(1)?)),
    ).context("Integrating entry owner or native binding unavailable")
}

pub(super) fn entry_binding(
    conn: &Connection,
    human: i64,
    entry: SessionKey,
    session: i64,
) -> Result<String> {
    let generation: String = conn.query_row(
        "SELECT generation FROM conversation_entry_generations WHERE session_id=?1",
        [session],
        |r| r.get(0),
    )?;
    Ok(digest(&json!([human, entry, session, generation])))
}

impl Store {
    /// Register at most one cosmetic choice for an actual current execution.
    /// A recorded final source cannot acquire a hint later and change revision.
    #[allow(clippy::too_many_arguments)]
    pub fn interaction_companion_hint(
        &self,
        key: SessionKey,
        human: i64,
        local: i64,
        native: &str,
        physical: &str,
        gesture: &str,
        rest: &str,
        pack: Option<&crate::ascii::ExpressionPack>,
        closed: &CancellationToken,
    ) -> Result<bool> {
        if !(if pack.is_some() {
            matches!(gesture, "play" | "none")
        } else {
            matches!(gesture, "wave" | "nod" | "none")
        }) || !(if let Some(pack) = pack {
            pack.validate()?;
            pack.expressions.contains_key(rest)
        } else {
            matches!(
                rest,
                "calm"
                    | "curious"
                    | "focused"
                    | "listening"
                    | "speaking"
                    | "delighted"
                    | "uncertain"
            )
        }) || physical.is_empty()
            || physical.len() > 128
            || closed.is_cancelled()
        {
            bail!("Invalid or expired companion hint");
        }
        let mut connection = self.conn.lock().expect("store mutex poisoned");
        let conn =
            connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let valid: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM turns t JOIN sessions s ON s.id=t.session_id
             JOIN users u ON u.tg_user_id=s.creator_user_id
             WHERE t.id=?1 AND t.from_user_id=?2 AND t.status='running' AND t.completed_at IS NULL
             AND t.native_thread_id=?3 AND s.codex_thread_id=?3 AND s.creator_user_id=?2
             AND s.chat_id=?4 AND s.thread_id=?5 AND u.allowed=1 AND s.force_fresh_thread=0)",
            params![local, human, native, key.chat_id, key.thread_id],
            |r| r.get(0),
        )?;
        let final_seen: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM interaction_native_messages
             WHERE native_thread=?1 AND native_turn=?2 AND (phase='final_answer' OR phase IS NULL))
             OR EXISTS(SELECT 1 FROM interaction_results WHERE native_thread=?1 AND native_turn=?2)",
            params![native,physical], |r|r.get(0),
        )?;
        if !valid || final_seen || closed.is_cancelled() {
            bail!("Companion hint execution changed or final source already exists");
        }
        let id = companion_hint_key(native, physical);
        let mut doc = json!({"gesture":gesture,"rest":rest});
        if let Some(pack) = pack {
            doc["catalog"] = json!(pack.fingerprint()?);
            doc["pack"] = json!(pack.id);
            doc["revision"] = json!(pack.revision);
        }
        let doc = doc.to_string();
        let previous: Option<String> = conn
            .query_row("SELECT value FROM bot_state WHERE key=?", [&id], |r| {
                r.get(0)
            })
            .optional()?;
        if let Some(previous) = previous {
            if previous != doc {
                bail!("A cosmetic choice is already registered for this native turn");
            }
            return Ok(false);
        }
        conn.execute(
            "INSERT INTO bot_state(key,value) VALUES(?1,?2)",
            params![id, doc],
        )?;
        conn.commit()?;
        Ok(true)
    }

    /// Current receiver bookkeeping, not a claim of native/model completion.
    /// Only configured, still-owned sources are included; no native I/O occurs.
    pub fn interaction_activity(
        &self,
        human: i64,
        entry: SessionKey,
        targets: &[(SessionKey, String)],
    ) -> Result<Value> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let (entry_id, _) = entry_on(&conn, human, entry)?;
        let entry_busy: bool =
            conn.query_row("SELECT busy FROM sessions WHERE id=?", [entry_id], |r| {
                r.get(0)
            })?;
        let mut sources = Vec::new();
        for (key, label) in targets.iter().take(64) {
            let selected: Option<(i64, bool, Option<String>)> = conn
                .query_row(
                    "SELECT s.id,s.busy,s.codex_thread_id FROM sessions s
                 JOIN users u ON u.tg_user_id=s.creator_user_id
                 WHERE s.chat_id=?1 AND s.thread_id=?2 AND s.creator_user_id=?3
                 AND u.allowed=1 AND s.force_fresh_thread=0",
                    params![key.chat_id, key.thread_id, human],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .optional()?;
            let Some((id, busy, native)) = selected else {
                sources.push(json!({"label":label,"known":false}));
                continue;
            };
            let synthesizing: i64 = conn.query_row(
                "SELECT count(*) FROM interaction_observer_attempts
                 WHERE session_id=?1 AND human_id=?2 AND native_thread=?3
                 AND instance_id=?4 AND phase='summarizing'",
                params![id, human, native, self.instance_id],
                |r| r.get(0),
            )?;
            sources.push(json!({"label":label,"known":native.is_some(),
                "busy":busy,"observer_synthesizing":synthesizing}));
        }
        Ok(json!({"schema":"telecodex.interaction-activity/v1",
            "observed_at":Utc::now().to_rfc3339(),"integrator_busy":entry_busy,
            "scope":"enrolled_sessions","sources":sources,"truncated":targets.len()>64}))
    }
    /// Bounded incremental metadata. This never marks a result delivered/heard.
    pub fn interaction_presentation_list(
        &self,
        human: i64,
        entry: SessionKey,
        after_delivery: i64,
        after_native: i64,
    ) -> Result<Value> {
        self.interaction_presentation_page(human, entry, after_delivery, after_native, false)
    }

    pub fn interaction_presentation_page(
        &self,
        human: i64,
        entry: SessionKey,
        after_delivery: i64,
        after_native: i64,
        before: bool,
    ) -> Result<Value> {
        self.interaction_presentation_page_matrix(
            human,
            entry,
            after_delivery,
            after_native,
            None,
            before,
        )
    }

    pub fn interaction_presentation_page_matrix(
        &self,
        human: i64,
        entry: SessionKey,
        after_delivery: i64,
        after_native: i64,
        after_matrix: Option<i64>,
        before: bool,
    ) -> Result<Value> {
        if after_delivery < 0 || after_native < 0 || after_matrix.is_some_and(|v| v < 0) {
            bail!("Invalid presentation cursor");
        }
        let conn = self.conn.lock().expect("store mutex poisoned");
        let (session, native) = entry_on(&conn, human, entry)?;
        let (comparison, order) = if before { ("<", "DESC") } else { (">", "ASC") };
        let delivery_bound = if before && after_delivery == 0 {
            i64::MAX
        } else {
            after_delivery
        };
        let native_bound = if before && after_native == 0 {
            i64::MAX
        } else {
            after_native
        };
        let mut stmt = conn.prepare(&format!(
            "SELECT d.rowid,d.id FROM interaction_deliveries d
            JOIN interaction_requests r ON r.id=d.request_id
            JOIN interaction_sources x ON x.id=r.input_id
            JOIN sessions s ON s.id=r.session_id
            WHERE d.rowid{comparison}?1 AND d.human_id=?2 AND r.human_id=?2 AND x.human_id=?2
            AND s.creator_user_id=?2 AND NOT EXISTS(SELECT 1 FROM interaction_session_authors a
                WHERE a.input_id=r.input_id AND json_extract(a.doc,'$.kind')='session_request')
            AND json_extract(d.doc,'$.origin.chat_id')=?3 AND json_extract(d.doc,'$.origin.thread_id')=?4
            ORDER BY d.rowid {order} LIMIT 9"
        ))?;
        let deliveries = stmt
            .query_map(
                params![delivery_bound, human, entry.chat_id, entry.thread_id],
                |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        format!("delivery:{}", r.get::<_, String>(1)?),
                    ))
                },
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut stmt = conn.prepare(&format!(
            "SELECT m.rowid FROM interaction_native_messages m
            JOIN turns t ON t.id=m.local_turn JOIN sessions s ON s.id=t.session_id
            WHERE m.rowid{comparison}?1 AND {NATIVE_VISIBILITY}
            ORDER BY m.rowid {order} LIMIT 9"
        ))?;
        let natives = stmt
            .query_map(params![native_bound, session, human, native], |r| {
                r.get::<_, i64>(0)
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let matrices = if let Some(cursor) = after_matrix {
            let bound = if before && cursor == 0 {
                i64::MAX
            } else {
                cursor
            };
            let mut query=conn.prepare(&format!("SELECT r.rowid,r.message_id FROM interaction_matrix_results r JOIN interaction_matrix_requests m ON m.id=r.request_id WHERE r.rowid{comparison}?1 AND r.human_id=?2 AND m.human_id=?2 AND m.entry_session=?3 ORDER BY r.rowid {order} LIMIT 9"))?;
            query
                .query_map(params![bound, human, session], |r| {
                    Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
                })?
                .collect::<rusqlite::Result<Vec<_>>>()?
        } else {
            vec![]
        };
        let mut items = Vec::new();
        for (sequence, id) in matrices.iter().take(8) {
            if let Some(mut source) = source_on(&conn, human, entry, &format!("matrix:{id}"))? {
                source["sequence"] = json!(sequence);
                preview(&mut source);
                items.push(source);
            }
        }
        for (sequence, id) in deliveries.iter().take(8) {
            if let Some(mut source) = source_on(&conn, human, entry, id)? {
                source["sequence"] = json!(sequence);
                preview(&mut source);
                items.push(source);
            }
        }
        for row in natives.iter().take(8) {
            if let Some(mut source) = source_on(&conn, human, entry, &format!("native:{row}"))? {
                source["sequence"] = json!(row);
                preview(&mut source);
                items.push(source);
            }
        }
        let mut result = json!({"schema":"telecodex.presentation-list/v1",
            "binding":entry_binding(&conn,human,entry,session)?,"items":items,"can_report_playback":true,
            "cursor":{"delivery":deliveries.iter().take(8).next_back().map_or(after_delivery,|r|r.0),
                      "native":natives.iter().take(8).next_back().copied().unwrap_or(after_native)},
            "newest":{"delivery":deliveries.iter().take(8).map(|r|r.0).max().unwrap_or(0),
                      "native":natives.iter().take(8).copied().max().unwrap_or(0)},
            "more":deliveries.len()>8 || natives.len()>8 || matrices.len()>8,"playback":"not_observed"});
        if let Some(cursor) = after_matrix {
            result["cursor"]["matrix"] =
                json!(matrices.iter().take(8).next_back().map_or(cursor, |r| r.0));
            result["newest"]["matrix"] =
                json!(matrices.iter().take(8).map(|r| r.0).max().unwrap_or(0));
        }
        Ok(result)
    }

    /// The revision fences source content, owner/native bindings and relevance.
    /// A caller rechecks it after synthesis and immediately before serving audio.
    pub fn interaction_presentation_part(
        &self,
        human: i64,
        entry: SessionKey,
        id: &str,
        revision: &str,
        offset: usize,
    ) -> Result<Value> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let mut source =
            source_on(&conn, human, entry, id)?.context("Presentation source unavailable")?;
        if source["revision"].as_str() != Some(revision) {
            bail!("Presentation source revision changed");
        }
        let text = source["text"]
            .as_str()
            .context("Presentation text unavailable")?;
        let chars = text.chars().count();
        if offset >= chars {
            bail!("Presentation offset outside source");
        }
        let selected: String = text.chars().skip(offset).take(1600).collect();
        let end = offset + selected.chars().count();
        source["text"] = json!(selected);
        source["offset"] = json!(offset);
        source["next_offset"] = if end < chars { json!(end) } else { Value::Null };
        source["schema"] = json!("telecodex.presentation-part/v1");
        Ok(source)
    }
}

fn preview(source: &mut Value) {
    source["preview"] = json!(
        source["text"]
            .as_str()
            .unwrap_or("")
            .chars()
            .take(240)
            .collect::<String>()
    );
    source
        .as_object_mut()
        .expect("source object")
        .remove("text");
}

pub(super) fn source_on(
    conn: &Connection,
    human: i64,
    entry: SessionKey,
    id: &str,
) -> Result<Option<Value>> {
    let (entry_session, entry_native) = entry_on(conn, human, entry)?;
    let mut source = if let Some(matrix) = id.strip_prefix("matrix:") {
        let Some(source) = super::matrix::result_on(conn, human, entry, matrix)? else {
            return Ok(None);
        };
        source
    } else if let Some(id) = id.strip_prefix("delivery:") {
        let row:Option<(String,String,Option<String>,String,bool,String)>=conn.query_row(
            "SELECT d.doc,d.phase,s.codex_thread_id,o.native_thread,s.force_fresh_thread,d.result_id FROM interaction_deliveries d
             JOIN interaction_requests r ON r.id=d.request_id JOIN interaction_sources x ON x.id=r.input_id
             JOIN sessions s ON s.id=r.session_id JOIN interaction_results o ON o.id=d.result_id
             WHERE d.id=?1 AND d.human_id=?2 AND r.human_id=?2 AND x.human_id=?2 AND s.creator_user_id=?2
             AND NOT EXISTS(SELECT 1 FROM interaction_session_authors a
                 WHERE a.input_id=r.input_id AND json_extract(a.doc,'$.kind')='session_request')
             AND json_extract(d.doc,'$.origin.chat_id')=?3 AND json_extract(d.doc,'$.origin.thread_id')=?4",
             params![id,human,entry.chat_id,entry.thread_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?))).optional()?;
        let Some((doc, phase, current_native, result_native, force_fresh, result_id)) = row else {
            return Ok(None);
        };
        let delivery: crate::interaction::Delivery = serde_json::from_str(&doc)?;
        let (current, pending) = super::routing::delivery_freshness(conn, &delivery)?;
        let request_ids = delivery.requests().to_vec();
        json!({"id":format!("delivery:{id}"),"kind":"worker_result","text":delivery.body,
            "request_id":delivery.request_id,"request_ids":request_ids,"result_id":result_id,"target":delivery.target,"current":current,
            "correction_pending":pending,"completed":delivery.completed,"delivery_phase":phase,
            "native_binding_current":!force_fresh && current_native.as_deref()==Some(result_native.as_str()),
            "target_native":current_native,"result_native":result_native})
    } else if let Some(row) = id
        .strip_prefix("native:")
        .and_then(|s| s.parse::<i64>().ok())
        .filter(|n| *n > 0)
    {
        let found = conn.query_row(&format!(
            "SELECT m.text,m.native_turn,m.item_id,m.phase,m.observed_at,
             t.session_id=?2 AND m.native_thread=?4,s.chat_id,s.thread_id,
             s.codex_thread_id IS m.native_thread AND s.force_fresh_thread=0
             FROM interaction_native_messages m JOIN turns t ON t.id=m.local_turn
             JOIN sessions s ON s.id=t.session_id WHERE m.rowid=?1 AND {NATIVE_VISIBILITY}"),
             params![row,entry_session,human,entry_native],|r| {
                let is_entry:bool=r.get(5)?;
                let phase:Option<String>=r.get(3)?;
                let phase=phase.as_deref().unwrap_or("final_answer");
                Ok(json!({"id":id,"kind":if !is_entry {if phase=="commentary" {"worker_progress"} else {"worker_result"}} else if phase=="commentary" {"integrator_progress"} else {"integrator_answer"},
                    "text":r.get::<_,String>(0)?,"native_turn":r.get::<_,String>(1)?,"native_item":r.get::<_,String>(2)?,"observed_at":r.get::<_,String>(4)?,
                    "target":if is_entry {Value::Null} else {json!(SessionKey::new(r.get(6)?,Some(r.get(7)?)))},
                    "current":Value::Null,"correction_pending":Value::Null,"completed":Value::Null,
                    "native_binding_current":r.get::<_,bool>(8)?}))
             }).optional()?;
        let Some(source) = found else {
            return Ok(None);
        };
        source
    } else {
        return Ok(None);
    };
    if let Some(delivery) = id.strip_prefix("delivery:") {
        let observed: Option<String> = conn.query_row(
            "SELECT m.observed_at FROM interaction_deliveries d JOIN interaction_results r ON r.id=d.result_id JOIN interaction_native_messages m ON m.native_thread=r.native_thread AND m.native_turn=r.native_turn AND m.item_id=r.item_id WHERE d.id=?1",
            [delivery], |r| r.get(0)).optional()?;
        source["observed_at"] = json!(observed);
    }
    // Only a returned final source carries its immutable per-turn cosmetic choice.
    let hint_source: Option<(String, String)> =
        if source["kind"] == "worker_result" && id.starts_with("native:") {
            conn.query_row(
                "SELECT native_thread,native_turn FROM interaction_native_messages WHERE rowid=?",
                [id.trim_start_matches("native:").parse::<i64>()?],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?
        } else if source["kind"] == "worker_result" {
            conn.query_row(
                "SELECT native_thread,native_turn FROM interaction_results WHERE id=?",
                [source["result_id"].as_str().unwrap_or("")],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?
        } else if source["kind"] == "integrator_answer" {
            Some((
                entry_native.clone(),
                source["native_turn"].as_str().unwrap_or("").to_owned(),
            ))
        } else {
            None
        };
    if let Some((native, physical)) = hint_source {
        let raw: Option<String> = conn
            .query_row(
                "SELECT value FROM bot_state WHERE key=?",
                [companion_hint_key(&native, &physical)],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(raw) = raw {
            source["companion_hint"] = serde_json::from_str(&raw)?;
        }
    }
    // Canonical native items keep controller metadata. All reader/player views
    // use the visible answer, including its hash, length and revision fences.
    if let Some(text) = source["text"].as_str() {
        let visible = crate::interaction::visible_answer(text).to_owned();
        source["text"] = json!(visible);
    }
    source["entry_binding"] = json!([
        human,
        entry,
        entry_session,
        entry_binding(conn, human, entry, entry_session)?
    ]);
    source["text_sha256"] = json!(format!(
        "{:x}",
        Sha256::digest(source["text"].as_str().unwrap_or("").as_bytes())
    ));
    source["text_characters"] = json!(source["text"].as_str().unwrap_or("").chars().count());
    source["revision"] = json!(digest(&source));
    source["playback"] = json!("not_observed");
    // File availability may change after a reply starts playing. It must not
    // invalidate the immutable spoken-text revision or replay narration.
    source["artifact_status"] = json!(super::artifacts::status_on(
        conn,
        source["result_id"].as_str(),
        id.strip_prefix("native:").and_then(|s| s.parse().ok())
    )?);
    source["artifacts"] = json!(super::artifacts::views_on(
        conn,
        human,
        entry_session,
        source["result_id"].as_str(),
        id.strip_prefix("native:").and_then(|s| s.parse().ok()),
        false
    )?);
    Ok(Some(source))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interaction::{RequestRelation, ReturnRoute, SourceInput, TransportPhase};

    #[test]
    fn companion_choice_is_bounded_owned_and_immutable_before_final_source() {
        let (_file, store, entry, _, _, _) = setup();
        let session = store.get_session(entry).unwrap().unwrap();
        let local = store
            .record_turn_started(
                session.id,
                &TurnRequest {
                    session_key: entry,
                    from_user_id: 100,
                    prompt: "Discuss".into(),
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
        let closed = CancellationToken::new();
        let register = |human, native, gesture, rest| {
            store.interaction_companion_hint(
                entry,
                human,
                local,
                native,
                "entry-physical",
                gesture,
                rest,
                None,
                &closed,
            )
        };
        assert!(register(200, "entry-native", "wave", "calm").is_err());
        assert!(register(100, "other-native", "wave", "calm").is_err());
        assert!(register(100, "entry-native", "run-command", "calm").is_err());
        assert!(register(100, "entry-native", "wave", "<script>").is_err());
        assert!(register(100, "entry-native", "nod", "delighted").unwrap());
        assert!(!register(100, "entry-native", "nod", "delighted").unwrap());
        assert!(register(100, "entry-native", "wave", "calm").is_err());
        store
            .record_native_message(
                local,
                "entry-native",
                "entry-physical",
                "reply",
                Some("final_answer"),
                "Visible reply",
            )
            .unwrap();
        let page = store
            .interaction_presentation_list(100, entry, 0, 0)
            .unwrap();
        let source = page["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["kind"] == "integrator_answer")
            .unwrap();
        assert_eq!(
            source["companion_hint"],
            json!({"gesture":"nod","rest":"delighted"})
        );
        assert_eq!(source["preview"], "Visible reply");
        let revision = source["revision"].clone();
        assert!(
            register(100, "entry-native", "nod", "delighted").is_err(),
            "a final source cannot acquire/rewrite metadata"
        );
        let again = store
            .interaction_presentation_list(100, entry, 0, 0)
            .unwrap();
        assert_eq!(
            again["items"]
                .as_array()
                .unwrap()
                .iter()
                .find(|s| s["kind"] == "integrator_answer")
                .unwrap()["revision"],
            revision
        );
        closed.cancel();
        assert!(register(100, "entry-native", "nod", "delighted").is_err());
    }

    #[test]
    fn live_activity_respects_current_ownership_binding_and_attempt_instance() {
        let (_file, store, entry, target, _, _) = setup();
        let key = SessionKey::new(100, Some(7));
        let targets = vec![
            (key, "Alpha".into()),
            (SessionKey::new(100, Some(88)), "Missing".into()),
        ];
        store.set_session_busy(key, true).unwrap();
        {
            let conn = store.conn.lock().unwrap();
            for (id, native, instance) in [
                ("current", "worker-native", store.instance_id.as_str()),
                ("old-native", "previous-native", store.instance_id.as_str()),
                ("old-process", "worker-native", "previous-process"),
            ] {
                conn.execute("INSERT INTO interaction_observer_attempts VALUES(?1,?2,100,?3,?1,'policy',0,'{}','now','summarizing',?4)",
                    params![id,target.id,native,instance]).unwrap();
            }
        }
        let activity = store.interaction_activity(100, entry, &targets).unwrap();
        assert_eq!(activity["sources"][0]["busy"], true);
        assert_eq!(activity["sources"][0]["observer_synthesizing"], 1);
        assert_eq!(activity["sources"][1]["known"], false);
        store
            .set_session_codex_thread(key, "replacement-native")
            .unwrap();
        assert_eq!(
            store.interaction_activity(100, entry, &targets).unwrap()["sources"][0]["observer_synthesizing"],
            0
        );
        store
            .conn
            .lock()
            .unwrap()
            .execute(
                "UPDATE sessions SET creator_user_id=200 WHERE id=?",
                [target.id],
            )
            .unwrap();
        let denied = store.interaction_activity(100, entry, &targets).unwrap();
        assert_eq!(denied["sources"][0], json!({"label":"Alpha","known":false}));
        assert!(store.interaction_activity(200, entry, &targets).is_err());
    }

    fn setup() -> (
        tempfile::NamedTempFile,
        Store,
        SessionKey,
        SessionRecord,
        crate::interaction::Instruction,
        String,
    ) {
        let file = tempfile::NamedTempFile::new().unwrap();
        let defaults = super::super::tests::defaults();
        let store = Store::open(file.path(), &[100], &defaults).unwrap();
        let entry = SessionKey::new(100, Some(1));
        let worker = SessionKey::new(100, Some(7));
        for (key, native) in [(entry, "entry-native"), (worker, "worker-native")] {
            store.ensure_session(key, 100, &defaults).unwrap();
            store.set_session_codex_thread(key, native).unwrap();
        }
        let target = store.get_session(worker).unwrap().unwrap();
        let (instruction, _) = store
            .interaction_admit(
                &SourceInput {
                    transcription: None,
                    channel: "fixture".into(),
                    event_id: "1".into(),
                    update_id: None,
                    human_id: 100,
                    target_sessions: vec![target.id],
                    origin: ReturnRoute {
                        chat_id: 100,
                        thread_id: 1,
                        message_id: Some(1),
                    },
                    text: "Calculate".into(),
                },
                worker,
                RequestRelation::New,
            )
            .unwrap();
        let local = store
            .record_turn_started(
                target.id,
                &TurnRequest {
                    session_key: worker,
                    from_user_id: 100,
                    prompt: instruction.text.clone(),
                    runtime_instructions: None,
                    attachments: vec![],
                    review_mode: None,
                    override_search_mode: None,
                },
            )
            .unwrap();
        store
            .record_turn_native_thread(local, "worker-native")
            .unwrap();
        store
            .interaction_transport(
                &instruction.id,
                100,
                local,
                TransportPhase::Submitting,
                None,
            )
            .unwrap();
        store
            .interaction_transport(
                &instruction.id,
                100,
                local,
                TransportPhase::Accepted,
                Some("physical"),
            )
            .unwrap();
        let body = "Respuesta á😀 ".repeat(180);
        let text = format!(
            "{body}\n<daimon-result>{}</daimon-result>",
            json!({"requests":instruction.requests,"covered":[instruction.id],"completed":instruction.requests})
        );
        store
            .interaction_record_result(
                &instruction.id,
                100,
                local,
                "worker-native",
                "physical",
                "final",
                &text,
                false,
            )
            .unwrap();
        (file, store, entry, target, instruction, body)
    }

    #[test]
    fn worker_progress_keeps_capture_scope_across_direct_switch_and_reobservation() {
        let (_file, store, entry, _, instruction, _) = setup();
        store
            .save_bot_state(
                "telegram_entry_projection",
                &json!([100, entry.chat_id, entry.thread_id]).to_string(),
            )
            .unwrap();
        let binding = store
            .interaction_presentation_list(100, entry, 0, 0)
            .unwrap()["binding"]
            .as_str()
            .unwrap()
            .to_owned();
        let id = Uuid::new_v5(&Uuid::NAMESPACE_URL, b"progress-scope").to_string();
        store
            .conversation_input_capture(100, entry, &binding, &id, "Follow this work")
            .unwrap();
        {
            let conn = store.conn.lock().unwrap();
            super::super::conversation_inputs::select_return_on(&conn, 100, "worker-native", &id)
                .unwrap();
        }
        let local = store
            .interaction_instruction(&instruction.id, 100)
            .unwrap()
            .unwrap()
            .local_turn
            .unwrap();
        store
            .record_native_message(
                local,
                "worker-native",
                "physical",
                "web-progress",
                Some("commentary"),
                "Progress for the web",
            )
            .unwrap();
        {
            let conn = store.conn.lock().unwrap();
            conn.execute(
                "INSERT INTO original_ingress_order(channel,event_id) VALUES('telegram','10')",
                [],
            )
            .unwrap();
            super::super::conversation_inputs::select_return_on(
                &conn,
                100,
                "worker-native",
                "telegram:10",
            )
            .unwrap();
        }
        assert!(
            !store
                .record_native_message(
                    local,
                    "worker-native",
                    "physical",
                    "web-progress",
                    Some("commentary"),
                    "Progress for the web"
                )
                .unwrap()
        );
        store
            .record_native_message(
                local,
                "worker-native",
                "physical",
                "direct-progress",
                Some("commentary"),
                "Direct work stays direct",
            )
            .unwrap();
        let page = store
            .interaction_presentation_list(100, entry, 0, 0)
            .unwrap();
        let progress = page["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|s| s["kind"] == "worker_progress")
            .collect::<Vec<_>>();
        assert_eq!(progress.len(), 1);
        assert_eq!(progress[0]["preview"], "Progress for the web");
        assert_eq!(progress[0]["target"], json!(SessionKey::new(100, Some(7))));
        assert!(!page.to_string().contains("Direct work stays direct"));
        let echo = store.topic_publication_claim(entry, 100).unwrap().unwrap();
        assert_eq!(echo.kind, "entry_progress");
        assert!(!echo.direct);
        assert!(echo.body.ends_with("Progress for the web"));
        let parts = store
            .topic_publication_parts(echo.id, std::slice::from_ref(&echo.body))
            .unwrap();
        assert_eq!(parts.len(), 1);
        assert!(store.topic_publication_reserve(echo.id, 0).unwrap());
        store
            .topic_publication_receipt(echo.id, 0, Some(123), false)
            .unwrap();
        store.topic_publication_finish(echo.id).unwrap();
        assert!(store.topic_publication_claim(entry, 100).unwrap().is_none());
        let conn = store.conn.lock().unwrap();
        let direct: i64 = conn
            .query_row(
                "SELECT rowid FROM interaction_native_messages WHERE item_id='direct-progress'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(
            source_on(&conn, 100, entry, &format!("native:{direct}"))
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn canonical_result_parts_survive_reopen_without_mutating_delivery() {
        let (file, store, entry, _, _, body) = setup();
        let list = store
            .interaction_presentation_list(100, entry, 0, 0)
            .unwrap();
        let source = &list["items"][0];
        assert_eq!(source["kind"], "worker_result");
        assert!(source.get("text").is_none());
        assert_eq!(source["current"], true);
        let id = source["id"].as_str().unwrap().to_string();
        let revision = source["revision"].as_str().unwrap().to_string();
        let mut joined = String::new();
        let mut offset = 0;
        loop {
            let part = store
                .interaction_presentation_part(100, entry, &id, &revision, offset)
                .unwrap();
            assert_eq!(part["playback"], "not_observed");
            assert_eq!(part["delivery_phase"], "reserved");
            joined.push_str(part["text"].as_str().unwrap());
            let Some(next) = part["next_offset"].as_u64() else {
                break;
            };
            offset = next as usize;
        }
        assert_eq!(joined, body.trim());
        drop(store);
        let reopened = Store::open(file.path(), &[100], &super::super::tests::defaults()).unwrap();
        assert!(
            reopened
                .interaction_presentation_part(100, entry, &id, &revision, 0)
                .is_ok()
        );
        let cursor = list["cursor"]["delivery"].as_i64().unwrap();
        assert!(
            reopened
                .interaction_presentation_list(100, entry, cursor, 0)
                .unwrap()["items"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn correction_binding_and_owner_changes_invalidate_selected_revision() {
        let (_file, store, entry, target, instruction, _) = setup();
        let list = store
            .interaction_presentation_list(100, entry, 0, 0)
            .unwrap();
        let source = &list["items"][0];
        let id = source["id"].as_str().unwrap();
        let revision = source["revision"].as_str().unwrap();
        assert!(
            store
                .interaction_presentation_list(101, entry, 0, 0)
                .is_err()
        );
        let other = SessionKey::new(100, Some(9));
        store
            .ensure_session(other, 100, &super::super::tests::defaults())
            .unwrap();
        store.set_session_codex_thread(other, "other").unwrap();
        assert!(
            store
                .interaction_presentation_part(100, other, id, revision, 0)
                .is_err()
        );
        store
            .interaction_admit(
                &SourceInput {
                    transcription: None,
                    channel: "fixture".into(),
                    event_id: "2".into(),
                    update_id: None,
                    human_id: 100,
                    target_sessions: vec![target.id],
                    origin: ReturnRoute {
                        chat_id: 100,
                        thread_id: 1,
                        message_id: Some(2),
                    },
                    text: "Use updated values".into(),
                },
                target.key,
                RequestRelation::Amends(instruction.requests),
            )
            .unwrap();
        assert!(
            store
                .interaction_presentation_part(100, entry, id, revision, 0)
                .is_err()
        );
        let updated = store
            .interaction_presentation_list(100, entry, 0, 0)
            .unwrap();
        let changed = &updated["items"][0];
        assert_eq!(changed["correction_pending"], true);
        assert_eq!(changed["text_sha256"], source["text_sha256"]);
        let revision = changed["revision"].as_str().unwrap();
        store
            .set_session_codex_thread(target.key, "replacement")
            .unwrap();
        assert!(
            store
                .interaction_presentation_part(100, entry, id, revision, 0)
                .is_err()
        );
        store.upsert_user(100, UserRole::Admin, false).unwrap();
        assert!(
            store
                .interaction_presentation_list(100, entry, 0, 0)
                .is_err()
        );
    }

    #[test]
    fn entry_progress_and_answers_retire_with_entry_binding() {
        let (_file, store, entry, _, _, _) = setup();
        let session = store.get_session(entry).unwrap().unwrap();
        let local = store
            .record_turn_started(
                session.id,
                &TurnRequest {
                    session_key: entry,
                    from_user_id: 100,
                    prompt: "Status".into(),
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
        let original = "Alpha is still working.\n<daimon-result>{\"requests\":[],\"covered\":[],\"completed\":[],\"deferred\":[]}</daimon-result>";
        store
            .record_native_message(
                local,
                "entry-native",
                "entry-turn",
                "progress",
                Some("commentary"),
                "Inspecting",
            )
            .unwrap();
        store
            .record_native_message(
                local,
                "entry-native",
                "entry-turn",
                "answer",
                None,
                original,
            )
            .unwrap();
        let list = store
            .interaction_presentation_list(100, entry, 0, 0)
            .unwrap();
        assert_eq!(list["items"].as_array().unwrap().len(), 3);
        let progress = list["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["kind"] == "integrator_progress")
            .unwrap();
        assert_eq!(progress["preview"], "Inspecting");
        assert!(progress["observed_at"].is_string());
        let source = list["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["kind"] == "integrator_answer")
            .unwrap();
        assert!(source["current"].is_null());
        assert_eq!(source["preview"], "Alpha is still working.");
        assert_eq!(
            source["text_characters"],
            "Alpha is still working.".chars().count()
        );
        assert_eq!(
            source["text_sha256"],
            format!("{:x}", Sha256::digest(b"Alpha is still working."))
        );
        let id = source["id"].as_str().unwrap();
        let revision = source["revision"].as_str().unwrap();
        let part = store
            .interaction_presentation_part(100, entry, id, revision, 0)
            .unwrap();
        assert_eq!(part["text"], "Alpha is still working.");
        assert!(part["next_offset"].is_null());
        {
            let conn = store.conn.lock().unwrap();
            let retained: String = conn.query_row(
                "SELECT text FROM interaction_native_messages WHERE native_thread='entry-native' AND item_id='answer'",
                [], |r| r.get(0),
            ).unwrap();
            assert_eq!(retained, original);
        }
        store
            .set_session_codex_thread(entry, "replacement-entry")
            .unwrap();
        assert!(
            store
                .interaction_presentation_part(100, entry, id, revision, 0)
                .is_err()
        );
        assert_ne!(
            store
                .interaction_presentation_list(100, entry, 0, 0)
                .unwrap()["binding"],
            list["binding"]
        );
    }

    #[test]
    fn recent_page_and_older_history_retain_every_parallel_reply_without_duplicates() {
        let (_file, store, entry, _, _, _) = setup();
        let session = store.get_session(entry).unwrap().unwrap();
        let local = store
            .record_turn_started(
                session.id,
                &TurnRequest {
                    session_key: entry,
                    from_user_id: 100,
                    prompt: "Updates".into(),
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
        for index in 0..20 {
            store
                .record_native_message(
                    local,
                    "entry-native",
                    "entry-turn",
                    &format!("item-{index}"),
                    Some("final_answer"),
                    &format!("Reply {index}"),
                )
                .unwrap();
        }
        let mut page = store
            .interaction_presentation_page(100, entry, 0, 0, true)
            .unwrap();
        let first = page["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["kind"] == "integrator_answer")
            .unwrap();
        assert_eq!(first["preview"], "Reply 19");
        let newest = page["newest"]["native"].as_i64().unwrap();
        assert!(
            store
                .interaction_presentation_list(100, entry, 1, newest)
                .unwrap()["items"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        let mut seen = std::collections::HashSet::new();
        loop {
            for item in page["items"].as_array().unwrap() {
                assert!(seen.insert(item["id"].as_str().unwrap().to_string()));
            }
            if page["more"] != true {
                break;
            }
            let delivery = page["cursor"]["delivery"].as_i64().unwrap();
            let native = page["cursor"]["native"].as_i64().unwrap();
            page = store
                .interaction_presentation_page(100, entry, delivery, native, true)
                .unwrap();
        }
        assert_eq!(seen.len(), 21); // Twenty entry answers and the routed worker result.
        store
            .record_native_message(
                local,
                "entry-native",
                "entry-turn",
                "new-item",
                Some("final_answer"),
                "New reply",
            )
            .unwrap();
        let new = store
            .interaction_presentation_list(100, entry, 1, newest)
            .unwrap();
        assert_eq!(new["items"].as_array().unwrap().len(), 1);
        assert_eq!(new["items"][0]["preview"], "New reply");
    }
}
