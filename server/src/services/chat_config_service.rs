//! Per-chat config service. Single source of truth for `chat_config` reads:
//! every site that needs a chat's tunables goes through [`ChatConfigService::get`]
//! so the in-memory Moka cache stays in sync with the DB.
//!
//! Hot reload contract: a successful [`ChatConfigService::update`] commits to
//! Postgres and publishes `invalidate` on the `chat_config:{chat_id}` Redis
//! channel. The subscribe loop in `bin/server.rs` parses the chat_id out of
//! the channel name and calls [`ChatConfigService::invalidate`], which evicts
//! the local Moka entry — next [`get`] will re-fetch from Postgres. Across
//! replicas, every subscriber drops its entry independently, so the new
//! value propagates without a redeploy.
//!
//! The Moka entry has a 1h TTL as a safety net (recovers from a missed
//! pub/sub message, e.g. during a brief Redis restart). Pub/sub is the
//! primary invalidation path.

use std::sync::Arc;
use std::time::Duration;

use moka::future::Cache;
use tracing::{instrument, warn};

use crate::database::{Database, Redis};
use crate::models::{ChatConfig, ChatConfigPatch, PatchValidationError};

const MOKA_TTL: Duration = Duration::from_secs(60 * 60);
const MOKA_CAPACITY: u64 = 1_000;

/// Channel name for hot-reload notifications.
pub fn channel_for(chat_id: i64) -> String {
    format!("chat_config:{chat_id}")
}

/// Parse the chat_id from a `chat_config:{chat_id}` channel name. Returns
/// `None` when the channel doesn't match the expected prefix (defensive
/// guard for the PSUBSCRIBE wildcard pattern).
pub fn chat_id_from_channel(channel: &str) -> Option<i64> {
    channel.strip_prefix("chat_config:")?.parse().ok()
}

#[derive(Clone)]
pub struct ChatConfigService {
    db: Arc<Database>,
    redis: Arc<Redis>,
    cache: Cache<i64, Arc<ChatConfig>>,
}

impl ChatConfigService {
    pub fn new(db: Arc<Database>, redis: Arc<Redis>) -> Self {
        let cache = Cache::builder()
            .max_capacity(MOKA_CAPACITY)
            .time_to_live(MOKA_TTL)
            .build();
        Self { db, redis, cache }
    }

    /// Returns the current config. Moka first, Postgres on miss.
    #[instrument(skip(self), fields(chat_id))]
    pub async fn get(&self, chat_id: i64) -> Result<Arc<ChatConfig>, ChatConfigError> {
        if let Some(cached) = self.cache.get(&chat_id).await {
            return Ok(cached);
        }
        let row = self.fetch(chat_id).await?;
        let arc = Arc::new(row);
        self.cache.insert(chat_id, arc.clone()).await;
        Ok(arc)
    }

