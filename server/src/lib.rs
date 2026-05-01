//! `vixen-server` — Telegram anti-spam bot.
//!
//! Subsystems are wired together by `bin/server.rs`. Most module bodies are
//! placeholders during M0 and are populated by later issues:
//!
//! - `config`     full clap parser + secret newtypes (#22)
//! - `telemetry`  rolling-file appender + `RedactedToken` (#23)
//! - `database`   Postgres pool + Redis pool + pub/sub (#21)
//! - `api`        Axum router + `/health` + `/about` + OpenAPI (#24)
//! - `telegram`   teloxide dispatcher + watched-chats filter (#25)
//! - `services`, `jobs`, `models`, `utils` populated from M1 onwards.

pub mod api;
pub mod build_info;
pub mod config;
pub mod database;
pub mod jobs;
pub mod models;
pub mod services;
pub mod telegram;
pub mod telemetry;
pub mod utils;
