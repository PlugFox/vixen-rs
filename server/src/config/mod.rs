//! Configuration. Single source of truth for global bot settings — secrets,
//! connection URLs, bind address, log level, deployment environment, telegram
//! mode, auth defaults and DB pool sizing. Per-chat tunables live in the
//! `chat_config` table (see `server/docs/database.md`), NOT here.
//!
//! Naming and scope follow `server/docs/config.md`. Env precedence is:
//! CLI flag > `CONFIG_*` env > default.

use std::path::PathBuf;

use clap::Parser;

pub mod secrets;
pub use secrets::{AdminSecret, BotToken, JwtSecret, OpenAiKey};

#[derive(Parser, Debug, Clone)]
#[command(
    name = "vixen-server",
    version,
    about = "Telegram anti-spam bot — captcha gating, spam pipeline, daily reports.",
    long_about = "Single-tenant Telegram anti-spam service. \
                  Watches a fixed set of chats configured via CONFIG_CHATS; \
                  per-chat tunables live in the database, not here."
)]
pub struct Config {
    // ── Required: secrets and connection URLs ──
    /// Telegram bot token from @BotFather. Never logged.
    #[arg(long, env = "CONFIG_BOT_TOKEN")]
    pub bot_token: BotToken,

    /// PostgreSQL connection URL.
    #[arg(long, env = "CONFIG_DATABASE_URL")]
    pub database_url: String,

    /// Redis connection URL (mandatory from M0 — caches + pub/sub).
    #[arg(long, env = "CONFIG_REDIS_URL")]
    pub redis_url: String,

    /// Comma-separated Telegram chat IDs the bot watches.
    /// Negative IDs (`-100…`) for groups/supergroups.
    #[arg(long, env = "CONFIG_CHATS", value_delimiter = ',')]
    pub chats: Vec<i64>,

    // ── Operational ──
    /// HTTP bind address.
    #[arg(long, env = "CONFIG_ADDRESS", default_value = "0.0.0.0:8000")]
    pub address: String,

