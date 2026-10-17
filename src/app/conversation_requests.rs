//! Canonical owner controls reuse the live native question/approval responders.
use super::*;
use crate::store::ConversationRequest;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

#[derive(Clone, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum Action {
    Answer { text: String },
    Option { index: usize },
    Approval { choice: String },
    Cancel,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Decision {
    binding: String,
    id: String,
    revision: String,
    action: Action,
}

fn question_payload(p: &questions::PendingDialogue) -> Value {
    let q = &p.request.questions[p.index];
    let mut body = format!("{}\n\n{}", q.header, q.question);
    let mut choices = Vec::new();
    for (index, option) in q.options.iter().flatten().enumerate() {
        body.push_str(&format!(
            "\n\n{}. {} — {}",
            index + 1,
            option.label,
            option.description
        ));
        choices.push(json!({"action":{"kind":"option","index":index},"label":option.label.chars().take(200).collect::<String>()}));
    }
    json!({"kind":"question","question_id":q.id,"source_turn":p.request.turn_id,"body":body,"choices":choices,"allows_text":true,
        "can_cancel":true,"question_index":p.index,"question_total":p.request.questions.len(),"blocking":p.request.is_blocking})
}
fn approval_payload(p: &PendingApproval) -> Value {
    json!({"kind":"approval","body":p.request.prompt,"allows_text":false,"can_cancel":false,
        "choices":p.request.options.iter().map(|choice|json!({
            "action":{"kind":"approval","choice":presentation::approval_button_code(*choice)},
            "label":presentation::approval_button_text(*choice)})).collect::<Vec<_>>()})
}
fn revision(snapshot: &ConversationRequest, payload: &Value) -> String {
    format!(
        "{:x}",
        Sha256::digest(
            json!([
                snapshot.scope,
                snapshot.item.id,
                snapshot.item.state,
                payload
            ])
            .to_string()
            .as_bytes()
        )
    )
}
fn response(snapshot: &ConversationRequest, payload: &Value, offset: usize) -> Result<Value> {
    let body = payload["body"].as_str().unwrap_or(&snapshot.item.action);
    let length = body.chars().count();
    if offset > length {
        bail!("Invalid request text offset");
    }
    let text: String = body.chars().skip(offset).take(4000).collect();
    let end = offset + text.chars().count();
    let mut result = payload.clone();
    result["body"] = json!(text);
    result["schema"] = json!("telecodex.conversation-request/v1");
    result["id"] = json!(snapshot.item.id);
    result["revision"] = json!(revision(snapshot, payload));
    result["binding"] = json!(snapshot.binding);
    result["entry_binding"] = snapshot.entry_binding.clone();
    result["state"] = json!(snapshot.item.state);
    result["title"] = json!(snapshot.item.title.chars().take(200).collect::<String>());
    result["offset"] = json!(offset);
    result["next_offset"] = if end < length {
        json!(end)
    } else {
        Value::Null
    };
    result["recorded_revisions"] = json!(
        snapshot.item.source["conversationDecisions"]
            .as_object()
            .map(|v| v.keys().cloned().collect::<Vec<_>>())
            .unwrap_or_default()
    );
    result["native_incorporation"] = json!("Not established by recording a decision");
    Ok(result)
}
fn receipt(binding: &Value, doc: &Value, fresh: bool) -> Value {
    json!({"schema":"telecodex.conversation-decision/v1","id":doc["id"],"revision":doc["revision"],
        "entry_binding":binding["entry_binding"],"binding":binding["binding"],"new_decision":fresh,
        "recorded":true,"recorded_at":doc["recorded_at"],"source_binding":doc["binding"],
        "native_incorporation":"Not established by recording a decision"})
}
fn retained_receipt(
    app: &App,
    human: i64,
    entry: SessionKey,
    id: &str,
    rev: &str,
    fresh: bool,
) -> Result<Value> {
    let binding = app
        .shared
        .store
        .conversation_request_binding(human, entry)?;
    let doc = app
        .shared
        .store
        .conversation_decision_receipt(human, entry, id, rev)?
        .context("Decision recording unconfirmed")?;
    Ok(receipt(&binding, &doc, fresh))
}
fn question_matches(
    p: &questions::PendingDialogue,
    snapshot: &ConversationRequest,
    human: i64,
) -> bool {
    p.requester == human
        && p.local_turn_id == snapshot.item.local_turn
        && p.native_session == snapshot.key
        && p.request.thread_id == snapshot.item.native_thread
        && !p.closed.is_cancelled()
        && !p.responder.is_closed()
}
fn approval_matches(p: &PendingApproval, snapshot: &ConversationRequest, human: i64) -> bool {
    p.requester_user_id == human
        && p.local_turn == Some(snapshot.item.local_turn)
        && !p.responder.is_closed()
}
fn selected_response(
    app: &App,
    entry: SessionKey,
    snapshot: &ConversationRequest,
    payload: &Value,
    offset: usize,
    live: bool,
) -> Result<Value> {
    let mut result = response(snapshot, payload, offset)?;
    result["actionable"] = json!(live && snapshot.item.state == "pending");
    result["label"] = json!(if snapshot.key == entry {
        "Conversation".into()
    } else {
        format!("#{}", super::routing::label(&app.shared, snapshot.key))
    });
    Ok(result)
}
async fn detail(
    app: &App,
    human: i64,
    entry: SessionKey,
    id: &str,
    offset: usize,
) -> Result<Value> {
    {
        let guard = app.shared.pending_questions.lock().await;
        let snapshot = app.shared.store.conversation_request(human, entry, id)?;
        if let Some(p) = guard
            .get(id)
            .filter(|p| question_matches(p, &snapshot, human))
        {
            return selected_response(app, entry, &snapshot, &question_payload(p), offset, true);
        }
    }
    let guard = app.shared.pending_approvals.lock().await;
    let snapshot = app.shared.store.conversation_request(human, entry, id)?;
    if let Some(p) = guard
        .get(id)
        .filter(|p| approval_matches(p, &snapshot, human))
    {
        return selected_response(app, entry, &snapshot, &approval_payload(p), offset, true);
    }
    let payload = json!({"kind":snapshot.item.source["kind"],"body":snapshot.item.action,
        "choices":[],"allows_text":false,"can_cancel":false});
    selected_response(app, entry, &snapshot, &payload, offset, false)
}
fn already_recorded(
    app: &App,
    human: i64,
    entry: SessionKey,
    d: &Decision,
    action: &Value,
) -> Result<Option<Value>> {
    let binding = app
        .shared
        .store
        .conversation_request_binding(human, entry)?;
    if binding["binding"] != d.binding {
        bail!("Selected conversation changed");
    }
    if let Some(doc) =
        app.shared
            .store
            .conversation_decision_receipt(human, entry, &d.id, &d.revision)?
    {
        if doc["action"] != *action {
            bail!("A recorded decision cannot be replaced");
        }
        return Ok(Some(receipt(&binding, &doc, false)));
    }
    Ok(None)
}

