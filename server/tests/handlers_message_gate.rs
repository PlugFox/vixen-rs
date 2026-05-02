//! Handler-level tests for the M1 message gate (with the M2 spam pipeline
//! glued in for verified users).
//!
//! Three behaviour cases:
//!
//! 1. Unverified non-admin posts text → message deleted + captcha photo
//!    issued.
//! 2. Unverified non-admin already has a live captcha → message deleted, NO
//!    second photo.
//! 3. Verified non-admin posts an n-gram phrase → spam pipeline returns
//!    `Delete` → moderation ledger row + `delete_message` API call.
//!
//! Out of scope: chat-admin path, since `getChatAdministrators` isn't mocked
//! by `teloxide_tests` 0.2 — tests must seed Redis admin cache to take that
//! branch, which is exercised in `handlers_ban.rs`.
//!
//! `#[ignore]`-gated: requires Postgres + Redis on `localhost`.

mod common;

use std::sync::Arc;

use common::*;
use sqlx::PgPool;
use teloxide::dispatching::UpdateHandler;
use teloxide::dptree;
use teloxide::prelude::*;
use teloxide_tests::{MockBot, MockMessageText, MockSupergroupChat, MockUser};
use vixen_server::api::AppState;
use vixen_server::models::daily_stats::{self, Metric};
use vixen_server::telegram::handlers::message_gate;

const REDIS_URL: &str = "redis://localhost:6379/11";

fn handler() -> UpdateHandler<Box<dyn std::error::Error + Send + Sync + 'static>> {
    Update::filter_message().endpoint(|bot: Bot, msg: Message, state: AppState| async move {
        message_gate::handle(bot, msg, state)
            .await
            .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.into() })
    })
}

fn text_message(chat_id: i64, sender_id: u64, text: &str) -> MockMessageText {
    MockMessageText::new()
        .text(text)
        .chat(MockSupergroupChat::new().id(chat_id).build())
        .from(MockUser::new().id(sender_id).build())
}

