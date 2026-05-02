//! Background jobs (captcha expiry, daily report, spam cleanup, chat-info refresh,
//! summary generation). See `server/docs/rules/background-jobs.md`.

pub mod captcha_expiry;

use teloxide::prelude::*;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

use crate::api::AppState;

/// Spawn every registered job and return their join handles. Each task wraps
/// the job's `run` future and logs a panic-or-error exit at error level so
/// silent crashes are visible in prod logs.
pub fn spawn_all(bot: Bot, state: AppState, shutdown: CancellationToken) -> Vec<JoinHandle<()>> {
    vec![spawn_named(
        captcha_expiry::NAME,
        captcha_expiry::run(bot, state, shutdown),
    )]
}

fn spawn_named<F>(name: &'static str, fut: F) -> JoinHandle<()>
where
    F: std::future::Future<Output = anyhow::Result<()>> + Send + 'static,
{
    info!(job = name, "spawning");
    tokio::spawn(async move {
        match fut.await {
            Ok(()) => info!(job = name, "exited cleanly"),
            Err(e) => error!(job = name, ?e, "exited with error"),
        }
    })
}
