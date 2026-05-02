//! `CaptchaService` — issues, solves and re-issues captcha challenges.
//!
//! The service is Telegram-free: it talks to Postgres only. Bot side-effects
//! (restrict / send_photo / delete_message / kick) live in the handlers and
//! the expiry job. This keeps the service trivially testable with `sqlx::test`.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use teloxide::types::InlineKeyboardMarkup;
use uuid::Uuid;
use xxhash_rust::xxh3::xxh3_64;

use super::fonts::Fonts;
use super::keyboard::digit_pad;
use super::render::render_webp;

const SOLUTION_LEN: usize = 4;
const DEFAULT_ATTEMPTS: i16 = 5;
const DEFAULT_LIFETIME_SECS: i32 = 60;

/// Public face of a freshly-issued challenge.
///
/// The plaintext `solution` is intentionally NOT exposed here — it would be a
/// trivial accident to land in a `tracing::debug!(?issued, ...)` and leak.
/// Callers that need it (tests only) recompute it from `challenge_id` via the
/// public deterministic helper [`solution_for`].
#[derive(Debug, Clone)]
pub struct IssuedChallenge {
    pub challenge_id: Uuid,
    pub image_webp: Vec<u8>,
    pub keyboard: InlineKeyboardMarkup,
    pub expires_at: DateTime<Utc>,
    pub attempts_left: i16,
}

/// Outcome of a single captcha interaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Correct solution; user is now in `verified_users`.
    Solved,
    /// Already verified before this call (idempotent re-fire).
    AlreadyVerified,
    /// Wrong attempt; `n` attempts remain and the row is intact.
    WrongLeft(i16),
    /// Final wrong attempt; the challenge row has been deleted and a
    /// `captcha_failed` ledger row is written. The user is NOT kicked — M1's
    /// policy is to keep silently deleting their messages until they pass
    /// captcha; the next message they send will trigger a fresh challenge.
    WrongFinal,
    /// Challenge expired before being solved; the row has been deleted and a
    /// `captcha_expired` ledger row is written. The user is NOT kicked — same
    /// rationale as `WrongFinal`. The expiry-job sweep won't pick it up.
    Expired,
    /// No pending challenge for `(chat_id, user_id)`. Likely a delayed callback
    /// after the row was already cleaned up.
    NotFound,
}

#[derive(Clone)]
pub struct CaptchaService {
    pool: PgPool,
    fonts: Fonts,
}

impl CaptchaService {
    pub fn new(pool: PgPool, fonts: Fonts) -> Self {
        Self { pool, fonts }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub fn fonts(&self) -> &Fonts {
        &self.fonts
    }

    /// True if the user is verified in this chat.
    pub async fn is_verified(&self, chat_id: i64, user_id: i64) -> Result<bool> {
        let row = sqlx::query_scalar!(
            r#"SELECT EXISTS(
                SELECT 1 FROM verified_users WHERE chat_id = $1 AND user_id = $2
            ) AS "exists!""#,
            chat_id,
            user_id,
        )
        .fetch_one(&self.pool)
        .await
        .context("SELECT verified_users")?;
        Ok(row)
    }

    /// Issue (or re-issue, on row conflict) a fresh challenge for the user.
    pub async fn issue_challenge(&self, chat_id: i64, user_id: i64) -> Result<IssuedChallenge> {
        let challenge_id = Uuid::new_v4();
        let solution = solution_for(challenge_id);
        let attempts = self.attempts_for(chat_id).await?;
        let lifetime = self.lifetime_for(chat_id).await?;

        let row = sqlx::query!(
            r#"
            INSERT INTO captcha_challenges
                (id, chat_id, user_id, solution, attempts_left, expires_at)
            VALUES
                ($1, $2, $3, $4, $5, NOW() + make_interval(secs => $6::DOUBLE PRECISION))
            ON CONFLICT (chat_id, user_id) DO UPDATE SET
                id                  = EXCLUDED.id,
                solution            = EXCLUDED.solution,
                attempts_left       = EXCLUDED.attempts_left,
                telegram_message_id = NULL,
                expires_at          = EXCLUDED.expires_at,
                created_at          = NOW()
            RETURNING id, attempts_left, expires_at
            "#,
            challenge_id,
            chat_id,
            user_id,
            &solution,
            attempts,
            lifetime as f64,
        )
        .fetch_one(&self.pool)
        .await
        .context("INSERT captcha_challenges")?;

        let bytes = self.render(row.id, &solution).await?;
        Ok(IssuedChallenge {
            challenge_id: row.id,
            image_webp: bytes,
            keyboard: digit_pad(row.id),
            expires_at: row.expires_at,
            attempts_left: row.attempts_left,
        })
    }

    /// Re-issue with a fresh solution + image. Used by the refresh button.
    pub async fn reissue(&self, chat_id: i64, user_id: i64) -> Result<IssuedChallenge> {
        // Same upsert path is fine — the existing row gets a new id, solution
        // and timer, telegram_message_id is reset because the bot will
        // edit_message_media and we'll overwrite it back to the same value.
        self.issue_challenge(chat_id, user_id).await
    }

