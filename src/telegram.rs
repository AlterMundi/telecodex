use std::{
    collections::HashMap,
    error::Error,
    fmt,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};
use reqwest::StatusCode;
use reqwest::multipart::{Form, Part};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use tokio::sync::Mutex;
use tokio::time::sleep;

#[derive(Clone)]
pub struct TelegramClient {
    http: reqwest::Client,
    token: String,
    api_base: String,
    outbound: Arc<OutboundRateLimiter>,
    local_file_root: Option<std::path::PathBuf>,
}

#[cfg(unix)]
fn open_local_file(
    root: &std::path::Path,
    path: &std::path::Path,
    directory: bool,
) -> Result<std::fs::File> {
    use std::os::unix::{
        ffi::OsStrExt,
        io::{AsRawFd, FromRawFd},
    };
    use std::path::Component;
    if !root.is_absolute()
        || root == std::path::Path::new("/")
        || !path.is_absolute()
        || !path.starts_with(root)
        || root
            .components()
            .chain(path.components())
            .any(|part| !matches!(part, Component::RootDir | Component::Normal(_)))
        || path
            .as_os_str()
            .as_bytes()
            .split(|byte| *byte == b'/')
            .skip(1)
            .any(|part| part.is_empty() || part == b"." || part == b"..")
    {
        bail!("unsupported Telegram file path outside trusted storage");
    }
    let parts: Vec<_> = path
        .components()
        .filter_map(|part| match part {
            Component::Normal(value) => Some(value),
            _ => None,
        })
        .collect();
    let mut current = std::fs::File::open("/")?;
    for (index, part) in parts.iter().enumerate() {
        let name =
            std::ffi::CString::new(part.as_bytes()).context("invalid local file component")?;
        let is_directory = index + 1 < parts.len() || directory;
        let flags = libc::O_RDONLY
            | libc::O_CLOEXEC
            | libc::O_NOFOLLOW
            | libc::O_NONBLOCK
            | if is_directory { libc::O_DIRECTORY } else { 0 };
        // Each open is anchored to the preceding directory descriptor, not a mutable pathname.
        let fd = unsafe { libc::openat(current.as_raw_fd(), name.as_ptr(), flags) };
        if fd < 0 {
            bail!("local Telegram storage path could not be opened safely");
        }
        current = unsafe { std::fs::File::from_raw_fd(fd) };
    }
    if !directory && !current.metadata()?.is_file() {
        bail!("local Telegram attachment is not a regular file");
    }
    Ok(current)
}

#[cfg(not(unix))]
fn open_local_file(_: &std::path::Path, _: &std::path::Path, _: bool) -> Result<std::fs::File> {
    bail!("trusted local Telegram storage requires a Unix host")
}

const TELEGRAM_REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
const TELEGRAM_GET_UPDATES_GRACE: Duration = Duration::from_secs(15);
const TELEGRAM_DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(600);
const TELEGRAM_UPLOAD_TIMEOUT: Duration = Duration::from_secs(120);
pub const TELEGRAM_GROUP_OUTBOUND_INTERVAL_MS: u64 = 3_500;
pub const TELEGRAM_PRIVATE_OUTBOUND_INTERVAL_MS: u64 = 1_000;
const TELEGRAM_GLOBAL_OUTBOUND_INTERVAL_MS: u64 = 40;

impl TelegramClient {
    pub fn new(token: String, api_base: String) -> Self {
        Self {
            http: reqwest::Client::new(),
            token,
            api_base: api_base.trim_end_matches('/').to_string(),
            outbound: Arc::new(OutboundRateLimiter::default()),
            local_file_root: None,
        }
    }

    pub fn with_local_file_root(mut self, root: Option<std::path::PathBuf>) -> Result<Self> {
        if let Some(root) = &root {
            let endpoint = reqwest::Url::parse(&self.api_base)?;
            if !matches!(
                endpoint.host_str(),
                Some("127.0.0.1" | "[::1]" | "::1" | "localhost")
            ) {
                bail!("local file storage requires a loopback Bot API endpoint");
            }
            // Validate the configured boundary without following any symlink.
            open_local_file(root, root, true)?;
        }
        self.local_file_root = root;
        Ok(self)
    }

    pub async fn get_me(&self) -> Result<User> {
        self.post::<(), User>("getMe", None).await
    }

    pub async fn get_updates(&self, offset: Option<i64>, timeout: u32) -> Result<Vec<Update>> {
        #[derive(Serialize)]
        struct Payload {
            offset: Option<i64>,
            timeout: u32,
            allowed_updates: Vec<&'static str>,
        }

        self.post_with_timeout(
            "getUpdates",
            Some(&Payload {
                offset,
                timeout,
                allowed_updates: vec!["message", "callback_query"],
            }),
            Duration::from_secs(timeout as u64).saturating_add(TELEGRAM_GET_UPDATES_GRACE),
        )
        .await
    }

    pub async fn set_my_commands(&self, commands: &[BotCommand]) -> Result<()> {
        #[derive(Serialize)]
        struct Payload<'a> {
            commands: &'a [BotCommand],
        }

