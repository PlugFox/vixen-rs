//! AI summary of recent chat activity. Per-chat: each chat has its own
//! `chat_config.openai_api_key` (or NULL = no summary for this chat) and
//! `chat_config.openai_model`. Token usage accumulates per chat-day in
//! `daily_stats('openai_tokens_used')` and is hard-capped at
//! `chat_config.summary_token_budget`.
//!
//! Inputs come from `allowed_messages` (filled by the spam pipeline when
//! `chat_config.log_allowed_messages = TRUE`). With message logging off,
//! the service has nothing to send to OpenAI and returns
//! [`SummaryOutcome::Skipped`] with [`SkipReason::NoMessages`] — that's a
//! fail-quiet design choice: a moderator who hasn't enabled logging
//! shouldn't be surprised by an extra "logging is off" reminder every day.
//!
//! Sanitisation runs on every message body before we hit OpenAI:
//!   * URLs collapse to `[link]`.
//!   * Phone-like sequences (≥7 digits, optional leading `+`) → `[phone]`.
//!   * Emails → `[email]`.
//!   * `@username` → `[user]`.
//!
//! See `server/docs/reports.md` for the moderator-facing contract.

use std::sync::{Arc, LazyLock};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use regex::Regex;
use sqlx::PgPool;
use tracing::{debug, info, instrument, warn};

use crate::models::daily_stats::{self, Metric, ReserveOutcome};
use crate::services::openai_client::{ChatMessage, ChatRole, OpenAiClient};

/// Hard limit on how many `allowed_messages` rows to feed into one summary
/// request. Roughly equivalent to ~20 KB of text — well inside the
/// 128k-token context of `gpt-4o-mini` even before any sanitisation.
const MAX_MESSAGES: usize = 2000;

/// Cap on the model's output. The renderer expects a short bullet list, not
/// a treatise. Caps the per-call token usage tightly.
const MAX_OUTPUT_TOKENS: u32 = 500;

/// Min content length per message — single-character "+1" / "ok" replies
/// add nothing to a summary and just drag the prompt budget down.
const MIN_MESSAGE_CHARS: usize = 4;

#[derive(Debug, Clone)]
pub enum SummaryOutcome {
    Generated { text: String, tokens_used: u32 },
    Skipped { reason: SkipReason },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    /// `chat_config.openai_api_key` is NULL.
    NoApiKey,
    /// `chat_config.summary_enabled = FALSE`.
    Disabled,
    /// `chat_config.log_allowed_messages = FALSE` or no rows in the window.
    NoMessages,
    /// Today's accumulated `openai_tokens_used` ≥ `summary_token_budget`.
    BudgetExhausted { used: i64, budget: i64 },
}

#[derive(Clone)]
pub struct SummaryService {
    db: PgPool,
    client: Arc<OpenAiClient>,
}

impl SummaryService {
    pub fn new(db: PgPool, client: Arc<OpenAiClient>) -> Arc<Self> {
        Arc::new(Self { db, client })
    }

