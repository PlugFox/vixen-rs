//! Captcha callback handler — the digit-pad keyboard buttons land here.
//!
//! Callback data scheme is `vc:{short}:{op}` (see `services::captcha::keyboard`).
//! Per-press state lives in Redis via `services::captcha::state::CaptchaState`:
//!
//!   * `cap:input:{chat}:{user}` — the digits typed so far (TTL = challenge lifetime).
//!   * `cap:meta:{chat}:{message}` — owner_user_id + uuid_short + lifetime_secs.
//!
//! The meta row enables an O(1) ownership check: a callback whose presser does
//! not match `meta.owner_user_id` is rejected with a "this isn't your captcha"
//! toast (visible only to the presser) — without it any chat member could wipe
//! a target's challenge by mashing buttons. The meta row is the source of
//! truth for the press's identity, so we look it up *before* acking the
//! callback (so non-owners actually see the toast).

use anyhow::Result;
use teloxide::payloads::EditMessageMediaSetters;
use teloxide::prelude::*;
use teloxide::types::{
    ChatId, InputFile, InputMedia, InputMediaPhoto, MaybeInaccessibleMessage, UserId,
};
use tracing::{info, instrument, warn};

use crate::api::AppState;
use crate::services::captcha::Outcome;
use crate::services::captcha::caption::{caption_progress, caption_wrong};
use crate::services::captcha::keyboard::{
    OP_BACKSPACE, OP_REFRESH, digit_pad_from_short, parse_callback, short_id,
};

const SOLUTION_LEN: usize = 4;

#[instrument(
    skip(bot, q, state),
    fields(
        callback_id = %q.id,
        from_user = q.from.id.0,
    )
)]
pub async fn handle(bot: Bot, q: CallbackQuery, state: AppState) -> Result<()> {
    let Some(data) = q.data.clone() else {
        return Ok(());
    };
    let Some(parsed) = parse_callback(&data) else {
        warn!(data = %data, "malformed callback");
        return Ok(());
    };

    let Some(maybe_msg) = q.message.as_ref() else {
        return Ok(());
    };
    let MaybeInaccessibleMessage::Regular(msg) = maybe_msg else {
        return Ok(());
    };
    let chat_id = msg.chat.id;
    let presser_id = q.from.id;
    let message_id = msg.id;

    // Look up meta BEFORE acking. If the meta is gone (TTL expired or restart
    // wiped Redis), there's nothing meaningful to do; ack and bail.
    let meta = match state.captcha_state.get_meta(chat_id.0, message_id.0).await {
        Ok(Some(m)) => m,
        Ok(None) => {
            let _ = bot.answer_callback_query(&q.id).await;
            return Ok(());
        }
        Err(e) => {
            warn!(error = ?e, "redis get_meta failed");
            let _ = bot.answer_callback_query(&q.id).await;
            return Ok(());
        }
    };

    // Ownership check. A non-owner gets a toast (visible only to them) and we
    // do NOT touch the captcha. Telegram requires answer_callback_query within
    // ~15s; the check itself is two Redis ops + a string compare.
    if (presser_id.0 as i64) != meta.owner_user_id {
        let _ = bot
            .answer_callback_query(&q.id)
            .text("This isn't your captcha.")
            .show_alert(false)
            .await;
        return Ok(());
    }

    // Stale callback from a previous challenge (refresh issued a new uuid →
    // the old keyboard's buttons still carry the old short). Silent drop.
    if parsed.short != meta.uuid_short {
        let _ = bot.answer_callback_query(&q.id).await;
        return Ok(());
    }

    // Owner + matching short. Ack first so Telegram stops retrying.
    let _ = bot.answer_callback_query(&q.id).await;

    let owner_id = meta.owner_user_id;
    let lifetime = meta.lifetime_secs;

    match parsed.op.as_str() {
        OP_REFRESH => refresh(&bot, &state, chat_id, message_id, owner_id).await,
        OP_BACKSPACE => {
            backspace(
                &bot,
                &state,
                chat_id,
                message_id,
                owner_id,
                lifetime,
                &parsed.short,
            )
            .await
        }
        digit if digit.len() == 1 && digit.chars().next().unwrap().is_ascii_digit() => {
            digit_pressed(
                &bot,
                &state,
                chat_id,
                presser_id,
                message_id,
                owner_id,
                lifetime,
                digit,
                &parsed.short,
            )
            .await
        }
        _ => Ok(()),
    }
}

