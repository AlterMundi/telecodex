//! Explicit owner-local transport to the existing Nexus conversation API.
use crate::models::SessionKey;
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{path::PathBuf, process::Stdio, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    process::Command,
};

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NexusConfig {
    pub python: PathBuf,
    pub operator: PathBuf,
    pub workspace: PathBuf,
    pub state: PathBuf,
    pub listener_id: String,
    pub responder_ref: String,
    pub launch_selector: String,
    pub routes: Vec<NexusRoute>,
    #[serde(default = "default_poll")]
    pub poll_seconds: u64,
}
fn default_poll() -> u64 {
    3
}
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NexusRoute {
    pub table_id: String,
    pub author_ref: String,
    pub human_user_id: i64,
    pub label: String,
    #[serde(default)]
    pub launch_selector: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct NexusTarget {
    pub listener_id: String,
    pub bot_id: i64,
    pub human_user_id: i64,
    pub chat_id: i64,
    pub topic_id: i64,
    pub native_thread_id: String,
}
impl NexusTarget {
    pub fn key(&self) -> SessionKey {
        SessionKey::new(self.chat_id, Some(self.topic_id))
    }
}
pub fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 160
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"._:-".contains(&c))
}
fn launch_selector(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c))
}
impl NexusConfig {
    pub fn launch_for<'a>(&'a self, route: &'a NexusRoute) -> &'a str {
        route
            .launch_selector
            .as_deref()
            .unwrap_or(&self.launch_selector)
    }
    pub fn validate(&self) -> Result<()> {
        if [&self.python, &self.operator, &self.workspace, &self.state]
            .iter()
            .any(|p| !p.is_absolute())
            || !identifier(&self.listener_id)
            || !identifier(&self.responder_ref)
            || !launch_selector(&self.launch_selector)
            || self.poll_seconds == 0
            || self.poll_seconds > 60
            || self.routes.is_empty()
            || self.routes.len() > 100
        {
            bail!("invalid Nexus operator configuration");
        }
        for (i, route) in self.routes.iter().enumerate() {
            if !identifier(&route.table_id)
                || !identifier(&route.author_ref)
                || route.human_user_id <= 0
                || route.label.is_empty()
                || route.label.len() > 200
                || !launch_selector(self.launch_for(route))
                || self.routes[..i]
                    .iter()
                    .any(|r| r.table_id == route.table_id && r.author_ref == route.author_ref)
            {
                bail!("invalid or duplicate Nexus route");
            }
        }
        Ok(())
    }
    pub async fn call(&self, operation: &str, input: &Value) -> Result<Value> {
        if !matches!(
            operation,
            "binding"
                | "configure"
                | "pending"
                | "inspect"
                | "dispatch"
                | "receipt"
                | "capture-input"
                | "current-visits"
        ) {
            bail!("unsupported Nexus operation");
        }
        let raw = serde_json::to_vec(input)?;
        if raw.len() > 512 * 1024 {
            bail!("Nexus operator input too large");
        }
        let mut child = Command::new(&self.python)
            .arg("-B")
            .arg(&self.operator)
            .arg("--workspace")
            .arg(&self.workspace)
            .arg("--state")
            .arg(&self.state)
            .arg(operation)
            .current_dir(&self.workspace)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .context("Nexus operator unavailable")?;
        let timeout = if operation == "capture-input" { 2 } else { 10 };
        let result = tokio::time::timeout(Duration::from_secs(timeout), async {
            let mut stdin = child.stdin.take().context("Nexus input pipe unavailable")?;
            stdin.write_all(&raw).await?;
            drop(stdin);
            let mut stdout = child
                .stdout
                .take()
                .context("Nexus output pipe unavailable")?
                .take(512 * 1024 + 1);
            let mut output = Vec::new();
            stdout.read_to_end(&mut output).await?;
            if output.len() > 512 * 1024 {
                bail!("Nexus operator output too large");
            }
            if !child.wait().await?.success() {
                bail!("Nexus operation rejected or unconfirmed; inspect its retained state");
            }
            Ok(serde_json::from_slice(&output)?)
        })
        .await
        .context("Nexus acknowledgement timed out; no automatic native retry")?;
        if result.is_err() {
            let _ = child.kill().await;
        }
        result
    }
}
