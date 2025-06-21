use axum::{Router, extract::State, routing::get};
use sqlx::SqlitePool;
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tracing::info;

use crate::config;

/// Health-check handler
async fn health(State(db_pool): State<SqlitePool>) -> &'static str {
    let row: (i32,) = match sqlx::query_as("SELECT 1 AS health")
        .fetch_one(&db_pool)
        .await
    {
        Ok(row) => row,
        Err(_) => {
            // Return 500 Internal Server Error if the database connection fails
            info!("Database connection failed");
            return axum::http::StatusCode::INTERNAL_SERVER_ERROR
                .canonical_reason()
                .unwrap_or("Internal Server Error");
        }
    };

    if row.0 == 1 {
        "OK"
    } else {
        "Database connection failed"
    }
}

/// Fallback handler for 404 Not Found
async fn not_found() -> &'static str {
    "Not Found"
}

/// Run the HTTP API server with graceful shutdown
pub async fn start(
    conf: &config::Config,
    pool: SqlitePool,
    shutdown_signal: impl std::future::Future<Output = ()> + Send + 'static,
) {
    let addr: SocketAddr = conf.address.parse().expect("Invalid address format");
    let app = Router::new()
        .route("/health", get(health))
        .route("/healthz", get(health))
        // Add more routes as needed
        .route("/404", get(not_found))
        .fallback(not_found)
        .with_state(pool);

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
