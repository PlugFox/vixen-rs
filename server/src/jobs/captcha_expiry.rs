//! `captcha_expiry` background job — sweeps timed-out captchas every 60s.
//!
//! Per docs/captcha.md the M1 policy is "delete the message, give them a
//! fresh captcha next time they speak". The user is NEVER kicked for failing
//! a captcha — kicks are reserved for the M2 spam pipeline. So a sweep here
//! just deletes the captcha photo (best-effort) and writes a `captcha_expired`
//! audit row.
//!
//! The DELETE … RETURNING below is the single source of truth for "what's
//! expired"; running the loop twice in a row finds zero rows on the second
//! pass, which is the idempotency contract.
//!
//! The sweep is **batched** (`LIMIT 200` per statement) and **cancel-aware**
//! (the shutdown token is checked between batches and between rows in a
//! batch). After a long downtime the queue can be deep; without batching a
//! single statement could run for seconds and shutdown would block on the
//! whole pass.

use std::time::Duration;

use anyhow::{Context, Result};
use sqlx::PgPool;
use teloxide::prelude::*;
use teloxide::types::{ChatId, MessageId};
use tokio_util::sync::CancellationToken;
use tracing::{info, instrument, warn};

use crate::api::AppState;

pub const NAME: &str = "captcha_expiry";
pub const INTERVAL: Duration = Duration::from_secs(60);
const BATCH_SIZE: i64 = 200;

pub async fn run(bot: Bot, state: AppState, shutdown: CancellationToken) -> Result<()> {
    let mut interval = tokio::time::interval(INTERVAL);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    info!(job = NAME, interval_secs = INTERVAL.as_secs(), "starting");
    loop {
        tokio::select! {
            biased;
            _ = shutdown.cancelled() => {
                info!(job = NAME, "shutdown");
                return Ok(());
            }
            _ = interval.tick() => {
                if let Err(e) = do_one_pass(&bot, &state, &shutdown).await {
                    warn!(job = NAME, ?e, "iteration failed");
                }
            }
        }
    }
}

#[instrument(skip(bot, state, shutdown), fields(job = NAME))]
async fn do_one_pass(bot: &Bot, state: &AppState, shutdown: &CancellationToken) -> Result<()> {
    let mut total = 0usize;
    loop {
        if shutdown.is_cancelled() {
            info!(swept = total, "shutdown mid-pass");
            return Ok(());
        }
        let batch = sweep_expired_batch(state.db.pool(), BATCH_SIZE).await?;
        if batch.is_empty() {
            break;
        }
        total += batch.len();
        for row in batch {
            if shutdown.is_cancelled() {
                info!(swept = total, "shutdown mid-batch");
                return Ok(());
            }
            process_expired(bot, state.db.pool(), row).await;
        }
    }
    if total > 0 {
        info!(count = total, "expired captchas swept");
    }
    Ok(())
}

#[derive(Debug)]
struct ExpiredRow {
    chat_id: i64,
    user_id: i64,
    telegram_message_id: Option<i32>,
}

/// Delete up to `limit` expired challenges in one statement. The
/// `WHERE id IN (SELECT … LIMIT …)` form bounds the worst-case statement
/// duration after a long downtime: a deep backlog drains in chunks instead
/// of one giant `DELETE`.
async fn sweep_expired_batch(pool: &PgPool, limit: i64) -> Result<Vec<ExpiredRow>> {
    let rows = sqlx::query!(
        r#"
        DELETE FROM captcha_challenges
        WHERE id IN (
            SELECT id FROM captcha_challenges
            WHERE expires_at < NOW()
            ORDER BY expires_at
            LIMIT $1
        )
        RETURNING chat_id, user_id, telegram_message_id
        "#,
        limit,
    )
    .fetch_all(pool)
    .await
    .context("DELETE captcha_challenges (batch)")?;
    Ok(rows
        .into_iter()
        .map(|r| ExpiredRow {
            chat_id: r.chat_id,
            user_id: r.user_id,
            telegram_message_id: r.telegram_message_id,
        })
        .collect())
}

async fn process_expired(bot: &Bot, pool: &PgPool, row: ExpiredRow) {
    let chat_id = ChatId(row.chat_id);

    if let Some(mid) = row.telegram_message_id {
        let _ = bot.delete_message(chat_id, MessageId(mid)).await;
    }

    if let Err(e) = ledger_expired(pool, &row).await {
        warn!(?e, "ledger insert (captcha_expired) failed");
    }
}

async fn ledger_expired(pool: &PgPool, row: &ExpiredRow) -> Result<()> {
    sqlx::query!(
        r#"
        INSERT INTO moderation_actions
            (chat_id, target_user_id, action, actor_kind, message_id, reason)
        VALUES ($1, $2, 'captcha_expired', 'bot', $3, 'lifetime')
        ON CONFLICT DO NOTHING
        "#,
        row.chat_id,
        row.user_id,
        row.telegram_message_id,
    )
    .execute(pool)
    .await?;
    Ok(())
}
