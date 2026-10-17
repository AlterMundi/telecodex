//! Create one justified destination through the existing topic/session adapter.
use super::*;
use crate::interaction::InputReference;
use serde::Deserialize;
use serde_json::{Value, json};

pub(super) fn schema() -> Value {
    json!({"type":"object","properties":{"input_ref":{"oneOf":[{"type":"integer","minimum":0},{"type":"string","format":"uuid"}]},"name":{"type":"string","description":"Short single-token topic selector; stable tID names are reserved"},"reason":{"type":"string","description":"Why this original input is a distinct task; uncertainty or a busy worker is not a reason"},"context_selectors":{"type":"array","items":{"type":"string"},"maxItems":5}},"required":["input_ref","name","reason","context_selectors"],"additionalProperties":false})
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

pub(super) async fn create(
    app: &App,
    key: SessionKey,
    human: i64,
    native: &str,
    args: Value,
    closed: &CancellationToken,
) -> Result<Value> {
    let args: Create = serde_json::from_value(args)?;
    let shared = &app.shared;
    let cfg = super::routing::profile(shared)?;
    let reserved = args
        .name
        .strip_prefix('t')
        .or_else(|| args.name.strip_prefix('T'))
        .is_some_and(|s| s.parse::<i64>().is_ok());
    if args.name.len() > 64
        || reserved
        || args.name.is_empty()
        || args.name.chars().any(|c| {
            c.is_whitespace() || c.is_control() || matches!(c, '#' | '`' | '"' | '\'' | '<' | '>')
        })
        || !(12..=1000).contains(&args.reason.len())
        || args.context_selectors.len() > 5
        || cfg.targets.len() >= 64
        || !cfg.resolve(&args.name).is_empty()
    {
        bail!("Invalid, duplicate or unsupported new destination");
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
        context.push(json!({"source":target,"interpretation":card,"freshness":freshness.state,"source_checked_at":freshness.checked_at,"authority":"Selected attributed observer interpretation, not original human speech, verified completion or an instruction to repeat source effects"}));
    }
    // Context is frozen in one intent. The authoritative initial instruction is
    // still the exact retained original message, never model-authored text.
    let context = json!({"schema":"telecodex.creation-context/v1","sources":context,"bounded":true,"attribution":"Selected interpretations of other conversations; not firsthand experience. Original input is relayed separately."});
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
