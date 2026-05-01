# Vixen-rs — Architecture

Single-tenant Telegram anti-spam bot with a SolidJS dashboard. One Rust process holds the HTTP server, the Telegram poller, and the background-job runner. PostgreSQL is the source of truth (data + per-chat configuration); Redis provides hot caches and a pub/sub channel for live config reload. The website is a static SPA served separately.

## Component map

```
                ┌──────────────────────────────────────────────────┐
                │                   Telegram                       │
                │       (Bot API: long-polling for v1)             │
                └─────────────┬───────────────────────▲────────────┘
                              │ updates               │ sendPhoto / banChatMember /
                              │                       │ deleteMessage / answerCallback
                              ▼                       │
       ┌──────────────────────────────────────────────────────────────┐
       │  vixen-server (single Rust process)                          │
       │                                                              │
       │  ┌────────────────────┐  ┌─────────────────┐                 │
       │  │  teloxide poller   │  │   axum HTTP     │                 │
       │  │  + dispatcher      │  │   server        │                 │
       │  │  watched-chats     │  │   /api/v1/*     │                 │
       │  │  filter            │  │   /report/*     │                 │
       │  └─────────┬──────────┘  └────────┬────────┘                 │
       │            │                      │                          │
       │            ▼                      ▼                          │
       │  ┌──────────────────────────────────────────┐  ┌───────────┐ │
       │  │   services/                              │  │ jobs/     │ │
       │  │   captcha · spam · moderation · reports  │  │ daily_rep │ │
       │  │   summary · auth · chat_config           │  │ captcha_  │ │
       │  └─────────────────────┬────────────────────┘  │ expiry    │ │
       │                        │                       │ spam_clean│ │
       │                        │                       │ chat_info │ │
       │                        ▼                       │ summary   │ │
       │  ┌──────────────────────────────────────────┐  └─────┬─────┘ │
       │  │  database/                                │◄───────┘       │
       │  │  - PgPool (sqlx)                          │                │
       │  │  - Redis pool (deadpool-redis):           │                │
       │  │      hot caches (CAS, verified_users,     │                │
       │  │      chat_config) + pub/sub for           │                │
       │  │      hot-reload                           │                │
       │  └─────────────────────┬────────────────────┘                │
       └────────────────────────┼─────────────────────────────────────┘
                                │
        ┌───────────────────────┼────────────────────────┐
        ▼                       ▼                        ▼
┌───────────────────┐   ┌───────────────────┐   ┌──────────────────┐
│   PostgreSQL      │   │     Redis         │   │  Telegram Bot    │
│  (chats,          │   │  caches:          │   │  outbound        │
│   chat_config,    │   │   verified, CAS,  │   │                  │
│   captcha_        │   │   chat_config     │   │                  │
│   challenges,     │   │  pub/sub:         │   │                  │
│   spam_messages,  │   │   chat_config:{}  │   │                  │
│   moderation_     │   │                   │   │                  │
│   actions, ...)   │   │                   │   │                  │
└───────────────────┘   └───────────────────┘   └──────────────────┘

       ┌──────────────────────────────────────────────────────────────┐
       │  vixen-website (static SPA)                                  │
       │                                                              │
       │  - Moderator dashboard (TG WebApp or Login Widget)           │
       │  - Public chat report (redacted, indexable)                  │
       │  Talks to vixen-server via /api/v1/*                         │
       └──────────────────────────────────────────────────────────────┘

External calls (all rate-limited and timed out):
  - Combot Anti-Spam: https://api.cas.chat/check?user_id=  (Moka 1h front + Redis 24h back)
  - OpenAI Chat Completions: optional, daily summary, per-chat budget. API key lives in chat_config (web-managed).
  - Telegram Bot API: outbound from teloxide
```

## Process model

`bin/server.rs` spawns four top-level tasks from one runtime, sharing one `Arc<AppState>` (`PgPool` + Redis pool + Moka caches + config + bot handle):

1. **HTTP server** via `axum::serve(...).with_graceful_shutdown(token.cancelled())`.
2. **Telegram dispatcher** via `teloxide::Dispatcher::dispatch_with_listener` against the same shutdown token.
3. **Background-job runner** that spawns one task per registered job (see `server/docs/background-jobs.md`).
4. **Redis subscribe loop** listening on `chat_config:*` (and other invalidation channels) to evict Moka entries and re-warm on demand.

A single `tokio_util::sync::CancellationToken` ties them together. `tokio::signal::ctrl_c()` + SIGTERM listener fires the token; `bin/server.rs` then `join!`s the four handles with a 30s outer timeout before exiting.

## Data flow — message handling

