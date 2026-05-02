//! CAS (Combot Anti-Spam) lookup client. Two-tier cache: Moka 1h front,
//! Redis 24h back, HTTP `{base_url}/check?user_id={id}` on miss.
//! Fail-open on any error — falsely flagging a real user is worse than
//! missing a spammer.
//!
//! See `server/docs/spam-detection.md` §"CAS integration".

use std::sync::Arc;
use std::time::Duration;

use moka::future::Cache;
use redis::AsyncCommands;
use tracing::{debug, instrument, warn};

use crate::database::Redis;

/// Production base URL for cas.chat.
pub const PRODUCTION_BASE_URL: &str = "https://api.cas.chat";

const HTTP_TIMEOUT: Duration = Duration::from_secs(3);
const MOKA_TTL: Duration = Duration::from_secs(60 * 60); // 1h
const REDIS_TTL_SECS: u64 = 24 * 60 * 60; // 24h
const MOKA_CAPACITY: u64 = 10_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Clean,
    Flagged,
}

impl Verdict {
    fn as_redis_str(self) -> &'static str {
        match self {
            Self::Clean => "clean",
            Self::Flagged => "flagged",
        }
    }

    fn parse_redis_str(s: &str) -> Option<Self> {
        match s {
            "clean" => Some(Self::Clean),
            "flagged" => Some(Self::Flagged),
            _ => None,
        }
    }
}

#[derive(Clone)]
pub struct CasClient {
    http: reqwest::Client,
    moka: Cache<i64, Verdict>,
    redis: Arc<Redis>,
    base_url: String,
}

impl CasClient {
    /// `base_url` should be `PRODUCTION_BASE_URL` in prod and the wiremock
    /// server URL in tests.
    pub fn new(redis: Arc<Redis>, base_url: impl Into<String>) -> Self {
        let http = reqwest::Client::builder()
            .timeout(HTTP_TIMEOUT)
            .build()
            .expect("reqwest::Client::build with timeout never fails");
        let moka = Cache::builder()
            .max_capacity(MOKA_CAPACITY)
            .time_to_live(MOKA_TTL)
            .build();
        Self {
            http,
            moka,
            redis,
            base_url: base_url.into(),
        }
    }

    /// Tier order: Moka → Redis → HTTP. On any failure (network, Redis,
    /// timeout) returns `Verdict::Clean` — falsely flagging is worse than
    /// missing a spammer.
    #[instrument(skip(self), fields(user_id))]
    pub async fn lookup(&self, user_id: i64) -> Verdict {
        if let Some(v) = self.moka.get(&user_id).await {
            debug!(verdict = ?v, "moka hit");
            return v;
        }
        if let Some(v) = self.redis_get(user_id).await {
            debug!(verdict = ?v, "redis hit");
            self.moka.insert(user_id, v).await;
            return v;
        }
        let verdict = self.http_check(user_id).await;
        self.write_through(user_id, verdict).await;
        verdict
    }

    async fn redis_get(&self, user_id: i64) -> Option<Verdict> {
        let mut conn = match self.redis.pool().get().await {
            Ok(c) => c,
            Err(e) => {
                warn!(error = %e, "redis pool acquire failed; skipping redis tier");
                return None;
            }
        };
        let key = redis_key(user_id);
        let raw: Option<String> = match conn.get(&key).await {
            Ok(v) => v,
            Err(e) => {
                warn!(error = %e, key, "redis GET failed; skipping redis tier");
                return None;
            }
        };
        raw.as_deref().and_then(Verdict::parse_redis_str)
    }

    async fn write_through(&self, user_id: i64, verdict: Verdict) {
        // Cache positive AND negative verdicts so we don't hammer CAS on
        // every clean message. Recovery from a misclassified ban happens via
        // moderator unban, which is a separate flow.
        self.moka.insert(user_id, verdict).await;
        let mut conn = match self.redis.pool().get().await {
            Ok(c) => c,
            Err(e) => {
                warn!(error = %e, "redis pool acquire failed; skipping redis write");
                return;
            }
        };
        let key = redis_key(user_id);
        if let Err(e) = conn
            .set_ex::<_, _, ()>(&key, verdict.as_redis_str(), REDIS_TTL_SECS)
            .await
        {
            warn!(error = %e, key, "redis SET EX failed");
        }
    }

    async fn http_check(&self, user_id: i64) -> Verdict {
        let url = format!("{}/check?user_id={}", self.base_url, user_id);
        let resp = match self.http.get(&url).send().await {
            Ok(r) => r,
            Err(e) => {
                warn!(error = %e, "CAS HTTP failed; fail-open Clean");
                return Verdict::Clean;
            }
        };
        if !resp.status().is_success() {
            warn!(status = %resp.status(), "CAS non-2xx; fail-open Clean");
            return Verdict::Clean;
        }
        let body: CasResponse = match resp.json().await {
            Ok(b) => b,
            Err(e) => {
                warn!(error = %e, "CAS body parse failed; fail-open Clean");
                return Verdict::Clean;
            }
        };
        if body.ok {
            Verdict::Flagged
        } else {
            Verdict::Clean
        }
    }
}

fn redis_key(user_id: i64) -> String {
    format!("cas:{user_id}")
}

#[derive(serde::Deserialize)]
struct CasResponse {
    ok: bool,
    // The full response includes `result: { user_id, offenses, messages }` for
    // flagged users and `description: String` for clean. We only need `ok`.
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redis_str_round_trip() {
        for v in [Verdict::Clean, Verdict::Flagged] {
            assert_eq!(Verdict::parse_redis_str(v.as_redis_str()), Some(v));
        }
        assert_eq!(Verdict::parse_redis_str("garbage"), None);
    }

    #[test]
    fn redis_key_format() {
        assert_eq!(redis_key(42), "cas:42");
        assert_eq!(redis_key(-1), "cas:-1");
    }
}
