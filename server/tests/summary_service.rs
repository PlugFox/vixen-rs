//! Integration tests for `SummaryService::summarize`. The OpenAI client is
//! pointed at a `wiremock` server, the chat config is seeded per case.

#![cfg(unix)]

mod common;
use common::*;

use std::sync::Arc;

use chrono::{Duration, Utc};
use serde_json::json;
use sqlx::PgPool;
use vixen_server::models::daily_stats::{self, Metric};
use vixen_server::services::openai_client::OpenAiClient;
use vixen_server::services::summary_service::{SkipReason, SummaryOutcome, SummaryService};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

fn requires_postgres() -> bool {
    std::env::var("DATABASE_URL").is_ok()
}

async fn build_service(pool: PgPool, base_url: String) -> Arc<SummaryService> {
    let client = Arc::new(OpenAiClient::new(base_url));
    SummaryService::new(pool, client)
}

async fn seed_summary_chat(
    pool: &PgPool,
    chat_id: i64,
    api_key: Option<&str>,
    summary_enabled: bool,
    log_messages: bool,
    budget: i32,
) {
    seed_chat(pool, chat_id).await;
    sqlx::query(
        r#"
        UPDATE chat_config
        SET summary_enabled = $2,
            summary_token_budget = $3,
            log_allowed_messages = $4,
            openai_api_key = $5
        WHERE chat_id = $1
        "#,
    )
    .bind(chat_id)
    .bind(summary_enabled)
    .bind(budget)
    .bind(log_messages)
    .bind(api_key)
    .execute(pool)
    .await
    .unwrap();
}

async fn seed_allowed_messages(pool: &PgPool, chat_id: i64, count: i32) {
    for i in 0..count {
        sqlx::query(
            r#"
            INSERT INTO allowed_messages
                (chat_id, message_id, user_id, kind, length, content)
            VALUES ($1, $2, $3, 'text', $4, $5)
            ON CONFLICT (chat_id, message_id) DO NOTHING
            "#,
        )
        .bind(chat_id)
        .bind(i64::from(i + 1))
        .bind(1000_i64)
        .bind(20_i32)
        .bind(format!("Sample chat message number {i} discussing topic X"))
        .execute(pool)
        .await
        .unwrap();
    }
}

#[sqlx::test]
async fn summarize_skips_when_no_api_key(pool: PgPool) {
    if !requires_postgres() {
        return;
    }
    let chat_id = unique_chat_id();
    seed_summary_chat(&pool, chat_id, None, true, true, 50_000).await;

    let service = build_service(pool, "http://localhost:0".to_string()).await;
    let to = Utc::now();
    let from = to - Duration::hours(24);
    let outcome = service.summarize(chat_id, from, to, "ru").await.unwrap();
    assert!(matches!(
        outcome,
        SummaryOutcome::Skipped {
            reason: SkipReason::NoApiKey
        }
    ));
}

#[sqlx::test]
async fn summarize_skips_when_disabled(pool: PgPool) {
    if !requires_postgres() {
        return;
    }
    let chat_id = unique_chat_id();
    seed_summary_chat(&pool, chat_id, Some("sk-test"), false, true, 50_000).await;

    let service = build_service(pool, "http://localhost:0".to_string()).await;
    let to = Utc::now();
    let from = to - Duration::hours(24);
    let outcome = service.summarize(chat_id, from, to, "ru").await.unwrap();
    assert!(matches!(
        outcome,
        SummaryOutcome::Skipped {
            reason: SkipReason::Disabled
        }
    ));
}

#[sqlx::test]
async fn summarize_skips_when_budget_exhausted(pool: PgPool) {
    if !requires_postgres() {
        return;
    }
    let chat_id = unique_chat_id();
    seed_summary_chat(&pool, chat_id, Some("sk-test"), true, true, 100).await;
    daily_stats::increment(&pool, chat_id, Metric::OpenaiTokensUsed, 200)
        .await
        .unwrap();

    let service = build_service(pool, "http://localhost:0".to_string()).await;
    let to = Utc::now();
    let from = to - Duration::hours(24);
    let outcome = service.summarize(chat_id, from, to, "ru").await.unwrap();
    match outcome {
        SummaryOutcome::Skipped {
            reason: SkipReason::BudgetExhausted { used, budget },
        } => {
            assert_eq!(used, 200);
            assert_eq!(budget, 100);
        }
        other => panic!("expected BudgetExhausted, got {other:?}"),
    }
}

