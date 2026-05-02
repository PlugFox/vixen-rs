//! Shared helpers for handler-level integration tests built on
//! `teloxide_tests::MockBot`. Each test file does:
//!
//! ```ignore
//! mod common;
//! use common::*;
//! ```
//!
//! All helpers are `#[ignore]`-friendly — they need a live Postgres + Redis,
//! flushed per-test (Postgres via `#[sqlx::test]`, Redis via per-file DB index
//! + `FLUSHDB`).

#![allow(dead_code)]

use std::sync::Arc;

use sqlx::PgPool;
use teloxide::Bot;

use vixen_server::api::AppState;
use vixen_server::config::Config;
use vixen_server::database::{Database, Redis};
use vixen_server::services::captcha::{CaptchaService, CaptchaState, Fonts};
use vixen_server::services::cas_client::CasClient;
use vixen_server::services::moderation_service::ModerationService;
use vixen_server::services::spam::service::SpamService;

/// Default supergroup ID used across handler tests.
pub const CHAT_ID: i64 = -1001234567890;
/// Default Telegram user ID for the actor under test.
pub const USER_ID: u64 = 4242;

/// Build a `Config` from clap with the four required flags filled. Used to
/// satisfy `AppState`'s `Arc<Config>` field — handler tests don't read any
/// config value, but the field must exist.
pub fn test_config() -> Config {
    use clap::Parser;
    Config::try_parse_from([
        "vixen-server",
        "--bot-token=1234567890:QWERTYUIOPASDFGHJKLZXCVBNMQWERTYUIO",
        "--database-url=postgresql://x:x@localhost/x",
        "--redis-url=redis://localhost:6379",
        "--chats=-1001234567890",
    ])
    .expect("parse test config")
}

/// Connect to Redis on the given URL. **Does NOT FLUSHDB** — flushing is
/// racy when multiple in-file tests run concurrently against the same DB.
/// Tests should use [`unique_chat_id`] so their per-chat keys
/// (`cap:admins:{chat_id}`, `cap:meta:{chat_id}:{message_id}`, etc.) live in
/// disjoint slots.
pub async fn fresh_redis(url: &str) -> Arc<Redis> {
    let r = Redis::connect(url).await.expect("redis connect");
    Arc::new(r)
}

/// Returns a unique negative chat_id for every call within the test process.
/// Use this in handler tests instead of a hard-coded constant so that Redis
/// keys (which are namespaced by chat_id) don't collide between concurrent
/// `#[sqlx::test]` runs in the same file.
pub fn unique_chat_id() -> i64 {
    use std::sync::atomic::{AtomicI64, Ordering};
    static N: AtomicI64 = AtomicI64::new(-1_001_000_000_000);
    N.fetch_add(1, Ordering::Relaxed)
}

/// Seed `chats` + `chat_config` for `chat_id`. CAS is force-disabled so the
/// spam pipeline never tries to reach an external service in handler tests.
pub async fn seed_chat(pool: &PgPool, chat_id: i64) {
    sqlx::query("INSERT INTO chats (chat_id) VALUES ($1) ON CONFLICT DO NOTHING")
        .bind(chat_id)
        .execute(pool)
        .await
        .expect("seed chats");
    sqlx::query(
        "INSERT INTO chat_config (chat_id, cas_enabled) VALUES ($1, FALSE)
         ON CONFLICT (chat_id) DO UPDATE SET cas_enabled = FALSE",
    )
    .bind(chat_id)
    .execute(pool)
    .await
    .expect("seed chat_config");
}

/// Mark `(chat_id, user_id)` as verified via the same write the captcha
/// service would do. Used by tests that need to exercise the verified-user
/// branch of `message_gate::handle` without going through the captcha solve
/// pipeline.
pub async fn seed_verified(pool: &PgPool, chat_id: i64, user_id: i64) {
    sqlx::query(
        "INSERT INTO verified_users (chat_id, user_id) VALUES ($1, $2)
         ON CONFLICT (chat_id, user_id) DO NOTHING",
    )
    .bind(chat_id)
    .bind(user_id)
    .execute(pool)
    .await
    .expect("seed verified_users");
}

/// Insert a row into `chat_moderators` so `/ban` and `/unban` permission
/// checks pass via the DB allow-list (skipping the `getChatAdministrators`
/// fallback, which `teloxide_tests` 0.2 does not mock).
pub async fn seed_moderator(pool: &PgPool, chat_id: i64, user_id: i64) {
    sqlx::query(
        "INSERT INTO chat_moderators (chat_id, user_id, granted_by) VALUES ($1, $2, $2)
         ON CONFLICT (chat_id, user_id) DO NOTHING",
    )
    .bind(chat_id)
    .bind(user_id)
    .execute(pool)
    .await
    .expect("seed chat_moderators");
}

/// Assemble a full `AppState` around the MockBot's `Bot`. The tests pass this
/// in as a dptree dep so the real handler endpoints can run unchanged.
pub async fn make_state(pool: PgPool, redis: Arc<Redis>, bot: Bot) -> AppState {
    let fonts = Fonts::load().expect("load fonts");
    let captcha = Arc::new(CaptchaService::new(pool.clone(), fonts));
    let captcha_state = Arc::new(CaptchaState::new(redis.clone()));
    // CAS base_url unused once `chat_config.cas_enabled = FALSE` (which
    // `seed_chat` forces) — the spam pipeline short-circuits before the HTTP
    // call. Any string accepted here.
    let cas = CasClient::new(redis.clone(), "http://localhost:0".to_string());
    let spam = Arc::new(SpamService::new(pool.clone(), cas));
    let moderation = ModerationService::new(pool.clone(), bot);

    AppState {
        config: Arc::new(test_config()),
        db: Arc::new(Database::from_pool(pool)),
        redis,
        captcha,
        captcha_state,
        spam,
        moderation,
    }
}
