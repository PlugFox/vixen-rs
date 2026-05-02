//! Centralised moderation service. Every ban / unban / delete (auto or manual)
//! flows through `apply()`, which writes the `moderation_actions` ledger row
//! inside the same transaction as the bot side-effect. Re-running the same
//! action is a no-op via the `(chat_id, target_user_id, action, message_id)`
//! uniqueness key (plus a behaviour check for id-mode bans where
//! `message_id IS NULL` and the unique constraint doesn't help).
//!
//! See `server/docs/moderation.md`.

use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use moka::future::Cache;
use sqlx::PgPool;
use teloxide::ApiError;
use teloxide::RequestError;
use teloxide::prelude::*;
use teloxide::types::{ChatId, MessageId, UserId};
use tracing::{info, instrument, warn};
use uuid::Uuid;

use crate::models::daily_stats::{self, Metric};
use crate::models::moderation_action::{ActorKind, ModerationActionKind};

const MODERATOR_CACHE_TTL: Duration = Duration::from_secs(5 * 60);
const MODERATOR_CACHE_CAPACITY: u64 = 10_000;

#[derive(Debug, Clone)]
pub enum Action {
    Ban {
        reason: String,
        until: Option<DateTime<Utc>>,
    },
    Unban,
    Delete {
        reason: String,
    },
}

impl Action {
    fn kind(&self) -> ModerationActionKind {
        match self {
            Self::Ban { .. } => ModerationActionKind::Ban,
            Self::Unban => ModerationActionKind::Unban,
            Self::Delete { .. } => ModerationActionKind::Delete,
        }
    }

