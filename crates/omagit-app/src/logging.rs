//! Rotating file logs, plus stderr when `OMAGIT_LOG` asks for it (SPEC §4).

use std::path::Path;

use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, fmt};

/// Keeps the background writer alive for the process's lifetime.
pub struct LogGuard(#[allow(dead_code)] tracing_appender::non_blocking::WorkerGuard);

/// Install the subscriber. Falls back to stderr alone when the log directory
/// cannot be created — a read-only home must not stop the app from starting.
pub fn init(dir: Option<&Path>) -> Option<LogGuard> {
    let filter = EnvFilter::try_from_env("OMAGIT_LOG").unwrap_or_else(|_| EnvFilter::new("info"));

    let Some(dir) = dir else {
        tracing_subscriber::registry()
            .with(filter)
            .with(fmt::layer().with_writer(std::io::stderr))
            .init();
        return None;
    };

    if let Err(error) = std::fs::create_dir_all(dir) {
        tracing_subscriber::registry()
            .with(filter)
            .with(fmt::layer().with_writer(std::io::stderr))
            .init();
        tracing::warn!(path = %dir.display(), %error, "log directory unavailable, logging to stderr");
        return None;
    }

    let appender = tracing_appender::rolling::daily(dir, "omagit.log");
    let (writer, guard) = tracing_appender::non_blocking(appender);
    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer().with_ansi(false).with_writer(writer))
        .with(fmt::layer().with_writer(std::io::stderr))
        .init();
    Some(LogGuard(guard))
}