pub(super) async fn command(app: &App, human: i64, args: &str) -> Result<Value> {
    let cfg = app
        .shared
        .config
        .interaction
        .as_ref()
        .context("Interaction not configured")?;
    if cfg.human_id != human {
        bail!("Conversation request owner mismatch");
    }
    let entry = cfg.entry();
    if let Some(raw) = args.strip_prefix("decide ") {
        let decision: Decision = serde_json::from_str(raw)?;
        return decide(app, human, entry, decision).await;
    }
    let fields: Vec<_> = args.split_whitespace().collect();
    match fields.as_slice() {
        ["list", after] => {
            let (rows, more) =
                app.shared
                    .store
                    .conversation_request_ids(human, entry, after.parse()?)?;
            let mut items = Vec::new();
            for (_, id) in &rows {
                let mut item = detail(app, human, entry, id, 0).await?;
                item["preview"] = json!(
                    item["body"]
                        .as_str()
                        .unwrap_or("")
                        .chars()
                        .take(240)
                        .collect::<String>()
                );
                for key in ["body", "choices", "allows_text", "can_cancel"] {
                    item.as_object_mut().expect("request object").remove(key);
                }
                items.push(item);
            }
            // Empty pages also carry authenticated configured-entry provenance.
            let page = app
                .shared
                .store
                .conversation_request_binding(human, entry)?;
            Ok(
                json!({"schema":"telecodex.conversation-request-list/v1","entry_binding":page["entry_binding"],
                "binding":page["binding"],"items":items,"cursor":rows.last().map_or(after.parse().unwrap_or(0),|row|row.0),"more":more}),
            )
        }
        ["receipt", id, rev] if id.len() <= 64 && rev.len() == 64 => {
            retained_receipt(app, human, entry, id, rev, false)
        }
        ["detail", id, "0"] if id.len() <= 64 => detail(app, human, entry, id, 0).await,
        ["detail", id, offset, expected] if id.len() <= 64 && expected.len() == 64 => {
            let result = detail(app, human, entry, id, offset.parse()?).await?;
            if result["revision"] != *expected {
                bail!("Native request changed; read its current proposal");
            }
            Ok(result)
        }
        _ => bail!("Expected requests list CURSOR, detail ID OFFSET or decide JSON"),
    }
}