    /// Deployment environment. Affects logging defaults, OpenAPI UI exposure,
    /// error message detail, secret-presence enforcement.
    #[arg(long, env = "CONFIG_ENVIRONMENT", default_value = "dev",
          value_parser = ["dev", "staging", "prod"])]
    pub environment: String,

    /// Console log level (`trace` | `debug` | `info` | `warn` | `error`).
    /// Overridden by `RUST_LOG`.
    #[arg(long, env = "CONFIG_LOG_LEVEL", default_value = "info")]
    pub log_level: String,

    /// Directory for the rolling JSON file logger.
    #[arg(long, env = "CONFIG_LOG_DIR", default_value = "./logs")]
    pub log_dir: PathBuf,

    /// Mount the Scalar OpenAPI UI at `/scalar`. Default `true` in dev,
    /// `false` in staging/prod.
    #[arg(long, env = "CONFIG_OPENAPI_UI")]
    pub openapi_ui: Option<bool>,

    /// CORS allowed origins. **No wildcards.** Default: `http://localhost:3000`
    /// (dev dashboard). Production must override with the explicit dashboard
    /// origin; pass an empty value to disable cross-origin access entirely.
    #[arg(long, env = "CONFIG_CORS_ORIGINS", value_delimiter = ',',
          default_values_t = vec!["http://localhost:3000".to_string()])]
    pub cors_origins: Vec<String>,

    // ── Telegram bot mode ──
    /// `polling` (default; M0–M5) or `webhook` (M6+).
    #[arg(long, env = "CONFIG_TELEGRAM_MODE", default_value = "polling",
          value_parser = ["polling", "webhook"])]
    pub telegram_mode: String,

    /// Public HTTPS URL Telegram POSTs updates to. Required if `telegram_mode = webhook`.
    #[arg(long, env = "CONFIG_TELEGRAM_WEBHOOK_URL")]
    pub telegram_webhook_url: Option<String>,

    /// Validated against the `X-Telegram-Bot-Api-Secret-Token` header.
    /// Required if `telegram_mode = webhook`.
    #[arg(long, env = "CONFIG_TELEGRAM_WEBHOOK_SECRET")]
    pub telegram_webhook_secret: Option<JwtSecret>,

    // ── Auth ──
    /// Constant-time-compared bearer for `/admin/*`. Required in prod.
    #[arg(long, env = "CONFIG_ADMIN_SECRET")]
    pub admin_secret: Option<AdminSecret>,

    /// HS256 signing secret for dashboard JWTs (≥32 bytes). Required in prod.
    #[arg(long, env = "CONFIG_JWT_SECRET")]
    pub jwt_secret: Option<JwtSecret>,

    /// Dashboard JWT TTL in seconds.
    #[arg(long, env = "CONFIG_JWT_TTL_SECS", default_value_t = 3600)]
    pub jwt_ttl_secs: i64,

    /// Reject Telegram WebApp `initData` whose `auth_date` is older than this.
    #[arg(long, env = "CONFIG_INIT_DATA_MAX_AGE_SECS", default_value_t = 86_400)]
    pub init_data_max_age_secs: u64,

    // ── DB pool ──
    /// PostgreSQL pool maximum size.
    #[arg(long, env = "CONFIG_DB_MAX_CONNECTIONS", default_value_t = 50)]
    pub db_max_connections: u32,
    /// PostgreSQL pool minimum size.
    #[arg(long, env = "CONFIG_DB_MIN_CONNECTIONS", default_value_t = 5)]
    pub db_min_connections: u32,
    /// Acquire timeout (ms).
    #[arg(long, env = "CONFIG_DB_ACQUIRE_TIMEOUT_MS", default_value_t = 10_000)]
    pub db_acquire_timeout_ms: u64,
    /// Idle timeout (ms).
    #[arg(long, env = "CONFIG_DB_IDLE_TIMEOUT_MS", default_value_t = 600_000)]
    pub db_idle_timeout_ms: u64,
    /// Per-connection statement timeout (ms) — set via `SET LOCAL` on acquire.
    #[arg(long, env = "CONFIG_DB_STATEMENT_TIMEOUT_MS", default_value_t = 30_000)]
    pub db_statement_timeout_ms: u64,

    // ── Spam pipeline (M2) ──
    /// CAS API base URL. Override in tests to point at a wiremock server;
    /// production uses the public CAS endpoint.
    #[arg(
        long,
        env = "CONFIG_CAS_BASE_URL",
        default_value = "https://api.cas.chat"
    )]
    pub cas_base_url: String,

    /// Days a `spam_messages` row survives before the cleanup job prunes it.
    /// 14 days matches the Dart prototype default — long enough to catch
    /// long-tail recurrences, short enough to bound table growth.
    #[arg(long, env = "CONFIG_SPAM_RETENTION_DAYS", default_value_t = 14)]
    pub spam_retention_days: u32,

    // ── Reports (M3) ──
    /// Public dashboard / WebApp base URL used by `/report` to build the
    /// deep-link the bot replies with. No trailing slash.
    #[arg(
        long,
        env = "CONFIG_WEBAPP_BASE_URL",
        default_value = "http://localhost:3000"
    )]
    pub webapp_base_url: String,

    /// OpenAI Chat Completions base URL. Override in tests to point at a
    /// wiremock server; production uses the public OpenAI endpoint.
    #[arg(
        long,
        env = "CONFIG_OPENAI_BASE_URL",
        default_value = "https://api.openai.com"
    )]
    pub openai_base_url: String,
}

impl Config {
    /// Validate cross-field invariants. Run immediately after `Self::parse()`.
    pub fn validate(&self) -> Result<(), ConfigError> {
        // Bot-token format: `<digits>:<≥30 url-safe chars>`.
        let token_re =
            regex::Regex::new(r"^\d+:[A-Za-z0-9_-]{30,}$").expect("bot-token regex compiles");
        if !token_re.is_match(self.bot_token.expose()) {
            return Err(ConfigError::BadBotToken);
        }

        if self.chats.is_empty() {
            return Err(ConfigError::NoChats);
        }

        for origin in &self.cors_origins {
            if origin == "*" || origin.contains('*') {
                return Err(ConfigError::WildcardCors(origin.clone()));
            }
        }

        if self.environment == "prod" {
            match &self.jwt_secret {
                None => return Err(ConfigError::MissingJwtSecret),
                Some(s) if s.len() < 32 => return Err(ConfigError::JwtSecretTooShort(s.len())),
                Some(_) => {}
            }
            if self.admin_secret.is_none() {
                return Err(ConfigError::MissingAdminSecret);
            }
        }

        if self.telegram_mode == "webhook" {
            if self.telegram_webhook_url.is_none() {
                return Err(ConfigError::WebhookMissing("CONFIG_TELEGRAM_WEBHOOK_URL"));
            }
            if self.telegram_webhook_secret.is_none() {
                return Err(ConfigError::WebhookMissing(
                    "CONFIG_TELEGRAM_WEBHOOK_SECRET",
                ));
            }
        }

        if self.db_min_connections > self.db_max_connections {
            return Err(ConfigError::DbPoolInverted {
                min: self.db_min_connections,
                max: self.db_max_connections,
            });
        }

        Ok(())
    }

