//! `spam_messages` dedup helpers — xxh3-keyed lookup and write-through.
//!
//! On a known-spam hit we bump `hit_count` and `last_seen`; on a fresh
//! n-gram match we register the message so subsequent copies short-circuit
//! to "Ban (dedup hit)" without re-running the n-gram scoring or CAS lookup.
//! Sample bodies are truncated to 4 KiB before storage — enough for an
//! audit/replay UI, bounded enough to keep the table small.

use anyhow::{Context, Result};
use sqlx::PgPool;

const SAMPLE_BODY_MAX: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DedupOutcome {
    Hit { hit_count: i64 },
    Miss,
}

pub async fn lookup(pool: &PgPool, hash: i64) -> Result<DedupOutcome> {
    let row = sqlx::query!(
        "SELECT hit_count FROM spam_messages WHERE xxh3_hash = $1",
        hash
    )
    .fetch_optional(pool)
    .await
    .context("SELECT spam_messages")?;
    Ok(match row {
        Some(r) => DedupOutcome::Hit {
            hit_count: r.hit_count,
        },
        None => DedupOutcome::Miss,
    })
}

/// Bump hit-count and last-seen on a known-spam re-occurrence.
pub async fn bump(pool: &PgPool, hash: i64) -> Result<()> {
    sqlx::query!(
        r#"
        UPDATE spam_messages
        SET hit_count = hit_count + 1,
            last_seen = NOW()
        WHERE xxh3_hash = $1
        "#,
        hash,
    )
    .execute(pool)
    .await
    .context("UPDATE spam_messages")?;
    Ok(())
}

/// Register a new spam fingerprint (called by the n-gram and CAS branches so
/// the next copy of the same text dedups in O(1)). Idempotent: re-recording
/// the same hash bumps `hit_count` instead of failing on the PK conflict.
pub async fn record(pool: &PgPool, hash: i64, sample: &str) -> Result<()> {
    let truncated = if sample.len() > SAMPLE_BODY_MAX {
        // Truncate on a char boundary to keep sample_body valid UTF-8.
        let mut end = SAMPLE_BODY_MAX;
        while !sample.is_char_boundary(end) {
            end -= 1;
        }
        &sample[..end]
    } else {
        sample
    };
    sqlx::query!(
        r#"
        INSERT INTO spam_messages (xxh3_hash, sample_body)
        VALUES ($1, $2)
        ON CONFLICT (xxh3_hash) DO UPDATE
            SET hit_count = spam_messages.hit_count + 1,
                last_seen = NOW()
        "#,
        hash,
        truncated,
    )
    .execute(pool)
    .await
    .context("INSERT spam_messages")?;
    Ok(())
}
