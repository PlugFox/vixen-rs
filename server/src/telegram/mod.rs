//! Telegram bot — teloxide dispatcher + watched-chats filter (M0).
//! Captcha / spam / moderation handlers populated from M1 onwards under
//! `handlers/`. See `server/docs/bot.md`.

pub mod commands;
pub mod dispatcher;
pub mod handlers;

pub use dispatcher::{WatchedChats, allowed_updates, build_dispatcher};
