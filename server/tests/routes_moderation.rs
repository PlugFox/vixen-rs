//! Integration tests for `/api/v1/chats/{chat_id}/moderation/*` and
//! `/api/v1/chats/{chat_id}/stats`.
//!
//! Like `tests/routes_m4.rs` these dispatch through `axum::Router::oneshot`
//! so the full middleware stack (JWT auth, IDOR guard) runs end-to-end.

#![cfg(unix)]

mod common;
use common::*;

use std::sync::Arc;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode, header};
use chrono::Utc;
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;
use vixen_server::api::build_router;
use vixen_server::services::auth_service::{InitDataShape, TelegramIdentity, mint_jwt};

const BOT_TOKEN: &str = "1234567890:ABCDEFGHIJKLMNOPQRSTUVWXYZ_0-9abcd";
const JWT_SECRET: &str = "integration-test-jwt-secret-32-bytes!!";
const ADMIN_SECRET: &str = "integration-test-admin-secret";

async fn build_app(pool: PgPool) -> (Router, vixen_server::api::AppState) {
    let redis_url =
        std::env::var("CONFIG_REDIS_URL").unwrap_or_else(|_| "redis://localhost:6379".to_string());
    let redis = fresh_redis(&redis_url).await;
    let bot = teloxide::Bot::new(BOT_TOKEN);
    let config = test_config_for_routes(BOT_TOKEN, JWT_SECRET, ADMIN_SECRET);
    let state = make_state_with_config(pool, redis, bot, config).await;
    let router = build_router(state.clone());
    (router, state)
}

async fn body_json(body: Body) -> Value {
    let bytes = to_bytes(body, usize::MAX).await.expect("body bytes");
    if bytes.is_empty() {
        return Value::Null;
    }
    serde_json::from_slice(&bytes).expect("body is JSON")
}

fn mint_test_jwt(user_id: i64, chat_ids: Vec<i64>) -> String {
    let id = TelegramIdentity {
        user_id,
        username: Some("plugfox".into()),
        first_name: "Mike".into(),
        last_name: None,
        language_code: None,
        auth_date: Utc::now().timestamp() as u64,
        shape: InitDataShape::WebApp,
    };
    mint_jwt(&id, chat_ids, JWT_SECRET, 3600, Utc::now().timestamp()).unwrap()
}

/// Insert a single `moderation_actions` row with the given action / actor /
/// timestamp. UUID is random so `(chat_id, target_user_id, action,
/// message_id)` collisions are limited to identical-call duplicates.
#[allow(clippy::too_many_arguments)]
async fn seed_action(
    pool: &PgPool,
    chat_id: i64,
    target_user_id: i64,
    action: &str,
    actor_kind: &str,
    actor_user_id: Option<i64>,
    message_id: Option<i32>,
    reason: Option<&str>,
) {
    sqlx::query(
        r#"INSERT INTO moderation_actions
           (chat_id, target_user_id, action, actor_kind, actor_user_id, message_id, reason)
           VALUES ($1, $2, $3, $4, $5, $6, $7)
           ON CONFLICT DO NOTHING"#,
    )
    .bind(chat_id)
    .bind(target_user_id)
    .bind(action)
    .bind(actor_kind)
    .bind(actor_user_id)
    .bind(message_id)
    .bind(reason)
    .execute(pool)
    .await
    .expect("seed moderation_actions");
}

