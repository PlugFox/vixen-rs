//! `chat_member` updates — issue a captcha to every fresh joiner.
//!
//! Order of operations is important: we restrict the user **before** we know
//! if the captcha can be issued, so even if image rendering or send_photo
//! fails the joiner stays muted until the expiry sweep cleans up.

use anyhow::Result;
use teloxide::prelude::*;
use teloxide::types::{
    ChatMemberKind, ChatMemberStatus, ChatMemberUpdated, ChatPermissions, InputFile,
};
use tracing::{info, instrument, warn};

use crate::api::AppState;
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

    // Restrict first; the captcha is best-effort but the silence is mandatory.
    if let Err(e) = bot
        .restrict_chat_member(chat_id, user_id, ChatPermissions::empty())
        .await
    {
        warn!(error = %e, "restrict_chat_member failed (bot likely not admin)");
        return Ok(());
    }

    let issued = match state.captcha.issue_challenge(chat_id.0, uid).await {
        Ok(c) => c,
        Err(e) => {
            warn!(error = ?e, "issue_challenge failed");
            return Ok(());
        }
    };

    let caption = format!(
        "{} please solve the captcha to start chatting.\nAttempts left: {}",
        mention(&event.new_chat_member.user),
        issued.attempts_left,
    );

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
            warn!(error = %e, "send_photo failed; expiry job will lift the restrict");
        }
    }

    Ok(())
}

/// True for transitions Left/Kicked → Member (or Restricted with `is_member`).
/// We deliberately ignore promotions / role changes that keep the user a
/// member already.
fn is_fresh_join(event: &ChatMemberUpdated) -> bool {
    let was_present = matches!(
        event.old_chat_member.kind,
        ChatMemberKind::Member
            | ChatMemberKind::Owner(_)
            | ChatMemberKind::Administrator(_)
            | ChatMemberKind::Restricted(_)
    );
    let is_present_now = matches!(event.new_chat_member.status(), ChatMemberStatus::Member);
    !was_present && is_present_now
}

fn mention(user: &teloxide::types::User) -> String {
    if let Some(username) = &user.username {
        format!("@{}", username)
    } else {
        user.full_name()
    }
}
