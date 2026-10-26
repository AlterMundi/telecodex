//! Create one justified destination through the existing topic/session adapter.
use super::*;
use crate::interaction::InputReference;
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

pub(super) fn schema() -> Value {
    json!({"type":"object","properties":{"input_ref":{"oneOf":[{"type":"integer","minimum":0},{"type":"string","format":"uuid"}]},"name":{"type":"string","description":"Human-selected topic title, including spaces (1-64 UTF-8 bytes, no surrounding whitespace, controls or markup). Multiword titles route through the returned stable tID selector; stable tID titles are reserved"},"reason":{"type":"string","description":"Why this original input is a distinct task; uncertainty or a busy worker is not a reason"},"context_selectors":{"type":"array","items":{"type":"string"},"maxItems":5}},"required":["input_ref","name","reason","context_selectors"],"additionalProperties":false})
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Create {
    #[serde(alias = "input_update")]
    input_ref: InputReference,
    name: String,
    reason: String,
    context_selectors: Vec<String>,
}

/// Only fixed, non-sensitive pre-admission diagnostics cross the tool boundary.
#[derive(Debug)]
pub(super) struct PreflightRejection(pub &'static str, pub &'static str);
impl std::fmt::Display for PreflightRejection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.1)
    }
}
impl std::error::Error for PreflightRejection {}

pub(super) fn rejection(error: &anyhow::Error) -> Option<Value> {
    error.downcast_ref::<PreflightRejection>().map(|e| json!({
        "kind":"creation_rejected","code":e.0,"stage":"before_intent",
        "message":e.1,"effect":"This call made no creation intent or topic mutation. Inspect creation_status for this original before correcting arguments; a prior or concurrent call may still exist."
    }))
}

fn validate_name(name: &str) -> Result<()> {
    let reserved = name
        .strip_prefix('t')
        .or_else(|| name.strip_prefix('T'))
        .is_some_and(|s| s.parse::<i64>().is_ok());
    if name.is_empty()
        || name.len() > 64
        || name.trim() != name
        || reserved
        || name.chars().any(|c| {
            c.is_control()
                || (c.is_whitespace() && c != ' ')
                || matches!(c, '#' | '`' | '"' | '\'' | '<' | '>')
        })
    {
        return Err(PreflightRejection("invalid_creation_title", "Use a 1-64 byte topic title. Ordinary spaces are supported; surrounding whitespace, controls, markup and reserved tID titles are not.").into());
    }
    Ok(())
}

/// Keep the selected captured revisions, not an unbounded copy of their text.
/// Entry positions remain intact so the card's support indices stay meaningful.
fn bound_interview_context(context: Value) -> Result<Value> {
    let limit = crate::store::Store::CREATION_CONTEXT_MAX_BYTES;
    if context.to_string().len() <= limit {
        return Ok(context);
    }
    for supported_cap in [2048, 1024, 512, 256, 128] {
        let mut bounded = context.clone();
        for source in bounded["sources"]
            .as_array_mut()
            .context("Interview sources unavailable")?
        {
            let supports = source["interpretation"]["supports"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_u64)
                .filter_map(|index| usize::try_from(index).ok())
                .collect::<std::collections::HashSet<_>>();
            let pinned = &mut source["pinned_originals"];
            let original = pinned.to_string();
            let mut index = 0;
            let mut truncated = Vec::new();
            for field in ["entries", "in_progress_entries"] {
                for entry in pinned[field].as_array_mut().into_iter().flatten() {
                    let cap = if supports.contains(&index) {
                        supported_cap
                    } else {
                        supported_cap.min(256)
                    };
                    if let Some(text) = entry.as_str()
                        && text.chars().count() > cap
                    {
                        *entry = Value::String(text.chars().take(cap).collect());
                        truncated.push(index);
                    }
                    index += 1;
                }
            }
            if !truncated.is_empty() {
                pinned["bounded"] = json!(true);
                pinned["text_truncated"] = json!(true);
                pinned["interview_excerpt"] = json!({
                    "captured_source_sha256":format!("{:x}",Sha256::digest(original.as_bytes())),
                    "captured_source_json_bytes":original.len(),
                    "supported_entry_character_cap":supported_cap,
                    "other_entry_character_cap":supported_cap.min(256),
                    "truncated_entry_indices":truncated,
                    "meaning":"Additional bounded interview excerpts. Entry positions and captured source identity are preserved; the full selected source snapshot remains retained. These excerpts are not the full observer input or live source state."
                });
            }
        }
        if bounded.to_string().len() <= limit {
            return Ok(bounded);
        }
    }
    bail!("Selected interview metadata exceed the context budget; select fewer sources")
}

