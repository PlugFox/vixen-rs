//! `daily_stats` counters — one UPSERT helper that every M3 metric flows
//! through. `(chat_id, stat_date, kind)` is the natural key; the writer is
//! `INSERT ... ON CONFLICT DO UPDATE SET value = value + EXCLUDED.value`.
//!
//! All metric keys live in [`Metric`]. Storing them as a typed enum (instead
//! of free-form strings) keeps callers honest — a typo in `"messages_seem"`
//! would silently create a phantom counter the aggregator never reads.

use anyhow::{Context, Result};
use chrono::NaiveDate;
use sqlx::{Executor, Postgres};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Metric {
    /// Every Telegram message that reached the watched-chat gate.
    MessagesSeen,
    /// Bot- or moderator-driven message deletions (not captcha-related).
    MessagesDeleted,
    /// Bot- or moderator-driven bans.
    UsersBanned,
    /// New rows in `verified_users` (captcha solve OR `/verify`).
    UsersVerified,
    /// Captcha challenges issued.
    CaptchaIssued,
    /// Captcha challenges solved (correct digit pad input).
    CaptchaSolved,
    /// Captcha challenges that timed out (sweep job + late-solve attempts).
    CaptchaExpired,
    /// OpenAI tokens consumed by the summary feature for this chat-day.
    OpenaiTokensUsed,
}

impl Metric {
    pub fn as_db_str(self) -> &'static str {
        match self {
            Self::MessagesSeen => "messages_seen",
            Self::MessagesDeleted => "messages_deleted",
            Self::UsersBanned => "users_banned",
            Self::UsersVerified => "users_verified",
            Self::CaptchaIssued => "captcha_issued",
            Self::CaptchaSolved => "captcha_solved",
            Self::CaptchaExpired => "captcha_expired",
            Self::OpenaiTokensUsed => "openai_tokens_used",
        }
    }
}

/// UPSERT `value += by` on `(chat_id, today, metric)`. `today` is server-UTC
/// (chat-local rollup happens at the aggregator boundary in M3). Idempotent
/// in the "running multiple times accumulates correctly" sense — not in the
/// "running multiple times is a no-op" sense; callers must own dedup at
/// the call site (e.g. via `moderation_actions` uniqueness).
///
/// Generic over `Executor` so callers can pass either a `&PgPool` (one-shot
/// increment) or a `&mut PgConnection` / `&mut Transaction` (when the
/// increment must commit atomically with another write — captcha solve in
/// particular).
pub async fn increment<'e, E>(executor: E, chat_id: i64, metric: Metric, by: i64) -> Result<()>
where
    E: Executor<'e, Database = Postgres>,
{
    sqlx::query!(
        r#"
        INSERT INTO daily_stats (chat_id, date, kind, value)
        VALUES ($1, CURRENT_DATE, $2, $3)
        ON CONFLICT (chat_id, date, kind) DO UPDATE
            SET value = daily_stats.value + EXCLUDED.value
        "#,
        chat_id,
        metric.as_db_str(),
        by,
    )
    .execute(executor)
    .await
    .context("UPSERT daily_stats")?;
    Ok(())
}

/// Read a single counter for `(chat_id, date, metric)`. Returns `0` when the
/// row doesn't exist — a missing row is "the counter is implicitly zero",
/// not an error.
pub async fn get<'e, E>(executor: E, chat_id: i64, date: NaiveDate, metric: Metric) -> Result<i64>
where
    E: Executor<'e, Database = Postgres>,
{
    let row = sqlx::query_scalar!(
        r#"
        SELECT value
        FROM daily_stats
        WHERE chat_id = $1 AND date = $2 AND kind = $3
        "#,
        chat_id,
        date,
        metric.as_db_str(),
    )
    .fetch_optional(executor)
    .await
    .context("SELECT daily_stats")?;
    Ok(row.unwrap_or(0))
}

/// Outcome of a [`try_reserve`] call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReserveOutcome {
    /// Reservation committed; counter is now `new_value`.
    Reserved { new_value: i64 },
    /// Counter would exceed `budget`; nothing changed. `used` is the current
    /// counter value (so callers can include it in their skip reason).
    Rejected { used: i64 },
}

/// Atomically reserve `by` against `(chat_id, today, metric)`, gated by
/// `budget`. The INSERT/UPDATE happens in a single statement, so two
/// concurrent reservations cannot both observe the same remaining budget
/// and both succeed.
///
/// Caller must ensure `by <= budget` — the SQL only gates the UPDATE branch
/// against the budget, so a first-time INSERT could otherwise go straight to
/// `value = by` even if that already exceeds it. We mirror that constraint
/// here as an early `Rejected`.
pub async fn try_reserve(
    pool: &sqlx::PgPool,
    chat_id: i64,
    metric: Metric,
    by: i64,
    budget: i64,
) -> Result<ReserveOutcome> {
    if by > budget {
        let used = get(pool, chat_id, chrono::Utc::now().date_naive(), metric).await?;
        return Ok(ReserveOutcome::Rejected { used });
    }
    let new_value = sqlx::query_scalar!(
        r#"
        INSERT INTO daily_stats AS ds (chat_id, date, kind, value)
        VALUES ($1, CURRENT_DATE, $2, $3)
        ON CONFLICT (chat_id, date, kind) DO UPDATE
            SET value = ds.value + EXCLUDED.value
        WHERE ds.value + EXCLUDED.value <= $4
        RETURNING value
        "#,
        chat_id,
        metric.as_db_str(),
        by,
        budget,
    )
    .fetch_optional(pool)
    .await
    .context("UPSERT daily_stats (try_reserve)")?;
    match new_value {
        Some(v) => Ok(ReserveOutcome::Reserved { new_value: v }),
        None => {
            let used = get(pool, chat_id, chrono::Utc::now().date_naive(), metric).await?;
            Ok(ReserveOutcome::Rejected { used })
        }
    }
}
