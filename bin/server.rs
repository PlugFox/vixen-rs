use clap::Parser;

use tracing::{debug, info /* trace, warn, error */};
//use tracing_log::log;
use std::error::Error;
use std::sync::Arc;
use tokio::{select, sync::broadcast, sync::oneshot};
use tracing_appender::rolling;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

use vixen::api;
use vixen::bot;
use vixen::config;
use vixen::db;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Parse command line arguments - this will automatically handle --help
    let config = Arc::new(config::Config::parse());
    init_logging(&config);

    info!("Starting Telegram Bot Server...");
    debug!(
        "Configuration:\n  Environment: {env}\n  Address: {address}\n  Log Level: {log_level}",
        env = config.environment.as_deref().unwrap_or("production"),
        address = config.address,
        log_level = config.log_level,
    );

    // Initialize database pool
    let (api_db, bot_db);
    {
        let db_pool = db::init_db_pool(&config.database).await?;
        // Make copies of the pool for API and Telegram services
        (api_db, bot_db) = {
            let pool = db::init_db_pool(&config.database).await?;
            (pool.clone(), pool.clone())
        };
        drop(db_pool); // Drop the original pool to avoid holding onto it unnecessarily
    }

    // Channels for graceful shutdowns
    let (api_tx, api_rx) = oneshot::channel::<()>();
    let (tg_tx, tg_rx) = oneshot::channel::<()>();

    // Spawn API service
    let api_handle: tokio::task::JoinHandle<()>;
    {
        let api_config = Arc::clone(&config);
        let api_shutdown = async {
            let _ = api_rx.await;
        };
        api_handle = tokio::spawn(async move {
            api::start(&api_config, api_db, api_shutdown).await;
        });
    }

    // Spawn Telegram polling service
    let tg_handle: tokio::task::JoinHandle<()>;
    {
        let tg_config = Arc::clone(&config);
        let tg_shutdown = async {
            let _ = tg_rx.await;
        };
        tg_handle = tokio::spawn(async move {
            bot::poll(&tg_config, bot_db, tg_shutdown).await;
        });
    }

    // Wait for shutdown signal (Ctrl+C)
    {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to listen for shutdown");
        tracing::info!("Shutdown signal received");
    }

    // Notify services to stop
    let _ = api_tx.send(());
    let _ = tg_tx.send(());

    // Wait for tasks to complete
    let _ = api_handle.await;
    let _ = tg_handle.await;

    Ok(())
}

/// Initialize logging with configurable levels and formats
/// This function sets up the logging system using `tracing` and `tracing_subscriber`.
fn init_logging(config: &config::Config) {
    // 1) Фильтр: RUST_LOG=debug или APP_LOG=info
    /* let env_filter = EnvFilter::try_from_env("APP_LOG")
    .or_else(|_| EnvFilter::try_from_default_env())
    .unwrap_or_else(|_| EnvFilter::new("info")); */

    // 1) Фильтр: RUST_LOG=debug или APP_LOG=info
    let env_filter = EnvFilter::builder()
        .with_default_directive(
            config
                .log_level
                .parse()
                .unwrap_or_else(|_| "info".parse().unwrap()),
        )
        .from_env_lossy();

    // 2) Консольный формат
    let console_layer = fmt::layer()
        .with_target(false) // не печатать имя модуля
        .with_ansi(true)
        .with_thread_ids(true);

    // 3) Файловый аппендер: новый файл каждый день
    let file_appender = rolling::RollingFileAppender::builder()
        .rotation(rolling::Rotation::DAILY) // ежедневная ротация
        .max_log_files(7) // хранить не более 7 файлов
        .build("logs") // каталог для логов
        .expect("Failed to create file appender");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);
    let file_layer = fmt::layer()
        .with_ansi(config.environment.as_deref() != Some("production")) // ANSI в dev, plain в prod
        .with_writer(non_blocking)
        .json(); // JSON-формат для парсинга

    // 4) Инициализация глобального подписчика
    tracing_subscriber::registry()
        .with(env_filter)
        .with(console_layer)
        .with(file_layer)
        .init();
}

/// Start the HTTP server
async fn http_server(shutdown: broadcast::Receiver<()>) -> Result<(), Box<dyn Error>> {
    // Здесь запускаете ваш HTTP-сервер, например с hyper или axum:
    // server.with_graceful_shutdown(async {
    //     let _ = shutdown.recv().await;
    // }).await?;
    Ok(())
}

/// Start the bot polling loop
async fn bot_polling(mut shutdown: broadcast::Receiver<()>) -> Result<(), Box<dyn Error>> {
    loop {
        select! {
            _ = shutdown.recv() => {
                // получили сигнал завершения
                break;
            }
            result = async {
                // Ваш polling: обращение к Telegram API, работа с БД и т.д.
            } => {
                // обработка результата polling
                let _ = result;
            }
        }
    }
    Ok(())
}
