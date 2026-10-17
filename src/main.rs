mod app;
mod codex;
mod codex_history;
mod commands;
mod config;
mod control_client;
mod interaction;
mod limits;
mod models;
mod nexus;
mod render;
mod store;
mod telegram;
mod transcribe;
mod user_input;
mod voice;

use anyhow::Result;
use tokio::time::{Duration, sleep};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--control") {
        if args.len() < 5 || !matches!(args[3].as_str(), "stop" | "attach-active" | "presentation")
        {
            anyhow::bail!(
                "Usage: telecodex --control CONFIG stop all|status ID|resume ID; attach-active CHAT TOPIC NATIVE_THREAD NATIVE_TURN; presentation list|before DELIVERY_CURSOR NATIVE_CURSOR|part SOURCE REVISION OFFSET"
            );
        }
        let config = config::Config::load(args[2].clone().into())?;
        let command = if matches!(args[3].as_str(), "attach-active" | "presentation") {
            args[3..].join(" ")
        } else {
            args[4..].join(" ")
        };
        return control_client::run(&config, &command).await;
    }
    if args.get(1).map(String::as_str) == Some("--probe-native") {
        if !(args.len() == 4 || (args.len() == 5 && args[4] == "--create-thread-probes")) {
            anyhow::bail!(
                "Usage: telecodex --probe-native CODEX_BINARY CWD [--create-thread-probes]"
            );
        }
        let cwd = std::fs::canonicalize(&args[3])?;
        if !cwd.is_dir() {
            anyhow::bail!("native probe cwd must be a directory");
        }
        let result =
            codex::probe_native(std::path::Path::new(&args[2]), &cwd, args.len() == 5).await?;
        println!("{}", serde_json::to_string_pretty(&result)?);
        return Ok(());
    }
    let _ = dotenvy::dotenv();
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("telecodex=info,reqwest=warn")),
        )
        .with_target(false)
        .compact()
        .init();

    if let Some(delay_ms) = restart_delay_ms_from_env() {
        sleep(Duration::from_millis(delay_ms)).await;
    }

    let config_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "telecodex.toml".to_string());
    let config = config::Config::load(config_path.into())?;
    let app = app::App::bootstrap(config).await?;
    app.run().await
}

fn restart_delay_ms_from_env() -> Option<u64> {
    std::env::var("TELECODEX_RESTART_DELAY_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
}