    /// Apply a partial patch inside a transaction (`SELECT … FOR UPDATE`
    /// serialises concurrent writers), publish `invalidate` on the pub/sub
    /// channel for cross-replica hot-reload, refresh the local Moka entry,
    /// and return the updated row.
    ///
    /// Returns [`ChatConfigError::EmptyPatch`] when the caller didn't actually
    /// pass anything — skips the DB write and the pub/sub fan-out.
    #[instrument(skip(self, patch), fields(chat_id))]
    pub async fn update(
        &self,
        chat_id: i64,
        patch: ChatConfigPatch,
    ) -> Result<Arc<ChatConfig>, ChatConfigError> {
        if patch.is_empty() {
            return Err(ChatConfigError::EmptyPatch);
        }
        patch.validate()?;

        let pool = self.db.pool();
        let mut tx = pool.begin().await?;

        let lock_row: Option<i64> = sqlx::query_scalar!(
            r#"SELECT chat_id FROM chat_config WHERE chat_id = $1 FOR UPDATE"#,
            chat_id
        )
        .fetch_optional(&mut *tx)
        .await?;
        if lock_row.is_none() {
            return Err(ChatConfigError::NotFound(chat_id));
        }

        // openai_api_key three-state encoding:
        //   patch.openai_api_key == None              -> leave the column alone
        //   patch.openai_api_key == Some(None)        -> SET column = NULL
        //   patch.openai_api_key == Some(Some(value)) -> SET column = value
        let openai_key_present = patch.openai_api_key.is_some();
        let openai_key_value = patch.openai_api_key.clone().flatten();

        let updated = sqlx::query_as!(
            ChatConfig,
            r#"
            UPDATE chat_config SET
                captcha_enabled       = COALESCE($2, captcha_enabled),
                captcha_lifetime_secs = COALESCE($3, captcha_lifetime_secs),
                captcha_attempts      = COALESCE($4, captcha_attempts),
                spam_enabled          = COALESCE($5, spam_enabled),
                spam_threshold        = COALESCE($6, spam_threshold),
                spam_weights          = COALESCE($7, spam_weights),
                cas_enabled           = COALESCE($8, cas_enabled),
                clown_chance          = COALESCE($9, clown_chance),
                log_allowed_messages  = COALESCE($10, log_allowed_messages),
                report_hour           = COALESCE($11, report_hour),
                timezone              = COALESCE($12, timezone),
                summary_enabled       = COALESCE($13, summary_enabled),
                summary_token_budget  = COALESCE($14, summary_token_budget),
                report_min_activity   = COALESCE($15, report_min_activity),
                openai_api_key        = CASE WHEN $16::bool THEN $17::text ELSE openai_api_key END,
                openai_model          = COALESCE($18, openai_model),
                language              = COALESCE($19, language)
            WHERE chat_id = $1
            RETURNING
                chat_id,
                captcha_enabled,
                captcha_lifetime_secs,
                captcha_attempts,
                spam_enabled,
                spam_threshold,
                spam_weights,
                cas_enabled,
                clown_chance,
                log_allowed_messages,
                report_hour,
                timezone,
                summary_enabled,
                summary_token_budget,
                report_min_activity,
                openai_api_key,
                openai_model,
                language,
                created_at,
                updated_at
            "#,
            chat_id,
            patch.captcha_enabled,
            patch.captcha_lifetime_secs,
            patch.captcha_attempts,
            patch.spam_enabled,
            patch.spam_threshold,
            patch.spam_weights,
            patch.cas_enabled,
            patch.clown_chance,
            patch.log_allowed_messages,
            patch.report_hour,
            patch.timezone,
            patch.summary_enabled,
            patch.summary_token_budget,
            patch.report_min_activity,
            openai_key_present,
            openai_key_value,
            patch.openai_model,
            patch.language,
        )
        .fetch_one(&mut *tx)
        .await?;

        tx.commit().await?;

        // Publish AFTER commit so other replicas re-read the new row, not the
        // pre-commit one. Failure here only delays propagation until the TTL
        // expires — it does NOT make the write invalid.
        let channel = channel_for(chat_id);
        if let Err(e) = self.redis.publish(&channel, "invalidate").await {
            warn!(
                error = %e,
                chat_id,
                channel,
                "chat_config: pub/sub publish failed; relying on local TTL"
            );
        }

        // Invalidate (not insert) the local entry so the next `get` re-reads
        // from Postgres. A post-commit `cache.insert(updated)` is unsafe
        // under interleaved commits: tx A and tx B both running this method
        // can commit in order A → B, but then race on `cache.insert`. If A's
        // insert lands after B's, the cache regresses to A's stale row until
        // the TTL or a fresh invalidation kicks in. Re-reading from PG on the
        // next get costs one extra SELECT but is always coherent.
        self.cache.invalidate(&chat_id).await;
        Ok(Arc::new(updated))
    }

    /// Evict the local cache entry. Idempotent. Called by the Redis subscribe
    /// loop in `bin/server.rs` on every `chat_config:{chat_id}` message.
    pub async fn invalidate(&self, chat_id: i64) {
        self.cache.invalidate(&chat_id).await;
    }

    async fn fetch(&self, chat_id: i64) -> Result<ChatConfig, ChatConfigError> {
        let pool = self.db.pool();
        match sqlx::query_as!(
            ChatConfig,
            r#"
            SELECT
                chat_id,
                captcha_enabled,
                captcha_lifetime_secs,
                captcha_attempts,
                spam_enabled,
                spam_threshold,
                spam_weights,
                cas_enabled,
                clown_chance,
                log_allowed_messages,
                report_hour,
                timezone,
                summary_enabled,
                summary_token_budget,
                report_min_activity,
                openai_api_key,
                openai_model,
                language,
                created_at,
                updated_at
            FROM chat_config WHERE chat_id = $1
            "#,
            chat_id
        )
        .fetch_one(pool)
        .await
        {
            Ok(row) => Ok(row),
            Err(sqlx::Error::RowNotFound) => Err(ChatConfigError::NotFound(chat_id)),
            Err(e) => Err(ChatConfigError::Db(e)),
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum ChatConfigError {
    #[error("no chat_config row for chat_id = {0}")]
    NotFound(i64),
    #[error("PATCH payload had no fields to update")]
    EmptyPatch,
    #[error("validation: {0}")]
    Validation(#[from] PatchValidationError),
    #[error("database: {0}")]
    Db(#[from] sqlx::Error),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_roundtrip() {
        assert_eq!(channel_for(-1001234), "chat_config:-1001234");
        assert_eq!(chat_id_from_channel("chat_config:-1001234"), Some(-1001234));
        assert_eq!(chat_id_from_channel("chat_config:42"), Some(42));
        assert_eq!(chat_id_from_channel("other:42"), None);
        assert_eq!(chat_id_from_channel("chat_config:not-a-number"), None);
        assert_eq!(chat_id_from_channel("chat_config:"), None);
    }
}
