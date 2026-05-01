//! `GET /health` — operational liveness probe.
//!
//! Returns `200` with `{"status":"ok","checks":{"db":"ok","redis":"ok"}}` when
//! both pools respond, and `503` with `"degraded"` plus per-component status
//! otherwise. Used by the load balancer / docker compose `condition:
//! service_healthy`.

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use serde::Serialize;
use utoipa::ToSchema;

use crate::api::state::AppState;

#[derive(Serialize, ToSchema)]
pub struct HealthResponse {
    /// Overall status — `ok` if every check is up, `degraded` otherwise.
    #[schema(example = "ok")]
    pub status: &'static str,
    pub checks: HealthChecks,
}

#[derive(Serialize, ToSchema)]
pub struct HealthChecks {
    /// Postgres connectivity (acquire + `SELECT 1`).
    #[schema(example = "ok")]
    pub db: &'static str,
    /// Redis connectivity (`PING`).
    #[schema(example = "ok")]
    pub redis: &'static str,
}

#[utoipa::path(
    get,
    path = "/health",
    responses(
        (status = 200, body = HealthResponse, description = "All checks passing"),
        (status = 503, body = HealthResponse, description = "One or more checks failing"),
    ),
    tag = "ops"
)]
pub async fn health(State(state): State<AppState>) -> (StatusCode, Json<HealthResponse>) {
    let db_ok = state.db.health_check().await.is_ok();
    let redis_ok = state.redis.ping().await.is_ok();
    let body = HealthResponse {
        status: if db_ok && redis_ok { "ok" } else { "degraded" },
        checks: HealthChecks {
            db: if db_ok { "ok" } else { "down" },
            redis: if redis_ok { "ok" } else { "down" },
        },
    };
    let code = if db_ok && redis_ok {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (code, Json(body))
}