    fn reason(&self) -> Option<&str> {
        match self {
            Self::Ban { reason, .. } | Self::Delete { reason } => Some(reason.as_str()),
            Self::Unban => None,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ApplyContext {
    pub chat_id: i64,
    pub target_user_id: i64,
    /// Set for message-scoped actions (`Delete`, reply-mode `/ban`); leave as
    /// `None` for id-mode bans/unbans. The service handles NULL idempotency
    /// via a behaviour check (last action wins).
    pub message_id: Option<i32>,
    pub actor_kind: ActorKind,
    /// `Some(user_id)` for moderator-driven actions, `None` for the bot.
    pub actor_user_id: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The action was newly applied. Bot side-effect was attempted; non-fatal
    /// Telegram errors (403/400) still produce `Applied` because the ledger
    /// records the *intent*.
    Applied,
    /// An equivalent ledger row already existed. The bot side-effect was
    /// skipped — this is the whole point of the idempotency contract.
    AlreadyApplied,
}

#[derive(Clone)]
pub struct ModerationService {
    db: PgPool,
    bot: Bot,
    moderator_cache: Cache<(i64, i64), bool>,
}

impl ModerationService {
    pub fn new(db: PgPool, bot: Bot) -> Arc<Self> {
        Arc::new(Self {
            db,
            bot,
            moderator_cache: Cache::builder()
                .max_capacity(MODERATOR_CACHE_CAPACITY)
                .time_to_live(MODERATOR_CACHE_TTL)
                .build(),
        })
    }

    /// Idempotent moderation action. INSERT, bot call, and final commit happen
    /// inside a single transaction so a fatal bot failure rolls back the
    /// ledger row atomically — a concurrent caller cannot observe the row
    /// before commit, so it cannot get a misleading `AlreadyApplied` for
    /// an action that ultimately did not apply.
    ///
    /// **Concurrency:** for id-mode bans/unbans where `message_id` is `NULL`
    /// the unique constraint can't dedup (PG treats NULLs as distinct), so
    /// we `SELECT … FOR UPDATE` the chat row before the behaviour check +
    /// INSERT. That serialises concurrent id-mode actions on the same chat
    /// for the duration of the transaction (including the bot call).
    /// Message-scoped actions skip the chat lock and rely on the unique
    /// constraint, which PG serialises atomically — the row-level lock on
    /// the inserted row blocks a concurrent INSERT until our tx commits or
    /// rolls back.
    ///
    /// Holding a PG connection across the bot call is acceptable here:
    /// `/ban` / `/unban` are infrequent moderator commands, and the spam
    /// pipeline always sets `message_id` (so it never holds the chat lock).
    #[instrument(
        skip(self),
        fields(
            chat_id = ctx.chat_id,
            target_user_id = ctx.target_user_id,
            kind = action.kind().as_db_str(),
        )
    )]
    pub async fn apply(&self, action: Action, ctx: ApplyContext) -> Result<Outcome> {
        let needs_lock =
            ctx.message_id.is_none() && matches!(action, Action::Ban { .. } | Action::Unban);

        let mut tx = self.db.begin().await.context("BEGIN apply tx")?;

        if needs_lock {
            // Lock the chat row to serialise concurrent id-mode actions on the
            // same chat for the lifetime of this transaction.
            sqlx::query("SELECT 1 FROM chats WHERE chat_id = $1 FOR UPDATE")
                .bind(ctx.chat_id)
                .execute(&mut *tx)
                .await
                .context("SELECT FOR UPDATE chats")?;

            let last: Option<String> = sqlx::query_scalar!(
                r#"
                SELECT action
                FROM moderation_actions
                WHERE chat_id = $1 AND target_user_id = $2 AND action IN ('ban', 'unban')
                ORDER BY created_at DESC
                LIMIT 1
                "#,
                ctx.chat_id,
                ctx.target_user_id,
            )
            .fetch_optional(&mut *tx)
            .await
            .context("SELECT last terminal action")?;

            let kind = action.kind();
            let already_in_effect = match (last.as_deref(), kind) {
                (Some(prev), _) => prev == kind.as_db_str(),
                (None, ModerationActionKind::Unban) => true,
                (None, _) => false,
            };
            if already_in_effect {
                tx.commit().await.context("COMMIT apply tx (no-op)")?;
                info!("id-mode action already in effect, skipping");
                return Ok(Outcome::AlreadyApplied);
            }
        }

        let inserted_id: Option<Uuid> = sqlx::query_scalar!(
            r#"
            INSERT INTO moderation_actions
                (chat_id, target_user_id, action, actor_kind, actor_user_id, message_id, reason)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (chat_id, target_user_id, action, message_id) DO NOTHING
            RETURNING id
            "#,
            ctx.chat_id,
            ctx.target_user_id,
            action.kind().as_db_str(),
            ctx.actor_kind.as_db_str(),
            ctx.actor_user_id,
            ctx.message_id,
            action.reason(),
        )
        .fetch_optional(&mut *tx)
        .await
        .context("INSERT moderation_actions")?;

        let Some(id) = inserted_id else {
            tx.commit().await.context("COMMIT apply tx (no-op)")?;
            info!("ledger row already exists, skipping bot call");
            return Ok(Outcome::AlreadyApplied);
        };

        match self.dispatch(&action, &ctx).await {
            Ok(()) => {
                tx.commit().await.context("COMMIT apply tx")?;
                info!(action_id = %id, "moderation applied");
                self.bump_daily_stats(action.kind(), ctx.chat_id).await;
                Ok(Outcome::Applied)
            }
            Err(BotCallOutcome::NonFatal(e)) => {
                tx.commit()
                    .await
                    .context("COMMIT apply tx (non-fatal bot error)")?;
                warn!(error = %e, "bot call non-fatal; ledger row kept");
                self.bump_daily_stats(action.kind(), ctx.chat_id).await;
                Ok(Outcome::Applied)
            }
            Err(BotCallOutcome::Fatal(e)) => {
                // Tx drops here without commit → rolls back automatically.
                // Concurrent callers never observed the row, so they will
                // not get a misleading AlreadyApplied for an action that
                // did not stick. A retry can succeed.
                Err(e).context("bot API call failed")
            }
        }
    }

    /// Membership in `chat_moderators` (Moka 5min cache). Chat admins are
    /// gated separately via the existing M1 admin cache (`CaptchaState`).
    pub async fn is_moderator(&self, chat_id: i64, user_id: i64) -> Result<bool> {
        if let Some(cached) = self.moderator_cache.get(&(chat_id, user_id)).await {
            return Ok(cached);
        }
        let row = sqlx::query_scalar!(
            r#"
            SELECT EXISTS(
                SELECT 1 FROM chat_moderators
                WHERE chat_id = $1 AND user_id = $2
            ) AS "exists!"
            "#,
            chat_id,
            user_id,
        )
        .fetch_one(&self.db)
        .await
        .context("SELECT chat_moderators")?;
        self.moderator_cache.insert((chat_id, user_id), row).await;
        Ok(row)
    }

    /// Invalidate a single (chat, user) entry — call this after writing to
    /// `chat_moderators` from elsewhere so the cache doesn't go stale.
    pub async fn invalidate_moderator(&self, chat_id: i64, user_id: i64) {
        self.moderator_cache.invalidate(&(chat_id, user_id)).await;
    }

    /// Mirror the moderation action into the M3 daily-stats counters. Best-
    /// effort: failure here is logged but never bubbled up — the action is
    /// already committed and we don't want to surface a misleading error
    /// just because the counters are unreachable.
    async fn bump_daily_stats(&self, kind: ModerationActionKind, chat_id: i64) {
        let metric = match kind {
            ModerationActionKind::Ban => Metric::UsersBanned,
            ModerationActionKind::Delete => Metric::MessagesDeleted,
            ModerationActionKind::Verify => Metric::UsersVerified,
            // Unban / Mute / Unmute / Unverify / Captcha* / Kick are not
            // surfaced in the daily report; skip the counter rather than
            // adding a metric the aggregator never reads.
            _ => return,
        };
        if let Err(e) = daily_stats::increment(&self.db, chat_id, metric, 1).await {
            warn!(error = ?e, "daily_stats bump failed");
        }
    }

    async fn dispatch(
        &self,
        action: &Action,
        ctx: &ApplyContext,
    ) -> std::result::Result<(), BotCallOutcome> {
        let chat = ChatId(ctx.chat_id);
        let user = UserId(ctx.target_user_id as u64);

        let result = match action {
            Action::Ban { until, .. } => {
                let mut req = self.bot.ban_chat_member(chat, user);
                if let Some(t) = until {
                    req = req.until_date(*t);
                }
                req.await.map(|_| ())
            }
            Action::Unban => self.bot.unban_chat_member(chat, user).await.map(|_| ()),
            Action::Delete { .. } => {
                let Some(mid) = ctx.message_id else {
                    return Err(BotCallOutcome::Fatal(anyhow::anyhow!(
                        "Delete action requires message_id"
                    )));
                };
                self.bot
                    .delete_message(chat, MessageId(mid))
                    .await
                    .map(|_| ())
            }
        };

        match result {
            Ok(()) => Ok(()),
            Err(e) if is_non_fatal(&e) => Err(BotCallOutcome::NonFatal(e)),
            Err(e) => Err(BotCallOutcome::Fatal(anyhow::Error::from(e))),
        }
    }
}

enum BotCallOutcome {
    /// Telegram returned 4xx that we treat as "intent recorded, no-op". Most
    /// common: bot not admin, user not in chat, message already deleted.
    NonFatal(RequestError),
    Fatal(anyhow::Error),
}

fn is_non_fatal(e: &RequestError) -> bool {
    use ApiError::*;
    matches!(
        e,
        RequestError::Api(
            BotKicked
                | BotKickedFromSupergroup
                | UserNotFound
                | ChatNotFound
                | NotEnoughRightsToRestrict
                | MessageToDeleteNotFound
                | MessageCantBeDeleted
                | MessageIdInvalid
        )
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_kind_mapping() {
        let ban = Action::Ban {
            reason: "x".into(),
            until: None,
        };
        let unban = Action::Unban;
        let del = Action::Delete { reason: "y".into() };
        assert_eq!(ban.kind(), ModerationActionKind::Ban);
        assert_eq!(unban.kind(), ModerationActionKind::Unban);
        assert_eq!(del.kind(), ModerationActionKind::Delete);
        assert_eq!(ban.reason(), Some("x"));
        assert_eq!(unban.reason(), None);
        assert_eq!(del.reason(), Some("y"));
    }
}
