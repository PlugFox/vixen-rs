//! `GET /api/v1/chats` — list of chats the authenticated moderator can manage.
//!
//! The set is the intersection of:
//!  1. the JWT's `chat_ids` claim (frozen at login),
//!  2. the rows in `chats` (so a chat dropped from `CONFIG_CHATS` between
//!     login and request stops appearing).
//!
//! Metadata (title / type / member count) is best-effort from
//! `chat_info_cache` — NULL columns when the bot hasn't seen the chat yet.

use axum::Extension;
use axum::extract::State;
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
