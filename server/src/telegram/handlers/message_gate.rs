//! Message gate — for **non-command** messages.
//!
//! Two paths share this endpoint:
//!
//! - **Verified users** → run the M2 spam pipeline (`spam.inspect()`) and
//!   dispatch any non-`Allow` verdict through `moderation.apply()`.
//! - **Unverified non-admin users** → delete + issue captcha (M1 policy:
//!   never restrict or kick; the user always gets another shot).
//!
//! Order of checks is fastest-first:
//!
//! 1. `verified_users` — Redis cache, then PG. Most healthy-chat messages
//!    hit this; on hit we run the spam pipeline and return.
//! 2. Chat admins — Redis cache, fallback to `get_chat_administrators` on
//!    miss with 6h TTL. Admins skip the spam pipeline (they wouldn't post
//!    spam, and the bot can't ban them anyway).
//! 3. Otherwise: delete the message; if no live challenge → issue + send;
//!    if a live challenge already exists → just delete (don't spam the chat
//!    with multiple captcha photos for one user).
//!
//! Slash-command messages don't reach this endpoint — they're routed by the
//! `filter_command::<Command>` branch upstream so unverified users can still
//! call `/help` or `/status`. `/verify`, `/ban`, `/unban` are gated by their
//! own permission checks.

use anyhow::Result;
use teloxide::prelude::*;
use teloxide::types::{ChatId, ChatMemberKind, InputFile};
use tracing::{info, instrument, warn};

use crate::api::AppState;
use crate::models::moderation_action::ActorKind;
use crate::services::captcha::short_id;
use crate::services::moderation_service::{Action, ApplyContext};
use crate::services::spam::service::Verdict;

#[instrument(
    skip(bot, msg, state),
    fields(chat_id = msg.chat.id.0, user_id = msg.from.as_ref().map(|u| u.id.0))
)]
pub async fn handle(bot: Bot, msg: Message, state: AppState) -> Result<()> {
    let Some(user) = msg.from.as_ref() else {
        // Channel posts, anonymous group admin posts, etc. — no per-user gate.
        return Ok(());
    };
    let chat_id = msg.chat.id;
    let user_id = user.id;
    let uid = user_id.0 as i64;

    if is_verified(&state, chat_id.0, uid).await {
        run_spam_pipeline(&state, &msg, chat_id.0, uid).await;
        return Ok(());
    }
    if is_chat_admin(&bot, &state, chat_id.0, uid).await {
        return Ok(());
    }

    // Unverified, non-admin → delete the message. Best-effort: if delete fails
    // (bot not admin, message already gone), the captcha still gets posted so
    // at least the user has a path to verification.
    if let Err(e) = bot.delete_message(chat_id, msg.id).await {
        warn!(error = %e, "delete_message failed (bot likely not admin)");
    }

    match state
        .captcha
        .active_challenge_message_id(chat_id.0, uid)
        .await
    {
        Ok(Some(_)) => {
            // Live challenge already in flight — don't post another photo.
            // The user already has an actionable keyboard somewhere above.
            info!("active challenge already exists, skipping reissue");
        }
        Ok(None) => {
            issue_and_post(&bot, &state, chat_id, user_id, uid, user).await;
        }
        Err(e) => {
            warn!(error = ?e, "active_challenge_message_id failed; attempting reissue anyway");
            issue_and_post(&bot, &state, chat_id, user_id, uid, user).await;
        }
    }

    Ok(())
}

async fn issue_and_post(
    bot: &Bot,
    state: &AppState,
    chat_id: ChatId,
    user_id: teloxide::types::UserId,
    uid: i64,
    user: &teloxide::types::User,
) {
    let issued = match state.captcha.issue_challenge(chat_id.0, uid).await {
        Ok(c) => c,
        Err(e) => {
            warn!(error = ?e, "issue_challenge failed");
            return;
        }
    };

    let caption = format!(
        "{} please solve the captcha to start chatting.\nAttempts left: {}",
        mention(user),
        issued.attempts_left,
    );
    let photo = InputFile::memory(issued.image_webp).file_name("captcha.webp");
    let sent = match bot
        .send_photo(chat_id, photo)
        .caption(caption)
        .reply_markup(issued.keyboard)
        .await
    {
        Ok(m) => m,
        Err(e) => {
            warn!(error = %e, "send_photo failed");
            return;
        }
    };

    if let Err(e) = state
        .captcha
        .record_message_id(chat_id.0, uid, sent.id.0)
        .await
    {
        warn!(error = ?e, "record_message_id failed");
    }
    let lifetime = match state.captcha.lifetime_for(chat_id.0).await {
        Ok(l) => l as u64,
        Err(e) => {
            warn!(error = ?e, "lifetime_for failed; using 60s for meta TTL");
            60
        }
    };
    let short = short_id(issued.challenge_id);
    if let Err(e) = state
        .captcha_state
        .set_meta(chat_id.0, sent.id.0, uid, &short, lifetime)
        .await
    {
        warn!(error = ?e, "redis set_meta failed");
    }
    info!(user_id = user_id.0, "issued captcha via message gate");
}

