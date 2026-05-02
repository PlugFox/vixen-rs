//! `captcha_expiry` background job — sweeps timed-out captchas every 60s.
//!
//! Per docs/captcha.md the policy is "give up = leave, not banned forever":
//! the user gets kicked (kick = ban + immediate unban so they can rejoin),
//! the captcha photo is deleted, and two ledger rows record the outcome.
//!
//! The DELETE … RETURNING below is the single source of truth for "what's
//! expired"; running the loop twice in a row finds zero rows on the second
//! pass, which is the idempotency contract.

use std::time::Duration;

use anyhow::{Context, Result};
use sqlx::PgPool;
use teloxide::prelude::*;
use teloxide::types::{ChatId, MessageId, UserId};
use tokio_util::sync::CancellationToken;
use tracing::{info, instrument, warn};

use crate::api::AppState;

pub const NAME: &str = "captcha_expiry";
pub const INTERVAL: Duration = Duration::from_secs(60);

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
                if let Err(e) = do_one_pass(&bot, &state).await {
                    warn!(job = NAME, ?e, "iteration failed");
                }
            }
        }
    }
}

#[instrument(skip(bot, state), fields(job = NAME))]
async fn do_one_pass(bot: &Bot, state: &AppState) -> Result<()> {
    let expired = sweep_expired(state.db.pool()).await?;
    if expired.is_empty() {
        return Ok(());
    }

    info!(count = expired.len(), "expired captchas swept");
    for row in expired {
        process_expired(bot, state.db.pool(), row).await;
    }
    Ok(())
}

#[derive(Debug)]
struct ExpiredRow {
    chat_id: i64,
    user_id: i64,
    telegram_message_id: Option<i64>,
}

/// Delete every expired challenge in one statement and stream them back. The
/// DELETE … RETURNING is atomic: a parallel job replica running the same
/// query gets the rows it picked up and nobody else sees them.
async fn sweep_expired(pool: &PgPool) -> Result<Vec<ExpiredRow>> {
    let rows = sqlx::query!(
        r#"
        DELETE FROM captcha_challenges
        WHERE expires_at < NOW()
        RETURNING chat_id, user_id, telegram_message_id
        "#,
    )
    .fetch_all(pool)
    .await
    .context("DELETE captcha_challenges WHERE expires_at < NOW()")?;
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
    let user_id = UserId(row.user_id as u64);

    if let Some(mid) = row.telegram_message_id {
        let _ = bot.delete_message(chat_id, MessageId(mid as i32)).await;
    }
    let _ = bot.unban_chat_member(chat_id, user_id).await;
    if let Err(e) = bot.kick_chat_member(chat_id, user_id).await {
        warn!(error = %e, chat_id = row.chat_id, user_id = row.user_id, "kick failed");
    }
    let _ = bot.unban_chat_member(chat_id, user_id).await;

    if let Err(e) = ledger_expired(pool, &row).await {
        warn!(?e, "ledger insert (captcha_expired) failed");
    }
    if let Err(e) = ledger_kick(pool, &row).await {
        warn!(?e, "ledger insert (kick) failed");
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

async fn ledger_kick(pool: &PgPool, row: &ExpiredRow) -> Result<()> {
    sqlx::query!(
        r#"
        INSERT INTO moderation_actions
            (chat_id, target_user_id, action, actor_kind, message_id, reason)
        VALUES ($1, $2, 'kick', 'bot', $3, 'captcha_expired')
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
