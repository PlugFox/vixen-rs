//! Slash-command handlers. M1 ships only `/verify` (manual moderator override).
//! `/help` and `/status` are stub replies; M2/M5 will fill them in.

use anyhow::Result;
use teloxide::prelude::*;
use teloxide::types::ChatPermissions;
use tracing::{info, instrument, warn};

use crate::api::AppState;
use crate::services::captcha::Outcome;
use crate::telegram::commands::Command;

#[instrument(skip(bot, msg, state, cmd), fields(chat_id = msg.chat.id.0))]
pub async fn dispatch(bot: Bot, msg: Message, state: AppState, cmd: Command) -> Result<()> {
    match cmd {
        Command::Help => {
            let _ = bot
                .send_message(
                    msg.chat.id,
                    "Vixen anti-spam bot — captcha + spam pipeline. \
                     Commands: /help, /status, /verify (reply or with user_id).",
                )
                .await;
            Ok(())
        }
        Command::Status => {
            let _ = bot
                .send_message(msg.chat.id, "Vixen is watching this chat.")
                .await;
            Ok(())
        }
        Command::Verify(arg) => verify(bot, msg, state, arg.trim()).await,
    }
}

async fn verify(bot: Bot, msg: Message, state: AppState, arg: &str) -> Result<()> {
    let actor = match msg.from.as_ref() {
        Some(u) => u,
        None => {
            return Ok(());
        }
    };

    if !is_moderator(&bot, msg.chat.id, actor.id).await {
        let _ = bot
            .send_message(msg.chat.id, "Only chat administrators can run /verify.")
            .await;
        return Ok(());
    }

    let target_user_id = match resolve_target(&msg, arg) {
        Some(id) => id,
        None => {
            let _ = bot
                .send_message(msg.chat.id, "Reply to a user or pass /verify <user_id>.")
                .await;
            return Ok(());
        }
    };

    let outcome = state
        .captcha
        .verify_manual(msg.chat.id.0, target_user_id, actor.id.0 as i64)
        .await?;

    // Populate the Redis verified cache so the next join skips a PG round-trip.
    // Best-effort: a Redis miss here just means lazy fill on next join.
    if let Err(e) = state
        .captcha_state
        .mark_verified(msg.chat.id.0, target_user_id)
        .await
    {
        warn!(error = ?e, "redis mark_verified (verify_manual) failed");
    }

    // Lift the restriction whether the user was already verified or not — the
    // restrict from the captcha join might still be in effect.
    if let Err(e) = bot
        .restrict_chat_member(
            msg.chat.id,
            teloxide::types::UserId(target_user_id as u64),
            ChatPermissions::all(),
        )
        .await
    {
        warn!(error = %e, "lift restriction in /verify failed");
    }

    let reply = match outcome {
        Outcome::Solved => format!("Verified user {target_user_id}."),
        Outcome::AlreadyVerified => format!("User {target_user_id} was already verified."),
        _ => "Unexpected verify state.".to_string(),
    };
    let _ = bot.send_message(msg.chat.id, reply).await;

    info!(target_user_id, ?outcome, "/verify completed");
    Ok(())
}

fn resolve_target(msg: &Message, arg: &str) -> Option<i64> {
    if !arg.is_empty() {
        return arg.parse::<i64>().ok();
    }
    let reply = msg.reply_to_message()?;
    Some(reply.from.as_ref()?.id.0 as i64)
}

/// True if `user` is a chat administrator (creator or admin) per Telegram.
/// On API failure (rate limit, permission revoked) returns `false` and logs
/// the error — `/verify` will deny the call rather than silently elevate. M2+
/// may add a per-chat moderator allow-list table; not in scope here.
async fn is_moderator(
    bot: &Bot,
    chat_id: teloxide::types::ChatId,
    user_id: teloxide::types::UserId,
) -> bool {
    match bot.get_chat_administrators(chat_id).await {
        Ok(list) => list.iter().any(|a| a.user.id == user_id),
        Err(e) => {
            warn!(error = %e, "get_chat_administrators failed");
            false
        }
    }
}