        let _: bool = self
            .post("setMyCommands", Some(&Payload { commands }))
            .await?;
        Ok(())
    }

    pub async fn send_message(&self, request: SendMessage) -> Result<Message> {
        let chat_id = request.chat_id;
        self.post_outbound(chat_id, "sendMessage", Some(&request))
            .await
    }

    /// Open Telegram's reply composer while retaining the same delivery limits.
    pub async fn send_force_reply(&self, message: SendMessage) -> Result<Message> {
        if message.reply_markup.is_some() {
            bail!("ForceReply cannot be combined with an inline keyboard");
        }
        #[derive(Serialize)]
        struct ForceReply {
            force_reply: bool,
            input_field_placeholder: &'static str,
        }
        #[derive(Serialize)]
        struct Request<'a> {
            #[serde(flatten)]
            message: &'a SendMessage,
            reply_markup: ForceReply,
        }
        self.post_outbound(
            message.chat_id,
            "sendMessage",
            Some(&Request {
                message: &message,
                reply_markup: ForceReply {
                    force_reply: true,
                    input_field_placeholder: "Your answer",
                },
            }),
        )
        .await
    }

    pub async fn send_chat_action(
        &self,
        chat_id: i64,
        message_thread_id: Option<i64>,
        action: ChatAction,
    ) -> Result<bool> {
        #[derive(Serialize)]
        struct Payload<'a> {
            chat_id: i64,
            message_thread_id: Option<i64>,
            action: &'a str,
        }

        self.post(
            "sendChatAction",
            Some(&Payload {
                chat_id,
                message_thread_id,
                action: action.as_str(),
            }),
        )
        .await
    }

    pub async fn edit_message_text(&self, request: EditMessageText) -> Result<Message> {
        let chat_id = request.chat_id;
        self.post_outbound(chat_id, "editMessageText", Some(&request))
            .await
    }

    pub async fn answer_callback_query(&self, callback_query_id: &str) -> Result<bool> {
        #[derive(Serialize)]
        struct Payload<'a> {
            callback_query_id: &'a str,
        }

        self.post("answerCallbackQuery", Some(&Payload { callback_query_id }))
            .await
    }

    pub async fn send_photo(
        &self,
        chat_id: i64,
        message_thread_id: Option<i64>,
        path: &std::path::Path,
        file_name: &str,
        mime_type: Option<&str>,
    ) -> Result<Message> {
        self.post_multipart_message(
            "sendPhoto",
            chat_id,
            message_thread_id,
            "photo",
            path,
            file_name,
            mime_type,
        )
        .await
    }

    pub async fn send_document(
        &self,
        chat_id: i64,
        message_thread_id: Option<i64>,
        path: &std::path::Path,
        file_name: &str,
        mime_type: Option<&str>,
    ) -> Result<Message> {
        self.post_multipart_message(
            "sendDocument",
            chat_id,
            message_thread_id,
            "document",
            path,
            file_name,
            mime_type,
        )
        .await
    }

    pub async fn send_audio(
        &self,
        chat_id: i64,
        message_thread_id: Option<i64>,
        path: &std::path::Path,
        file_name: &str,
        mime_type: Option<&str>,
    ) -> Result<Message> {
        self.post_multipart_message(
            "sendAudio",
            chat_id,
            message_thread_id,
            "audio",
            path,
            file_name,
            mime_type,
        )
        .await
    }

    pub async fn send_video(
        &self,
        chat_id: i64,
        message_thread_id: Option<i64>,
        path: &std::path::Path,
        file_name: &str,
        mime_type: Option<&str>,
    ) -> Result<Message> {
        self.post_multipart_message(
            "sendVideo",
            chat_id,
            message_thread_id,
            "video",
            path,
            file_name,
            mime_type,
        )
        .await
    }

    pub async fn create_forum_topic(&self, chat_id: i64, name: &str) -> Result<ForumTopic> {
        #[derive(Serialize)]
        struct Payload<'a> {
            chat_id: i64,
            name: &'a str,
        }

        self.post_outbound(
            chat_id,
            "createForumTopic",
            Some(&Payload { chat_id, name }),
        )
        .await
    }

    pub async fn close_forum_topic(&self, chat_id: i64, message_thread_id: i64) -> Result<bool> {
        #[derive(Serialize)]
        struct Payload {
            chat_id: i64,
            message_thread_id: i64,
        }

        self.post_outbound(
            chat_id,
            "closeForumTopic",
            Some(&Payload {
                chat_id,
                message_thread_id,
            }),
        )
        .await
    }

    pub async fn delete_forum_topic(&self, chat_id: i64, message_thread_id: i64) -> Result<bool> {
        #[derive(Serialize)]
        struct Payload {
            chat_id: i64,
            message_thread_id: i64,
        }

        self.post_outbound(
            chat_id,
            "deleteForumTopic",
            Some(&Payload {
                chat_id,
                message_thread_id,
            }),
        )
        .await
    }

    pub async fn edit_forum_topic(
        &self,
        chat_id: i64,
        message_thread_id: i64,
        name: &str,
    ) -> Result<bool> {
        #[derive(Serialize)]
        struct Payload<'a> {
            chat_id: i64,
            message_thread_id: i64,
            name: &'a str,
        }

        self.post_outbound(
            chat_id,
            "editForumTopic",
            Some(&Payload {
                chat_id,
                message_thread_id,
                name,
            }),
        )
        .await
    }

    pub async fn get_forum_topic_icon_stickers(&self) -> Result<Vec<TopicIconSticker>> {
        self.post::<(), Vec<TopicIconSticker>>("getForumTopicIconStickers", None)
            .await
    }

    pub async fn edit_forum_topic_icon(
        &self,
        chat_id: i64,
        message_thread_id: i64,
        icon_custom_emoji_id: &str,
    ) -> Result<bool> {
        #[derive(Serialize)]
        struct Payload<'a> {
            chat_id: i64,
            message_thread_id: i64,
            icon_custom_emoji_id: &'a str,
        }
        self.post_outbound(
            chat_id,
            "editForumTopic",
            Some(&Payload {
                chat_id,
                message_thread_id,
                icon_custom_emoji_id,
            }),
        )
        .await
    }

    pub async fn send_message_draft(&self, request: SendMessageDraft) -> Result<bool> {
        let chat_id = request.chat_id;
        self.post_outbound(chat_id, "sendMessageDraft", Some(&request))
            .await
    }

    pub async fn get_file(&self, file_id: &str) -> Result<File> {
        #[derive(Serialize)]
        struct Payload<'a> {
            file_id: &'a str,
        }

        self.post_with_timeout(
            "getFile",
            Some(&Payload { file_id }),
            TELEGRAM_DOWNLOAD_TIMEOUT,
        )
        .await
    }

    /// Stream into a private temporary file; publish only a complete download.
    pub async fn download_file(
        &self,
        file_path: &str,
        destination: &std::path::Path,
        expected_size: Option<u64>,
    ) -> Result<()> {
        self.download_file_with_timeout(
            file_path,
            destination,
            expected_size,
            TELEGRAM_DOWNLOAD_TIMEOUT,
        )
        .await
    }

    async fn download_file_with_timeout(
        &self,
        file_path: &str,
        destination: &std::path::Path,
        expected_size: Option<u64>,
        timeout: Duration,
    ) -> Result<()> {
        use tokio::io::AsyncWriteExt;

        if file_path.starts_with('/') {
            let root = self.local_file_root.as_deref().context(
                "unsupported Telegram file path: trusted local storage is not configured",
            )?;
            let source = open_local_file(root, std::path::Path::new(file_path), false)?;
            let directory = destination
                .parent()
                .context("attachment destination has no parent")?;
            let temporary = tempfile::NamedTempFile::new_in(directory)?;
            let mut output = tokio::fs::File::from_std(temporary.reopen()?);
            let mut source = tokio::fs::File::from_std(source);
            let expected = source.metadata().await?.len();
            if expected_size.is_some_and(|size| size != expected) {
                bail!("local Telegram attachment size differs from getFile");
            }
            let received = tokio::time::timeout(timeout, tokio::io::copy(&mut source, &mut output))
                .await
                .context("local attachment copy timed out")??;
            if received != expected || source.metadata().await?.len() != expected {
                bail!("incomplete local Telegram file copy");
            }
            output.sync_all().await?;
            drop(output);
            temporary
                .persist_noclobber(destination)
                .map_err(|error| error.error)?;
            return Ok(());
        }
        if file_path.contains('\\')
            || file_path
                .split('/')
                .any(|part| part.is_empty() || part == "." || part == "..")
        {
            bail!("unsupported Telegram file path");
        }
        let mut url = reqwest::Url::parse(&self.api_base)?;
        url.path_segments_mut()
            .map_err(|_| anyhow::anyhow!("invalid Telegram API base"))?
            .pop_if_empty()
            .push("file")
            .push(&format!("bot{}", self.token))
            .extend(file_path.split('/'));
        let mut response = self
            .http
            .get(url)
            .timeout(timeout)
            .send()
            .await
            .map_err(reqwest::Error::without_url)
            .context("telegram getFile download failed")?;
        let status = response.status();
        if !status.is_success() {
            bail!("telegram file download failed with status {status}");
        }
        let directory = destination
            .parent()
            .context("attachment destination has no parent")?;
        let temporary = tempfile::NamedTempFile::new_in(directory)?;
        let mut output = tokio::fs::File::from_std(temporary.reopen()?);
        let expected = response.content_length();
        let mut received = 0u64;
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(reqwest::Error::without_url)?
        {
            output.write_all(&chunk).await?;
            received += chunk.len() as u64;
        }
        if expected.is_some_and(|length| length != received)
            || expected_size.is_some_and(|length| length != received)
        {
            bail!("incomplete Telegram file download");
        }
        output.flush().await?;
        output.sync_all().await?;
        drop(output);
        temporary
            .persist_noclobber(destination)
            .map_err(|error| error.error)?;
        Ok(())
    }

    async fn post<T, R>(&self, method: &str, payload: Option<&T>) -> Result<R>
    where
        T: Serialize + ?Sized,
        R: DeserializeOwned,
    {
        self.post_with_timeout(method, payload, TELEGRAM_REQUEST_TIMEOUT)
            .await
    }

    async fn post_outbound<T, R>(
        &self,
        chat_id: i64,
        method: &str,
        payload: Option<&T>,
    ) -> Result<R>
    where
        T: Serialize + ?Sized,
        R: DeserializeOwned,
    {
        self.outbound.wait(chat_id).await;
        let result = self.post(method, payload).await;
        if let Err(error) = &result {
            self.outbound.note_retry_after(chat_id, error).await;
        }
        result
    }

    async fn post_with_timeout<T, R>(
        &self,
        method: &str,
        payload: Option<&T>,
        timeout: Duration,
    ) -> Result<R>
    where
        T: Serialize + ?Sized,
        R: DeserializeOwned,
    {
        let url = format!("{}/bot{}/{}", self.api_base, self.token, method);
        let mut request = self.http.post(url);
        if let Some(payload) = payload {
            request = request.json(payload);
        }
        request = request.timeout(timeout);

        let response = request
            .send()
            .await
            .map_err(reqwest::Error::without_url)
            .with_context(|| format!("telegram {method} request failed"))?;
        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(reqwest::Error::without_url)
            .with_context(|| format!("telegram {method} response body failed"))?;

        if !status.is_success() {
            let parsed = serde_json::from_str::<ApiResponse<R>>(&body).ok();
            if let Some(parsed) = parsed {
                if let Some(parameters) = parsed.parameters {
                    return Err(TelegramError {
                        status,
                        description: parsed
                            .description
                            .unwrap_or_else(|| "telegram api error".to_string())
                            .replace(&self.token, "[redacted]"),
                        retry_after: parameters.retry_after,
                    }
                    .into());
                }
            }
            return Err(TelegramError {
                status,
                description: body.replace(&self.token, "[redacted]"),
                retry_after: None,
            }
            .into());
        }

        // Serde's error can quote a server-controlled value, including a reflected token.
        // Retain the method diagnostic without retaining the raw decoding cause.
        let parsed: ApiResponse<R> = serde_json::from_str(&body)
            .map_err(|_| anyhow::anyhow!("telegram {method} JSON decode failed"))?;
        if !parsed.ok {
            return Err(TelegramError {
                status,
                description: parsed
                    .description
                    .unwrap_or_else(|| "telegram api error".to_string())
                    .replace(&self.token, "[redacted]"),
                retry_after: parsed
                    .parameters
                    .and_then(|parameters| parameters.retry_after),
            }
            .into());
        }

        parsed
            .result
            .ok_or_else(|| anyhow::anyhow!("telegram {method} returned ok without result"))
    }

    #[allow(clippy::too_many_arguments)]
    async fn post_multipart_message(
        &self,
        method: &str,
        chat_id: i64,
        message_thread_id: Option<i64>,
        file_field: &str,
        path: &std::path::Path,
        file_name: &str,
        mime_type: Option<&str>,
    ) -> Result<Message> {
        let bytes = tokio::fs::read(path)
            .await
            .with_context(|| format!("failed to read upload file {}", path.display()))?;
        self.outbound.wait(chat_id).await;
        let url = format!("{}/bot{}/{}", self.api_base, self.token, method);
        let part = if let Some(mime_type) = mime_type {
            match Part::bytes(bytes.clone())
                .file_name(file_name.to_string())
                .mime_str(mime_type)
            {
                Ok(part) => part,
                Err(_) => Part::bytes(bytes).file_name(file_name.to_string()),
            }
        } else {
            Part::bytes(bytes).file_name(file_name.to_string())
        };

        let mut form = Form::new()
            .text("chat_id", chat_id.to_string())
            .part(file_field.to_string(), part);
        if let Some(thread_id) = message_thread_id {
            form = form.text("message_thread_id", thread_id.to_string());
        }

        let response = self
            .http
            .post(url)
            .multipart(form)
            .timeout(TELEGRAM_UPLOAD_TIMEOUT)
            .send()
            .await
            .map_err(reqwest::Error::without_url)
            .with_context(|| format!("telegram {method} multipart request failed"))?;
        let status = response.status();
        let body = response
            .text()
            .await
            .map_err(reqwest::Error::without_url)
            .with_context(|| format!("telegram {method} response body failed"))?;

        if !status.is_success() {
            let parsed = serde_json::from_str::<ApiResponse<Message>>(&body).ok();
            if let Some(parsed) = parsed {
                if let Some(parameters) = parsed.parameters {
                    if let Some(retry_after) = parameters.retry_after {
                        self.outbound.backoff(chat_id, retry_after).await;
                    }
                    return Err(TelegramError {
                        status,
                        description: parsed
                            .description
                            .unwrap_or_else(|| "telegram api error".to_string())
                            .replace(&self.token, "[redacted]"),
                        retry_after: parameters.retry_after,
                    }
                    .into());
                }
            }
            return Err(TelegramError {
                status,
                description: body.replace(&self.token, "[redacted]"),
                retry_after: None,
            }
            .into());
        }

        let parsed: ApiResponse<Message> = serde_json::from_str(&body)
            .map_err(|_| anyhow::anyhow!("telegram {method} JSON decode failed"))?;
        if !parsed.ok {
            if let Some(retry_after) = parsed
                .parameters
                .as_ref()
                .and_then(|parameters| parameters.retry_after)
            {
                self.outbound.backoff(chat_id, retry_after).await;
            }
            return Err(TelegramError {
                status,
                description: parsed
                    .description
                    .unwrap_or_else(|| "telegram api error".to_string())
                    .replace(&self.token, "[redacted]"),
                retry_after: parsed
                    .parameters
                    .and_then(|parameters| parameters.retry_after),
            }
            .into());
        }

        parsed
            .result
            .ok_or_else(|| anyhow::anyhow!("telegram {method} returned ok without result"))
    }
}

