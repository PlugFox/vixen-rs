use axum::{Router, extract::State, routing::get};
use sqlx::SqlitePool;
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tracing::info;

/// Health-check handler
async fn health() -> &'static str {
    "OK"
}

/// Run the HTTP API server with graceful shutdown
pub async fn start(
    addr: SocketAddr,
    pool: SqlitePool,
    shutdown_signal: impl std::future::Future<Output = ()> + Send + 'static,
) {
    let app = Router::new().route("/health", get(health)).with_state(pool);

    let listener = TcpListener::bind(&addr)
        .await
        .expect("Failed to bind to address");

    info!(%addr, "Starting API server");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal)
        .await
        .expect("API server crashed");

    info!("API server stopped");
}
