//! CAS client integration tests. `#[ignore]`-gated because they need a live
//! Redis at `redis://localhost:6379`. Run as part of CI's `integration` job
//! (which spins up `redis:7-alpine`) or locally via `cargo test --
//! --include-ignored`.

use std::sync::Arc;

use redis::AsyncCommands;
use serde_json::json;
use vixen_server::database::Redis;
use vixen_server::services::cas_client::{CasClient, Verdict};
use wiremock::matchers::{method, path, query_param};
use wiremock::{Mock, MockServer, ResponseTemplate};

const REDIS_URL: &str = "redis://localhost:6379/15";

async fn fresh_redis() -> Arc<Redis> {
    let r = Redis::connect(REDIS_URL).await.expect("redis connect");
    let mut conn = r.pool().get().await.expect("pool acquire");
    let _: () = redis::cmd("FLUSHDB")
        .query_async(&mut *conn)
        .await
        .expect("flushdb");
    Arc::new(r)
}

#[tokio::test]
#[ignore = "requires redis://localhost:6379"]
async fn flagged_lookup_caches_after_first_hit() {
    let redis = fresh_redis().await;
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/check"))
        .and(query_param("user_id", "42"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "ok": true,
            "result": { "user_id": 42, "offenses": 1, "messages": [] }
        })))
        .mount(&server)
        .await;

    let client = CasClient::new(redis.clone(), server.uri());

    let v1 = client.lookup(42).await;
    let v2 = client.lookup(42).await;
    assert_eq!(v1, Verdict::Flagged);
    assert_eq!(v2, Verdict::Flagged);

    let calls = server.received_requests().await.unwrap();
    assert_eq!(calls.len(), 1, "expected 1 mock call (Moka caches)");

    let mut conn = redis.pool().get().await.unwrap();
    let raw: Option<String> = conn.get("cas:42").await.unwrap();
    assert_eq!(raw.as_deref(), Some("flagged"), "redis should be primed");
}

#[tokio::test]
#[ignore = "requires redis://localhost:6379"]
async fn clean_lookup_is_cached_too() {
    let redis = fresh_redis().await;
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/check"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "ok": false,
            "description": "Record not found."
        })))
        .mount(&server)
        .await;

    let client = CasClient::new(redis, server.uri());
    assert_eq!(client.lookup(7).await, Verdict::Clean);
    assert_eq!(client.lookup(7).await, Verdict::Clean);

    let calls = server.received_requests().await.unwrap();
    assert_eq!(calls.len(), 1, "negative verdicts are cached too");
}

#[tokio::test]
#[ignore = "requires redis://localhost:6379"]
async fn http_5xx_is_fail_open_and_not_cached() {
    let redis = fresh_redis().await;
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/check"))
        .respond_with(ResponseTemplate::new(502))
        .mount(&server)
        .await;

    let client = CasClient::new(redis.clone(), server.uri());
    assert_eq!(client.lookup(99).await, Verdict::Clean);
    assert_eq!(client.lookup(99).await, Verdict::Clean);

    // A short CAS outage must not poison Moka or Redis with a stale Clean,
    // otherwise we'd be unable to ban anyone the upstream subsequently flags
    // for up to 24h.
    let calls = server.received_requests().await.unwrap();
    assert_eq!(calls.len(), 2, "fail-open should retry network, not cache");
    let mut conn = redis.pool().get().await.unwrap();
    let raw: Option<String> = conn.get("cas:99").await.unwrap();
    assert!(raw.is_none(), "fail-open Clean must not land in Redis");
}

#[tokio::test]
#[ignore = "requires redis://localhost:6379"]
async fn redis_tier_serves_after_moka_eviction() {
    // Prove the Redis tier works: prime Redis directly, then call lookup
    // against a CAS server that returns 502 — if Moka were the only cache,
    // we'd fail-open Clean; instead we read the primed Flagged from Redis.
    let redis = fresh_redis().await;
    {
        let mut conn = redis.pool().get().await.unwrap();
        let _: () = conn.set("cas:777", "flagged").await.unwrap();
    }
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/check"))
        .respond_with(ResponseTemplate::new(502))
        .mount(&server)
        .await;

    let client = CasClient::new(redis, server.uri());
    assert_eq!(client.lookup(777).await, Verdict::Flagged);

    let calls = server.received_requests().await.unwrap();
    assert!(
        calls.is_empty(),
        "redis tier should short-circuit before HTTP"
    );
}
