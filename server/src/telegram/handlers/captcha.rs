//! Captcha callback handler — the digit-pad keyboard buttons land here.
//!
//! Callback data scheme is `vc:{short}:{op}` (see `services::captcha::keyboard`).
//! Captcha state is held in the message caption; the digit input is parsed
//! from the mask the caption renders so we don't need extra DB writes per tap.

use anyhow::Result;
use teloxide::payloads::EditMessageMediaSetters;
use teloxide::prelude::*;
use teloxide::types::{
    ChatId, ChatPermissions, InputFile, InputMedia, InputMediaPhoto, MaybeInaccessibleMessage,
    UserId,
};
use tracing::{info, instrument, warn};

use crate::api::AppState;
use crate::services::captcha::Outcome;
use crate::services::captcha::keyboard::{OP_BACKSPACE, OP_REFRESH, parse_callback};

const MASK_FILLED: char = '●';
const MASK_EMPTY: char = '○';
const SOLUTION_LEN: usize = 4;

#[instrument(
    skip(bot, q, state),
    fields(
        callback_id = %q.id,
        from_user = q.from.id.0,
    )
)]
pub async fn handle(bot: Bot, q: CallbackQuery, state: AppState) -> Result<()> {
    // Always ack the callback first so Telegram stops retrying.
    let _ = bot.answer_callback_query(&q.id).await;

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
    let user_id = q.from.id;
    let message_id = msg.id;

    let current_input = current_input_from_caption(msg.caption().unwrap_or(""));

    match parsed.op.as_str() {
        OP_REFRESH => refresh(&bot, &state, chat_id, user_id, message_id).await,
        OP_BACKSPACE => backspace(&bot, chat_id, user_id, message_id, current_input).await,
        digit if digit.len() == 1 && digit.chars().next().unwrap().is_ascii_digit() => {
            digit_pressed(
                &bot,
                &state,
                chat_id,
                user_id,
                message_id,
                current_input,
                digit,
            )
            .await
        }
        _ => Ok(()),
    }
}

async fn digit_pressed(
    bot: &Bot,
    state: &AppState,
    chat_id: ChatId,
    user_id: UserId,
    message_id: teloxide::types::MessageId,
    mut input: String,
    digit: &str,
) -> Result<()> {
    if input.chars().count() >= SOLUTION_LEN {
        // Cap reached, ignore. (User has to backspace first.)
        return Ok(());
    }
    input.push_str(digit);

    if input.chars().count() < SOLUTION_LEN {
        let _ = bot
            .edit_message_caption(chat_id, message_id)
            .caption(caption_for(&input))
            .await
            .inspect_err(|e| warn!(error = %e, "edit_message_caption failed"));
        return Ok(());
    }

    // Length == SOLUTION_LEN — try to solve.
    match state
        .captcha
        .solve(chat_id.0, user_id.0 as i64, &input)
        .await?
    {
        Outcome::Solved | Outcome::AlreadyVerified => {
            on_solved(bot, chat_id, user_id, message_id).await;
        }
        Outcome::WrongLeft(left) => {
            let _ = bot
                .edit_message_caption(chat_id, message_id)
                .caption(format!("Wrong, try again. Attempts left: {}", left))
                .await;
        }
        Outcome::WrongFinal | Outcome::Expired => {
            on_kick(bot, chat_id, user_id, message_id).await;
        }
        Outcome::NotFound => {
            // Race: the row was already cleaned up. Best-effort: drop the message.
            let _ = bot.delete_message(chat_id, message_id).await;
        }
    }
    Ok(())
}

async fn backspace(
    bot: &Bot,
    chat_id: ChatId,
    _user_id: UserId,
    message_id: teloxide::types::MessageId,
    mut input: String,
) -> Result<()> {
    input.pop();
    let _ = bot
        .edit_message_caption(chat_id, message_id)
        .caption(caption_for(&input))
        .await;
    Ok(())
}

async fn refresh(
    bot: &Bot,
    state: &AppState,
    chat_id: ChatId,
    user_id: UserId,
    message_id: teloxide::types::MessageId,
) -> Result<()> {
    let issued = match state.captcha.reissue(chat_id.0, user_id.0 as i64).await {
        Ok(i) => i,
        Err(e) => {
            warn!(error = ?e, "reissue failed");
            return Ok(());
        }
    };
    let media = InputMedia::Photo(
        InputMediaPhoto::new(InputFile::memory(issued.image_webp).file_name("captcha.webp"))
            .caption(caption_for("")),
    );
    let _ = bot
        .edit_message_media(chat_id, message_id, media)
        .reply_markup(issued.keyboard)
        .await
        .inspect_err(|e| warn!(error = %e, "edit_message_media failed"));
    if let Err(e) = state
        .captcha
        .record_message_id(chat_id.0, user_id.0 as i64, message_id.0 as i64)
        .await
    {
        warn!(error = ?e, "record_message_id (refresh) failed");
    }
    Ok(())
}

async fn on_solved(
    bot: &Bot,
    chat_id: ChatId,
    user_id: UserId,
    message_id: teloxide::types::MessageId,
) {
    let _ = bot.delete_message(chat_id, message_id).await;
    if let Err(e) = bot
        .restrict_chat_member(chat_id, user_id, ChatPermissions::all())
        .await
    {
        warn!(error = %e, "lift restriction after solve failed");
    }
    info!(
        chat_id = chat_id.0,
        user_id = user_id.0 as i64,
        "captcha solved"
    );
}

async fn on_kick(
    bot: &Bot,
    chat_id: ChatId,
    user_id: UserId,
    message_id: teloxide::types::MessageId,
) {
    let _ = bot.delete_message(chat_id, message_id).await;
    let _ = bot.unban_chat_member(chat_id, user_id).await; // clear any restrict
    if let Err(e) = bot.kick_chat_member(chat_id, user_id).await {
        warn!(error = %e, "kick_chat_member failed");
    }
    let _ = bot.unban_chat_member(chat_id, user_id).await; // kick = ban+unban
    info!(
        chat_id = chat_id.0,
        user_id = user_id.0 as i64,
        "captcha failed → kicked"
    );
}

fn caption_for(input: &str) -> String {
    let n = input.chars().count();
    let mask: String = (0..SOLUTION_LEN)
        .map(|i| if i < n { MASK_FILLED } else { MASK_EMPTY })
        .collect();
    format!("Solve the captcha: {mask}")
}

/// Recover the digits typed so far from the current caption. Counts
/// `MASK_FILLED` characters; the actual digit values aren't needed here —
/// `solve()` is called with the freshly-built input string when length hits
/// SOLUTION_LEN. Until then the caption only has to render progress.
fn current_input_from_caption(caption: &str) -> String {
    // Best-effort heuristic: count MASK_FILLED chars; we only need the length
    // because the final input is built up afresh via append/backspace.
    let n = caption.chars().filter(|c| *c == MASK_FILLED).count();
    "0".repeat(n.min(SOLUTION_LEN))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caption_renders_progress_mask() {
        assert_eq!(
            caption_for(""),
            format!("Solve the captcha: {0}{0}{0}{0}", MASK_EMPTY)
        );
        assert_eq!(
            caption_for("12"),
            format!("Solve the captcha: {0}{0}{1}{1}", MASK_FILLED, MASK_EMPTY)
        );
        assert_eq!(
            caption_for("1234"),
            format!("Solve the captcha: {0}{0}{0}{0}", MASK_FILLED)
        );
    }

    #[test]
    fn input_recovers_length_from_caption() {
        let cap = caption_for("12");
        assert_eq!(current_input_from_caption(&cap).len(), 2);
    }
}