    /// Returns `Some(telegram_message_id_or_none)` if there is a *live* (not
    /// expired) captcha row for `(chat_id, user_id)`, `None` otherwise. The
    /// outer `Option` distinguishes "no live challenge" from "live challenge
    /// without a recorded message id yet" — both shapes the message-gate
    /// branches on.
    pub async fn active_challenge_message_id(
        &self,
        chat_id: i64,
        user_id: i64,
    ) -> Result<Option<Option<i32>>> {
        let row = sqlx::query!(
            r#"
            SELECT telegram_message_id
            FROM captcha_challenges
            WHERE chat_id = $1 AND user_id = $2 AND expires_at >= NOW()
            "#,
            chat_id,
            user_id,
        )
        .fetch_optional(&self.pool)
        .await
        .context("SELECT captcha_challenges (active)")?;
        Ok(row.map(|r| r.telegram_message_id))
    }

    /// Persist the Telegram message id of the captcha photo so the expiry job
    /// can later `delete_message`. Best-effort: a missing row means the user
    /// solved or the row was swept between send_photo and this call —
    /// log + return.
    pub async fn record_message_id(
        &self,
        chat_id: i64,
        user_id: i64,
        message_id: i32,
    ) -> Result<()> {
        sqlx::query!(
            r#"
            UPDATE captcha_challenges
            SET telegram_message_id = $3
            WHERE chat_id = $1 AND user_id = $2
            "#,
            chat_id,
            user_id,
            message_id,
        )
        .execute(&self.pool)
        .await
        .context("UPDATE captcha_challenges.telegram_message_id")?;
        Ok(())
    }

    /// Process a candidate solution. All transitions happen in one transaction
    /// guarded by `SELECT ... FOR UPDATE` so two concurrent solvers can't both
    /// win.
    pub async fn solve(&self, chat_id: i64, user_id: i64, attempt: &str) -> Result<Outcome> {
        let mut tx = self.pool.begin().await.context("begin solve tx")?;

        let row = sqlx::query!(
            r#"
            SELECT id, solution, attempts_left, expires_at, telegram_message_id
            FROM captcha_challenges
            WHERE chat_id = $1 AND user_id = $2
            FOR UPDATE
            "#,
            chat_id,
            user_id,
        )
        .fetch_optional(&mut *tx)
        .await
        .context("SELECT captcha_challenges FOR UPDATE")?;

        let Some(row) = row else {
            // The challenge row may be gone because a parallel solver just
            // committed: re-read `verified_users` inside the same tx so we
            // can distinguish AlreadyVerified from a true NotFound.
            let just_verified: bool = sqlx::query_scalar!(
                r#"SELECT EXISTS(
                    SELECT 1 FROM verified_users WHERE chat_id = $1 AND user_id = $2
                ) AS "exists!""#,
                chat_id,
                user_id,
            )
            .fetch_one(&mut *tx)
            .await?;
            tx.commit().await?;
            return Ok(if just_verified {
                Outcome::AlreadyVerified
            } else {
                Outcome::NotFound
            });
        };

        if row.expires_at < Utc::now() {
            // Clean up inside the tx so the expiry job's sweep doesn't pick
            // this row up later and produce a duplicate ledger row. The user
            // is NOT kicked — M1's policy is "delete the message, show captcha
            // again on next message". `ON CONFLICT DO NOTHING` keeps the ledger
            // idempotent if the sweep races us.
            sqlx::query!(
                r#"DELETE FROM captcha_challenges WHERE chat_id = $1 AND user_id = $2"#,
                chat_id,
                user_id,
            )
            .execute(&mut *tx)
            .await?;
            sqlx::query!(
                r#"
                INSERT INTO moderation_actions
                    (chat_id, target_user_id, action, actor_kind, message_id, reason)
                VALUES ($1, $2, 'captcha_expired', 'bot', $3, 'lifetime')
                ON CONFLICT DO NOTHING
                "#,
                chat_id,
                user_id,
                row.telegram_message_id,
            )
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
            return Ok(Outcome::Expired);
        }

        if row.solution == attempt {
            sqlx::query!(
                r#"DELETE FROM captcha_challenges WHERE chat_id = $1 AND user_id = $2"#,
                chat_id,
                user_id,
            )
            .execute(&mut *tx)
            .await?;
            sqlx::query!(
                r#"
                INSERT INTO verified_users (chat_id, user_id)
                VALUES ($1, $2)
                ON CONFLICT DO NOTHING
                "#,
                chat_id,
                user_id,
            )
            .execute(&mut *tx)
            .await?;
            sqlx::query!(
                r#"
                INSERT INTO moderation_actions
                    (chat_id, target_user_id, action, actor_kind, message_id)
                VALUES ($1, $2, 'verify', 'bot', $3)
                ON CONFLICT DO NOTHING
                "#,
                chat_id,
                user_id,
                row.telegram_message_id,
            )
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
            return Ok(Outcome::Solved);
        }

