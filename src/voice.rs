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
    /// Read an already completed, source-bound transcript. Must never initiate ASR.
    #[serde(default)]
    pub completed_transcription_command: Option<Vec<String>>,
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
        if let Some(command) = &self.completed_transcription_command {
            if command.is_empty()
                || command.len() > 32
                || command
                    .iter()
                    .any(|a| a.is_empty() || a.len() > 4096 || a.contains('\0'))
            {
                bail!("Invalid completed-transcript reader command");
            }
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub telegram_file_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capture_id: Option<String>,
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
    transcribe_committed(config, path, Some(file_id), None, None).await
}

pub async fn transcribe_committed(
    config: &VoiceConfig,
    path: &Path,
    telegram_file_id: Option<&str>,
    capture_id: Option<&str>,
    expected_sha256: Option<&str>,
) -> Result<(String, TranscriptSource)> {
    if telegram_file_id.is_some() == capture_id.is_some() {
        bail!("Voice requires one original identity");
    }
    let preparation_started = std::time::Instant::now();
    let bytes = read_media(path).await?;
    if bytes.is_empty() || bytes.len() > MAX_BYTES {
        bail!("Voice media size outside bounds");
    }
    let hash = format!("{:x}", Sha256::digest(&bytes));
    if expected_sha256.is_some_and(|expected| expected != hash) {
        bail!("Committed voice bytes changed before preparation");
    }
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
    Ok((text,TranscriptSource{audio_sha256:hash,telegram_file_id:telegram_file_id.map(str::to_owned),capture_id:capture_id.map(str::to_owned),configured_transcriber:config.transcriber_label.clone(),committed_at:chrono::Utc::now().to_rfc3339(),audio_millis:(pcm.len()/4) as u64*1000/16000,preparation_millis:preparation_started.elapsed().as_millis() as u64,transcription_millis,interpretation:"Machine transcription of the original human voice clip; not exact verified written speech".into()}))
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

/// Remove only the exact owned download whose interpretation was durably committed.
/// The caller must commit text/provenance first. Cleanup failure never retries ASR.
#[cfg(unix)]
pub fn retire_transcribed_clip(path: &Path, expected_sha256: &str) -> Result<()> {
    use std::io::Read;
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
    let mut file = match std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    let info = file.metadata()?;
    if !info.is_file()
        || info.uid() != unsafe { libc::geteuid() }
        || info.mode() & 0o077 != 0
        || info.len() > MAX_BYTES as u64
    {
        bail!("Committed voice cleanup requires a bounded private owner file");
    }
    let mut bytes = Vec::new();
    file.by_ref()
        .take(MAX_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_BYTES || format!("{:x}", Sha256::digest(&bytes)) != expected_sha256 {
        bail!("Committed voice bytes changed before cleanup");
    }
    let current = std::fs::symlink_metadata(path)?;
    if current.dev() != info.dev() || current.ino() != info.ino() {
        bail!("Committed voice path changed before cleanup");
    }
    std::fs::remove_file(path)?;
    std::fs::File::open(path.parent().context("Voice cleanup parent unavailable")?)?.sync_all()?;
    Ok(())
}

#[cfg(not(unix))]
pub fn retire_transcribed_clip(_path: &Path, _expected_sha256: &str) -> Result<()> {
    bail!("Committed voice cleanup requires Unix file ownership");
}

#[cfg(all(test, unix))]
mod retirement_tests {
    use super::*;
    use std::os::unix::fs::{PermissionsExt, symlink};
    #[test]
    fn committed_payload_disposal_is_exact_and_idempotent() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("voice.ogg");
        std::fs::write(&path, b"owned confirmed clip").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        let hash = format!("{:x}", Sha256::digest(b"owned confirmed clip"));
        assert!(retire_transcribed_clip(&path, &"0".repeat(64)).is_err());
        assert!(path.exists());
        retire_transcribed_clip(&path, &hash).unwrap();
        assert!(!path.exists());
        retire_transcribed_clip(&path, &hash).unwrap();
    }
    #[test]
    fn public_or_linked_sources_are_preserved() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source");
        let link = root.path().join("link");
        std::fs::write(&source, b"private original").unwrap();
        let hash = format!("{:x}", Sha256::digest(b"private original"));
        std::fs::set_permissions(&source, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(retire_transcribed_clip(&source, &hash).is_err());
        std::fs::set_permissions(&source, std::fs::Permissions::from_mode(0o600)).unwrap();
        symlink(&source, &link).unwrap();
        assert!(retire_transcribed_clip(&link, &hash).is_err());
        assert!(source.exists());
    }
}

/// Serialize bounded Telegram media publication; no transcription or work lock.
pub struct DownloadReservation {
    pub directory: std::path::PathBuf,
    _lock: std::fs::File,
}

#[cfg(unix)]
pub fn reserve_telegram_download(database: &Path) -> Result<DownloadReservation> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
    let database = std::path::absolute(database)?;
    let directory = std::path::PathBuf::from(format!("{}.telegram-voice", database.display()));
    match std::fs::DirBuilder::new().mode(0o700).create(&directory) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error.into()),
    }
    let info = std::fs::symlink_metadata(&directory)?;
    if !info.is_dir() || info.uid() != unsafe { libc::geteuid() } || info.mode() & 0o077 != 0 {
        bail!("Voice download storage must be a private owner directory");
    }
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(directory.join("admission.lock"))?;
    let info = lock.metadata()?;
    if !info.is_file() || info.uid() != unsafe { libc::geteuid() } || info.mode() & 0o077 != 0 {
        bail!("Voice download admission lock changed");
    }
    use std::os::fd::AsRawFd;
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
            break;
        }
        let error = std::io::Error::last_os_error();
        if error.kind() != std::io::ErrorKind::WouldBlock || std::time::Instant::now() >= deadline {
            bail!("Voice download storage busy; text remains available");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let mut bytes = 0u64;
    let mut count = 0usize;
    for entry in std::fs::read_dir(&directory)? {
        let entry = entry?;
        let info = match std::fs::symlink_metadata(entry.path()) {
            Ok(info) => info,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        };
        if !info.is_file() || info.uid() != unsafe { libc::geteuid() } || info.mode() & 0o077 != 0 {
            bail!("Voice download storage changed");
        }
        bytes = bytes
            .checked_add(info.len())
            .context("Voice storage size overflow")?;
        if entry.file_name() != "admission.lock" {
            count += 1;
        }
        if count >= 128 || bytes > 256 * 1024 * 1024 - MAX_BYTES as u64 {
            bail!("Voice download storage full; text remains available");
        }
    }
    std::fs::File::open(&directory)?.sync_all()?;
    std::fs::File::open(directory.parent().context("Voice storage parent missing")?)?.sync_all()?;
    Ok(DownloadReservation {
        directory,
        _lock: lock,
    })
}

