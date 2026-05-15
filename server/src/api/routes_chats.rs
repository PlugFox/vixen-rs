//! `GET /api/v1/chats` — list of chats the authenticated moderator can manage,
//! plus `GET /api/v1/chats/{chat_id}/stats` — a summary card used by the
//! dashboard's chat-detail header (member count, verified count, banned
//! count, last-24h captcha outcome counts).
//!
//! The set is the intersection of:
//!  1. the JWT's `chat_ids` claim (frozen at login),
//!  2. the rows in `chats` (so a chat dropped from `CONFIG_CHATS` between
//!     login and request stops appearing).
//!
//! Metadata (title / type / member count) is best-effort from
//! `chat_info_cache` — NULL columns when the bot hasn't seen the chat yet.

use axum::Extension;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::Serialize;
use utoipa::ToSchema;

use crate::api::AuthContext;
use crate::api::response::ApiResult;
use crate::api::state::AppState;
use crate::{api_error, api_success};

#[derive(Debug, Serialize, ToSchema)]
pub struct ChatSummary {
    pub chat_id: i64,
    /// Public slug (kebab-case fallback if unset).
    pub slug: Option<String>,
    pub title: Option<String>,
    /// `private` | `group` | `supergroup` | `channel`, or null if unseen.
    pub kind: Option<String>,
    pub members_count: Option<i32>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ChatsListResponse {
    pub chats: Vec<ChatSummary>,
}

#[utoipa::path(
    get,
    path = "/api/v1/chats",
    tag = "chats",
    responses(
        (status = 200, description = "Chats the caller moderates"),
        (status = 401, description = "Missing or invalid JWT"),
    ),
    security(("bearerAuth" = []))
)]
pub async fn list_chats(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
) -> ApiResult<ChatsListResponse> {
    if ctx.chat_ids.is_empty() {
        return api_success!(ChatsListResponse { chats: vec![] });
    }

    let rows = match sqlx::query!(
        r#"
        SELECT
            c.chat_id,
            c.slug,
            ci.title         AS "title?",
            ci.type          AS "kind?",
            ci.members_count AS "members_count?"
        FROM chats c
        LEFT JOIN chat_info_cache ci ON ci.chat_id = c.chat_id
        WHERE c.chat_id = ANY($1)
        ORDER BY c.chat_id
        "#,
        &ctx.chat_ids
    )
    .fetch_all(state.db.pool())
    .await
    {
        Ok(r) => r,
        Err(e) => return api_error!("DB_ERROR", e.to_string()),
    };

    let chats = rows
        .into_iter()
        .map(|r| ChatSummary {
            chat_id: r.chat_id,
            slug: r.slug,
            title: r.title,
            kind: r.kind,
            members_count: r.members_count,
        })
        .collect();

    api_success!(ChatsListResponse { chats })
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ChatStatsResponse {
    pub chat_id: i64,
    /// Live members count from the bot's 6h refresh of `chat_info_cache`.
    /// `null` when the bot hasn't seen the chat yet.
    pub members_count: Option<i32>,
    /// Total rows in `verified_users` for this chat.
    pub verified_count: i64,
    /// Users whose latest terminal moderation action is `ban` (i.e. not
    /// subsequently unbanned). Derived from `moderation_actions`.
    pub banned_count: i64,
    /// Bot-driven verifies in the last 24h — captcha-solve events.
    pub captcha_solved_24h: i64,
    /// `captcha_failed` ledger rows in the last 24h.
    pub captcha_failed_24h: i64,
}

#[utoipa::path(
    get,
    path = "/api/v1/chats/{chat_id}/stats",
    tag = "chats",
    params(("chat_id" = i64, Path, description = "Telegram chat ID (i64)")),
    responses(
        (status = 200, description = "Chat summary (counts only)"),
        (status = 401, description = "Missing or invalid JWT"),
        (status = 403, description = "Caller is not a moderator of this chat"),
    ),
    security(("bearerAuth" = []))
)]
pub async fn get_stats(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Path(chat_id): Path<i64>,
) -> ApiResult<ChatStatsResponse> {
    if !ctx.can_access(chat_id) {
        return api_error!(
            "FORBIDDEN",
            "You are not a moderator of this chat",
            StatusCode::FORBIDDEN
        );
    }

    let row = match sqlx::query!(
        r#"
        SELECT
            (SELECT ci.members_count FROM chat_info_cache ci WHERE ci.chat_id = $1)
                                                                            AS "members_count?",
            (SELECT COUNT(*) FROM verified_users WHERE chat_id = $1)
                                                                            AS "verified_count!",
            -- `id DESC` is the same tie-breaker `list_banned` uses — without
            -- it `DISTINCT ON` would pick either row non-deterministically
            -- when a tied ban + unban share `created_at`, and stats could
            -- disagree with the audit-log derived banned list.
            (SELECT COUNT(*) FROM (
                SELECT DISTINCT ON (target_user_id) action
                FROM moderation_actions
                WHERE chat_id = $1 AND action IN ('ban', 'unban')
                ORDER BY target_user_id, created_at DESC, id DESC
            ) sub WHERE sub.action = 'ban')                                 AS "banned_count!",
            (SELECT COUNT(*) FROM moderation_actions
                WHERE chat_id = $1
                  AND action = 'verify' AND actor_kind = 'bot'
                  AND created_at > NOW() - INTERVAL '1 day')                AS "captcha_solved_24h!",
            (SELECT COUNT(*) FROM moderation_actions
                WHERE chat_id = $1
                  AND action = 'captcha_failed'
                  AND created_at > NOW() - INTERVAL '1 day')                AS "captcha_failed_24h!"
        "#,
        chat_id,
    )
    .fetch_one(state.db.pool())
    .await
    {
        Ok(r) => r,
        Err(e) => return api_error!("DB_ERROR", e.to_string()),
    };

    api_success!(ChatStatsResponse {
        chat_id,
        members_count: row.members_count,
        verified_count: row.verified_count,
        banned_count: row.banned_count,
        captcha_solved_24h: row.captcha_solved_24h,
        captcha_failed_24h: row.captcha_failed_24h,
    })
}
