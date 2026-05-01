//! Redis pooled client (`deadpool-redis`) plus a generic pattern-pub/sub helper.
//!
//! Pooled connections are used for ordinary commands (PUBLISH, GET/SET, ...).
//! A subscription needs a dedicated long-lived connection — `subscribe()` opens
//! its own `redis::Client` to keep the pool clean.

use std::time::Duration;

use deadpool_redis::{Config as DpConfig, Pool, Runtime};
use futures::StreamExt;
use redis::AsyncCommands;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

#[derive(Clone)]
pub struct Redis {
    pool: Pool,
    /// Captured at construction time so `subscribe()` can build a dedicated
    /// pubsub client without callers re-passing the URL.
    url: String,
}

impl Redis {
    /// Build the pool from a `redis://` URL and verify reachability with a
    /// PING round-trip (subject to the pool's wait+create timeouts).
    pub async fn connect(url: impl Into<String>) -> Result<Self, RedisError> {
        let url = url.into();
        let pool = DpConfig::from_url(&url).create_pool(Some(Runtime::Tokio1))?;
        let me = Self { pool, url };
        me.ping().await?;
        Ok(me)
    }

    pub fn pool(&self) -> &Pool {
        &self.pool
    }

    /// Cheap liveness probe used by `/health`.
    pub async fn ping(&self) -> Result<(), RedisError> {
        let mut conn = self.pool.get().await?;
        let pong: String = redis::cmd("PING").query_async(&mut *conn).await?;
        if pong != "PONG" {
            return Err(RedisError::UnexpectedPong(pong));
        }
        Ok(())
    }

    /// Publish a payload on a channel.
    pub async fn publish(&self, channel: &str, payload: &str) -> Result<u64, RedisError> {
        let mut conn = self.pool.get().await?;
        let receivers: u64 = conn.publish(channel, payload).await?;
        Ok(receivers)
    }

    /// Spawn a task that PSUBSCRIBEs to `pattern` and invokes `on_message` for
    /// every received message. The task stops when `cancel` is fired.
    ///
    /// `on_message` is async-free: it runs synchronously within the message
    /// loop. Heavy work belongs in a separate spawned task so the loop keeps
    /// reading. Failures during connect / psubscribe are logged at error and
    /// the task returns; consumers can re-subscribe by calling this again.
    pub fn subscribe<F>(
        &self,
        pattern: impl Into<String>,
        cancel: CancellationToken,
        mut on_message: F,
    ) -> JoinHandle<()>
    where
        F: FnMut(String, String) + Send + 'static,
    {
        let url = self.url.clone();
        let pattern = pattern.into();
        tokio::spawn(async move {
            let client = match redis::Client::open(url) {
                Ok(c) => c,
                Err(e) => {
                    error!(error = %e, pattern, "redis pubsub: open failed");
                    return;
                }
            };
            let mut pubsub = match client.get_async_pubsub().await {
                Ok(c) => c,
                Err(e) => {
                    error!(error = %e, pattern, "redis pubsub: connect failed");
                    return;
                }
            };
            if let Err(e) = pubsub.psubscribe(&pattern).await {
                error!(error = %e, pattern, "redis pubsub: psubscribe failed");
                return;
            }
            info!(pattern, "redis pubsub: subscribed");

            let mut stream = pubsub.on_message();
            loop {
                tokio::select! {
                    biased;
                    _ = cancel.cancelled() => {
                        info!(pattern, "redis pubsub: shutting down");
                        return;
                    }
                    next = stream.next() => {
                        let Some(msg) = next else {
                            warn!(pattern, "redis pubsub: stream ended");
                            return;
                        };
                        let channel = msg.get_channel_name().to_string();
                        let payload: String = match msg.get_payload() {
                            Ok(p) => p,
                            Err(e) => {
                                warn!(error = %e, channel, "redis pubsub: bad payload");
                                continue;
                            }
                        };
                        debug!(channel, "redis pubsub: message");
                        on_message(channel, payload);
                    }
                }
            }
        })
    }
}

/// Default for ad-hoc command timeouts. Not used internally; exported for
/// callers that want a sensible bound.
pub const COMMAND_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(thiserror::Error, Debug)]
pub enum RedisError {
    #[error("redis pool create: {0}")]
    PoolCreate(#[from] deadpool_redis::CreatePoolError),
    #[error("redis pool acquire: {0}")]
    PoolAcquire(#[from] deadpool_redis::PoolError),
    #[error("redis: {0}")]
    Redis(#[from] redis::RedisError),
    #[error("unexpected PING response: {0:?}")]
    UnexpectedPong(String),
}
