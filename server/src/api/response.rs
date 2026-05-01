//! Unified REST response envelope. All M1+ endpoints return `ApiResult<T>`,
//! which serialises as either:
//!
//! ```json
//! { "status": "ok", "data": { ... } }
//! ```
//!
//! or
//!
//! ```json
//! { "status": "error", "error": { "code": "MODERATOR_REQUIRED", "message": "..." } }
//! ```
//!
//! `/health` and `/about` are operational endpoints and bypass this envelope —
//! see `routes_health.rs` and `routes_about.rs`.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use utoipa::ToSchema;

/// Unified result type for REST handlers. Use the `api_success!` /
/// `api_error!` macros to construct.
#[derive(Debug)]
pub enum ApiResult<T: Serialize> {
    Success { data: T, status: StatusCode },
    Error(ApiError),
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ApiError {
    pub code: String,
    pub message: String,
    #[serde(skip)]
    pub status: StatusCode,
}

impl<T: Serialize> IntoResponse for ApiResult<T> {
    fn into_response(self) -> Response {
        match self {
            Self::Success { data, status } => {
                let body = serde_json::json!({ "status": "ok", "data": data });
                (status, Json(body)).into_response()
            }
            Self::Error(e) => e.into_response(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = serde_json::json!({
            "status": "error",
            "error": { "code": self.code, "message": self.message },
        });
        (self.status, Json(body)).into_response()
    }
}

/// `api_success!(data)` → 200 OK envelope. `api_success!(data, status)` to override.
#[macro_export]
macro_rules! api_success {
    ($data:expr) => {
        $crate::api::response::ApiResult::Success {
            data: $data,
            status: ::axum::http::StatusCode::OK,
        }
    };
    ($data:expr, $status:expr) => {
        $crate::api::response::ApiResult::Success {
            data: $data,
            status: $status,
        }
    };
}

/// `api_error!(code, message)` → 500. `api_error!(code, message, status)` to override.
#[macro_export]
macro_rules! api_error {
    ($code:expr, $msg:expr) => {
        $crate::api::response::ApiResult::Error($crate::api::response::ApiError {
            code: ($code).into(),
            message: ($msg).into(),
            status: ::axum::http::StatusCode::INTERNAL_SERVER_ERROR,
        })
    };
    ($code:expr, $msg:expr, $status:expr) => {
        $crate::api::response::ApiResult::Error($crate::api::response::ApiError {
            code: ($code).into(),
            message: ($msg).into(),
            status: $status,
        })
    };
}
