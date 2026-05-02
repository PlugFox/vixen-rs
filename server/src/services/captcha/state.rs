//! Ephemeral captcha state in Redis.
//!
//! `CaptchaService` is Telegram-free / PG-only — it owns the durable challenge
//! row and the verified-user ledger. This module owns the *ephemeral* slices
//! that don't belong in PG: the partial digit input typed by the user between
//! callback presses (`cap:input:{chat}:{user}`), the per-message callback meta
//! used for an O(1) ownership check (`cap:meta:{chat}:{message}`), and a
//! short-lived `is_verified` cache (`cap:verified:{chat}:{user}`) that lets
//! the join hot path skip a PG round-trip for returning users.
//!
//! All keys live under the `cap:` namespace. Redis is the source of truth for
//! input and meta (lost on restart → user must press refresh). PG is the
//! source of truth for verification — the cache is a hint that gets
//! repopulated lazily on PG hit.
//!
//! Cache misses return empty/false, never errors: a flaky Redis must degrade
//! the captcha UI, never break it.

use std::sync::Arc;

use anyhow::{Context, Result};
use redis::AsyncCommands;

use crate::database::Redis;

/// 7 days. Verification is per-chat and effectively permanent in PG; the cache
/// just shaves the join-time lookup. A week balances "hot path stays warm for
/// returning users" against "purge on schema/policy changes within a week".
pub const VERIFIED_CACHE_TTL_SECS: u64 = 604_800;

#[derive(Clone)]
pub struct CaptchaState {
    redis: Arc<Redis>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetaPayload {
    pub owner_user_id: i64,
    pub uuid_short: String,
    pub lifetime_secs: u64,
}

impl MetaPayload {
    /// Pipe-delimited `{owner}|{short}|{lifetime}`. Compact, and Redis-key-safe
    /// (no JSON escaping headaches for an 8-hex `short` and ASCII numerics).
    pub(crate) fn to_redis_string(&self) -> String {
        format!(
            "{}|{}|{}",
            self.owner_user_id, self.uuid_short, self.lifetime_secs
        )
    }

    /// Strict three-field parse. Anything else → `None` and the caller treats
    /// it as a cache miss (silent — a bad value left over from a schema change
    /// shouldn't crash the callback handler).
    pub(crate) fn from_redis_string(s: &str) -> Option<Self> {
        let mut it = s.splitn(3, '|');
        let owner = it.next()?.parse::<i64>().ok()?;
        let short = it.next()?;
        let lifetime = it.next()?.parse::<u64>().ok()?;
        if it.next().is_some() {
            return None;
        }
        if short.len() != 8 || !short.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        Some(Self {
            owner_user_id: owner,
            uuid_short: short.to_owned(),
            lifetime_secs: lifetime,
        })
    }
}

impl CaptchaState {
    pub fn new(redis: Arc<Redis>) -> Self {
        Self { redis }
    }

    // ── Input buffer ──────────────────────────────────────────────────────

    pub async fn set_input(
        &self,
        chat_id: i64,
        user_id: i64,
        input: &str,
        ttl_secs: u64,
    ) -> Result<()> {
        let key = input_key(chat_id, user_id);
        let mut conn = self
            .redis
            .pool()
            .get()
            .await
            .context("redis pool acquire (set_input)")?;
        let _: () = conn
            .set_ex(&key, input, ttl_secs)
            .await
            .context("SETEX cap:input")?;
        Ok(())
    }

    pub async fn get_input(&self, chat_id: i64, user_id: i64) -> Result<String> {
        let key = input_key(chat_id, user_id);
        let mut conn = self
            .redis
            .pool()
            .get()
            .await
            .context("redis pool acquire (get_input)")?;
        let value: Option<String> = conn.get(&key).await.context("GET cap:input")?;
        Ok(value.unwrap_or_default())
    }

    pub async fn clear_input(&self, chat_id: i64, user_id: i64) -> Result<()> {
        let key = input_key(chat_id, user_id);
        let mut conn = self
            .redis
            .pool()
            .get()
            .await
            .context("redis pool acquire (clear_input)")?;
        let _: i64 = conn.del(&key).await.context("DEL cap:input")?;
        Ok(())
    }

    // ── Callback meta ─────────────────────────────────────────────────────

