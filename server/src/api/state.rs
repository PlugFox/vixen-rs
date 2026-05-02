//! Shared application state passed to every handler via `axum::extract::State`
//! and to the teloxide dispatcher via `dptree::deps!`.

use std::sync::Arc;

use crate::config::Config;
use crate::database::{Database, Redis};
use crate::services::captcha::{CaptchaService, CaptchaState};
use crate::services::moderation_service::ModerationService;
use crate::services::report_service::ReportService;
use crate::services::spam::service::SpamService;
use crate::services::summary_service::SummaryService;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub db: Arc<Database>,
    pub redis: Arc<Redis>,
    pub captcha: Arc<CaptchaService>,
    /// Ephemeral captcha state in Redis: in-progress digit input,
    /// callback meta (owner + uuid_short keyed by message), and the
    /// `is_verified` cache. PG owns the durable challenge row and
    /// verified-user ledger; this is the UI scratchpad alongside it.
    pub captcha_state: Arc<CaptchaState>,
    /// M2 spam pipeline: normalize → xxh3 dedup → CAS → n-gram cascade.
    /// Returns a `Verdict` that the handler dispatches through `moderation`.
    pub spam: Arc<SpamService>,
    /// Centralised moderation: idempotent ledger + bot side-effect for every
    /// ban / unban / delete (auto or manual).
    pub moderation: Arc<ModerationService>,
    /// M3 report aggregator: pure-DB read of `daily_stats` /
    /// `moderation_actions` / `spam_messages` into `ReportData`.
    pub reports: Arc<ReportService>,
    /// M3 AI summary: per-chat OpenAI key resolved at call time, daily
    /// token budget enforced via `daily_stats('openai_tokens_used')`.
    pub summary: Arc<SummaryService>,
}