#[derive(Debug)]
struct OutboundRateLimiter {
    state: Mutex<OutboundLimiterState>,
}

impl Default for OutboundRateLimiter {
    fn default() -> Self {
        Self {
            state: Mutex::new(OutboundLimiterState::default()),
        }
    }
}

impl OutboundRateLimiter {
    async fn wait(&self, chat_id: i64) {
        loop {
            let next = {
                let mut state = self.state.lock().await;
                state.acquire(chat_id, Instant::now())
            };
            match next {
                Ok(()) => return,
                Err(next_at) => {
                    let wait = next_at.saturating_duration_since(Instant::now());
                    if !wait.is_zero() {
                        sleep(wait).await;
                    }
                }
            }
        }
    }

    async fn note_retry_after(&self, chat_id: i64, error: &anyhow::Error) {
        if let Some(retry_after) = telegram_retry_after(error) {
            self.backoff(chat_id, retry_after).await;
        }
    }

    async fn backoff(&self, chat_id: i64, retry_after: u64) {
        let mut state = self.state.lock().await;
        state.backoff(chat_id, Instant::now(), retry_after);
    }
}

#[derive(Debug, Default)]
struct OutboundLimiterState {
    next_global_at: Option<Instant>,
    next_chat_at: HashMap<i64, Instant>,
}

