//! Telegram bot — teloxide dispatcher + watched-chats filter (M0).
//! Captcha / spam / moderation handlers populated from M1 onwards under
//! `handlers/`. See `server/docs/bot.md`.

pub mod dispatcher;

pub use dispatcher::{WatchedChats, build_dispatcher};
