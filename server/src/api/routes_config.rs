//! `GET` / `PATCH /api/v1/chats/{chat_id}/config` — per-chat tunables.
//!
//! Authorisation is two-step:
//!  1. JWT middleware proves the caller's identity.
//!  2. `AuthContext::can_access(chat_id)` (this module) proves the JWT was
//!     minted for a user who moderates the chat.
//!
//! Writes go through [`ChatConfigService::update`] which serialises via
//! `SELECT ... FOR UPDATE`, publishes on Redis, and refreshes the local
//! Moka entry — the bot reads the new value within ~1s on every replica.

use axum::Extension;
use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;

use crate::api::AuthContext;
use crate::api::response::ApiResult;
use crate::api::state::AppState;
use crate::models::{ChatConfigDto, ChatConfigPatch};
use crate::services::chat_config_service::ChatConfigError;
use crate::{api_error, api_success};

#[utoipa::path(
    get,
    path = "/api/v1/chats/{chat_id}/config",
    tag = "config",
    params(("chat_id" = i64, Path, description = "Telegram chat ID (i64)")),
    responses(
        (status = 200, description = "Current per-chat config"),
        (status = 401, description = "Missing or invalid JWT"),
        (status = 403, description = "Caller is not a moderator of this chat"),
        (status = 404, description = "No chat_config row for this chat"),
    ),
    security(("bearerAuth" = []))
)]
pub async fn get_config(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Path(chat_id): Path<i64>,
) -> ApiResult<ChatConfigDto> {
    if !ctx.can_access(chat_id) {
        return api_error!(
            "FORBIDDEN",
            "You are not a moderator of this chat",
            StatusCode::FORBIDDEN
        );
    }

    match state.chat_config.get(chat_id).await {
        Ok(cfg) => api_success!(ChatConfigDto::from(&*cfg)),
        Err(ChatConfigError::NotFound(_)) => api_error!(
            "CHAT_CONFIG_NOT_FOUND",
            "No config row for this chat",
            StatusCode::NOT_FOUND
        ),
        Err(e) => api_error!("DB_ERROR", e.to_string()),
    }
}

#[utoipa::path(
    patch,
    path = "/api/v1/chats/{chat_id}/config",
    tag = "config",
    params(("chat_id" = i64, Path)),
    request_body = ChatConfigPatch,
    responses(
        (status = 200, description = "Updated config"),
        (status = 400, description = "Patch field out of range or invalid"),
        (status = 401, description = "Missing or invalid JWT"),
        (status = 403, description = "Caller is not a moderator of this chat"),
        (status = 404, description = "No chat_config row for this chat"),
        (status = 422, description = "Patch body had no fields"),
    ),
    security(("bearerAuth" = []))
)]
pub async fn patch_config(
    State(state): State<AppState>,
    Extension(ctx): Extension<AuthContext>,
    Path(chat_id): Path<i64>,
    Json(patch): Json<ChatConfigPatch>,
) -> ApiResult<ChatConfigDto> {
    if !ctx.can_access(chat_id) {
        return api_error!(
            "FORBIDDEN",
            "You are not a moderator of this chat",
            StatusCode::FORBIDDEN
        );
    }

    match state.chat_config.update(chat_id, patch).await {
        Ok(cfg) => api_success!(ChatConfigDto::from(&*cfg)),
        Err(ChatConfigError::NotFound(_)) => api_error!(
            "CHAT_CONFIG_NOT_FOUND",
            "No config row for this chat",
            StatusCode::NOT_FOUND
        ),
        Err(ChatConfigError::EmptyPatch) => api_error!(
            "EMPTY_PATCH",
            "Patch body had no recognised fields",
            StatusCode::UNPROCESSABLE_ENTITY
        ),
        Err(ChatConfigError::Validation(e)) => {
            api_error!("PATCH_VALIDATION", e.to_string(), StatusCode::BAD_REQUEST)
        }
        Err(e) => api_error!("DB_ERROR", e.to_string()),
    }
}
