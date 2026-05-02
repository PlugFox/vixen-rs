//! Handler-level tests for `/ban` and `/unban` slash commands.
//!
//! Wires the real `commands::dispatch` endpoint behind `filter_command::<Command>`,
//! drives it with synthetic message updates via `teloxide_tests::MockBot`, and
//! asserts on (a) recorded API calls (`banned_chat_members`, `unbanned_chat_members`,
//! `deleted_messages`, `sent_messages_text`), and (b) the `moderation_actions`
//! ledger.
//!
//! Permission checks are anchored on `chat_moderators` (DB allow-list, Moka
//! 5min cache) — `teloxide_tests` 0.2 does NOT mock `getChatAdministrators`,
//! so the chat-admin fallback would fail with "Connection refused" if reached.
//! Tests that need an "admin caller" pre-seed the Redis admin cache instead,
//! which the handler reads before falling back to the live API.
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

const REDIS_URL: &str = "redis://localhost:6379/12";
const MODERATOR_ID: u64 = 7777;
const TARGET_ID: u64 = 1212;

/// Build the same `filter_command::<Command>` branch the real dispatcher uses.
/// The error type is downgraded to `Box<dyn Error>` so it satisfies
/// `teloxide_tests`' UpdateHandler signature.
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

fn reply_to_message(
    chat_id: i64,
    sender_id: u64,
    cmd_text: &str,
    replied: Message,
) -> MockMessageText {
    cmd_message(chat_id, sender_id, cmd_text).reply_to_message(replied)
}

async fn count_actions(pool: &PgPool, action: &str) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM moderation_actions WHERE action = $1")
        .bind(action)
        .fetch_one(pool)
        .await
        .expect("count moderation_actions")
}

