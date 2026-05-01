# Server Architecture

## Tech stack

- Rust 2024 edition
- Axum + Tower (HTTP)
- teloxide (Telegram Bot API)
- SQLx with compile-time-checked queries (PostgreSQL 15+)
- tokio async runtime, tokio-util `CancellationToken`
- tracing + tracing-subscriber (JSON file rotation + human console)
- clap Parser for config (env var prefix `CONFIG_*`)
- moka in-memory cache (verified-user list, chat config, CAS verdicts)
- image + ab_glyph for CAPTCHA rendering
- plotters for daily-report charts
- utoipa + utoipa-scalar for OpenAPI / Scalar UI
- jsonwebtoken (HS256) with `aws_lc_rs` backend (avoids RUSTSEC-2023-0071)
- reqwest (rustls, no openssl) for outbound HTTP (CAS, OpenAI)

## Code layout

```
server/
├── bin/
│   └── server.rs           # Entry: load config → init tracing → spawn HTTP + bot + jobs → wait on shutdown
├── src/
│   ├── lib.rs              # Re-exports for integration tests
│   ├── api/                # Axum HTTP layer
│   │   ├── server.rs               # Router + middleware stack
│   │   ├── webapp_auth_middleware.rs   # JWT validation + chat_ids extraction
│   │   ├── admin_secret_middleware.rs  # Constant-time secret compare
│   │   ├── pub_rate_limit_middleware.rs
│   │   ├── routes_auth.rs          # POST /auth/telegram/login, GET /auth/me, POST /auth/logout
│   │   ├── routes_chats.rs         # Watched chats list + detail
│   │   ├── routes_moderation.rs    # Ban/unban/verify, action ledger
│   │   ├── routes_reports.rs       # Per-chat report queries (auth)
│   │   ├── routes_public.rs        # /report/{slug}, /report/{slug}/chart.png (no auth)
│   │   ├── routes_admin.rs         # /admin/* (admin secret)
│   │   ├── routes_health.rs        # /health, /about
│   │   └── response.rs             # ApiResult<T> + macros
│   ├── telegram/           # teloxide
│   │   ├── dispatcher.rs           # dptree wiring + watched-chats filter
│   │   ├── commands.rs             # BotCommands derive
│   │   └── handlers/               # One file per concern
│   │       ├── member_update.rs    # ChatMemberUpdated → captcha
│   │       ├── messages.rs         # Message → spam pipeline
│   │       ├── captcha.rs          # CallbackQuery → solve / refresh
│   │       └── commands.rs         # /start /help /status /verify /ban /unban /stats
│   ├── services/           # Business logic — no HTTP / Telegram concerns
│   │   ├── captcha_service.rs
│   │   ├── spam_service.rs
│   │   ├── chat_config_service.rs
│   │   ├── moderation_service.rs
│   │   ├── report_service.rs
│   │   ├── summary_service.rs
│   │   ├── auth_service.rs         # initData HMAC + JWT mint
│   │   ├── cas_client.rs           # Combot Anti-Spam
│   │   └── openai_client.rs
│   ├── jobs/               # Periodic tasks
│   │   ├── mod.rs                  # Registry + spawn_all
│   │   ├── captcha_expiry.rs
│   │   ├── spam_cleanup.rs
│   │   ├── chat_info_refresh.rs
│   │   ├── daily_report.rs
│   │   └── summary_generation.rs
│   ├── models/             # DB structs (sqlx::FromRow) + API DTOs (Serde)
│   ├── database/
│   │   └── db.rs                   # Database wrapping PgPool, SharedDatabase = Arc<Database>
│   ├── config/
│   │   └── mod.rs                  # clap Parser, CONFIG_* prefix
│   ├── telemetry/                  # tracing setup, span conventions
│   └── utils/
│       ├── redact.rs               # RedactedToken newtype
│       ├── normalize.rs            # text normalization for spam dedup
│       ├── cursor.rs               # Cursor-based pagination
│       └── validation.rs
├── assets/
│   └── captcha/                    # Immutable fonts; CHANGELOG entry per addition
├── config/
│   └── template.env                # Documented CONFIG_* vars
├── migrations/             # SQLx CLI format: YYYYMMDDHHMMSS_*.sql + .down.sql
├── tests/                  # Integration tests (`use vixen_server::*`)
│   ├── spam_corpus/                # YAML corpus per spam rule
│   └── fixtures/                   # SQL fixtures for #[sqlx::test]
├── examples/                # Stand-alone helpers (e.g. tg-init-validate)
├── .sqlx/                  # SQLx offline cache — committed, refreshed via cargo sqlx prepare
├── Cargo.toml
├── Cargo.lock
├── rustfmt.toml            # max_width=120, reorder_imports=true
└── clippy.toml             # cognitive_complexity=30, allow_unwrap_in_tests=true
```

