//! Authenticated, journal-backed feedback; reactions never dispatch native work.
use super::*;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Deserialize)]
struct ReactionEvent {
    chat: crate::telegram::Chat,
    message_id: i64,
    user: Option<crate::telegram::User>,
    actor_chat: Option<Value>,
    date: i64,
    old_reaction: Vec<Reaction>,
    new_reaction: Vec<Reaction>,
}

#[derive(Deserialize, Serialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
enum Reaction {
    Emoji { emoji: String },
    CustomEmoji { custom_emoji_id: String },
    Paid,
}

impl Reaction {
    fn valid(&self) -> bool {
        match self {
            Self::Emoji { emoji } => !emoji.is_empty() && emoji.len() <= 128,
            Self::CustomEmoji { custom_emoji_id } => {
                !custom_emoji_id.is_empty() && custom_emoji_id.len() <= 128
            }
            Self::Paid => true,
        }
    }
}

/// Runs inside the journal/offset transaction. Invalid/unknown events remain raw receipts.
pub(super) fn admit(conn: &Connection, update: &crate::telegram::Update) -> Result<()> {
    let Some(raw) = update.message_reaction.as_ref() else {
        return Ok(());
    };
    let Ok(event) = serde_json::from_value::<ReactionEvent>(raw.clone()) else {
        return Ok(());
    };
    let Some(actor) = &event.user else {
        return Ok(());
    };
    if actor.is_bot
        || event.actor_chat.is_some()
        || event.date <= 0
        || event.message_id <= 0
        || !matches!(event.chat.kind.as_str(), "private" | "group" | "supergroup")
        || (event.chat.kind == "private" && event.chat.id != actor.id)
        || event.old_reaction == event.new_reaction
        || event.old_reaction.len() > 20
        || event.new_reaction.len() > 20
        || !event
            .old_reaction
            .iter()
            .chain(&event.new_reaction)
            .all(Reaction::valid)
    {
        return Ok(());
    }
    // Count every matching boundary before checking owner/native binding. A broken
    // or ambiguous local map must not be narrowed into a plausible destination.
    let mut stmt = conn.prepare("SELECT h.thread_id,h.turn_id,h.text,t.session_id,t.native_thread_id FROM handoff_messages h JOIN turns t ON t.id=h.turn_id WHERE h.chat_id=?1 AND h.message_id=?2")?;
    let targets = stmt
        .query_map(params![event.chat.id, event.message_id], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, Option<String>>(4)?,
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let [target] = targets.as_slice() else {
        return Ok(());
    };
    let (topic, target_turn, text, session, native) = target;
    let Some(native) = native.as_deref().filter(|id| !id.is_empty()) else {
        return Ok(());
    };
    let allowed: bool = conn.query_row("SELECT EXISTS(SELECT 1 FROM sessions s JOIN users u ON u.tg_user_id=?1 WHERE s.id=?2 AND s.chat_id=?3 AND s.thread_id=?4 AND s.creator_user_id=?1 AND u.allowed=1 AND s.codex_thread_id=?5 AND s.force_fresh_thread=0)",params![actor.id,session,event.chat.id,topic,native],|r|r.get(0))?;
    if !allowed {
        return Ok(());
    }
    let action = if event.new_reaction.is_empty() {
        "removed"
    } else if event.old_reaction.is_empty() {
        "added"
    } else {
        "changed"
    };
    let record = json!({
        "source":"telegram.message_reaction", "update_id":update.update_id,
        "actor_user_id":actor.id, "chat_id":event.chat.id, "topic_id":topic,
        "assistant_message_id":event.message_id, "assistant_turn_id":target_turn,
        "native_thread_id":native, "date":event.date, "action":action,
        "old_reaction":event.old_reaction, "new_reaction":event.new_reaction,
        "assistant_text_excerpt":text.chars().take(1000).collect::<String>(),
        "assistant_text_truncated":text.chars().count()>1000,
    });
    conn.execute("INSERT INTO reaction_feedback(update_id,session_id,actor_user_id,native_thread_id,record_json,phase) VALUES(?1,?2,?3,?4,?5,'pending')", params![update.update_id,session,actor.id,native,record.to_string()])?;
    Ok(())
}

impl Store {
    pub(super) fn init_reactions(&self) -> Result<()> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        conn.execute_batch("CREATE TABLE IF NOT EXISTS reaction_feedback(
            update_id INTEGER PRIMARY KEY REFERENCES incoming_updates(update_id) ON DELETE CASCADE,
            session_id INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
            actor_user_id INTEGER NOT NULL,
            native_thread_id TEXT NOT NULL,
            record_json TEXT NOT NULL,
            phase TEXT NOT NULL CHECK(phase IN ('pending','prepared','transmitted','accepted','unconfirmed')),
            delivery_turn INTEGER REFERENCES turns(id) ON DELETE SET NULL,
            delivery_instance TEXT
        );
        CREATE INDEX IF NOT EXISTS reaction_feedback_pending ON reaction_feedback(session_id,actor_user_id,native_thread_id,phase,update_id);")?;
        conn.execute("UPDATE reaction_feedback SET phase='pending',delivery_turn=NULL,delivery_instance=NULL WHERE phase='prepared' AND delivery_instance!=?1",[&self.instance_id])?;
        conn.execute("UPDATE reaction_feedback SET phase='unconfirmed' WHERE phase='transmitted' AND delivery_instance!=?1",[&self.instance_id])?;
        Ok(())
    }

    /// Reserve a bounded batch for an actual ordinary human turn, never a command/steer.
    pub fn prepare_reaction_feedback(
        &self,
        session: &SessionRecord,
        turn: i64,
        update: i64,
    ) -> Result<Option<String>> {
        let Some(native) = session
            .codex_thread_id
            .as_deref()
            .filter(|_| !session.force_fresh_thread)
        else {
            return Ok(None);
        };
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction()?;
        let raw: Option<String> = tx.query_row("SELECT i.payload_json FROM incoming_updates i JOIN turns t ON t.id=i.turn_id JOIN sessions s ON s.id=t.session_id JOIN users u ON u.tg_user_id=t.from_user_id WHERE i.update_id=?1 AND t.id=?2 AND s.id=?3 AND s.codex_thread_id=?4 AND s.force_fresh_thread=0 AND s.creator_user_id=t.from_user_id AND u.allowed=1", params![update,turn,session.id,native], |r|r.get(0)).optional()?;
        let Some(raw) = raw else {
            return Ok(None);
        };
        let input: Value = serde_json::from_str(&raw)?;
        let message = &input["message"];
        let Some(actor) = message["from"]["id"].as_i64() else {
            return Ok(None);
        };
        let text = message["text"]
            .as_str()
            .or_else(|| message["caption"].as_str())
            .unwrap_or("");
        if message["chat"]["id"].as_i64() != Some(session.key.chat_id)
            || message["message_thread_id"].as_i64().unwrap_or(0) != session.key.thread_id
            || message["from"]["is_bot"].as_bool() != Some(false)
            || text.trim().starts_with('/')
        {
            return Ok(None);
        }
        let candidates = {
            let mut stmt=tx.prepare("SELECT update_id,record_json FROM reaction_feedback WHERE session_id=?1 AND actor_user_id=?2 AND native_thread_id=?3 AND phase='pending' AND update_id<?4 ORDER BY update_id LIMIT 20")?;
            stmt.query_map(params![session.id, actor, native, update], |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?
        };
        let mut records = Vec::new();
        let mut bytes = 0;
        for (id, record) in candidates {
            if bytes + record.len() > 32_000 {
                break;
            }
            bytes += record.len();
            records.push(serde_json::from_str::<Value>(&record)?);
            tx.execute("UPDATE reaction_feedback SET phase='prepared',delivery_turn=?2,delivery_instance=?3 WHERE update_id=?1 AND phase='pending'",params![id,turn,self.instance_id])?;
        }
        tx.commit()?;
        if records.is_empty() {
            Ok(None)
        } else {
            Ok(Some(serde_json::to_string(&records)?))
        }
    }

    /// Persist uncertainty before native transport; revalidate against the actual resumed thread.
    pub fn reaction_feedback_submitting(&self, turn: i64) -> Result<()> {
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction()?;
        let invalid: bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM reaction_feedback f JOIN turns t ON t.id=f.delivery_turn JOIN sessions s ON s.id=t.session_id LEFT JOIN users u ON u.tg_user_id=f.actor_user_id WHERE f.delivery_turn=?1 AND f.phase='prepared' AND (f.delivery_instance!=?2 OR u.allowed IS NOT 1 OR s.creator_user_id!=f.actor_user_id OR t.from_user_id!=f.actor_user_id OR s.id!=f.session_id OR s.force_fresh_thread!=0 OR s.codex_thread_id IS NOT f.native_thread_id OR t.native_thread_id IS NOT f.native_thread_id))",params![turn,self.instance_id],|r|r.get(0))?;
        if invalid {
            anyhow::bail!("Reaction feedback actor or native binding changed before submission");
        }
        tx.execute("UPDATE reaction_feedback SET phase='transmitted' WHERE delivery_turn=?1 AND phase='prepared' AND delivery_instance=?2",params![turn,self.instance_id])?;
        tx.commit()?;
        Ok(())
    }

    pub fn reaction_feedback_receipt(&self, turn: i64, phase: &str) -> Result<()> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        match phase {
            "accepted" => {
                conn.execute("UPDATE reaction_feedback SET phase='accepted' WHERE delivery_turn=?1 AND phase='transmitted' AND delivery_instance=?2",params![turn,self.instance_id])?;
            }
            "rejected" => {
                conn.execute("UPDATE reaction_feedback SET phase='pending',delivery_turn=NULL,delivery_instance=NULL WHERE delivery_turn=?1 AND phase='transmitted' AND delivery_instance=?2",params![turn,self.instance_id])?;
            }
            _ => anyhow::bail!("Unsupported reaction receipt"),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    fn defaults() -> SessionDefaults {
        SessionDefaults {
            cwd: std::env::temp_dir(),
            model: None,
            reasoning_effort: None,
            session_prompt: None,
            sandbox_mode: "read-only".into(),
            approval_policy: "never".into(),
            search_mode: SearchMode::Disabled,
            add_dirs: vec![],
        }
    }

    fn request(key: SessionKey, text: &str) -> TurnRequest {
        TurnRequest {
            session_key: key,
            from_user_id: 100,
            prompt: text.into(),
            runtime_instructions: None,
            attachments: vec![],
            review_mode: None,
            override_search_mode: None,
        }
    }

    fn setup() -> (NamedTempFile, Store, SessionRecord) {
        let file = NamedTempFile::new().unwrap();
        let store = Store::open(file.path(), &[100, 101], &defaults()).unwrap();
        let key = SessionKey::new(100, Some(7));
        let session = store.ensure_session(key, 100, &defaults()).unwrap();
        store.set_session_codex_thread(key, "native-a").unwrap();
        let turn = store
            .record_turn_started(session.id, &request(key, "Earlier human request"))
            .unwrap();
        store.record_turn_native_thread(turn, "native-a").unwrap();
        store
            .record_turn_finished(turn, "completed", Some("Earlier assistant answer"))
            .unwrap();
        store
            .record_handoff_message(key, turn, 500, "Earlier assistant answer")
            .unwrap();
        let session = store.get_session(key).unwrap().unwrap();
        (file, store, session)
    }

    fn event(id: i64) -> crate::telegram::Update {
        serde_json::from_value(json!({"update_id":id,"message_reaction":{
            "chat":{"id":100,"type":"private"},"message_id":500,"date":1791756000,
            "user":{"id":100,"first_name":"Human","is_bot":false},
            "old_reaction":[],"new_reaction":[{"type":"emoji","emoji":"👍"}]}}))
        .unwrap()
    }

    fn turn(store: &Store, session: &SessionRecord, id: i64, text: &str) -> i64 {
        let input=serde_json::from_value(json!({"update_id":id,"message":{
            "message_id":id,"message_thread_id":session.key.thread_id,"chat":{"id":session.key.chat_id,"type":"private"},
            "from":{"id":100,"is_bot":false,"first_name":"Human"},"text":text}})).unwrap();
        store.admit_update(&input).unwrap();
        store.begin_update(id).unwrap();
        store.mark_update_queued(id).unwrap();
        let local = store
            .record_turn_started(session.id, &request(session.key, text))
            .unwrap();
        store.link_update_turn(id, local).unwrap();
        store.record_turn_native_thread(local, "native-a").unwrap();
        local
    }

    fn phases(store: &Store) -> Vec<String> {
        let conn = store.conn.lock().unwrap();
        conn.prepare("SELECT phase FROM reaction_feedback ORDER BY update_id")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<std::result::Result<_, _>>()
            .unwrap()
    }

    #[test]
    fn projection_is_atomic_with_journal_and_offset_and_deduplicates() {
        let (_file, store, session) = setup();
        store.conn.lock().unwrap().execute_batch("CREATE TRIGGER refuse_reaction BEFORE INSERT ON reaction_feedback BEGIN SELECT RAISE(ABORT,'fault');END;").unwrap();
        assert!(store.admit_update_with_reactions(&event(10), true).is_err());
        assert_eq!(store.last_update_id().unwrap(), None);
        assert_eq!(
            store
                .conn
                .lock()
                .unwrap()
                .query_row("SELECT COUNT(*) FROM incoming_updates", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
        store
            .conn
            .lock()
            .unwrap()
            .execute_batch("DROP TRIGGER refuse_reaction;")
            .unwrap();
        assert!(store.admit_update_with_reactions(&event(10), true).unwrap());
        assert!(!store.admit_update_with_reactions(&event(10), true).unwrap());
        let mut conflict = event(10);
        conflict.message_reaction.as_mut().unwrap()["date"] = json!(1791756001);
        assert!(store.admit_update_with_reactions(&conflict, true).is_err());
        let local = turn(&store, &session, 11, "Next human message");
        let packet: Value = serde_json::from_str(
            &store
                .prepare_reaction_feedback(&session, local, 11)
                .unwrap()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(packet[0]["assistant_message_id"], 500);
        assert_eq!(packet[0]["actor_user_id"], 100);
        assert_eq!(packet[0]["native_thread_id"], "native-a");
        assert_eq!(
            packet[0]["assistant_text_excerpt"],
            "Earlier assistant answer"
        );
        assert_eq!(phases(&store), vec!["prepared"]);
    }

    #[test]
    fn rejects_unknown_anonymous_disallowed_foreign_or_rebound_targets() {
        let (_file, store, session) = setup();
        for id in 1..=7 {
            let mut input = event(id);
            let event = input.message_reaction.as_mut().unwrap();
            match id {
                1 => event["user"]["id"] = json!(101),
                2 => event["user"]["id"] = json!(999),
                3 => {
                    event.as_object_mut().unwrap().remove("user");
                    event["actor_chat"] = json!({"id":-10,"type":"channel"});
                }
                4 => event["message_id"] = json!(999),
                5 => event["chat"]["id"] = json!(200),
                6 => event["user"]["is_bot"] = json!(true),
                _ => event["new_reaction"] = json!([{"type":"future-kind"}]),
            }
            store.admit_update_with_reactions(&input, true).unwrap();
        }
        store
            .set_session_codex_thread(session.key, "native-b")
            .unwrap();
        store.admit_update_with_reactions(&event(8), true).unwrap();
        assert!(phases(&store).is_empty());
        store
            .set_session_codex_thread(session.key, "native-a")
            .unwrap();
        store.admit_update_with_reactions(&event(9), false).unwrap();
        assert!(phases(&store).is_empty());
        // Even a corrupt duplicate map must not be resolved by an owner/native guess.
        store
            .record_handoff_message(SessionKey::new(100, Some(8)), 1, 500, "Ambiguous")
            .unwrap();
        store.admit_update_with_reactions(&event(10), true).unwrap();
        assert!(phases(&store).is_empty());
    }

    #[test]
    fn pending_feedback_survives_restart_but_uncertain_submission_is_never_replayed() {
        let (file, store, session) = setup();
        store.admit_update_with_reactions(&event(10), true).unwrap();
        let local = turn(&store, &session, 11, "First message");
        store
            .prepare_reaction_feedback(&session, local, 11)
            .unwrap()
            .unwrap();
        drop(store);
        let store = Store::open(file.path(), &[100], &defaults()).unwrap();
        assert_eq!(phases(&store), vec!["pending"]);
        let local = turn(&store, &session, 12, "Later human message");
        store
            .prepare_reaction_feedback(&session, local, 12)
            .unwrap()
            .unwrap();
        store.reaction_feedback_submitting(local).unwrap();
        drop(store);
        let store = Store::open(file.path(), &[100], &defaults()).unwrap();
        assert_eq!(phases(&store), vec!["unconfirmed"]);
        let local = turn(&store, &session, 13, "Another human message");
        assert!(
            store
                .prepare_reaction_feedback(&session, local, 13)
                .unwrap()
                .is_none()
        );
        assert!(!store.admit_update_with_reactions(&event(10), true).unwrap());
    }

    #[test]
    fn commands_other_topics_and_changed_binding_do_not_consume_feedback() {
        let (_file, store, session) = setup();
        store.admit_update_with_reactions(&event(10), true).unwrap();
        let local = turn(&store, &session, 11, " /goal resume");
        assert!(
            store
                .prepare_reaction_feedback(&session, local, 11)
                .unwrap()
                .is_none()
        );
        let other = store
            .ensure_session(SessionKey::new(100, Some(8)), 100, &defaults())
            .unwrap();
        let local = turn(&store, &other, 12, "Other topic");
        assert!(
            store
                .prepare_reaction_feedback(&other, local, 12)
                .unwrap()
                .is_none()
        );
        let local = turn(&store, &session, 13, "Same topic");
        store
            .prepare_reaction_feedback(&session, local, 13)
            .unwrap()
            .unwrap();
        store
            .set_session_codex_thread(session.key, "native-b")
            .unwrap();
        assert!(store.reaction_feedback_submitting(local).is_err());
        store.record_turn_finished(local, "failed", None).unwrap();
        assert_eq!(phases(&store), vec!["pending"]);
        let rebound = store.get_session(session.key).unwrap().unwrap();
        let local = turn(&store, &rebound, 14, "New native session");
        assert!(
            store
                .prepare_reaction_feedback(&rebound, local, 14)
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn native_acceptance_is_once_only_and_explicit_rejection_releases_feedback() {
        let (_file, store, session) = setup();
        store.admit_update_with_reactions(&event(10), true).unwrap();
        let local = turn(&store, &session, 11, "Try once");
        store
            .prepare_reaction_feedback(&session, local, 11)
            .unwrap()
            .unwrap();
        store.reaction_feedback_submitting(local).unwrap();
        store.reaction_feedback_receipt(local, "rejected").unwrap();
        assert_eq!(phases(&store), vec!["pending"]);
        let next = turn(&store, &session, 12, "A fresh human request");
        store
            .prepare_reaction_feedback(&session, next, 12)
            .unwrap()
            .unwrap();
        store.reaction_feedback_submitting(next).unwrap();
        store.reaction_feedback_receipt(next, "accepted").unwrap();
        store.reaction_feedback_receipt(next, "rejected").unwrap();
        assert_eq!(phases(&store), vec!["accepted"]);
        let later = turn(&store, &session, 13, "Continue");
        assert!(
            store
                .prepare_reaction_feedback(&session, later, 13)
                .unwrap()
                .is_none()
        );
    }
}
