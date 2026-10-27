//! Capture only files from the directory created for this actual execution.
use super::*;

pub(super) fn anchor(path: &Path) -> Result<Option<Arc<fs::File>>> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        Ok(Some(Arc::new(
            fs::OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW)
                .open(path)?,
        )))
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Ok(None)
    }
}

/// Persist the source identity before releasing the native session. No file
/// bytes, storage lock or secondary delivery are awaited by the conversation.
pub(super) fn retain(
    shared: Arc<AppShared>,
    local: i64,
    human: i64,
    workspace: &TurnWorkspace,
) -> Result<bool> {
    if !shared
        .config
        .interaction
        .as_ref()
        .is_some_and(|c| c.human_id == human && c.artifact_store.is_some())
    {
        return Ok(false);
    }
    let Some(mut output) = shared.store.artifact_output(local, human)? else {
        return Ok(false);
    };
    if fs::read_dir(&workspace.out_dir)?.next().is_none() {
        return Ok(false);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let source = workspace
            .out_anchor
            .as_ref()
            .context("Missing output directory anchor")?
            .metadata()?;
        // Freeze a bounded selection at this canonical final. Later goal cycles
        // must not add or replace files attributed to this response.
        let entries = fs::read_dir(&workspace.out_dir)?.collect::<std::io::Result<Vec<_>>>()?;
        if entries.len() > 10 {
            bail!("At most ten generated files per execution are supported");
        }
        let mut files = serde_json::Map::new();
        for entry in entries {
            let meta = fs::symlink_metadata(entry.path())?;
            if let Some(name) = entry.file_name().to_str() {
                files.insert(
                    name.to_owned(),
                    serde_json::json!([
                        meta.dev(),
                        meta.ino(),
                        meta.len(),
                        meta.mtime(),
                        meta.mtime_nsec(),
                        meta.ctime(),
                        meta.ctime_nsec()
                    ]),
                );
            }
        }
        output["files"] = serde_json::Value::Object(files);
        let source_key = shared.store.native_interaction_source(local, human)?.0;
        let publish_files = super::topic_publications::enabled(&shared, source_key, human)
            && shared.store.topic_projection_active(local)?;
        shared.store.artifact_job_stage(
            local,
            human,
            &serde_json::json!({
                "out":workspace.out_dir,"device":source.dev(),"inode":source.ino(),"output":output,"publish_files":publish_files
            }),
        )?;
        recover(shared)?;
        Ok(true)
    }
    #[cfg(not(unix))]
    {
        let _ = (output, workspace);
        bail!("Retained artifacts require descriptor-relative file access")
    }
}

pub(super) fn recover(shared: Arc<AppShared>) -> Result<()> {
    let Some(config) = shared
        .config
        .interaction
        .as_ref()
        .filter(|c| c.artifact_store.is_some())
    else {
        return Ok(());
    };
    let human = config.human_id;
    let cfg = config
        .artifact_store
        .clone()
        .expect("checked file configuration");
    // At most four independent byte jobs, even when many sessions finish together.
    for _ in 0..4 {
        let Some((job, local, doc)) = shared.store.artifact_job_claim(human)? else {
            break;
        };
        let shared = shared.clone();
        let cfg = cfg.clone();
        tokio::spawn(async move {
            let mut result: Result<()> =
                Err(std::io::Error::from(std::io::ErrorKind::WouldBlock).into());
            for attempt in 0..60 {
                if shared.shutdown.is_cancelled() {
                    break;
                }
                let worker = shared.clone();
                let cfg = cfg.clone();
                let doc = doc.clone();
                result = match tokio::task::spawn_blocking(move || {
                    copy_job(&worker, &cfg, local, human, &doc)
                })
                .await
                {
                    Ok(result) => result,
                    Err(error) => Err(error.into()),
                };
                if !result.as_ref().is_err_and(|e| {
                    e.downcast_ref::<std::io::Error>()
                        .is_some_and(|e| e.kind() == std::io::ErrorKind::WouldBlock)
                }) || attempt == 59
                {
                    break;
                }
                // Waiting for storage owns no blocking thread. Runtime shutdown
                // can cancel this wait, leaving the durable byte job recoverable.
                tokio::select! {
                    _=shared.shutdown.cancelled()=>break,
                    _=sleep(Duration::from_millis(250))=>{}
                }
            }
            let phase = match &result {
                Ok(()) => "done",
                Err(e)
                    if e.downcast_ref::<std::io::Error>()
                        .is_some_and(|e| e.kind() == std::io::ErrorKind::WouldBlock) =>
                {
                    "pending"
                }
                Err(_) => "partial",
            };
            if shared.store.artifact_job_finish(&job, phase).is_err() || result.is_err() {
                tracing::warn!(
                    "Generated files remain pending or partially unavailable; native text is unaffected"
                );
            }
            // Pending contention retries at the existing heartbeat, without a
            // tight loop or any native/model/input replay.
            if super::topic_publications::recover(shared.clone()).is_err() {
                tracing::warn!("Retained file publication recovery unconfirmed");
            }
            if phase != "pending" && recover(shared).is_err() {
                tracing::warn!("Retained file recovery unconfirmed");
            }
        });
    }
    Ok(())
}

