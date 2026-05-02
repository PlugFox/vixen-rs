//! Slash-command handlers. `/verify`, `/ban`, `/unban` go through
//! `ModerationService` for ledger + idempotent bot side-effect; `/help` and
//! `/status` are stub replies.

use anyhow::Result;
use teloxide::prelude::*;
use teloxide::types::{ChatId, ChatMemberKind};
use tracing::{info, instrument, warn};

use crate::api::AppState;
use crate::models::moderation_action::ActorKind;
use crate::services::captcha::Outcome;
use crate::services::moderation_service::{Action, ApplyContext, Outcome as ModOutcome};
use crate::telegram::commands::Command;

#[instrument(skip(bot, msg, state, cmd), fields(chat_id = msg.chat.id.0))]
pub async fn dispatch(bot: Bot, msg: Message, state: AppState, cmd: Command) -> Result<()> {
    match cmd {
        Command::Help => {
            let _ = bot
                .send_message(
                    msg.chat.id,
                    "Vixen anti-spam bot — captcha + spam pipeline.\n\
                     /help — this message\n\
                     /status — bot status in this chat\n\
                     /verify (reply or <user_id>) — moderator: manually verify a user\n\
                     /ban (reply or <user_id> [reason]) — moderator: ban a user\n\
                     /unban <user_id> — moderator: lift a ban",
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
        Command::Ban(arg) => ban(bot, msg, state, arg.trim()).await,
        Command::Unban(arg) => unban(bot, msg, state, arg.trim()).await,
    }
}

async fn verify(bot: Bot, msg: Message, state: AppState, arg: &str) -> Result<()> {
    let actor = match msg.from.as_ref() {
        Some(u) => u,
        None => {
            return Ok(());
        }
    };

    if !is_moderator_or_admin(&bot, &state, msg.chat.id, actor).await {
        let _ = bot
            .send_message(
                msg.chat.id,
                "Only chat moderators or admins can run /verify.",
            )
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
    // Telegram user IDs are positive (`u64` on the wire). Reject non-positive
    // arguments here so callers can't accidentally cast a negative `i64` into
    // a giant `u64` user_id when calling Telegram APIs downstream.
    if !arg.is_empty() {
        return arg.parse::<i64>().ok().filter(|id| *id > 0);
    }
    let reply = msg.reply_to_message()?;
    Some(reply.from.as_ref()?.id.0 as i64)
}

/// Permission gate shared by `/verify`, `/ban`, `/unban`: moderator (DB row in
/// `chat_moderators`, Moka 5min cache) **OR** chat admin (existing M1 admin
/// cache, 6h Redis TTL, falls back to a live `getChatAdministrators`).
///
/// On every cache repopulation we filter out `Banned` / `Left` admins — the
/// same rule message_gate uses — so a stale ex-admin id can't sneak into the
/// cache via this path.
async fn is_moderator_or_admin(
    bot: &Bot,
    state: &AppState,
    chat_id: ChatId,
    user: &teloxide::types::User,
) -> bool {
    let uid = user.id.0 as i64;

    // 1. chat_moderators (DB allow-list).
    match state.moderation.is_moderator(chat_id.0, uid).await {
        Ok(true) => return true,
        Ok(false) => {}
        Err(e) => {
            warn!(error = ?e, "moderation.is_moderator failed; falling back to admin check");
        }
    }

    // 2. Existing M1 admin cache → live API.
    if let Ok(Some(admins)) = state.captcha_state.get_admins(chat_id.0).await {
        return admins.contains(&uid);
    }
    match bot.get_chat_administrators(chat_id).await {
        Ok(admins) => {
            let ids: Vec<i64> = admins
                .iter()
                .filter(|a| !matches!(a.kind, ChatMemberKind::Banned(_) | ChatMemberKind::Left))
                .map(|a| a.user.id.0 as i64)
                .collect();
            if let Err(e) = state.captcha_state.set_admins(chat_id.0, &ids).await {
                warn!(error = ?e, "redis set_admins failed");
            }
            ids.contains(&uid)
        }
        Err(e) => {
            warn!(error = %e, "get_chat_administrators failed");
            false
        }
    }
}

async fn ban(bot: Bot, msg: Message, state: AppState, arg: &str) -> Result<()> {
    let Some(actor) = msg.from.as_ref() else {
        return Ok(());
    };

    if !is_moderator_or_admin(&bot, &state, msg.chat.id, actor).await {
        let _ = bot
            .send_message(msg.chat.id, "Only chat moderators or admins can run /ban.")
            .await;
        return Ok(());
    }

    // Resolve target + optional reason. Reply-mode wins when both are present.
    let (target_user_id, message_id, reason) = match parse_ban_target(&msg, arg) {
        Some(t) => t,
        None => {
            let _ = bot
                .send_message(
                    msg.chat.id,
                    "Reply to a user's message or pass /ban <user_id> [reason].",
                )
                .await;
            return Ok(());
        }
    };

    let ctx = ApplyContext {
        chat_id: msg.chat.id.0,
        target_user_id,
        message_id,
        actor_kind: ActorKind::Moderator,
        actor_user_id: Some(actor.id.0 as i64),
    };
    let action = Action::Ban {
        reason: reason.unwrap_or_else(|| "manual ban (no reason)".to_string()),
        until: None,
    };

    match state.moderation.apply(action, ctx).await {
        Ok(ModOutcome::Applied) => {
            info!(target_user_id, "/ban applied");
            // Remove the moderator's command message to keep the chat clean.
            // Best-effort: bot may not be admin, in which case the line stays.
            if let Err(e) = bot.delete_message(msg.chat.id, msg.id).await {
                warn!(error = %e, "delete /ban command message failed");
            }
        }
        Ok(ModOutcome::AlreadyApplied) => {
            let _ = bot
                .send_message(
                    msg.chat.id,
                    format!("User {target_user_id} is already banned."),
                )
                .await;
        }
        Err(e) => {
            warn!(error = ?e, "moderation.apply (Ban) failed");
            let _ = bot
                .send_message(msg.chat.id, "Ban failed; check bot permissions.")
                .await;
        }
    }
    Ok(())
}

async fn unban(bot: Bot, msg: Message, state: AppState, arg: &str) -> Result<()> {
    let Some(actor) = msg.from.as_ref() else {
        return Ok(());
    };

    if !is_moderator_or_admin(&bot, &state, msg.chat.id, actor).await {
        let _ = bot
            .send_message(
                msg.chat.id,
                "Only chat moderators or admins can run /unban.",
            )
            .await;
        return Ok(());
    }

    // /unban is id-only by design — replying to a banned user's old message
    // doesn't help (their messages are deleted on ban) and the moderator
    // already needs the user_id from the dashboard / audit log to find them.
    let target_user_id = match arg.split_whitespace().next() {
        Some(s) => match s.parse::<i64>() {
            Ok(id) if id > 0 => id,
            _ => {
                let _ = bot
                    .send_message(msg.chat.id, "Usage: /unban <user_id>")
                    .await;
                return Ok(());
            }
        },
        None => {
            let _ = bot
                .send_message(msg.chat.id, "Usage: /unban <user_id>")
                .await;
            return Ok(());
        }
    };

    let ctx = ApplyContext {
        chat_id: msg.chat.id.0,
        target_user_id,
        message_id: None,
        actor_kind: ActorKind::Moderator,
        actor_user_id: Some(actor.id.0 as i64),
    };

    match state.moderation.apply(Action::Unban, ctx).await {
        Ok(ModOutcome::Applied) => {
            info!(target_user_id, "/unban applied");
            if let Err(e) = bot.delete_message(msg.chat.id, msg.id).await {
                warn!(error = %e, "delete /unban command message failed");
            }
        }
        Ok(ModOutcome::AlreadyApplied) => {
            let _ = bot
                .send_message(
                    msg.chat.id,
                    format!("User {target_user_id} is not currently banned."),
                )
                .await;
        }
        Err(e) => {
            warn!(error = ?e, "moderation.apply (Unban) failed");
            let _ = bot
                .send_message(msg.chat.id, "Unban failed; check bot permissions.")
                .await;
        }
    }
    Ok(())
}

/// Returns `(target_user_id, message_id_for_ledger, reason)`. Reply-mode
/// wins: if there's a `reply_to_message`, ignore the textual user_id and
/// use the replied-to message. Otherwise parse `<user_id> [rest as reason]`.
fn parse_ban_target(msg: &Message, arg: &str) -> Option<(i64, Option<i32>, Option<String>)> {
    if let Some(reply) = msg.reply_to_message() {
        let target = reply.from.as_ref()?.id.0 as i64;
        let reason = (!arg.is_empty()).then(|| arg.to_string());
        return Some((target, Some(reply.id.0), reason));
    }
    let mut parts = arg.splitn(2, char::is_whitespace);
    let id = parts.next()?.parse::<i64>().ok().filter(|n| *n > 0)?;
    let reason = parts
        .next()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    Some((id, None, reason))
}
