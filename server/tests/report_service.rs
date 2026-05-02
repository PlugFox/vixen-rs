//! Integration tests for `ReportService::aggregate`. Live Postgres only —
//! we seed `daily_stats` + `moderation_actions` + `spam_messages` rows, run
//! the aggregator, and assert the returned struct matches the seed.

#![cfg(unix)]

mod common;
use common::*;

use chrono::{Duration, Utc};
use sqlx::PgPool;
use vixen_server::services::report_service::ReportService;

fn requires_postgres() -> bool {
    std::env::var("DATABASE_URL").is_ok()
}

#[sqlx::test]
async fn aggregate_sums_seeded_metrics(pool: PgPool) {
    if !requires_postgres() {
        return;
    }
    let chat_id = unique_chat_id();
    seed_chat(&pool, chat_id).await;

    // Seed counters.
    sqlx::query(
        r#"
        INSERT INTO daily_stats (chat_id, date, kind, value) VALUES
            ($1, CURRENT_DATE, 'messages_seen',   42),
            ($1, CURRENT_DATE, 'captcha_issued',  3),
            ($1, CURRENT_DATE, 'captcha_solved',  2),
            ($1, CURRENT_DATE, 'captcha_expired', 1)
        "#,
    )
    .bind(chat_id)
    .execute(&pool)
    .await
    .unwrap();

    // Seed moderation_actions for the COUNT(*) branch.
    sqlx::query(
        r#"
        INSERT INTO moderation_actions
            (chat_id, target_user_id, action, actor_kind, message_id)
        VALUES
            ($1, 1001, 'ban',    'bot', 100),
            ($1, 1002, 'delete', 'bot', 101),
            ($1, 1003, 'verify', 'bot', 102),
            ($1, 1004, 'verify', 'bot', 103)
        "#,
    )
    .bind(chat_id)
    .execute(&pool)
    .await
    .unwrap();

    // Seed top phrases (global table).
    sqlx::query(
        r#"
        INSERT INTO spam_messages (xxh3_hash, sample_body, hit_count, last_seen)
        VALUES (1, 'spam-A', 7, NOW()),
               (2, 'spam-B', 3, NOW())
        ON CONFLICT (xxh3_hash) DO NOTHING
        "#,
    )
    .execute(&pool)
    .await
    .unwrap();

    let service = ReportService::new(pool.clone());
    let to = Utc::now() + Duration::hours(1);
    let from = to - Duration::hours(48);
    let report = service.aggregate(chat_id, from, to).await.unwrap();

    assert_eq!(report.chat_id, chat_id);
    assert_eq!(report.messages_seen, 42);
    assert_eq!(report.users_banned, 1);
    assert_eq!(report.messages_deleted, 1);
    assert_eq!(report.users_verified, 2);
    assert_eq!(report.captcha.issued, 3);
    assert_eq!(report.captcha.solved, 2);
    assert_eq!(report.captcha.expired, 1);
    assert_eq!(report.last_7_days_messages.len(), 7);
    let last_day = report.last_7_days_messages.last().unwrap();
    assert_eq!(last_day.messages, 42);

    // Top phrases ordered by hit_count DESC.
    assert_eq!(
        report.top_phrases.first().map(|p| p.text.as_str()),
        Some("spam-A")
    );
}

#[sqlx::test]
async fn aggregate_with_no_data_returns_zeros(pool: PgPool) {
    if !requires_postgres() {
        return;
    }
    let chat_id = unique_chat_id();
    seed_chat(&pool, chat_id).await;

    let service = ReportService::new(pool.clone());
    let to = Utc::now() + Duration::hours(1);
    let from = to - Duration::hours(24);
    let report = service.aggregate(chat_id, from, to).await.unwrap();

    assert_eq!(report.messages_seen, 0);
    assert_eq!(report.users_banned, 0);
    assert_eq!(report.captcha.issued, 0);
    assert_eq!(report.last_7_days_messages.len(), 7);
    for p in &report.last_7_days_messages {
        assert_eq!(p.messages, 0);
    }
}