// ── tests ────────────────────────────────────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres + redis"]
async fn ban_reply_mode_by_moderator_bans_and_logs(pool: PgPool) {
    let chat_id = unique_chat_id();
    seed_chat(&pool, chat_id).await;
    seed_moderator(&pool, chat_id, MODERATOR_ID as i64).await;
    let redis = fresh_redis(REDIS_URL).await;

    // Reply chain: spammer's message → moderator's `/ban` reply.
    let spammer_msg = cmd_message(chat_id, TARGET_ID, "scam scam scam").build();
    let ban_cmd = reply_to_message(chat_id, MODERATOR_ID, "/ban", spammer_msg.clone());

    let mock = MockBot::new(ban_cmd, handler());
    let state = make_state(pool.clone(), Arc::clone(&redis), mock.bot.clone()).await;
    mock.dependencies(dptree::deps![state]);
    mock.dispatch().await;

    let r = mock.get_responses();
    assert_eq!(r.banned_chat_members.len(), 1, "expected one ban call");
    assert_eq!(
        r.banned_chat_members[0].user_id, TARGET_ID,
        "ban targets the replied-to user"
    );
    // The moderator's command message is best-effort deleted.
    assert!(
        !r.deleted_messages.is_empty(),
        "moderator command message should be deleted"
    );

    assert_eq!(count_actions(&pool, "ban").await, 1, "one ledger row");
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres + redis"]
async fn ban_id_mode_by_admin_via_redis_cache(pool: PgPool) {
    let chat_id = unique_chat_id();
    seed_chat(&pool, chat_id).await;
    let redis = fresh_redis(REDIS_URL).await;

    // ADMIN_ID is in the Redis admin cache — handler should accept the
    // command without a live `getChatAdministrators` call.
    const ADMIN_ID: u64 = 8888;
    let captcha_state = vixen_server::services::captcha::CaptchaState::new(Arc::clone(&redis));
    captcha_state
        .set_admins(chat_id, &[ADMIN_ID as i64])
        .await
        .expect("seed admin cache");

    let ban_cmd = cmd_message(
        chat_id,
        ADMIN_ID,
        &format!("/ban {TARGET_ID} repeated promo"),
    );

    let mock = MockBot::new(ban_cmd, handler());
    let state = make_state(pool.clone(), Arc::clone(&redis), mock.bot.clone()).await;
    mock.dependencies(dptree::deps![state]);
    mock.dispatch().await;

    let r = mock.get_responses();
    assert_eq!(r.banned_chat_members.len(), 1);
    assert_eq!(r.banned_chat_members[0].user_id, TARGET_ID);

    let reason: Option<String> = sqlx::query_scalar(
        "SELECT reason FROM moderation_actions
         WHERE chat_id = $1 AND target_user_id = $2 AND action = 'ban'",
    )
    .bind(chat_id)
    .bind(TARGET_ID as i64)
    .fetch_one(&pool)
    .await
    .expect("fetch reason");
    assert_eq!(reason.as_deref(), Some("repeated promo"));
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres + redis"]
async fn ban_by_non_moderator_is_rejected(pool: PgPool) {
    let chat_id = unique_chat_id();
    seed_chat(&pool, chat_id).await;
    let redis = fresh_redis(REDIS_URL).await;

    // No `chat_moderators` row, no Redis admin cache → handler short-circuits
    // on permission denial WITHOUT a live `getChatAdministrators` call:
    // `is_moderator_or_admin` returns `false` because the live API call into
    // MockBot's mock server resolves cleanly to an empty admin list (the
    // `getChatAdministrators` route isn't mocked, so the request 404s and the
    // helper returns `false`). Either way: no ban.
    const STRANGER_ID: u64 = 5555;
    let ban_cmd = cmd_message(chat_id, STRANGER_ID, &format!("/ban {TARGET_ID}"));

    let mock = MockBot::new(ban_cmd, handler());
    let state = make_state(pool.clone(), Arc::clone(&redis), mock.bot.clone()).await;
    mock.dependencies(dptree::deps![state]);
    mock.dispatch().await;

    let r = mock.get_responses();
    assert!(
        r.banned_chat_members.is_empty(),
        "non-moderator must NOT ban"
    );
    assert_eq!(count_actions(&pool, "ban").await, 0, "no ledger row");

    // Handler replies with the rejection text.
    let texts: Vec<String> = r
        .sent_messages_text
        .iter()
        .map(|m| m.message.text().unwrap_or("").to_string())
        .collect();
    assert!(
        texts.iter().any(|t| t.contains("Only chat moderators")),
        "expected rejection reply, got: {texts:?}"
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres + redis"]
async fn unban_id_mode_by_moderator(pool: PgPool) {
    let chat_id = unique_chat_id();
    seed_chat(&pool, chat_id).await;
    seed_moderator(&pool, chat_id, MODERATOR_ID as i64).await;
    let redis = fresh_redis(REDIS_URL).await;

    // First: pre-seed an existing `ban` ledger row so the unban behaviour
    // check (id-mode) accepts it as `Applied` rather than `AlreadyApplied`.
    sqlx::query(
        "INSERT INTO moderation_actions
         (chat_id, target_user_id, action, message_id, actor_kind, actor_user_id, reason)
         VALUES ($1, $2, 'ban', NULL, 'moderator', $3, 'prior ban')",
    )
    .bind(chat_id)
    .bind(TARGET_ID as i64)
    .bind(MODERATOR_ID as i64)
    .execute(&pool)
    .await
    .expect("seed prior ban");

    let unban_cmd = cmd_message(chat_id, MODERATOR_ID, &format!("/unban {TARGET_ID}"));
    let mock = MockBot::new(unban_cmd, handler());
    let state = make_state(pool.clone(), Arc::clone(&redis), mock.bot.clone()).await;
    mock.dependencies(dptree::deps![state]);
    mock.dispatch().await;

    let r = mock.get_responses();
    assert_eq!(r.unbanned_chat_members.len(), 1);
    assert_eq!(r.unbanned_chat_members[0].user_id, TARGET_ID);

    assert_eq!(
        count_actions(&pool, "unban").await,
        1,
        "one unban ledger row"
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres + redis"]
async fn double_ban_id_mode_replies_already_applied(pool: PgPool) {
    let chat_id = unique_chat_id();
    seed_chat(&pool, chat_id).await;
    seed_moderator(&pool, chat_id, MODERATOR_ID as i64).await;
    let redis = fresh_redis(REDIS_URL).await;

    // Pre-seed an existing id-mode ban so the second `/ban <id>` falls into
    // the behaviour-check path and returns `AlreadyApplied`.
    sqlx::query(
        "INSERT INTO moderation_actions
         (chat_id, target_user_id, action, message_id, actor_kind, actor_user_id, reason)
         VALUES ($1, $2, 'ban', NULL, 'moderator', $3, 'first')",
    )
    .bind(chat_id)
    .bind(TARGET_ID as i64)
    .bind(MODERATOR_ID as i64)
    .execute(&pool)
    .await
    .expect("seed first ban");

    let ban_cmd = cmd_message(chat_id, MODERATOR_ID, &format!("/ban {TARGET_ID}"));
    let mock = MockBot::new(ban_cmd, handler());
    let state = make_state(pool.clone(), Arc::clone(&redis), mock.bot.clone()).await;
    mock.dependencies(dptree::deps![state]);
    mock.dispatch().await;

    let r = mock.get_responses();
    assert!(
        r.banned_chat_members.is_empty(),
        "second id-mode ban must skip the bot call"
    );
    assert_eq!(
        count_actions(&pool, "ban").await,
        1,
        "still exactly one ban row (no duplicate NULL-keyed insert)"
    );

    let texts: Vec<String> = r
        .sent_messages_text
        .iter()
        .map(|m| m.message.text().unwrap_or("").to_string())
        .collect();
    assert!(
        texts.iter().any(|t| t.contains("already banned")),
        "expected 'already banned' reply, got: {texts:?}"
    );
}