impl OutboundLimiterState {
    fn acquire(&mut self, chat_id: i64, now: Instant) -> std::result::Result<(), Instant> {
        let slot = self.next_available_at(chat_id, now);
        if slot > now {
            return Err(slot);
        }
        self.next_global_at =
            Some(now + Duration::from_millis(TELEGRAM_GLOBAL_OUTBOUND_INTERVAL_MS));
        self.next_chat_at
            .insert(chat_id, now + outbound_interval_for_chat(chat_id));
        Ok(())
    }

    fn next_available_at(&self, chat_id: i64, now: Instant) -> Instant {
        let next_global = self.next_global_at.unwrap_or(now);
        let next_chat = self.next_chat_at.get(&chat_id).copied().unwrap_or(now);
        next_global.max(next_chat).max(now)
    }

    fn backoff(&mut self, chat_id: i64, now: Instant, retry_after: u64) {
        let until = now + Duration::from_secs(retry_after.saturating_add(1));
        self.next_global_at = Some(self.next_global_at.unwrap_or(now).max(until));
        let next_chat = self.next_chat_at.entry(chat_id).or_insert(now);
        *next_chat = (*next_chat).max(until);
    }
}

pub fn outbound_interval_for_chat(chat_id: i64) -> Duration {
    if chat_id < 0 {
        Duration::from_millis(TELEGRAM_GROUP_OUTBOUND_INTERVAL_MS)
    } else {
        Duration::from_millis(TELEGRAM_PRIVATE_OUTBOUND_INTERVAL_MS)
    }
}

