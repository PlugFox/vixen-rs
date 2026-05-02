//! Mirrors `captcha_challenges` rows. Telegram IDs are `i64` (BIGINT).

use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

#[derive(Debug, Clone, FromRow)]
pub struct CaptchaChallenge {
    pub id: Uuid,
    pub chat_id: i64,
    pub user_id: i64,
    pub solution: String,
    pub attempts_left: i16,
    pub telegram_message_id: Option<i32>,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}
