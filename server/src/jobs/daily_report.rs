//! `daily_report` job — fires a per-chat report at the chat-local hour.
//!
//! The scheduler ticks every 5 minutes. On each tick:
//!
//! 1. Iterate over every watched chat (`Config::chats`).
//! 2. Read `chat_config.{report_hour, timezone, report_min_activity,
//!    summary_enabled, language}`.
//! 3. Compute current chat-local time. Fire iff:
//!      * `chat_local.hour() == report_hour`,
//!      * `chat_local.minute() < TICK_INTERVAL.minutes()`, and
//!      * no `report_messages` row exists for `(chat_id, chat_local.date())`.
//! 4. Aggregate via `ReportService`. Skip silently if `messages_seen <
//!    report_min_activity` (low-activity day).
//! 5. Render text + chart, optionally generate AI summary, post both
//!    messages, record in `report_messages`.
//!
//! This loop is idempotent on re-fire (the `report_messages` lookup gates
//! re-sends), so the 5-min cadence + ±5-min fire window safely covers
//! restart-during-tick, missed-tick (e.g. clock skew), and graceful-shutdown
//! windows.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use chrono::{NaiveDate, Timelike, Utc};
use chrono_tz::Tz;
use sqlx::PgPool;
use teloxide::prelude::*;
use teloxide::types::{ChatId, InputFile, MessageId, ParseMode};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, instrument, warn};

use crate::api::AppState;
use crate::models::report_message::{self, ReportKind};
use crate::services::report_render::{HeaderKind, Lang};
use crate::services::report_service::{ReportService, day_window_utc};
use crate::services::summary_service::SummaryOutcome;
use crate::services::{chart_service, report_render};

pub const NAME: &str = "daily_report";
pub const INTERVAL: Duration = Duration::from_secs(5 * 60);

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
    let chats = state.config.chats.clone();
    let report_service = Arc::new(ReportService::new(state.db.pool().clone()));
    for chat_id in chats {
        if let Err(e) = maybe_fire(bot, state, &report_service, chat_id).await {
            warn!(chat_id, ?e, "daily_report iteration for chat failed");
        }
    }
    Ok(())
}

#[derive(Debug, Clone)]
struct ScheduleConfig {
    report_hour: i16,
    timezone: String,
    min_activity: i16,
    language: String,
    summary_enabled: bool,
}

async fn fetch_schedule_config(pool: &PgPool, chat_id: i64) -> Result<Option<ScheduleConfig>> {
    let row = sqlx::query!(
        r#"
        SELECT report_hour, timezone, report_min_activity, language, summary_enabled
        FROM chat_config
        WHERE chat_id = $1
        "#,
        chat_id,
    )
    .fetch_optional(pool)
    .await
    .context("SELECT chat_config (schedule)")?;
    Ok(row.map(|r| ScheduleConfig {
        report_hour: r.report_hour,
        timezone: r.timezone,
        min_activity: r.report_min_activity,
        language: r.language,
        summary_enabled: r.summary_enabled,
    }))
}

async fn maybe_fire(
    bot: &Bot,
    state: &AppState,
    reports: &Arc<ReportService>,
    chat_id: i64,
) -> Result<()> {
    let cfg = match fetch_schedule_config(state.db.pool(), chat_id).await? {
        Some(c) => c,
        None => return Ok(()),
    };
    let tz = match cfg.timezone.parse::<Tz>() {
        Ok(t) => t,
        Err(e) => {
            warn!(chat_id, tz = %cfg.timezone, error = %e, "invalid timezone, skipping");
            return Ok(());
        }
    };

    let now_local = Utc::now().with_timezone(&tz);
    if now_local.hour() as i16 != cfg.report_hour {
        return Ok(());
    }
    if now_local.minute() >= (INTERVAL.as_secs() / 60) as u32 {
        // Outside the ±INTERVAL window — wait for the next hour or for the
        // next scheduler day. Keeps us from firing twice when the operator
        // restarts the bot mid-hour.
        return Ok(());
    }

    let report_date: NaiveDate = now_local.date_naive();
    if report_message::already_posted_today(state.db.pool(), chat_id, report_date).await? {
        debug!(chat_id, %report_date, "already posted, skipping");
        return Ok(());
    }

    let (from, to) = day_window_utc(report_date);
    let report = reports.aggregate(chat_id, from, to).await?;
    if report.messages_seen < cfg.min_activity as i64 {
        info!(
            chat_id,
            messages_seen = report.messages_seen,
            min = cfg.min_activity,
            "below activity threshold, skipping"
        );
        return Ok(());
    }

    deliver(
        bot,
        state,
        chat_id,
        report_date,
        &cfg.language,
        cfg.summary_enabled,
        &report,
        HeaderKind::Daily,
    )
    .await?;
    Ok(())
}