#[allow(clippy::too_many_arguments)]
async fn digit_pressed(
    bot: &Bot,
    state: &AppState,
    chat_id: ChatId,
    presser_id: UserId,
    message_id: teloxide::types::MessageId,
    owner_id: i64,
    lifetime_secs: u64,
    digit: &str,
    short: &str,
) -> Result<()> {
    let mut input = match state.captcha_state.get_input(chat_id.0, owner_id).await {
        Ok(s) => s,
        Err(e) => {
            warn!(error = ?e, "redis get_input failed; treating as empty buffer");
            String::new()
        }
    };

    if input.chars().count() >= SOLUTION_LEN {
        // Cap reached, ignore. (User has to backspace first.)
        return Ok(());
    }
    input.push_str(digit);

    if input.chars().count() < SOLUTION_LEN {
        if let Err(e) = state
            .captcha_state
            .set_input(chat_id.0, owner_id, &input, lifetime_secs)
            .await
        {
            warn!(error = ?e, "redis set_input failed");
        }
        let _ = bot
            .edit_message_caption(chat_id, message_id)
            .caption(caption_progress(&input))
            .reply_markup(digit_pad_from_short(short))
            .await
            .inspect_err(|e| warn!(error = %e, "edit_message_caption failed"));
        return Ok(());
    }

    // Length == SOLUTION_LEN — try to solve.
    match state.captcha.solve(chat_id.0, owner_id, &input).await? {
        Outcome::Solved | Outcome::AlreadyVerified => {
            clear_state(state, chat_id.0, owner_id, message_id.0).await;
            if let Err(e) = state.captcha_state.mark_verified(chat_id.0, owner_id).await {
                warn!(error = ?e, "redis mark_verified failed");
            }
            on_solved(bot, chat_id, presser_id, message_id).await;
        }
        Outcome::WrongLeft(left) => {
            // Reset the input buffer so the user can immediately retry — the
            // challenge row stays alive (attempts_left decremented in PG), but
            // the typed digits are wiped both server-side and in the caption.
            // Lifetime is fixed at issuance and intentionally NOT extended on
            // wrong attempts (otherwise an attacker could farm wrong tries to
            // keep the timer alive forever).
            if let Err(e) = state.captcha_state.clear_input(chat_id.0, owner_id).await {
                warn!(error = ?e, "redis clear_input (WrongLeft) failed");
            }
            let _ = bot
                .edit_message_caption(chat_id, message_id)
                .caption(caption_wrong(left))
                .reply_markup(digit_pad_from_short(short))
                .await;
        }
        Outcome::WrongFinal | Outcome::Expired => {
            clear_state(state, chat_id.0, owner_id, message_id.0).await;
            on_failed(bot, chat_id, presser_id, message_id).await;
        }
        Outcome::NotFound => {
            // Ownership was already verified above, so this is a true vanish:
            // the challenge row was already cleaned up by the expiry job or a
            // parallel solver. Drop the message + scrub Redis.
            clear_state(state, chat_id.0, owner_id, message_id.0).await;
            let _ = bot.delete_message(chat_id, message_id).await;
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn backspace(
    bot: &Bot,
    state: &AppState,
    chat_id: ChatId,
    message_id: teloxide::types::MessageId,
    owner_id: i64,
    lifetime_secs: u64,
    short: &str,
) -> Result<()> {
    let mut input = match state.captcha_state.get_input(chat_id.0, owner_id).await {
        Ok(s) => s,
        Err(e) => {
            warn!(error = ?e, "redis get_input failed; backspace on empty buffer");
            String::new()
        }
    };
    input.pop();
    if let Err(e) = state
        .captcha_state
        .set_input(chat_id.0, owner_id, &input, lifetime_secs)
        .await
    {
        warn!(error = ?e, "redis set_input failed");
    }
    let _ = bot
        .edit_message_caption(chat_id, message_id)
        .caption(caption_progress(&input))
        .reply_markup(digit_pad_from_short(short))
        .await;
    Ok(())
}

async fn refresh(
    bot: &Bot,
    state: &AppState,
    chat_id: ChatId,
    message_id: teloxide::types::MessageId,
    owner_id: i64,
) -> Result<()> {
    // Wipe ephemeral UI state for the old challenge. The new challenge
    // gets its own meta row written below; the old input buffer (if any)
    // would otherwise survive the refresh and pre-fill the new keyboard.
    if let Err(e) = state.captcha_state.clear_input(chat_id.0, owner_id).await {
        warn!(error = ?e, "redis clear_input (refresh) failed");
    }
    if let Err(e) = state
        .captcha_state
        .clear_meta(chat_id.0, message_id.0)
        .await
    {
        warn!(error = ?e, "redis clear_meta (refresh) failed");
    }

    let issued = match state.captcha.reissue(chat_id.0, owner_id).await {
        Ok(i) => i,
        Err(e) => {
            warn!(error = ?e, "reissue failed");
            return Ok(());
        }
    };
    let media = InputMedia::Photo(
        InputMediaPhoto::new(InputFile::memory(issued.image_webp).file_name("captcha.webp"))
            .caption(caption_progress("")),
    );
    let _ = bot
        .edit_message_media(chat_id, message_id, media)
        .reply_markup(issued.keyboard)
        .await
        .inspect_err(|e| warn!(error = %e, "edit_message_media failed"));
    if let Err(e) = state
        .captcha
        .record_message_id(chat_id.0, owner_id, message_id.0)
        .await
    {
        warn!(error = ?e, "record_message_id (refresh) failed");
    }

    // Re-anchor meta to the same message_id with the NEW challenge's short.
    // (Ownership of the new meta == owner of the previous meta — we just
    // verified it in `handle()` before dispatching here.)
    let lifetime = match state.captcha.lifetime_for(chat_id.0).await {
        Ok(l) => l as u64,
        Err(e) => {
            warn!(error = ?e, "lifetime_for (refresh) failed; using 60s");
            60
        }
    };
    let new_short = short_id(issued.challenge_id);
    if let Err(e) = state
        .captcha_state
        .set_meta(chat_id.0, message_id.0, owner_id, &new_short, lifetime)
        .await
    {
        warn!(error = ?e, "redis set_meta (refresh) failed");
    }
    Ok(())
}

/// Best-effort scrub of both Redis keys for a finished interaction.
async fn clear_state(state: &AppState, chat_id: i64, owner_id: i64, message_id: i32) {
    if let Err(e) = state.captcha_state.clear_input(chat_id, owner_id).await {
        warn!(error = ?e, "redis clear_input failed");
    }
    if let Err(e) = state.captcha_state.clear_meta(chat_id, message_id).await {
        warn!(error = ?e, "redis clear_meta failed");
    }
}

async fn on_solved(
    bot: &Bot,
    chat_id: ChatId,
    user_id: UserId,
    message_id: teloxide::types::MessageId,
) {
    let _ = bot.delete_message(chat_id, message_id).await;
    info!(
        chat_id = chat_id.0,
        user_id = user_id.0 as i64,
        "captcha solved"
    );
}

/// Final wrong attempt or expired-during-solve. M1 policy: no kick. We just
/// drop the captcha photo + scrub Redis. The user keeps their membership;
/// their next message will be deleted by `message_gate` and a fresh captcha
/// will be issued.
async fn on_failed(
    bot: &Bot,
    chat_id: ChatId,
    user_id: UserId,
    message_id: teloxide::types::MessageId,
) {
    let _ = bot.delete_message(chat_id, message_id).await;
    info!(
        chat_id = chat_id.0,
        user_id = user_id.0 as i64,
        "captcha failed; row cleared, user retains membership"
    );
}
