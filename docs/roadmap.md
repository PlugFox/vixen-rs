# Vixen-rs — Roadmap

Concrete milestones from "empty repo" to "bot live in prod with dashboard". Each milestone has a single user-visible done-criterion and a short list of issue seeds. Detailed task tracking lives in GitHub Issues (one per seed); this file is the durable source of truth for the milestone story.

## Architectural shifts vs. the Dart prototype

- **PostgreSQL + Redis** from M0. Redis is mandatory (hot caches + pub/sub for hot-reload). Single Rust process holds Axum + teloxide dispatcher + job runner.
- **Most configuration lives in PostgreSQL**, not env vars. Env keeps only secrets and bind/db/redis URLs. Per-chat tunables (captcha policy, spam thresholds, report hour, OpenAI key/model, language) are stored in `chat_config` JSONB and edited from the dashboard. Writes publish on Redis pub/sub `chat_config:{chat_id}`; in-process Moka cache invalidates on subscribe — no redeploy.
- **Captcha** rendered as **WebP** with a polished visual style (gradients, varied fonts/colors, light noise). Digit-pad mode only for v1; picture-pick / math live in the backlog.
- **Daily reports** are conditionally posted in two messages per chat: a Telegram **MarkdownV2** message with ASCII pseudographics + counts + top phrases + (optional) OpenAI summary, plus a **WebP chart** with a caption. Skipped when activity is below threshold.
- **Bot adds `/info <user>`** (DM-only history lookup) **and `/report`** (deep-link into the public WebApp report for the chat).
- **Public report page** (`/report/{chat_slug}`) is in scope, indexable, redacted (URLs / @mentions / phone numbers replaced).
- **SQLite → PostgreSQL data migration** is a dedicated milestone before prod cutover; migrates `verified_users` and the xxhash spam dictionary only.

## M0 — Foundation & infra

**Done when**: `cargo run` boots server with HTTP + bot poller + job runner against Postgres + Redis (docker-compose). `/health` returns 200. Polling logs every update from watched chats; no handlers yet. Server CI green.

- `server/Cargo.toml` with axum, sqlx, teloxide, tracing, clap, moka, redis, image (webp), ab_glyph, plotters, utoipa, xxhash-rust, jsonwebtoken
- `server/src/{api,telegram,services,jobs,models,database,config,telemetry,utils}/mod.rs` skeletons + `bin/server.rs` boot (one runtime, three top-level tasks, shared `CancellationToken`)
- `server/migrations/000_initial_schema.sql`: `chats`, `chat_config`, `chat_moderators`, `verified_users`, `captcha_challenges`, `spam_messages`, `moderation_actions`, `report_messages`, `daily_stats`, `chat_info_cache`
- `server/src/database/redis.rs`: pooled async client, healthcheck, pub/sub helper
- `server/config/template.env` enumerating every `CONFIG_*` (only secrets and bind/db/redis URLs)
- `tracing` setup: human console + JSON file rotation + `RedactedToken` newtype
- `/health` (DB+Redis ping) and `/about` (version, build sha) routes with `utoipa`
- Watched-chats filter polling stub: log every update, no handlers
- `docker/docker-compose.yml` with `postgres` + `redis` services, healthchecks, named volumes
- `.github/workflows/server-ci.yml`: fmt + clippy `-D warnings` + test + `sqlx prepare --check`
- Carry `DejaVuSans.ttf` from `.old/src/captcha/assets/` to `server/assets/captcha/` + `CHANGELOG`

## M1 — Captcha pipeline (WebP)

**Done when**: a new user joining a watched chat is restricted, gets a WebP digit-pad with inline keyboard, and is verified-and-unrestricted on correct solve. Wrong solve → attempts decremented; expired → kicked. `/verify` manual override works.

