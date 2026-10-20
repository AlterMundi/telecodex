//! Connect-only owner-local client. It starts no listener or model turn.
use anyhow::{Context, Result, bail};
#[cfg(unix)]
pub async fn run(config: &crate::config::Config, command: &str) -> Result<()> {
    let cfg = config
        .session_control
        .as_ref()
        .context("No receiving session_control configured")?;
    exchange(&cfg.socket_path, command).await
}

/// Read the owner-only turn credential without putting its token in argv/output.
#[cfg(unix)]
pub async fn controller(path: &std::path::Path, payload: &str) -> Result<()> {
    use std::{
        io::Read,
        os::unix::fs::{MetadataExt, OpenOptionsExt},
    };
    if payload.len() > 16000 {
        bail!("Controller arguments exceed bound");
    }
    let mut envelope: serde_json::Value = serde_json::from_str(payload)?;
    let object = envelope
        .as_object_mut()
        .context("Controller envelope required")?;
    if object.len() != 2
        || !object.get("operation").is_some_and(|v| v.is_string())
        || !object.get("arguments").is_some_and(|v| v.is_object())
    {
        bail!("Expected operation and arguments only");
    }
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)?;
    let meta = file.metadata()?;
    if !meta.is_file()
        || meta.uid() != unsafe { libc::geteuid() }
        || meta.mode() & 0o077 != 0
        || meta.len() > 16384
    {
        bail!("Unsafe live controller credential");
    }
    let mut bytes = Vec::new();
    file.take(16385).read_to_end(&mut bytes)?;
    let access: crate::app::controller::Access = serde_json::from_slice(&bytes)?;
    if access.schema != "telecodex.turn-controller/v1" || access.token.len() != 64 {
        bail!("Unsupported live controller credential");
    }
    object.insert("token".into(), serde_json::json!(access.token));
    exchange(&access.socket, &format!("controller {}", envelope)).await
}

#[cfg(unix)]
async fn exchange(path: &std::path::Path, command: &str) -> Result<()> {
    use std::os::unix::fs::{FileTypeExt, MetadataExt};
    use tokio::{
        io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
        net::UnixStream,
    };
    let meta = std::fs::symlink_metadata(path)?;
    if !meta.file_type().is_socket()
        || meta.uid() != unsafe { libc::geteuid() }
        || meta.mode() & 0o077 != 0
    {
        bail!("Unsafe receiving control socket");
    }
    tokio::time::timeout(std::time::Duration::from_secs(30), async {
        let mut stream = UnixStream::connect(path).await?;
        if stream.peer_cred()?.uid() != unsafe { libc::geteuid() } {
            bail!("Receiving control peer owner mismatch");
        }
        let nonce = uuid::Uuid::now_v7().to_string();
        let mut request =
            serde_json::to_vec(&serde_json::json!({"command":command,"nonce":nonce}))?;
        request.push(b'\n');
        stream.write_all(&request).await?;
        let mut bytes = Vec::new();
        BufReader::new(stream.take(262145))
            .read_until(b'\n', &mut bytes)
            .await?;
        if bytes.len() > 262144 {
            bail!("Control reply exceeds bound");
        }
        let reply: serde_json::Value = serde_json::from_slice(&bytes)?;
        println!("{}", serde_json::to_string_pretty(&reply)?);
        if reply["ok"] != true {
            bail!("Receiving control did not confirm success");
        }
        if reply["result"]["success"] == false {
            bail!("Selected controller operation was refused");
        }
        Ok(())
    })
    .await
    .context("Control acceptance unavailable; inspect the retained operation before retrying")?
}
#[cfg(not(unix))]
pub async fn run(_config: &crate::config::Config, _command: &str) -> Result<()> {
    bail!("Owner-local session control requires Unix")
}
#[cfg(not(unix))]
pub async fn controller(_path: &std::path::Path, _payload: &str) -> Result<()> {
    bail!("Live turn controller requires Unix")
}
