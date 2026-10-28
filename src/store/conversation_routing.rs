//! Explicit human selection reuses the existing immutable routing instructions.
use super::*;
use anyhow::bail;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConversationReply {
    pub source: String,
    pub revision: String,
    #[serde(default, skip_serializing_if = "is_false")]
    pub context_only: bool,
}
fn is_false(value: &bool) -> bool {
    !value
}
impl ConversationReply {
    pub(super) fn validate_context(&self) -> Result<()> {
        let question = self
            .source
            .strip_prefix("request:")
            .or_else(|| self.source.strip_prefix("task:"))
            .is_some_and(|id| {
                id.len() == 32
                    && id
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            });
        let delivery = self
            .source
            .strip_prefix("delivery:")
            .or_else(|| self.source.strip_prefix("input:"))
            .or_else(|| self.source.strip_prefix("matrix:"))
            .is_some_and(|id| Uuid::parse_str(id).is_ok_and(|uuid| uuid.to_string() == id));
        let native = self.context_only
            && self.source.strip_prefix("native:").is_some_and(|id| {
                id.parse::<i64>()
                    .is_ok_and(|n| n > 0 && id == n.to_string())
            });
        if !(question || delivery || native)
            || self.revision.len() != 64
            || !self
                .revision
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            bail!("Invalid reported conversation context");
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationFocus {
    pub target: SessionKey,
    pub session: i64,
    pub native: Option<String>,
    #[serde(default)]
    pub retired: bool,
    pub requests: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationRouting {
    pub target: SessionKey,
    pub label: String,
    pub session: i64,
    pub selection: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selection_input: Option<crate::interaction::InputReference>,
    pub instruction: Option<String>,
    pub phase: String,
}
pub enum ConversationRouteAction {
    Worker(Box<crate::interaction::Instruction>, bool),
    Interrupt(SessionKey),
}
fn digest(value: Value) -> String {
    format!("{:x}", Sha256::digest(value.to_string().as_bytes()))
}
fn input_focus_on(
    conn: &Connection,
    human: i64,
    entry: SessionKey,
    input: &ConversationInput,
) -> Result<Option<(ConversationReply, ConversationFocus)>> {
    let Some(instruction) = input.routing.as_ref().and_then(|r| r.instruction.as_ref()) else {
        return Ok(None);
    };
    let (session, native) = super::presentation::entry_on(conn, human, entry)?;
    if input.retired || input.entry != entry || input.session != session || input.native != native {
        return Ok(None);
    }
    let row:Option<(String,i64)>=conn.query_row("SELECT i.doc,s.id FROM interaction_instructions i JOIN interaction_sources x ON x.id=i.input_id JOIN sessions s ON s.id=i.session_id JOIN users u ON u.tg_user_id=s.creator_user_id WHERE i.id=?1 AND x.human_id=?2 AND x.channel='conversation' AND x.event_id=?3 AND s.creator_user_id=?2 AND u.allowed=1",params![instruction,human,input.id],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
    let Some((doc, target_session)) = row else {
        return Ok(None);
    };
    let i: crate::interaction::Instruction = serde_json::from_str(&doc)?;
    let focus = ConversationFocus {
        target: i.target,
        session: target_session,
        retired: false,
        native: conn.query_row(
            "SELECT codex_thread_id FROM sessions WHERE id=?1",
            [target_session],
            |r| r.get(0),
        )?,
        requests: i.requests,
    };
    Ok(Some((
        ConversationReply {
            context_only: false,
            source: format!("input:{}", input.id),
            revision: digest(json!([
                human,
                entry,
                session,
                super::presentation::entry_binding(conn, human, entry, session)?,
                input.id,
                instruction,
                focus.session,
                conn.query_row(
                    "SELECT generation FROM conversation_entry_generations WHERE session_id=?1",
                    [focus.session],
                    |r| r.get::<_, String>(0)
                )?,
                focus.requests
            ])),
        },
        focus,
    )))
}
pub(super) fn continuation_on(
    conn: &Connection,
    human: i64,
    entry: SessionKey,
    input: &ConversationInput,
) -> Result<Option<ConversationReply>> {
    Ok(input_focus_on(conn, human, entry, input)?.map(|(reply, _)| reply))
}
pub(super) fn reply_on(
    conn: &Connection,
    human: i64,
    entry: SessionKey,
    reply: &ConversationReply,
) -> Result<ConversationFocus> {
    if reply.revision.len() != 64
        || !reply
            .revision
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        bail!("Invalid reply source revision");
    }
    if let Some(id) = reply.source.strip_prefix("request:") {
        let snapshot = super::conversation_requests::read_on(conn, human, entry, id)?;
        if snapshot.item.source["kind"] != "question"
            || (!reply.context_only && snapshot.item.state != "pending")
        {
            bail!("Select a current native question");
        }
        return Ok(ConversationFocus {
            target: snapshot.key,
            session: conn.query_row(
                "SELECT id FROM sessions WHERE chat_id=?1 AND thread_id=?2",
                params![snapshot.key.chat_id, snapshot.key.thread_id],
                |r| r.get(0),
            )?,
            native: Some(snapshot.item.native_thread),
            retired: false,
            requests: vec![],
        });
    }
    if let Some(id) = reply.source.strip_prefix("input:") {
        let original = super::conversation_inputs::read_on(conn, id, human)?;
        let (expected, focus) = input_focus_on(conn, human, entry, &original)?
            .context("Original input cannot be continued")?;
        if expected != *reply {
            bail!("Selected original input changed");
        }
        return Ok(focus);
    }
    let source = super::presentation::source_on(conn, human, entry, &reply.source)?
        .context("Reply source unavailable")?;
    if source["kind"] != "worker_result"
        || (!reply.context_only && source["revision"] != reply.revision)
    {
        bail!("Select the current worker result");
    }
    let request = source["request_id"]
        .as_str()
        .context("Reply request missing")?;
    let target: SessionKey = serde_json::from_value(source["target"].clone())?;
    let session=conn.query_row("SELECT r.session_id FROM interaction_requests r JOIN sessions s ON s.id=r.session_id WHERE r.id=?1 AND r.human_id=?2 AND s.creator_user_id=?2 AND s.chat_id=?3 AND s.thread_id=?4",params![request,human,target.chat_id,target.thread_id],|r|r.get(0))?;
    Ok(ConversationFocus {
        target,
        session,
        retired: false,
        native: conn.query_row(
            "SELECT codex_thread_id FROM sessions WHERE id=?1",
            [session],
            |r| r.get(0),
        )?,
        requests: serde_json::from_value(source["request_ids"].clone())?,
    })
}
fn save_on(conn: &Connection, input: &ConversationInput) -> Result<()> {
    conn.execute(
        "UPDATE conversation_inputs SET phase=?1,doc=?2 WHERE id=?3 AND human_id=?4",
        params![
            input.phase,
            serde_json::to_string(input)?,
            input.id,
            input.human
        ],
    )?;
    Ok(())
}
impl Store {
    /// Only current originals expose their exact retained, definitely-unsent routes.
    pub fn conversation_recoverable_routes(
        &self,
        human: i64,
        entry: SessionKey,
        id: &str,
    ) -> Result<Vec<crate::interaction::Instruction>> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let input = super::conversation_inputs::read_on(&conn, id, human)?;
        if input.entry != entry || super::conversation_inputs::eligible_on(&conn, &input).is_err() {
            return Ok(vec![]);
        }
        let mut stmt = conn.prepare("SELECT i.doc FROM interaction_instructions i JOIN interaction_sources source ON source.id=i.input_id JOIN sessions target ON target.id=i.session_id JOIN users u ON u.tg_user_id=target.creator_user_id WHERE source.channel='conversation' AND source.event_id=?1 AND source.human_id=?2 AND target.creator_user_id=?2 AND u.allowed=1 AND i.phase='retained_unsent' ORDER BY i.rowid")?;
        let rows = stmt
            .query_map(params![id, human], |r| r.get::<_, String>(0))?
            .map(|r| {
                Ok(serde_json::from_str::<crate::interaction::Instruction>(
                    &r?,
                )?)
            })
            .collect::<Result<Vec<_>>>()?;
        let mut available = vec![];
        for route in rows {
            if !super::stopping::fenced_on(&conn, route.target)?
                && !super::maintenance_writer_held_on(&conn, route.target)?
            {
                available.push(route);
            }
        }
        Ok(available)
    }

    /// Explicit authenticated recovery uses the same original/instruction identity.
    /// The current entry and selected source are checked in the transition transaction.
    pub fn conversation_recover_route(
        &self,
        human: i64,
        entry: SessionKey,
        id: &str,
        instruction: &str,
    ) -> Result<(bool, SessionKey)> {
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction()?;
        let input = super::conversation_inputs::read_on(&tx, id, human)?;
        if input.entry != entry {
            bail!("Recovery belongs to another entry");
        }
        super::conversation_inputs::eligible_on(&tx, &input)?;
        let raw:String=tx.query_row("SELECT i.doc FROM interaction_instructions i JOIN interaction_sources source ON source.id=i.input_id WHERE i.id=?1 AND source.channel='conversation' AND source.event_id=?2 AND source.human_id=?3",params![instruction,id,human],|r|r.get(0)).context("Recovery does not match this original route")?;
        let selected: crate::interaction::Instruction = serde_json::from_str(&raw)?;
        if super::stopping::fenced_on(&tx, selected.target)?
            || super::maintenance_writer_held_on(&tx, selected.target)?
        {
            bail!("Selected work session is held");
        }
        let changed = super::interaction::recover_on(&tx, instruction, human, &self.instance_id)?;
        if changed {
            tx.execute("INSERT INTO audit_log(actor_user_id,action,details_json,created_at) VALUES(?1,'conversation_route_recovery',?2,?3)",params![human,json!({"original":id,"instruction":instruction,"entry":entry,"target":selected.target,"kind":"explicit_known_unsent_recovery"}).to_string(),now_string()])?;
        }
        tx.commit()?;
        Ok((changed, selected.target))
    }

    pub fn conversation_continuation(
        &self,
        human: i64,
        entry: SessionKey,
        id: &str,
    ) -> Result<Option<ConversationReply>> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let input = super::conversation_inputs::read_on(&conn, id, human)?;
        continuation_on(&conn, human, entry, &input)
    }
    pub fn conversation_route(
        &self,
        human: i64,
        id: &str,
        profile: &crate::interaction::InteractionConfig,
        resolve: Option<&str>,
    ) -> Result<Option<ConversationRouteAction>> {
        self.conversation_route_selected(human, id, profile, resolve, None)
    }
    pub fn conversation_route_clarified(
        &self,
        human: i64,
        id: &str,
        profile: &crate::interaction::InteractionConfig,
        selector: &str,
        clarification: (
            &str,
            &crate::interaction::InputReference,
            &tokio_util::sync::CancellationToken,
        ),
    ) -> Result<Option<ConversationRouteAction>> {
        self.conversation_route_selected(human, id, profile, Some(selector), Some(clarification))
    }
    fn conversation_route_selected(
        &self,
        human: i64,
        id: &str,
        profile: &crate::interaction::InteractionConfig,
        resolve: Option<&str>,
        clarification: Option<(
            &str,
            &crate::interaction::InputReference,
            &tokio_util::sync::CancellationToken,
        )>,
    ) -> Result<Option<ConversationRouteAction>> {
        use crate::interaction::{RequestRelation, ReturnRoute, SourceInput};
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction()?;
        if clarification.is_some_and(|(_, _, closed)| closed.is_cancelled()) {
            bail!("Clarification request closed before admission");
        }
        let mut input = super::conversation_inputs::read_on(&tx, id, human)?;
        super::conversation_inputs::eligible_on(&tx, &input)?;
        if input.entry != profile.entry() || profile.human_id != human {
            bail!("Routing profile mismatch");
        }
        let selection_input = if let Some((native, reference, _)) = clarification {
            // Resolve from a later actual human utterance, never model-authored
            // replacement text or an earlier statement reused as fresh consent.
            if input.native != native {
                bail!("Pending input belongs to another entry generation");
            }
            let (source, _) = super::dialogue::original_on(
                &tx,
                input.entry,
                human,
                native,
                reference,
                "",
                false,
            )?;
            let selected_order = super::dialogue::original_order_on(&tx, &source)?
                .context("Clarification capture order unavailable")?;
            let pending_order:i64 = tx.query_row("SELECT id FROM original_ingress_order WHERE channel='conversation' AND event_id=?1",[&input.id],|r|r.get(0))?;
            if selected_order <= pending_order {
                bail!("Clarification must follow the pending input");
            }
            Some(reference.clone())
        } else {
            None
        };
        if matches!(input.phase.as_str(), "routed" | "interruption") {
            if selection_input.is_some()
                && input
                    .routing
                    .as_ref()
                    .and_then(|r| r.selection_input.as_ref())
                    != selection_input.as_ref()
            {
                bail!("A retained clarification cannot change");
            }
            if let Some(selector) = resolve {
                let selected = profile.resolve(selector.strip_prefix('#').unwrap_or(selector));
                if selected.len() != 1
                    || input.routing.as_ref().map(|r| r.target) != Some(selected[0])
                {
                    bail!("A retained destination cannot change");
                }
            }
            return Ok(None);
        }
        if (resolve.is_some() && input.phase != "awaiting_target")
            || (resolve.is_none() && input.phase != "retained")
        {
            bail!("Input no longer accepts routing selection");
        }
        let explicit = crate::interaction::explicit_selector(&input.text);
        let (targets, body) = if let Some(selector) = resolve {
            (
                profile.resolve(selector.strip_prefix('#').unwrap_or(selector)),
                explicit.map_or(input.text.as_str(), |(_, body)| body),
            )
        } else if let Some((selector, body)) = explicit {
            (profile.resolve(selector), body)
        } else if let Some(focus) = input
            .focus
            .as_ref()
            .filter(|_| !input.reply.as_ref().is_some_and(|r| r.context_only))
        {
            (vec![focus.target], input.text.as_str())
        } else {
            return Ok(None);
        };
        if targets.len() != 1 {
            if clarification.is_some() {
                bail!("Select one unambiguous enrolled clarification destination");
            }
            input.phase = "awaiting_target".into();
            save_on(&tx, &input)?;
            if clarification.is_some_and(|(_, _, closed)| closed.is_cancelled()) {
                bail!("Clarification request closed before commit");
            }
            tx.commit()?;
            return Ok(None);
        }
        let target = targets[0];
        if profile.target(target.thread_id) != Some(target) {
            bail!("Target is not enrolled");
        }
        let label = profile
            .targets
            .iter()
            .find(|t| t.topic_id == target.thread_id)
            .context("Missing target label")?
            .name
            .clone();
        if body.trim().is_empty() {
            bail!("Write a message after the selector");
        }
        if body.starts_with('/') && body != "/interrupt" {
            bail!("Only scoped /interrupt is supported after a selector");
        }
        let target_session:i64=tx.query_row("SELECT s.id FROM sessions s JOIN users u ON u.tg_user_id=s.creator_user_id WHERE s.chat_id=?1 AND s.thread_id=?2 AND s.creator_user_id=?3 AND u.allowed=1",params![target.chat_id,target.thread_id,human],|r|r.get(0))?;
        let inherited = input
            .focus
            .as_ref()
            .filter(|f| f.target == target && !f.requests.is_empty());
        if let Some(focus) = inherited {
            let (native, fresh): (Option<String>, bool) = tx.query_row(
                "SELECT codex_thread_id,force_fresh_thread FROM sessions WHERE id=?1",
                [target_session],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            if focus.retired
                || focus.session != target_session
                || (fresh && focus.native.is_some())
                || focus
                    .native
                    .as_ref()
                    .is_some_and(|n| Some(n) != native.as_ref())
            {
                bail!("Selected work session changed during input preparation");
            }
        }
        if body == "/interrupt" {
            input.phase = "interruption".into();
            input.routing = Some(ConversationRouting {
                target,
                label,
                session: target_session,
                selection: if selection_input.is_some() {
                    "integrator_clarification"
                } else if resolve.is_some() {
                    "human_clarification"
                } else if explicit.is_some() {
                    "explicit_selector"
                } else {
                    "reply"
                }
                .into(),
                selection_input,
                instruction: None,
                phase: "unconfirmed".into(),
            });
            save_on(&tx, &input)?;
            if clarification.is_some_and(|(_, _, closed)| closed.is_cancelled()) {
                bail!("Clarification request closed before commit");
            }
            tx.commit()?;
            return Ok(Some(ConversationRouteAction::Interrupt(target)));
        }
        let source = SourceInput {
            transcription: input.voice.as_ref().and_then(|v| v.transcription.clone()),
            channel: "conversation".into(),
            event_id: input.id.clone(),
            update_id: None,
            human_id: human,
            target_sessions: vec![target_session],
            origin: ReturnRoute {
                chat_id: input.entry.chat_id,
                thread_id: input.entry.thread_id,
                message_id: None,
            },
            text: input.text.clone(),
        };
        let relation = inherited
            .map(|f| RequestRelation::Amends(f.requests.clone()))
            .unwrap_or(RequestRelation::New);
        let (instruction, fresh) =
            super::interaction::admit_on(&tx, &source, target, relation, &self.instance_id)?;
        input.phase = "routed".into();
        input.routing = Some(ConversationRouting {
            target,
            label,
            session: target_session,
            selection: if selection_input.is_some() {
                "integrator_clarification"
            } else if resolve.is_some() {
                "human_clarification"
            } else if explicit.is_some() {
                "explicit_selector"
            } else {
                "reply"
            }
            .into(),
            selection_input,
            instruction: Some(instruction.id.clone()),
            phase: instruction.phase.as_str().into(),
        });
        save_on(&tx, &input)?;
        if clarification.is_some_and(|(_, _, closed)| closed.is_cancelled()) {
            bail!("Clarification request closed before commit");
        }
        tx.commit()?;
        Ok(Some(ConversationRouteAction::Worker(
            Box::new(instruction),
            fresh,
        )))
    }
    pub fn conversation_pending_routes(
        &self,
        human: i64,
        entry: SessionKey,
        native: &str,
    ) -> Result<Value> {
        let conn = self.conn.lock().expect("store mutex poisoned");
        let (_, current) = super::presentation::entry_on(&conn, human, entry)?;
        if current != native {
            bail!("Entry native generation changed");
        }
        let epoch = super::stopping::admission_epoch_on(&conn, entry)?;
        let mut query=conn.prepare("SELECT id FROM conversation_inputs WHERE human_id=?1 AND session_id=(SELECT id FROM sessions WHERE chat_id=?2 AND thread_id=?3) AND native_thread=?4 AND phase='awaiting_target' AND retired=0 AND json_extract(doc,'$.epoch')=?5 ORDER BY created_at DESC,id LIMIT 13")?;
        let ids = query
            .query_map(
                params![human, entry.chat_id, entry.thread_id, native, epoch],
                |r| r.get::<_, String>(0),
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let more = ids.len() > 12;
        let mut items = Vec::new();
        for id in ids.into_iter().take(12) {
            let input = super::conversation_inputs::read_on(&conn, &id, human)?;
            super::conversation_inputs::eligible_on(&conn, &input)?;
            items.push(json!({"id":input.id,"text_excerpt":input.text.chars().take(2000).collect::<String>(),"excerpt_bounded":true,"voice":input.voice,"phase":input.phase}));
        }
        Ok(
            json!({"kind":"pending_route_selections","items":items,"more":more,"authority":"Retained original inputs awaiting a destination, not permission to choose without a later human clarification"}),
        )
    }
    pub fn conversation_interruption_apply(
        &self,
        human: i64,
        id: &str,
        apply: impl FnOnce(SessionKey) -> Option<bool>,
    ) -> Result<Option<bool>> {
        let mut conn = self.conn.lock().expect("store mutex poisoned");
        let tx = conn.transaction()?;
        let mut input = super::conversation_inputs::read_on(&tx, id, human)?;
        super::conversation_inputs::eligible_on(&tx, &input)?;
        if input.phase != "interruption" {
            bail!("Not a retained scoped interruption");
        }
        let route = input
            .routing
            .as_mut()
            .context("Missing interruption target")?;
        if route.instruction.is_some() || route.phase != "unconfirmed" {
            bail!("Interruption already settled");
        }
        let owned:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM sessions s JOIN users u ON u.tg_user_id=s.creator_user_id WHERE s.id=?1 AND s.chat_id=?2 AND s.thread_id=?3 AND s.creator_user_id=?4 AND u.allowed=1)",params![route.session,route.target.chat_id,route.target.thread_id,human],|r|r.get(0))?;
        if !owned {
            bail!("Interruption target ownership changed");
        }
        // No await or native lookup is allowed between authority and token use.
        // A failed commit leaves the retained outcome unconfirmed, never replayable.
        let requested = apply(route.target);
        route.phase = match requested {
            Some(true) => "requested",
            Some(false) => "no_active_turn",
            None => "scope_unavailable",
        }
        .into();
        save_on(&tx, &input)?;
        tx.commit()?;
        Ok(requested)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interaction::{InputReference, InteractionConfig, RequestRelation};
    #[test]
    fn integrator_message_reference_is_compact_context_without_a_worker_target() {
        let (_file, store, profile, binding) = setup();
        let reference = ConversationReply {
            source: "native:42".into(),
            revision: "a".repeat(64),
            context_only: true,
        };
        assert!(reference.validate_context().is_ok());
        let id = capture(
            &store,
            &profile,
            &binding,
            "About that earlier answer",
            Some(reference.clone()),
        );
        let original = store
            .conversation_input_read(100, profile.entry(), &id)
            .unwrap();
        assert_eq!(original.reply, Some(reference.clone()));
        assert!(original.focus.is_none());
        let rendered = original.rendered();
        assert!(rendered.contains("native:42"));
        assert!(rendered.contains("conversation_part"));
        assert_eq!(original.text, "About that earlier answer");
        for source in [
            "native:0",
            "native:01",
            "native:-1",
            "native:9223372036854775808",
        ] {
            assert!(
                ConversationReply {
                    source: source.into(),
                    ..reference.clone()
                }
                .validate_context()
                .is_err()
            );
        }
        assert!(
            ConversationReply {
                context_only: false,
                ..reference
            }
            .validate_context()
            .is_err()
        );
    }
    fn setup() -> (tempfile::NamedTempFile, Store, InteractionConfig, String) {
        let file = tempfile::NamedTempFile::new().unwrap();
        let defaults = super::super::tests::defaults();
        let store = Store::open(file.path(), &[100], &defaults).unwrap();
        let profile:InteractionConfig=serde_json::from_value(json!({"human_id":100,"chat_id":100,"topic_id":1,"targets":[{"name":"Alpha","topic_id":7},{"name":"Beta","topic_id":8}]})).unwrap();
        for topic in [1, 7, 8] {
            let key = SessionKey::new(100, Some(topic));
            store.ensure_session(key, 100, &defaults).unwrap();
            store
                .set_session_codex_thread(key, &format!("native-{topic}"))
                .unwrap();
        }
        let binding = store
            .conversation_request_binding(100, profile.entry())
            .unwrap()["binding"]
            .as_str()
            .unwrap()
            .to_owned();
        (file, store, profile, binding)
    }
    fn capture(
        store: &Store,
        profile: &InteractionConfig,
        binding: &str,
        text: &str,
        reply: Option<ConversationReply>,
    ) -> String {
        let id = Uuid::now_v7().to_string();
        store
            .conversation_input_capture_reply(100, profile.entry(), binding, &id, text, reply)
            .unwrap();
        id
    }
    fn worker(action: Option<ConversationRouteAction>) -> crate::interaction::Instruction {
        match action.unwrap() {
            ConversationRouteAction::Worker(i, true) => *i,
            _ => panic!("Expected one new worker instruction"),
        }
    }
    #[test]
    fn closed_clarification_waiting_for_store_lock_cannot_admit() {
        let (_file, store, p, binding) = setup();
        let pending = capture(
            &store,
            &p,
            &binding,
            "#Unknown Preserve this original",
            None,
        );
        assert!(
            store
                .conversation_route(100, &pending, &p, None)
                .unwrap()
                .is_none()
        );
        let later = capture(
            &store,
            &p,
            &binding,
            "#Beta A later actual human input",
            None,
        );
        worker(store.conversation_route(100, &later, &p, None).unwrap());
        let reference = InputReference::Conversation(later);
        let store = std::sync::Arc::new(store);
        let closed = tokio_util::sync::CancellationToken::new();
        let guard = store.conn.lock().unwrap();
        let (send, ready) = std::sync::mpsc::channel();
        let task_store = store.clone();
        let task_profile = p.clone();
        let task_id = pending.clone();
        let task_closed = closed.clone();
        let task = std::thread::spawn(move || {
            send.send(()).unwrap();
            task_store
                .conversation_route_clarified(
                    100,
                    &task_id,
                    &task_profile,
                    "Alpha",
                    ("native-1", &reference, &task_closed),
                )
                .err()
                .unwrap()
                .to_string()
        });
        ready.recv().unwrap();
        closed.cancel();
        drop(guard);
        assert!(task.join().unwrap().contains("closed before admission"));
        assert_eq!(
            store
                .conversation_input_read(100, p.entry(), &pending)
                .unwrap()
                .phase,
            "awaiting_target"
        );
        assert_eq!(
            store
                .conn
                .lock()
                .unwrap()
                .query_row("SELECT count(*) FROM interaction_instructions", [], |r| r
                    .get::<_, i64>(
                    0
                ))
                .unwrap(),
            1
        );
    }
    #[test]
    fn old_admission_epoch_does_not_hide_current_pending_routes() {
        let (_file, store, p, binding) = setup();
        let old = capture(
            &store,
            &p,
            &binding,
            "#Unknown Earlier pending original",
            None,
        );
        store.conversation_route(100, &old, &p, None).unwrap();
        store.conn.lock().unwrap().execute("INSERT INTO bot_state(key,value) VALUES('stop_admission_epoch:100','synthetic-new-epoch')",[]).unwrap();
        let current = capture(
            &store,
            &p,
            &binding,
            "#Unknown Current pending original",
            None,
        );
        store.conversation_route(100, &current, &p, None).unwrap();
        let inventory = store
            .conversation_pending_routes(100, p.entry(), "native-1")
            .unwrap();
        assert_eq!(inventory["items"].as_array().unwrap().len(), 1);
        assert_eq!(inventory["items"][0]["id"], current);
        assert_eq!(
            store
                .conversation_input_read(100, p.entry(), &old)
                .unwrap()
                .phase,
            "awaiting_target"
        );
    }
    #[test]
    fn explicit_original_and_reply_reuse_requests_without_entry_model_or_retargeting() {
        let (_file, store, p, binding) = setup();
        let id = capture(&store, &p, &binding, "#Alpha Work on requirement13", None);
        let first = worker(store.conversation_route(100, &id, &p, None).unwrap());
        assert_eq!(first.target.thread_id, 7);
        let root = store.conversation_input_read(100, p.entry(), &id).unwrap();
        assert_eq!(root.phase, "routed");
        assert_eq!(root.local_turn, None);
        assert_eq!(
            store
                .conn
                .lock()
                .unwrap()
                .query_row("SELECT count(*) FROM turns", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert!(
            store
                .interaction_entry_original(
                    p.entry(),
                    100,
                    "native-1",
                    &InputReference::Conversation(id.clone()),
                    ""
                )
                .is_ok()
        );
        assert!(
            store
                .interaction_entry_references(p.entry(), 100, "native-1")
                .unwrap()
                .contains(&InputReference::Conversation(id.clone()))
        );
        assert!(store.conversation_route(100, &id, &p, Some("t8")).is_err());
        assert!(
            store
                .conversation_route(100, &id, &p, Some("t7"))
                .unwrap()
                .is_none()
        );
        let reply = store
            .conversation_continuation(100, p.entry(), &id)
            .unwrap()
            .unwrap();
        let amended = capture(
            &store,
            &p,
            &binding,
            "Use requirement17 instead",
            Some(reply.clone()),
        );
        let second = worker(store.conversation_route(100, &amended, &p, None).unwrap());
        assert_eq!(
            second.relation,
            RequestRelation::Amends(first.requests.clone())
        );
        let explicit = capture(
            &store,
            &p,
            &binding,
            "#Beta Start independent work",
            Some(reply.clone()),
        );
        let other = worker(store.conversation_route(100, &explicit, &p, None).unwrap());
        assert_eq!(other.target.thread_id, 8);
        assert_eq!(other.relation, RequestRelation::New);
        let mut forged = reply.clone();
        forged.revision = "0".repeat(64);
        assert!(
            store
                .conversation_input_capture_reply(
                    100,
                    p.entry(),
                    &binding,
                    &Uuid::now_v7().to_string(),
                    "Forged focus",
                    Some(forged)
                )
                .is_err()
        );
        assert!(
            store
                .conversation_input_capture_reply(
                    100,
                    p.entry(),
                    &binding,
                    &amended,
                    "Use requirement17 instead",
                    None
                )
                .is_err()
        );
        assert!(
            !store
                .conversation_input_capture_reply(
                    100,
                    p.entry(),
                    &binding,
                    &amended,
                    "Use requirement17 instead",
                    Some(reply)
                )
                .unwrap()
                .1
        );
    }
    #[test]
    fn ambiguous_route_survives_restart_and_accepts_one_explicit_choice() {
        let (file, store, p, binding) = setup();
        let id = capture(
            &store,
            &p,
            &binding,
            "#Unknown Keep this exact source",
            None,
        );
        assert!(
            store
                .conversation_route(100, &id, &p, None)
                .unwrap()
                .is_none()
        );
        assert_eq!(
            store
                .conversation_input_read(100, p.entry(), &id)
                .unwrap()
                .phase,
            "awaiting_target"
        );
        drop(store);
        let reopened = Store::open(file.path(), &[100], &super::super::tests::defaults()).unwrap();
        let selected = worker(
            reopened
                .conversation_route(100, &id, &p, Some("t8"))
                .unwrap(),
        );
        assert_eq!(selected.target.thread_id, 8);
        assert!(
            reopened
                .conversation_route(100, &id, &p, Some("t7"))
                .is_err()
        );
        assert!(
            reopened
                .conversation_route(100, &id, &p, Some("t8"))
                .unwrap()
                .is_none()
        );
        assert_eq!(
            reopened
                .conn
                .lock()
                .unwrap()
                .query_row("SELECT count(*) FROM interaction_instructions", [], |r| r
                    .get::<_, i64>(
                    0
                ))
                .unwrap(),
            1
        );
        let plain = capture(
            &reopened,
            &p,
            &binding,
            "A literal quoted #Alpha is ordinary conversation",
            None,
        );
        assert!(
            reopened
                .conversation_route(100, &plain, &p, None)
                .unwrap()
                .is_none()
        );
        assert_eq!(
            reopened
                .conversation_input_read(100, p.entry(), &plain)
                .unwrap()
                .phase,
            "retained"
        );
    }
    #[test]
    fn captured_reply_cannot_follow_deleted_recreated_target_or_restored_owner() {
        let (_file, store, p, binding) = setup();
        let parent = capture(&store, &p, &binding, "#Alpha Initial", None);
        worker(store.conversation_route(100, &parent, &p, None).unwrap());
        let reply = store
            .conversation_continuation(100, p.entry(), &parent)
            .unwrap()
            .unwrap();
        let child = capture(
            &store,
            &p,
            &binding,
            "Continue exact original request",
            Some(reply),
        );
        store
            .conn
            .lock()
            .unwrap()
            .execute("DELETE FROM sessions WHERE thread_id=7", [])
            .unwrap();
        let key = SessionKey::new(100, Some(7));
        store
            .ensure_session(key, 100, &super::super::tests::defaults())
            .unwrap();
        store
            .set_session_codex_thread(key, "replacement-native")
            .unwrap();
        assert!(store.conversation_route(100, &child, &p, None).is_err());
        let held = capture(&store, &p, &binding, "#Unknown Waiting", None);
        store.conversation_route(100, &held, &p, None).unwrap();
        store.conn.lock().unwrap().execute_batch("UPDATE users SET allowed=0 WHERE tg_user_id=100; UPDATE users SET allowed=1 WHERE tg_user_id=100;").unwrap();
        assert!(
            store
                .conversation_route(100, &held, &p, Some("t8"))
                .is_err()
        );
    }
    #[test]
    fn held_focus_retires_on_native_and_target_owner_aba_without_retiring_entry() {
        for change in ["native", "owner"] {
            let (_file, store, p, binding) = setup();
            let first = capture(&store, &p, &binding, "#Alpha Original work", None);
            worker(store.conversation_route(100, &first, &p, None).unwrap());
            let reply = store
                .conversation_continuation(100, p.entry(), &first)
                .unwrap()
                .unwrap();
            let held = capture(
                &store,
                &p,
                &binding,
                "Delayed amendment",
                Some(reply.clone()),
            );
            let override_input = capture(
                &store,
                &p,
                &binding,
                "#Beta Explicit independent destination",
                Some(reply),
            );
            if change == "native" {
                let target = SessionKey::new(100, Some(7));
                store
                    .set_session_codex_thread(target, "replacement")
                    .unwrap();
                store.set_session_codex_thread(target, "native-7").unwrap();
            } else {
                store.conn.lock().unwrap().execute_batch("UPDATE sessions SET creator_user_id=101 WHERE thread_id=7;UPDATE sessions SET creator_user_id=100 WHERE thread_id=7;").unwrap();
            }
            assert!(
                store
                    .conversation_input_read(100, p.entry(), &held)
                    .unwrap()
                    .focus
                    .unwrap()
                    .retired
            );
            assert!(store.conversation_route(100, &held, &p, None).is_err());
            let explicit = worker(
                store
                    .conversation_route(100, &override_input, &p, None)
                    .unwrap(),
            );
            assert_eq!(explicit.target.thread_id, 8);
            assert_eq!(explicit.relation, RequestRelation::New);
            // Retirement is specific to the selected pending reply. Fresh work
            // still resolves the logical session's current generation.
            let fresh = capture(&store, &p, &binding, "#Alpha Fresh instruction", None);
            worker(store.conversation_route(100, &fresh, &p, None).unwrap());
        }
    }
    #[test]
    fn reply_while_first_worker_native_is_preparing_preserves_request_lineage() {
        for establish_before_admission in [false, true] {
            let (_file, store, p, binding) = setup();
            store.conn.lock().unwrap().execute_batch("UPDATE sessions SET codex_thread_id=NULL,force_fresh_thread=1 WHERE thread_id=7;").unwrap();
            let id = capture(&store, &p, &binding, "#Alpha First work", None);
            let first = worker(store.conversation_route(100, &id, &p, None).unwrap());
            let reply = store
                .conversation_continuation(100, p.entry(), &id)
                .unwrap()
                .unwrap();
            let amendment = capture(
                &store,
                &p,
                &binding,
                "Correction while first thread starts",
                Some(reply),
            );
            if establish_before_admission {
                store
                    .set_session_codex_thread(
                        SessionKey::new(100, Some(7)),
                        "first-established-native",
                    )
                    .unwrap();
            }
            let amended = worker(store.conversation_route(100, &amendment, &p, None).unwrap());
            assert_eq!(amended.relation, RequestRelation::Amends(first.requests));
        }
    }
    #[test]
    fn clarification_without_original_execution_scope_never_claims_inactivity() {
        let (_file, store, p, binding) = setup();
        let id = capture(&store, &p, &binding, "#Unknown /interrupt", None);
        assert!(
            store
                .conversation_route(100, &id, &p, None)
                .unwrap()
                .is_none()
        );
        assert!(matches!(
            store.conversation_route(100, &id, &p, Some("t7")).unwrap(),
            Some(ConversationRouteAction::Interrupt(_))
        ));
        assert_eq!(
            store
                .conversation_interruption_apply(100, &id, |_| None)
                .unwrap(),
            None
        );
        assert_eq!(
            store
                .conversation_input_read(100, p.entry(), &id)
                .unwrap()
                .routing
                .unwrap()
                .phase,
            "scope_unavailable"
        );
        assert!(
            store
                .conversation_interruption_apply(100, &id, |_| panic!(
                    "Must never select a newer execution"
                ))
                .is_err()
        );
    }
    #[test]
    fn scoped_interruption_applies_once_under_current_authority_and_never_rearms() {
        let (_file, store, p, binding) = setup();
        let id = capture(&store, &p, &binding, "#Alpha /interrupt", None);
        assert!(matches!(
            store.conversation_route(100, &id, &p, None).unwrap(),
            Some(ConversationRouteAction::Interrupt(_))
        ));
        let mut calls = 0;
        assert!(
            store
                .conversation_interruption_apply(100, &id, |key| {
                    assert_eq!(key.thread_id, 7);
                    calls += 1;
                    Some(true)
                })
                .unwrap()
                .unwrap()
        );
        assert_eq!(calls, 1);
        assert!(
            store
                .conversation_interruption_apply(100, &id, |_| {
                    calls += 1;
                    Some(true)
                })
                .is_err()
        );
        assert_eq!(calls, 1);
        assert!(
            store
                .conversation_route(100, &id, &p, None)
                .unwrap()
                .is_none()
        );
        let retired = capture(&store, &p, &binding, "#Beta /interrupt", None);
        store.conversation_route(100, &retired, &p, None).unwrap();
        store.conn.lock().unwrap().execute_batch("UPDATE users SET allowed=0 WHERE tg_user_id=100;UPDATE users SET allowed=1 WHERE tg_user_id=100;").unwrap();
        assert!(
            store
                .conversation_interruption_apply(100, &retired, |_| {
                    calls += 1;
                    Some(true)
                })
                .is_err()
        );
        assert_eq!(calls, 1);
    }
}
