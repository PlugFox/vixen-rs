//! `/admin/*` — operational endpoints guarded by [`admin_secret_middleware`].
//!
//! [`admin_secret_middleware`]: crate::api::admin_secret_middleware
//!
//! `POST /admin/ping` is the canary used by deployment scripts to verify the
//! middleware chain end-to-end (server is up, secret matches, router wired).

use chrono::{DateTime, Utc};
use serde::Serialize;
use utoipa::ToSchema;

use crate::api::response::ApiResult;
use crate::api_success;

#[derive(Debug, Serialize, ToSchema)]
pub struct AdminPingResponse {
    pub status: &'static str,
    pub received_at: DateTime<Utc>,
}

#[utoipa::path(
    post,
    path = "/admin/ping",
    tag = "admin",
    responses(
        (status = 200, description = "Admin secret accepted"),
        (status = 401, description = "Admin secret missing or invalid"),
        (status = 503, description = "Admin secret not configured on this server"),
    ),
    security(("adminSecret" = []))
)]
pub async fn ping() -> ApiResult<AdminPingResponse> {
    api_success!(AdminPingResponse {
        status: "ok",
        received_at: Utc::now(),
    })
}