fn telegram_retry_after(error: &anyhow::Error) -> Option<u64> {
    error
        .downcast_ref::<TelegramError>()
        .and_then(|telegram| telegram.retry_after)
}

#[derive(Debug)]
pub struct TelegramError {
    pub status: StatusCode,
    pub description: String,
    pub retry_after: Option<u64>,
}

impl fmt::Display for TelegramError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "telegram api error {}: {}",
            self.status, self.description
        )
    }
}

impl Error for TelegramError {}

#[derive(Debug, Deserialize)]
struct ApiResponse<T> {
    ok: bool,
    result: Option<T>,
    description: Option<String>,
    parameters: Option<ResponseParameters>,
}

#[derive(Debug, Deserialize)]
struct ResponseParameters {
    retry_after: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Update {
    pub update_id: i64,
    pub message: Option<Message>,
    pub callback_query: Option<CallbackQuery>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub message_id: i64,
    pub message_thread_id: Option<i64>,
    pub from: Option<User>,
    pub chat: Chat,
    pub text: Option<String>,
    pub reply_to_message: Option<ReplyMessage>,
    pub caption: Option<String>,
    #[serde(default)]
    pub photo: Vec<PhotoSize>,
    pub document: Option<Document>,
    pub audio: Option<Audio>,
    pub voice: Option<Voice>,
    pub video: Option<Video>,
    pub forum_topic_edited: Option<ForumTopicEdited>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForumTopicEdited {
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplyMessage {
    pub message_id: i64,
    pub from: Option<User>,
    pub text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallbackQuery {
    pub id: String,
    pub from: User,
    pub message: Option<Message>,
    pub data: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: i64,
    pub is_bot: bool,
    #[allow(dead_code)]
    pub first_name: String,
    pub username: Option<String>,
    pub has_topics_enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chat {
    pub id: i64,
    #[serde(rename = "type")]
    pub kind: String,
    pub is_forum: Option<bool>,
    pub username: Option<String>,
    pub title: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PhotoSize {
    pub file_id: String,
    pub width: i64,
    pub height: i64,
    pub file_size: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    pub file_id: String,
    pub file_name: Option<String>,
    pub mime_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Audio {
    pub file_id: String,
    pub file_name: Option<String>,
    pub mime_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Voice {
    pub file_id: String,
    pub mime_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Video {
    pub file_id: String,
    pub file_name: Option<String>,
    pub mime_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct File {
    pub file_path: Option<String>,
    pub file_size: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForumTopic {
    pub message_thread_id: i64,
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TopicIconSticker {
    pub emoji: Option<String>,
    pub custom_emoji_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct BotCommand {
    pub command: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SendMessage {
    pub chat_id: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_thread_id: Option<i64>,
    pub text: String,
    pub parse_mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link_preview_options: Option<LinkPreviewOptions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_markup: Option<InlineKeyboardMarkup>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EditMessageText {
    pub chat_id: i64,
    pub message_id: i64,
    pub text: String,
    pub parse_mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link_preview_options: Option<LinkPreviewOptions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_markup: Option<InlineKeyboardMarkup>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SendMessageDraft {
    pub chat_id: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_thread_id: Option<i64>,
    pub draft_id: i64,
    pub text: String,
    pub parse_mode: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct InlineKeyboardMarkup {
    pub inline_keyboard: Vec<Vec<InlineKeyboardButton>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InlineKeyboardButton {
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub callback_data: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LinkPreviewOptions {
    pub is_disabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatAction {
    Typing,
    UploadPhoto,
    UploadDocument,
    UploadVideo,
    UploadAudio,
}

impl ChatAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Typing => "typing",
            Self::UploadPhoto => "upload_photo",
            Self::UploadDocument => "upload_document",
            Self::UploadVideo => "upload_video",
            Self::UploadAudio => "upload_audio",
        }
    }
}

impl SendMessage {
    pub fn html(chat_id: i64, thread_id: Option<i64>, text: String) -> Self {
        Self {
            chat_id,
            message_thread_id: thread_id,
            text,
            parse_mode: "HTML".to_string(),
            link_preview_options: Some(LinkPreviewOptions { is_disabled: true }),
            reply_markup: None,
        }
    }
}

impl EditMessageText {
    pub fn html(chat_id: i64, message_id: i64, text: String) -> Self {
        Self {
            chat_id,
            message_id,
            text,
            parse_mode: "HTML".to_string(),
            link_preview_options: Some(LinkPreviewOptions { is_disabled: true }),
            reply_markup: None,
        }
    }
}

impl SendMessageDraft {
    pub fn html(chat_id: i64, thread_id: Option<i64>, draft_id: i64, text: String) -> Self {
        Self {
            chat_id,
            message_thread_id: thread_id,
            draft_id,
            text,
            parse_mode: "HTML".to_string(),
        }
    }
}

pub fn normalize_command(text: &str, bot_username: Option<&str>) -> Option<(String, String)> {
    let trimmed = text.trim();
    if !trimmed.starts_with('/') {
        return None;
    }
    let mut split = trimmed.splitn(2, char::is_whitespace);
    let raw_command = split.next()?.trim();
    let args = split.next().unwrap_or("").trim().to_string();
    let command_without_slash = raw_command.trim_start_matches('/');
    let (name, mention) = command_without_slash
        .split_once('@')
        .unwrap_or((command_without_slash, ""));
    if !mention.is_empty() {
        let expected = bot_username.unwrap_or_default();
        if !expected.is_empty() && !mention.eq_ignore_ascii_case(expected) {
            return None;
        }
    }
    Some((format!("/{}", name.to_lowercase()), args))
}

pub fn is_foreign_bot_command(text: &str, bot_username: Option<&str>) -> bool {
    let trimmed = text.trim();
    if !trimmed.starts_with('/') {
        return false;
    }
    let raw_command = trimmed.split_whitespace().next().unwrap_or_default().trim();
    let command_without_slash = raw_command.trim_start_matches('/');
    let Some((_, mention)) = command_without_slash.split_once('@') else {
        return false;
    };
    let expected = bot_username.unwrap_or_default();
    !mention.is_empty() && !expected.is_empty() && !mention.eq_ignore_ascii_case(expected)
}

pub fn preferred_image_file_id(message: &Message) -> Option<&str> {
    if let Some(document) = &message.document {
        if document
            .mime_type
            .as_deref()
            .unwrap_or_default()
            .starts_with("image/")
        {
            return Some(document.file_id.as_str());
        }
    }

    message
        .photo
        .iter()
        .max_by_key(|size| size.file_size.unwrap_or(size.width * size.height))
        .map(|photo| photo.file_id.as_str())
}

#[cfg(test)]
mod rate_limit_tests {
    use super::*;

    #[test]
    fn outbound_limiter_spaces_group_messages() {
        let mut state = OutboundLimiterState::default();
        let now = Instant::now();

        let first = state.acquire(-100123, now);
        let second = state.acquire(-100123, now);

        assert_eq!(first, Ok(()));
        assert_eq!(
            second.unwrap_err().duration_since(now),
            Duration::from_millis(TELEGRAM_GROUP_OUTBOUND_INTERVAL_MS)
        );
    }

    #[test]
    fn outbound_limiter_spaces_private_messages() {
        let mut state = OutboundLimiterState::default();
        let now = Instant::now();

        let first = state.acquire(123, now);
        let second = state.acquire(123, now);

        assert_eq!(first, Ok(()));
        assert_eq!(
            second.unwrap_err().duration_since(now),
            Duration::from_millis(TELEGRAM_PRIVATE_OUTBOUND_INTERVAL_MS)
        );
    }

    #[test]
    fn outbound_limiter_applies_global_spacing_between_chats() {
        let mut state = OutboundLimiterState::default();
        let now = Instant::now();

        let first = state.acquire(1, now);
        let second = state.acquire(2, now);

        assert_eq!(first, Ok(()));
        assert_eq!(
            second.unwrap_err().duration_since(now),
            Duration::from_millis(TELEGRAM_GLOBAL_OUTBOUND_INTERVAL_MS)
        );
    }

    #[test]
    fn outbound_limiter_extends_chat_after_retry_after() {
        let mut state = OutboundLimiterState::default();
        let now = Instant::now();
        state.backoff(-100123, now, 7);

        let next = state.acquire(-100123, now);

        assert_eq!(
            next.unwrap_err().duration_since(now),
            Duration::from_secs(8)
        );
    }

    #[test]
    fn outbound_limiter_rechecks_backoff_after_waiting() {
        let mut state = OutboundLimiterState::default();
        let now = Instant::now();

        assert_eq!(state.acquire(-100123, now), Ok(()));
        let queued_at = state.acquire(-100123, now).unwrap_err();
        state.backoff(-100123, now, 7);

        assert_eq!(
            queued_at.duration_since(now),
            Duration::from_millis(TELEGRAM_GROUP_OUTBOUND_INTERVAL_MS)
        );
        assert_eq!(
            state
                .acquire(-100123, queued_at)
                .unwrap_err()
                .duration_since(now),
            Duration::from_secs(8)
        );
    }

    #[test]
    fn send_message_draft_payload_includes_draft_id() {
        let payload = SendMessageDraft::html(42, Some(7), 123, "partial".to_string());
        let value = serde_json::to_value(payload).unwrap();

        assert_eq!(value["chat_id"], 42);
        assert_eq!(value["message_thread_id"], 7);
        assert_eq!(value["draft_id"], 123);
        assert_eq!(value["text"], "partial");
        assert_eq!(value["parse_mode"], "HTML");
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn serve_api(
        replies: Vec<(u16, serde_json::Value)>,
    ) -> (
        String,
        std::thread::JoinHandle<Vec<(String, serde_json::Value)>>,
    ) {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = format!("http://{}", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let worker = std::thread::spawn(move || {
            let mut requests = Vec::new();
            for (status, value) in replies {
                let deadline = Instant::now() + Duration::from_secs(5);
                let mut stream = loop {
                    match listener.accept() {
                        Ok((stream, _)) => break stream,
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            assert!(Instant::now() < deadline, "missing HTTP request");
                            std::thread::sleep(Duration::from_millis(5));
                        }
                        Err(error) => panic!("test HTTP accept: {error}"),
                    }
                };
                // macOS inherits the listener's nonblocking flag on accepted sockets.
                // The fixture uses bounded blocking reads on every supported platform.
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut bytes = Vec::new();
                let split = loop {
                    let mut byte = [0u8; 1];
                    stream.read_exact(&mut byte).unwrap();
                    bytes.push(byte[0]);
                    if bytes.ends_with(b"\r\n\r\n") {
                        break bytes.len();
                    }
                };
                let header = String::from_utf8(bytes).unwrap();
                let length: usize = header
                    .lines()
                    .find_map(|line| {
                        let (key, val) = line.split_once(':')?;
                        key.eq_ignore_ascii_case("content-length")
                            .then(|| val.trim().parse().unwrap())
                    })
                    .unwrap_or(0);
                assert!(length <= 8192);
                let mut body = vec![0; length];
                stream.read_exact(&mut body).unwrap();
                assert_eq!(split, header.len());
                requests.push((
                    header.lines().next().unwrap().to_string(),
                    if body.is_empty() || header.contains("multipart/form-data") {
                        serde_json::Value::Null
                    } else {
                        serde_json::from_slice(&body).unwrap()
                    },
                ));
                let body = value.to_string();
                write!(stream, "HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            }
            requests
        });
        (address, worker)
    }

    #[tokio::test]
    async fn attachment_download_is_atomic_and_preserves_existing_files() {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let responses = [
                "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n3\r\nabc\r\n3\r\ndef\r\n0\r\n\r\n",
                "HTTP/1.1 200 OK\r\nContent-Length: 100\r\nConnection: close\r\n\r\npartial",
                "HTTP/1.1 200 OK\r\nContent-Length: 3\r\nConnection: close\r\n\r\nnew",
            ];
            for response in responses {
                let (mut socket, _) = listener.accept().unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut headers = Vec::new();
                loop {
                    let mut byte = [0];
                    socket.read_exact(&mut byte).unwrap();
                    headers.push(byte[0]);
                    if headers.ends_with(b"\r\n\r\n") {
                        break;
                    }
                }
                assert!(
                    String::from_utf8(headers)
                        .unwrap()
                        .starts_with("GET /file/botsynthetic-token/audio/clip.m4a ")
                );
                socket.write_all(response.as_bytes()).unwrap();
            }
        });
        let dir = tempfile::tempdir().unwrap();
        let destination = dir.path().join("clip.m4a");
        let client = TelegramClient::new("synthetic-token".to_string(), base);
        client
            .download_file("audio/clip.m4a", &destination, None)
            .await
            .unwrap();
        assert_eq!(std::fs::read(&destination).unwrap(), b"abcdef");
        let failed = dir.path().join("failed.m4a");
        let error = client
            .download_file("audio/clip.m4a", &failed, None)
            .await
            .unwrap_err();
        assert!(!format!("{error:?}").contains("synthetic-token"));
        assert!(!failed.exists());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
        assert!(
            client
                .download_file("audio/clip.m4a", &destination, None)
                .await
                .is_err()
        );
        assert_eq!(std::fs::read(&destination).unwrap(), b"abcdef");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
        server.join().unwrap();
        let error = client
            .download_file("audio/clip.m4a", &failed, None)
            .await
            .unwrap_err();
        assert!(!format!("{error:?}").contains("synthetic-token"));
    }

    #[tokio::test]
    async fn timed_out_download_never_publishes_partial_file() {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            let mut data = [0; 4096];
            let _ = socket.read(&mut data);
            socket
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\nConnection: close\r\n\r\npartial",
                )
                .unwrap();
            std::thread::sleep(Duration::from_millis(200));
        });
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("audio");
        let client = TelegramClient::new("synthetic-token".into(), base);
        let error = client
            .download_file_with_timeout("audio/clip", &target, None, Duration::from_millis(50))
            .await
            .unwrap_err();
        assert!(!format!("{error:?}").contains("synthetic-token"));
        assert!(!target.exists());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
        server.join().unwrap();
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn trusted_local_storage_copies_large_files_and_rejects_escapes() {
        use std::os::unix::fs::symlink;
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().canonicalize().unwrap();
        let storage = root.join("storage");
        std::fs::create_dir(&storage).unwrap();
        let source = storage.join("large.ogg");
        let bytes = vec![0x5au8; 21 * 1024 * 1024];
        std::fs::write(&source, &bytes).unwrap();
        let target = root.join("received.ogg");
        let client = TelegramClient::new("synthetic-token".into(), "http://127.0.0.1:1".into())
            .with_local_file_root(Some(storage.clone()))
            .unwrap();
        client
            .download_file(source.to_str().unwrap(), &target, Some(bytes.len() as u64))
            .await
            .unwrap();
        assert_eq!(std::fs::read(&target).unwrap(), bytes);
        assert!(
            client
                .download_file(source.to_str().unwrap(), &target, None)
                .await
                .is_err()
        );
        let failed = root.join("failed");
        assert!(
            client
                .download_file(source.to_str().unwrap(), &failed, Some(1))
                .await
                .is_err()
        );
        let outside = root.join("secret");
        std::fs::write(&outside, b"secret").unwrap();
        symlink(&outside, storage.join("link")).unwrap();
        symlink(&root, storage.join("directory-link")).unwrap();
        std::fs::create_dir(storage.join("sub")).unwrap();
        for path in [
            outside,
            storage.join("link"),
            storage.join("directory-link/secret"),
            storage.join("sub/../../secret"),
            storage.join("sub/../large.ogg"),
        ] {
            assert!(
                client
                    .download_file(path.to_str().unwrap(), &failed, None)
                    .await
                    .is_err()
            );
            assert!(!failed.exists());
        }
        let alias = root.join("alias");
        symlink(&storage, &alias).unwrap();
        assert!(
            TelegramClient::new("token".into(), "http://127.0.0.1:1".into())
                .with_local_file_root(Some(alias))
                .is_err()
        );
        assert!(
            TelegramClient::new("token".into(), "https://api.telegram.org".into())
                .with_local_file_root(Some(storage))
                .is_err()
        );
        assert!(
            TelegramClient::new("token".into(), "http://127.0.0.1:1".into())
                .with_local_file_root(Some(std::path::PathBuf::from("/")))
                .is_err()
        );
    }

    #[tokio::test]
    async fn attachment_paths_never_authorize_local_reads_or_traversal() {
        let client = TelegramClient::new(
            "synthetic-token".to_string(),
            "http://127.0.0.1:1".to_string(),
        );
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("output");
        for path in [
            "/etc/passwd",
            "../secret",
            "audio/../secret",
            "audio//clip",
            "audio\\clip",
        ] {
            let error = client.download_file(path, &target, None).await.unwrap_err();
            assert!(error.to_string().contains("unsupported Telegram file path"));
        }
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[tokio::test]
    async fn actual_api_and_network_failures_never_expose_the_token() {
        let token = "synthetic-secret-token";
        let (api, server) = serve_api(vec![(
            400,
            serde_json::json!({
                "ok": false, "description": format!("reflected bot{token}")
            }),
        )]);
        let client = TelegramClient::new(token.to_string(), api.clone());
        let error = client.get_me().await.unwrap_err();
        assert!(!format!("{error:#}").contains(token));
        assert!(format!("{error:#}").contains("[redacted]"));
        assert!(server.join().unwrap()[0].0.contains("/getMe"));
        let error = client.get_me().await.unwrap_err();
        assert!(!format!("{error:#}").contains(token));
        assert!(!format!("{error:?}").contains(token));
    }

    #[tokio::test]
    async fn malformed_success_decoding_never_retains_a_reflected_token() {
        let token = "synthetic-secret-token";
        let (api, server) = serve_api(vec![
            (200, serde_json::json!({"ok":true,"result":token})),
            (200, serde_json::json!({"ok":true,"result":token})),
        ]);
        let client = TelegramClient::new(token.to_string(), api);
        let error = client.get_me().await.unwrap_err();
        assert!(!format!("{error:#}").contains(token));
        assert!(!format!("{error:?}").contains(token));
        assert!(error.to_string().contains("getMe JSON decode failed"));
        let file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(file.path(), "synthetic upload").unwrap();
        let error = client
            .send_document(100, Some(7), file.path(), "fixture.txt", None)
            .await
            .unwrap_err();
        assert!(!format!("{error:#}").contains(token));
        assert!(!format!("{error:?}").contains(token));
        assert!(
            error
                .to_string()
                .contains("sendDocument JSON decode failed")
        );
        let requests = server.join().unwrap();
        assert!(requests[1].0.contains("/sendDocument"));
    }

    #[test]
    fn detects_foreign_bot_command_mentions() {
        assert!(is_foreign_bot_command(
            "/status@other_bot",
            Some("telecodex_bot")
        ));
        assert!(!is_foreign_bot_command(
            "/status@telecodex_bot",
            Some("telecodex_bot")
        ));
        assert!(!is_foreign_bot_command("/status", Some("telecodex_bot")));
    }
}
