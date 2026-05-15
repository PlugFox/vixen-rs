//! Integration tests for the M4 HTTP surface:
//!  * `POST /api/v1/auth/telegram/login`
//!  * `GET  /api/v1/auth/me`
//!  * `GET  /api/v1/chats`
//!  * `GET  /api/v1/chats/{chat_id}/config`
//!  * `PATCH /api/v1/chats/{chat_id}/config`
//!  * `POST /admin/ping`
//!
//! Requests are dispatched through `axum::Router::oneshot` — no TCP listener,
//! no real bot. The router is built from the same `build_router` the binary
//! uses, so middleware ordering, OpenAPI registration and CORS are all
//! exercised in situ.

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

async fn seed_moderator_row(pool: &PgPool, chat_id: i64, user_id: i64) {
    sqlx::query(
        r#"INSERT INTO chat_moderators (chat_id, user_id, granted_by)
           VALUES ($1, $2, $2)
           ON CONFLICT (chat_id, user_id) DO NOTHING"#,
    )
    .bind(chat_id)
    .bind(user_id)
    .execute(pool)
    .await
    .unwrap();
}

// ── Login ────────────────────────────────────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres + redis"]
async fn login_happy_path_mints_jwt_with_chat_ids(pool: PgPool) {
    let chat_id = unique_chat_id();
    seed_chat(&pool, chat_id).await;
    let user_id = 4242_i64;
    seed_moderator_row(&pool, chat_id, user_id).await;

    let (app, _) = build_app(pool).await;

    let init_data = sign_webapp_init_data(
        &json!({"id": user_id, "first_name": "Mike", "username": "plugfox"}).to_string(),
        BOT_TOKEN,
        Utc::now().timestamp() as u64,
    );
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/telegram/login")
        .header("content-type", "application/json")
        .body(Body::from(json!({"init_data": init_data}).to_string()))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = body_json(resp.into_body()).await;
    assert_eq!(body["status"], "ok");
    let token = body["data"]["token"].as_str().unwrap();
    assert!(!token.is_empty());
    assert_eq!(body["data"]["expires_in"], 3600);
    assert_eq!(body["data"]["user"]["id"], user_id);
    let chat_ids = body["data"]["chat_ids"].as_array().unwrap();
    assert!(chat_ids.iter().any(|v| v.as_i64() == Some(chat_id)));
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres + redis"]
async fn login_with_tampered_hash_returns_401(pool: PgPool) {
    let (app, _) = build_app(pool).await;
    let mut init_data = sign_webapp_init_data(
        &json!({"id": 1, "first_name": "Mike"}).to_string(),
        BOT_TOKEN,
        Utc::now().timestamp() as u64,
    );
    let last = init_data.pop().unwrap();
    init_data.push(if last == '0' { '1' } else { '0' });

    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/telegram/login")
        .header("content-type", "application/json")
        .body(Body::from(json!({"init_data": init_data}).to_string()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres + redis"]
async fn login_with_expired_auth_date_returns_401(pool: PgPool) {
    let (app, _) = build_app(pool).await;
    let very_old = (Utc::now().timestamp() as u64).saturating_sub(86_400 * 2);
    let init_data = sign_webapp_init_data(
        &json!({"id": 1, "first_name": "Mike"}).to_string(),
        BOT_TOKEN,
        very_old,
    );
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/telegram/login")
        .header("content-type", "application/json")
        .body(Body::from(json!({"init_data": init_data}).to_string()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// ── /auth/me ─────────────────────────────────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres + redis"]
async fn me_returns_claims_with_valid_jwt(pool: PgPool) {
    let (app, _) = build_app(pool).await;
    let token = mint_test_jwt(7, vec![-100, -101]);
    let req = Request::builder()
        .method("GET")
        .uri("/api/v1/auth/me")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = body_json(resp.into_body()).await;
    assert_eq!(body["data"]["user_id"], 7);
    let ids = body["data"]["chat_ids"].as_array().unwrap();
    assert_eq!(ids.len(), 2);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres + redis"]
async fn me_rejects_missing_auth(pool: PgPool) {
    let (app, _) = build_app(pool).await;
    let req = Request::builder()
        .method("GET")
        .uri("/api/v1/auth/me")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres + redis"]
async fn me_rejects_bad_signature(pool: PgPool) {
    let (app, _) = build_app(pool).await;
    // Mint with a different secret entirely.
    let id = TelegramIdentity {
        user_id: 1,
        username: None,
        first_name: "A".into(),
        last_name: None,
        language_code: None,
        auth_date: 0,
        shape: InitDataShape::WebApp,
    };
    let bad_token = mint_jwt(
        &id,
        vec![],
        "wrong-secret-but-still-32-bytes-or-so",
        3600,
        Utc::now().timestamp(),
    )
    .unwrap();
    let req = Request::builder()
        .method("GET")
        .uri("/api/v1/auth/me")
        .header(header::AUTHORIZATION, format!("Bearer {bad_token}"))
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

// ── /chats ────────────────────────────────────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres + redis"]
async fn list_chats_returns_only_intersection_with_jwt_claim(pool: PgPool) {
    let chat_a = unique_chat_id();
    let chat_b = unique_chat_id();
    let chat_c = unique_chat_id();
    seed_chat(&pool, chat_a).await;
    seed_chat(&pool, chat_b).await;
    // chat_c not seeded - JWT claims it but the chats table doesn't have it.

    let (app, _) = build_app(pool).await;
    // JWT claims a, b, c; DB has only a, b → response should list a + b.
    let token = mint_test_jwt(1, vec![chat_a, chat_b, chat_c]);
    let req = Request::builder()
        .method("GET")
        .uri("/api/v1/chats")
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = body_json(resp.into_body()).await;
    let chats = body["data"]["chats"].as_array().unwrap();
    let ids: Vec<i64> = chats
        .iter()
        .map(|c| c["chat_id"].as_i64().unwrap())
        .collect();
    assert!(ids.contains(&chat_a));
    assert!(ids.contains(&chat_b));
    assert!(!ids.contains(&chat_c));
}

// ── /chats/{id}/config ───────────────────────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres + redis"]
async fn get_config_returns_defaults_and_masks_openai_key(pool: PgPool) {
    let chat_id = unique_chat_id();
    seed_chat(&pool, chat_id).await;
    let (app, _) = build_app(pool).await;
    let token = mint_test_jwt(1, vec![chat_id]);

    let req = Request::builder()
        .method("GET")
        .uri(format!("/api/v1/chats/{chat_id}/config"))
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = body_json(resp.into_body()).await;
    assert_eq!(body["data"]["chat_id"], chat_id);
    assert_eq!(body["data"]["language"], "ru");
    assert_eq!(body["data"]["openai_api_key_set"], false);
    // Raw openai_api_key field must never be in the response.
    assert!(body["data"].get("openai_api_key").is_none());
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres + redis"]
async fn get_config_cross_chat_returns_403(pool: PgPool) {
    let chat_id = unique_chat_id();
    seed_chat(&pool, chat_id).await;
    let (app, _) = build_app(pool).await;
    // JWT claims a DIFFERENT chat — caller is not a moderator of `chat_id`.
    let token = mint_test_jwt(1, vec![chat_id - 1]);

    let req = Request::builder()
        .method("GET")
        .uri(format!("/api/v1/chats/{chat_id}/config"))
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres + redis"]
async fn patch_config_updates_fields_and_returns_fresh(pool: PgPool) {
    let chat_id = unique_chat_id();
    seed_chat(&pool, chat_id).await;
    let (app, _) = build_app(pool).await;
    let token = mint_test_jwt(1, vec![chat_id]);

    let req = Request::builder()
        .method("PATCH")
        .uri(format!("/api/v1/chats/{chat_id}/config"))
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(
            json!({
                "captcha_enabled": false,
                "language": "en",
                "openai_api_key": "sk-from-patch"
            })
            .to_string(),
        ))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let body = body_json(resp.into_body()).await;
    assert_eq!(body["data"]["captcha_enabled"], false);
    assert_eq!(body["data"]["language"], "en");
    assert_eq!(body["data"]["openai_api_key_set"], true);
    // Raw key never echoed.
    let json_str = body.to_string();
    assert!(!json_str.contains("sk-from-patch"));
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres + redis"]
async fn patch_config_null_clears_openai_key(pool: PgPool) {
    let chat_id = unique_chat_id();
    seed_chat(&pool, chat_id).await;
    let (app, _) = build_app(pool).await;
    let token = mint_test_jwt(1, vec![chat_id]);

    // Set then clear.
    let set_req = Request::builder()
        .method("PATCH")
        .uri(format!("/api/v1/chats/{chat_id}/config"))
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(json!({"openai_api_key": "sk-temp"}).to_string()))
        .unwrap();
    let resp = app.clone().oneshot(set_req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let clear_req = Request::builder()
        .method("PATCH")
        .uri(format!("/api/v1/chats/{chat_id}/config"))
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(json!({"openai_api_key": null}).to_string()))
        .unwrap();
    let resp = app.oneshot(clear_req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp.into_body()).await;
    assert_eq!(body["data"]["openai_api_key_set"], false);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres + redis"]
async fn patch_config_rejects_bad_range_with_400(pool: PgPool) {
    let chat_id = unique_chat_id();
    seed_chat(&pool, chat_id).await;
    let (app, _) = build_app(pool).await;
    let token = mint_test_jwt(1, vec![chat_id]);

    let req = Request::builder()
        .method("PATCH")
        .uri(format!("/api/v1/chats/{chat_id}/config"))
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(json!({"report_hour": 99}).to_string()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres + redis"]
async fn patch_config_empty_body_returns_422(pool: PgPool) {
    let chat_id = unique_chat_id();
    seed_chat(&pool, chat_id).await;
    let (app, _) = build_app(pool).await;
    let token = mint_test_jwt(1, vec![chat_id]);

    let req = Request::builder()
        .method("PATCH")
        .uri(format!("/api/v1/chats/{chat_id}/config"))
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from("{}"))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres + redis"]
async fn patch_config_cross_chat_returns_403(pool: PgPool) {
    let chat_id = unique_chat_id();
    seed_chat(&pool, chat_id).await;
    let (app, _) = build_app(pool).await;
    // Caller does not moderate `chat_id`.
    let token = mint_test_jwt(1, vec![chat_id - 1]);

    let req = Request::builder()
        .method("PATCH")
        .uri(format!("/api/v1/chats/{chat_id}/config"))
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(json!({"captcha_enabled": false}).to_string()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

// ── /admin/ping ──────────────────────────────────────────────────────────

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres + redis"]
async fn admin_ping_happy_path(pool: PgPool) {
    let (app, _) = build_app(pool).await;
    let req = Request::builder()
        .method("POST")
        .uri("/admin/ping")
        .header("x-admin-secret", ADMIN_SECRET)
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp.into_body()).await;
    assert_eq!(body["data"]["status"], "ok");
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres + redis"]
async fn admin_ping_rejects_missing_secret(pool: PgPool) {
    let (app, _) = build_app(pool).await;
    let req = Request::builder()
        .method("POST")
        .uri("/admin/ping")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres + redis"]
async fn admin_ping_rejects_wrong_secret(pool: PgPool) {
    let (app, _) = build_app(pool).await;
    let req = Request::builder()
        .method("POST")
        .uri("/admin/ping")
        .header("x-admin-secret", "totally-the-wrong-secret")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

/// Regression: a configured-but-empty `CONFIG_ADMIN_SECRET` previously
/// authenticated every request that omitted the header (`Sha256("")
/// == Sha256("")`). `Config::validate` now rejects it at parse time, and
/// the middleware refuses even when an empty secret slips into AppState
/// through some other path.
#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres + redis"]
async fn admin_ping_refuses_empty_configured_secret(pool: PgPool) {
    use vixen_server::config::AdminSecret;

    let redis_url =
        std::env::var("CONFIG_REDIS_URL").unwrap_or_else(|_| "redis://localhost:6379".to_string());
    let redis = fresh_redis(&redis_url).await;
    let bot = teloxide::Bot::new(BOT_TOKEN);
    let mut config = test_config_for_routes(BOT_TOKEN, JWT_SECRET, ADMIN_SECRET);
    // Bypass `Config::validate` here — we are exactly testing the
    // defense-in-depth path inside the middleware for the "empty admin
    // secret somehow reached AppState" scenario.
    config.admin_secret = Some(AdminSecret::new(""));
    let state = make_state_with_config(pool, redis, bot, config).await;
    let app = build_router(state);

    // No X-Admin-Secret header — would historically authenticate via the
    // `"" == ""` SHA-256 collision.
    let req = Request::builder()
        .method("POST")
        .uri("/admin/ping")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::SERVICE_UNAVAILABLE,
        "empty configured secret must short-circuit to 503, not authenticate"
    );

    // Even sending the matching empty header must fail.
    let req = Request::builder()
        .method("POST")
        .uri("/admin/ping")
        .header("x-admin-secret", "")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
}

// ── End-to-end: PATCH propagates to bot reads via Moka invalidation ─────

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres + redis"]
async fn patch_config_invalidates_in_process_moka_cache(pool: PgPool) {
    let chat_id = unique_chat_id();
    seed_chat(&pool, chat_id).await;
    let (app, state) = build_app(pool).await;
    let token = mint_test_jwt(1, vec![chat_id]);

    // Prime the cache via the service directly (mimics a bot-side read).
    let before = state.chat_config.get(chat_id).await.unwrap();
    assert!(before.captcha_enabled);

    // Edit via the API.
    let req = Request::builder()
        .method("PATCH")
        .uri(format!("/api/v1/chats/{chat_id}/config"))
        .header(header::AUTHORIZATION, format!("Bearer {token}"))
        .header("content-type", "application/json")
        .body(Body::from(json!({"captcha_enabled": false}).to_string()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Subsequent service read sees the new value — `update` refreshes the
    // local Moka entry before returning.
    let after = state.chat_config.get(chat_id).await.unwrap();
    assert!(!after.captcha_enabled);

    // Keep the variable in scope so the compiler can't elide the Arc.
    drop(Arc::clone(&state.chat_config));
}