// ── tests ────────────────────────────────────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres + redis"]
async fn unverified_user_message_deleted_and_captcha_issued(pool: PgPool) {
    let chat_id = unique_chat_id();
    seed_chat(&pool, chat_id).await;
    let redis = fresh_redis(REDIS_URL).await;

    const POSTER: u64 = 9001;
    let msg = text_message(chat_id, POSTER, "hello chat");

    let mock = MockBot::new(msg, handler());
    let state = make_state(pool.clone(), Arc::clone(&redis), mock.bot.clone()).await;
    mock.dependencies(dptree::deps![state]);
    mock.dispatch().await;

    let r = mock.get_responses();
    assert!(
        !r.deleted_messages.is_empty(),
        "unverified poster's message must be deleted"
    );

    // PG side is the load-bearing assertion: `issue_challenge` ran inside the
    // handler and persisted a row before `send_photo` was attempted. We do
    // NOT assert on `r.sent_messages_photo` because `teloxide_tests` 0.2's
    // multipart parser tries to UTF-8-decode the binary WebP and panics on
    // its actix worker, so the photo never reaches the recorded responses.
    // The challenge row proves the gate took the captcha-issuing branch.
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM captcha_challenges WHERE chat_id = $1 AND user_id = $2",
    )
    .bind(chat_id)
    .bind(POSTER as i64)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 1, "exactly one challenge row");
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres + redis"]
async fn unverified_user_with_live_challenge_skips_reissue(pool: PgPool) {
    let chat_id = unique_chat_id();
    seed_chat(&pool, chat_id).await;
    let redis = fresh_redis(REDIS_URL).await;

    const POSTER: u64 = 9002;

    // Pre-seed a live challenge for this user via the captcha service so
    // `active_challenge_message_id` returns Some(...) in the handler.
    let fonts = vixen_server::services::captcha::Fonts::load().expect("fonts");
    let svc = vixen_server::services::captcha::CaptchaService::new(pool.clone(), fonts);
    let issued = svc
        .issue_challenge(chat_id, POSTER as i64)
        .await
        .expect("issue challenge");
    svc.record_message_id(chat_id, POSTER as i64, 999_888)
        .await
        .expect("anchor message_id");
    drop(issued);

    let msg = text_message(chat_id, POSTER, "second message before solving");
    let mock = MockBot::new(msg, handler());
    let state = make_state(pool.clone(), Arc::clone(&redis), mock.bot.clone()).await;
    mock.dependencies(dptree::deps![state]);
    mock.dispatch().await;

    let r = mock.get_responses();
    assert!(
        !r.deleted_messages.is_empty(),
        "second message still gets deleted"
    );
    assert!(
        r.sent_messages_photo.is_empty(),
        "must NOT send a second captcha photo while one is live"
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres + redis"]
async fn verified_user_phrase_match_deletes_via_moderation_ledger(pool: PgPool) {
    let chat_id = unique_chat_id();
    seed_chat(&pool, chat_id).await;
    let redis = fresh_redis(REDIS_URL).await;

    const POSTER: u64 = 9003;
    seed_verified(&pool, chat_id, POSTER as i64).await;

    // A long n-gram phrase from the corpus that triggers Delete (not Ban —
    // first time we see it, no dedup hit). 48-char minimum is enforced by
    // SpamService::inspect, so use a substantial sample.
    let body = "Заработок в интернете на дому без вложений, пишите в лс для подробностей и условий";
    let msg = text_message(chat_id, POSTER, body);

    let mock = MockBot::new(msg, handler());
    let state = make_state(pool.clone(), Arc::clone(&redis), mock.bot.clone()).await;
    mock.dependencies(dptree::deps![state]);
    mock.dispatch().await;

    let r = mock.get_responses();
    assert_eq!(
        r.deleted_messages.len(),
        1,
        "exactly one delete from the spam pipeline"
    );
    assert!(
        r.sent_messages_photo.is_empty(),
        "verified user does NOT get a captcha"
    );

    // Ledger row from the moderation service.
    let action: Option<String> = sqlx::query_scalar(
        "SELECT action FROM moderation_actions
         WHERE chat_id = $1 AND target_user_id = $2",
    )
    .bind(chat_id)
    .bind(POSTER as i64)
    .fetch_optional(&pool)
    .await
    .unwrap();
    assert_eq!(
        action.as_deref(),
        Some("delete"),
        "expected 'delete' ledger"
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres + redis"]
async fn message_gate_bumps_messages_seen(pool: PgPool) {
    // Every observable message bumps `daily_stats(messages_seen)`. Verified
    // for the verified-user fast-path; the unverified branch is asserted in
    // `bumps_messages_seen_for_unverified` below.
    let chat_id = unique_chat_id();
    seed_chat(&pool, chat_id).await;
    let redis = fresh_redis(REDIS_URL).await;

    const POSTER: u64 = 9101;
    seed_verified(&pool, chat_id, POSTER as i64).await;

    let msg = text_message(chat_id, POSTER, "ping");
    let mock = MockBot::new(msg, handler());
    let state = make_state(pool.clone(), Arc::clone(&redis), mock.bot.clone()).await;
    mock.dependencies(dptree::deps![state]);
    mock.dispatch().await;

    let v = daily_stats::get(
        &pool,
        chat_id,
        chrono::Utc::now().date_naive(),
        Metric::MessagesSeen,
    )
    .await
    .unwrap();
    assert_eq!(v, 1);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres + redis"]
async fn message_gate_bumps_messages_seen_for_unverified(pool: PgPool) {
    // Even a captcha-deleted message increments messages_seen — it's the
    // "noise the bot saw" baseline that `report_min_activity` measures.
    let chat_id = unique_chat_id();
    seed_chat(&pool, chat_id).await;
    let redis = fresh_redis(REDIS_URL).await;

    const POSTER: u64 = 9102;
    let msg = text_message(chat_id, POSTER, "first message ever");
    let mock = MockBot::new(msg, handler());
    let state = make_state(pool.clone(), Arc::clone(&redis), mock.bot.clone()).await;
    mock.dependencies(dptree::deps![state]);
    mock.dispatch().await;

    let v = daily_stats::get(
        &pool,
        chat_id,
        chrono::Utc::now().date_naive(),
        Metric::MessagesSeen,
    )
    .await
    .unwrap();
    assert_eq!(v, 1);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres + redis"]
async fn allowed_messages_logging_respects_chat_config_off(pool: PgPool) {
    // Default is `log_allowed_messages = FALSE` — the spam-pipeline Allow
    // path must NOT write to allowed_messages until a moderator opts in.
    let chat_id = unique_chat_id();
    seed_chat(&pool, chat_id).await;
    let redis = fresh_redis(REDIS_URL).await;

    const POSTER: u64 = 9103;
    seed_verified(&pool, chat_id, POSTER as i64).await;

    let msg = text_message(
        chat_id,
        POSTER,
        "totally innocent chat message that's long enough",
    );
    let mock = MockBot::new(msg, handler());
    let state = make_state(pool.clone(), Arc::clone(&redis), mock.bot.clone()).await;
    mock.dependencies(dptree::deps![state]);
    mock.dispatch().await;

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM allowed_messages WHERE chat_id = $1")
        .bind(chat_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0, "logging off → no allowed_messages rows");
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres + redis"]
async fn allowed_messages_logging_writes_when_opted_in(pool: PgPool) {
    let chat_id = unique_chat_id();
    seed_chat(&pool, chat_id).await;
    sqlx::query("UPDATE chat_config SET log_allowed_messages = TRUE WHERE chat_id = $1")
        .bind(chat_id)
        .execute(&pool)
        .await
        .unwrap();
    let redis = fresh_redis(REDIS_URL).await;

    const POSTER: u64 = 9104;
    seed_verified(&pool, chat_id, POSTER as i64).await;

    let body = "totally innocent chat message that's long enough to satisfy spam pipeline";
    let msg = text_message(chat_id, POSTER, body);
    let mock = MockBot::new(msg, handler());
    let state = make_state(pool.clone(), Arc::clone(&redis), mock.bot.clone()).await;
    mock.dependencies(dptree::deps![state]);
    mock.dispatch().await;

    let stored: Option<String> =
        sqlx::query_scalar("SELECT content FROM allowed_messages WHERE chat_id = $1")
            .bind(chat_id)
            .fetch_optional(&pool)
            .await
            .unwrap();
    assert_eq!(stored.as_deref(), Some(body));
}
