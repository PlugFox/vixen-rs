use axum::middleware::from_fn_with_state;
use axum::{
    Router,
    routing::{delete, get, put},
};

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

    /* // --- Meta --- //
    ..get('/<ignored|health|healthz|status>', $GET$HealthCheck)
    ..get('/<ignored|about|version>', $GET$About)
    // --- Database --- //
    ..get('/admin/<ignored|db|database|sqlite|sqlite3>', $GET$Admin$Database)
    // --- Logs --- //
    ..get('/admin/logs', $GET$Admin$Logs)
    ..get('/admin/logs/<id>', $GET$Admin$Logs)
    // --- Users --- //
    ..get('/admin/users/verified', $GET$Admin$Users$Verified)
    ..put('/admin/users/verified', $PUT$Admin$Users$Verified)
    ..delete('/admin/users/verified', $DELETE$Admin$Users$Verified)
    // --- Messages --- //
    ..get('/admin/messages/deleted', $GET$Admin$Messages$Deleted)
    ..get('/admin/messages/deleted/hash', $GET$Admin$Messages$Deleted$Hash)
    // --- Reports --- //
    ..get('/report', $GET$Report)
    ..get('/admin/report', $GET$Admin$Report)
    ..get('/admin/chart', $GET$Admin$Chart)
    ..get('/admin/chart.png', $GET$Admin$ChartPng)
    ..get('/admin/summary/<cid>', $GET$Admin$Summary) */

    // Public routes without authentication
    let public_routes = Router::new()
        // --- Meta --- //
        .route("/report", get(routes::get_report))
        .route("/health", get(routes::get_health))
        .route("/healthz", get(routes::get_health))
        .route("/status", get(routes::get_health))
        .route("/about", get(routes::get_about))
        .route("/version", get(routes::get_about))
        .route("/404", get(routes::not_found));

    // Private routes that require authentication
    let protected_routes = Router::new()
        // --- Database --- //
        .route("/download", get(routes::admin_get_download_database))
        .route("/database", get(routes::admin_get_download_database))
        .route("/sqlite", get(routes::admin_get_download_database))
        .route("/sqlite3", get(routes::admin_get_download_database))
        .route("/db", get(routes::admin_get_download_database))
        // --- Logs --- //
        .route("/logs", get(routes::admin_get_logs))
        .route("/logs/:id", get(routes::admin_get_logs_by_id))
        // --- Users --- //
        .route("/users/verified", get(routes::admin_get_users_verified))
        .route("/users/verified", put(routes::admin_put_users_verified))
        .route(
            "/users/verified",
            delete(routes::admin_delete_verified_users),
        )
        // --- Messages --- //
        .route("/messages/deleted", get(routes::admin_get_messages_deleted))
        .route(
            "/messages/deleted/hash",
            get(routes::admin_get_messages_deleted_hash),
        )
        // --- Reports --- //
        .route("/report", get(routes::admin_get_report))
        .route("/chart", get(routes::admin_get_chart))
        .route("/chart.png", get(routes::admin_get_chart_png))
        .route("/summary/:cid", get(routes::admin_get_summary))
        // Apply the authentication middleware to all protected routes
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
