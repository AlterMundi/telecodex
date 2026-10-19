//! Read-only canonical sources for receiving text/audio presentation.
use super::*;
use anyhow::bail;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

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
            AND s.creator_user_id=?2 AND json_extract(d.doc,'$.origin.chat_id')=?3 AND json_extract(d.doc,'$.origin.thread_id')=?4
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
            JOIN turns t ON t.id=m.local_turn WHERE m.rowid{comparison}?1 AND t.session_id=?2
            AND t.from_user_id=?3 AND m.native_thread=?4 AND m.phase IN ('commentary','final_answer')
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
            "binding":entry_binding(&conn,human,entry,session)?,"items":items,
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
        let found:Option<(String,String,String,String,String)>=conn.query_row(
            "SELECT m.text,m.native_turn,m.item_id,m.phase,m.observed_at FROM interaction_native_messages m JOIN turns t ON t.id=m.local_turn
             WHERE m.rowid=?1 AND t.session_id=?2 AND t.from_user_id=?3 AND m.native_thread=?4 AND m.phase IN ('commentary','final_answer')",
             params![row,entry_session,human,entry_native],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?))).optional()?;
        let Some((text, turn, item, phase, observed_at)) = found else {
            return Ok(None);
        };
        json!({"id":id,"kind":if phase == "commentary" {"integrator_progress"} else {"integrator_answer"},"text":text,"native_turn":turn,"native_item":item,"observed_at":observed_at,
            "current":Value::Null,"correction_pending":Value::Null,"completed":Value::Null,
            "native_binding_current":true})
    } else {
        return Ok(None);
    };
    if let Some(delivery) = id.strip_prefix("delivery:") {
        let observed: Option<String> = conn.query_row(
            "SELECT m.observed_at FROM interaction_deliveries d JOIN interaction_results r ON r.id=d.result_id JOIN interaction_native_messages m ON m.native_thread=r.native_thread AND m.native_turn=r.native_turn AND m.item_id=r.item_id WHERE d.id=?1",
            [delivery], |r| r.get(0)).optional()?;
        source["observed_at"] = json!(observed);
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
    Ok(Some(source))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interaction::{RequestRelation, ReturnRoute, SourceInput, TransportPhase};

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
                Some("final_answer"),
                "Alpha is still working.",
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
        let id = source["id"].as_str().unwrap();
        let revision = source["revision"].as_str().unwrap();
        let part = store
            .interaction_presentation_part(100, entry, id, revision, 0)
            .unwrap();
        assert_eq!(part["text"], "Alpha is still working.");
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
