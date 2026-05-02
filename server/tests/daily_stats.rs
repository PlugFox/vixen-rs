//! Integration tests for `models::daily_stats::increment` and `::get`.
//!
//! The helper is the single write-path for every M3 counter. These tests
//! pin the semantics that the rest of the wiring assumes:
//!
//!   * UPSERT on `(chat_id, date, kind)` accumulates `value += by`.
//!   * Distinct kinds are independent counters within the same day.
//!   * `get` returns 0 on a missing row, not an error.
//!   * The helper composes inside a `Transaction<Postgres>` (used by
//!     `captcha_service::solve` to atomically increment alongside the
//!     verified-user INSERT).

#![cfg(unix)]

mod common;
use common::*;

use sqlx::PgPool;
use vixen_server::models::daily_stats::{self, Metric};

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres"]
async fn increment_creates_row_with_initial_value(pool: PgPool) {
    let chat_id = unique_chat_id();
    seed_chat(&pool, chat_id).await;

    daily_stats::increment(&pool, chat_id, Metric::MessagesSeen, 5)
        .await
        .unwrap();

    let v = daily_stats::get(
        &pool,
        chat_id,
        chrono::Utc::now().date_naive(),
        Metric::MessagesSeen,
    )
    .await
    .unwrap();
    assert_eq!(v, 5);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres"]
async fn increment_accumulates_on_conflict(pool: PgPool) {
    let chat_id = unique_chat_id();
    seed_chat(&pool, chat_id).await;

    for delta in [3, 7, 1, 1] {
        daily_stats::increment(&pool, chat_id, Metric::MessagesSeen, delta)
            .await
            .unwrap();
    }
    let v = daily_stats::get(
        &pool,
        chat_id,
        chrono::Utc::now().date_naive(),
        Metric::MessagesSeen,
    )
    .await
    .unwrap();
    assert_eq!(v, 12);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres"]
async fn distinct_kinds_are_independent(pool: PgPool) {
    let chat_id = unique_chat_id();
    seed_chat(&pool, chat_id).await;

    daily_stats::increment(&pool, chat_id, Metric::MessagesSeen, 10)
        .await
        .unwrap();
    daily_stats::increment(&pool, chat_id, Metric::UsersBanned, 2)
        .await
        .unwrap();
    daily_stats::increment(&pool, chat_id, Metric::CaptchaSolved, 4)
        .await
        .unwrap();

    let today = chrono::Utc::now().date_naive();
    assert_eq!(
        daily_stats::get(&pool, chat_id, today, Metric::MessagesSeen)
            .await
            .unwrap(),
        10
    );
    assert_eq!(
        daily_stats::get(&pool, chat_id, today, Metric::UsersBanned)
            .await
            .unwrap(),
        2
    );
    assert_eq!(
        daily_stats::get(&pool, chat_id, today, Metric::CaptchaSolved)
            .await
            .unwrap(),
        4
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres"]
async fn distinct_chats_are_independent(pool: PgPool) {
    let chat_a = unique_chat_id();
    let chat_b = unique_chat_id();
    seed_chat(&pool, chat_a).await;
    seed_chat(&pool, chat_b).await;

    daily_stats::increment(&pool, chat_a, Metric::MessagesSeen, 50)
        .await
        .unwrap();
    daily_stats::increment(&pool, chat_b, Metric::MessagesSeen, 1)
        .await
        .unwrap();

    let today = chrono::Utc::now().date_naive();
    assert_eq!(
        daily_stats::get(&pool, chat_a, today, Metric::MessagesSeen)
            .await
            .unwrap(),
        50
    );
    assert_eq!(
        daily_stats::get(&pool, chat_b, today, Metric::MessagesSeen)
            .await
            .unwrap(),
        1
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres"]
async fn get_returns_zero_for_missing_row(pool: PgPool) {
    let chat_id = unique_chat_id();
    seed_chat(&pool, chat_id).await;
    let v = daily_stats::get(
        &pool,
        chat_id,
        chrono::Utc::now().date_naive(),
        Metric::OpenaiTokensUsed,
    )
    .await
    .unwrap();
    assert_eq!(v, 0);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres"]
async fn increment_inside_transaction_commits_with_outer_write(pool: PgPool) {
    // The captcha solve path increments daily_stats inside the same tx as
    // the INSERT into verified_users. This test pins that the helper plays
    // nice with `&mut Transaction<Postgres>`.
    let chat_id = unique_chat_id();
    seed_chat(&pool, chat_id).await;
    let user_id = 1234567_i64;

    let mut tx = pool.begin().await.unwrap();
    sqlx::query("INSERT INTO verified_users (chat_id, user_id) VALUES ($1, $2)")
        .bind(chat_id)
        .bind(user_id)
        .execute(&mut *tx)
        .await
        .unwrap();
    daily_stats::increment(&mut *tx, chat_id, Metric::UsersVerified, 1)
        .await
        .unwrap();
    tx.commit().await.unwrap();

    let v = daily_stats::get(
        &pool,
        chat_id,
        chrono::Utc::now().date_naive(),
        Metric::UsersVerified,
    )
    .await
    .unwrap();
    assert_eq!(v, 1);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres"]
async fn increment_inside_transaction_rolls_back_with_outer_write(pool: PgPool) {
    // A rolled-back tx must NOT leak the counter — confirms that
    // increment uses the supplied executor rather than `&self.pool`.
    let chat_id = unique_chat_id();
    seed_chat(&pool, chat_id).await;

    let mut tx = pool.begin().await.unwrap();
    daily_stats::increment(&mut *tx, chat_id, Metric::MessagesSeen, 100)
        .await
        .unwrap();
    tx.rollback().await.unwrap();

    let v = daily_stats::get(
        &pool,
        chat_id,
        chrono::Utc::now().date_naive(),
        Metric::MessagesSeen,
    )
    .await
    .unwrap();
    assert_eq!(v, 0);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres"]
async fn metric_db_str_round_trips(pool: PgPool) {
    // Sanity: every Metric variant maps to a DB string the report
    // aggregator can read back. Caught one regression already where
    // a renamed variant silently dropped from a report column.
    let chat_id = unique_chat_id();
    seed_chat(&pool, chat_id).await;
    for (m, expected) in [
        (Metric::MessagesSeen, "messages_seen"),
        (Metric::MessagesDeleted, "messages_deleted"),
        (Metric::UsersBanned, "users_banned"),
        (Metric::UsersVerified, "users_verified"),
        (Metric::CaptchaIssued, "captcha_issued"),
        (Metric::CaptchaSolved, "captcha_solved"),
        (Metric::CaptchaExpired, "captcha_expired"),
        (Metric::OpenaiTokensUsed, "openai_tokens_used"),
    ] {
        assert_eq!(m.as_db_str(), expected);
        daily_stats::increment(&pool, chat_id, m, 1).await.unwrap();
    }
    let row_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM daily_stats WHERE chat_id = $1 AND date = CURRENT_DATE",
    )
    .bind(chat_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row_count, 8);
}
