# Vixen-rs — Agent Guide

Telegram anti-spam bot. Single-tenant.

The bot watches a fixed set of chats configured at deploy time. New users in watched chats get a CAPTCHA challenge before they can speak. Spam is detected via xxhash dedup + Combot Anti-Spam API + heuristic n-gram phrase match. Daily PNG reports + optional OpenAI summaries are posted back into each watched chat. A SolidJS dashboard exposes the same data to moderators (Telegram-Login-authenticated) and a redacted public report to anyone.

## Repository Structure

```
server/     Rust backend (Axum, SQLx, PostgreSQL, teloxide, tracing)
website/    TypeScript frontend (SolidJS, Kobalte, Tailwind, Vite, bun, Biome)
docker/     Docker Compose (PostgreSQL, server, website)
docs/       Cross-cutting docs (architecture, deployment, features, roadmap)
.github/    CI/CD workflows
```

## Server Architecture

- **Framework**: Axum + Tower middleware.
- **Database**: PostgreSQL with SQLx (compile-time checked queries). Source of truth for everything, including per-chat configuration.
- **Cache + pub/sub**: Redis (deadpool-redis). In-memory Moka in front of Redis for hot lookups (verified-user list, chat_config, CAS verdicts). Redis pub/sub channel `chat_config:{chat_id}` invalidates Moka entries on write so config edits go live within ~1s without redeploy.
- **Bot driver**: teloxide. Polling in v1; webhook abstraction is in place so a switch later doesn't ripple through handlers.
- **Auth**: Telegram WebApp `initData` HMAC validation → short-lived internal JWT for the dashboard. One shared `CONFIG_ADMIN_SECRET` for `/admin/...` ops endpoints.
- **Tracing**: structured JSON to file (daily rotation, 7-day retention) + human-readable console.
- **Configuration**: env vars hold only secrets and connection URLs (`CONFIG_BOT_TOKEN`, `CONFIG_DATABASE_URL`, `CONFIG_REDIS_URL`, `CONFIG_JWT_SECRET`, `CONFIG_ADMIN_SECRET`, `CONFIG_BIND_ADDR`). All feature toggles, thresholds, captcha policy, report hour, OpenAI key/model live in `chat_config` and are edited from the dashboard.

### Server Code Layout

```
server/src/
├── api/          Routes, middleware, error handling
│   ├── server.rs            Router + middleware stack
│   ├── webapp_auth_middleware.rs   JWT validation, chat_ids extraction
│   ├── admin_secret_middleware.rs  Constant-time secret compare
│   ├── routes_*.rs          Endpoint handlers (auth, chats, moderation, reports, public)
│   └── response.rs          ApiResult<T> + response macros
├── telegram/     Telegram bot
│   ├── dispatcher.rs        teloxide Dispatcher + watched-chats filter
│   ├── handlers/            One file per update type / command group
│   └── commands.rs          BotCommands enum (/start, /help, /status, /verify, /ban, /unban, /stats)
├── services/     Business logic (no HTTP / Telegram concerns)
│   ├── captcha_service.rs   Atomic challenge issuance + solve
│   ├── spam_service.rs      Pipeline: normalize → hash → CAS → n-gram → decide
│   ├── chat_config_service.rs  Per-chat config read/write
│   ├── moderation_service.rs   Ban / unban / verify orchestration
│   ├── report_service.rs    Daily aggregates + chart rendering
│   ├── summary_service.rs   Optional OpenAI summary
│   ├── auth_service.rs      initData HMAC + JWT mint
│   ├── cas_client.rs        Combot Anti-Spam API
│   └── openai_client.rs     OpenAI Chat Completions
├── jobs/         Background tasks
│   ├── mod.rs               Scheduler + JobConfig registry
│   ├── daily_report.rs
│   ├── captcha_expiry.rs
│   ├── spam_cleanup.rs
│   ├── chat_info_refresh.rs
│   └── summary_generation.rs
├── models/       DB models (SQLx) + API DTOs (Serde)
├── database/     PgPool wrapper + SharedDatabase = Arc<Database>
├── config/       Config struct (clap Parser, CONFIG_* env vars)
├── telemetry/    tracing setup, span conventions, redaction helpers
└── utils/        Validation, normalization, redaction, time helpers
```

## Key Domain Concepts

