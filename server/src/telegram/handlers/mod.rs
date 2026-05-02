//! Telegram update handlers. Each module owns one update kind. Common rules
//! live in `server/docs/rules/telegram-handlers.md`.

pub mod captcha;
pub mod commands;
pub mod member_update;
