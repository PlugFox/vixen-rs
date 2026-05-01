//! HTTP API surface — Axum router, response envelope, route handlers.

pub mod response;
pub mod routes_about;
pub mod routes_health;
pub mod server;
pub mod state;

pub use response::{ApiError, ApiResult};
pub use server::build_router;
pub use state::AppState;
