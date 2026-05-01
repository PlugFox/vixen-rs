//! Tracing setup. Two sinks:
//!
//! 1. **Console** — human-readable layer at `RUST_LOG`/`CONFIG_LOG_LEVEL`.
//! 2. **File** — rolling JSON appender, daily rotation, 7-day retention.
//!
//! See `server/docs/observability.md` for span conventions and the redaction
//! policy (raw bot tokens / `initData` / JWTs are never logged at info+; use
//! `crate::utils::RedactedToken` or the `crate::config::secrets` newtypes).

use std::path::Path;

use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, fmt};

/// Initialise the global tracing subscriber.
///
/// Filter precedence: `RUST_LOG` env > `default_level` arg > `info`.
///
/// Returns a `WorkerGuard` that MUST be held for the lifetime of the process —
/// dropping it flushes and closes the non-blocking JSON file writer.
pub fn init(default_level: &str, log_dir: impl AsRef<Path>) -> WorkerGuard {
    let filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new(default_level))
        .unwrap_or_else(|_| EnvFilter::new("info"));

    let console = fmt::layer()
        .with_target(false)
        .with_ansi(crate::build_info::IS_DEV);

    let appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .max_log_files(7)
        .filename_prefix("vixen-server")
        .filename_suffix("log")
        .build(log_dir.as_ref())
        .expect("rolling file appender");

    let (writer, guard) = tracing_appender::non_blocking(appender);
    let json = fmt::layer().json().with_writer(writer);

    tracing_subscriber::registry()
        .with(filter)
        .with(console)
        .with(json)
        .init();

    guard
}
