//! Handler-level tests for `chat_member` updates (M1 fresh-join captcha).
//!
//! `teloxide_tests` 0.2 has no `MockChatMemberUpdated`, so we build the
//! `Update` manually and wrap it in an `IntoUpdate` newtype. The handler
//! itself runs unchanged — the dispatcher tree just routes our synthetic
//! `Update::ChatMember` straight into `member_update::handle`.
//!
//! Coverage:
//!
//! 1. Fresh non-bot join (Left → Member) → captcha challenge created.
//! 2. Already-verified user joins (cache hit) → no challenge created.
//! 3. Owner join (Left → Owner) → no challenge created.
//! 4. Promotion of an existing member (Member → Administrator, not a fresh
//!    join) → no challenge created.
//!
//! Photo-upload assertions are PG-side only — `teloxide_tests` 0.2 panics
//! on binary multipart, so the recorded `sent_messages_photo` vector is
//! always empty for `bot.send_photo(InputFile::memory(bytes))`. The
//! `captcha_challenges` row proves the handler ran the issuance branch.
//!
//! `#[ignore]`-gated: requires Postgres + Redis on `localhost`.

mod common;

use std::sync::Arc;
use std::sync::atomic::{AtomicI32, Ordering};

use chrono::Utc;
use common::*;
use sqlx::PgPool;
use teloxide::dispatching::UpdateHandler;
use teloxide::dptree;
use teloxide::prelude::*;
use teloxide::types::{
    ChatMember, ChatMemberKind, ChatMemberUpdated, Owner, Update, UpdateId, UpdateKind, User,
    UserId,
};
use teloxide_tests::{IntoUpdate, MockBot, MockSupergroupChat, MockUser};
use vixen_server::api::AppState;
use vixen_server::telegram::handlers::member_update;

const REDIS_URL: &str = "redis://localhost:6379/10";

/// Newtype wrapper around `ChatMemberUpdated` that satisfies
/// `teloxide_tests::IntoUpdate`. `MockBot::new` requires `IntoUpdate`; the
/// upstream crate only impls it for `MockMessage*` and `MockCallbackQuery`,
/// so we plug the gap locally.
struct MockChatMemberUpdated(ChatMemberUpdated);

impl IntoUpdate for MockChatMemberUpdated {
    fn into_update(self, id: AtomicI32) -> Vec<Update> {
        vec![Update {
            id: UpdateId(id.fetch_add(1, Ordering::Relaxed) as u32),
            kind: UpdateKind::ChatMember(self.0),
        }]
    }
}

fn handler() -> UpdateHandler<Box<dyn std::error::Error + Send + Sync + 'static>> {
    Update::filter_chat_member().endpoint(
        |bot: Bot, event: ChatMemberUpdated, state: AppState| async move {
            member_update::handle(bot, event, state)
                .await
                .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> { e.into() })
        },
    )
}

fn user(id: u64) -> User {
    MockUser::new().id(id).build()
}

fn member(user: User, kind: ChatMemberKind) -> ChatMember {
    ChatMember { user, kind }
}

/// Build a `ChatMemberUpdated` with reasonable defaults for fields the
/// handler doesn't touch (`from`, `date`, `invite_link`, etc.).
fn chat_member_update(
    chat_id: i64,
    user_id: u64,
    old: ChatMemberKind,
    new: ChatMemberKind,
) -> MockChatMemberUpdated {
    let chat = MockSupergroupChat::new().id(chat_id).build();
    let u = user(user_id);
    MockChatMemberUpdated(ChatMemberUpdated {
        chat,
        from: u.clone(),
        date: Utc::now(),
        old_chat_member: member(u.clone(), old),
        new_chat_member: member(u, new),
        invite_link: None,
        via_chat_folder_invite_link: false,
    })
}

fn owner_kind() -> ChatMemberKind {
    ChatMemberKind::Owner(Owner {
        custom_title: None,
        is_anonymous: false,
    })
}

async fn count_challenges(pool: &PgPool, chat_id: i64, user_id: i64) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM captcha_challenges WHERE chat_id = $1 AND user_id = $2",
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
async fn fresh_join_issues_captcha(pool: PgPool) {
    let chat_id = unique_chat_id();
    seed_chat(&pool, chat_id).await;
    let redis = fresh_redis(REDIS_URL).await;

    const JOINER: u64 = 11_001;
    let upd = chat_member_update(
        chat_id,
        JOINER,
        ChatMemberKind::Left,
        ChatMemberKind::Member,
    );

    let mock = MockBot::new(upd, handler());
    let state = make_state(pool.clone(), Arc::clone(&redis), mock.bot.clone()).await;
    mock.dependencies(dptree::deps![state]);
    mock.dispatch().await;

    assert_eq!(
        count_challenges(&pool, chat_id, JOINER as i64).await,
        1,
        "fresh non-bot join must persist a challenge row"
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres + redis"]
async fn already_verified_user_skips_captcha(pool: PgPool) {
    let chat_id = unique_chat_id();
    seed_chat(&pool, chat_id).await;
    let redis = fresh_redis(REDIS_URL).await;

    const JOINER: u64 = 11_002;
    seed_verified(&pool, chat_id, JOINER as i64).await;

    let upd = chat_member_update(
        chat_id,
        JOINER,
        ChatMemberKind::Left,
        ChatMemberKind::Member,
    );
    let mock = MockBot::new(upd, handler());
    let state = make_state(pool.clone(), Arc::clone(&redis), mock.bot.clone()).await;
    mock.dependencies(dptree::deps![state]);
    mock.dispatch().await;

    assert_eq!(
        count_challenges(&pool, chat_id, JOINER as i64).await,
        0,
        "already-verified rejoiner must NOT get a captcha"
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres + redis"]
async fn owner_join_skips_captcha(pool: PgPool) {
    let chat_id = unique_chat_id();
    seed_chat(&pool, chat_id).await;
    let redis = fresh_redis(REDIS_URL).await;

    const JOINER: u64 = 11_003;
    let upd = chat_member_update(chat_id, JOINER, ChatMemberKind::Left, owner_kind());

    let mock = MockBot::new(upd, handler());
    let state = make_state(pool.clone(), Arc::clone(&redis), mock.bot.clone()).await;
    mock.dependencies(dptree::deps![state]);
    mock.dispatch().await;

    assert_eq!(
        count_challenges(&pool, chat_id, JOINER as i64).await,
        0,
        "owner join is not a captcha event"
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres + redis"]
async fn promotion_of_existing_member_skips_captcha(pool: PgPool) {
    let chat_id = unique_chat_id();
    seed_chat(&pool, chat_id).await;
    let redis = fresh_redis(REDIS_URL).await;

    const JOINER: u64 = 11_004;
    // Member → Owner is a role change, not a fresh join — `is_fresh_join`
    // requires the OLD state to be Left/Banned. The handler must skip.
    let upd = chat_member_update(chat_id, JOINER, ChatMemberKind::Member, owner_kind());

    let mock = MockBot::new(upd, handler());
    let state = make_state(pool.clone(), Arc::clone(&redis), mock.bot.clone()).await;
    mock.dependencies(dptree::deps![state]);
    mock.dispatch().await;

    assert_eq!(
        count_challenges(&pool, chat_id, JOINER as i64).await,
        0,
        "role change is not a fresh join"
    );
    // Quiet `unused_imports` if linker doesn't see UserId.
    let _ = UserId(JOINER);
}
