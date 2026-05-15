//! `Authorization: Bearer <jwt>` middleware for `/api/v1/*` routes that need a
//! moderator identity. On success injects an [`AuthContext`] into the request
//! extensions; downstream handlers extract it via `Extension<AuthContext>`.
//!
//! Authority always derives from `chat_ids`, never from the display fields
//! (`tg.username`, `tg.first_name`). The list is frozen at JWT-mint time —
//! moderator changes take effect on next login (default 1h TTL).

use axum::extract::{Request, State};
use axum::http::{StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use tracing::debug;

use crate::api::response::ApiError;
use crate::api::state::AppState;
use crate::services::auth_service::decode_jwt;

/// Per-request authentication state attached by the middleware.
///
/// Routes pull it out via `axum::Extension<AuthContext>`.
#[derive(Clone, Debug)]
pub struct AuthContext {
    pub user_id: i64,
    pub chat_ids: Vec<i64>,
    pub username: Option<String>,
    pub first_name: String,
    pub last_name: Option<String>,
}

impl AuthContext {
    /// True when `chat_id` was in the token's `chat_ids` claim — i.e. the
    /// user was a moderator of that chat at login. Use this for the
    /// per-route IDOR guard.
    pub fn can_access(&self, chat_id: i64) -> bool {
        self.chat_ids.contains(&chat_id)
    }
}

pub async fn webapp_auth_middleware(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Response {
    let header_value = match request.headers().get(header::AUTHORIZATION) {
        Some(v) => v,
        None => {
            return unauthorized("MISSING_AUTHORIZATION", "Authorization header is required");
        }
    };
    let header_str = match header_value.to_str() {
        Ok(s) => s,
        Err(_) => {
            return unauthorized(
                "BAD_AUTHORIZATION",
                "Authorization header is not valid ASCII",
            );
        }
    };
    let token = match header_str.strip_prefix("Bearer ") {
        Some(t) if !t.is_empty() => t,
        _ => {
            return unauthorized(
                "BAD_AUTHORIZATION",
                "Authorization must use the Bearer scheme",
            );
        }
    };

    let Some(secret) = state.config.jwt_secret.as_ref() else {
        return ApiError {
            code: "JWT_NOT_CONFIGURED".into(),
            message: "JWT signing secret is not configured on this server".into(),
            status: StatusCode::SERVICE_UNAVAILABLE,
        }
        .into_response();
    };

    let claims = match decode_jwt(token, secret.expose()) {
        Ok(c) => c,
        Err(e) => {
            debug!(error = %e, "JWT decode rejected");
            return unauthorized(
                "INVALID_TOKEN",
                "Authentication token is invalid or expired",
            );
        }
    };

    let ctx = AuthContext {
        user_id: claims.sub,
        chat_ids: claims.chat_ids,
        username: claims.tg.username,
        first_name: claims.tg.first_name,
        last_name: claims.tg.last_name,
    };
    request.extensions_mut().insert(ctx);
    next.run(request).await
}

fn unauthorized(code: &str, message: &str) -> Response {
    ApiError {
        code: code.into(),
        message: message.into(),
        status: StatusCode::UNAUTHORIZED,
    }
    .into_response()
}
