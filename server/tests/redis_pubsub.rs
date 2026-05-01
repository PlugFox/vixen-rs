//! Integration test for `Redis::publish` / `Redis::subscribe` round-trip.
//!
//! Requires a running Redis on `redis://localhost:6379` (the docker compose
//! default). Marked `#[ignore]` so default `cargo test` runs do not require
//! the daemon; opt-in via `cargo test --test redis_pubsub -- --ignored`.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tokio_util::sync::CancellationToken;
use vixen_server::database::Redis;

#[tokio::test]
#[ignore = "requires running redis on localhost:6379"]
async fn publish_subscribe_roundtrip() {
    let redis = Redis::connect("redis://localhost:6379")
        .await
        .expect("redis connect");

    let cancel = CancellationToken::new();
    let received = Arc::new(AtomicBool::new(false));
    let received_clone = received.clone();

    let handle = redis.subscribe(
        "vixen:test:pubsub:*".to_string(),
        cancel.clone(),
        move |channel, payload| {
            assert_eq!(channel, "vixen:test:pubsub:hello");
            assert_eq!(payload, "world");
            received_clone.store(true, Ordering::SeqCst);
        },
    );

    // Give the subscriber a beat to PSUBSCRIBE before we publish — the
    // dedicated pubsub connection is established asynchronously.
    tokio::time::sleep(Duration::from_millis(150)).await;

    let receivers = redis
        .publish("vixen:test:pubsub:hello", "world")
        .await
        .expect("publish");
    assert!(
        receivers >= 1,
        "expected at least one subscriber, got {receivers}"
    );

    // Wait up to 1s for the callback to fire.
    let deadline = std::time::Instant::now() + Duration::from_secs(1);
    while !received.load(Ordering::SeqCst) && std::time::Instant::now() < deadline {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(
        received.load(Ordering::SeqCst),
        "subscriber callback never fired"
    );

    cancel.cancel();
    let _ = tokio::time::timeout(Duration::from_secs(2), handle).await;
}
