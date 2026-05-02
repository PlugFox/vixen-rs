//! teloxide dispatcher with the watched-chats filter.
//!
//! Branches:
//!
//!   * `Update::filter_chat_member()`    — captcha issuance on join
//!   * `Update::filter_callback_query()` — captcha digit-pad solve / refresh
//!   * `Update::filter_message()`        — slash commands; future: spam pipeline
//!
//! The watched-chats filter sits at the trunk so non-watched chats never reach
//! a handler.

use std::collections::HashSet;
use std::sync::Arc;

use teloxide::dispatching::{DefaultKey, Dispatcher};
use teloxide::dptree;
use teloxide::prelude::*;
use teloxide::types::AllowedUpdate;
use tracing::info;

use crate::api::AppState;
use crate::services::captcha::keyboard::CALLBACK_PREFIX;
use crate::telegram::commands::Command;
use crate::telegram::handlers::{
    captcha as captcha_handler, commands as command_handler, member_update,
};

/// Set of chat IDs the bot is allowed to react to. Constructed once at startup
/// from `Config::chats` and cloned cheaply via `Arc`.
#[derive(Clone)]
pub struct WatchedChats(Arc<HashSet<i64>>);

impl WatchedChats {
    pub fn new<I: IntoIterator<Item = i64>>(chats: I) -> Self {
        Self(Arc::new(chats.into_iter().collect()))
    }

    pub fn contains(&self, chat_id: i64) -> bool {
        self.0.contains(&chat_id)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Build a teloxide `Dispatcher` with the M1 handler tree. The dispatcher
/// drives polling itself; the caller stops it via `Dispatcher::shutdown_token()`.
pub fn build_dispatcher(
    bot: Bot,
    watched: WatchedChats,
    state: AppState,
) -> Dispatcher<Bot, anyhow::Error, DefaultKey> {
    let chat_member_branch = Update::filter_chat_member()
        .filter(|event: ChatMemberUpdated, watched: WatchedChats| watched.contains(event.chat.id.0))
        .endpoint(member_update::handle);

    let callback_branch = Update::filter_callback_query()
        .filter(|q: CallbackQuery, watched: WatchedChats| {
            q.message
                .as_ref()
                .map(|m| watched.contains(m.chat().id.0))
                .unwrap_or(false)
        })
        .filter(|q: CallbackQuery| {
            q.data
                .as_deref()
                .is_some_and(|d| d.starts_with(CALLBACK_PREFIX))
        })
        .endpoint(captcha_handler::handle);

    let message_branch = Update::filter_message()
        .filter(|msg: Message, watched: WatchedChats| watched.contains(msg.chat.id.0))
        .branch(
            dptree::entry()
                .filter_command::<Command>()
                .endpoint(command_handler::dispatch),
        )
        .endpoint(|msg: Message| async move {
            // Non-command, non-captcha messages — full spam pipeline lands in M2.
            tracing::trace!(
                chat_id = msg.chat.id.0,
                user_id = msg.from.as_ref().map(|u| u.id.0),
                "message received (no handler in M1)"
            );
            Ok::<(), anyhow::Error>(())
        });

    let handler = dptree::entry()
        .branch(chat_member_branch)
        .branch(callback_branch)
        .branch(message_branch);

    info!("telegram dispatcher: M1 handler tree ready");

    Dispatcher::builder(bot, handler)
        .dependencies(dptree::deps![watched, state])
        .default_handler(|update| async move {
            tracing::trace!(update_id = update.id.0, "no matching handler");
        })
        .error_handler(teloxide::error_handlers::LoggingErrorHandler::new())
        .enable_ctrlc_handler()
        .build()
}

/// Updates the bot subscribes to. teloxide defaults exclude `chat_member` —
/// the captcha pipeline depends on it, so we list every variant we use.
pub fn allowed_updates() -> Vec<AllowedUpdate> {
    vec![
        AllowedUpdate::Message,
        AllowedUpdate::CallbackQuery,
        AllowedUpdate::ChatMember,
        AllowedUpdate::MyChatMember,
    ]
}
