//! Integration tests for `ChatConfigService` against a real Postgres + Redis.
//!
//! Per the project convention every live-Postgres test is `#[ignore]`-gated so
//! the default `cargo test` stays DB-free. CI's "Integration" job
//! (`.github/workflows/server-ci.yml`) runs them with `--include-ignored`.

#![cfg(unix)]

mod common;
use common::*;

use std::sync::Arc;

use sqlx::PgPool;
use vixen_server::database::Database;
use vixen_server::models::ChatConfigPatch;
use vixen_server::services::chat_config_service::{ChatConfigError, ChatConfigService};

async fn build_service(pool: PgPool) -> Arc<ChatConfigService> {
    let redis_url =
        std::env::var("CONFIG_REDIS_URL").unwrap_or_else(|_| "redis://localhost:6379".to_string());
    let redis = fresh_redis(&redis_url).await;
    let db = Arc::new(Database::from_pool(pool));
    Arc::new(ChatConfigService::new(db, redis))
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres + redis"]
async fn get_returns_seeded_defaults(pool: PgPool) {
    let chat_id = unique_chat_id();
    seed_chat(&pool, chat_id).await;
    let svc = build_service(pool).await;

    let cfg = svc.get(chat_id).await.expect("get default config");
    // seed_chat overrides cas_enabled = FALSE; every other column is the
    // schema default (see migration 20260502000000).
    assert_eq!(cfg.chat_id, chat_id);
    assert!(cfg.captcha_enabled);
    assert!(cfg.spam_enabled);
    assert!(!cfg.cas_enabled);
    assert_eq!(cfg.report_hour, 17);
    assert_eq!(cfg.timezone, "UTC");
    assert_eq!(cfg.language, "ru");
    assert!(cfg.openai_api_key.is_none());
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres + redis"]
async fn get_returns_not_found_for_unseeded_chat(pool: PgPool) {
    let svc = build_service(pool).await;
    let err = svc
        .get(-9_999_999)
        .await
        .expect_err("missing row -> NotFound");
    assert!(matches!(err, ChatConfigError::NotFound(-9_999_999)));
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres + redis"]
async fn update_changes_single_field(pool: PgPool) {
    let chat_id = unique_chat_id();
    seed_chat(&pool, chat_id).await;
    let svc = build_service(pool).await;

    let patch = ChatConfigPatch {
        captcha_enabled: Some(false),
        ..Default::default()
    };
    let updated = svc.update(chat_id, patch).await.unwrap();
    assert!(!updated.captcha_enabled);
    // Other fields untouched.
    assert_eq!(updated.report_hour, 17);
    assert_eq!(updated.language, "ru");

    // Re-fetch hits Moka (refreshed by update); should match what update returned.
    let refetched = svc.get(chat_id).await.unwrap();
    assert!(!refetched.captcha_enabled);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres + redis"]
async fn update_multiple_fields(pool: PgPool) {
    let chat_id = unique_chat_id();
    seed_chat(&pool, chat_id).await;
    let svc = build_service(pool).await;

    let patch = ChatConfigPatch {
        report_hour: Some(9),
        timezone: Some("Europe/Moscow".into()),
        language: Some("en".into()),
        summary_enabled: Some(true),
        ..Default::default()
    };
    let updated = svc.update(chat_id, patch).await.unwrap();
    assert_eq!(updated.report_hour, 9);
    assert_eq!(updated.timezone, "Europe/Moscow");
    assert_eq!(updated.language, "en");
    assert!(updated.summary_enabled);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres + redis"]
async fn update_openai_key_three_state_semantics(pool: PgPool) {
    let chat_id = unique_chat_id();
    seed_chat(&pool, chat_id).await;
    let svc = build_service(pool).await;

    // Set the key.
    let set_patch = ChatConfigPatch {
        openai_api_key: Some(Some("sk-test-1".into())),
        ..Default::default()
    };
    let after_set = svc.update(chat_id, set_patch).await.unwrap();
    assert_eq!(after_set.openai_api_key.as_deref(), Some("sk-test-1"));

    // Absent field: key stays.
    let touch_patch = ChatConfigPatch {
        report_hour: Some(10),
        ..Default::default()
    };
    let after_touch = svc.update(chat_id, touch_patch).await.unwrap();
    assert_eq!(after_touch.openai_api_key.as_deref(), Some("sk-test-1"));

    // Explicit null: key cleared.
    let clear_patch = ChatConfigPatch {
        openai_api_key: Some(None),
        ..Default::default()
    };
    let after_clear = svc.update(chat_id, clear_patch).await.unwrap();
    assert!(after_clear.openai_api_key.is_none());
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres + redis"]
async fn update_rejects_empty_patch(pool: PgPool) {
    let chat_id = unique_chat_id();
    seed_chat(&pool, chat_id).await;
    let svc = build_service(pool).await;

    let err = svc
        .update(chat_id, ChatConfigPatch::default())
        .await
        .expect_err("empty patch rejected");
    assert!(matches!(err, ChatConfigError::EmptyPatch));
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres + redis"]
async fn update_rejects_out_of_range_values(pool: PgPool) {
    let chat_id = unique_chat_id();
    seed_chat(&pool, chat_id).await;
    let svc = build_service(pool).await;

    let patch = ChatConfigPatch {
        report_hour: Some(99),
        ..Default::default()
    };
    let err = svc.update(chat_id, patch).await.expect_err("bad range");
    assert!(matches!(err, ChatConfigError::Validation(_)));
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres + redis"]
async fn update_unseeded_chat_returns_not_found(pool: PgPool) {
    let svc = build_service(pool).await;
    let patch = ChatConfigPatch {
        captcha_enabled: Some(false),
        ..Default::default()
    };
    let err = svc
        .update(-12_345_678, patch)
        .await
        .expect_err("missing row -> NotFound");
    assert!(matches!(err, ChatConfigError::NotFound(_)));
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres + redis"]
async fn cache_is_refreshed_on_update(pool: PgPool) {
    let chat_id = unique_chat_id();
    seed_chat(&pool, chat_id).await;
    let svc = build_service(pool).await;

    let before = svc.get(chat_id).await.unwrap();
    assert!(before.spam_enabled);

    svc.update(
        chat_id,
        ChatConfigPatch {
            spam_enabled: Some(false),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let after = svc.get(chat_id).await.unwrap();
    assert!(!after.spam_enabled, "post-update cache must hold new value");
}
