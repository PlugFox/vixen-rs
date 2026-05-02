//! `ReportData` — the in-memory aggregate the renderer + chart consume.
//!
//! Built by [`crate::services::report_service::ReportService::aggregate`]
//! from `daily_stats`, `moderation_actions` and `spam_messages`. No I/O once
//! constructed — the renderer and chart are pure functions of this struct.

use chrono::{DateTime, NaiveDate, Utc};

#[derive(Debug, Clone)]
pub struct ReportData {
    pub chat_id: i64,
    /// Inclusive lower bound of the aggregation window (UTC).
    pub from: DateTime<Utc>,
    /// Exclusive upper bound of the aggregation window (UTC).
    pub to: DateTime<Utc>,
    /// Chat title, when known. Resolved from `chat_info_cache`. The renderer
    /// MUST escape this for MarkdownV2 because it's user-supplied.
    pub chat_title: Option<String>,

    pub messages_seen: i64,
    pub messages_deleted: i64,
    pub users_verified: i64,
    pub users_banned: i64,
    pub captcha: CaptchaCounts,

    /// Top-N spam phrases by `hit_count`, descending. Source is the global
    /// `spam_messages` table (xxh3-keyed, not chat-scoped). The renderer
    /// MUST escape `text` for MarkdownV2 — phrase samples are user-supplied.
    /// TODO(M6): redact via the to-be-introduced `redact::phrase` helper
    /// before exposing in any public-facing surface; for now the in-chat
    /// report shows the raw match.
    pub top_phrases: Vec<TopPhrase>,

    /// `messages_seen` for the last 7 calendar days (server-UTC), oldest
    /// first. Used by the renderer for the sparkline. Length is always 7;
    /// missing days are zero.
    pub last_7_days_messages: Vec<DailyPoint>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct CaptchaCounts {
    pub issued: i64,
    pub solved: i64,
    pub expired: i64,
}

impl CaptchaCounts {
    pub fn total(&self) -> i64 {
        self.issued + self.solved + self.expired
    }
}

#[derive(Debug, Clone)]
pub struct TopPhrase {
    pub text: String,
    pub hits: i64,
}

#[derive(Debug, Clone, Copy)]
pub struct DailyPoint {
    pub date: NaiveDate,
    pub messages: i64,
}
