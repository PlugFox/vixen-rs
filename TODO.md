# TODO

Short-term work-in-progress and ideas. Long-form roadmap lives in [docs/roadmap.md](docs/roadmap.md). Long-form feature backlog lives in [docs/features.md](docs/features.md).

## Now (M0 — Foundation & infra)

- [ ] `server/Cargo.toml` with edition 2024, axum, sqlx, teloxide, tracing, clap, moka, redis, image (webp), ab_glyph, plotters, utoipa, xxhash-rust, jsonwebtoken
- [ ] `server/bin/server.rs` entry: load config, init tracing, start HTTP server + bot poller + job runner with shared `CancellationToken`
- [ ] `server/src/{api,telegram,services,jobs,models,database,config,telemetry,utils}/mod.rs` skeletons
- [ ] `server/migrations/000_initial_schema.sql`: chats, chat_config, chat_moderators, verified_users, captcha_challenges, spam_messages, moderation_actions, report_messages, daily_stats, chat_info_cache
- [ ] `server/src/database/redis.rs`: pooled async client, healthcheck, pub/sub helper
- [ ] `server/config/template.env` with every `CONFIG_*` documented (only secrets and bind/db/redis URLs)
- [ ] tracing setup: console (human) + JSON file with daily rotation + `RedactedToken` newtype
- [ ] `/health` (DB+Redis ping) and `/about` REST endpoints with utoipa
- [ ] Polling worker stub: drop every update through the watched-chats filter (no handlers yet)
- [ ] Carry over `DejaVuSans.ttf` from `.old/src/captcha/assets/` → `server/assets/captcha/`
- [ ] `docker/docker-compose.yml`: postgres + redis (healthchecks, named volumes)
- [ ] `.github/workflows/server-ci.yml` (fmt + clippy + test + sqlx prepare check)

## Next (M1 — Captcha pipeline / WebP)

- [ ] WebP image renderer with deterministic output per challenge ID (`image` + `ab_glyph`)
- [ ] Visual upgrade: gradient bg, varied fonts/colors, light noise (480x180 WebP)
- [ ] Captcha challenge state machine (issue → solve → expire) with atomic DB writes
- [ ] `captcha_expiry` background job (60s tick, kick-not-ban on expiry)
- [ ] `/verify` slash command (manual override by moderator)
- [ ] Inline-keyboard digit pad + refresh + backspace
- [ ] `server/docs/captcha.md`

## Ideas (post-M8)

- [ ] Webhook + horizontal scaling via Postgres advisory lock for jobs
- [ ] Picture-pick captcha mode ("tap the cat", 3-of-9 grid)
- [ ] Math captcha mode ("3 + 4 = ?")
- [ ] Per-chat captcha policy override (only-for-deep-links / off-hours / on-CAS-flag)
- [ ] Top-N spammed phrases on the public report (with extra redaction)
- [ ] Spam-rule weight tuning per chat
- [ ] Ad-hoc report on demand (`POST /api/v1/chats/{id}/reports/generate`)
- [ ] Audit-log search filters in the dashboard
- [ ] Bulk verify / unban from the action ledger
- [ ] PWA install promo for the dashboard inside Telegram WebApp