## How the three workers coexist

`bin/server.rs` does:

1. Parse config (clap).
2. Init tracing (`telemetry::init`).
3. Open `PgPool`. Run pending migrations via `sqlx::migrate!()`.
4. Build `AppState { pool, config, caches, bot, captcha, spam, moderation, ... }` and wrap in `Arc`.
5. Build a `tokio_util::sync::CancellationToken`.
6. Spawn:
   - `tokio::spawn(run_http_server(state.clone(), shutdown.clone()))`.
   - `tokio::spawn(run_telegram_dispatcher(state.clone(), shutdown.clone()))`.
   - For each registered job: `tokio::spawn(job::run(state.clone(), shutdown.clone()))`.
7. Listen for SIGTERM / Ctrl+C → `shutdown.cancel()`.
8. `join!` all handles with a 30s outer timeout. Force-exit if anything hangs.

All three share the same `PgPool` (50 max connections), the same Moka caches, and the same `Bot` handle.

## Middleware stack (HTTP, applied in reverse — outermost first)

1. `CatchPanicLayer` — turns a panic into a 500 (and logs).
2. `TraceLayer::new_for_http()` — request span, latency, status.
3. `TimeoutLayer` — 30s default per request.
4. `CorsLayer` — restricted to `CONFIG_CORS_ORIGINS`.
5. `pub_rate_limit_middleware` — only on public routes.
6. `webapp_auth_middleware` or `admin_secret_middleware` — only on protected routes.
7. Per-route handler.

## Configuration

All env vars are prefixed `CONFIG_`. See [config.md](config.md) for the exhaustive list. clap loads from env first, then optionally from a `.env` file via `dotenvy`. Required: `CONFIG_DATABASE_URL`, `CONFIG_BOT_TOKEN`, `CONFIG_CHATS`. The bot refuses to start if any required var is missing.

## Logging

- **Console**: human-readable, level from `CONFIG_LOG_LEVEL` (`info` default).
- **File**: JSON, daily rotation in `CONFIG_LOG_DIR`, 7-day retention.
- **Span fields**: `update_id` / `chat_id` / `user_id` for bot events; `request_id` / `route` / `user_id` for HTTP; `job` for background jobs.
- **Redaction**: `RedactedToken` for bot tokens; `initData` only at debug level; user PII (names, message bodies) opt-in only.

## Database pool

- `PoolOptions`: max 50, min 5, acquire timeout 10s, idle timeout 600s.
- `statement_timeout = 30s` set per connection on acquire.
- Wrapped in `Database { pool: PgPool }`, accessed as `Arc<Database>` from `AppState`.

## Caching

Moka in-memory caches in `AppState`:

- **Verified users** — `(chat_id, user_id) → bool`, 10min TTL. Invalidated on verify / unverify.
- **Chat config** — `chat_id → Arc<ChatConfig>`, 5min TTL. Invalidated on update.
- **CAS verdicts** — `user_id → bool`, 1h TTL.

Don't cache user state more aggressively than the time it takes a moderator to react (~minutes). Cached invalidation is correctness-critical.

## What lives where, summary

- HTTP-only concerns (CORS, JWT, rate limiting): `src/api/`.
- Telegram-only concerns (dispatcher, command parsing, handlers): `src/telegram/`.
- Pure business logic (used by both HTTP and Telegram entry points): `src/services/`.
- Periodic tasks: `src/jobs/`.
- Data shape: `src/models/`.
- Database access: `src/database/`.
