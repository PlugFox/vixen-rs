//! `spam_cleanup` job — prunes `spam_messages` rows older than the
//! configured retention window (default 14 days, matches the Dart prototype).
//! Without this the dedup table grows monotonically; the trade-off is that a
//! long-tail recurrence after the retention window starts fresh (`hit_count
//! = 1`), which is acceptable.
//!
//! Tick: every 24h, cancel-aware. The DELETE is a single statement; spam
//! tables are small (millions of rows worst-case) so we don't need batching
//! the way captcha_expiry does.

use std::time::Duration;

use anyhow::{Context, Result};
use sqlx::PgPool;
use teloxide::prelude::*;
use tokio_util::sync::CancellationToken;
use tracing::{info, instrument, warn};

use crate::api::AppState;

pub const NAME: &str = "spam_cleanup";
pub const INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);

pub async fn run(_bot: Bot, state: AppState, shutdown: CancellationToken) -> Result<()> {
    let retention_days = i64::from(state.config.spam_retention_days);
    let mut interval = tokio::time::interval(INTERVAL);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    info!(
        job = NAME,
        interval_secs = INTERVAL.as_secs(),
        retention_days,
        "starting"
    );
    loop {
        tokio::select! {
            biased;
            _ = shutdown.cancelled() => {
                info!(job = NAME, "shutdown");
                return Ok(());
            }
            _ = interval.tick() => {
                if let Err(e) = do_one_pass(state.db.pool(), retention_days).await {
                    warn!(job = NAME, ?e, "iteration failed");
                }
            }
        }
    }
}

#[instrument(skip(pool), fields(job = NAME, retention_days))]
async fn do_one_pass(pool: &PgPool, retention_days: i64) -> Result<()> {
    let pruned = prune_expired(pool, retention_days).await?;
    if pruned > 0 {
        info!(pruned, "spam_messages rows pruned");
    }
    Ok(())
}

/// Delete `spam_messages` rows whose `last_seen` is older than `now -
/// retention_days days`. Returns the count.
pub async fn prune_expired(pool: &PgPool, retention_days: i64) -> Result<u64> {
    let res = sqlx::query!(
        r#"
        DELETE FROM spam_messages
        WHERE last_seen < NOW() - make_interval(days => $1::int)
        "#,
        retention_days as i32,
    )
    .execute(pool)
    .await
    .context("DELETE spam_messages (expired)")?;
    Ok(res.rows_affected())
}
