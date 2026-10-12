//! Connect-only owner-local client. It starts no listener or model turn.
use anyhow::{Context, Result, bail};
#[cfg(unix)]
pub async fn run(config: &crate::config::Config, command: &str) -> Result<()> {
    use std::os::unix::fs::{FileTypeExt, MetadataExt};
    use tokio::{
        io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
        net::UnixStream,
    };
    let cfg = config
        .session_control
        .as_ref()
        .context("No receiving session_control configured")?;
    let meta = std::fs::symlink_metadata(&cfg.socket_path)?;
    if !meta.file_type().is_socket()
        || meta.uid() != unsafe { libc::geteuid() }
        || meta.mode() & 0o077 != 0
    {
        bail!("Unsafe receiving control socket");
    }
    tokio::time::timeout(std::time::Duration::from_secs(30), async {
        let mut stream = UnixStream::connect(&cfg.socket_path).await?;
        if stream.peer_cred()?.uid() != unsafe { libc::geteuid() } {
            bail!("Receiving control peer owner mismatch");
        }
        let nonce = uuid::Uuid::now_v7().to_string();
        let mut request =
            serde_json::to_vec(&serde_json::json!({"command":command,"nonce":nonce}))?;
        request.push(b'\n');
        stream.write_all(&request).await?;
        let mut bytes = Vec::new();
        BufReader::new(stream.take(65537))
            .read_until(b'\n', &mut bytes)
            .await?;
        if bytes.len() > 65536 {
            bail!("Control reply exceeds bound");
        }
        let reply: serde_json::Value = serde_json::from_slice(&bytes)?;
        println!("{}", serde_json::to_string_pretty(&reply)?);
        if reply["ok"] != true {
            bail!("Receiving control did not confirm success");
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
