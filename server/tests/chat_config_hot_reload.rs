//! End-to-end hot-reload test for the M4 contract.
//!
//! Setup mimics two replicas of the server. `writer` invokes `update(chat_id,
//! patch)` (commits to Postgres + PUBLISHes on `chat_config:{chat_id}`).
//! `reader` is subscribed to the same pub/sub channel and its
//! `chat_config_service::invalidate` is wired to the subscribe callback.
//!
//! The assertion: after `writer.update()` completes, `reader.get(chat_id)`
//! returns the new value within 1 second WITHOUT any restart. That mirrors
//! the bot's runtime path — it reads `chat_config` through the service, and
//! the service's Moka entry is evicted by the same callback this test wires.

#![cfg(unix)]

mod common;
use common::*;

use std::sync::Arc;
use std::time::{Duration, Instant};

use sqlx::PgPool;
use tokio_util::sync::CancellationToken;
use vixen_server::database::Database;
use vixen_server::models::ChatConfigPatch;
use vixen_server::services::chat_config_service::{self, ChatConfigService};

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires postgres + redis"]
async fn update_propagates_to_subscriber_within_one_second(pool: PgPool) {
    let redis_url =
        std::env::var("CONFIG_REDIS_URL").unwrap_or_else(|_| "redis://localhost:6379".to_string());
    let chat_id = unique_chat_id();
    seed_chat(&pool, chat_id).await;

    let redis = fresh_redis(&redis_url).await;
    let db = Arc::new(Database::from_pool(pool));

    // Reader and writer share the same DB + Redis but have independent Moka caches.
    let reader = Arc::new(ChatConfigService::new(db.clone(), redis.clone()));
    let writer = Arc::new(ChatConfigService::new(db.clone(), redis.clone()));

    // Wire the subscriber: reader invalidates on every chat_config message.
    let cancel = CancellationToken::new();
    let reader_sub = reader.clone();
    let sub_handle = redis.subscribe("chat_config:*", cancel.clone(), move |channel, _payload| {
        if let Some(id) = chat_config_service::chat_id_from_channel(&channel) {
            let svc = reader_sub.clone();
            tokio::spawn(async move {
                svc.invalidate(id).await;
            });
        }
    });

    // PSUBSCRIBE on Redis is async; give the loop a moment to register
    // before publishing. 200ms is plenty in practice.
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Seed reader's cache.
    let before = reader.get(chat_id).await.unwrap();
    assert!(
        before.captcha_enabled,
        "seed default is captcha_enabled=true"
    );

    // Writer flips the bit.
    let started = Instant::now();
    writer
        .update(
            chat_id,
            ChatConfigPatch {
                captcha_enabled: Some(false),
                ..Default::default()
            },
        )
        .await
        .unwrap();

    // Poll reader.get until it observes the flip or 1s elapses.
    let deadline = started + Duration::from_secs(1);
    let mut observed = before.captcha_enabled;
    while Instant::now() < deadline {
        let v = reader.get(chat_id).await.unwrap();
        observed = v.captcha_enabled;
        if !observed {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let elapsed = started.elapsed();

    cancel.cancel();
    let _ = sub_handle.await;

    assert!(
        !observed,
        "reader still saw stale captcha_enabled=true after {elapsed:?}; pub/sub did not propagate"
    );
    assert!(
        elapsed < Duration::from_secs(1),
        "propagation took {elapsed:?}; budget is 1s"
    );
}
