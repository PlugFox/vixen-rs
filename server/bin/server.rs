//! `vixen-server` entrypoint. Loads config, initialises tracing, brings up the
//! HTTP listener (and later the teloxide dispatcher + job runner) under one
//! `CancellationToken`. SIGINT / SIGTERM cancels the token; tasks have 30s to
//! drain before the process exits.

use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use clap::Parser;
use tokio::signal::unix::{SignalKind, signal};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{error, info, warn};
use vixen_server::{build_info, config::Config, telemetry};

const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(30);

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Best-effort .env load for local dev; production injects vars via the orchestrator.
    let _ = dotenvy::dotenv();

    let config = Arc::new(Config::parse());

    if let Err(e) = config.validate() {
        eprintln!("configuration error: {e}");
        std::process::exit(2);
    }

    telemetry::init(&config.log_level);

    info!(
        version = build_info::VERSION,
        git = build_info::GIT_HASH,
        rust = build_info::RUST_VERSION,
        built = build_info::BUILD_DATE,
        profile = build_info::BUILD_PROFILE,
        environment = %config.environment,
        chats = config.chats.len(),
        bot_token = %config.bot_token,
        "vixen-server starting"
    );

    let cancel = CancellationToken::new();
    spawn_signal_listener(cancel.clone());

    let http_handle = spawn_http(&config.address, cancel.clone())
        .await
        .context("HTTP server failed to start")?;

    // Future tasks land here under the same `cancel`:
    //   - teloxide dispatcher (#25)
    //   - background-job runner (M1+)
    //   - Redis pub/sub subscriber (#21)

    match tokio::time::timeout(SHUTDOWN_TIMEOUT, http_handle).await {
        Ok(Ok(())) => info!("shutdown clean"),
        Ok(Err(e)) => error!(error = %e, "http task panicked"),
        Err(_) => warn!(
            timeout_secs = SHUTDOWN_TIMEOUT.as_secs(),
            "shutdown timed out, exiting"
        ),
    }
    Ok(())
}

async fn spawn_http(addr: &str, cancel: CancellationToken) -> anyhow::Result<JoinHandle<()>> {
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("binding {addr}"))?;
    info!(address = addr, "HTTP listening");

    // Routes land in #24. Until then the router 404s every path — the listener
    // existing is what `cargo run` verification needs.
    let app = axum::Router::<()>::new();

    let handle = tokio::spawn(async move {
        let shutdown = async move { cancel.cancelled().await };
        if let Err(e) = axum::serve(listener, app)
            .with_graceful_shutdown(shutdown)
            .await
        {
            error!(error = %e, "HTTP server error");
        }
    });
    Ok(handle)
}

fn spawn_signal_listener(cancel: CancellationToken) {
    tokio::spawn(async move {
        let mut sigterm = signal(SignalKind::terminate()).expect("install SIGTERM handler");
        let mut sigint = signal(SignalKind::interrupt()).expect("install SIGINT handler");
        tokio::select! {
            _ = sigterm.recv() => info!("SIGTERM received, shutting down"),
            _ = sigint.recv() => info!("SIGINT received, shutting down"),
        }
        cancel.cancel();
    });
}