- **Watched chats** — fixed list from `CONFIG_CHATS` env var; the bot ignores updates from any other chat. Server-side filter runs before any handler.
- **Verified user** — `(chat_id, user_id)` row in `verified_users`. Per-chat verification (one chat verified ≠ all chats verified). Verified users bypass anti-spam (modulo the optional `clown_chance` random reaction).
- **Captcha challenge** — `captcha_challenges` row with the rendered image bytes hash, the expected digit sequence, expiry timestamp, and `attempts_left`. Solve = transactional delete + verified_user insert + lift Telegram restriction.
- **Spam ledger** — every detected-spam message is hashed (xxh3-64 of normalized body) and stored in `spam_messages` keyed by hash, with `hit_count`, `first_seen`, `last_seen`. Subsequent identical messages from any user are O(1) detected. TTL 14 days.
- **Moderator** — Telegram user listed in `chat_moderators(chat_id, user_id)`. Used by the dashboard for write actions and by `/ban` / `/unban` slash commands.
- **Action ledger** — `moderation_actions` row per ban / unban / mute / delete / verify, attributable to either the bot (`actor_kind = 'bot'`) or a moderator (`actor_kind = 'moderator'`). Uniqueness key `(chat_id, target_user_id, action, message_id)` makes spam detection idempotent.
- **Per-chat config** — `chat_config(chat_id, settings JSONB)` is the source of truth for tunables. Service reads go through a Moka cache; writes commit to Postgres and publish on Redis `chat_config:{chat_id}` to invalidate caches in every replica.

## Development

```bash
# Dependencies (Postgres + Redis)
docker compose -f docker/docker-compose.yml up -d postgres redis

# Database
cd server && sqlx migrate run

# Server (HTTP + bot polling)
cd server && cargo run

# Validate a Telegram WebApp initData payload (M4+)
cd server && cargo run --example tg-init-validate -- "$INIT_DATA"

# Website (http://localhost:3000)
cd website && bun install && bun run dev
```

**Environment**: see `server/config/template.env` for all `CONFIG_*` variables.

## Documentation Map

Feature docs in `server/docs/`:

| File | Content |
|------|---------|
| architecture.md | Tech stack, code layout, services, configuration, logging, DB pool |
| bot.md | teloxide dispatcher, watched-chats filter, handler routing, slash-command table |
| captcha.md | Image generation, atomicity contract, expiry, refresh, asset immutability |
| spam-detection.md | Normalize → xxh3 dedup → CAS → n-gram → decide; idempotency rules |
| moderation.md | Manual ban/unban/verify (slash + dashboard), action ledger |
| reports.md | Daily aggregates, plotters chart, optional OpenAI summary |
| api.md | All endpoints (link to OpenAPI), auth requirements, error codes |
| auth.md | WebApp initData HMAC validation, JWT mint, threat model |
| database.md | Schema, indexes, triggers, connection pool |
| background-jobs.md | Scheduler pattern, idempotency, cancellation, observability |
| config.md | Every CONFIG_* env var |
| observability.md | tracing spans, redaction policy, planned metrics |

LLM rules in `server/docs/rules/`:

| File | When to read |
|------|-------------|
| rust.md | Before writing or modifying Rust code |
| migrations.md | Before creating database migrations |
| api-routes.md | Before adding or modifying API endpoints |
| error-handling.md | Before implementing error handling |
| telegram-handlers.md | Before adding a teloxide handler |
| background-jobs.md | Before adding a periodic task |
| testing.md | Before adding tests |

LLM rules in `website/docs/rules/`:

| File | When to read |
|------|-------------|
| solidjs.md | Before writing SolidJS components |
| typescript.md | Before writing or modifying TypeScript |
| components.md | Before creating a UI primitive |
| styling.md | Before writing Tailwind classes / theme tokens |

## Conventions

- **Language**: English for all code, comments, documentation, and commit messages. Russian only when chatting with the user.
- **Rust**: `cargo fmt` + `cargo clippy --all-targets --all-features -- -D warnings` before commits.
- **SQL**: Migrations with sequential timestamps via SQLx CLI. Telegram IDs are `BIGINT NOT NULL`.
- **API**: All endpoints documented with `#[utoipa::path(...)]`. Telegram handlers don't need utoipa.
- **Git**: Conventional commits (feat, fix, refactor, docs, chore).

## Mandatory Rules

1. When modifying API endpoints → update `server/docs/api.md`.
2. When modifying database schema → update `server/docs/database.md` and create a migration.
3. When modifying auth flow → update `server/docs/auth.md`.
4. When adding/changing a feature → update the corresponding `server/docs/` file.
5. Before writing a migration → read `server/docs/rules/migrations.md`.
6. Before adding an API route → read `server/docs/rules/api-routes.md`.
7. Do not add marketing language or filler to documentation — only facts and useful information.
8. After completing any user-visible change, add an entry to the root [`CHANGELOG.md`](CHANGELOG.md) under `[Unreleased]` with a `(server)` / `(website)` / `(infra)` tag.
9. When modifying captcha generation or assets → update `server/docs/captcha.md` AND add an entry to `server/assets/captcha/CHANGELOG` (asset version log). Never overwrite an existing asset.
10. When adding a slash command → register it both in the `BotCommands` enum / dispatcher AND in `server/docs/bot.md`'s command table.