    /// Default for `CONFIG_OPENAPI_UI` when unset: on in dev, off elsewhere.
    pub fn resolve_openapi_ui(&self) -> bool {
        self.openapi_ui.unwrap_or(self.environment == "dev")
    }
}

#[derive(thiserror::Error, Debug)]
pub enum ConfigError {
    #[error("CONFIG_BOT_TOKEN format invalid (expected `<digits>:<≥30 url-safe chars>`)")]
    BadBotToken,
    #[error("CONFIG_CHATS must list at least one chat ID")]
    NoChats,
    #[error("CONFIG_CORS_ORIGINS contains a wildcard ({0:?}) — explicit origins only")]
    WildcardCors(String),
    #[error("CONFIG_JWT_SECRET is required in prod")]
    MissingJwtSecret,
    #[error("CONFIG_JWT_SECRET must be ≥32 bytes in prod (got {0})")]
    JwtSecretTooShort(usize),
    #[error("CONFIG_ADMIN_SECRET is required in prod")]
    MissingAdminSecret,
    #[error("{0} is required when CONFIG_TELEGRAM_MODE=webhook")]
    WebhookMissing(&'static str),
    #[error("CONFIG_DB_MIN_CONNECTIONS ({min}) cannot exceed CONFIG_DB_MAX_CONNECTIONS ({max})")]
    DbPoolInverted { min: u32, max: u32 },
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    /// Build a CLI argument vector with the four required flags pre-populated.
    /// Each override replaces the default; an empty override value drops the flag,
    /// which lets a test exercise the "absent" case for that variable.
    fn args(overrides: &[(&str, &str)]) -> Vec<String> {
        use std::collections::BTreeMap;
        let mut all: BTreeMap<&str, String> = BTreeMap::from([
            (
                "bot-token",
                "12345:abcdefghijklmnopqrstuvwxyz_0-9".to_string(),
            ),
            ("database-url", "postgresql://x:x@localhost/x".to_string()),
            ("redis-url", "redis://localhost:6379".to_string()),
            ("chats", "-1001,-1002".to_string()),
        ]);
        for (k, v) in overrides {
            all.insert(k, (*v).to_string());
        }
        let mut args = vec!["vixen-server".to_string()];
        for (k, v) in all {
            if !v.is_empty() {
                args.push(format!("--{k}={v}"));
            }
        }
        args
    }

    #[test]
    fn parses_minimum_required() {
        let cfg = Config::try_parse_from(args(&[])).expect("parses");
        assert_eq!(cfg.chats, vec![-1001, -1002]);
        assert_eq!(cfg.address, "0.0.0.0:8000");
        assert_eq!(cfg.environment, "dev");
        assert_eq!(cfg.telegram_mode, "polling");
        cfg.validate().expect("dev defaults validate");
    }

    #[test]
    fn rejects_bad_bot_token_format() {
        let cfg =
            Config::try_parse_from(args(&[("bot-token", "garbage")])).expect("clap parses string");
        assert!(matches!(cfg.validate(), Err(ConfigError::BadBotToken)));
    }

    #[test]
    fn rejects_empty_chats() {
        let cfg = Config::try_parse_from(args(&[("chats", "")])).expect("parses");
        assert!(matches!(cfg.validate(), Err(ConfigError::NoChats)));
    }

    #[test]
    fn prod_requires_jwt_and_admin_secrets() {
        let cfg = Config::try_parse_from(args(&[("environment", "prod")])).expect("parses");
        assert!(matches!(cfg.validate(), Err(ConfigError::MissingJwtSecret)));

        let cfg = Config::try_parse_from(args(&[
            ("environment", "prod"),
            ("jwt-secret", "abcdefghij1234567890ABCDEFGHIJ12"),
            ("admin-secret", "ops"),
        ]))
        .expect("parses");
        cfg.validate().expect("prod with both secrets passes");
    }

    #[test]
    fn webhook_requires_url_and_secret() {
        let cfg = Config::try_parse_from(args(&[("telegram-mode", "webhook")])).expect("parses");
        assert!(matches!(
            cfg.validate(),
            Err(ConfigError::WebhookMissing(_))
        ));
    }

    #[test]
    fn rejects_wildcard_cors() {
        let cfg = Config::try_parse_from(args(&[("cors-origins", "*")])).expect("parses");
        assert!(matches!(cfg.validate(), Err(ConfigError::WildcardCors(_))));
    }

    #[test]
    fn rejects_inverted_db_pool() {
        let cfg = Config::try_parse_from(args(&[
            ("db-max-connections", "5"),
            ("db-min-connections", "10"),
        ]))
        .expect("parses");
        assert!(matches!(
            cfg.validate(),
            Err(ConfigError::DbPoolInverted { .. })
        ));
    }
}
