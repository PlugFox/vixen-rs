//! Handler-level tests for the captcha digit-pad callback.
//!
//! Each test pre-seeds:
//!
//! 1. A `captcha_challenges` row via `CaptchaService::issue_challenge`
//!    (which gives us the deterministic `solution`).
//! 2. A Redis meta row keyed to a synthetic `message_id`, anchoring the
//!    presser ownership check + the `uuid_short` parsed off the callback.
//!
//! Then drives a single `MockCallbackQuery` through the real
//! `captcha::handle` endpoint. Refresh is intentionally NOT covered:
//! `teloxide_tests` 0.2 does not mock `editMessageMedia`, so the refresh
//! arm logs a warn and otherwise has no observable effect from the mock's
//! perspective. Service-level coverage of refresh lives in `tests/captcha.rs`.
//!
//! `#[ignore]`-gated: requires Postgres + Redis on `localhost`.

mod common;

use std::sync::Arc;

use common::*;
use sqlx::PgPool;
use teloxide::dispatching::UpdateHandler;
use teloxide::dptree;
use teloxide::prelude::*;
use teloxide::types::CallbackQuery;
use teloxide_tests::{MockCallbackQuery, MockMessageText, MockSupergroupChat, MockUser};
use uuid::Uuid;
use vixen_server::api::AppState;
use vixen_server::services::captcha::{
    CaptchaService, CaptchaState, Fonts,
    keyboard::{data_for, short_id},
    solution_for,
};
use vixen_server::telegram::handlers::captcha as captcha_handler;

const REDIS_URL: &str = "redis://localhost:6379/9";

/// `MockBot::add_message` rewrites `message.id` to `max_message_id() + 1`
/// whenever our value is `<= max_id` or already present in the global store.
/// To keep our pre-seeded meta in sync with the id the handler actually
/// observes, we feed each test a strictly-monotonic id starting well above
/// any value MockBot would auto-assign on its own.
fn unique_message_id() -> i32 {
    use std::sync::atomic::{AtomicI32, Ordering};
    static N: AtomicI32 = AtomicI32::new(100_000_000);
    N.fetch_add(1, Ordering::Relaxed)
}

fn handler() -> UpdateHandler<Box<dyn std::error::Error + Send + Sync + 'static>> {
    Update::filter_callback_query().endpoint(
        |bot: Bot, q: CallbackQuery, state: AppState| async move {
            captcha_handler::handle(bot, q, state)
                .await
                .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.into() })
        },
    )
}

/// Build a `MockCallbackQuery` whose `message` carries the given chat + id,
/// and whose `from` is the presser. The `data` field carries the encoded
/// callback `vc:{short}:{op}`.
fn callback(
    chat_id: i64,
    message_id: i32,
    presser_id: u64,
    challenge_id: Uuid,
    op: &str,
) -> teloxide_tests::MockCallbackQuery {
    let chat = MockSupergroupChat::new().id(chat_id).build();
    // The MockMessageText id() is wrapped MessageId — we can drive it via the
    // raw setter (the field is `pub id: MessageId` after build, i32-shaped).
    let msg = MockMessageText::new()
        .text("captcha")
        .chat(chat)
        .id(message_id)
        .build();
    MockCallbackQuery::new()
        .from(MockUser::new().id(presser_id).build())
        .message(msg)
        .data(data_for(&short_id(challenge_id), op))
}

/// Pre-seed `(chat_id, owner_id)` with a captcha challenge AND a Redis meta
/// row anchored to `message_id`. Returns `(challenge_id, expected_solution)`.
async fn prime_challenge(
    pool: &PgPool,
    redis: Arc<vixen_server::database::Redis>,
    chat_id: i64,
    owner_id: i64,
    message_id: i32,
) -> (Uuid, String) {
    let fonts = Fonts::load().expect("fonts");
    let svc = CaptchaService::new(pool.clone(), fonts);
    let issued = svc
        .issue_challenge(chat_id, owner_id)
        .await
        .expect("issue challenge");
    svc.record_message_id(chat_id, owner_id, message_id)
        .await
        .expect("anchor message id");

    let state = CaptchaState::new(redis);
    let lifetime = svc.lifetime_for(chat_id).await.expect("lifetime") as u64;
    let short = short_id(issued.challenge_id);
    state
        .set_meta(chat_id, message_id, owner_id, &short, lifetime)
        .await
        .expect("seed meta");

    let solution = solution_for(issued.challenge_id);
    (issued.challenge_id, solution)
}