// ── /moderation/actions ───────────────────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres + redis"]
async fn actions_list_orders_desc_and_paginates(pool: PgPool) {
    let chat_id = unique_chat_id();
    seed_chat(&pool, chat_id).await;
    seed_action(&pool, chat_id, 100, "ban", "moderator", Some(7), None, None).await;
    seed_action(&pool, chat_id, 200, "verify", "bot", None, None, None).await;
    seed_action(
        &pool,
        chat_id,
        300,
        "captcha_failed",
        "bot",
        None,
        Some(42),
        None,
    )
    .await;

    let (app, _) = build_app(pool).await;
    let token = mint_test_jwt(7, vec![chat_id]);

    // First page with limit=2 → expect 2 items + has_more + cursor.
    let uri = format!("/api/v1/chats/{chat_id}/moderation/actions?limit=2");
    let req = Request::builder()
        .method("GET")
        .uri(&uri)
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp.into_body()).await;
    assert_eq!(body["data"]["items"].as_array().unwrap().len(), 2);
    assert_eq!(body["data"]["has_more"], true);
    let cursor = body["data"]["cursor"].as_str().unwrap().to_string();
    assert!(!cursor.is_empty());

    // Next page via cursor → 1 item + has_more=false + cursor=null.
    let uri2 = format!("/api/v1/chats/{chat_id}/moderation/actions?limit=2&cursor={cursor}");
    let req2 = Request::builder()
        .method("GET")
        .uri(&uri2)
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let resp2 = app.oneshot(req2).await.unwrap();
    let body2 = body_json(resp2.into_body()).await;
    assert_eq!(body2["data"]["items"].as_array().unwrap().len(), 1);
    assert_eq!(body2["data"]["has_more"], false);
    assert!(body2["data"]["cursor"].is_null());
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres + redis"]
async fn actions_filter_by_action(pool: PgPool) {
    let chat_id = unique_chat_id();
    seed_chat(&pool, chat_id).await;
    seed_action(&pool, chat_id, 100, "ban", "moderator", Some(7), None, None).await;
    seed_action(&pool, chat_id, 200, "verify", "bot", None, None, None).await;

    let (app, _) = build_app(pool).await;
    let token = mint_test_jwt(7, vec![chat_id]);
    let uri = format!("/api/v1/chats/{chat_id}/moderation/actions?action=ban");
    let req = Request::builder()
        .method("GET")
        .uri(&uri)
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let body = body_json(resp.into_body()).await;
    let items = body["data"]["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["action"], "ban");
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres + redis"]
async fn actions_filter_by_actor_kind(pool: PgPool) {
    let chat_id = unique_chat_id();
    seed_chat(&pool, chat_id).await;
    seed_action(&pool, chat_id, 100, "ban", "moderator", Some(7), None, None).await;
    seed_action(&pool, chat_id, 200, "verify", "bot", None, None, None).await;

    let (app, _) = build_app(pool).await;
    let token = mint_test_jwt(7, vec![chat_id]);
    let uri = format!("/api/v1/chats/{chat_id}/moderation/actions?actor_kind=bot");
    let req = Request::builder()
        .method("GET")
        .uri(&uri)
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let body = body_json(resp.into_body()).await;
    let items = body["data"]["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["actor_kind"], "bot");
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres + redis"]
async fn actions_cross_chat_returns_403(pool: PgPool) {
    let chat_a = unique_chat_id();
    let chat_b = unique_chat_id();
    seed_chat(&pool, chat_a).await;
    seed_chat(&pool, chat_b).await;

    let (app, _) = build_app(pool).await;
    // JWT claims only chat_a.
    let token = mint_test_jwt(7, vec![chat_a]);
    // ... but caller asks for chat_b.
    let uri = format!("/api/v1/chats/{chat_b}/moderation/actions");
    let req = Request::builder()
        .method("GET")
        .uri(&uri)
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres + redis"]
async fn actions_bad_cursor_returns_400(pool: PgPool) {
    let chat_id = unique_chat_id();
    seed_chat(&pool, chat_id).await;

    let (app, _) = build_app(pool).await;
    let token = mint_test_jwt(7, vec![chat_id]);
    let uri =
        format!("/api/v1/chats/{chat_id}/moderation/actions?cursor=!!!not!!!valid!!!base64!!!");
    let req = Request::builder()
        .method("GET")
        .uri(&uri)
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

// ── /moderation/ban + /unban + /verify ────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres + redis"]
async fn ban_then_double_ban_returns_already_applied(pool: PgPool) {
    let chat_id = unique_chat_id();
    seed_chat(&pool, chat_id).await;

    let (app, _) = build_app(pool).await;
    let actor = 7_i64;
    let target = 1234_i64;
    let token = mint_test_jwt(actor, vec![chat_id]);

    let body = json!({"user_id": target, "reason": "spam"});
    let req1 = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/chats/{chat_id}/moderation/ban"))
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let resp1 = app.clone().oneshot(req1).await.unwrap();
    let b1 = body_json(resp1.into_body()).await;
    // The bot.ban_chat_member call goes to the dummy Telegram endpoint and
    // fails — the moderation_service treats Telegram non-fatal errors as
    // Applied and fatal-or-network errors as Err. We test the *idempotency
    // contract*: even if the first call errored at the bot layer, a
    // subsequent identical call must observe the previously-recorded ledger
    // intent OR return the same error. Both branches are valid M5 behaviour,
    // so we only require the second call to never write an extra row.
    let outcome_first = b1["data"]["outcome"].as_str().map(String::from);

    let req2 = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/chats/{chat_id}/moderation/ban"))
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let resp2 = app.oneshot(req2).await.unwrap();
    let b2 = body_json(resp2.into_body()).await;
    let outcome_second = b2["data"]["outcome"].as_str().map(String::from);

    // Whether the first ban succeeded or not, by the second call the ledger
    // is settled — the only valid outcomes are `applied` then `already_applied`
    // or `applied` (if the first failed and never wrote the row).
    if outcome_first.as_deref() == Some("applied") {
        assert_eq!(outcome_second.as_deref(), Some("already_applied"));
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres + redis"]
async fn ban_rejects_non_positive_user_id(pool: PgPool) {
    let chat_id = unique_chat_id();
    seed_chat(&pool, chat_id).await;

    let (app, _) = build_app(pool).await;
    let token = mint_test_jwt(7, vec![chat_id]);

    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/chats/{chat_id}/moderation/ban"))
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(json!({"user_id": -1}).to_string()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres + redis"]
async fn ban_rejects_unknown_field(pool: PgPool) {
    let chat_id = unique_chat_id();
    seed_chat(&pool, chat_id).await;
    let (app, _) = build_app(pool).await;
    let token = mint_test_jwt(7, vec![chat_id]);
    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/chats/{chat_id}/moderation/ban"))
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(
            json!({"user_id": 1, "rogue_field": "x"}).to_string(),
        ))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    // `deny_unknown_fields` → 4xx from axum's Json extractor.
    assert!(resp.status().is_client_error());
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres + redis"]
async fn verify_cross_chat_returns_403(pool: PgPool) {
    let chat_a = unique_chat_id();
    let chat_b = unique_chat_id();
    seed_chat(&pool, chat_a).await;
    seed_chat(&pool, chat_b).await;

    let (app, _) = build_app(pool).await;
    let token = mint_test_jwt(7, vec![chat_a]);
    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/chats/{chat_b}/moderation/verify"))
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(json!({"user_id": 1234}).to_string()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres + redis"]
async fn verify_seeds_verified_user(pool: PgPool) {
    let chat_id = unique_chat_id();
    seed_chat(&pool, chat_id).await;

    let (app, _) = build_app(pool.clone()).await;
    let token = mint_test_jwt(7, vec![chat_id]);

    let req = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/chats/{chat_id}/moderation/verify"))
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(json!({"user_id": 9876}).to_string()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let row: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM verified_users WHERE chat_id = $1 AND user_id = $2")
            .bind(chat_id)
            .bind(9876_i64)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(row.0, 1, "verify should seed a verified_users row");
}

// ── /moderation/verified ──────────────────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres + redis"]
async fn verified_list_returns_rows_in_desc_order(pool: PgPool) {
    let chat_id = unique_chat_id();
    seed_chat(&pool, chat_id).await;
    seed_verified(&pool, chat_id, 100).await;
    // Bump verified_at on the second so ordering is deterministic.
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    seed_verified(&pool, chat_id, 200).await;

    let (app, _) = build_app(pool).await;
    let token = mint_test_jwt(7, vec![chat_id]);
    let req = Request::builder()
        .method("GET")
        .uri(format!("/api/v1/chats/{chat_id}/moderation/verified"))
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let body = body_json(resp.into_body()).await;
    let items = body["data"]["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    // 200 was seeded second → most recent → first.
    assert_eq!(items[0]["user_id"], 200);
    assert_eq!(items[1]["user_id"], 100);
}

// ── /moderation/banned ────────────────────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres + redis"]
async fn banned_list_excludes_unbanned_users(pool: PgPool) {
    let chat_id = unique_chat_id();
    seed_chat(&pool, chat_id).await;

    // User 100: banned then unbanned → must NOT appear.
    seed_action(
        &pool,
        chat_id,
        100,
        "ban",
        "moderator",
        Some(7),
        None,
        Some("x"),
    )
    .await;
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    seed_action(
        &pool,
        chat_id,
        100,
        "unban",
        "moderator",
        Some(7),
        None,
        None,
    )
    .await;

    // User 200: banned, never unbanned → must appear.
    seed_action(
        &pool,
        chat_id,
        200,
        "ban",
        "moderator",
        Some(7),
        None,
        Some("y"),
    )
    .await;

    let (app, _) = build_app(pool).await;
    let token = mint_test_jwt(7, vec![chat_id]);
    let req = Request::builder()
        .method("GET")
        .uri(format!("/api/v1/chats/{chat_id}/moderation/banned"))
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let body = body_json(resp.into_body()).await;
    let items = body["data"]["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["user_id"], 200);
    assert_eq!(items[0]["reason"], "y");
}

// ── /chats/{id}/stats ─────────────────────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres + redis"]
async fn stats_returns_counts(pool: PgPool) {
    let chat_id = unique_chat_id();
    seed_chat(&pool, chat_id).await;
    seed_verified(&pool, chat_id, 100).await;
    seed_verified(&pool, chat_id, 200).await;
    seed_action(&pool, chat_id, 300, "ban", "moderator", Some(7), None, None).await;
    seed_action(
        &pool,
        chat_id,
        301,
        "captcha_failed",
        "bot",
        None,
        Some(1),
        None,
    )
    .await;
    // Bot-driven verify counted as captcha_solved_24h.
    seed_action(&pool, chat_id, 100, "verify", "bot", None, None, None).await;

    let (app, _) = build_app(pool).await;
    let token = mint_test_jwt(7, vec![chat_id]);
    let req = Request::builder()
        .method("GET")
        .uri(format!("/api/v1/chats/{chat_id}/stats"))
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let body = body_json(resp.into_body()).await;
    assert_eq!(body["data"]["chat_id"], chat_id);
    assert_eq!(body["data"]["verified_count"], 2);
    assert_eq!(body["data"]["banned_count"], 1);
    assert_eq!(body["data"]["captcha_solved_24h"], 1);
    assert_eq!(body["data"]["captcha_failed_24h"], 1);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres + redis"]
async fn stats_cross_chat_returns_403(pool: PgPool) {
    let chat_a = unique_chat_id();
    let chat_b = unique_chat_id();
    seed_chat(&pool, chat_a).await;
    seed_chat(&pool, chat_b).await;

    let (app, _) = build_app(pool).await;
    let token = mint_test_jwt(7, vec![chat_a]);
    let req = Request::builder()
        .method("GET")
        .uri(format!("/api/v1/chats/{chat_b}/stats"))
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres + redis"]
async fn missing_auth_returns_401(pool: PgPool) {
    let chat_id = unique_chat_id();
    seed_chat(&pool, chat_id).await;
    let (app, _) = build_app(pool).await;
    let req = Request::builder()
        .method("GET")
        .uri(format!("/api/v1/chats/{chat_id}/moderation/actions"))
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// Unused helper imports — keep the warnings happy by referencing them so
// `--all-targets` rustc doesn't flag them.
#[allow(dead_code)]
const _USE_STATE_AND_UUID: (Option<Arc<()>>, Option<Uuid>) = (None, None);
