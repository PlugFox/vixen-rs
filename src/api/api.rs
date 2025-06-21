use axum::middleware::from_fn_with_state;
use axum::{Router, routing::get};

use sqlx::SqlitePool;
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tracing::info;

use crate::api::{middleware, routes};
use crate::config;

/// Run the HTTP API server with graceful shutdown
pub async fn start(
    conf: &config::Config,
    pool: SqlitePool,
    shutdown_signal: impl std::future::Future<Output = ()> + Send + 'static,
) {
    // Try to parse the address from the configuration
    let addr: SocketAddr = conf.address.parse().expect("Invalid address format");

    // Initialize the private scope for authentication middleware
    // This scope contains the secret used for authentication
    let private = middleware::AdminScope {
        secret: conf.secret.clone(),
    };

    // Public routes without authentication
    let public_routes = Router::new()
        .route("/health", get(routes::health))
        .route("/healthz", get(routes::health))
        .route("/404", get(routes::not_found));

    // Private routes that require authentication
    let protected_routes = Router::new()
        .route("/download", get(routes::download_database))
        .route("/database", get(routes::download_database))
        .layer(from_fn_with_state(private, middleware::auth_middleware));

    let app = Router::new()
        .merge(public_routes)
        .nest("/admin", protected_routes)
        .fallback(routes::not_found)
        .with_state(pool);

    // Listen on the specified address and handle incoming connections
    let listener = TcpListener::bind(&addr)
        .await
        .expect("Failed to bind to address");

    info!(%addr, "Starting API server");

    // Start the server with graceful shutdown support
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal)
        .await
        .expect("API server crashed");

    // Right after the server stops, we log that the server has stopped
    info!("API server stopped");
}