- `services/captcha_service.rs`: deterministic WebP renderer (`image` + `ab_glyph`), atomic `INSERT INTO captcha_challenges`
- Visual upgrade: gradient background, varied fonts/colors per challenge, light noise overlay; output WebP at ~480x180
- `telegram/handlers/member_update.rs`: detect new member → restrict + issue challenge
- `telegram/handlers/captcha.rs`: callback queries for digit input, refresh, backspace
- `jobs/captcha_expiry.rs`: 60s tick, kick (not ban) on expiry, edit message to reflect timeout
- `/verify` slash command (moderator override; reply-mode + id-mode)
- Captcha integration tests under `server/tests/captcha/`
- `server/docs/captcha.md`: state machine, atomicity contract, asset immutability

## M2 — Spam pipeline

**Done when**: a duplicated message (xxh3 dedup) gets the user banned; a CAS-flagged user gets banned on first message; an n-gram-matched message gets deleted with a `moderation_actions` ledger entry. Idempotent under retry. `/ban` and `/unban` work both as reply and with id args.

- `services/spam_service.rs`: `normalize → xxh3-64 → cas_lookup → n-gram match → score → decide`
- Port `vixen/lib/src/anti_spam.dart` phrase set into `services/spam_phrases.rs` (`HashSet<&'static str>`) + tests
- `services/cas_client.rs` with Moka (1h) front + Redis (24h) back tier, fail-open on outage
- `services/moderation_service.rs`: ban / unban / delete with `moderation_actions` ledger uniqueness `(chat_id, target_user_id, action, message_id)` enforcing idempotency
- `jobs/spam_cleanup.rs`: 24h tick, drop `spam_messages` older than 14d
- `/ban` and `/unban` slash commands (moderator-only)
- Spam-corpus tests `server/tests/spam_corpus/*.yaml` (must-ban / must-allow / must-delete)
- `server/docs/spam-detection.md`: pipeline + idempotency rules

## M3 — Reports

**Done when**: each watched chat receives a daily report at the configured hour: a MarkdownV2 message with pseudographics + counts + top phrases + (optional) OpenAI summary, plus a WebP chart message with a caption. Skipped when activity is below threshold. `/stats` returns last-24h summary inline.

- `services/report_service.rs`: aggregate `messages_seen / deleted / verified / banned / captcha_attempts / top_phrases`
- `services/report_render.rs`: MarkdownV2 pseudographics formatter (ASCII bars, sparklines, top-phrase block, redacted)
- `services/chart_service.rs`: WebP chart renderer (`plotters` + image-webp encoder)
- `services/summary_service.rs`: optional OpenAI summary with per-chat daily token cap; reads OpenAI key from chat_config (web-managed, not env)
- `jobs/daily_report.rs`: per-chat scheduling at `chat_config.report_hour` (chat-local TZ); skip when activity below threshold
- `report_messages` replace-on-redo, `daily_stats` cumulative aggregate
- `/stats` slash command — last-24h summary inline
- `/report` slash command stub: returns deep-link to public WebApp page (page itself lands in M6)
- `server/docs/reports.md` with sample MarkdownV2 output checked in

## M4 — Web foundation: auth + hot-reload config

**Done when**: server validates Telegram `initData` HMAC, mints JWT, exposes per-chat config CRUD over `/api/v1/*`. Config writes publish to Redis; in-process cache invalidates on subscribe. End-to-end: edit a value via API → bot picks it up within 1s without restart.

- `services/auth_service.rs`: Telegram `initData` HMAC-SHA256 with `key = HMAC_SHA256("WebAppData", bot_token)`; reject `auth_date > 24h`
- JWT mint (HS256, 1h, `sub = user_id`, `chat_ids` claim derived from `chat_moderators`)
- `api/webapp_auth_middleware.rs` + `api/admin_secret_middleware.rs` (constant-time compare)
- `services/chat_config_service.rs`: rich JSONB schema covering captcha, spam thresholds, report hour, OpenAI key/model, language
- Redis pub/sub channel `chat_config:{chat_id}` invalidates Moka entries; subscribe loop in `bin/server.rs`
- `routes_auth.rs`, `routes_chats.rs`, `routes_config.rs` (read + write per-chat config) with `utoipa`
- End-to-end test: PATCH config → bot reloads cache within 1s → next handler reads new value
- `server/docs/auth.md` + `server/docs/config.md`

