//! `/info` slash command — quick moderation-history reference for a
//! target user in the current chat. Moderator/admin-gated, same
//! authorisation gate as `/ban` / `/verify`.
//!
//! Target resolution mirrors `/verify` / `/ban`:
//!  - Reply-mode: `/info` replied to the target user.
//!  - Id-mode:    `/info <user_id>` — numeric Telegram user id only.
//!
//! `@username` is NOT accepted: Telegram doesn't expose a stable
//! username→id resolver to bots, so the lookup would be unreliable.
//! Moderators copy the numeric id from the dashboard's audit log or
//! verified/banned tabs.
//!
//! Output is MarkdownV2-formatted:
//!
//! ```text
//! Info for user <id>:
//! - Verified: yes/no (verified_at)
//! - Bans: N (last <when>)
//! - Unbans: N
//! - Captcha failed: N
//! - Recent (last 5):
//!     - <when> <action> by <actor>: <reason>
//! ```
//!
//! Scope is single-chat by design: the moderator's authority is per-chat, so
//! the reply respects the same boundary.

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use teloxide::prelude::*;
use teloxide::types::ParseMode;
use teloxide::utils::markdown::escape;
use tracing::{info, instrument, warn};

use crate::api::AppState;

/// Shared by `/ban`, `/unban`, `/verify`, `/info`. Mirrors the helper in
/// `commands.rs::resolve_target` but re-declared here to avoid pulling that
/// private fn into a sibling module.
fn resolve_target(msg: &Message, arg: &str) -> Option<i64> {
    if !arg.is_empty() {
        return arg.parse::<i64>().ok().filter(|id| *id > 0);
    }
    let reply = msg.reply_to_message()?;
    Some(reply.from.as_ref()?.id.0 as i64)
}

#[instrument(skip(bot, msg, state), fields(chat_id = msg.chat.id.0))]
pub async fn info(
    bot: Bot,
    msg: Message,
    state: AppState,
    arg: &str,
    is_authorized: bool,
) -> Result<()> {
    if !is_authorized {
        let _ = bot
            .send_message(msg.chat.id, "Only chat moderators or admins can run /info.")
            .await;
        return Ok(());
    }

    let target_user_id = match resolve_target(&msg, arg) {
        Some(id) => id,
        None => {
            let _ = bot
                .send_message(
                    msg.chat.id,
                    "Reply to a user or pass /info <user_id> \
                     (numeric Telegram id — @username is not supported).",
                )
                .await;
            return Ok(());
        }
    };

    let chat_id = msg.chat.id.0;
    let counts = fetch_counts(state.db.pool(), chat_id, target_user_id)
        .await
        .context("fetch /info counts")?;
    let recent = fetch_recent(state.db.pool(), chat_id, target_user_id)
        .await
        .context("fetch /info recent actions")?;

    let body = render(target_user_id, &counts, &recent);

    if let Err(e) = bot
        .send_message(msg.chat.id, body)
        .parse_mode(ParseMode::MarkdownV2)
        .await
    {
        warn!(error = %e, "failed to send /info reply");
    }

    info!(target_user_id, "/info delivered");
    Ok(())
}

#[derive(Debug)]
pub struct InfoCounts {
    pub verified_at: Option<DateTime<Utc>>,
    pub bans: i64,
    pub unbans: i64,
    pub captcha_failed: i64,
    pub captcha_expired: i64,
}

#[derive(Debug)]
pub struct RecentAction {
    pub action: String,
    pub actor_kind: String,
    pub actor_user_id: Option<i64>,
    pub reason: Option<String>,
    pub created_at: DateTime<Utc>,
}