#[sqlx::test]
async fn summarize_skips_when_no_messages(pool: PgPool) {
    if !requires_postgres() {
        return;
    }
    let chat_id = unique_chat_id();
    seed_summary_chat(&pool, chat_id, Some("sk-test"), true, false, 50_000).await;

    let service = build_service(pool, "http://localhost:0".to_string()).await;
    let to = Utc::now();
    let from = to - Duration::hours(24);
    let outcome = service.summarize(chat_id, from, to, "ru").await.unwrap();
    assert!(matches!(
        outcome,
        SummaryOutcome::Skipped {
            reason: SkipReason::NoMessages
        }
    ));
}

#[sqlx::test]
async fn summarize_generates_and_increments_tokens(pool: PgPool) {
    if !requires_postgres() {
        return;
    }
    let chat_id = unique_chat_id();
    seed_summary_chat(&pool, chat_id, Some("sk-test"), true, true, 50_000).await;
    seed_allowed_messages(&pool, chat_id, 5).await;

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "choices": [{"message": {"content": "• Topic X discussed"}}],
            "usage": {"total_tokens": 123}
        })))
        .mount(&server)
        .await;

    let service = build_service(pool.clone(), server.uri()).await;
    let to = Utc::now() + Duration::hours(1);
    let from = to - Duration::hours(24);
    let outcome = service.summarize(chat_id, from, to, "ru").await.unwrap();

    match outcome {
        SummaryOutcome::Generated { text, tokens_used } => {
            assert!(text.contains("Topic X"));
            assert_eq!(tokens_used, 123);
        }
        other => panic!("expected Generated, got {other:?}"),
    }

    let used = daily_stats::get(
        &pool,
        chat_id,
        Utc::now().date_naive(),
        Metric::OpenaiTokensUsed,
    )
    .await
    .unwrap();
    assert_eq!(used, 123);
}

#[sqlx::test]
async fn summarize_retries_on_429_then_succeeds(pool: PgPool) {
    if !requires_postgres() {
        return;
    }
    let chat_id = unique_chat_id();
    seed_summary_chat(&pool, chat_id, Some("sk-test"), true, true, 50_000).await;
    seed_allowed_messages(&pool, chat_id, 3).await;

    let server = MockServer::start().await;
    // First call: 429.
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(429)
                .insert_header("retry-after", "1")
                .set_body_string("rate limited"),
        )
        .up_to_n_times(1)
        .mount(&server)
        .await;
    // Subsequent: 200.
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "choices": [{"message": {"content": "summary after retry"}}],
            "usage": {"total_tokens": 50}
        })))
        .mount(&server)
        .await;

    let service = build_service(pool, server.uri()).await;
    let to = Utc::now() + Duration::hours(1);
    let from = to - Duration::hours(24);
    let outcome = service.summarize(chat_id, from, to, "ru").await.unwrap();

    match outcome {
        SummaryOutcome::Generated { text, tokens_used } => {
            assert!(text.contains("retry"));
            assert_eq!(tokens_used, 50);
        }
        other => panic!("expected Generated after retry, got {other:?}"),
    }
}

#[sqlx::test]
async fn summarize_returns_error_when_retries_exhausted(pool: PgPool) {
    // Persistent 429 → after MAX_RETRIES the client surfaces an error. We
    // do NOT swallow this as `Skipped` — Skipped is for policy decisions,
    // transport failures must propagate so the caller can pick the right
    // user-facing message (or, in /summary's case, fall back to a generic
    // "временно недоступна" reply).
    if !requires_postgres() {
        return;
    }
    let chat_id = unique_chat_id();
    seed_summary_chat(&pool, chat_id, Some("sk-test"), true, true, 50_000).await;
    seed_allowed_messages(&pool, chat_id, 3).await;

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(429)
                .insert_header("retry-after", "1")
                .set_body_string("rate limited forever"),
        )
        .mount(&server)
        .await;

    let service = build_service(pool, server.uri()).await;
    let to = Utc::now() + Duration::hours(1);
    let from = to - Duration::hours(24);
    let result = service.summarize(chat_id, from, to, "ru").await;
    assert!(result.is_err(), "expected Err on exhausted retries");
    let msg = format!("{:#}", result.unwrap_err());
    assert!(
        msg.contains("openai") || msg.to_lowercase().contains("attempts"),
        "expected error message to mention OpenAI / attempts: {msg}"
    );
}

