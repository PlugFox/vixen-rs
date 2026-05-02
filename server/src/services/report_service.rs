//! Daily-report aggregator. Pure-DB; produces a [`ReportData`] from
//! `daily_stats`, `moderation_actions`, `spam_messages` and
//! `chat_info_cache` for a `(chat_id, [from, to))` window.
//!
//! Design: one query per metric, keyed on the `daily_stats(chat_id, date,
//! kind)` index for cheap re-aggregation. No N+1 — top phrases come from a
//! single ORDER BY on `spam_messages`. The renderer + chart are pure
//! functions of the returned struct, so unit tests don't need a bot.

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, NaiveDate, Utc};
use sqlx::PgPool;

use crate::models::report::{CaptchaCounts, DailyPoint, ReportData, TopPhrase};

/// How many spam-phrase samples the renderer can fit in one MarkdownV2
/// message. Ten is the upper bound the issue spec mentions.
const TOP_PHRASES_LIMIT: i64 = 10;

/// Sparkline window length. Always emits a fully-padded `Vec` of this
/// length so the renderer doesn't have to handle ragged input.
const SPARKLINE_DAYS: i64 = 7;

#[derive(Clone)]
pub struct ReportService {
    db: PgPool,
}

impl ReportService {
    pub fn new(db: PgPool) -> Self {
        Self { db }
    }

    pub fn pool(&self) -> &PgPool {
        &self.db
    }

