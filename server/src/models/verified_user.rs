//! Mirrors `verified_users` rows.

use chrono::{DateTime, Utc};
use sqlx::FromRow;

#[derive(Debug, Clone, FromRow)]
pub struct VerifiedUser {
    pub chat_id: i64,
    pub user_id: i64,
    pub verified_at: DateTime<Utc>,
}
