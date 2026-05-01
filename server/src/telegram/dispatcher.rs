//! teloxide dispatcher with the watched-chats filter.
//!
//! M0 only logs every received update. Captcha / spam / moderation handlers
//! land from M1 onwards in `server/src/telegram/handlers/`.

use std::collections::HashSet;
use std::sync::Arc;

use teloxide::dispatching::{DefaultKey, Dispatcher};
use teloxide::prelude::*;
use tracing::info;

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

/// Build a teloxide `Dispatcher` that filters every update through `watched`
/// and logs anything that passes. Returns the dispatcher; the caller drives it
/// with `dispatch()` and stops it via `Dispatcher::shutdown_token()`.
pub fn build_dispatcher(bot: Bot, watched: WatchedChats) -> Dispatcher<Bot, (), DefaultKey> {
    let handler = Update::filter_message()
        .filter(|msg: Message, watched: WatchedChats| watched.contains(msg.chat.id.0))
        .endpoint(|update: Update, msg: Message| async move {
            info!(
                update_id = update.id.0,
                chat_id = msg.chat.id.0,
                user_id = msg.from.as_ref().map(|u| u.id.0),
                kind = ?std::mem::discriminant(&msg.kind),
                "update received"
            );
            Ok::<(), ()>(())
        });

    Dispatcher::builder(bot, handler)
        .dependencies(dptree::deps![watched])
        .build()
}
