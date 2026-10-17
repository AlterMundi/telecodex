//! Committed voice preparation. No microphone, native writer or message admission.
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{path::Path, process::Stdio, time::Duration};
use tokio::{io::AsyncReadExt, process::Command};

pub const MAX_BYTES: usize = 20 * 1024 * 1024;
const MAX_SECONDS: usize = 120;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VoiceConfig {
    /// Owner-configured argv, never a shell fragment or model-provided command.
    pub transcription_command: Vec<String>,
    pub transcriber_label: String,
    #[serde(default = "default_timeout")]
    pub timeout_seconds: u64,
}
fn default_timeout() -> u64 {
    60
}
impl VoiceConfig {
    pub fn validate(&self) -> Result<()> {
        if self.transcription_command.is_empty()
            || self.transcription_command.len() > 32
            || self
                .transcription_command
                .iter()
                .any(|a| a.is_empty() || a.len() > 4096 || a.contains('\0'))
            || self.transcriber_label.trim().is_empty()
            || self.transcriber_label.len() > 128
            || !(1..=120).contains(&self.timeout_seconds)
        {
            bail!("Invalid bounded voice transcriber configuration");
        }
        #[cfg(not(unix))]
        bail!("Voice command lifecycle currently requires Unix process groups");
        #[cfg(unix)]
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranscriptSource {
    pub audio_sha256: String,
    pub telegram_file_id: String,
    pub configured_transcriber: String,
    pub committed_at: String,
    pub audio_millis: u64,
    pub preparation_millis: u64,
    pub transcription_millis: u64,
    pub interpretation: String,
}

#[cfg(unix)]
struct OwnedCommandGroup(u32);
#[cfg(unix)]
impl Drop for OwnedCommandGroup {
    fn drop(&mut self) {
        unsafe {
            libc::kill(-(self.0 as i32), libc::SIGKILL);
        }
    }
}

/// Own the complete command group until bounded completion, including converters.
/// Stopping preparation affects only this group, never playback or native work.
#[cfg(unix)]
async fn bounded(mut command: Command, limit: usize, timeout: u64) -> Result<Vec<u8>> {
    use std::os::unix::process::CommandExt;
    command.as_std_mut().process_group(0);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    let mut child = command.spawn().context("Voice command unavailable")?;
    let id = child.id().context("Missing owned voice process")?;
    let group = OwnedCommandGroup(id);
    let mut stdout = child
        .stdout
        .take()
        .context("Missing voice output")?
        .take(limit as u64 + 1);
    let mut output = vec![];
    let result = tokio::time::timeout(Duration::from_secs(timeout), async {
        stdout.read_to_end(&mut output).await?;
        if output.len() > limit {
            bail!("Voice command output exceeded bound");
        }
        if !child.wait().await?.success() {
            bail!("Voice command did not complete successfully");
        }
        Ok(())
    })
    .await;
    // Also collect descendants left by a failed, timed-out or early-exiting adapter.
    drop(group);
    let _ = child.wait().await;
    result.context("Voice preparation expired; no automatic retry")??;
    Ok(output)
}
#[cfg(not(unix))]
async fn bounded(_command: Command, _limit: usize, _timeout: u64) -> Result<Vec<u8>> {
    bail!("Voice unavailable on this platform")
}

pub async fn transcribe(
    config: &VoiceConfig,
    path: &Path,
    file_id: &str,
) -> Result<(String, TranscriptSource)> {
    let preparation_started = std::time::Instant::now();
    let bytes = read_media(path).await?;
    if bytes.is_empty() || bytes.len() > MAX_BYTES {
        bail!("Voice media size outside bounds");
    }
    let hash = format!("{:x}", Sha256::digest(&bytes));
    // Decode the whole bounded clip before paid ASR. Digital silence is not speech.
    let mut decode = Command::new("ffmpeg");
    decode
        .args(["-v", "error", "-nostdin", "-i"])
        .arg(path)
        .args(["-vn", "-ac", "1", "-ar", "16000", "-f", "f32le", "pipe:1"]);
    let pcm = bounded(decode, MAX_SECONDS * 16000 * 4, 15).await?;
    if pcm.is_empty() || pcm.len() % 4 != 0 {
        bail!("Voice decode incomplete");
    }
    let peak = pcm
        .chunks_exact(4)
        .map(|v| f32::from_le_bytes(v.try_into().expect("sample width")).abs())
        .fold(0f32, f32::max);
    if !peak.is_finite() || peak < 0.0001 {
        bail!("No audible signal; no ASR or work admitted");
    }
    let mut command = Command::new(&config.transcription_command[0]);
    command.args(&config.transcription_command[1..]).arg(path);
    let transcription_started = std::time::Instant::now();
    let output = bounded(command, 64 * 1024, config.timeout_seconds).await?;
    let transcription_millis = transcription_started.elapsed().as_millis() as u64;
    let value: serde_json::Value = serde_json::from_slice(&output)?;
    if value["final"] == false || value["partial"] == true {
        bail!("Transcriber returned a partial interpretation");
    }
    let text = value["text"]
        .as_str()
        .context("Transcriber returned no final text")?
        .trim()
        .to_string();
    if text.is_empty() || text.len() > 16000 {
        bail!("Voice transcript outside bounds");
    }
    if format!("{:x}", Sha256::digest(read_media(path).await?)) != hash {
        bail!("Voice source changed during preparation");
    }
    Ok((text,TranscriptSource{audio_sha256:hash,telegram_file_id:file_id.into(),configured_transcriber:config.transcriber_label.clone(),committed_at:chrono::Utc::now().to_rfc3339(),audio_millis:(pcm.len()/4) as u64*1000/16000,preparation_millis:preparation_started.elapsed().as_millis() as u64,transcription_millis,interpretation:"Machine transcription of the original human voice clip; not exact verified written speech".into()}))
}

async fn read_media(path: &Path) -> Result<Vec<u8>> {
    let mut file = tokio::fs::File::open(path)
        .await?
        .take(MAX_BYTES as u64 + 1);
    let mut bytes = vec![];
    file.read_to_end(&mut bytes).await?;
    if bytes.is_empty() || bytes.len() > MAX_BYTES {
        bail!("Voice media size outside bounds");
    }
    Ok(bytes)
}