async fn fetch_counts(pool: &sqlx::PgPool, chat_id: i64, user_id: i64) -> Result<InfoCounts> {
    let row = sqlx::query!(
        r#"
        SELECT
            (SELECT verified_at FROM verified_users
                WHERE chat_id = $1 AND user_id = $2)              AS "verified_at?",
            (SELECT COUNT(*) FROM moderation_actions
                WHERE chat_id = $1 AND target_user_id = $2
                  AND action = 'ban')                             AS "bans!",
            (SELECT COUNT(*) FROM moderation_actions
                WHERE chat_id = $1 AND target_user_id = $2
                  AND action = 'unban')                           AS "unbans!",
            (SELECT COUNT(*) FROM moderation_actions
                WHERE chat_id = $1 AND target_user_id = $2
                  AND action = 'captcha_failed')                  AS "captcha_failed!",
            (SELECT COUNT(*) FROM moderation_actions
                WHERE chat_id = $1 AND target_user_id = $2
                  AND action = 'captcha_expired')                 AS "captcha_expired!"
        "#,
        chat_id,
        user_id,
    )
    .fetch_one(pool)
    .await?;
    Ok(InfoCounts {
        verified_at: row.verified_at,
        bans: row.bans,
        unbans: row.unbans,
        captcha_failed: row.captcha_failed,
        captcha_expired: row.captcha_expired,
    })
}

async fn fetch_recent(
    pool: &sqlx::PgPool,
    chat_id: i64,
    user_id: i64,
) -> Result<Vec<RecentAction>> {
    let rows = sqlx::query!(
        r#"
        SELECT action, actor_kind, actor_user_id, reason, created_at
        FROM moderation_actions
        WHERE chat_id = $1 AND target_user_id = $2
        ORDER BY created_at DESC, id DESC
        LIMIT 5
        "#,
        chat_id,
        user_id,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| RecentAction {
            action: r.action,
            actor_kind: r.actor_kind,
            actor_user_id: r.actor_user_id,
            reason: r.reason,
            created_at: r.created_at,
        })
        .collect())
}

pub fn render(target_user_id: i64, counts: &InfoCounts, recent: &[RecentAction]) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "*Info for user* `{}`:\n",
        escape(&target_user_id.to_string())
    ));
    let verified_line = match counts.verified_at {
        Some(at) => format!(
            "• Verified: yes \\({}\\)",
            escape(&at.format("%Y\\-%m\\-%d %H:%M UTC").to_string())
        ),
        None => "• Verified: no".to_string(),
    };
    out.push_str(&format!("{verified_line}\n"));
    out.push_str(&format!("• Bans: {}\n", counts.bans));
    out.push_str(&format!("• Unbans: {}\n", counts.unbans));
    out.push_str(&format!("• Captcha failed: {}\n", counts.captcha_failed));
    out.push_str(&format!("• Captcha expired: {}\n", counts.captcha_expired));

    if recent.is_empty() {
        out.push_str("\n*Recent:* none");
    } else {
        out.push_str("\n*Recent \\(last 5\\):*");
        for r in recent {
            let actor = match (r.actor_kind.as_str(), r.actor_user_id) {
                ("bot", _) => "bot".to_string(),
                ("moderator", Some(id)) => format!("mod {id}"),
                ("moderator", None) => "moderator".to_string(),
                (other, _) => other.to_string(),
            };
            let reason = r
                .reason
                .as_deref()
                .map(|s| format!(": {}", escape(s)))
                .unwrap_or_default();
            out.push_str(&format!(
                "\n• `{}` {} by {}{}",
                escape(&r.created_at.format("%Y-%m-%d %H:%M").to_string()),
                escape(&r.action),
                escape(&actor),
                reason,
            ));
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_verified_yes() {
        let counts = InfoCounts {
            verified_at: Some(Utc::now()),
            bans: 0,
            unbans: 0,
            captcha_failed: 0,
            captcha_expired: 0,
        };
        let out = render(1234, &counts, &[]);
        assert!(out.contains("Verified: yes"));
        assert!(out.contains("Bans: 0"));
        assert!(out.contains("Recent:* none"));
    }

    #[test]
    fn renders_with_recent_history() {
        let counts = InfoCounts {
            verified_at: None,
            bans: 2,
            unbans: 1,
            captcha_failed: 3,
            captcha_expired: 0,
        };
        let recent = vec![
            RecentAction {
                action: "ban".into(),
                actor_kind: "moderator".into(),
                actor_user_id: Some(42),
                reason: Some("spam".into()),
                created_at: Utc::now(),
            },
            RecentAction {
                action: "captcha_failed".into(),
                actor_kind: "bot".into(),
                actor_user_id: None,
                reason: None,
                created_at: Utc::now(),
            },
        ];
        let out = render(1234, &counts, &recent);
        assert!(out.contains("Bans: 2"));
        assert!(out.contains("ban by mod 42"));
        assert!(out.contains("captcha"));
    }
}
