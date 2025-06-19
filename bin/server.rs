use clap::Parser;

use tracing::{debug, info, /* trace, warn, error */};
//use tracing_log::log;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};
use tracing_appender::rolling;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Parse command line arguments - this will automatically handle --help
    let config = Config::parse();
    init_tracing(&config);

    info!("Starting Telegram Bot Server...");
    debug!("Configuration:");
    debug!("  Environment: {}", config.environment.as_deref().unwrap_or("production"));
    debug!("  Address: {}", config.address);
    debug!("  Port: {}", config.port);
    debug!("  Log Level: {}", config.log_level);

    // TODO: Implement actual server logic here
    info!("Server would start at {}:{}", config.address, config.port);

    Ok(())
}

#[derive(Parser, Debug)]
#[command(
    name = "vixen",
    version,
    about = "Telegram Bot Server for automatically banning spammers",
    long_about = "A Telegram bot server that automatically detects and bans spammers in Telegram chats. \
                  This server provides a REST API and connects to the Telegram Bot API to monitor \
                  chat messages and take action against spam accounts."
)]
pub struct Config {
    /// Environment to run the server in (CLI > ENV > default)
    #[arg(
        short = 'e',
        long = "env",
        env = "VIXEN_ENVIRONMENT",
        default_value = "development",
        help = "Environment to run the server in (development, production, etc.)",
        aliases = ["environment", "mode"]
    )]
    environment: Option<String>,

    /// Address to bind the server to (CLI > ENV > default)
    #[arg(
        short = 'a',
        long,
        env = "VIXEN_ADDRESS",
        default_value = "0.0.0.0",
        aliases = ["host", "addr"],
        help = "IP address to bind the server to"
    )]
    address: String,

    /// Port to bind the server to (CLI > ENV > default)
    #[arg(
        short = 'p',
        long = "port",
        env = "VIXEN_PORT",
        default_value_t = 8080,
        help = "Port number to bind the server to"
    )]
    port: u16,

    /// Level of logging (CLI > ENV > default)
    /// This can be set to trace, debug, info, warn, or error
    #[arg(
        short = 'l',
        long = "logs",
        env = "VIXEN_LOGS",
        default_value = "info",
        aliases = ["log", "level", "verbose"],
        help = "Logging level (trace, debug, info, warn, error)"
    )]
    log_level: String,
}

fn init_tracing(config: &Config) {
    // 1) Фильтр: RUST_LOG=debug или APP_LOG=info
    /* let env_filter = EnvFilter::try_from_env("APP_LOG")
        .or_else(|_| EnvFilter::try_from_default_env())
        .unwrap_or_else(|_| EnvFilter::new("info")); */

    // 1) Фильтр: RUST_LOG=debug или APP_LOG=info
    let env_filter = EnvFilter::builder()
        .with_default_directive(config.log_level.parse().unwrap_or_else(|_| "info".parse().unwrap()))
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