//! HTTP API surface — Axum router, response envelope, route handlers.

pub mod admin_secret_middleware;
pub mod response;
pub mod routes_about;
pub mod routes_admin;
pub mod routes_auth;
pub mod routes_chats;
pub mod routes_config;
pub mod routes_health;
pub mod routes_moderation;
pub mod server;
pub mod state;
pub mod webapp_auth_middleware;

pub use response::{ApiError, ApiResult};
pub use server::build_router;
pub use state::AppState;
pub use webapp_auth_middleware::AuthContext;
