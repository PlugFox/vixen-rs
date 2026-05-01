# Configuration

All env vars are prefixed `CONFIG_`. Loaded by `src/config/mod.rs` (clap Parser). At startup the server first reads env, then optionally `dotenvy::dotenv()` for local dev.

Required values cause the server to refuse to start with a clear error message. Optional values have safe defaults.

## Reference

| Env var | Type | Default | Required | Description |
|---|---|---|---|---|
| `CONFIG_BOT_TOKEN` | string | — | yes | Telegram bot token from @BotFather. **Never logged**, never exposed via any endpoint. |
| `CONFIG_DATABASE_URL` | URL | — | yes | `postgresql://user:pass@host:port/dbname`. |
| `CONFIG_CHATS` | comma-separated `i64` list | `""` | yes (must be non-empty) | Telegram chat IDs to watch. Negative for groups/supergroups (`-100…`), positive for private. |
| `CONFIG_ADDRESS` | socket | `0.0.0.0:8000` | no | HTTP bind address. |
| `CONFIG_ENVIRONMENT` | `dev` \| `staging` \| `prod` | `dev` | no | Affects logging verbosity defaults, OpenAPI UI exposure, error message detail. |
| `CONFIG_LOG_LEVEL` | `error` \| `warn` \| `info` \| `debug` \| `trace` | `info` | no | Console log level. |
| `CONFIG_LOG_DIR` | path | `./logs` | no | Where the JSON file logger writes; daily rotation, 7-day retention. |
| `CONFIG_OPENAPI_UI` | bool | `true` (dev) / `false` (prod) | no | Whether `/scalar` is mounted. |
| `CONFIG_CORS_ORIGINS` | comma-separated URLs | `http://localhost:3000` | no | Allowed origins for the dashboard. **No wildcards.** |
| `CONFIG_TELEGRAM_MODE` | `polling` \| `webhook` | `polling` | no | v1 supports polling; webhook lands in M6. |
| `CONFIG_TELEGRAM_WEBHOOK_URL` | URL | — | only if `webhook` mode | Public HTTPS endpoint Telegram POSTs to. |
| `CONFIG_TELEGRAM_WEBHOOK_SECRET` | string | — | only if `webhook` mode | Validated against `X-Telegram-Bot-Api-Secret-Token` header. |
| `CONFIG_CAS` | bool | `true` | no | Whether to call Combot Anti-Spam during the spam pipeline. |
| `CONFIG_CAS_URL` | URL | `https://api.cas.chat/check` | no | CAS endpoint. |
| `CONFIG_CAS_TIMEOUT_MS` | int | `3000` | no | Per-request timeout. Failure is fail-open. |
| `CONFIG_REPORT_HOUR` | int 0-23 | `17` | no | Default daily-report hour (per-chat override in `chat_config.report_hour`). |
| `CONFIG_OPENAI_KEY` | string | — | no | If set, AI summary is available per-chat. |
| `CONFIG_OPENAI_MODEL` | string | `gpt-4o-mini` | no | Model name. |
| `CONFIG_OPENAI_URL` | URL | `https://api.openai.com/v1/chat/completions` | no | Override for self-hosted compatible APIs. |
| `CONFIG_OPENAI_TIMEOUT_MS` | int | `30000` | no | Per-request timeout. |
| `CONFIG_ADMIN_SECRET` | string | — | yes (in prod) | Bearer for `/admin/*`. Constant-time compared. |
| `CONFIG_JWT_SECRET` | string ≥ 32 bytes | — | yes (in prod) | HS256 secret for dashboard JWTs. Rotate to invalidate all sessions. |
| `CONFIG_JWT_TTL_SECS` | int | `3600` | no | JWT expiry. |
| `CONFIG_INIT_DATA_MAX_AGE_SECS` | int | `86400` | no | Reject `initData` with `auth_date` older than this. |
| `CONFIG_DB_MAX_CONNECTIONS` | int | `50` | no | PgPool max. |
| `CONFIG_DB_MIN_CONNECTIONS` | int | `5` | no | PgPool min. |
| `CONFIG_DB_ACQUIRE_TIMEOUT_MS` | int | `10000` | no | |
| `CONFIG_DB_IDLE_TIMEOUT_MS` | int | `600000` | no | |
| `CONFIG_DB_STATEMENT_TIMEOUT_MS` | int | `30000` | no | Set per connection on acquire. |
| `CONFIG_SPAM_RETENTION_DAYS` | int | `14` | no | TTL for `spam_messages` rows. |
| `CONFIG_ALLOWED_MESSAGES_RETENTION_DAYS` | int | `14` | no | TTL for `allowed_messages` rows (when feature enabled). |
| `CONFIG_RATE_LIMIT_PUB_RPM` | int | `60` | no | Per-IP rate limit for public endpoints. |

## Per-chat overrides

These tunables also have per-chat versions in `chat_config` (see [database.md](database.md)). When both are set, the per-chat value wins — the global config is the default for newly-added chats.

- `report_hour` → `chat_config.report_hour`
- `cas` → `chat_config.cas_enabled`
- `summary_enabled` (no global; opt-in per-chat) → `chat_config.summary_enabled`
- `summary_token_budget` (no global) → `chat_config.summary_token_budget`

## Secret handling

Three values are sensitive:

- `CONFIG_BOT_TOKEN`
- `CONFIG_OPENAI_KEY`
- `CONFIG_ADMIN_SECRET`
- `CONFIG_JWT_SECRET`

Loaded into `Config` once, wrapped in newtypes that `Display`/`Debug` as `***redacted***`. Never log raw, never expose via any endpoint, never write to disk. The `.claude/settings.json` deny-list blocks Bash patterns that could echo / printenv these.

For prod injection: env via secret manager (k8s `Secret`, Compose `secrets:`, Swarm `secrets`). Local dev: `.env.local` (gitignored).

## Where the template lives

`server/config/template.env` is the canonical reference — every var listed above with a comment explaining when to set it. Copy to `.env.local` and fill in for local development.

## Validating config at startup

`Config::validate()` runs immediately after parsing:

- `CONFIG_BOT_TOKEN` matches `^\d+:[A-Za-z0-9_-]{30,}$` (the Bot API token format).
- `CONFIG_CHATS` parses to a non-empty `Vec<i64>`.
- `CONFIG_CORS_ORIGINS` parses to valid origins (no `*`, no schemeless).
- In `prod` environment, `CONFIG_ADMIN_SECRET` and `CONFIG_JWT_SECRET` are required.
- `CONFIG_TELEGRAM_MODE = webhook` requires `CONFIG_TELEGRAM_WEBHOOK_URL` and `CONFIG_TELEGRAM_WEBHOOK_SECRET`.

Any failure prints a clear message and exits with code 2.
