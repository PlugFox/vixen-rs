//! `vixen-server` entrypoint. Loads config, initialises tracing, brings up the
//! HTTP listener (and later the teloxide dispatcher + job runner) under one
//! `CancellationToken`. SIGINT / SIGTERM cancels the token; tasks have 30s to
//! drain before the process exits.

use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use clap::Parser;
use teloxide::prelude::*;
use teloxide::utils::command::BotCommands;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};
use vixen_server::{
    api::{AppState, build_router},
    build_info,
    config::Config,
    database::{Database, Redis, ensure_watched_chats},
    jobs,
    services::captcha::{CaptchaService, CaptchaState, Fonts},
    services::cas_client::CasClient,
    services::moderation_service::ModerationService,
    services::spam::service::SpamService,
    telegram::commands::Command,
    telegram::{WatchedChats, build_dispatcher},
    telemetry,
};

const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(30);

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Best-effort .env load for local dev; production injects vars via the orchestrator.
    // First match wins, so an explicit `.env` in CWD overrides the repo template path.
    for path in [".env", "config/.env"] {
        if dotenvy::from_filename(path).is_ok() {
            break;
        }
    }

    let config = Arc::new(Config::parse());

    if let Err(e) = config.validate() {
        eprintln!("configuration error: {e}");
        std::process::exit(2);
    }

    // Hold the guard until main returns: dropping it flushes the JSON file writer.
    let _telemetry_guard = telemetry::init(&config.log_level, &config.log_dir);

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

    let db = Arc::new(
        Database::connect(&config)
            .await
            .context("postgres connect")?,
    );
    db.health_check().await.context("postgres health")?;
    info!("postgres connected");

    db.migrate().await.context("apply pending migrations")?;
    info!("postgres migrations applied");

    let redis = Arc::new(
        Redis::connect(config.redis_url.clone())
            .await
            .context("redis connect")?,
    );
    info!("redis connected");

    // Idempotent: every CONFIG_CHATS entry must have a row in `chats` and
    // `chat_config` so subsequent INSERTs (captcha challenges, moderation
    // actions, etc.) don't trip foreign-key constraints.
    ensure_watched_chats(db.pool(), &config.chats)
        .await
        .context("seed watched chats")?;

    // Hot-reload subscription: M4 publishes `chat_config:{chat_id}` invalidations
    // here when a moderator edits per-chat settings. For now we just log them.
    let pubsub_handle = redis.subscribe("chat_config:*", cancel.clone(), |channel, payload| {
        debug!(channel, payload, "chat_config invalidation received");
    });

    let fonts = Fonts::load().context("load captcha fonts")?;
    let captcha = Arc::new(CaptchaService::new(db.pool().clone(), fonts));
    let captcha_state = Arc::new(CaptchaState::new(redis.clone()));

    // Bot is constructed before AppState so M2 services (ModerationService)
    // can capture a clone — Bot is cheap to clone.
    let bot = Bot::new(config.bot_token.expose());

    let cas = CasClient::new(redis.clone(), config.cas_base_url.clone());
    let spam = Arc::new(SpamService::new(db.pool().clone(), cas));
    let moderation = ModerationService::new(db.pool().clone(), bot.clone());

    let state = AppState {
        config: config.clone(),
        db: db.clone(),
        redis: redis.clone(),
        captcha: captcha.clone(),
        captcha_state: captcha_state.clone(),
        spam: spam.clone(),
        moderation: moderation.clone(),
    };

    let http_handle = spawn_http(&config.address, state.clone(), cancel.clone())
        .await
        .context("HTTP server failed to start")?;

    if let Err(e) = bot.set_my_commands(Command::bot_commands()).await {
        warn!(error = %e, "set_my_commands failed");
    }

    let dispatcher_handle = spawn_dispatcher(bot.clone(), &config, state.clone(), cancel.clone());
    let job_handles = jobs::spawn_all(bot.clone(), state.clone(), cancel.clone());

    // Surface JoinErrors (panics, abort) from each long-running task. Without
    // this, a job panic during shutdown is swallowed and the operator sees
    // only a "shutdown clean" log line.
    let join = async {
        if let Err(e) = http_handle.await {
            error!(?e, "HTTP server task join error");
        }
        if let Err(e) = dispatcher_handle.await {
            error!(?e, "telegram dispatcher task join error");
        }
        for (idx, handle) in job_handles.into_iter().enumerate() {
            if let Err(e) = handle.await {
                error!(job_index = idx, ?e, "background job task join error");
            }
        }
        if let Err(e) = pubsub_handle.await {
            error!(?e, "redis pubsub task join error");
        }
    };

    cancel.cancelled().await;
    info!("shutdown initiated, draining tasks");

    match tokio::time::timeout(SHUTDOWN_TIMEOUT, join).await {
        Ok(()) => info!("shutdown clean"),
        Err(_) => warn!(
            timeout_secs = SHUTDOWN_TIMEOUT.as_secs(),
            "shutdown timed out, exiting"
        ),
    }

    db.close().await;
    drop(redis);
    Ok(())
}

async fn spawn_http(
    addr: &str,
    state: AppState,
    cancel: CancellationToken,
) -> anyhow::Result<JoinHandle<()>> {
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .with_context(|| format!("binding {addr}"))?;
    info!(address = addr, "HTTP listening");

    let app = build_router(state);

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

fn spawn_dispatcher(
    bot: Bot,
    config: &Config,
    state: AppState,
    cancel: CancellationToken,
) -> JoinHandle<()> {
    let watched = WatchedChats::new(config.chats.iter().copied());
    info!(
        chats = watched.len(),
        "telegram dispatcher: starting polling"
    );

    let mut dispatcher = build_dispatcher(bot, watched, state);
    let shutdown = dispatcher.shutdown_token();

    // Bridge our CancellationToken to teloxide's ShutdownToken: when the global
    // cancel fires, ask the dispatcher to drain.
    tokio::spawn(async move {
        cancel.cancelled().await;
        info!("telegram dispatcher: shutdown requested");
        if let Err(e) = shutdown.shutdown() {
            warn!(error = %e, "telegram dispatcher: shutdown signal not delivered");
        }
    });

    tokio::spawn(async move {
        dispatcher.dispatch().await;
        info!("telegram dispatcher: stopped");
    })
}

/// Listen for shutdown signals and fire the global cancel.
///
/// Production deploys are Linux containers, so the Unix path watches both
/// SIGTERM (orchestrator stop) and SIGINT (interactive Ctrl-C). The non-Unix
/// path falls back to `ctrl_c()` so the binary still builds on Windows for
/// `cargo check` / IDE workflows.
#[cfg(unix)]
fn spawn_signal_listener(cancel: CancellationToken) {
    use tokio::signal::unix::{SignalKind, signal};

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

#[cfg(not(unix))]
fn spawn_signal_listener(cancel: CancellationToken) {
    tokio::spawn(async move {
        if let Err(e) = tokio::signal::ctrl_c().await {
            warn!(error = %e, "ctrl_c listener failed; cancelling immediately");
        } else {
            info!("Ctrl-C received, shutting down");
        }
        cancel.cancel();
    });
}