#[sqlx::test]
async fn summarize_propagates_non_retryable_4xx(pool: PgPool) {
    // 401 (bad key) is NOT retried. The error surfaces immediately.
    if !requires_postgres() {
        return;
    }
    let chat_id = unique_chat_id();
    seed_summary_chat(&pool, chat_id, Some("sk-bad"), true, true, 50_000).await;
    seed_allowed_messages(&pool, chat_id, 3).await;

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(401).set_body_string("{\"error\":{\"message\":\"bad key\"}}"),
        )
        .expect(1) // exactly one call — no retries on 401
        .mount(&server)
        .await;

    let service = build_service(pool, server.uri()).await;
    let to = Utc::now() + Duration::hours(1);
    let from = to - Duration::hours(24);
    let result = service.summarize(chat_id, from, to, "ru").await;
    assert!(result.is_err(), "401 must surface as Err");
}

#[sqlx::test]
async fn summarize_sanitizes_outgoing_body(pool: PgPool) {
    // The user prompt the client sends to OpenAI must contain none of the
    // raw URL / phone / email / @-mention patterns from the input — only
    // their `[link]` / `[phone]` / `[email]` / `[user]` placeholders.
    if !requires_postgres() {
        return;
    }
    let chat_id = unique_chat_id();
    seed_summary_chat(&pool, chat_id, Some("sk-test"), true, true, 50_000).await;

    // Insert one message containing every PII pattern the sanitizer covers.
    sqlx::query(
        r#"
        INSERT INTO allowed_messages (chat_id, message_id, user_id, kind, length, content)
        VALUES ($1, 1, 100, 'text', 200, $2)
        "#,
    )
    .bind(chat_id)
    .bind("ping @plugfox at foo@bar.com or +1 (555) 123-4567 about https://secret.example.com/path")
    .execute(&pool)
    .await
    .unwrap();

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(|req: &Request| {
            let body: serde_json::Value =
                serde_json::from_slice(&req.body).expect("decode request body");
            let user_msg = body["messages"][1]["content"].as_str().unwrap_or_default();
            // Hard-fail by returning 500 — wiremock's `expect()` panics on
            // unmet but the response itself is what we read; since we cannot
            // panic from inside the closure cleanly, we instead echo the
            // sanitised body so the test asserts on the response.
            ResponseTemplate::new(200).set_body_json(json!({
                "choices": [{"message": {"content": user_msg}}],
                "usage": {"total_tokens": 1},
            }))
        })
        .mount(&server)
        .await;

    let service = build_service(pool, server.uri()).await;
    let to = Utc::now() + Duration::hours(1);
    let from = to - Duration::hours(24);
    let outcome = service.summarize(chat_id, from, to, "ru").await.unwrap();

    match outcome {
        SummaryOutcome::Generated { text, .. } => {
            // Echoed user-prompt should NOT contain any raw PII.
            assert!(!text.contains("@plugfox"), "@-mention leaked: {text}");
            assert!(!text.contains("foo@bar.com"), "email leaked: {text}");
            assert!(!text.contains("secret.example.com"), "URL leaked: {text}");
            assert!(!text.contains("555"), "phone digits leaked: {text}");
            // …and SHOULD contain the placeholder tokens.
            assert!(text.contains("[user]"));
            assert!(text.contains("[email]"));
            assert!(text.contains("[link]"));
            assert!(text.contains("[phone]"));
        }
        other => panic!("expected Generated, got {other:?}"),
    }
}

#[sqlx::test]
async fn summarize_skips_short_messages(pool: PgPool) {
    // The MIN_MESSAGE_CHARS filter drops sub-4-char messages; if every
    // logged message is short, the prompt is empty and the service
    // short-circuits to NoMessages without an HTTP call.
    if !requires_postgres() {
        return;
    }
    let chat_id = unique_chat_id();
    seed_summary_chat(&pool, chat_id, Some("sk-test"), true, true, 50_000).await;
    for i in 0..5 {
        sqlx::query(
            "INSERT INTO allowed_messages (chat_id, message_id, user_id, kind, length, content)
             VALUES ($1, $2, 100, 'text', 2, $3)",
        )
        .bind(chat_id)
        .bind(i64::from(i + 1))
        .bind("hi")
        .execute(&pool)
        .await
        .unwrap();
    }

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0) // must not be called
        .mount(&server)
        .await;

    let service = build_service(pool, server.uri()).await;
    let to = Utc::now() + Duration::hours(1);
    let from = to - Duration::hours(24);
    let outcome = service.summarize(chat_id, from, to, "ru").await.unwrap();
    assert!(matches!(
        outcome,
        SummaryOutcome::Skipped {
            reason: SkipReason::NoMessages
        }
    ));
}
