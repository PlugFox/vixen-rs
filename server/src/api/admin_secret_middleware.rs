//! `X-Admin-Secret` middleware for `/admin/*` ops endpoints.
//!
//! Comparison runs in constant time on **hashed** values so the secret's
//! length isn't leaked through early-exit timing. The hash function is
//! SHA-256 (already in the dep graph for the captcha pipeline); both sides
//! of the comparison are therefore exactly 32 bytes.
//!
//! When `CONFIG_ADMIN_SECRET` is unset the middleware refuses every request
//! with 503. Config validation enforces presence in prod; in dev a missing
//! secret means `/admin/*` is shut.

use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use tracing::debug;

use crate::api::response::ApiError;
use crate::api::state::AppState;

pub const ADMIN_SECRET_HEADER: &str = "x-admin-secret";

pub async fn admin_secret_middleware(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    let Some(secret) = state.config.admin_secret.as_ref() else {
        return ApiError {
            code: "ADMIN_NOT_CONFIGURED".into(),
            message: "Admin secret is not configured on this server".into(),
            status: StatusCode::SERVICE_UNAVAILABLE,
        }
        .into_response();
    };

    let provided = request
        .headers()
        .get(ADMIN_SECRET_HEADER)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let provided_hash = Sha256::digest(provided.as_bytes());
    let expected_hash = Sha256::digest(secret.expose().as_bytes());

    let matches: bool = provided_hash.ct_eq(&expected_hash).into();
    if !matches {
        debug!("admin secret rejected");
        return ApiError {
            code: "ADMIN_AUTH_FAILED".into(),
            message: "Admin secret missing or invalid".into(),
            status: StatusCode::UNAUTHORIZED,
        }
        .into_response();
    }

    next.run(request).await
}