fn copy_job(
    shared: &AppShared,
    cfg: &crate::interaction::ArtifactConfig,
    local: i64,
    human: i64,
    doc: &serde_json::Value,
) -> Result<()> {
    let out = Path::new(
        doc["out"]
            .as_str()
            .context("Missing retained output directory")?,
    );
    let source = anchor(out)?.context("Output directory anchor unsupported")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let meta = source.metadata()?;
        if Some(meta.dev()) != doc["device"].as_u64() || Some(meta.ino()) != doc["inode"].as_u64() {
            bail!("Retained output directory identity changed");
        }
    }
    snapshot(
        shared,
        cfg,
        local,
        human,
        out,
        Some(&source),
        &doc["output"],
    )
}

#[cfg(not(unix))]
pub(super) fn snapshot(
    _: &AppShared,
    _: &crate::interaction::ArtifactConfig,
    _: i64,
    _: i64,
    _: &Path,
    _: Option<&fs::File>,
    _: &serde_json::Value,
) -> Result<()> {
    bail!("Retained artifacts require descriptor-relative file access")
}

#[cfg(unix)]
pub(super) fn snapshot(
    shared: &AppShared,
    cfg: &crate::interaction::ArtifactConfig,
    local: i64,
    human: i64,
    out: &Path,
    source: Option<&fs::File>,
    output: &serde_json::Value,
) -> Result<()> {
    use sha2::{Digest, Sha256};
    use std::{
        ffi::CString,
        io::{Read, Write},
        os::{
            fd::{AsRawFd, FromRawFd},
            unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
        },
    };
    let source = source.context("Output directory was not anchored at execution creation")?;
    let before = source.metadata()?;
    let current = fs::symlink_metadata(out)?;
    if !current.is_dir()
        || current.file_type().is_symlink()
        || (before.dev(), before.ino()) != (current.dev(), current.ino())
    {
        bail!("Output directory identity changed")
    }
    if !cfg.directory.exists() {
        fs::create_dir_all(&cfg.directory)?;
        fs::set_permissions(&cfg.directory, fs::Permissions::from_mode(0o700))?;
    }
    let root = fs::symlink_metadata(&cfg.directory)?;
    if !root.is_dir()
        || root.file_type().is_symlink()
        || root.uid() != unsafe { libc::geteuid() }
        || root.mode() & 0o077 != 0
    {
        bail!("Artifact directory must be private and owned")
    }
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(cfg.directory.join(".lock"))?;
    if !lock.metadata()?.is_file()
        || lock.metadata()?.uid() != unsafe { libc::geteuid() }
        || lock.metadata()?.mode() & 0o077 != 0
    {
        bail!("Artifact storage lock unavailable")
    }
    if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    let owner = format!(
        "{:x}",
        Sha256::digest(
            shared
                .config
                .db_path
                .canonicalize()?
                .as_os_str()
                .as_encoded_bytes()
        )
    );
    let binding = cfg.directory.join(".owner");
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&binding)
    {
        Ok(mut file) => {
            file.write_all(owner.as_bytes())?;
            file.sync_all()?;
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            let file = fs::OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NOFOLLOW)
                .open(&binding)?;
            let mut existing = String::new();
            file.take(128).read_to_string(&mut existing)?;
            if existing != owner {
                bail!("Artifact directory belongs to another database")
            }
        }
        Err(e) => return Err(e.into()),
    }
    for sha in shared.store.artifact_expired_blobs()? {
        match fs::remove_file(cfg.directory.join(&sha)) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
        shared.store.artifact_blob_removed(&sha)?;
    }
    let mut entries = fs::read_dir(out)?.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(|e| e.file_name());
    if entries.len() > 10 {
        bail!("At most ten generated files per execution are supported")
    }
    let expires =
        (chrono::Utc::now() + chrono::Duration::hours(cfg.retention_hours as i64)).to_rfc3339();
    let mut failed = 0;
    for (index, entry) in entries.into_iter().enumerate() {
        let label = entry
            .file_name()
            .to_str()
            .filter(|s| {
                !s.is_empty()
                    && s.chars().count() <= 128
                    && !s.chars().any(|c| c.is_control() || matches!(c, '/' | '\\'))
            })
            .map(str::to_owned)
            .unwrap_or_else(|| format!("Unsupported output {}", index + 1));
        let result = (|| -> Result<()> {
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| anyhow!("Unsupported output filename"))?;
            if name.is_empty()
                || name.chars().count() > 128
                || name.chars().any(|c| c.is_control() || c == '\\')
            {
                bail!("Unsupported output filename")
            }
            if output
                .get("files")
                .is_some_and(|files| files.get(&name).is_none())
            {
                return Ok(());
            }
            let component = CString::new(name.as_bytes())?;
            let fd = unsafe {
                libc::openat(
                    source.as_raw_fd(),
                    component.as_ptr(),
                    libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC,
                )
            };
            if fd < 0 {
                return Err(std::io::Error::last_os_error().into());
            }
            let mut file = unsafe { fs::File::from_raw_fd(fd) };
            let initial = file.metadata()?;
            if let Some(expected) = output.get("files").and_then(|files| files.get(&name)) {
                if *expected
                    != serde_json::json!([
                        initial.dev(),
                        initial.ino(),
                        initial.len(),
                        initial.mtime(),
                        initial.mtime_nsec(),
                        initial.ctime(),
                        initial.ctime_nsec()
                    ])
                {
                    bail!("Response output changed before copying");
                }
            }
            if !initial.is_file() || initial.nlink() != 1 || initial.len() > cfg.max_file_bytes {
                bail!("Unsupported or oversized output")
            }
            let occupied = fs::read_dir(&cfg.directory)?
                .filter_map(|e| e.ok())
                .filter(|e| !matches!(e.file_name().to_str(), Some(".lock" | ".owner")))
                .try_fold(0u64, |sum, e| {
                    Ok::<_, std::io::Error>(sum + e.metadata()?.len())
                })?;
            if occupied >= cfg.max_total_bytes {
                bail!("Artifact storage limit reached")
            }
            let mut temporary = tempfile::NamedTempFile::new_in(&cfg.directory)?;
            let mut hash = Sha256::new();
            let mut bytes = 0u64;
            let mut buffer = [0u8; 65536];
            loop {
                let read = file.read(&mut buffer)?;
                if read == 0 {
                    break;
                }
                bytes += read as u64;
                if bytes > cfg.max_file_bytes {
                    bail!("Artifact exceeded its limit")
                };
                hash.update(&buffer[..read]);
                temporary.write_all(&buffer[..read])?;
            }
            let after = file.metadata()?;
            if bytes != initial.len()
                || (
                    initial.dev(),
                    initial.ino(),
                    initial.len(),
                    initial.mtime(),
                    initial.mtime_nsec(),
                    initial.ctime(),
                    initial.ctime_nsec(),
                ) != (
                    after.dev(),
                    after.ino(),
                    after.len(),
                    after.mtime(),
                    after.mtime_nsec(),
                    after.ctime(),
                    after.ctime_nsec(),
                )
            {
                bail!("Output changed while being retained")
            }
            let sha = format!("{:x}", hash.finalize());
            if shared
                .store
                .artifact_already_retained(local, human, &name, &sha)?
            {
                return Ok(());
            }
            let destination = cfg.directory.join(&sha);
            match fs::OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NOFOLLOW)
                .open(&destination)
            {
                Ok(mut existing) => {
                    let metadata = existing.metadata()?;
                    let mut verify = Sha256::new();
                    let mut count = 0u64;
                    loop {
                        let read = existing.read(&mut buffer)?;
                        if read == 0 {
                            break;
                        }
                        count += read as u64;
                        if count > cfg.max_file_bytes {
                            bail!("Retained artifact exceeded its limit")
                        };
                        verify.update(&buffer[..read]);
                    }
                    if !metadata.is_file()
                        || metadata.len() != bytes
                        || count != bytes
                        || format!("{:x}", verify.finalize()) != sha
                    {
                        bail!("Retained artifact integrity changed")
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    let physical = fs::read_dir(&cfg.directory)?
                        .filter_map(|e| e.ok())
                        .filter(|e| {
                            e.file_name().to_str().is_some_and(|n| {
                                n.len() == 64 && n.bytes().all(|b| b.is_ascii_hexdigit())
                            })
                        })
                        .try_fold(0u64, |sum, e| {
                            Ok::<_, std::io::Error>(sum + e.metadata()?.len())
                        })?;
                    if physical.max(shared.store.artifact_storage_bytes()? as u64) + bytes
                        > cfg.max_total_bytes
                    {
                        bail!("Artifact storage limit reached")
                    }
                    temporary.as_file().sync_all()?;
                    temporary
                        .as_file()
                        .set_permissions(fs::Permissions::from_mode(0o400))?;
                    temporary
                        .persist_noclobber(&destination)
                        .map_err(|e| e.error)?;
                    fs::File::open(&cfg.directory)?.sync_all()?;
                }
                Err(e) => return Err(e.into()),
            }
            shared.store.artifact_register(
                local,
                human,
                output["native"].as_str().context("Missing native source")?,
                output["turn"].as_str().context("Missing source turn")?,
                output["item"].as_str().context("Missing source item")?,
                &name,
                &sha,
                bytes as i64,
                &expires,
            )?;
            Ok(())
        })();
        if result.is_err() {
            failed += 1;
            if let (Some(native), Some(turn), Some(item)) = (
                output["native"].as_str(),
                output["turn"].as_str(),
                output["item"].as_str(),
            ) {
                let _ = shared
                    .store
                    .artifact_unavailable(local, human, native, turn, item, &label, &expires);
            }
        }
    }
    if failed > 0 {
        bail!("Some output files could not be retained; successful references remain available")
    }
    Ok(())
}

/// Read the existing immutable store through the descriptor-relative storage
/// helper, never through a worker-supplied path. Bytes remain frozen in memory
/// through the final audience check and network reservation.
pub(super) fn verified_bytes(
    cfg: &crate::interaction::ArtifactConfig,
    file: &crate::store::ArtifactRecord,
) -> Result<Vec<u8>> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let root = crate::telegram::open_local_file(&cfg.directory, &cfg.directory, true)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let m = root.metadata()?;
        if m.uid() != unsafe { libc::geteuid() } || m.mode() & 0o077 != 0 {
            bail!("Retained storage ownership changed");
        }
    }
    let mut bytes = Vec::new();
    let input =
        crate::telegram::open_local_file(&cfg.directory, &cfg.directory.join(&file.sha256), false)?;
    if input.metadata()?.len() != file.bytes as u64
        || file.bytes < 0
        || file.bytes as u64 > cfg.max_file_bytes
    {
        bail!("Retained file size changed");
    }
    input.take(cfg.max_file_bytes + 1).read_to_end(&mut bytes)?;
    if bytes.len() as i64 != file.bytes || format!("{:x}", Sha256::digest(&bytes)) != file.sha256 {
        bail!("Retained file integrity changed");
    }
    Ok(bytes)
}
