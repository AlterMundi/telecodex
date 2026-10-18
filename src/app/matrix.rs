//! Selected cross-body plans. This path retains correlation only; it neither
//! starts receiving inference nor runs a Matrix inbox consumer.
use super::*;
use crate::interaction::MatrixDestination;
use serde::Deserialize;
use serde_json::{Value, json};

pub(super) fn capabilities(shared: &AppShared, value: &mut Value) -> Result<()> {
    let profile = super::routing::profile(shared)?;
    if profile.matrix_targets.is_empty() {
        return Ok(());
    }
    value["operations"]["matrix_request"] = json!({"arguments":{"input_ref":"One retained original integer Telegram reference or conversation UUID","selector":"One explicitly configured Matrix target name","relation":"new, amends or context","requests":"Selected existing Matrix request IDs for amends/context; empty for new"},"description":"Freeze the exact original and return binding for one selected Matrix recipient. Retain the original and, only with an explicitly configured host transport, submit its technical send. This never grants receiving execution approval or proves delivery. No replacement origin, model-selected text, retry, inbox polling or budget is created."});
    value["operations"]["matrix_status"] = json!({"arguments":{"request_id":"Retained Matrix request UUID; omit when selecting message_id","message_id":"Selected Matrix reply event UUID; omit when selecting request_id"},"description":"Inspect existing request, submission and canonical replies only. Do not poll in a tool loop or reissue work."});
    value["matrix_transport"] = json!({"configured":profile.matrix_transport.is_some(),"receiving_execution":"Independent admission required; technical send never activates a model","uncertain":"Inspect retained native send; do not repeat or select an alternate destination"});
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

pub(super) async fn tool(
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
    let selected = app.shared.store.interaction_matrix_prepare(
        key,
        human,
        native,
        &args.input_ref,
        &format!("telegram:{}", app.shared.bot_id),
        &target.name,
        &target.destination,
        relation,
        closed,
    )?;
    let Some(transport) = &profile.matrix_transport else {
        return Ok(selected);
    };
    let id = selected["request"]["id"]
        .as_str()
        .context("Missing Matrix request identity")?;
    let binding = selected["request"]["binding"]
        .as_str()
        .context("Missing Matrix original binding")?;
    if !app
        .shared
        .store
        .interaction_matrix_claim(human, key, id, binding, closed)?
    {
        return app.shared.store.interaction_matrix_status(human, key, id);
    }
    let MatrixDestination::Body {
        being_ref,
        embodiment_id,
    } = &transport.source
    else {
        bail!("Explicit Matrix source body required");
    };
    let source = crate::store::MatrixAuthor {
        being_ref: being_ref.clone(),
        embodiment_id: embodiment_id.clone(),
    };
    let outcome = tokio::select! {
        biased;
        _ = closed.cancelled() => Err(anyhow!("Matrix host request closed; inspect its retained send")),
        _ = app.shared.shutdown.cancelled() => Err(anyhow!("Matrix host shutdown; inspect its retained send")),
        result = host_send(transport, &selected, &source) => result,
    };
    let recorded = outcome.and_then(|submission| {
        app.shared
            .store
            .interaction_matrix_submitted(human, key, id, binding, &source, &submission)
    });
    match recorded {
        Ok(status) => Ok(status),
        Err(_) => {
            app.shared
                .store
                .interaction_matrix_uncertain(human, key, id)?;
            // The SDK journal, not a failed child or model, decides whether a
            // native message exists. Never automatically reset this reservation.
            app.shared.store.interaction_matrix_status(human, key, id)
        }
    }
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

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HostOutcome {
    schema: String,
    request: String,
    binding: String,
    submission: Value,
}

#[cfg(unix)]
struct OwnedMatrixCommand(u32);
#[cfg(unix)]
impl Drop for OwnedMatrixCommand {
    fn drop(&mut self) {
        // Only the process group created for this one configured host command.
        unsafe {
            libc::kill(-(self.0 as i32), libc::SIGKILL);
        }
    }
}

#[cfg(unix)]
async fn host_send(
    config: &crate::interaction::MatrixTransportConfig,
    selected: &Value,
    source: &crate::store::MatrixAuthor,
) -> Result<crate::store::MatrixSubmission> {
    use std::{os::unix::process::CommandExt, process::Stdio, time::Duration};
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        process::Command,
    };
    let raw = serde_json::to_vec(&json!({"status":selected,"source":source}))?;
    if raw.len() > 65536 {
        bail!("Matrix host input exceeds bound");
    }
    let mut command = Command::new(&config.command[0]);
    command.args(&config.command[1..]);
    command.as_std_mut().process_group(0);
    command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    let mut child = command
        .spawn()
        .context("Configured Matrix host unavailable")?;
    let group = OwnedMatrixCommand(child.id().context("Missing Matrix host process")?);
    let mut stdin = child.stdin.take().context("Missing Matrix host input")?;
    let mut stdout = child
        .stdout
        .take()
        .context("Missing Matrix host output")?
        .take(65537);
    let mut output = Vec::new();
    let result = tokio::time::timeout(Duration::from_secs(config.timeout_seconds), async {
        let write = async {
            stdin.write_all(&raw).await?;
            stdin.shutdown().await?;
            drop(stdin);
            Ok::<_, std::io::Error>(())
        };
        let read = async {
            stdout.read_to_end(&mut output).await?;
            Ok::<_, std::io::Error>(())
        };
        tokio::try_join!(write, read)?;
        if output.len() > 65536 || !child.wait().await?.success() {
            bail!("Matrix host outcome unconfirmed");
        }
        let outcome: HostOutcome = serde_json::from_slice(&output)?;
        if outcome.schema != "telecodex.matrix-transport-outcome/v1"
            || outcome.request != selected["request"]["id"]
            || outcome.binding != selected["request"]["binding"]
        {
            bail!("Matrix host outcome differs from the selected request");
        }
        let keys = [
            "send_id",
            "message_id",
            "message_hash",
            "thread_id",
            "author",
            "destination",
        ];
        let submission = keys
            .into_iter()
            .map(|key| (key.to_owned(), outcome.submission[key].clone()))
            .collect::<serde_json::Map<_, _>>();
        Ok(serde_json::from_value(Value::Object(submission))?)
    })
    .await;
    drop(group);
    let _ = child.wait().await;
    result.context("Matrix host deadline expired; no automatic retry")?
}

#[cfg(not(unix))]
async fn host_send(
    _config: &crate::interaction::MatrixTransportConfig,
    _selected: &Value,
    _source: &crate::store::MatrixAuthor,
) -> Result<crate::store::MatrixSubmission> {
    bail!("Matrix host transport unavailable on this platform")
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[tokio::test]
    async fn host_deadline_closes_its_descendants_even_after_parent_exit() {
        let directory = tempfile::tempdir().unwrap();
        let marker = directory.path().join("unexpected-descendant-effect");
        let child = format!(
            "import time,pathlib;time.sleep(1.5);pathlib.Path({:?}).write_text('unexpected')",
            marker.to_string_lossy()
        );
        let parent = format!(
            "import subprocess;subprocess.Popen(['python3','-c',{:?}])",
            child
        );
        let source = crate::store::MatrixAuthor {
            being_ref: format!("dm:being:v1:{}", "A".repeat(43)),
            embodiment_id: "embodiment:source".into(),
        };
        let config = crate::interaction::MatrixTransportConfig {
            command: vec!["/usr/bin/env".into(), "python3".into(), "-c".into(), parent],
            source: MatrixDestination::Body {
                being_ref: source.being_ref.clone(),
                embodiment_id: source.embodiment_id.clone(),
            },
            timeout_seconds: 1,
        };
        assert!(host_send(&config, &json!({}), &source).await.is_err());
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        assert!(!marker.exists());
    }
}
