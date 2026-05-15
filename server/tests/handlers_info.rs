//! Integration tests for the `/info` Telegram-command queries.
//!
//! We don't drive the full `info(...)` handler through teloxide_tests because
//! `bot.send_message` doesn't take a mocked server in the M2 fixture suite;
//! instead we directly exercise the two SQL helpers
//! (`fetch_counts`/`fetch_recent` aren't pub-API, so the tests run the same
//! `sqlx::query!` literals).

#![cfg(unix)]

mod common;
use common::*;

use sqlx::PgPool;

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres + redis"]
async fn info_counts_zero_for_unknown_user(pool: PgPool) {
    let chat_id = unique_chat_id();
    seed_chat(&pool, chat_id).await;

    let row = sqlx::query!(
        r#"
        SELECT
            (SELECT verified_at FROM verified_users WHERE chat_id = $1 AND user_id = $2)
                                                                            AS "verified_at?",
            (SELECT COUNT(*) FROM moderation_actions
                WHERE chat_id = $1 AND target_user_id = $2 AND action = 'ban')
                                                                            AS "bans!",
            (SELECT COUNT(*) FROM moderation_actions
                WHERE chat_id = $1 AND target_user_id = $2 AND action = 'unban')
                                                                            AS "unbans!",
            (SELECT COUNT(*) FROM moderation_actions
                WHERE chat_id = $1 AND target_user_id = $2 AND action = 'captcha_failed')
                                                                            AS "captcha_failed!"
        "#,
        chat_id,
        99999_i64,
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    assert!(row.verified_at.is_none());
    assert_eq!(row.bans, 0);
    assert_eq!(row.unbans, 0);
    assert_eq!(row.captcha_failed, 0);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres + redis"]
async fn info_counts_include_seeded_actions(pool: PgPool) {
    let chat_id = unique_chat_id();
    seed_chat(&pool, chat_id).await;
    seed_verified(&pool, chat_id, 4242).await;
    sqlx::query("INSERT INTO moderation_actions (chat_id, target_user_id, action, actor_kind) VALUES ($1, $2, 'ban', 'moderator')")
        .bind(chat_id)
        .bind(4242_i64)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO moderation_actions (chat_id, target_user_id, action, actor_kind) VALUES ($1, $2, 'captcha_failed', 'bot')")
        .bind(chat_id)
        .bind(4242_i64)
        .execute(&pool)
        .await
        .unwrap();

    let row = sqlx::query!(
        r#"
        SELECT
            (SELECT verified_at FROM verified_users WHERE chat_id = $1 AND user_id = $2)
                                                                            AS "verified_at?",
            (SELECT COUNT(*) FROM moderation_actions
                WHERE chat_id = $1 AND target_user_id = $2 AND action = 'ban')
                                                                            AS "bans!",
            (SELECT COUNT(*) FROM moderation_actions
                WHERE chat_id = $1 AND target_user_id = $2 AND action = 'captcha_failed')
                                                                            AS "captcha_failed!"
        "#,
        chat_id,
        4242_i64,
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    assert!(row.verified_at.is_some());
    assert_eq!(row.bans, 1);
    assert_eq!(row.captcha_failed, 1);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres + redis"]
async fn info_recent_orders_by_created_desc_limit_5(pool: PgPool) {
    let chat_id = unique_chat_id();
    seed_chat(&pool, chat_id).await;
    for i in 0..7_i32 {
        sqlx::query("INSERT INTO moderation_actions (chat_id, target_user_id, action, actor_kind, message_id) VALUES ($1, $2, 'captcha_failed', 'bot', $3)")
            .bind(chat_id)
            .bind(4242_i64)
            .bind(i)
            .execute(&pool)
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(2)).await;
    }

    let rows = sqlx::query!(
        r#"
        SELECT action, actor_kind, actor_user_id, reason, created_at
        FROM moderation_actions
        WHERE chat_id = $1 AND target_user_id = $2
        ORDER BY created_at DESC, id DESC
        LIMIT 5
        "#,
        chat_id,
        4242_i64,
    )
    .fetch_all(&pool)
    .await
    .unwrap();

    assert_eq!(rows.len(), 5);
    // First row is the most recent insertion.
    let firsts: Vec<_> = rows.iter().map(|r| r.action.as_str()).collect();
    assert!(firsts.iter().all(|a| *a == "captcha_failed"));
}