pub(super) async fn create(
    app: &App,
    key: SessionKey,
    human: i64,
    native: &str,
    args: Value,
    closed: &CancellationToken,
) -> Result<Value> {
    create_selected(app, key, human, native, args, closed, false).await
}

pub(super) async fn interview(
    app: &App,
    key: SessionKey,
    human: i64,
    native: &str,
    args: Value,
    closed: &CancellationToken,
) -> Result<Value> {
    create_selected(app, key, human, native, args, closed, true).await
}

async fn create_selected(
    app: &App,
    key: SessionKey,
    human: i64,
    native: &str,
    args: Value,
    closed: &CancellationToken,
    interview: bool,
) -> Result<Value> {
    let args: Create = serde_json::from_value(args)?;
    if interview && args.context_selectors.is_empty() {
        bail!("An interview requires at least one selected observer source");
    }
    let shared = &app.shared;
    let cfg = super::routing::profile(shared)?;
    validate_name(&args.name)?;
    if !(12..=1000).contains(&args.reason.len()) || args.context_selectors.len() > 5 {
        return Err(PreflightRejection(
            "invalid_creation_arguments",
            "Use a 12-1000 byte reason and at most five context selectors.",
        )
        .into());
    }
    if cfg.targets.len() >= 64
        || !cfg.resolve(&args.name).is_empty()
        || cfg
            .matrix_targets
            .iter()
            .any(|target| target.name.eq_ignore_ascii_case(&args.name))
    {
        return Err(PreflightRejection("destination_unavailable", "The title already selects a destination or destination capacity is full. Inspect candidates; do not create a duplicate.").into());
    }
    args.input_ref.validate()?;
    let (original, message) = shared.store.interaction_entry_original(
        key,
        human,
        native,
        &args.input_ref,
        &format!("telegram:{}", shared.bot_id),
    )?;
    if original.text.trim_start().starts_with('/')
        || message.as_ref().is_some_and(|message| {
            !(is_text_only_steer_candidate(message, &original.text)
                || original.transcription.is_some())
                || (message.chat.kind != "private" && !message.chat.is_forum.unwrap_or(false))
        })
    {
        bail!("Creation requires committed text and supported topics");
    }
    let mut selected = vec![];
    let mut context = vec![];
    for selector in &args.context_selectors {
        let matches = cfg.resolve(selector.trim_start_matches('#'));
        if matches.len() != 1 || selected.contains(&matches[0]) {
            bail!("Select distinct owned context sources");
        }
        let target = matches[0];
        selected.push(target);
        let observed = shared
            .store
            .interaction_observation(target, human)?
            .context("Context source unavailable")?;
        let (card, freshness) = observed.semantic.context(
            "Selected context has no observer evidence; inspect or refresh it before creation",
        )?;
        let mut source = json!({"source":target,"interpretation":card,"freshness":freshness.state,"source_checked_at":freshness.checked_at,"authority":"Selected attributed observer interpretation, not original human speech, verified completion or an instruction to repeat source effects"});
        if interview {
            // Retain the actual source window that supported this revision,
            // rather than asking an independent interview to rely on a summary
            // alone. This reads no new native history and never resumes work.
            let originals =
                shared
                    .store
                    .observer_originals(target, human, &card.native, card.revision)?;
            source["source_session"] = originals["source_session"].clone();
            source["source_generation_at_selection"] =
                originals["source_generation_at_read"].clone();
            source["source_selected_at"] = originals["source_binding_observed_at"].clone();
            source["pinned_originals"] = originals["source"].clone();
            source["local_work_running_at_selection"] = json!(
                observed
                    .local_turn
                    .as_ref()
                    .is_some_and(|(_, status, _)| status == "running")
            );
        }
        context.push(source);
    }
    // Context is frozen in one intent. The authoritative initial instruction is
    // still the exact retained original message, never model-authored text.
    let mut context = json!({"schema":"telecodex.creation-context/v1","sources":context,"bounded":true,"attribution":"Selected interpretations of other conversations; not firsthand experience. Original input is relayed separately."});
    if interview {
        context["purpose"] = json!("interview");
        context["derivation"] = json!("pinned_observer_sources");
        context["exact_native_fork"] = json!(false);
        context["limits"] = json!(
            "An independent interview from explicitly selected bounded historical evidence, not an exact native fork or the source's live state. Busy sources continue independently. Later corrections are not silently imported; select a new context explicitly. This does not publish content, transfer pending tasks/tools/goals or authorize source execution."
        );
        context = bound_interview_context(context)?;
    }
    let (operation, fresh) = shared.store.interaction_prepare_creation(
        key,
        human,
        native,
        args.input_ref,
        &args.name,
        &args.reason,
        &context,
        64 - shared
            .config
            .interaction
            .as_ref()
            .context("Missing profile")?
            .targets
            .len(),
        closed,
    )?;
    if !fresh {
        return Ok(operation);
    }
    execute(app, key, human, native, operation).await
}

