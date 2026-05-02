//! `ModerationService` integration tests via `teloxide_tests::MockBot`.
//!
//! The mock server only records API calls during `dispatch()`, so we run
//! `apply()` from inside a handler endpoint with two updates queued: the
//! second update lets us assert that a duplicate apply does NOT issue a
//! second `ban_chat_member` call.
//!
//! `#[ignore]`-gated because it needs Postgres on `localhost:5432`.

use std::sync::Arc;

use sqlx::PgPool;
use teloxide::dispatching::UpdateHandler;
use teloxide::dptree;
use teloxide::prelude::*;
use teloxide_tests::{MockBot, MockMessageText};
use vixen_server::models::moderation_action::ActorKind;
use vixen_server::services::moderation_service::{Action, ApplyContext, ModerationService};

const CHAT_ID: i64 = -1001234567890;
const USER_ID: i64 = 9999;

async fn seed_chat(pool: &PgPool, chat_id: i64) {
    sqlx::query("INSERT INTO chats (chat_id) VALUES ($1) ON CONFLICT DO NOTHING")
        .bind(chat_id)
        .execute(pool)
        .await
        .expect("seed chats");
}

#[derive(Clone)]
struct Trigger {
    pool: PgPool,
    action: Action,
    ctx: ApplyContext,
}

fn handler() -> UpdateHandler<Box<dyn std::error::Error + Send + Sync + 'static>> {
    dptree::entry().endpoint(|bot: Bot, trigger: Arc<Trigger>| async move {
        let svc = ModerationService::new(trigger.pool.clone(), bot);
        let _ = svc.apply(trigger.action.clone(), trigger.ctx).await;
        Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
    })
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres"]
async fn ban_message_scoped_idempotent(pool: PgPool) {
    seed_chat(&pool, CHAT_ID).await;

    let trigger = Arc::new(Trigger {
        pool: pool.clone(),
        action: Action::Ban {
            reason: "test".into(),
            until: None,
        },
        ctx: ApplyContext {
            chat_id: CHAT_ID,
            target_user_id: USER_ID,
            message_id: Some(123),
            actor_kind: ActorKind::Bot,
            actor_user_id: None,
        },
    });

    // Two updates → handler runs apply() twice. The second hit should be
    // a unique-key conflict, so only one ban_chat_member call leaves.
    let mock = MockBot::new(
        vec![MockMessageText::new(), MockMessageText::new().text("again")],
        handler(),
    );
    mock.dependencies(dptree::deps![trigger]);
    mock.dispatch().await;

    let r = mock.get_responses();
    assert_eq!(
        r.banned_chat_members.len(),
        1,
        "second apply must skip the bot call"
    );

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM moderation_actions")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1, "exactly one ledger row");
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres"]
async fn ban_id_mode_uses_behaviour_check(pool: PgPool) {
    // id-mode bans (message_id IS NULL) can't rely on the unique key — PG
    // treats NULLs as distinct. The behaviour check (last action = ban?)
    // catches the dupe.
    seed_chat(&pool, CHAT_ID).await;

    let trigger = Arc::new(Trigger {
        pool: pool.clone(),
        action: Action::Ban {
            reason: "manual".into(),
            until: None,
        },
        ctx: ApplyContext {
            chat_id: CHAT_ID,
            target_user_id: USER_ID,
            message_id: None,
            actor_kind: ActorKind::Moderator,
            actor_user_id: Some(7),
        },
    });

    let mock = MockBot::new(
        vec![MockMessageText::new(), MockMessageText::new().text("again")],
        handler(),
    );
    mock.dependencies(dptree::deps![trigger]);
    mock.dispatch().await;

    let r = mock.get_responses();
    assert_eq!(
        r.banned_chat_members.len(),
        1,
        "id-mode dedup via behaviour check"
    );

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM moderation_actions")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1, "no second NULL-keyed row");
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres"]
async fn unban_after_ban_applies(pool: PgPool) {
    seed_chat(&pool, CHAT_ID).await;

    // First: ban via handler.
    let ban_trigger = Arc::new(Trigger {
        pool: pool.clone(),
        action: Action::Ban {
            reason: "first".into(),
            until: None,
        },
        ctx: ApplyContext {
            chat_id: CHAT_ID,
            target_user_id: USER_ID,
            message_id: None,
            actor_kind: ActorKind::Moderator,
            actor_user_id: Some(7),
        },
    });
    let mock = MockBot::new(MockMessageText::new(), handler());
    mock.dependencies(dptree::deps![ban_trigger]);
    mock.dispatch().await;
    drop(mock);

    // Then: unban — last action is now 'ban', so behaviour check allows
    // the unban to run.
    let unban_trigger = Arc::new(Trigger {
        pool: pool.clone(),
        action: Action::Unban,
        ctx: ApplyContext {
            chat_id: CHAT_ID,
            target_user_id: USER_ID,
            message_id: None,
            actor_kind: ActorKind::Moderator,
            actor_user_id: Some(7),
        },
    });
    let mock = MockBot::new(MockMessageText::new(), handler());
    mock.dependencies(dptree::deps![unban_trigger]);
    mock.dispatch().await;

    let r = mock.get_responses();
    assert_eq!(r.unbanned_chat_members.len(), 1);

    let actions: Vec<String> =
        sqlx::query_scalar("SELECT action FROM moderation_actions ORDER BY created_at")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(actions, vec!["ban".to_string(), "unban".to_string()]);
}
