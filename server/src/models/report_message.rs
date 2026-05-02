//! `report_messages` row + writers/readers used by the M3 daily-report flow.
//!
//! Schema (from migration 20260503000000_m3_reports):
//!
//! ```text
//! report_messages (chat_id, report_date, kind, telegram_message_id, generated_at)
//!   PRIMARY KEY (chat_id, report_date, kind)
//!   kind ∈ {'daily_text', 'daily_photo'}
//! ```
//!
//! `kind` discriminates the two messages a daily report posts (the MarkdownV2
//! text block and the WebP chart photo). Replace-on-redo: when the same chat
//! re-runs on the same `report_date`, [`prior_today`] returns both rows, the
//! caller `delete_message`s them via the bot, [`delete_for_day`] clears the
//! ledger, and [`record`] inserts the fresh pair.

use anyhow::{Context, Result};
use chrono::NaiveDate;
use sqlx::PgPool;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReportKind {
    Text,
    Photo,
}

impl ReportKind {
    pub fn as_db_str(self) -> &'static str {
        match self {
            Self::Text => "daily_text",
            Self::Photo => "daily_photo",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ReportMessage {
    pub kind: ReportKind,
    pub telegram_message_id: i32,
}

/// Record a freshly-sent report message. Uses INSERT…ON CONFLICT DO UPDATE so
/// a re-run replaces the message_id without needing a prior DELETE — but the
/// caller still issues `delete_message` on the old id (returned by
/// [`prior_today`]) before calling `record`, otherwise the old chat message
/// is orphaned in Telegram.
pub async fn record(
    pool: &PgPool,
    chat_id: i64,
    report_date: NaiveDate,
    kind: ReportKind,
    telegram_message_id: i32,
) -> Result<()> {
    sqlx::query!(
        r#"
        INSERT INTO report_messages (chat_id, report_date, kind, telegram_message_id)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (chat_id, report_date, kind) DO UPDATE
            SET telegram_message_id = EXCLUDED.telegram_message_id,
                generated_at        = NOW()
        "#,
        chat_id,
        report_date,
        kind.as_db_str(),
        telegram_message_id,
    )
    .execute(pool)
    .await
    .context("INSERT report_messages")?;
    Ok(())
}

/// All report rows for `(chat_id, report_date)`. Returns at most two entries
/// (text + photo). Empty Vec when no report has been posted for that day.
pub async fn prior_today(
    pool: &PgPool,
    chat_id: i64,
    report_date: NaiveDate,
) -> Result<Vec<ReportMessage>> {
    let rows = sqlx::query!(
        r#"
        SELECT kind, telegram_message_id
        FROM report_messages
        WHERE chat_id = $1 AND report_date = $2
        "#,
        chat_id,
        report_date,
    )
    .fetch_all(pool)
    .await
    .context("SELECT report_messages")?;

    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        let kind = match r.kind.as_str() {
            "daily_text" => ReportKind::Text,
            "daily_photo" => ReportKind::Photo,
            other => {
                tracing::warn!(kind = %other, "unknown report_messages.kind, ignoring");
                continue;
            }
        };
        out.push(ReportMessage {
            kind,
            telegram_message_id: r.telegram_message_id,
        });
    }
    Ok(out)
}

/// Drop every row for `(chat_id, report_date)`. Called after the bot has
/// (best-effort) deleted the corresponding Telegram messages, just before
/// re-inserting the new pair.
pub async fn delete_for_day(pool: &PgPool, chat_id: i64, report_date: NaiveDate) -> Result<()> {
    sqlx::query!(
        r#"
        DELETE FROM report_messages
        WHERE chat_id = $1 AND report_date = $2
        "#,
        chat_id,
        report_date,
    )
    .execute(pool)
    .await
    .context("DELETE report_messages")?;
    Ok(())
}

/// True iff **both** the text and the photo rows are present for
/// `(chat_id, report_date)`. The scheduler uses this to skip days that have
/// already been fully posted; if `deliver()` succeeded on the text message
/// but failed on the chart (e.g. spawn-blocking error, send_photo timeout),
/// only one row will exist and the next 5-min tick must retry the missing
/// half rather than treating the day as done. Reading the count rather than
/// EXISTS keeps the predicate transactionally consistent with `record`.
pub async fn already_posted_today(
    pool: &PgPool,
    chat_id: i64,
    report_date: NaiveDate,
) -> Result<bool> {
    let count = sqlx::query_scalar!(
        r#"
        SELECT COUNT(*)::BIGINT AS "n!"
        FROM report_messages
        WHERE chat_id = $1 AND report_date = $2
          AND kind IN ('daily_text', 'daily_photo')
        "#,
        chat_id,
        report_date,
    )
    .fetch_one(pool)
    .await
    .context("SELECT COUNT report_messages")?;
    Ok(count >= 2)
}
