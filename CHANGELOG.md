# Changelog

All notable changes are tracked here using the [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) format and adhere to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

The vixen-rs monorepo ships two artifacts with separate version numbers:

- **server** — Rust crate `vixen-server`, source in [`server/`](server/), versioned in `server/Cargo.toml`.
- **website** — TypeScript SPA `vixen-website`, source in [`website/`](website/), versioned in `website/package.json`.

Each release entry calls out the affected component(s) via a `(server)` / `(website)` / `(infra)` tag. Skip CHANGELOG updates only for trivial internal-only changes (formatting, comment tweaks, refactors with no behaviour change).

## [Unreleased]

### Added

- Full clap `Config` parser in `server/src/config/mod.rs` covering every `CONFIG_*` env var that M0–M5 needs (secrets, connection URLs, address, environment, log level / dir, OpenAPI UI gate, CORS origins, telegram mode + webhook pair, JWT TTL, init-data max age, DB pool sizing). Secret newtypes `BotToken`, `JwtSecret`, `AdminSecret`, `OpenAiKey` redact to `***redacted***` in `Display`/`Debug`; `Config::validate()` enforces token format, non-empty chats, no-wildcard CORS, prod-only JWT/admin secrets, webhook url+secret pair and DB pool ordering — startup exits 2 with a clear message on failure. `server/config/template.env` documents every variable. (server)
- `server/bin/server.rs` entry point: HTTP listener on `CONFIG_ADDRESS` (default `0.0.0.0:8000`), SIGINT + SIGTERM listener that fires a shared `CancellationToken`, 30s outer shutdown timeout. Module skeletons under `server/src/{api,telegram,services,jobs,models,database,config,telemetry,utils}/`; `server/build.rs` captures git short-SHA, build date, rust version, profile and target as compile-time env vars exposed via `server/src/build_info.rs`. (server)
- `server/assets/captcha/DejaVuSans.ttf` (DejaVu Fonts 2.37, permissive license) and `server/assets/captcha/CHANGELOG` — first captcha asset, will be loaded via `include_bytes!` by the M1 digit-pad renderer. Asset immutability rules and bump-on-change protocol are documented in `server/docs/captcha.md`. (server)
- `docker/docker-compose.yml` and `docker/.env.example` — local dev infrastructure: PostgreSQL 16 + Redis 7 with healthchecks, named volumes (`vixen_pg-data`, `vixen_redis-data`), explicit `name: vixen` project namespace to avoid collisions with other repos that nest compose under a `docker/` directory. (infra)
- 29 new Claude Code skills covering meta-workflow, server subsystems, website patterns, and infra. Index in [.claude/skills/README.md](.claude/skills/README.md). (infra)
  - **Meta workflow** (7): `plan-before-code`, `verifiable-goal`, `code-review-self`, `debug-systematically`, `change-impact-assessment`, `pr-description`, `find-external-skill`.
  - **Server foundations** (4): `transaction-discipline`, `tracing-spans`, `connection-pool-tuning`, `serde-strict-deserialization`.
  - **Server vixen subsystems** (8): `add-telegram-handler`, `add-slash-command`, `captcha-pipeline`, `spam-rule`, `background-job`, `tg-webapp-auth`, `per-chat-config`, `seed-test-chat`.
  - **Website patterns** (8): `solid-resource-pattern`, `solid-async-cleanup`, `typescript-discriminated-union`, `form-error-ux`, `loading-empty-error-states`, `responsive-breakpoints-telegram`, `interaction-states-kobalte`, `design-tokens-system`.
  - **Website vixen-specific** (1): `telegram-login-widget`.
  - **Infra** (1): `infra/github-actions-workflow`.

### Changed

- Updated 10 existing skills with research-derived additions: `server/sqlx-query` (keyset pagination, `ON CONFLICT`), `server/postgres-optimization` (lock strength), `server/rust-error-handling` (`#[from]`, Telegram `inspect_err`), `server/rust-async-tokio` (cancel-safety table), `server/rust-testing` (`sqlx::test` fixtures + corpus tests), `website/add-solid-component` (refs + cleanup), `website/add-i18n-string` (RU plurals + ICU braces), `website/design-anti-patterns` (OLED black, cursor-pointer, motion timing), `website/ui-accessibility` (touch targets + ARIA), `verify-changes` (`.sqlx/` staging + concurrency stress), `docker-multi-stage` (healthcheck + SQLX_OFFLINE), `commit-message` (breaking-change footer + co-author rules). (infra)
- Roadmap rewritten as M0–M8 (foundation → captcha → spam → reports → web auth/hot-reload → dashboard → public reports + WebApp → SQLite migration → prod webhook). Redis is now a mandatory dependency from M0 (hot caches + `chat_config:{chat_id}` pub/sub for live config reload). Most tunables move out of env vars into `chat_config` (PostgreSQL JSONB, edited from the dashboard). Captcha output switches from PNG to WebP. Daily reports gain MarkdownV2 pseudographics alongside the chart, conditionally emitted. Bot adds `/info <user>` and `/report` commands. (infra)

### Fixed

### Removed

### Security
