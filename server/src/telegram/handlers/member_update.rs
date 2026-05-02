//! `chat_member` updates — issue a captcha to every fresh joiner.
//!
//! M1 policy: we do **not** restrict or kick the user. The captcha is purely
//! a gate — if they fail or ignore it, their messages keep getting deleted by
//! `message_gate` until they pass. So this handler only sends the photo and
//! anchors the Redis meta; no `restrict_chat_member` call.

use anyhow::Result;
use teloxide::prelude::*;
use teloxide::types::{ChatMemberKind, ChatMemberUpdated, InputFile};
use tracing::{info, instrument, warn};

use crate::api::AppState;
use crate::services::captcha::caption::caption_initial;
use crate::services::captcha::short_id;

#[instrument(
    skip(bot, event, state),
    fields(
        chat_id = event.chat.id.0,
        user_id = event.new_chat_member.user.id.0,
    )
)]
pub async fn handle(bot: Bot, event: ChatMemberUpdated, state: AppState) -> Result<()> {
    if !is_fresh_join(&event) {
        return Ok(());
    }
    if matches!(
        event.new_chat_member.kind,
        ChatMemberKind::Owner(_) | ChatMemberKind::Administrator(_)
    ) {
        info!("admin/owner join, skipping captcha");
        return Ok(());
    }

    let chat_id = event.chat.id;
    let user_id = event.new_chat_member.user.id;
    let uid = user_id.0 as i64;

    // Cache-then-PG: returning users hit Redis and skip a PG round-trip on
    // every join event. Cache is best-effort — a Redis miss / error falls
    // through to the authoritative PG check.
    if state
        .captcha_state
        .is_verified_cached(chat_id.0, uid)
        .await
        .unwrap_or(false)
    {
        info!("user already verified (cache hit), skipping captcha");
        return Ok(());
    }
    if state.captcha.is_verified(chat_id.0, uid).await? {
        let _ = state.captcha_state.mark_verified(chat_id.0, uid).await;
        info!("user already verified (cache populated), skipping captcha");
        return Ok(());
    }

    let issued = match state.captcha.issue_challenge(chat_id.0, uid).await {
        Ok(c) => c,
        Err(e) => {
            warn!(error = ?e, "issue_challenge failed");
            return Ok(());
        }
    };

    let caption = caption_initial(&mention(&event.new_chat_member.user), issued.attempts_left);

    let photo = InputFile::memory(issued.image_webp).file_name("captcha.webp");
    let send_result = bot
        .send_photo(chat_id, photo)
        .caption(caption)
        .reply_markup(issued.keyboard)
        .await;

    match send_result {
        Ok(msg) => {
            if let Err(e) = state
                .captcha
                .record_message_id(chat_id.0, uid, msg.id.0)
                .await
            {
                warn!(error = ?e, "record_message_id failed");
            }
            // Anchor the callback ownership check to this specific message.
            // Lifetime is the same value used to bound the PG row's expires_at,
            // so meta + input both vanish from Redis when the challenge dies.
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
                .set_meta(chat_id.0, msg.id.0, uid, &short, lifetime)
                .await
            {
                warn!(error = ?e, "redis set_meta failed");
            }
        }
        Err(e) => {
            warn!(
                error = %e,
                "send_photo failed; user remains unverified — message_gate will issue a fresh captcha on their next message"
            );
        }
    }

    Ok(())
}

/// True for transitions Left/Kicked → present-in-chat. "Present" includes
/// `Restricted { is_member: true }` because chats with default-restricted
/// permissions deliver fresh joins in that state — without this branch the
/// captcha never fires for those chats. Promotions / role changes (already a
/// member) are deliberately skipped.
fn is_fresh_join(event: &ChatMemberUpdated) -> bool {
    !is_present_in_chat(&event.old_chat_member.kind)
        && is_present_in_chat(&event.new_chat_member.kind)
}

fn is_present_in_chat(kind: &ChatMemberKind) -> bool {
    match kind {
        ChatMemberKind::Member | ChatMemberKind::Owner(_) | ChatMemberKind::Administrator(_) => {
            true
        }
        ChatMemberKind::Restricted(r) => r.is_member,
        ChatMemberKind::Left | ChatMemberKind::Banned(_) => false,
    }
}

fn mention(user: &teloxide::types::User) -> String {
    if let Some(username) = &user.username {
        format!("@{username}")
    } else {
        user.full_name()
    }
}