    pub async fn set_meta(
        &self,
        chat_id: i64,
        message_id: i32,
        owner_user_id: i64,
        uuid_short: &str,
        ttl_secs: u64,
    ) -> Result<()> {
        let key = meta_key(chat_id, message_id);
        let payload = MetaPayload {
            owner_user_id,
            uuid_short: uuid_short.to_owned(),
            lifetime_secs: ttl_secs,
        }
        .to_redis_string();
        let mut conn = self
            .redis
            .pool()
            .get()
            .await
            .context("redis pool acquire (set_meta)")?;
        let _: () = conn
            .set_ex(&key, payload, ttl_secs)
            .await
            .context("SETEX cap:meta")?;
        Ok(())
    }

    pub async fn get_meta(&self, chat_id: i64, message_id: i32) -> Result<Option<MetaPayload>> {
        let key = meta_key(chat_id, message_id);
        let mut conn = self
            .redis
            .pool()
            .get()
            .await
            .context("redis pool acquire (get_meta)")?;
        let raw: Option<String> = conn.get(&key).await.context("GET cap:meta")?;
        Ok(raw.and_then(|s| MetaPayload::from_redis_string(&s)))
    }

    pub async fn clear_meta(&self, chat_id: i64, message_id: i32) -> Result<()> {
        let key = meta_key(chat_id, message_id);
        let mut conn = self
            .redis
            .pool()
            .get()
            .await
            .context("redis pool acquire (clear_meta)")?;
        let _: i64 = conn.del(&key).await.context("DEL cap:meta")?;
        Ok(())
    }

    // ── Verified cache ────────────────────────────────────────────────────

    pub async fn mark_verified(&self, chat_id: i64, user_id: i64) -> Result<()> {
        let key = verified_key(chat_id, user_id);
        let mut conn = self
            .redis
            .pool()
            .get()
            .await
            .context("redis pool acquire (mark_verified)")?;
        let _: () = conn
            .set_ex(&key, "1", VERIFIED_CACHE_TTL_SECS)
            .await
            .context("SETEX cap:verified")?;
        Ok(())
    }

    pub async fn is_verified_cached(&self, chat_id: i64, user_id: i64) -> Result<bool> {
        let key = verified_key(chat_id, user_id);
        let mut conn = self
            .redis
            .pool()
            .get()
            .await
            .context("redis pool acquire (is_verified_cached)")?;
        let v: Option<String> = conn.get(&key).await.context("GET cap:verified")?;
        Ok(v.is_some())
    }
}

fn input_key(chat_id: i64, user_id: i64) -> String {
    format!("cap:input:{chat_id}:{user_id}")
}

fn meta_key(chat_id: i64, message_id: i32) -> String {
    format!("cap:meta:{chat_id}:{message_id}")
}

fn verified_key(chat_id: i64, user_id: i64) -> String {
    format!("cap:verified:{chat_id}:{user_id}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meta_payload_roundtrip() {
        let p = MetaPayload {
            owner_user_id: -100123456,
            uuid_short: "deadbeef".into(),
            lifetime_secs: 60,
        };
        let s = p.to_redis_string();
        let parsed = MetaPayload::from_redis_string(&s).expect("parse");
        assert_eq!(parsed.owner_user_id, p.owner_user_id);
        assert_eq!(parsed.uuid_short, p.uuid_short);
        assert_eq!(parsed.lifetime_secs, p.lifetime_secs);
    }

    #[test]
    fn meta_payload_rejects_malformed() {
        assert!(MetaPayload::from_redis_string("nope").is_none());
        assert!(MetaPayload::from_redis_string("123|short").is_none()); // wrong short len
        assert!(MetaPayload::from_redis_string("123|deadbeef").is_none()); // missing lifetime
        assert!(MetaPayload::from_redis_string("abc|deadbeef|60").is_none()); // owner not i64
        assert!(MetaPayload::from_redis_string("123|deadbeef|nope").is_none()); // lifetime not u64
        assert!(MetaPayload::from_redis_string("123|zzzzzzzz|60").is_none()); // short not hex
        assert!(MetaPayload::from_redis_string("123|deadbeef|60|extra").is_none()); // trailing
    }

    #[test]
    fn keys_use_canonical_decimal_for_negative_chat_ids() {
        assert_eq!(input_key(-1001234567890, 42), "cap:input:-1001234567890:42");
        assert_eq!(meta_key(-1001234567890, 7), "cap:meta:-1001234567890:7");
        assert_eq!(
            verified_key(-1001234567890, 42),
            "cap:verified:-1001234567890:42"
        );
    }
}
