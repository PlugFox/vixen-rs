//! Idempotent startup seeding for the watched-chats invariants.
//!
//! Every `CONFIG_CHATS` entry must have a row in `chats` (so all FK references
//! to `chats(chat_id)` resolve) and in `chat_config` (so per-chat tunables
//! exist with the schema defaults). Both inserts are `ON CONFLICT DO NOTHING`
//! so re-running the binary against an already-seeded database is a no-op.

use anyhow::{Context, Result};
use sqlx::PgPool;

pub async fn ensure_watched_chats(pool: &PgPool, chat_ids: &[i64]) -> Result<()> {
    if chat_ids.is_empty() {
        return Ok(());
    }
    let mut tx = pool.begin().await.context("begin seed tx")?;
    for &chat_id in chat_ids {
        sqlx::query!(
            r#"INSERT INTO chats (chat_id) VALUES ($1) ON CONFLICT DO NOTHING"#,
            chat_id,
        )
        .execute(&mut *tx)
        .await
        .with_context(|| format!("seed chats({chat_id})"))?;
        sqlx::query!(
            r#"INSERT INTO chat_config (chat_id) VALUES ($1) ON CONFLICT DO NOTHING"#,
            chat_id,
        )
        .execute(&mut *tx)
        .await
        .with_context(|| format!("seed chat_config({chat_id})"))?;
    }
    tx.commit().await.context("commit seed tx")?;
    Ok(())
}