pub(super) async fn resume(
    app: &App,
    key: SessionKey,
    human: i64,
    native: &str,
    reference: InputReference,
    closed: &CancellationToken,
) -> Result<Value> {
    let (operation, claimed) = app
        .shared
        .store
        .interaction_creation_resume(key, human, reference, closed)?;
    if !claimed {
        return Ok(operation);
    }
    execute(app, key, human, native, operation).await
}

async fn execute(
    app: &App,
    key: SessionKey,
    human: i64,
    native: &str,
    operation: Value,
) -> Result<Value> {
    let id = operation["id"]
        .as_str()
        .context("Missing creation identity")?;
    let result = execute_claimed(app, key, human, native, &operation).await;
    if result.is_err() {
        app.shared.store.interaction_creation_suspend(id, human)?;
    }
    result
}

async fn execute_claimed(
    app: &App,
    key: SessionKey,
    human: i64,
    native: &str,
    operation: &Value,
) -> Result<Value> {
    let shared = &app.shared;
    let id = operation["id"]
        .as_str()
        .context("Missing creation identity")?;
    let intent = &operation["intent"];
    let name = intent["name"].as_str().context("Missing retained name")?;
    let reference: InputReference = serde_json::from_value(intent["input_update"].clone())?;
    let topic = if operation["phase"] == "prepared" {
        let topic = shared
            .telegram
            .create_forum_topic_checked(key.chat_id, name, || {
                shared.store.interaction_creation_start(id, human)
            })
            .await?;
        app.record_created_topic(key.chat_id, &topic);
        // A confirmed effect is retained even if its source retired meanwhile.
        shared
            .store
            .interaction_creation_topic(id, human, topic.message_thread_id)?;
        topic.message_thread_id
    } else if operation["phase"] == "topic_known" {
        operation["topic"]
            .as_i64()
            .context("Confirmed topic missing")?
    } else {
        bail!("Creation is not claimed for execution");
    };
    let mut template = shared
        .store
        .get_session(key)?
        .context("Entry unavailable")?;
    if template.codex_thread_id.as_deref() != Some(native) {
        bail!("Entry rebound; known topic retained without new native work");
    }
    template.codex_thread_id = None;
    template.force_fresh_thread = true;
    template.session_title = Some(name.into());
    if intent["context"]["purpose"] == "interview" {
        template.session_prompt = Some(format!(
            "{}\n\nThis is an independent interview about the explicitly selected observer finding. The pinned original source window (or explicitly marked excerpts) and derived observer card are attributed context, not your firsthand experience or live source state. Respect any additional interview excerpt bounds; the full captured source remains retained separately. Explain what the supplied evidence supports, preserve uncertainty and ask for missing context. Source work, tools and goals are not transferred. Do not continue or modify the source task, publish its content or claim an exact native fork. Later source changes remain separate unless explicitly selected.",
            template.session_prompt.as_deref().unwrap_or_default()
        ));
    }
    let target = SessionKey::new(key.chat_id, Some(topic));
    app.initialize_topic_session(target, human, &template)?;
    let instruction = shared.store.interaction_creation_finish(
        id,
        human,
        &format!("telegram:{}", shared.bot_id),
    )?;
    let error = super::dialogue::dispatch_original(app, &instruction.id, human, &reference)
        .await
        .err()
        .map(|_| "Native delivery unconfirmed; inspect retained input, do not replay");
    Ok(
        json!({"kind":"created_destination","operation":shared.store.interaction_creation(id,human)?,"selector":format!("t{topic}"),"instruction":instruction.id,"dispatch_error":error,"incorporation":"Creation and transport acceptance do not establish worker incorporation or completion"}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titles_preserve_human_spaces_and_rejections_are_safe() {
        for name in ["Diamond Badge", "Research", "共同 調査"] {
            validate_name(name).unwrap();
        }
        for name in [
            "",
            " padded",
            "trailing ",
            "t77",
            "bad\nname",
            "bad\tname",
            "<tag>",
            "#route",
        ] {
            let error = validate_name(name).unwrap_err();
            let result = rejection(&error).unwrap();
            assert_eq!(result["code"], "invalid_creation_title");
            assert_eq!(result["stage"], "before_intent");
        }
        assert!(validate_name(&"x".repeat(65)).is_err());
        assert!(rejection(&anyhow::anyhow!("private arbitrary failure")).is_none());
    }

    #[test]
    fn bounded_interview_keeps_source_identity_indices_and_small_originals() {
        let sources = (0..5)
            .map(|index| {
                json!({
                    "source_session":index,
                    "interpretation":{"revision":3,"supports":[2,3]},
                    "pinned_originals":{
                        "revision":format!("captured-{index}"),"boundary":"completed-cut",
                        "turn_ids":["a","b","c"],
                        "entries":["Other original evidence. ".repeat(4000),"Short original.","支持 evidence \"quoted\"\n".repeat(4000)],
                        "in_progress_entries":["Provisional 知見. ".repeat(4000)],
                        "bounded":false,"text_truncated":false
                    }
                })
            })
            .collect::<Vec<_>>();
        let original = json!({"purpose":"interview","exact_native_fork":false,"sources":sources});
        let bounded = bound_interview_context(original.clone()).unwrap();
        assert!(bounded.to_string().len() <= crate::store::Store::CREATION_CONTEXT_MAX_BYTES);
        for (before, after) in original["sources"]
            .as_array()
            .unwrap()
            .iter()
            .zip(bounded["sources"].as_array().unwrap())
        {
            assert_eq!(before["interpretation"], after["interpretation"]);
            let source = &after["pinned_originals"];
            assert_eq!(source["revision"], before["pinned_originals"]["revision"]);
            assert_eq!(source["turn_ids"], before["pinned_originals"]["turn_ids"]);
            assert_eq!(source["entries"].as_array().unwrap().len(), 3);
            assert_eq!(source["entries"][1], "Short original.");
            assert!(
                source["entries"][2]
                    .as_str()
                    .unwrap()
                    .starts_with("支持 evidence")
            );
            assert_eq!(source["in_progress_entries"].as_array().unwrap().len(), 1);
            assert_eq!(source["bounded"], true);
            assert_eq!(source["text_truncated"], true);
            assert_eq!(
                source["interview_excerpt"]["truncated_entry_indices"],
                json!([0, 2, 3])
            );
            assert_eq!(
                source["interview_excerpt"]["captured_source_sha256"],
                format!(
                    "{:x}",
                    Sha256::digest(before["pinned_originals"].to_string().as_bytes())
                )
            );
        }
    }

    #[test]
    fn small_interview_context_is_unchanged() {
        let context = json!({"sources":[{"pinned_originals":{"entries":["Exact source"]}}]});
        assert_eq!(bound_interview_context(context.clone()).unwrap(), context);
    }
}
