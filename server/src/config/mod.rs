//! Configuration. M0 minimal stub — full clap parser, secret newtypes and
//! validation land in #22 (per `server/docs/config.md`).

use clap::Parser;

/// Bot configuration. All fields read from `CONFIG_*` env vars; CLI overrides supported.
#[derive(Parser, Debug, Clone)]
#[command(
    name = "vixen-server",
    version,
    about = "Telegram anti-spam bot — captcha gating, spam pipeline, daily reports."
)]
pub struct Config {
    /// HTTP bind address (host:port).
    #[arg(long, env = "CONFIG_ADDRESS", default_value = "0.0.0.0:8000")]
    pub address: String,

    /// Log level (trace, debug, info, warn, error). Overridden by `RUST_LOG`.
    #[arg(long, env = "CONFIG_LOG_LEVEL", default_value = "info")]
    pub log_level: String,

    /// Telegram bot token from @BotFather. Required for the poller to start.
    #[arg(long, env = "CONFIG_BOT_TOKEN")]
    pub bot_token: Option<String>,

    /// Postgres connection URL (postgresql://user:pass@host:port/db).
    #[arg(long, env = "CONFIG_DATABASE_URL")]
    pub database_url: Option<String>,

    /// Redis connection URL (redis://host:port).
    #[arg(long, env = "CONFIG_REDIS_URL")]
    pub redis_url: Option<String>,

    /// Comma-separated Telegram chat IDs the bot watches. Other chats are ignored.
    #[arg(long, env = "CONFIG_CHATS", value_delimiter = ',')]
    pub chats: Vec<i64>,
}
