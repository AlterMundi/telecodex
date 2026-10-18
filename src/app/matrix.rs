//! Selected cross-body plans. This path retains correlation only; it neither
//! starts receiving inference nor runs a Matrix inbox consumer.
use super::*;
use serde::Deserialize;
use serde_json::{Value, json};

pub(super) fn capabilities(shared: &AppShared, value: &mut Value) -> Result<()> {
    let profile = super::routing::profile(shared)?;
    if profile.matrix_targets.is_empty() {
        return Ok(());
    }
    value["operations"]["matrix_request"] = json!({"arguments":{"input_ref":"One retained original integer Telegram reference or conversation UUID","selector":"One explicitly configured Matrix target name","relation":"new, amends or context","requests":"Selected existing Matrix request IDs for amends/context; empty for new"},"description":"Freeze the exact original and return binding for one selected Matrix recipient. This prepares a passive host request, not receiving execution approval or delivery. No replacement origin, model-selected text, retry, inbox polling or budget is created."});
    value["operations"]["matrix_status"] = json!({"arguments":{"request_id":"Retained Matrix request UUID; omit when selecting message_id","message_id":"Selected Matrix reply event UUID; omit when selecting request_id"},"description":"Inspect existing request, submission and canonical replies only. Do not poll in a tool loop or reissue work."});
    value["matrix_targets"] = json!(
        profile
            .matrix_targets
            .iter()
            .map(|target| json!({"name":target.name,"destination":target.destination}))
            .collect::<Vec<_>>()
    );
    value["unsupported"] = json!(["automatic peer activation", "global Stop"]);
    Ok(())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Prepare {
    input_ref: crate::interaction::InputReference,
    selector: String,
    relation: String,
    requests: Vec<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Inspect {
    request_id: Option<String>,
    message_id: Option<String>,
}

pub(super) fn tool(
    app: &App,
    key: SessionKey,
    human: i64,
    native: &str,
    args: Value,
    closed: &CancellationToken,
    prepare: bool,
) -> Result<Value> {
    let profile = super::routing::profile(&app.shared)?;
    if profile.entry() != key || profile.human_id != human || profile.matrix_targets.is_empty() {
        bail!("Selected Matrix routing is not configured for this integrating entry");
    }
    if !prepare {
        let args: Inspect = serde_json::from_value(args)?;
        return match (args.request_id, args.message_id) {
            (Some(id), None) => app.shared.store.interaction_matrix_status(human, key, &id),
            (None, Some(id)) => app
                .shared
                .store
                .interaction_matrix_message_status(human, key, &id),
            _ => bail!("Select exactly one retained Matrix request or reply message"),
        };
    }
    let args: Prepare = serde_json::from_value(args)?;
    let matches = profile
        .matrix_targets
        .iter()
        .filter(|t| t.name.eq_ignore_ascii_case(&args.selector))
        .collect::<Vec<_>>();
    if matches.len() != 1 {
        bail!("Select one explicitly configured Matrix recipient");
    }
    let target = matches[0];
    let relation = match args.relation.as_str() {
        "new" if args.requests.is_empty() => crate::interaction::RequestRelation::New,
        "amends" if !args.requests.is_empty() => {
            crate::interaction::RequestRelation::Amends(args.requests)
        }
        "context" if !args.requests.is_empty() => {
            crate::interaction::RequestRelation::Context(args.requests)
        }
        _ => bail!("Explicit Matrix request relation required"),
    };
    app.shared.store.interaction_matrix_prepare(
        key,
        human,
        native,
        &args.input_ref,
        &format!("telegram:{}", app.shared.bot_id),
        &target.name,
        &target.destination,
        relation,
        closed,
    )
}

pub(super) fn owner(app: &App, human: i64, args: &str) -> Result<Value> {
    let cfg = app
        .shared
        .config
        .interaction
        .as_ref()
        .context("Integrating entry unavailable")?;
    if human != cfg.human_id {
        bail!("Matrix origin owner differs from the authenticated controller");
    }
    if let Some(id) = args.strip_prefix("status ") {
        return app
            .shared
            .store
            .interaction_matrix_status(human, cfg.entry(), id);
    }
    if let Some(body) = args.strip_prefix("return ") {
        if body.len() > 64000 {
            bail!("Matrix return metadata exceeds bounded input");
        }
        let value: crate::store::MatrixReturn = serde_json::from_str(body)?;
        return app
            .shared
            .store
            .interaction_matrix_return(human, cfg.entry(), &value);
    }
    bail!("Expected Matrix status UUID or trusted-host return JSON")
}
