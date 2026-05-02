//! Integration test for `services::captcha::state::CaptchaState`.
//!
//! Requires a running Redis on `redis://localhost:6379` (the docker compose
//! default). Marked `#[ignore]` so default `cargo test` runs do not require
//! the daemon; opt-in via
//! `cargo test --test captcha_state -- --ignored`.

use std::sync::Arc;

use redis::AsyncCommands;
use vixen_server::database::Redis;
use vixen_server::services::captcha::state::{CaptchaState, VERIFIED_CACHE_TTL_SECS};

const TEST_CHAT: i64 = -10099887766; // negative supergroup id, on purpose
const TEST_USER: i64 = 424242;
const TEST_MSG: i32 = 7777;

async fn fresh_state() -> (CaptchaState, Arc<Redis>) {
    let url =
        std::env::var("CONFIG_REDIS_URL").unwrap_or_else(|_| "redis://localhost:6379/0".into());
    let redis = Arc::new(Redis::connect(url).await.expect("redis connect"));

    // Best-effort prior cleanup so reruns are isolated.
    let mut conn = redis.pool().get().await.expect("pool get");
    let _: i64 = conn
        .del(format!("cap:input:{TEST_CHAT}:{TEST_USER}"))
        .await
        .unwrap_or(0);
    let _: i64 = conn
        .del(format!("cap:meta:{TEST_CHAT}:{TEST_MSG}"))
        .await
        .unwrap_or(0);
    let _: i64 = conn
        .del(format!("cap:verified:{TEST_CHAT}:{TEST_USER}"))
        .await
        .unwrap_or(0);

    let state = CaptchaState::new(redis.clone());
    (state, redis)
}

async fn ttl(redis: &Redis, key: &str) -> i64 {
    let mut conn = redis.pool().get().await.expect("pool get");
    redis::cmd("TTL")
        .arg(key)
        .query_async(&mut *conn)
        .await
        .expect("TTL")
}

#[tokio::test]
#[ignore = "requires running redis on localhost:6379"]
async fn input_set_get_clear_with_ttl() {
    let (state, redis) = fresh_state().await;

    assert_eq!(
        state.get_input(TEST_CHAT, TEST_USER).await.unwrap(),
        "",
        "miss returns empty string, not error"
    );

    state
        .set_input(TEST_CHAT, TEST_USER, "12", 60)
        .await
        .expect("set_input");
    assert_eq!(state.get_input(TEST_CHAT, TEST_USER).await.unwrap(), "12");
    let t = ttl(&redis, &format!("cap:input:{TEST_CHAT}:{TEST_USER}")).await;
    assert!(t > 0, "TTL must be set, got {t}");

    state
        .clear_input(TEST_CHAT, TEST_USER)
        .await
        .expect("clear_input");
    assert_eq!(state.get_input(TEST_CHAT, TEST_USER).await.unwrap(), "");
}

#[tokio::test]
#[ignore = "requires running redis on localhost:6379"]
async fn meta_set_get_clear_with_ttl() {
    let (state, redis) = fresh_state().await;

    assert!(
        state.get_meta(TEST_CHAT, TEST_MSG).await.unwrap().is_none(),
        "miss returns None"
    );

    state
        .set_meta(TEST_CHAT, TEST_MSG, TEST_USER, "deadbeef", 60)
        .await
        .expect("set_meta");

    let meta = state
        .get_meta(TEST_CHAT, TEST_MSG)
        .await
        .expect("get_meta")
        .expect("meta present");
    assert_eq!(meta.owner_user_id, TEST_USER);
    assert_eq!(meta.uuid_short, "deadbeef");
    assert_eq!(meta.lifetime_secs, 60);

    let t = ttl(&redis, &format!("cap:meta:{TEST_CHAT}:{TEST_MSG}")).await;
    assert!(t > 0, "TTL must be set, got {t}");

    state
        .clear_meta(TEST_CHAT, TEST_MSG)
        .await
        .expect("clear_meta");
    assert!(state.get_meta(TEST_CHAT, TEST_MSG).await.unwrap().is_none());
}

#[tokio::test]
#[ignore = "requires running redis on localhost:6379"]
async fn verified_cache_mark_and_check() {
    let (state, redis) = fresh_state().await;

    assert!(
        !state
            .is_verified_cached(TEST_CHAT, TEST_USER)
            .await
            .unwrap()
    );

    state
        .mark_verified(TEST_CHAT, TEST_USER)
        .await
        .expect("mark_verified");
    assert!(
        state
            .is_verified_cached(TEST_CHAT, TEST_USER)
            .await
            .unwrap()
    );

    let t = ttl(&redis, &format!("cap:verified:{TEST_CHAT}:{TEST_USER}")).await;
    assert!(t > 0, "TTL must be set, got {t}");
    assert!(
        t <= VERIFIED_CACHE_TTL_SECS as i64,
        "TTL must be at most {VERIFIED_CACHE_TTL_SECS}, got {t}"
    );
    assert!(
        t > (VERIFIED_CACHE_TTL_SECS as i64) - 10,
        "TTL must be near {VERIFIED_CACHE_TTL_SECS}, got {t}"
    );

    // Cleanup
    let mut conn = redis.pool().get().await.expect("pool get");
    let _: i64 = conn
        .del(format!("cap:verified:{TEST_CHAT}:{TEST_USER}"))
        .await
        .expect("DEL");
}
