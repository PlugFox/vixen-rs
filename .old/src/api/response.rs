use axum::{
    Json,
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
};
use serde::Serialize;

/// Basic structure for successful API responses
#[derive(Serialize)]
pub struct ApiResponse<T> {
    pub ok: bool,
    pub data: T,
}

/// Basic error structure for API responses
#[derive(Serialize)]
pub struct ApiError {
    pub code: String,
    pub message: String,
}

/// Standard error response
#[derive(Serialize)]
pub struct ApiErrorResponse {
    pub ok: bool,
    pub error: ApiError,
}

/// Enum for every possible API response type
pub enum ApiResult<T> {
    Success(T),
    Error {
        code: String,
        message: String,
        status: StatusCode,
    },
    File {
        content: Vec<u8>,
        filename: String,
        content_type: String,
    },
}

impl<T> ApiResult<T>
where
    T: Serialize,
{
    /// Creates a successful response with the provided data.
    pub fn success(data: T) -> Self {
        Self::Success(data)
    }

    /// Creates an error response with a default status of 400 Bad Request
    /// and a custom error code and message.
    pub fn error(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Error {
            code: code.into(),
            message: message.into(),
            status: StatusCode::BAD_REQUEST,
        }
    }

    /// Creates an error response with a custom status code,
    /// along with a custom error code and message.
    pub fn error_with_status(
        code: impl Into<String>,
        message: impl Into<String>,
        status: StatusCode,
    ) -> Self {
        Self::Error {
            code: code.into(),
            message: message.into(),
            status,
        }
    }

    /// Creates a file response with the provided content, filename, and content type.
    pub fn file(
        content: Vec<u8>,
        filename: impl Into<String>,
        content_type: impl Into<String>,
    ) -> Self {
        Self::File {
            content,
            filename: filename.into(),
            content_type: content_type.into(),
        }
    }
}

/// Implement IntoResponse for ApiResult to convert it into an Axum Response
impl<T> IntoResponse for ApiResult<T>
where
    T: Serialize,
{
    /// Converts ApiResult into an Axum Response.
    fn into_response(self) -> Response {
        match self {
            ApiResult::Success(data) => {
                let response = ApiResponse { ok: true, data };
                Json(response).into_response()
            }
            ApiResult::Error {
                code,
                message,
                status,
            } => {
                let response = ApiErrorResponse {
                    ok: false,
                    error: ApiError { code, message },
                };
                (status, Json(response)).into_response()
            }
            ApiResult::File {
                content,
                filename,
                content_type,
            } => {
                let mut headers = HeaderMap::new();
                headers.insert(
                    header::CONTENT_TYPE,
                    content_type
                        .parse()
                        .unwrap_or_else(|_| "application/octet-stream".parse().unwrap()),
                );
                headers.insert(
                    header::CONTENT_DISPOSITION,
                    format!("attachment; filename=\"{}\"", filename)
                        .parse()
                        .unwrap(),
                );

                (headers, content).into_response()
            }
        }
    }
}

/// Useful macros for creating API responses
/// Creates a successful API response with the provided data.
/// Creates an error API response with a custom code and message.
/// Creates an error API response with a custom code, message, and status code.
#[macro_export]
macro_rules! api_success {
    ($data:expr) => {
        $crate::api::response::ApiResult::success($data)
    };
}

#[macro_export]
macro_rules! api_error {
    ($code:expr, $message:expr) => {
        $crate::api::response::ApiResult::error($code, $message)
    };
    ($code:expr, $message:expr, $status:expr) => {
        $crate::api::response::ApiResult::error_with_status($code, $message, $status)
    };
}

#[macro_export]
macro_rules! api_file {
    ($content:expr, $filename:expr, $content_type:expr) => {
        $crate::api::response::ApiResult::file($content, $filename, $content_type)
    };
}
