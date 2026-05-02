//! Handler-level tests for `/stats`, `/report` and `/summary`.
//!
//! Wires the real `commands::dispatch` endpoint behind `filter_command::<Command>`,
//! drives it with synthetic message updates via `teloxide_tests::MockBot`. The
//! M2 handler-test caveat applies: `getChatAdministrators` is not mocked by
//! `teloxide_tests` 0.2, so tests pre-seed `chat_moderators` to take the DB
//! allow-list path.
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
use vixen_server::telegram::commands::Command;
use vixen_server::telegram::handlers::commands as command_handler;

const REDIS_URL: &str = "redis://localhost:6379/13";
const MODERATOR_ID: u64 = 7777;
const NON_MODERATOR_ID: u64 = 9999;

fn handler() -> UpdateHandler<Box<dyn std::error::Error + Send + Sync + 'static>> {
    Update::filter_message().branch(dptree::entry().filter_command::<Command>().endpoint(
        |bot: Bot, msg: Message, state: AppState, cmd: Command| async move {
            command_handler::dispatch(bot, msg, state, cmd)
                .await
                .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.into() })
        },
    ))
}

fn cmd_message(chat_id: i64, sender_id: u64, text: &str) -> MockMessageText {
    MockMessageText::new()
        .text(text)
        .chat(MockSupergroupChat::new().id(chat_id).build())
        .from(MockUser::new().id(sender_id).build())
}

/// Pre-seed today's `messages_seen` so the in-chat report has at least one
/// non-zero counter to render.
async fn seed_messages_seen(pool: &PgPool, chat_id: i64, value: i64) {
    sqlx::query(
        "INSERT INTO daily_stats (chat_id, date, kind, value)
         VALUES ($1, CURRENT_DATE, 'messages_seen', $2)
         ON CONFLICT (chat_id, date, kind) DO UPDATE SET value = EXCLUDED.value",
    )
    .bind(chat_id)
    .bind(value)
    .execute(pool)
    .await
    .unwrap();
}

async fn flush_cooldown(redis_url: &str, chat_id: i64) {
    let client = redis::Client::open(redis_url).expect("client");
    let mut conn = client
        .get_multiplexed_async_connection()
        .await
        .expect("conn");
    let _: () = redis::cmd("DEL")
        .arg(format!("cmd:stats:{chat_id}"))
        .arg(format!("cmd:summary:{chat_id}"))
        .query_async(&mut conn)
        .await
        .unwrap_or(());
}