/// Common send / record logic, shared by the scheduler and the on-demand
/// `/report` command. `summary_enabled` is the chat-config flag; the
/// summary is also gated by the per-chat OpenAI key resolved inside
/// `summary_service`.
#[allow(clippy::too_many_arguments)]
pub async fn deliver(
    bot: &Bot,
    state: &AppState,
    chat_id: i64,
    report_date: NaiveDate,
    language: &str,
    summary_enabled: bool,
    report: &crate::models::report::ReportData,
    header: HeaderKind,
) -> Result<()> {
    // Replace-on-redo: best-effort delete prior pair, then drop ledger rows.
    let prior = report_message::prior_today(state.db.pool(), chat_id, report_date).await?;
    let chat = ChatId(chat_id);
    for m in &prior {
        if let Err(e) = bot
            .delete_message(chat, MessageId(m.telegram_message_id))
            .await
        {
            warn!(error = %e, kind = ?m.kind, "delete prior report message failed");
        }
    }
    if !prior.is_empty() {
        report_message::delete_for_day(state.db.pool(), chat_id, report_date).await?;
    }

    let lang = Lang::from_db_str(language);
    let body = report_render::render(report, lang, header);
    let text_msg = bot
        .send_message(chat, body)
        .parse_mode(ParseMode::MarkdownV2)
        .await
        .context("send_message (report text)")?;
    report_message::record(
        state.db.pool(),
        chat_id,
        report_date,
        ReportKind::Text,
        text_msg.id.0,
    )
    .await?;

    let chart_bytes = {
        let report_owned = report.clone();
        tokio::task::spawn_blocking(move || chart_service::render(&report_owned))
            .await
            .context("chart spawn_blocking join")??
    };

    let caption = if summary_enabled {
        let outcome = state
            .summary
            .summarize(chat_id, report.from, report.to, language)
            .await?;
        match outcome {
            SummaryOutcome::Generated { text, .. } => Some(report_render::escape(&text)),
            SummaryOutcome::Skipped { reason } => {
                debug!(chat_id, ?reason, "summary skipped");
                None
            }
        }
    } else {
        None
    };

    let photo = InputFile::memory(chart_bytes).file_name("report.webp");
    let mut req = bot.send_photo(chat, photo);
    if let Some(c) = caption {
        req = req.caption(c).parse_mode(ParseMode::MarkdownV2);
    }
    let photo_msg = req.await.context("send_photo (report chart)")?;
    report_message::record(
        state.db.pool(),
        chat_id,
        report_date,
        ReportKind::Photo,
        photo_msg.id.0,
    )
    .await?;

    info!(chat_id, %report_date, "daily report delivered");
    Ok(())
}

/// Helper for `/report` handlers: convert the bot's UTC `now` into the
/// chat-local date used as the report key, falling back to UTC if the
/// chat's `timezone` is malformed.
pub async fn current_report_date(pool: &PgPool, chat_id: i64) -> Result<NaiveDate> {
    let cfg = fetch_schedule_config(pool, chat_id).await?;
    let tz = cfg
        .as_ref()
        .and_then(|c| c.timezone.parse::<Tz>().ok())
        .unwrap_or(chrono_tz::UTC);
    Ok(Utc::now().with_timezone(&tz).date_naive())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interval_is_5_minutes() {
        assert_eq!(INTERVAL.as_secs(), 300);
    }

    #[test]
    fn timezone_parses_known_iana() {
        let _: Tz = "Europe/Berlin".parse().unwrap();
        let _: Tz = "UTC".parse().unwrap();
    }
}