    /// Aggregate everything the renderer + chart need. The `[from, to)`
    /// window is half-open by convention; both bounds are UTC and the
    /// rollup uses each row's `created_at` for moderation_actions and the
    /// stat_date for daily_stats. `daily_stats` rows are server-UTC dated;
    /// the M3 design intentionally avoids reaggregating per chat-local day
    /// because the gain (cleaner midnight boundaries) doesn't justify a
    /// double-precision date column.
    pub async fn aggregate(
        &self,
        chat_id: i64,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<ReportData> {
        let from_date: NaiveDate = from.date_naive();
        let to_date: NaiveDate = to.date_naive();

        let chat_title = sqlx::query_scalar!(
            r#"SELECT title FROM chat_info_cache WHERE chat_id = $1"#,
            chat_id,
        )
        .fetch_optional(&self.db)
        .await
        .context("SELECT chat_info_cache.title")?;

        let messages_seen = self
            .sum_metric(chat_id, "messages_seen", from_date, to_date)
            .await?;

        // moderation_actions counters use created_at — the daily_stats
        // mirror exists for cheap reads but the source-of-truth count for
        // the report is the ledger (so retroactive bans counted on the
        // right day even if the daily_stats UPSERT raced something).
        let messages_deleted = count_actions(&self.db, chat_id, "delete", from, to).await?;
        let users_verified = count_actions(&self.db, chat_id, "verify", from, to).await?;
        let users_banned = count_actions(&self.db, chat_id, "ban", from, to).await?;

        let captcha = CaptchaCounts {
            issued: self
                .sum_metric(chat_id, "captcha_issued", from_date, to_date)
                .await?,
            solved: self
                .sum_metric(chat_id, "captcha_solved", from_date, to_date)
                .await?,
            expired: self
                .sum_metric(chat_id, "captcha_expired", from_date, to_date)
                .await?,
        };

        let top_phrases = sqlx::query!(
            r#"
            SELECT sample_body, hit_count
            FROM spam_messages
            WHERE last_seen >= $1 AND last_seen < $2
            ORDER BY hit_count DESC, last_seen DESC
            LIMIT $3
            "#,
            from,
            to,
            TOP_PHRASES_LIMIT,
        )
        .fetch_all(&self.db)
        .await
        .context("SELECT spam_messages (top phrases)")?
        .into_iter()
        .map(|r| TopPhrase {
            text: r.sample_body,
            hits: r.hit_count,
        })
        .collect();

        let last_7_days_messages = self.sparkline(chat_id, to_date).await?;

        Ok(ReportData {
            chat_id,
            from,
            to,
            chat_title,
            messages_seen,
            messages_deleted,
            users_verified,
            users_banned,
            captcha,
            top_phrases,
            last_7_days_messages,
        })
    }

    /// SUM(value) over `daily_stats` for a metric and `[from_date, to_date]`
    /// inclusive lower / inclusive upper window. Returns 0 when no rows
    /// exist (`COALESCE`).
    async fn sum_metric(
        &self,
        chat_id: i64,
        kind: &str,
        from_date: NaiveDate,
        to_date: NaiveDate,
    ) -> Result<i64> {
        let row = sqlx::query_scalar!(
            r#"
            SELECT COALESCE(SUM(value), 0)::BIGINT AS "sum!"
            FROM daily_stats
            WHERE chat_id = $1 AND kind = $2 AND date >= $3 AND date <= $4
            "#,
            chat_id,
            kind,
            from_date,
            to_date,
        )
        .fetch_one(&self.db)
        .await
        .context("SUM daily_stats")?;
        Ok(row)
    }

    /// Last 7 days (inclusive of `to_date`) of `messages_seen`, oldest
    /// first. Missing days are zero — the result is always exactly
    /// [`SPARKLINE_DAYS`] long so the renderer can index it directly.
    async fn sparkline(&self, chat_id: i64, to_date: NaiveDate) -> Result<Vec<DailyPoint>> {
        let from_date = to_date - Duration::days(SPARKLINE_DAYS - 1);
        let rows = sqlx::query!(
            r#"
            SELECT date, value
            FROM daily_stats
            WHERE chat_id = $1 AND kind = 'messages_seen'
              AND date >= $2 AND date <= $3
            "#,
            chat_id,
            from_date,
            to_date,
        )
        .fetch_all(&self.db)
        .await
        .context("SELECT daily_stats (sparkline)")?;

        let mut by_date = std::collections::BTreeMap::new();
        for r in rows {
            by_date.insert(r.date, r.value);
        }

        let mut out = Vec::with_capacity(SPARKLINE_DAYS as usize);
        for offset in 0..SPARKLINE_DAYS {
            let day = from_date + Duration::days(offset);
            out.push(DailyPoint {
                date: day,
                messages: by_date.get(&day).copied().unwrap_or(0),
            });
        }
        Ok(out)
    }
}

/// COUNT moderation_actions rows for a `(chat_id, action, [from, to))` window.
/// Lives outside the impl so the daily_report job can call it directly with
/// just a `&PgPool` if it wants — no need to construct a service.
async fn count_actions(
    pool: &PgPool,
    chat_id: i64,
    action: &str,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Result<i64> {
    let row = sqlx::query_scalar!(
        r#"
        SELECT COUNT(*)::BIGINT AS "n!"
        FROM moderation_actions
        WHERE chat_id = $1 AND action = $2
          AND created_at >= $3 AND created_at < $4
        "#,
        chat_id,
        action,
        from,
        to,
    )
    .fetch_one(pool)
    .await
    .context("COUNT moderation_actions")?;
    Ok(row)
}

/// Convenience: build a UTC `[from, to)` window covering the last 24 hours
/// ending at `to` (inclusive of `to.date_naive()` on the daily_stats side).
/// Used by `/stats` and the daily-report scheduler when no explicit window
/// is supplied.
pub fn last_24h_window(to: DateTime<Utc>) -> (DateTime<Utc>, DateTime<Utc>) {
    (to - Duration::hours(24), to)
}

/// Convenience: window covering the entire chat-local day for `date`. The
/// daily_stats column is server-UTC, so this just bounds at the UTC midnights
/// surrounding the given date — consistent with the way [`sum_metric`] reads.
pub fn day_window_utc(date: NaiveDate) -> (DateTime<Utc>, DateTime<Utc>) {
    let start = date
        .and_hms_opt(0, 0, 0)
        .expect("midnight is always valid")
        .and_utc();
    let end = (date + Duration::days(1))
        .and_hms_opt(0, 0, 0)
        .expect("midnight is always valid")
        .and_utc();
    (start, end)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Datelike;

    #[test]
    fn last_24h_window_is_24h_long() {
        let now = Utc::now();
        let (from, to) = last_24h_window(now);
        assert_eq!(to - from, Duration::hours(24));
    }

    #[test]
    fn day_window_utc_spans_one_day() {
        let date = NaiveDate::from_ymd_opt(2026, 5, 3).unwrap();
        let (from, to) = day_window_utc(date);
        assert_eq!(to - from, Duration::days(1));
        assert_eq!(from.date_naive(), date);
        assert_eq!(from.day(), 3);
    }
}
