//! Scoped live-turn fallback for native threads with persisted older tool sets.
//! Uses the same controller handlers; neither owner commands nor fake human input.
use super::*;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone)]
pub(super) struct Lease {
    key: SessionKey,
    human: i64,
    local_turn: i64,
    physical: Arc<StdMutex<Option<String>>>,
    closed: CancellationToken,
}

pub(super) struct Guard {
    shared: Arc<AppShared>,
    token: String,
    file: PathBuf,
    closed: CancellationToken,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Access {
    pub schema: String,
    pub socket: PathBuf,
    pub token: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Call {
    token: String,
    operation: String,
    arguments: Value,
}

impl Guard {
    pub(super) fn prepare(
        shared: Arc<AppShared>,
        request: &mut TurnRequest,
        local_turn: i64,
        workspace: &TurnWorkspace,
        physical: Arc<StdMutex<Option<String>>>,
        cancel: &CancellationToken,
        selected: bool,
    ) -> Result<Option<Self>> {
        if !selected {
            return Ok(None);
        }
        let Some(config) = shared.config.session_control.as_ref() else {
            // Existing dynamic tools still work. Do not invent an available fallback.
            return Ok(None);
        };
        let token = format!("{}{}", Uuid::now_v7().simple(), Uuid::now_v7().simple());
        let file = workspace.root.join("controller-access.json");
        let access = Access {
            schema: "telecodex.turn-controller/v1".into(),
            socket: config.socket_path.clone(),
            token: token.clone(),
        };
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
        }
        use std::io::Write;
        let mut output = options.open(&file)?;
        output.write_all(&serde_json::to_vec(&access)?)?;
        let closed = cancel.child_token();
        shared
            .controller_leases
            .lock()
            .expect("controller leases poisoned")
            .insert(
                token.clone(),
                Lease {
                    key: request.session_key,
                    human: request.from_user_id,
                    local_turn,
                    physical,
                    closed: closed.clone(),
                },
            );
        let guard = Self {
            shared,
            token,
            file,
            closed,
        };
        let executable = std::env::current_exe()?;
        request.runtime_instructions.get_or_insert_with(String::new).push_str(&format!(
            "\n\nLive controller fallback when the dynamic tool is absent: {} --controller {} '{{\"operation\":\"capabilities\",\"arguments\":{{}}}}'. Use the same command with the selected operation/arguments JSON. Same handlers, scoped to this execution. Do not reveal/copy the private access file or token. Read actual outcomes; never poll helpers or replay uncertain operations.",
            shell_arg(&executable), shell_arg(&guard.file)
        ));
        Ok(Some(guard))
    }
}
fn shell_arg(path: &Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', "'\"'\"'"))
}
impl Drop for Guard {
    fn drop(&mut self) {
        self.closed.cancel();
        self.shared
            .controller_leases
            .lock()
            .expect("controller leases poisoned")
            .remove(&self.token);
        let _ = fs::remove_file(&self.file);
    }
}

pub(super) async fn command(app: &App, human: i64, raw: &str) -> Result<Value> {
    if raw.len() > 20000 {
        bail!("Controller request too large");
    }
    let call: Call = serde_json::from_str(raw)?;
    let lease = app
        .shared
        .controller_leases
        .lock()
        .expect("controller leases poisoned")
        .get(&call.token)
        .cloned()
        .context("Live controller access expired")?;
    if human != lease.human || lease.closed.is_cancelled() {
        bail!("Live controller access closed");
    }
    let native = app
        .shared
        .store
        .controller_native_turn(lease.local_turn, lease.key, human)?;
    let physical = lease
        .physical
        .lock()
        .expect("physical turn poisoned")
        .clone()
        .context("Native turn has not started")?;
    let request = crate::codex::DynamicToolRequest {
        thread_id: native,
        turn_id: physical,
        call_id: Uuid::now_v7().to_string(),
        tool: "interaction_controller".into(),
        namespace: None,
        arguments: json!({"operation":call.operation,"arguments":call.arguments}),
    };
    // Same role checks, original input reservations and scoped mutations as dynamic tools.
    let response =
        super::dialogue::tool_call(app.clone(), lease.key, human, request, lease.closed.clone());
    tokio::time::timeout(Duration::from_secs(25), response)
        .await
        .context("Controller outcome unavailable; inspect retained state before recovery")?
        .context("Controller response closed")
}
