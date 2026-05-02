//! Background jobs (captcha expiry, daily report, spam cleanup, chat-info refresh,
//! summary generation). See `server/docs/rules/background-jobs.md`.

pub mod captcha_expiry;
pub mod spam_cleanup;

use teloxide::prelude::*;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

use crate::api::AppState;

/// Spawn every registered job and return their join handles. Each task logs
/// a clean exit (`Ok(())`) or a job-level error returned by `run`. **Panics
/// inside `run` are NOT caught here** — they unwind the spawned task and
/// surface as a `JoinError` when the caller awaits the returned handle, which
/// is where panic logging happens (see `bin/server.rs`).
pub fn spawn_all(bot: Bot, state: AppState, shutdown: CancellationToken) -> Vec<JoinHandle<()>> {
    vec![
        spawn_named(
            captcha_expiry::NAME,
            captcha_expiry::run(bot.clone(), state.clone(), shutdown.clone()),
        ),
        spawn_named(spam_cleanup::NAME, spam_cleanup::run(bot, state, shutdown)),
    ]
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
