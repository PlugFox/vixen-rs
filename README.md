# Vixen-rs

Single-tenant Telegram anti-spam bot. Rust backend (Axum + sqlx + teloxide), SolidJS dashboard, PostgreSQL + Redis.

Vixen watches a fixed set of Telegram chats, gates new members behind a CAPTCHA, removes spam through xxhash deduplication + Combot Anti-Spam + n-gram phrase matching, and posts a daily aggregated report back into each chat. Moderators run the bot through slash commands and a Telegram-Login-authenticated dashboard; every watched chat also has a redacted public report page.

This repository is a ground-up rewrite of the Dart prototype at [github.com/PlugFox/vixen](https://github.com/PlugFox/vixen). The rewrite trades a single-language project for a typed monorepo with a real moderation UI, live-reload configuration, and Postgres persistence — see [docs/roadmap.md](docs/roadmap.md) for the milestone plan.

## Status

Active development. Implementation is staged across nine milestones (M0 through M8). See the [issues board](https://github.com/PlugFox/vixen-rs/issues) for what's in flight and the [roadmap](docs/roadmap.md) for the done-criteria. There is no released version yet; first tagged release is `1.0.0` at the end of M8.

## Headline features

- **CAPTCHA on join.** Every new member is restricted, presented with a WebP digit-pad on an inline keyboard, and only unrestricted on a correct solve. Wrong solves decrement attempts; expiry kicks (does not ban). Captcha assets are versioned and immutable to keep deterministic re-rendering tests sound.
- **Spam pipeline.** `normalize → xxh3-64 dedup → CAS lookup → n-gram phrase match → ban / delete`. Idempotent under retry: every action goes through one `moderation_actions` ledger keyed `(chat_id, target_user_id, action, message_id)`, so retried updates never double-ban.
- **Daily reports.** Per chat, at the configured local hour: a Telegram MarkdownV2 message with ASCII pseudographics + counts + redacted top phrases + (optional) OpenAI summary, plus a WebP chart with a caption. Skipped automatically when activity is below threshold.
- **Hot-reload configuration.** Most tunables — captcha policy, spam thresholds, report hour and timezone, OpenAI key and model, language — live in `chat_config` JSONB and are edited from the dashboard. Writes publish on Redis pub/sub `chat_config:{chat_id}`; in-process Moka caches invalidate within ~1s. No redeploy.
- **Moderator dashboard.** Telegram Login or Telegram WebApp authenticated. Per-chat verified users, banned users, audit log, and config form. Bans, unbans, and verifies straight from the UI.
- **Public chat report.** Indexable `/report/{chat_slug}` page with redacted aggregates: URLs, @mentions, and phone numbers replaced before publication. Same data is served to the in-Telegram WebApp opened by `/report`.
- **Bot commands.** `/start`, `/help`, `/status`, `/stats`, `/verify`, `/ban`, `/unban`, `/info <user>`, `/report`.

## Stack

| Layer | Choice |
|---|---|
| Runtime | Tokio, single process, four cooperating tasks under one `CancellationToken` |
| HTTP | Axum + Tower middleware |
| Bot driver | teloxide (long-polling in v1, webhook in M8) |
| Database | PostgreSQL via sqlx with compile-time-checked queries |
| Cache + pub/sub | Redis (deadpool-redis) with Moka in-process front |
| Image stack | image, ab_glyph, plotters, webp |
| Auth | Telegram WebApp `initData` HMAC-SHA256 → short-lived JWT |
| Frontend | SolidJS + Kobalte + Tailwind, built with Vite, packaged by bun |
| Lint / format | Rust: `cargo fmt` + `cargo clippy -D warnings`; TypeScript: Biome |
| Test | `cargo test` with `sqlx::test` fixtures + spam-corpus YAML samples |
| CI | GitHub Actions: `server-ci`, `website-ci`, image build on tag |
| Observability | `tracing` console + JSON file (daily rotation, 7-day retention) |

## Architecture in one paragraph

A single Rust process owns the HTTP server (Axum), the Telegram dispatcher (teloxide), the background-job runner, and a Redis subscribe loop, all sharing one runtime and one `CancellationToken`. PostgreSQL is the source of truth for everything, including per-chat configuration in `chat_config` (JSONB). Redis fronts hot caches (CAS verdicts, verified-users, chat config) and broadcasts cache-invalidation messages on writes so config edits go live within a second across replicas. The website is a static SolidJS SPA served separately; it talks only to `/api/v1/*`. Full diagram and data-flow walkthroughs in [docs/architecture.md](docs/architecture.md).

## Quick start (local development)

Requires Docker, Rust ≥ 1.85 (edition 2024), bun, and `sqlx-cli`:

```bash
cargo install sqlx-cli --no-default-features --features postgres
```

Then:

```bash
# 1. Boot Postgres + Redis
docker compose -f docker/docker-compose.yml up -d postgres redis

# 2. Apply migrations + cache offline queries
cd server
sqlx migrate run
cargo sqlx prepare --workspace

# 3. Configure (only secrets and connection URLs go in env)
cp config/template.env .env
$EDITOR .env  # set CONFIG_BOT_TOKEN, CONFIG_DATABASE_URL, CONFIG_REDIS_URL

# 4. Run server (HTTP + bot polling on http://localhost:8000)
cargo run

# 5. Run dashboard (separate terminal, http://localhost:3000)
cd ../website
bun install
bun run dev
```

Health check: `curl localhost:8000/health` → `{ "db": "up", "redis": "up", "status": "ok" }`. OpenAPI spec: `curl localhost:8000/api/v1/openapi.json`.

## Configuration

Anything that is **not** a secret or a connection URL lives in PostgreSQL and is edited from the dashboard. The env file holds:

| Variable | Purpose |
|---|---|
| `CONFIG_BOT_TOKEN` | Telegram bot token (redacted in logs via `RedactedToken`) |
| `CONFIG_DATABASE_URL` | Postgres connection string |
| `CONFIG_REDIS_URL` | Redis connection string |
| `CONFIG_BIND_ADDR` | HTTP listener address (default `0.0.0.0:8000`) |
| `CONFIG_JWT_SECRET` | Dashboard JWT signing key |
| `CONFIG_ADMIN_SECRET` | Constant-time-compared secret for `/admin/*` endpoints |
| `CONFIG_TELEGRAM_MODE` | `polling` (default) or `webhook` |
| `CONFIG_TELEGRAM_WEBHOOK_SECRET` | Required when `mode=webhook` |
| `CONFIG_WATCHED_CHATS` | Comma-separated chat IDs the bot serves |
| `CONFIG_LOG_LEVEL`, `CONFIG_LOG_DIR` | Tracing knobs |

Per-chat tunables — captcha policy, spam thresholds, report hour and timezone, OpenAI key and model, locale — are stored in `chat_config` (JSONB) and changed live from the dashboard. There is no env var for any of them, by design.

## Repository structure

```
server/      Rust backend — Axum + sqlx + teloxide + tracing + plotters
website/     SolidJS dashboard + public report page
docker/      Docker Compose (postgres, redis); prod compose lands in M8
docs/        Cross-cutting docs (architecture, deployment, roadmap, features)
.claude/     Project-shared Claude Code skills, slash commands, hooks
.old/        Dart-prototype source kept as a reference during the rewrite
```

Per-component docs:

- Server feature docs: [server/docs/](server/docs/)
- Server LLM rules (read these before contributing): [server/docs/rules/](server/docs/rules/)
- Website feature docs: [website/docs/](website/docs/)
- Website LLM rules: [website/docs/rules/](website/docs/rules/)
- Architecture: [docs/architecture.md](docs/architecture.md)
- Deployment: [docs/deployment.md](docs/deployment.md)
- Roadmap: [docs/roadmap.md](docs/roadmap.md)
- Backlog (post-M8): [docs/features.md](docs/features.md)
- Changelog: [CHANGELOG.md](CHANGELOG.md)

## Roadmap

| | Milestone | Ships |
|---|---|---|
| M0 | Foundation & infra | Server skeleton, Postgres + Redis, `/health`, polling stub, CI |
| M1 | CAPTCHA pipeline (WebP) | Restrict + WebP digit-pad + solve / expire flow + `/verify` |
| M2 | Spam pipeline | xxh3 dedup + CAS + n-gram + idempotent ledger + `/ban`, `/unban` |
| M3 | Reports | Daily MarkdownV2 + WebP chart + optional OpenAI summary + `/stats`, `/report` stub |
| M4 | Web foundation | Telegram WebApp HMAC + JWT + chat_config CRUD + Redis invalidation |
| M5 | Moderator dashboard | SolidJS app: auth, settings form, audit log, ban/unban/verify, `/info` |
| M6 | Public reports + WebApp | `/report/{slug}` redacted indexable page + WebApp embed |
| M7 | Data migration | One-shot SQLite → Postgres for verified_users + spam dictionary |
| M8 | Production deploy | Webhook mode, prod compose, TLS, backups, 1.0.0 release |

Every milestone has a single user-visible done-criterion in [docs/roadmap.md](docs/roadmap.md). Detailed task lists live in GitHub Issues, tagged by milestone.

## Project commands

If you use Claude Code in this repo, the following slash commands are pre-wired (see [.claude/commands/](.claude/commands/)):

| Command | Action |
|---|---|
| `/server-check` | `cargo fmt` + `clippy -D warnings` + `cargo test` + `sqlx prepare --check` |
| `/website-check` | Biome lint + `tsc --noEmit` + `bun run build` |
| `/db-up` | Start the local Postgres + Redis stack |
| `/db-migrate` | Apply migrations and refresh the offline `.sqlx/` cache |
| `/bot-token` | Verify the configured Telegram token via `getMe` (token never echoed) |
| `/tg-init-debug` | Validate a Telegram WebApp `initData` payload (HMAC, expiry, decoded fields) |

The same checks run in CI; nothing prevents running them by hand.

## Contributing

PRs welcome. Conventional Commits (`feat`, `fix`, `refactor`, `docs`, `chore`, `test`, `perf`, `ci`). Before opening a PR:

- `server/` changes — run `/server-check`. Schema changes also need [server/docs/rules/migrations.md](server/docs/rules/migrations.md). New routes: [server/docs/rules/api-routes.md](server/docs/rules/api-routes.md). New handlers: [server/docs/rules/telegram-handlers.md](server/docs/rules/telegram-handlers.md).
- `website/` changes — run `/website-check`. New components: [website/docs/rules/components.md](website/docs/rules/components.md). New SolidJS code: [website/docs/rules/solidjs.md](website/docs/rules/solidjs.md).
- User-visible changes — add a `[Unreleased]` entry to [CHANGELOG.md](CHANGELOG.md), tagged `(server)` / `(website)` / `(infra)`.

Project-wide LLM agent conventions: [AGENTS.md](AGENTS.md) and [CLAUDE.md](CLAUDE.md). They apply equally to other agentic tools.

## License

MIT — same as the Dart prototype at [github.com/PlugFox/vixen](https://github.com/PlugFox/vixen). The `LICENSE` file will be reinstated before the 1.0.0 release.