async fn is_verified(state: &AppState, chat_id: i64, user_id: i64) -> bool {
    if state
        .captcha_state
        .is_verified_cached(chat_id, user_id)
        .await
        .unwrap_or(false)
    {
        return true;
    }
    match state.captcha.is_verified(chat_id, user_id).await {
        Ok(true) => {
            let _ = state.captcha_state.mark_verified(chat_id, user_id).await;
            true
        }
        Ok(false) => false,
        Err(e) => {
            warn!(error = ?e, "is_verified PG check failed; treating as unverified");
            false
        }
    }
}

async fn is_chat_admin(bot: &Bot, state: &AppState, chat_id: i64, user_id: i64) -> bool {
    if let Ok(Some(list)) = state.captcha_state.get_admins(chat_id).await {
        return list.contains(&user_id);
    }
    // Cache miss → fetch + repopulate. A failure here surfaces as "treat as
    // non-admin": better to delete one admin's message and let them solve a
    // captcha than to leave a hole that lets unverified users through.
    match bot.get_chat_administrators(ChatId(chat_id)).await {
        Ok(admins) => {
            let admins_filtered: Vec<_> = admins
                .into_iter()
                .filter(|a| !matches!(a.kind, ChatMemberKind::Banned(_) | ChatMemberKind::Left))
                .collect();
            let ids: Vec<i64> = admins_filtered.iter().map(|a| a.user.id.0 as i64).collect();
            if let Err(e) = state.captcha_state.set_admins(chat_id, &ids).await {
                warn!(error = ?e, "redis set_admins failed");
            }
            ids.contains(&user_id)
        }
        Err(e) => {
            warn!(error = %e, "get_chat_administrators failed; treating user as non-admin");
            false
        }
    }
}

fn mention(user: &teloxide::types::User) -> String {
    if let Some(username) = &user.username {
        format!("@{username}")
    } else {
        user.full_name()
    }
}

/// Run the M2 spam pipeline for a verified, non-admin user. Verdicts other
/// than `Allow` are dispatched through `ModerationService::apply` so the
/// ledger row and the bot side-effect stay paired.
///
/// Errors are swallowed at warn level — spam-detection failure must not
/// block the conversation. The captcha gate (above) is the hard guarantee;
/// the spam pipeline is best-effort defense in depth.
async fn run_spam_pipeline(state: &AppState, msg: &Message, chat_id: i64, user_id: i64) {
    let verdict = match state.spam.inspect(msg).await {
        Ok(v) => v,
        Err(e) => {
            warn!(error = ?e, "spam.inspect failed");
            return;
        }
    };

    let (action, ctx) = match verdict {
        Verdict::Allow => return,
        Verdict::Delete { reason_json } => (
            Action::Delete {
                reason: reason_json.to_string(),
            },
            ApplyContext {
                chat_id,
                target_user_id: user_id,
                message_id: Some(msg.id.0),
                actor_kind: ActorKind::Bot,
                actor_user_id: None,
            },
        ),
        Verdict::Ban { reason_json, until } => (
            Action::Ban {
                reason: reason_json.to_string(),
                until,
            },
            ApplyContext {
                chat_id,
                target_user_id: user_id,
                // Ledger keys the row to the message that triggered the
                // ban — useful for replay/audit and gives the unique
                // constraint a non-NULL value.
                message_id: Some(msg.id.0),
                actor_kind: ActorKind::Bot,
                actor_user_id: None,
            },
        ),
    };

    if let Err(e) = state.moderation.apply(action, ctx).await {
        warn!(error = ?e, "moderation.apply failed (spam pipeline)");
    } else {
        info!(chat_id, user_id, "spam verdict applied");
    }
}