// ── /stats ──────────────────────────────────────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres + redis"]
async fn stats_replies_with_24h_summary(pool: PgPool) {
    let chat_id = unique_chat_id();
    seed_chat(&pool, chat_id).await;
    seed_moderator(&pool, chat_id, MODERATOR_ID as i64).await;
    seed_messages_seen(&pool, chat_id, 42).await;
    let redis = fresh_redis(REDIS_URL).await;
    flush_cooldown(REDIS_URL, chat_id).await;

    let mock = MockBot::new(cmd_message(chat_id, MODERATOR_ID, "/stats"), handler());
    let state = make_state(pool.clone(), Arc::clone(&redis), mock.bot.clone()).await;
    mock.dependencies(dptree::deps![state]);
    mock.dispatch().await;

    let r = mock.get_responses();
    assert_eq!(r.sent_messages_text.len(), 1);
    let text = &r.sent_messages_text[0].message.text().unwrap_or_default();
    assert!(text.contains("Сводка за 24 часа"), "got: {text}");
    assert!(text.contains("42"), "messages_seen value missing: {text}");
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres + redis"]
async fn stats_rejects_non_moderator(pool: PgPool) {
    let chat_id = unique_chat_id();
    seed_chat(&pool, chat_id).await;
    let redis = fresh_redis(REDIS_URL).await;

    let mock = MockBot::new(cmd_message(chat_id, NON_MODERATOR_ID, "/stats"), handler());
    let state = make_state(pool.clone(), Arc::clone(&redis), mock.bot.clone()).await;
    mock.dependencies(dptree::deps![state]);
    mock.dispatch().await;

    let r = mock.get_responses();
    assert_eq!(r.sent_messages_text.len(), 1);
    let text = r.sent_messages_text[0].message.text().unwrap_or_default();
    assert!(text.to_lowercase().contains("moderator"), "got: {text}");
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres + redis"]
async fn stats_cooldown_blocks_repeat(pool: PgPool) {
    let chat_id = unique_chat_id();
    seed_chat(&pool, chat_id).await;
    seed_moderator(&pool, chat_id, MODERATOR_ID as i64).await;
    seed_messages_seen(&pool, chat_id, 5).await;
    let redis = fresh_redis(REDIS_URL).await;
    flush_cooldown(REDIS_URL, chat_id).await;

    let updates = vec![
        cmd_message(chat_id, MODERATOR_ID, "/stats"),
        cmd_message(chat_id, MODERATOR_ID, "/stats"),
    ];
    let mock = MockBot::new(updates, handler());
    let state = make_state(pool.clone(), Arc::clone(&redis), mock.bot.clone()).await;
    mock.dependencies(dptree::deps![state]);
    mock.dispatch().await;

    let r = mock.get_responses();
    assert_eq!(
        r.sent_messages_text.len(),
        2,
        "two replies (one is cooldown)"
    );
    let bodies: Vec<String> = r
        .sent_messages_text
        .iter()
        .map(|m| m.message.text().unwrap_or_default().to_string())
        .collect();
    let cooldown_count = bodies
        .iter()
        .filter(|b| b.contains("/stats") && b.to_lowercase().contains("подождите"))
        .count();
    assert_eq!(
        cooldown_count, 1,
        "second invocation must be a cooldown reply: {bodies:?}"
    );
}

// ── /summary ────────────────────────────────────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres + redis"]
async fn summary_replies_no_api_key_when_unset(pool: PgPool) {
    let chat_id = unique_chat_id();
    seed_chat(&pool, chat_id).await;
    seed_moderator(&pool, chat_id, MODERATOR_ID as i64).await;
    // Enable the feature flag so we cleanly hit the NoApiKey branch (and
    // not the SummaryDisabled branch that fires when summary_enabled is FALSE).
    sqlx::query("UPDATE chat_config SET summary_enabled = TRUE WHERE chat_id = $1")
        .bind(chat_id)
        .execute(&pool)
        .await
        .unwrap();
    let redis = fresh_redis(REDIS_URL).await;
    flush_cooldown(REDIS_URL, chat_id).await;

    let mock = MockBot::new(cmd_message(chat_id, MODERATOR_ID, "/summary"), handler());
    let state = make_state(pool.clone(), Arc::clone(&redis), mock.bot.clone()).await;
    mock.dependencies(dptree::deps![state]);
    mock.dispatch().await;

    let r = mock.get_responses();
    assert_eq!(r.sent_messages_text.len(), 1);
    let text = r.sent_messages_text[0].message.text().unwrap_or_default();
    assert!(
        text.contains("OpenAI") && text.contains("ключ"),
        "expected 'no key' message, got: {text}"
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres + redis"]
async fn summary_replies_disabled_when_summary_enabled_false(pool: PgPool) {
    // summary_enabled defaults to FALSE — even with a key, the moderator
    // must opt in to AI summaries.
    let chat_id = unique_chat_id();
    seed_chat(&pool, chat_id).await;
    seed_moderator(&pool, chat_id, MODERATOR_ID as i64).await;
    sqlx::query("UPDATE chat_config SET openai_api_key = 'sk-test' WHERE chat_id = $1")
        .bind(chat_id)
        .execute(&pool)
        .await
        .unwrap();
    let redis = fresh_redis(REDIS_URL).await;
    flush_cooldown(REDIS_URL, chat_id).await;

    let mock = MockBot::new(cmd_message(chat_id, MODERATOR_ID, "/summary"), handler());
    let state = make_state(pool.clone(), Arc::clone(&redis), mock.bot.clone()).await;
    mock.dependencies(dptree::deps![state]);
    mock.dispatch().await;

    let r = mock.get_responses();
    let text = r.sent_messages_text[0].message.text().unwrap_or_default();
    assert!(
        text.to_lowercase().contains("отключена"),
        "expected 'disabled' message, got: {text}"
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres + redis"]
async fn summary_rejects_non_moderator(pool: PgPool) {
    let chat_id = unique_chat_id();
    seed_chat(&pool, chat_id).await;
    let redis = fresh_redis(REDIS_URL).await;

    let mock = MockBot::new(
        cmd_message(chat_id, NON_MODERATOR_ID, "/summary"),
        handler(),
    );
    let state = make_state(pool.clone(), Arc::clone(&redis), mock.bot.clone()).await;
    mock.dependencies(dptree::deps![state]);
    mock.dispatch().await;

    let r = mock.get_responses();
    let text = r.sent_messages_text[0].message.text().unwrap_or_default();
    assert!(text.to_lowercase().contains("moderator"), "got: {text}");
}

// ── /report ─────────────────────────────────────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres + redis"]
async fn report_rejects_non_moderator(pool: PgPool) {
    let chat_id = unique_chat_id();
    seed_chat(&pool, chat_id).await;
    let redis = fresh_redis(REDIS_URL).await;

    let mock = MockBot::new(cmd_message(chat_id, NON_MODERATOR_ID, "/report"), handler());
    let state = make_state(pool.clone(), Arc::clone(&redis), mock.bot.clone()).await;
    mock.dependencies(dptree::deps![state]);
    mock.dispatch().await;

    let r = mock.get_responses();
    let text = r.sent_messages_text[0].message.text().unwrap_or_default();
    assert!(text.to_lowercase().contains("moderator"), "got: {text}");
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM report_messages WHERE chat_id = $1")
        .bind(chat_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0, "rejected /report must not record any messages");
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres + redis"]
async fn report_records_text_message_in_ledger(pool: PgPool) {
    // The bot's `send_photo` for the WebP chart is exercised in chart_service
    // tests; teloxide_tests 0.2 panics on the WebP multipart parser, so the
    // photo path is observed only via the report_messages INSERT it triggers.
    // The text-side assertion is the load-bearing one for /report wiring.
    let chat_id = unique_chat_id();
    seed_chat(&pool, chat_id).await;
    seed_moderator(&pool, chat_id, MODERATOR_ID as i64).await;
    seed_messages_seen(&pool, chat_id, 100).await;
    let redis = fresh_redis(REDIS_URL).await;

    let mock = MockBot::new(cmd_message(chat_id, MODERATOR_ID, "/report"), handler());
    let state = make_state(pool.clone(), Arc::clone(&redis), mock.bot.clone()).await;
    mock.dependencies(dptree::deps![state]);
    mock.dispatch().await;

    let kinds: Vec<String> =
        sqlx::query_scalar("SELECT kind FROM report_messages WHERE chat_id = $1 ORDER BY kind")
            .bind(chat_id)
            .fetch_all(&pool)
            .await
            .unwrap();
    assert!(
        kinds.contains(&"daily_text".to_string()),
        "expected daily_text row, got {kinds:?}"
    );
}
