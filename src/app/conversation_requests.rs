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
    result["capture_version"] = json!(snapshot.capture_version);
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
    detail_scoped(app, human, entry, id, offset, false).await
}
fn request_snapshot(
    app: &App,
    human: i64,
    entry: SessionKey,
    id: &str,
    internal: bool,
) -> Result<ConversationRequest> {
    if internal {
        app.shared.store.internal_native_request(human, entry, id)
    } else {
        app.shared.store.conversation_request(human, entry, id)
    }
}
async fn detail_scoped(
    app: &App,
    human: i64,
    entry: SessionKey,
    id: &str,
    offset: usize,
    internal: bool,
) -> Result<Value> {
    {
        let guard = app.shared.pending_questions.lock().await;
        let snapshot = request_snapshot(app, human, entry, id, internal)?;
        if let Some(p) = guard
            .get(id)
            .filter(|p| question_matches(p, &snapshot, human))
        {
            return selected_response(app, entry, &snapshot, &question_payload(p), offset, true);
        }
    }
    let guard = app.shared.pending_approvals.lock().await;
    let snapshot = request_snapshot(app, human, entry, id, internal)?;
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
    source_input: Option<&str>,
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
        if doc["action"] != *action || source_input.is_some_and(|id| doc["source_input"] != id) {
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
    if let Some(raw) = args.strip_prefix("internal decide ") {
        let decision: Decision = serde_json::from_str(raw)?;
        return decide_scoped(app, human, entry, decision, None, None, true).await;
    }
    if let Some(raw) = args.strip_prefix("internal dismiss ") {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Dismissal {
            binding: String,
            id: String,
            revision: String,
        }
        let selected: Dismissal = serde_json::from_str(raw)?;
        return app.shared.store.internal_dismiss_human_task(
            human,
            entry,
            &selected.binding,
            &selected.id,
            &selected.revision,
        );
    }
    if let Some(args) = args.strip_prefix("internal ") {
        let fields: Vec<_> = args.split_whitespace().collect();
        return match fields.as_slice() {
            ["list", after] => {
                let mut page =
                    app.shared
                        .store
                        .internal_pending_page(human, entry, after.parse()?)?;
                let binding = page["binding"].clone();
                let entry_binding = page["entry_binding"].clone();
                for item in page["items"]
                    .as_array_mut()
                    .context("Missing internal items")?
                {
                    let id = item["id"]
                        .as_str()
                        .context("Missing internal identity")?
                        .to_owned();
                    item["binding"] = binding.clone();
                    item["entry_binding"] = entry_binding.clone();
                    if item["kind"] == "native" {
                        let detail = detail_scoped(app, human, entry, &id, 0, true).await?;
                        for key in [
                            "kind",
                            "revision",
                            "state",
                            "actionable",
                            "question_index",
                            "question_total",
                            "capture_version",
                        ] {
                            item[key] = detail[key].clone();
                        }
                    } else if item["kind"] == "task" {
                        let task = app.shared.store.internal_human_task(human, entry, &id)?;
                        for key in ["revision", "state", "can_dismiss"] {
                            item[key] = task[key].clone();
                        }
                    }
                }
                Ok(page)
            }
            ["request", id, offset] | ["request", id, offset, _] if id.len() <= 64 => {
                let value = detail_scoped(app, human, entry, id, offset.parse()?, true).await?;
                if fields.len() == 4 && value["revision"] != fields[3] {
                    bail!("Internal request changed; read its current proposal");
                }
                Ok(value)
            }
            ["task", id, offset] | ["task", id, offset, _] if id.len() <= 64 => {
                let mut value = app.shared.store.internal_human_task(human, entry, id)?;
                if fields.len() == 4 && value["revision"] != fields[3] {
                    bail!("Internal action changed; read its current text");
                }
                let offset: usize = offset.parse()?;
                let body = value["body"].as_str().context("Missing task text")?;
                let length = body.chars().count();
                if offset > length {
                    bail!("Invalid task offset");
                }
                let part: String = body.chars().skip(offset).take(4000).collect();
                let end = offset + part.chars().count();
                value["body"] = json!(part);
                value["offset"] = json!(offset);
                value["next_offset"] = if end < length {
                    json!(end)
                } else {
                    Value::Null
                };
                let target: SessionKey = serde_json::from_value(value["target"].clone())?;
                value["label"] = json!(super::routing::label(&app.shared, target));
                Ok(value)
            }
            ["receipt", id, rev] if id.len() <= 64 && rev.len() == 64 => {
                retained_receipt(app, human, entry, id, rev, false)
            }
            _ => bail!("Expected internal list CURSOR, request ID OFFSET or decide JSON"),
        };
    }
    if let Some(raw) = args.strip_prefix("tasks dismiss ") {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Dismissal {
            binding: String,
            id: String,
            revision: String,
        }
        let selected: Dismissal = serde_json::from_str(raw)?;
        let mut armed = app.shared.pending_task_text.lock().await;
        let result = app.shared.store.conversation_dismiss_human_task(
            human,
            entry,
            &selected.binding,
            &selected.id,
            &selected.revision,
        )?;
        let target: SessionKey = serde_json::from_value(result["target"].clone())?;
        if armed.get(&(target, human)) == Some(&selected.id) {
            armed.remove(&(target, human));
        }
        return Ok(result);
    }
    if let Some(args) = args.strip_prefix("tasks ") {
        let fields: Vec<_> = args.split_whitespace().collect();
        return match fields.as_slice() {
            ["list", after] => {
                let mut value =
                    app.shared
                        .store
                        .conversation_human_tasks(human, entry, after.parse()?)?;
                for item in value["items"].as_array_mut().context("Missing task list")? {
                    let key: SessionKey = serde_json::from_value(item["target"].clone())?;
                    item["label"] = json!(if key == entry {
                        "Conversation".into()
                    } else {
                        format!("#{}", super::routing::label(&app.shared, key))
                    });
                }
                Ok(value)
            }
            ["detail", id, offset] | ["detail", id, offset, _] if id.len() <= 64 => {
                let mut value = app.shared.store.conversation_human_task(human, entry, id)?;
                if fields.len() == 4 && value["revision"] != fields[3] {
                    bail!("Task changed; read its current action");
                }
                let body = value["body"].as_str().context("Missing task action")?;
                let offset: usize = offset.parse()?;
                let length = body.chars().count();
                if offset > length {
                    bail!("Invalid task text offset");
                }
                let text: String = body.chars().skip(offset).take(4000).collect();
                let end = offset + text.chars().count();
                value["body"] = json!(text);
                value["offset"] = json!(offset);
                value["next_offset"] = if end < length {
                    json!(end)
                } else {
                    Value::Null
                };
                let key: SessionKey = serde_json::from_value(value["target"].clone())?;
                value["label"] = json!(if key == entry {
                    "Conversation".into()
                } else {
                    format!("#{}", super::routing::label(&app.shared, key))
                });
                Ok(value)
            }
            _ => bail!("Expected tasks list CURSOR or detail ID OFFSET [REVISION]"),
        };
    }
    if let Some(raw) = args.strip_prefix("decide ") {
        let decision: Decision = serde_json::from_str(raw)?;
        return decide(app, human, entry, decision, None, None).await;
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

/// Check a frozen human question selection before capture, without answering it.
/// Delivery rechecks the revision after any transcription or other delay.
pub(super) async fn validate_reply(
    app: &App,
    human: i64,
    reply: Option<&crate::store::ConversationReply>,
) -> Result<()> {
    let Some(reply) = reply else { return Ok(()) };
    if reply.context_only {
        // Ownership is checked in the capture transaction. A question that
        // advanced after playback remains context, never an implicit answer.
        return Ok(());
    }
    let Some(id) = reply.source.strip_prefix("request:") else {
        return Ok(());
    };
    let profile = super::routing::profile(&app.shared)?;
    let view = detail(app, human, profile.entry(), id, 0).await?;
    if view["kind"] != "question"
        || view["actionable"] != true
        || view["revision"] != reply.revision
    {
        bail!("Selected question is no longer current");
    }
    Ok(())
}
/// Answer only the selected live native question from the retained original.
pub(super) async fn answer_original(
    app: &App,
    input: &crate::store::ConversationInput,
) -> Result<()> {
    let selected = input.reply.as_ref().context("Question selection missing")?;
    let id = selected
        .source
        .strip_prefix("request:")
        .context("Not a native question reply")?;
    let binding = app
        .shared
        .store
        .conversation_request_binding(input.human, input.entry)?;
    let d = Decision {
        binding: binding["binding"]
            .as_str()
            .context("Missing entry binding")?
            .into(),
        id: id.into(),
        revision: selected.revision.clone(),
        action: Action::Answer {
            text: input.question_answer_text(&app.shared.config.db_path),
        },
    };
    decide(app, input.human, input.entry, d, Some(&input.id), None).await?;
    Ok(())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ContextQuestion {
    input_ref: String,
    #[serde(default)]
    offset: usize,
    #[serde(default)]
    question: Option<QuestionSelection>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct QuestionSelection {
    id: String,
    revision: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct QuestionCatalog {
    input_ref: String,
    #[serde(default)]
    after: i64,
}

pub(super) async fn questions(
    app: &App,
    human: i64,
    entry: SessionKey,
    native: &str,
    arguments: Value,
    closed: &CancellationToken,
) -> Result<Value> {
    let args: QuestionCatalog = serde_json::from_value(arguments)?;
    let input = app
        .shared
        .store
        .conversation_input_read(human, entry, &args.input_ref)?;
    if input.native != native || closed.is_cancelled() {
        bail!("Question inspection belongs to another native conversation");
    }
    let page = command(app, human, &format!("list {}", args.after)).await?;
    let mut items = Vec::new();
    for item in page["items"].as_array().context("Missing questions")? {
        if item["kind"] != "question" {
            continue;
        }
        let mut item = item.clone();
        item["available_at_capture"] = json!(app.shared.store.conversation_question_at_capture(
            human,
            entry,
            &input.id,
            item["id"].as_str().context("Missing question identity")?
        )?);
        items.push(item);
    }
    Ok(
        json!({"input_ref":input.id,"items":items,"cursor":page["cursor"],"more":page["more"],"selection":"No answer or destination selected; inspect the full question before interpreting an original"}),
    )
}

/// Classify a retained contextual input without model-authored answer text.
pub(super) async fn interpret_context(
    app: &App,
    human: i64,
    entry: SessionKey,
    native: &str,
    arguments: Value,
    closed: &CancellationToken,
    answer: bool,
) -> Result<Value> {
    let args: ContextQuestion = serde_json::from_value(arguments)?;
    let input = app
        .shared
        .store
        .conversation_input_read(human, entry, &args.input_ref)?;
    let (id, selected_revision, explicit) = if let Some(selected) = &args.question {
        if selected.id.len() != 32 || selected.revision.len() != 64 {
            bail!("Invalid question selection");
        }
        (selected.id.as_str(), selected.revision.as_str(), true)
    } else {
        let selected = input.reply.as_ref().filter(|r| r.context_only).context(
            "This input has no contextual source; inspect questions and select one explicitly",
        )?;
        (
            selected
                .source
                .strip_prefix("request:")
                .context("Context is not a native question")?,
            selected.revision.as_str(),
            false,
        )
    };
    if input.native != native || closed.is_cancelled() {
        bail!("Context interpretation belongs to another native conversation");
    }
    if !answer {
        let view = detail(app, human, entry, id, args.offset).await?;
        return Ok(
            json!({"reported_context":input.reply,"selection":{"id":id,"revision":selected_revision},"matches_current":view["revision"]==selected_revision,"available_at_capture":app.shared.store.conversation_question_at_capture(human,entry,&input.id,id)?,"question":view}),
        );
    }
    if args.offset != 0 {
        bail!("An answer does not select a fragment");
    }
    let binding = app
        .shared
        .store
        .conversation_request_binding(human, entry)?;
    let decision = Decision {
        binding: binding["binding"]
            .as_str()
            .context("Missing conversation binding")?
            .into(),
        id: id.into(),
        revision: selected_revision.into(),
        action: Action::Answer {
            text: input.question_answer_text(&app.shared.config.db_path),
        },
    };
    decide(
        app,
        human,
        entry,
        decision,
        Some(&input.id),
        Some((native, closed, explicit)),
    )
    .await
}

async fn decide(
    app: &App,
    human: i64,
    entry: SessionKey,
    d: Decision,
    source_input: Option<&str>,
    interpretation: Option<(&str, &CancellationToken, bool)>,
) -> Result<Value> {
    decide_scoped(app, human, entry, d, source_input, interpretation, false).await
}
async fn decide_scoped(
    app: &App,
    human: i64,
    entry: SessionKey,
    d: Decision,
    source_input: Option<&str>,
    interpretation: Option<(&str, &CancellationToken, bool)>,
    internal: bool,
) -> Result<Value> {
    if d.id.len() > 64 || d.revision.len() != 64 {
        bail!("Invalid request decision identity");
    }
    let action = serde_json::to_value(&d.action)?;
    if let Some(result) = already_recorded(app, human, entry, &d, &action, source_input)? {
        return Ok(result);
    }
    let snapshot = request_snapshot(app, human, entry, &d.id, internal)?;
    if snapshot.item.state != "pending" {
        bail!("Native request no longer pending");
    }
    if snapshot.item.source["kind"] == "question" {
        let mut guard = app.shared.pending_questions.lock().await;
        let snapshot = request_snapshot(app, human, entry, &d.id, internal)?;
        if let Some(result) = already_recorded(app, human, entry, &d, &action, source_input)? {
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
            source_input,
            interpretation,
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
        let snapshot = request_snapshot(app, human, entry, &d.id, internal)?;
        if let Some(result) = already_recorded(app, human, entry, &d, &action, source_input)? {
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
            None,
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
