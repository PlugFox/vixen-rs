//! `GET /about` — build metadata (no secrets, no PII).

use axum::Json;
use serde::Serialize;
use utoipa::ToSchema;

use crate::build_info;

#[derive(Serialize, ToSchema)]
pub struct AboutResponse {
    /// Crate name (`vixen-server`).
    #[schema(example = "vixen-server")]
    pub name: &'static str,
    /// SemVer from `Cargo.toml`.
    #[schema(example = "0.1.0")]
    pub version: &'static str,
    /// Short git SHA (7 chars).
    #[schema(example = "abc1234")]
    pub commit_hash: &'static str,
    /// ISO-8601 UTC build timestamp.
    #[schema(example = "2026-05-02T00:00:00Z")]
    pub built_at: &'static str,
    /// Rustc version that compiled the binary.
    #[schema(example = "1.93.0")]
    pub rust_version: &'static str,
    /// Cargo build profile.
    #[schema(example = "release")]
    pub profile: &'static str,
    /// Target triple.
    #[schema(example = "aarch64-apple-darwin")]
    pub target: &'static str,
}

#[utoipa::path(
    get,
    path = "/about",
    responses((status = 200, body = AboutResponse, description = "Build metadata")),
    tag = "ops"
)]
pub async fn about() -> Json<AboutResponse> {
    Json(AboutResponse {
        name: build_info::NAME,
        version: build_info::VERSION,
        commit_hash: build_info::GIT_HASH,
        built_at: build_info::BUILD_DATE,
        rust_version: build_info::RUST_VERSION,
        profile: build_info::BUILD_PROFILE,
        target: build_info::BUILD_TARGET,
    })
}
