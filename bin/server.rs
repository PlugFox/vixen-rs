use clap::Parser;

use tracing::{debug, info /* trace, warn, error */};
//use tracing_log::log;
use std::error::Error;
use tokio::{select, signal, spawn, sync::broadcast};
use tracing_appender::rolling;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

use vixen::config::Config;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Shutdown channel for graceful shutdown
    let (shutdown_tx, _) = broadcast::channel::<()>(1);

    // Parse command line arguments - this will automatically handle --help
    let config = Config::parse();
    init_logging(&config);

    info!("Starting Telegram Bot Server...");
    debug!("Configuration:");
    debug!(
        "  Environment: {}",
        config.environment.as_deref().unwrap_or("production")
    );
    debug!("  Address: {}", config.address);
    debug!("  Log Level: {}", config.log_level);

    // Start the HTTP server
    /* let http_shutdown_rx = shutdown_tx.subscribe();
    let http_shutdown_tx = shutdown_tx.clone();
    let http_handle = spawn(async move {
        if let Err(e) = http_server(http_shutdown_rx).await {
            eprintln!("HTTP server error: {}", e);
            // при ошибке проталкиваем shutdown
            let _ = http_shutdown_tx.send(());
        }
    }); */

    // Start the bot polling
    /* let bot_shutdown_rx = shutdown_tx.subscribe();
    let bot_shutdown_tx = shutdown_tx.clone();
    let bot_handle = spawn(async move {
        if let Err(e) = bot_polling(bot_shutdown_rx).await {
            eprintln!("Bot polling error: {}", e);
            let _ = bot_shutdown_tx.send(());
        }
    });
    info!("Server would start at {}:{}", config.address, config.port); */

    // Wait for shutdown signal (Ctrl+C)
    signal::ctrl_c().await?;

    // let _ = tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

    let _ = shutdown_tx.send(());

    /* let _ = http_handle.await;
    let _ = bot_handle.await; */

    Ok(())
}

fn init_logging(config: &Config) {
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