async fn is_verified(pool: &PgPool, chat_id: i64, user_id: i64) -> bool {
    sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM verified_users WHERE chat_id = $1 AND user_id = $2)",
    )
    .bind(chat_id)
    .bind(user_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

// ── tests ────────────────────────────────────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres + redis"]
async fn digit_press_below_solution_len_edits_caption(pool: PgPool) {
    let chat_id = unique_chat_id();
    seed_chat(&pool, chat_id).await;
    let redis = fresh_redis(REDIS_URL).await;

    const OWNER: u64 = 22_001;
    let mid = unique_message_id();
    let (cid, _) = prime_challenge(&pool, Arc::clone(&redis), chat_id, OWNER as i64, mid).await;

    // Press one digit ("1"). Buffer becomes 1 char (< SOLUTION_LEN=4) → handler
    // sets input in Redis and calls edit_message_caption.
    let q = callback(chat_id, mid, OWNER, cid, "1");
    let mock = teloxide_tests::MockBot::new(q, handler());
    let state = make_state(pool.clone(), Arc::clone(&redis), mock.bot.clone()).await;
    mock.dependencies(dptree::deps![state]);
    mock.dispatch().await;

    let r = mock.get_responses();
    assert_eq!(r.answered_callback_queries.len(), 1, "ack must always fire");
    assert_eq!(
        r.edited_messages_caption.len(),
        1,
        "intermediate digit edits the caption"
    );
    assert!(!is_verified(&pool, chat_id, OWNER as i64).await);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres + redis"]
async fn correct_solution_marks_user_verified(pool: PgPool) {
    let chat_id = unique_chat_id();
    seed_chat(&pool, chat_id).await;
    let redis = fresh_redis(REDIS_URL).await;

    const OWNER: u64 = 22_002;
    let mid = unique_message_id();
    let (cid, solution) =
        prime_challenge(&pool, Arc::clone(&redis), chat_id, OWNER as i64, mid).await;

    // Pre-seed first three digits; the test will press the fourth which
    // triggers `solve()`. Reusing the live captcha state mirrors what
    // sequential clicks would produce.
    let captcha_state = CaptchaState::new(Arc::clone(&redis));
    let lifetime = 60u64;
    captcha_state
        .set_input(chat_id, OWNER as i64, &solution[..3], lifetime)
        .await
        .expect("seed partial input");

    // Final digit — pushes input to length 4 and runs `solve()`.
    let final_digit = &solution[3..4];
    let q = callback(chat_id, mid, OWNER, cid, final_digit);
    let mock = teloxide_tests::MockBot::new(q, handler());
    let state = make_state(pool.clone(), Arc::clone(&redis), mock.bot.clone()).await;
    mock.dependencies(dptree::deps![state]);
    mock.dispatch().await;

    assert!(
        is_verified(&pool, chat_id, OWNER as i64).await,
        "correct solution must persist verified_users row"
    );
    let r = mock.get_responses();
    assert_eq!(r.answered_callback_queries.len(), 1);
    // `on_solved` deletes the captcha message.
    assert!(
        !r.deleted_messages.is_empty(),
        "solved captcha message should be deleted"
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres + redis"]
async fn wrong_solution_clears_input_and_edits_caption(pool: PgPool) {
    let chat_id = unique_chat_id();
    seed_chat(&pool, chat_id).await;
    let redis = fresh_redis(REDIS_URL).await;

    const OWNER: u64 = 22_003;
    let mid = unique_message_id();
    let (cid, solution) =
        prime_challenge(&pool, Arc::clone(&redis), chat_id, OWNER as i64, mid).await;

    // Build a definitely-wrong 3-char prefix from a digit not in the solution.
    let bad_digit = (b'0'..=b'9')
        .map(|b| b as char)
        .find(|d| !solution.contains(*d))
        .expect("at least one absent digit");
    let captcha_state = CaptchaState::new(Arc::clone(&redis));
    let mut three: String = bad_digit.to_string();
    three.push(bad_digit);
    three.push(bad_digit);
    captcha_state
        .set_input(chat_id, OWNER as i64, &three, 60)
        .await
        .expect("seed wrong prefix");

    // Press another bad digit → length 4 → solve fails → `WrongLeft` →
    // input cleared in Redis, caption edited with "Wrong, try again".
    let q = callback(chat_id, mid, OWNER, cid, &bad_digit.to_string());
    let mock = teloxide_tests::MockBot::new(q, handler());
    let state = make_state(pool.clone(), Arc::clone(&redis), mock.bot.clone()).await;
    mock.dependencies(dptree::deps![state]);
    mock.dispatch().await;

    assert!(!is_verified(&pool, chat_id, OWNER as i64).await);

    let captcha_state_check = CaptchaState::new(Arc::clone(&redis));
    let cleared = captcha_state_check
        .get_input(chat_id, OWNER as i64)
        .await
        .expect("get_input");
    assert_eq!(cleared, "", "wrong solution must clear the input buffer");

    let r = mock.get_responses();
    assert_eq!(r.edited_messages_caption.len(), 1);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres + redis"]
async fn non_owner_press_gets_toast_and_no_state_change(pool: PgPool) {
    let chat_id = unique_chat_id();
    seed_chat(&pool, chat_id).await;
    let redis = fresh_redis(REDIS_URL).await;

    const OWNER: u64 = 22_004;
    const STRANGER: u64 = 22_005;
    let mid = unique_message_id();
    let (cid, _) = prime_challenge(&pool, Arc::clone(&redis), chat_id, OWNER as i64, mid).await;

    // STRANGER presses "1" — must be rejected with the "isn't yours" toast.
    let q = callback(chat_id, mid, STRANGER, cid, "1");
    let mock = teloxide_tests::MockBot::new(q, handler());
    let state = make_state(pool.clone(), Arc::clone(&redis), mock.bot.clone()).await;
    mock.dependencies(dptree::deps![state]);
    mock.dispatch().await;

    let r = mock.get_responses();
    assert_eq!(r.answered_callback_queries.len(), 1);
    let toast_text = r.answered_callback_queries[0]
        .text
        .clone()
        .unwrap_or_default();
    assert!(
        toast_text.contains("isn't your captcha"),
        "expected ownership toast, got {toast_text:?}"
    );
    assert!(
        r.edited_messages_caption.is_empty(),
        "stranger's press must not edit the caption"
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres + redis"]
async fn backspace_shortens_buffer_and_edits_caption(pool: PgPool) {
    let chat_id = unique_chat_id();
    seed_chat(&pool, chat_id).await;
    let redis = fresh_redis(REDIS_URL).await;

    const OWNER: u64 = 22_006;
    let mid = unique_message_id();
    let (cid, _) = prime_challenge(&pool, Arc::clone(&redis), chat_id, OWNER as i64, mid).await;

    // Pre-seed a 2-digit prefix; backspace should leave 1 digit.
    let captcha_state = CaptchaState::new(Arc::clone(&redis));
    captcha_state
        .set_input(chat_id, OWNER as i64, "12", 60)
        .await
        .expect("seed input");

    let q = callback(chat_id, mid, OWNER, cid, "bs");
    let mock = teloxide_tests::MockBot::new(q, handler());
    let state = make_state(pool.clone(), Arc::clone(&redis), mock.bot.clone()).await;
    mock.dependencies(dptree::deps![state]);
    mock.dispatch().await;

    let cs2 = CaptchaState::new(Arc::clone(&redis));
    let after = cs2
        .get_input(chat_id, OWNER as i64)
        .await
        .expect("get_input");
    assert_eq!(after, "1", "backspace must drop the last digit");

    let r = mock.get_responses();
    assert_eq!(r.edited_messages_caption.len(), 1);
}
