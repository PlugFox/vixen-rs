//! Telemetry. M0 stub — full rolling-file appender + `RedactedToken` integration
//! lands in #23 (per `server/docs/observability.md`).

use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, fmt};

/// Initialise the global tracing subscriber with a console layer.
///
/// Filter precedence: `RUST_LOG` env > `default_level` arg > `info`.
pub fn init(default_level: &str) {
    let filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new(default_level))
        .unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(filter)
        .with(
            fmt::layer()
                .with_target(false)
                .with_ansi(crate::build_info::IS_DEV),
        )
        .init();
}
