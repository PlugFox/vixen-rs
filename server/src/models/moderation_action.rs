//! `moderation_actions` row + the action / actor enums.
//!
//! `action` and `actor_kind` are stored as `TEXT` with CHECK constraints — we
//! map them to Rust enums and round-trip via `as_db_str` / `from_db_str`. The
//! list is the full M1 set (initial seven plus `captcha_expired`,
//! `captcha_failed`, `kick`).

use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModerationActionKind {
    Ban,
    Unban,
    Mute,
    Unmute,
    Delete,
    Verify,
    Unverify,
    CaptchaExpired,
    CaptchaFailed,
    Kick,
}

impl ModerationActionKind {
    pub fn as_db_str(self) -> &'static str {
        match self {
            Self::Ban => "ban",
            Self::Unban => "unban",
            Self::Mute => "mute",
            Self::Unmute => "unmute",
            Self::Delete => "delete",
            Self::Verify => "verify",
            Self::Unverify => "unverify",
            Self::CaptchaExpired => "captcha_expired",
            Self::CaptchaFailed => "captcha_failed",
            Self::Kick => "kick",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActorKind {
    Bot,
    Moderator,
}

impl ActorKind {
    pub fn as_db_str(self) -> &'static str {
        match self {
            Self::Bot => "bot",
            Self::Moderator => "moderator",
        }
    }
}

#[derive(Debug, Clone, FromRow)]
pub struct ModerationAction {
    pub id: Uuid,
    pub chat_id: i64,
    pub target_user_id: i64,
    pub action: String,
    pub actor_kind: String,
    pub actor_user_id: Option<i64>,
    pub message_id: Option<i32>,
    pub reason: Option<String>,
    pub created_at: DateTime<Utc>,
}