    /// Resolve the per-chat config + budget, build a sanitised prompt, call
    /// OpenAI, and (on success) increment the per-day token counter. The
    /// caller decides what to do with `Generated` / `Skipped`; the renderer
    /// uses the `text` directly, the `/summary` command formats the skip
    /// reason for the user.
    #[instrument(skip(self), fields(chat_id))]
    pub async fn summarize(
        &self,
        chat_id: i64,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        language: &str,
    ) -> Result<SummaryOutcome> {
        let cfg = match self.fetch_config(chat_id).await? {
            Some(c) => c,
            None => {
                return Ok(SummaryOutcome::Skipped {
                    reason: SkipReason::NoApiKey,
                });
            }
        };

        if !cfg.summary_enabled {
            return Ok(SummaryOutcome::Skipped {
                reason: SkipReason::Disabled,
            });
        }
        let Some(api_key) = cfg.openai_api_key.as_ref() else {
            return Ok(SummaryOutcome::Skipped {
                reason: SkipReason::NoApiKey,
            });
        };

        // If allowed-message logging is disabled, the chat has not opted in
        // to having its raw text leave the bot. Don't reach for stale rows
        // captured before the moderator turned the flag off — short-circuit
        // before either reading allowed_messages or hitting OpenAI.
        if !cfg.log_allowed_messages {
            return Ok(SummaryOutcome::Skipped {
                reason: SkipReason::NoMessages,
            });
        }

        // Cheap pre-flight: if today's counter already meets/exceeds the
        // configured budget there's nothing to reserve, and we want to
        // surface BudgetExhausted before we go fetch any messages or hit
        // OpenAI. The atomic check below is the actual race-safe gate.
        let budget = cfg.summary_token_budget as i64;
        let used_today = daily_stats::get(
            &self.db,
            chat_id,
            chrono::Utc::now().date_naive(),
            Metric::OpenaiTokensUsed,
        )
        .await?;
        if used_today >= budget {
            return Ok(SummaryOutcome::Skipped {
                reason: SkipReason::BudgetExhausted {
                    used: used_today,
                    budget,
                },
            });
        }

        let messages = self.fetch_messages(chat_id, from, to).await?;
        if messages.is_empty() {
            return Ok(SummaryOutcome::Skipped {
                reason: SkipReason::NoMessages,
            });
        }

        // Pre-charge the worst-case output token count atomically. Two
        // concurrent /summary calls can't both observe the same remaining
        // budget — the second one trips the SQL-side WHERE gate and gets
        // Rejected. After the OpenAI call returns, we adjust by
        // `actual - reserve` (positive = overshoot, negative = refund).
        let reserve = MAX_OUTPUT_TOKENS as i64;
        match daily_stats::try_reserve(&self.db, chat_id, Metric::OpenaiTokensUsed, reserve, budget)
            .await?
        {
            ReserveOutcome::Reserved { .. } => {}
            ReserveOutcome::Rejected { used } => {
                return Ok(SummaryOutcome::Skipped {
                    reason: SkipReason::BudgetExhausted { used, budget },
                });
            }
        }

        let prompt = build_user_prompt(&messages);
        let chat_messages = vec![
            ChatMessage {
                role: ChatRole::System,
                content: system_prompt(language),
            },
            ChatMessage {
                role: ChatRole::User,
                content: prompt,
            },
        ];

        debug!(
            chat_id,
            inputs = messages.len(),
            "calling openai for chat summary"
        );

        let completion = self
            .client
            .chat(api_key, &cfg.openai_model, chat_messages, MAX_OUTPUT_TOKENS)
            .await
            .context("openai chat call")?;

        // Reconcile the pre-charge against actual usage. A full-output
        // response with a small prompt may settle below `reserve` (refund);
        // a long-prompt response may push past it (overshoot, but still
        // bounded — the next call's reservation will see the higher counter
        // and reject).
        let delta = completion.total_tokens as i64 - reserve;
        if delta != 0 {
            if let Err(e) =
                daily_stats::increment(&self.db, chat_id, Metric::OpenaiTokensUsed, delta).await
            {
                warn!(error = ?e, "failed to reconcile openai_tokens_used");
            }
        }

        info!(
            chat_id,
            tokens = completion.total_tokens,
            "summary generated"
        );
        Ok(SummaryOutcome::Generated {
            text: completion.content,
            tokens_used: completion.total_tokens,
        })
    }

    async fn fetch_config(&self, chat_id: i64) -> Result<Option<SummaryConfig>> {
        let row = sqlx::query_as!(
            SummaryConfig,
            r#"
            SELECT
                summary_enabled       AS "summary_enabled!",
                summary_token_budget  AS "summary_token_budget!",
                openai_api_key,
                openai_model          AS "openai_model!",
                log_allowed_messages  AS "log_allowed_messages!"
            FROM chat_config
            WHERE chat_id = $1
            "#,
            chat_id,
        )
        .fetch_optional(&self.db)
        .await
        .context("SELECT chat_config (summary)")?;
        Ok(row)
    }

