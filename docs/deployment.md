# Vixen-rs — Deployment

This document covers local dev (Docker Compose) and a sketch for production. Production specifics (orchestrator choice, hostnames, TLS termination) are deliberately not finalized — we ship polling-mode v1 first and pick the prod story when the bot has soaked.

## Local dev

```bash
# 1. Bring up Postgres
docker compose -f docker/docker-compose.yml up -d postgres

# 2. Apply migrations
cd server && sqlx migrate run

# 3. Run server (HTTP + bot poller)
TELEGRAM_BOT_TOKEN=... cd server && cargo run

# 4. Run website
cd website && bun install && bun run dev
```

Required env vars (see `server/config/template.env` for the exhaustive list): `CONFIG_DATABASE_URL`, `CONFIG_BOT_TOKEN`, `CONFIG_CHATS`. Optional: `CONFIG_ADMIN_SECRET`, `CONFIG_OPENAI_KEY`, `CONFIG_CAS=on`, `CONFIG_REPORT_HOUR=17`.

## Production sketch

Single host with Docker Compose is sufficient for v1 (one bot, ≤ a few chats, low write rate). The deployable surface:

```
┌──────────────┐      TLS termination, rate limiting
│   Traefik    │  ──  (or nginx, or Caddy — operator's choice)
└───────┬──────┘
        │ /api/* → server
        │ /report/* → website (static) → server (data)
        │ /  → website (static)
        ▼
┌──────────┐   ┌──────────┐
│  server  │   │ website  │  (static files served by nginx in the website image)
└─────┬────┘   └──────────┘
      │
      ▼
┌──────────────┐
│  Postgres    │  Persistent volume.
└──────────────┘
```

Orchestrator choice (Compose / Swarm / k8s) is deferred — the multi-stage Docker images are deployable to any of them. Single-tenant means horizontal scaling is unnecessary; one server pod is the steady state.

## Polling vs webhook

**Polling (v1)** — simpler. The server makes long-polling requests to Telegram. No public ingress for Telegram is needed; only the dashboard / public report needs HTTPS exposure. Single process owns the poller.

**Webhook (planned)** — `setWebhook` to a public HTTPS endpoint on the server. Telegram POSTs updates as they arrive. Lower latency, supports horizontal scaling (multiple replicas behind a load balancer, with leader-elected scheduler). Requires:

- Public HTTPS URL with a valid certificate (Telegram requires it).
- A `routes_telegram_webhook.rs` handler that decodes the update and dispatches it through the same `dispatcher.rs` tree.
- A `/webhook` route protected by Telegram's secret-token header (`X-Telegram-Bot-Api-Secret-Token`).
- Disabling `setWebhook` rolls back to polling cleanly — both modes share the same dispatcher.

The polling↔webhook switch should be a config flag, not a code refactor.

## Secrets

Three values are sensitive and **must never** be checked into git, baked into Docker images at build time, or printed by any command:

- `CONFIG_BOT_TOKEN` (`TELEGRAM_BOT_TOKEN`) — gives full control of the bot.
- `CONFIG_OPENAI_KEY` (`OPENAI_API_KEY`) — billable.
- `CONFIG_ADMIN_SECRET` — opens `/admin/*`.
- `CONFIG_JWT_SECRET` — signs dashboard JWTs.

How to inject them:

- **Local dev**: `.env.local` (gitignored). Loaded via `direnv` or `source .env.local`.
- **Compose**: `env_file:` pointing at a non-committed file, or `secrets:` (Docker Swarm).
- **k8s / cloud**: secret manager → env, never inline in deployment YAML.

Claude Code's `.claude/settings.json` denies any Bash command that could echo, grep, or pipe these env vars to disk. Do not work around it.

## CI/CD (planned)

GitHub Actions workflows under `.github/workflows/` (to be created in M0):

- **`server-ci.yml`** — on PR touching `server/**`: `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test`, `cargo sqlx prepare --check`.
- **`website-ci.yml`** — on PR touching `website/**`: `bun run check`, `bun run typecheck`, `bun run build`.
- **`build-server.yml`** — manual dispatch: bump version, `docker build`, push to Docker Hub / GHCR, git tag.
- **`build-website.yml`** — manual dispatch: bump version, `docker build`, push, tag.
- A deploy step (Portainer webhook / k8s rollout) is added once the orchestrator is chosen.

## Observability

`tracing` writes to two sinks:

- **Console** — human-readable, level controlled by `CONFIG_LOG_LEVEL` (default `info`).
- **File** — JSON, daily rotation, 7-day retention. Path from `CONFIG_LOG_DIR`.

Critical fields per span: `update_id`, `chat_id`, `user_id` (for bot events); `request_id`, `route`, `user_id` (for HTTP). The redaction policy is enforced in `server/src/utils/redact.rs` — bot tokens and raw `initData` never leave `debug` level.

A planned `/admin/metrics` endpoint will expose Prometheus metrics (process-level + per-job counters). Not in M0.

## Maintenance

- **Backups**: nightly `pg_dump` of the database to off-host storage. The bot's state is entirely in Postgres; restoring the dump is the disaster recovery story.
- **Updates**: bump `server/Cargo.toml` or `website/package.json`, append CHANGELOG entry, push, run the build workflow.
- **Migrations**: applied automatically at server startup (`sqlx::migrate!()`). Down migrations are committed but not auto-applied — manual rollback only.
- **Rotating the bot token**: re-issue via @BotFather, update the secret, redeploy. All existing JWTs are invalidated (because they were minted with the old token's HMAC chain). Moderators re-login through Telegram.

## Troubleshooting

**Bot not responding to messages**
- Run `/bot-token` to verify the token still works (`getMe` returns 200).
- Check `tracing` logs for "polling stalled" or `RequestError`.
- `CONFIG_CHATS` typo: chat IDs are negative for groups (`-100…`) and positive for private chats. Mismatched ID = silent drop.

**Dashboard returns 401 on every request**
- The JWT is bound to the bot token's HMAC chain. If the token rotated, every JWT becomes invalid. Have moderators sign in again.
- Check that the website is sending the `Authorization: Bearer ...` header — the auth interceptor in `website/src/shared/api/client.ts` should add it automatically.

**Public report shows "no data"**
- Reports are computed by the daily `report_service` job. If the chat is new or the day hasn't rolled over, there's no row in `daily_stats` yet.

**Migration fails in production**
- Do **not** run `sqlx migrate revert` blindly. Check the error, fix the migration in a follow-up commit, ship the fix. Vixen migrations are append-only.

**Postgres connection errors at startup**
- Compose dependency: server `depends_on` postgres with `condition: service_healthy`. If the health check is too strict, the server retries with backoff. After 30s of failure, the pool returns errors to handlers — that surfaces as 500s to dashboard requests.