## M5 — Moderator dashboard

**Done when**: a moderator opens the dashboard via Telegram Login or WebApp, sees a list of watched chats, drills into a chat (verified / banned / actions / settings tabs), edits per-chat config with optimistic update, and bans/unbans/verifies users from the UI. `/info <user>` slash command landed.

- `website/package.json` + `tsconfig.json` + `vite.config.ts` + `biome.json` + `tailwind.config.cjs`
- `src/app/router.tsx` + root layout + Telegram Login Widget integration + `initData` POST flow
- Chats list + chat detail page (tabs: verified / banned / actions / settings)
- Per-chat settings form bound to `routes_config` with optimistic update + rollback on 4xx
- Audit-log filtered table over `moderation_actions` (keyset pagination)
- Ban / unban / verify dialogs
- i18n bootstrap (RU + EN); CI lint locale parity
- `/info <user>` slash command — DM-only reply with verified/banned history + last 5 actions
- `.github/workflows/website-ci.yml` (biome + typecheck + build)
- `website/docs/architecture.md` refresh

## M6 — Public reports + `/report` WebApp

**Done when**: `/report/{chat_slug}` returns an indexable, redacted public report page. `/report` bot command opens the WebApp with the chat's report. SEO meta + OG image rendered.

- `routes_public.rs`: `GET /report/{chat_slug}` returning redacted aggregates
- `services/redact.rs`: replace URLs / @mentions / phone numbers with `[link]` / `[user]` / `[phone]`
- `chat_slug` column on `chats` (unique, kebab-case fallback to `chat_id_xxxxxx`)
- Public report page `/report/{slug}` with SEO meta + OG image fetch from server
- `/report` Telegram WebApp entry: `tg.WebApp.expand()`, theme-aware
- `/report` slash command swaps stub for deep-link into the WebApp
- `docs/features.md` redact-policy section

## M7 — Data migration from SQLite

**Done when**: a one-shot migration tool reads the legacy `vixen.db`, ports `verified_users` and `deleted_message_hash` (with TTL recompute) into Postgres. Re-running is a no-op. Runbook documented.

- `tools/migrate-from-sqlite/`: standalone Rust binary using `rusqlite` reader + `sqlx` writer
- Map `verified` → `verified_users` (preserve `verified_at`, `chat_id`, `user_id`, `name`)
- Map `deleted_message_hash` → `spam_messages` with TTL recompute (`last_seen + 14d`)
- Idempotent re-run via `INSERT … ON CONFLICT DO NOTHING`
- Verification queries (counts before/after, sample diff)
- `docs/migration-runbook.md` step-by-step

## M8 — Production deploy (webhook)

**Done when**: prod runs on webhook with `X-Telegram-Bot-Api-Secret-Token` validation; polling fallback retained. Production docker-compose with TLS (Traefik), Postgres backup cron, dashboard hosted, runbook in `docs/deployment.md`. CHANGELOG `1.0.0` for both server and website.

- `routes_telegram_webhook.rs` with secret-token check
- `CONFIG_TELEGRAM_MODE = polling | webhook` switch in `bin/server.rs`
- `docker/docker-compose.prod.yml` (server + website + postgres + redis + Traefik)
- Postgres backup cron + restore drill documented
- `.github/workflows/build-server.yml` + `build-website.yml` (multi-arch images, GHCR push)
- TLS via Traefik / Caddy
- `docs/deployment.md` end-to-end runbook + token rotation procedure
- CHANGELOG `1.0.0` for both server and website + version bump

After M8, vixen-rs is feature-equivalent to the Dart prototype with an added dashboard and live web-config. Forward work is prioritized via [features.md](features.md).