async fn decide(app: &App, human: i64, entry: SessionKey, d: Decision) -> Result<Value> {
    if d.id.len() > 64 || d.revision.len() != 64 {
        bail!("Invalid request decision identity");
    }
    let action = serde_json::to_value(&d.action)?;
    if let Some(result) = already_recorded(app, human, entry, &d, &action)? {
        return Ok(result);
    }
    let snapshot = app.shared.store.conversation_request(human, entry, &d.id)?;
    if snapshot.item.state != "pending" {
        bail!("Native request no longer pending");
    }
    if snapshot.item.source["kind"] == "question" {
        let mut guard = app.shared.pending_questions.lock().await;
        let snapshot = app.shared.store.conversation_request(human, entry, &d.id)?;
        if let Some(result) = already_recorded(app, human, entry, &d, &action)? {
            return Ok(result);
        }
        let p = guard
            .get(&d.id)
            .filter(|p| question_matches(p, &snapshot, human))
            .context("Native question response channel closed")?;
        if revision(&snapshot, &question_payload(p)) != d.revision {
            bail!("Question changed; read its current request");
        }
        let value = match &d.action {
            Action::Answer { text }
                if !text.trim().is_empty() && text.len() <= 32768 && !text.contains('\0') =>
            {
                Some(text.clone())
            }
            Action::Option { index } => Some(
                p.request.questions[p.index]
                    .options
                    .as_ref()
                    .and_then(|v| v.get(*index))
                    .context("Invalid answer option")?
                    .label
                    .clone(),
            ),
            Action::Cancel => None,
            _ => bail!("Invalid question action"),
        };
        let final_answer = value.is_none() || p.index + 1 == p.request.questions.len();
        let next = if final_answer {
            None
        } else {
            let q = &p.request.questions[p.index + 1];
            Some((q.header.as_str(), q.question.as_str(), p.index + 1))
        };
        let state = if value.is_none() {
            "cancelled"
        } else if final_answer {
            "answering"
        } else {
            "pending"
        };
        let fresh = app.shared.store.conversation_request_decide(
            human,
            entry,
            &snapshot,
            &d.revision,
            &action,
            &question_payload(p),
            &json!({"question_id":p.request.questions[p.index].id,"answer":value}),
            state,
            next,
        )?;
        if !fresh {
            return retained_receipt(app, human, entry, &d.id, &d.revision, false);
        }
        let mut p = guard.remove(&d.id).expect("validated live question");
        p.retire_telegram_controls();
        let sent = if let Some(value) = value {
            p.apply_answer(value);
            if final_answer {
                p.responder
                    .send(UserInputDecision::Answers(p.answers))
                    .is_ok()
            } else {
                guard.insert(d.id.clone(), p);
                true
            }
        } else {
            p.responder.send(UserInputDecision::Cancel).is_ok()
        };
        if !sent {
            app.shared
                .store
                .conversation_request_channel_closed(human, &d.id)?;
        }
    } else if snapshot.item.source["kind"] == "approval" {
        let mut guard = app.shared.pending_approvals.lock().await;
        let snapshot = app.shared.store.conversation_request(human, entry, &d.id)?;
        if let Some(result) = already_recorded(app, human, entry, &d, &action)? {
            return Ok(result);
        }
        let p = guard
            .get(&d.id)
            .filter(|p| approval_matches(p, &snapshot, human))
            .context("Native approval response channel closed")?;
        if revision(&snapshot, &approval_payload(p)) != d.revision {
            bail!("Approval changed; read its current request");
        }
        let Action::Approval { choice } = &d.action else {
            bail!("Invalid approval action");
        };
        let selected = p
            .request
            .options
            .iter()
            .find(|option| presentation::approval_button_code(**option) == choice)
            .copied()
            .context("Approval choice was not offered")?;
        let fresh = app.shared.store.conversation_request_decide(
            human,
            entry,
            &snapshot,
            &d.revision,
            &action,
            &approval_payload(p),
            &json!({"code":choice,"label":presentation::approval_button_text(selected)}),
            "answering",
            None,
        )?;
        if !fresh {
            return retained_receipt(app, human, entry, &d.id, &d.revision, false);
        }
        let pending = guard.remove(&d.id).expect("validated live approval");
        if pending.responder.send(selected).is_err() {
            app.shared
                .store
                .conversation_request_channel_closed(human, &d.id)?;
        }
    } else {
        bail!("Unsupported native request");
    }
    retained_receipt(app, human, entry, &d.id, &d.revision, true)
}
