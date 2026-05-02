//! `SpamService::inspect(message)` cascade — the M2 entry point.
//!
//! The cascade follows `server/docs/spam-detection.md`:
//!
//! 1. Skip non-text and short (`< 48` char normalized) messages.
//! 2. Per-chat config lookup (`spam_enabled`, `spam_threshold`,
//!    `spam_weights`, `cas_enabled`).
//! 3. Normalize → xxh3-64 → `spam_messages` lookup.
//!    - Hit: `Verdict::Ban` + bump hit_count.
//! 4. CAS lookup (when `cas_enabled`). Flagged → `Verdict::Ban` + record
//!    the message so future copies dedup.
//! 5. n-gram score = Σ phrase_weight. `score >= threshold` →
//!    `Verdict::Delete` + record. Otherwise `Verdict::Allow`.
//!
//! `inspect()` does not invoke moderation_service — the caller (handler)
//! routes the verdict through `ModerationService::apply` so the ledger
//! write and the bot side-effect stay in one place.

use anyhow::{Context, Result};
use chrono::{Duration, Utc};
use serde_json::json;
use sqlx::PgPool;
use teloxide::types::Message;
use tracing::{debug, instrument};
use xxhash_rust::xxh3::xxh3_64;

use crate::services::cas_client::{CasClient, Verdict as CasVerdict};
use crate::services::spam::dedup::{self, DedupOutcome};
use crate::services::spam::normalize;
use crate::services::spam::phrases::{PHRASES, SpamWeights};

/// Min normalized length before dedup/CAS/n-gram apply. Below this we Allow
/// — short text aliases too easily and bans become indiscriminate.
pub const MIN_NORMALIZED_LEN: usize = 48;

/// Default ban duration when the dedup branch fires. Short bans match the
/// Dart prototype and let moderators recover from false-positives by simply
/// waiting (or via `/unban`).
const DEDUP_BAN_DURATION_MIN: i64 = 10;

#[derive(Debug, Clone, PartialEq)]
pub enum Verdict {
    Allow,
    Delete {
        reason_json: serde_json::Value,
    },
    Ban {
        reason_json: serde_json::Value,
        /// `None` = permanent. Telegram interprets a past timestamp as
        /// permanent too, but we use `None` so the ban payload omits the
        /// field entirely.
        until: Option<chrono::DateTime<Utc>>,
    },
}

impl Verdict {
    pub fn is_action(&self) -> bool {
        !matches!(self, Verdict::Allow)
    }
}

#[derive(Clone)]
pub struct SpamService {
    db: PgPool,
    cas: CasClient,
}

impl SpamService {
    pub fn new(db: PgPool, cas: CasClient) -> Self {
        Self { db, cas }
    }

    #[instrument(
        skip_all,
        fields(
            chat_id = msg.chat.id.0,
            user_id = msg.from.as_ref().map(|u| u.id.0),
            message_id = msg.id.0,
        )
    )]
    pub async fn inspect(&self, msg: &Message) -> Result<Verdict> {
        let Some(text) = msg.text() else {
            return Ok(Verdict::Allow);
        };
        let Some(user) = msg.from.as_ref() else {
            return Ok(Verdict::Allow);
        };

        let chat_id = msg.chat.id.0;
        let cfg = match self.fetch_config(chat_id).await? {
            Some(c) if c.spam_enabled => c,
            _ => return Ok(Verdict::Allow),
        };

        let normalized = normalize::normalize(text);
        // Char count, not byte length — Cyrillic et al. are 2+ bytes per
        // codepoint in UTF-8, and a byte-length cutoff would short-circuit
        // around 24 chars for Russian, well below the 48-char threshold the
        // pipeline is documented to use.
        let normalized_chars = normalized.chars().count();
        if normalized_chars < MIN_NORMALIZED_LEN {
            debug!(chars = normalized_chars, "skipping (short)");
            return Ok(Verdict::Allow);
        }

        // xxh3_64 returns u64; cast to i64 for the BIGINT column. The bit
        // pattern round-trips losslessly — what matters is that the same
        // input always maps to the same DB key.
        let hash = xxh3_64(normalized.as_bytes()) as i64;

        // Step 1 — spam_messages dedup.
        if let DedupOutcome::Hit { hit_count } = dedup::lookup(&self.db, hash).await? {
            dedup::bump(&self.db, chat_id, hash).await?;
            return Ok(Verdict::Ban {
                reason_json: json!({
                    "matched_rules": ["xxh3_dedup"],
                    "hash": hash,
                    "hit_count": hit_count + 1,
                }),
                until: Some(Utc::now() + Duration::minutes(DEDUP_BAN_DURATION_MIN)),
            });
        }

        // Step 2 — CAS lookup.
        if cfg.cas_enabled {
            let user_id = user.id.0 as i64;
            if self.cas.lookup(user_id).await == CasVerdict::Flagged {
                dedup::record(&self.db, chat_id, hash, &normalized).await?;
                return Ok(Verdict::Ban {
                    reason_json: json!({
                        "matched_rules": ["cas"],
                        "user_id": user_id,
                    }),
                    until: None,
                });
            }
        }

        // Step 3 — n-gram phrase match (weighted).
        let weights = SpamWeights::from_json(&cfg.spam_weights);
        let (score, matched) = PHRASES.score(&normalized, &weights);
        if score >= cfg.spam_threshold && !matched.is_empty() {
            dedup::record(&self.db, chat_id, hash, &normalized).await?;
            return Ok(Verdict::Delete {
                reason_json: json!({
                    "matched_rules": ["ngram"],
                    "ngram_phrases": matched,
                    "score": score,
                    "threshold": cfg.spam_threshold,
                }),
            });
        }

        Ok(Verdict::Allow)
    }

    async fn fetch_config(&self, chat_id: i64) -> Result<Option<ChatSpamConfig>> {
        let row = sqlx::query_as!(
            ChatSpamConfig,
            r#"
            SELECT
                spam_enabled    AS "spam_enabled!",
                spam_threshold  AS "spam_threshold!",
                spam_weights    AS "spam_weights!: serde_json::Value",
                cas_enabled     AS "cas_enabled!"
            FROM chat_config
            WHERE chat_id = $1
            "#,
            chat_id,
        )
        .fetch_optional(&self.db)
        .await
        .context("SELECT chat_config")?;
        Ok(row)
    }
}

struct ChatSpamConfig {
    spam_enabled: bool,
    spam_threshold: f32,
    spam_weights: serde_json::Value,
    cas_enabled: bool,
}