1. Telegram → poller → dispatcher.
2. Watched-chats filter drops updates from chats not in `CONFIG_CHATS`.
3. Per-update-type handler (Message / EditedMessage / CallbackQuery / ChatMemberUpdated / MyChatMember).
4. For Message in a watched chat:
   - Lookup `verified_users(chat_id, user_id)` — verified → bypass anti-spam (modulo `clown_chance`).
   - Unverified → CAPTCHA pipeline (issue challenge, restrict user, log to `captcha_challenges`).
   - Verified message → spam pipeline: normalize → xxh3 hash → `spam_messages` lookup → CAS API (cached) → n-gram → decide → ban/delete + `moderation_actions` insert.

## Data flow — daily report

1. Scheduler fires at `CONFIG_REPORT_HOUR` chat-local time per chat.
2. `report_service` aggregates: messages_seen, messages_deleted, users_verified, users_banned, top_spam_phrases, captcha_attempts.
3. `plotters` renders a PNG bar chart.
4. `bot.send_photo(chat_id, ...)`. Captures `message_id` into `report_messages` so a future replace can delete it.
5. If `CONFIG_OPENAI_KEY` is set and the chat's daily token budget isn't exhausted: sanitize → POST to OpenAI → append summary as caption.

## Data flow — auth

1. Website (in Telegram WebApp or browser) reads `Telegram.WebApp.initData` (or composes one from Login Widget callback).
2. POSTs raw `initData` to `/api/v1/auth/telegram/login`.
3. Server validates HMAC-SHA256 against `secret = HMAC_SHA256("WebAppData", bot_token)` (per Telegram spec). Rejects if `auth_date > 24h`.
4. Looks up `chat_moderators` rows for the verified `user_id`. Mints a JWT (HS256, 1h) with `sub = user_id` + `chat_ids = [...]`.
5. Subsequent dashboard requests carry `Authorization: Bearer <jwt>`. Server-side double-check on every chat-scoped endpoint.

## Single-tenant assumption

One deploy = one bot token = N watched chats from `CONFIG_CHATS`. There is **no** multi-tenancy:

- No per-tenant DB schema or row-level security.
- No `tenant_id` column on any table.
- No tenant-scoped JWT.

If a future deploy ever needs multi-tenancy, the migration path is to add a `tenant_id BIGINT NOT NULL` column to every table that currently scopes by chat — not refactor the existing code paths. We are NOT designing for that today.

## Hot-reload config

Per-chat configuration lives in `chat_config` (PostgreSQL JSONB) and is read through a Moka cache keyed by `chat_id`. Writes happen via `PATCH /api/v1/chats/{id}/config`:

1. Service updates Postgres in a `SELECT ... FOR UPDATE` transaction.
2. On commit, publishes `chat_config:{chat_id}` on Redis pub/sub.
3. The Redis subscribe loop in `bin/server.rs` evicts the Moka entry; the next read repopulates from Postgres.

The same channel is used by future multi-replica deployments: every replica subscribes and invalidates its own cache. Caches always read-through Postgres on miss, so a missed pub/sub message degrades to staleness up to the Moka TTL, never inconsistency.

Env vars retain only secrets, bind addresses, and the Postgres / Redis connection URLs. Tunables (captcha policy, spam thresholds, report hour, OpenAI key/model, language) are edited from the dashboard.

## External dependencies

| Dependency | Required? | What if it's down |
|---|---|---|
| Telegram Bot API | required | Bot can't poll or send. Polling retries with backoff. |
| PostgreSQL | required | Server refuses to start. |
| Redis | required | Server refuses to start. Hot-reload + caches both depend on it. |
| Combot Anti-Spam | optional (toggled per-chat) | Pipeline falls back to xxh3 + n-gram only. Failure is fail-open (no false ban). |
| OpenAI | optional (per-chat, key in chat_config) | Reports ship without AI summary. |

## Security boundaries

- Inbound HTTP: terminated by Traefik / nginx in prod (TLS), proxied to Axum.
- Bot token: env-only, never logged, never written to disk by the server.
- `initData` HMAC: validated on every dashboard request.
- `CONFIG_ADMIN_SECRET`: constant-time compared on `/admin/*` endpoints; not used by the dashboard.
- Public report (`/report/{chat_slug}`): redacted aggregates only, no PII.

## What's deliberately not here

- No object storage (no S3 / MinIO). Captcha images are generated on demand and not persisted.
- No Google OAuth. Auth is Telegram-only.
- No multi-process scaling out of the box. One process owns the polling loop in v1; horizontal scaling on top of webhook + leader-elected scheduler is in the post-M8 backlog.
- No worker queue. Background jobs are simple `tokio` interval loops; Redis is used for caches and pub/sub only, not as a job queue.
