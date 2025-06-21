use clap::Parser;

/// Application settings loaded from environment or CLI args
#[derive(Parser, Debug)]
#[command(
    name = "vixen",
    version,
    author = "@plugfox",
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
        env = "CONFIG_ENVIRONMENT",
        default_value = "development",
        aliases = ["mode", "runmode", "runtime", "env"],
        help = "Environment to run the server in (development, production, etc.)"
    )]
    pub environment: Option<String>,

    /// Level of logging (CLI > ENV > default)
    /// This can be set to trace, debug, info, warn, or error
    #[arg(
        short = 'l',
        long = "logs",
        env = "VIXEN_LOGS",
        default_value = "info",
        aliases = ["log", "level", "verbose", "verbosity", "loglevel"],
        help = "Logging level (trace, debug, info, warn, error)"
    )]
    pub log_level: String,

    /// Address to bind the server to (CLI > ENV > default)
    /// e.g. 127.0.0.1:8080
    #[arg(
        short = 'a',
        long,
        env = "CONFIG_ADDRESS",
        default_value = "0.0.0.0:8080",
        aliases = ["host", "addr", "api", "connection"],
        help = "Address to bind the server to"
    )]
    pub address: String,

    /// SQLite connection URL (CLI > ENV > default)
    /// e.g. sqlite::memory: or sqlite://data/vixen.db?mode=rwc
    #[arg(
        short = 'd',
        long,
        env = "CONFIG_DATABASE",
        aliases = ["db", "sqlite", "sqlite3", "sql", "storage"],
        default_value = "sqlite://data/vixen.db?mode=rwc",
        help = "SQLite connection URL (e.g. sqlite://vixen.db or sqlite://data/vixen.db?mode=rwc)"
    )]
    pub database: String,

    /// Telegram Bot API token (CLI > ENV > default)
    /// e.g. 123456789:ABCdefGhIJKlmnoPQRstuVWXyz
    #[arg(
        short = 't',
        long,
        env = "CONFIG_TELEGRAM",
        aliases = ["vixen", "token", "bot", "tg", "tgbot"],
        help = "Telegram Bot API token (e.g. 123456789:ABCdefGhIJKlmnoPQRstuVWXyz)"
    )]
    pub telegram: String,

    /// Secret admin API key for API authentication (CLI > ENV > default)
    #[arg(
        short = 's',
        long,
        env = "CONFIG_SECRET",
        aliases = ["admin", "apikey", "key", "auth"],
        help = "Secret admin API key for API authentication"
    )]
    pub secret: String,

    /// Chats to monitor (CLI > ENV > default)
    /// e.g. 123456789, 987654321
    #[arg(
        short = 'c',
        long,
        env = "CONFIG_CHATS",
        aliases = ["chat", "groups", "channels", "monitored", "watched", "observed"],
        help = "Chats to monitor (e.g. 123456789, 987654321)",
    )]
    pub chats: Vec<String>,
}