#[cfg(all(test, unix))]
mod capacity_tests {
    use super::*;
    use std::os::unix::fs::{PermissionsExt, symlink};
    #[test]
    fn download_admission_reserves_maximum_and_releases_only_the_lock() {
        let root = tempfile::tempdir().unwrap();
        let db = root.path().join("state.sqlite");
        let reservation = reserve_telegram_download(&db).unwrap();
        assert!(reserve_telegram_download(&db).is_err());
        let protected = reservation.directory.join("unconfirmed");
        let file = std::fs::File::create(&protected).unwrap();
        std::fs::set_permissions(&protected, std::fs::Permissions::from_mode(0o600)).unwrap();
        file.set_len(256 * 1024 * 1024).unwrap();
        drop(file);
        drop(reservation);
        assert!(reserve_telegram_download(&db).is_err());
        assert!(protected.exists());
        std::fs::remove_file(&protected).unwrap();
        assert!(reserve_telegram_download(&db).is_ok());
    }
    #[test]
    fn relative_database_paths_keep_the_existing_working_directory_resolution() {
        let root = tempfile::tempdir_in(".").unwrap();
        let relative = root
            .path()
            .strip_prefix(std::env::current_dir().unwrap())
            .unwrap_or(root.path());
        assert!(!relative.is_absolute());
        let reservation = reserve_telegram_download(&relative.join("state.sqlite")).unwrap();
        assert!(reservation.directory.is_absolute());
        assert_eq!(
            reservation
                .directory
                .parent()
                .unwrap()
                .canonicalize()
                .unwrap(),
            root.path().canonicalize().unwrap()
        );
    }
    #[test]
    fn tiny_pending_files_and_links_are_preserved_and_refuse_admission() {
        let root = tempfile::tempdir().unwrap();
        let db = root.path().join("state.sqlite");
        let reservation = reserve_telegram_download(&db).unwrap();
        let directory = reservation.directory.clone();
        drop(reservation);
        for index in 0..128 {
            let p = directory.join(index.to_string());
            std::fs::write(&p, b"x").unwrap();
            std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o600)).unwrap();
        }
        assert!(reserve_telegram_download(&db).is_err());
        assert_eq!(std::fs::read_dir(&directory).unwrap().count(), 129);
        for index in 0..128 {
            std::fs::remove_file(directory.join(index.to_string())).unwrap();
        }
        let original = root.path().join("unrelated");
        std::fs::write(&original, b"preserve").unwrap();
        symlink(&original, directory.join("linked")).unwrap();
        assert!(reserve_telegram_download(&db).is_err());
        assert_eq!(std::fs::read(original).unwrap(), b"preserve");
    }
}

#[cfg(not(unix))]
pub fn reserve_telegram_download(_database: &Path) -> Result<DownloadReservation> {
    bail!("Voice download storage currently requires Unix ownership and locking");
}