    async fn fetch_messages(
        &self,
        chat_id: i64,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<String>> {
        let rows = sqlx::query!(
            r#"
            SELECT content
            FROM allowed_messages
            WHERE chat_id = $1 AND created_at >= $2 AND created_at < $3
              AND content IS NOT NULL
            ORDER BY created_at ASC
            LIMIT $4
            "#,
            chat_id,
            from,
            to,
            MAX_MESSAGES as i64,
        )
        .fetch_all(&self.db)
        .await
        .context("SELECT allowed_messages")?;
        Ok(rows
            .into_iter()
            .filter_map(|r| r.content)
            .filter(|c| c.chars().count() >= MIN_MESSAGE_CHARS)
            .map(|c| sanitize(&c))
            .collect())
    }
}

#[derive(Debug)]
struct SummaryConfig {
    summary_enabled: bool,
    summary_token_budget: i32,
    openai_api_key: Option<String>,
    openai_model: String,
    log_allowed_messages: bool,
}

fn system_prompt(language: &str) -> String {
    let lang = match language {
        "en" => "English",
        _ => "Russian",
    };
    format!(
        "You are summarising a Telegram chat. Output 3-5 short bullet points \
         in {lang}. Focus on topics and decisions, NOT individuals. Do not \
         include URLs, phone numbers, emails, or @-mentions even if present \
         in the input. Do not invent facts."
    )
}

fn build_user_prompt(messages: &[String]) -> String {
    let mut s = String::with_capacity(messages.iter().map(|m| m.len() + 2).sum());
    for m in messages {
        s.push_str("- ");
        s.push_str(m);
        s.push('\n');
    }
    s
}

// ── sanitisation ───────────────────────────────────────────────────────────

static URL_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"https?://[^\s]+").expect("URL regex compiles"));
static EMAIL_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}").expect("email regex")
});
static PHONE_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\+?\d[\d\s().-]{6,}\d").expect("phone regex"));
static MENTION_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"@\w{3,}").expect("mention regex"));

pub(crate) fn sanitize(input: &str) -> String {
    // Order matters: URL first because it can contain @ and digits we don't
    // want phone/mention patterns chewing up.
    let s = URL_RE.replace_all(input, "[link]");
    let s = EMAIL_RE.replace_all(&s, "[email]");
    let s = PHONE_RE.replace_all(&s, "[phone]");
    let s = MENTION_RE.replace_all(&s, "[user]");
    s.into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_strips_urls() {
        let s = sanitize("see https://example.com/path?q=1 for more");
        assert!(!s.contains("https"));
        assert!(s.contains("[link]"));
    }

    #[test]
    fn sanitize_strips_emails() {
        assert!(!sanitize("ping me at foo@bar.com").contains("foo@bar.com"));
        assert!(sanitize("ping foo@bar.com").contains("[email]"));
    }

    #[test]
    fn sanitize_strips_phone_numbers() {
        assert!(sanitize("call +1 (555) 123-4567 now").contains("[phone]"));
        assert!(sanitize("8 800 555 35 35").contains("[phone]"));
    }

    #[test]
    fn sanitize_strips_mentions() {
        assert!(sanitize("hello @plugfox how are you").contains("[user]"));
        assert!(!sanitize("hello @plugfox").contains("@plugfox"));
    }

    #[test]
    fn sanitize_keeps_plain_text() {
        assert_eq!(sanitize("just a normal message"), "just a normal message");
    }

    #[test]
    fn build_prompt_lists_bullets() {
        let p = build_user_prompt(&["a".into(), "b".into()]);
        assert_eq!(p, "- a\n- b\n");
    }

    #[test]
    fn system_prompt_picks_language() {
        assert!(system_prompt("ru").contains("Russian"));
        assert!(system_prompt("en").contains("English"));
        assert!(system_prompt("fr").contains("Russian"));
    }
}
