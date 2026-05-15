//! `/api/v1/auth/*` endpoints.
//!
//! - `POST /api/v1/auth/telegram/login` — validates a Telegram `initData`
//!   payload (WebApp HMAC or Login Widget SHA256), looks up
//!   `chat_moderators` for the verified user, mints a 1h JWT.
//! - `GET  /api/v1/auth/me`             — JWT-protected echo of the current
//!   user's identity + chat ids. Used by the dashboard on app boot.

use axum::Extension;
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::api::AuthContext;
use crate::api::response::ApiResult;
use crate::api::state::AppState;
use crate::services::auth_service::{AuthError, chats_for, mint_jwt, validate_init_data};
use crate::{api_error, api_success};

#[derive(Debug, Deserialize, ToSchema)]
pub struct LoginRequest {
    /// Raw URL-encoded initData string from `Telegram.WebApp.initData` or
    /// the Login Widget callback. The server validates the HMAC against the
    /// bot token. Never log this field — the auth_date / hash leak the
    /// raw signature.
    pub init_data: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct LoginResponse {
    /// HS256 JWT. Send as `Authorization: Bearer <token>` on subsequent calls.
    pub token: String,
    /// Token lifetime in seconds.
    pub expires_in: i64,
    pub user: LoginUser,
    /// Chats this user moderates. Same as the JWT's `chat_ids` claim.
    pub chat_ids: Vec<i64>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct LoginUser {
    pub id: i64,
    pub username: Option<String>,
    pub first_name: String,
    pub last_name: Option<String>,
}

#[utoipa::path(
    post,
    path = "/api/v1/auth/telegram/login",
    tag = "auth",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Login succeeded"),
        (status = 400, description = "initData malformed"),
        (status = 401, description = "initData failed HMAC or auth_date expired"),
        (status = 503, description = "JWT signing secret not configured"),
    ),
)]
pub async fn telegram_login(
    State(state): State<AppState>,
    Json(payload): Json<LoginRequest>,
) -> ApiResult<LoginResponse> {
    let Some(jwt_secret) = state.config.jwt_secret.as_ref() else {
        return api_error!(
            "JWT_NOT_CONFIGURED",
            "JWT signing secret is not configured on this server",
            StatusCode::SERVICE_UNAVAILABLE
        );
    };

    let now_secs = Utc::now().timestamp();
    let now_u64: u64 = if now_secs < 0 { 0 } else { now_secs as u64 };

    let identity = match validate_init_data(
        &payload.init_data,
        state.config.bot_token.expose(),
        now_u64,
        state.config.init_data_max_age_secs,
    ) {
        Ok(id) => id,
        Err(AuthError::HmacMismatch | AuthError::BadHashFormat) => {
            return api_error!(
                "INVALID_INIT_DATA",
                "Telegram initData failed HMAC check",
                StatusCode::UNAUTHORIZED
            );
        }
        Err(AuthError::AuthDateExpired { .. }) => {
            return api_error!(
                "AUTH_DATE_EXPIRED",
                "Telegram initData is too old",
                StatusCode::UNAUTHORIZED
            );
        }
        Err(
            AuthError::Malformed
            | AuthError::MissingHash
            | AuthError::MissingAuthDate
            | AuthError::MissingUserFields
            | AuthError::UnknownShape,
        ) => {
            return api_error!(
                "BAD_INIT_DATA",
                "Telegram initData is malformed",
                StatusCode::BAD_REQUEST
            );
        }
        Err(e) => {
            return api_error!("AUTH_ERROR", e.to_string());
        }
    };

    let chat_ids = match chats_for(state.db.pool(), identity.user_id).await {
        Ok(v) => v,
        Err(e) => {
            return api_error!("DB_ERROR", e.to_string());
        }
    };

    let token = match mint_jwt(
        &identity,
        chat_ids.clone(),
        jwt_secret.expose(),
        state.config.jwt_ttl_secs,
        now_secs,
    ) {
        Ok(t) => t,
        Err(e) => return api_error!("JWT_ERROR", e.to_string()),
    };

    api_success!(LoginResponse {
        token,
        expires_in: state.config.jwt_ttl_secs,
        user: LoginUser {
            id: identity.user_id,
            username: identity.username,
            first_name: identity.first_name,
            last_name: identity.last_name,
        },
        chat_ids,
    })
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MeResponse {
    pub user_id: i64,
    pub username: Option<String>,
    pub first_name: String,
    pub last_name: Option<String>,
    pub chat_ids: Vec<i64>,
}

#[utoipa::path(
    get,
    path = "/api/v1/auth/me",
    tag = "auth",
    responses(
        (status = 200, description = "Current user identity"),
        (status = 401, description = "Missing or invalid JWT"),
    ),
    security(("bearerAuth" = []))
)]
pub async fn me(Extension(ctx): Extension<AuthContext>) -> ApiResult<MeResponse> {
    api_success!(MeResponse {
        user_id: ctx.user_id,
        username: ctx.username,
        first_name: ctx.first_name,
        last_name: ctx.last_name,
        chat_ids: ctx.chat_ids,
    })
}
