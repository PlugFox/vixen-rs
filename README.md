# Vixen-rs

Single-tenant Telegram anti-spam bot. Rust backend (Axum + sqlx + teloxide), SolidJS dashboard, PostgreSQL + Redis.

Vixen watches a fixed set of Telegram chats, gates new members behind a CAPTCHA, removes spam through xxhash deduplication + Combot Anti-Spam + n-gram phrase matching, and posts a daily aggregated report back into each chat. Moderators run the bot through slash commands and a Telegram-Login-authenticated dashboard; every watched chat also has a redacted public report page.

This repository is a ground-up rewrite of the Dart prototype at [github.com/PlugFox/vixen](https://github.com/PlugFox/vixen). The rewrite trades a single-language project for a typed monorepo with a real moderation UI, live-reload configuration, and Postgres persistence — see [docs/roadmap.md](docs/roadmap.md) for the milestone plan.

## Status

Active development. Implementation is staged across nine milestones (M0 through M8). See the [issues board](https://github.com/PlugFox/vixen-rs/issues) for what's in flight and the [roadmap](docs/roadmap.md) for the done-criteria. There is no released version yet; first tagged release is `1.0.0` at the end of M8.

## Headline features

- **CAPTCHA gate.** Every unverified non-admin user is shown a deterministic WebP digit-pad on an inline keyboard. The bot never restricts, mutes, or kicks — instead, all of an unverified user's messages are silently deleted and a fresh captcha is posted. They keep their chat membership and can keep retrying — third attempt, thirtieth, or never. Captcha assets are versioned and immutable to keep deterministic re-rendering tests sound.
- **Zero-FP spam ban.** The bot bans on one signal only: a long-enough message recurring as the first pre-captcha message across multiple distinct accounts (xxh3-64 fingerprint of the gate's deletion stream). No CAS verdicts, no n-gram heuristics, no behavioural rules — just a signature no honest user can produce. Idempotent under retry: every action goes through one `moderation_actions` ledger keyed `(chat_id, target_user_id, action, message_id)`, so retried updates never double-ban.
- **Daily reports.** Per chat, at the configured local hour: a Telegram MarkdownV2 message with ASCII pseudographics + counts + redacted top phrases + (optional) OpenAI summary, plus a WebP chart with a caption. Skipped automatically when activity is below threshold.
- **Hot-reload configuration.** Most tunables — captcha policy, spam thresholds, report hour and timezone, OpenAI key and model, language — live in `chat_config` JSONB and are edited from the dashboard. Writes publish on Redis pub/sub `chat_config:{chat_id}`; in-process Moka caches invalidate within ~1s. No redeploy.
- **Moderator dashboard.** Telegram Login or Telegram WebApp authenticated. Per-chat verified users, banned users, audit log, and config form. Bans, unbans, and verifies straight from the UI.
- **Public chat report.** Indexable `/report/{chat_slug}` page with redacted aggregates: URLs, @mentions, and phone numbers replaced before publication. Same data is served to the in-Telegram WebApp opened by `/report`.
- **Bot commands.** `/start`, `/help`, `/status`, `/stats`, `/verify`, `/ban`, `/unban`, `/info <user>`, `/report`.

## How spam prevention works

One design principle: **never punish on speculation.** The bot never mutes, restricts, or kicks. Bans fire only on a signal no honest user produces. The worst-case for a borderline user is "your message disappeared", never "you can't speak" or "you're out".

1. **Captcha gate (M1).** Every unverified non-admin user is shown a 4-digit captcha with an on-screen keyboard. Until they solve it, every message they send is silently deleted and a fresh captcha is posted. They are never muted; they keep their chat membership and can keep trying — third attempt, thirtieth, or never. Failing the captcha is not a ban trigger; ignoring it isn't either. The captcha just sits there, gating their messages.

2. **Hash-fingerprint of the deletion stream (M2).** Every message deleted by the gate is normalised (whitespace collapsed, casing folded, zero-width filtered) and hashed (xxh3-64). The bot tracks each unverified user's *first* pre-captcha message. When the same long-enough message recurs as the first pre-captcha message across multiple distinct accounts, the bot bans those accounts — spam bots reuse copy, real users don't.

3. **What never triggers a ban.** Failing captcha. Ignoring captcha. A single message, however off-topic. A short message that collides with someone else's. New-account or odd-hour heuristics. Third-party blocklists. No false positives by construction: the only behaviour that matches the ban signature is "send the same long template from several accounts before any of them pass the gate", which is exactly what spam-bot operators do — and which honest users have no reason to do.

4. **Why this is enough.** Even a spam message that slips past the fingerprint (unique copy per account, or short copy below the length floor) was already deleted by the captcha gate. The fingerprint upgrades a campaign-scale incident from "every message vanishes silently" to "the accounts behind the campaign are gone". The hand-off between (1) and (2) means the chat never sees spam content regardless of which path catches it.

The captcha gate ships in M1; the cross-account hash-collision ban lands in M2. See [docs/roadmap.md](docs/roadmap.md) for milestone done-criteria.

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

Toolchain (bun, sqlx-cli, taplo, jq, yq) is pinned via [mise](https://mise.jdx.dev); Rust is pinned via `rustup` through [server/rust-toolchain.toml](server/rust-toolchain.toml). Docker is the only host-level prerequisite.

```bash
# 0. One-time: install all pinned tools (skips Rust — rustup picks it up).
mise install

# 1. Boot Postgres + Redis, apply migrations, refresh .sqlx/.
mise run db:up
mise run db:migrate

# 2. Configure (only secrets and connection URLs go in env).
cp server/config/template.env server/.env
$EDITOR server/.env   # set CONFIG_BOT_TOKEN, CONFIG_DATABASE_URL, CONFIG_REDIS_URL

# 3. Run server (HTTP + bot polling on http://localhost:8000).
mise run server:run

# 4. Run dashboard (separate terminal, http://localhost:3000).
mise run website:install
mise run website:dev
```

`mise tasks` lists every wrapper; the full set lives in [mise.toml](mise.toml). The Claude Code slash commands (`/server-check`, `/website-check`, `/db-up`, `/db-migrate`, `/bot-token`) call the underlying `cargo` / `bun` directly and work without mise installed — pick whichever entry-point fits your shell.

Without mise, the equivalent setup is:

```bash
cargo install sqlx-cli --no-default-features --features postgres,rustls
docker compose -f docker/docker-compose.yml up -d postgres redis
cd server && sqlx migrate run && cargo sqlx prepare -- --all-targets
cargo run                              # server
cd ../website && bun install && bun run dev   # dashboard
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
| M1 | CAPTCHA gate (WebP) | WebP digit-pad + delete-then-reissue gate + solve / expire flow + `/verify` (no restrict, no kick) |
| M2 | Zero-FP spam ban | xxh3 fingerprint of pre-captcha first messages → cross-account ban + idempotent ledger + `/ban`, `/unban` |
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