        let new_attempts = row.attempts_left - 1;
        if new_attempts <= 0 {
            sqlx::query!(
                r#"DELETE FROM captcha_challenges WHERE chat_id = $1 AND user_id = $2"#,
                chat_id,
                user_id,
            )
            .execute(&mut *tx)
            .await?;
            sqlx::query!(
                r#"
                INSERT INTO moderation_actions
                    (chat_id, target_user_id, action, actor_kind, message_id, reason)
                VALUES ($1, $2, 'captcha_failed', 'bot', $3, 'wrong-final')
                ON CONFLICT DO NOTHING
                "#,
                chat_id,
                user_id,
                row.telegram_message_id,
            )
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
            Ok(Outcome::WrongFinal)
        } else {
            sqlx::query!(
                r#"
                UPDATE captcha_challenges
                SET attempts_left = $3
                WHERE chat_id = $1 AND user_id = $2
                "#,
                chat_id,
                user_id,
                new_attempts,
            )
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
            Ok(Outcome::WrongLeft(new_attempts))
        }
    }

    /// Manual verification (used by `/verify`). Idempotent — verifying an
    /// already-verified user is a no-op that returns `AlreadyVerified`.
    pub async fn verify_manual(
        &self,
        chat_id: i64,
        target_user_id: i64,
        actor_user_id: i64,
    ) -> Result<Outcome> {
        let mut tx = self.pool.begin().await.context("begin verify tx")?;

        let already: bool = sqlx::query_scalar!(
            r#"SELECT EXISTS(
                SELECT 1 FROM verified_users WHERE chat_id = $1 AND user_id = $2
            ) AS "exists!""#,
            chat_id,
            target_user_id,
        )
        .fetch_one(&mut *tx)
        .await?;
        if already {
            tx.commit().await?;
            return Ok(Outcome::AlreadyVerified);
        }

        let pending = sqlx::query!(
            r#"
            DELETE FROM captcha_challenges
            WHERE chat_id = $1 AND user_id = $2
            RETURNING telegram_message_id
            "#,
            chat_id,
            target_user_id,
        )
        .fetch_optional(&mut *tx)
        .await?;
        let pending_msg = pending.and_then(|r| r.telegram_message_id);

        sqlx::query!(
            r#"
            INSERT INTO verified_users (chat_id, user_id)
            VALUES ($1, $2)
            ON CONFLICT DO NOTHING
            "#,
            chat_id,
            target_user_id,
        )
        .execute(&mut *tx)
        .await?;

        sqlx::query!(
            r#"
            INSERT INTO moderation_actions
                (chat_id, target_user_id, action, actor_kind, actor_user_id, message_id, reason)
            VALUES ($1, $2, 'verify', 'moderator', $3, $4, 'manual')
            ON CONFLICT DO NOTHING
            "#,
            chat_id,
            target_user_id,
            actor_user_id,
            pending_msg,
        )
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(Outcome::Solved)
    }

    // ── Internal helpers ──────────────────────────────────────────────────

    async fn render(&self, challenge_id: Uuid, solution: &str) -> Result<Vec<u8>> {
        let fonts = self.fonts.clone();
        let solution = solution.to_owned();
        tokio::task::spawn_blocking(move || render_webp(challenge_id, &solution, &fonts))
            .await
            .context("render task join")?
    }

    async fn attempts_for(&self, chat_id: i64) -> Result<i16> {
        let row = sqlx::query_scalar!(
            r#"SELECT captcha_attempts FROM chat_config WHERE chat_id = $1"#,
            chat_id,
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.unwrap_or(DEFAULT_ATTEMPTS))
    }

    /// Per-chat captcha lifetime (seconds). Public because the callback handler
    /// uses it to compute the Redis TTL for the in-progress input buffer / meta
    /// row at issuance time so the ephemeral state expires alongside the
    /// challenge row in PG.
    pub async fn lifetime_for(&self, chat_id: i64) -> Result<i32> {
        let row = sqlx::query_scalar!(
            r#"SELECT captcha_lifetime_secs FROM chat_config WHERE chat_id = $1"#,
            chat_id,
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.unwrap_or(DEFAULT_LIFETIME_SECS))
    }
}

/// Deterministic 4-digit string derived from the challenge UUID. Public so
/// that integration tests can recompute the expected solution from the
/// `challenge_id` returned by [`CaptchaService::issue_challenge`] without
/// having to expose the value through the public `IssuedChallenge` struct.
pub fn solution_for(challenge_id: Uuid) -> String {
    let mut h = xxh3_64(challenge_id.as_bytes());
    let mut s = String::with_capacity(SOLUTION_LEN);
    for _ in 0..SOLUTION_LEN {
        s.push((b'0' + (h % 10) as u8) as char);
        h /= 10;
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn solution_is_four_digits() {
        let s = solution_for(Uuid::from_u128(0xdead_beef));
        assert_eq!(s.len(), SOLUTION_LEN);
        assert!(s.chars().all(|c| c.is_ascii_digit()));
    }

    #[test]
    fn solution_is_deterministic() {
        let id = Uuid::from_u128(42);
        assert_eq!(solution_for(id), solution_for(id));
    }
}
