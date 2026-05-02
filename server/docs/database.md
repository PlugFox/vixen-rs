# Database Schema

PostgreSQL 15+. SQLx for compile-time-checked queries. Migrations are SQLx-CLI format under `server/migrations/`.

## Extensions

```sql
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";   -- uuid_generate_v4()
```

`citext` is **not** used (no case-insensitive lookups in vixen v1).

## Tables

### `chats`

The set of watched chats and their metadata.

| Column | Type | Notes |
|---|---|---|
| `chat_id` | `BIGINT PRIMARY KEY` | Telegram chat ID |
| `slug` | `VARCHAR(64) UNIQUE` | URL-safe identifier for public report; NULL = no public report |
| `created_at` | `TIMESTAMPTZ NOT NULL DEFAULT NOW()` | |
| `updated_at` | `TIMESTAMPTZ NOT NULL DEFAULT NOW()` | trigger-managed |

### `chat_config`

Per-chat tunable settings. One row per chat.

| Column | Type | Default | Notes |
|---|---|---|---|
| `chat_id` | `BIGINT PRIMARY KEY REFERENCES chats(chat_id) ON DELETE CASCADE` | | |
| `captcha_enabled` | `BOOLEAN NOT NULL` | `TRUE` | |
| `captcha_lifetime_secs` | `INTEGER NOT NULL CHECK (>0)` | `60` | |
| `captcha_attempts` | `SMALLINT NOT NULL CHECK (>0)` | `5` | |
| `spam_enabled` | `BOOLEAN NOT NULL` | `TRUE` | |
| `spam_threshold` | `REAL NOT NULL CHECK (>=0)` | `1.0` | |
| `spam_weights` | `JSONB NOT NULL` | `'{}'` | per-feature weight overrides; NULL value = use code default |
| `cas_enabled` | `BOOLEAN NOT NULL` | `TRUE` | |
| `clown_chance` | `SMALLINT NOT NULL CHECK (BETWEEN 0 AND 100)` | `0` | |
| `log_allowed_messages` | `BOOLEAN NOT NULL` | `FALSE` | |
| `report_hour` | `SMALLINT NOT NULL CHECK (BETWEEN 0 AND 23)` | `17` | chat-local |
| `timezone` | `VARCHAR(64) NOT NULL` | `'UTC'` | IANA tz name |
| `report_min_activity` | `SMALLINT NOT NULL CHECK (>=0)` | `20` | daily-report scheduler skips when `messages_seen` for the chat-local day is below this |
| `summary_enabled` | `BOOLEAN NOT NULL` | `FALSE` | gates the AI-summary caption on the daily report and `/summary` |
| `summary_token_budget` | `INTEGER NOT NULL CHECK (>0)` | `50000` | per chat-day; hard cap on `daily_stats('openai_tokens_used')` |
| `openai_api_key` | `TEXT` | `NULL` | per-chat OpenAI key; NULL → no AI summary for this chat |
| `openai_model` | `VARCHAR(64) NOT NULL` | `'gpt-4o-mini'` | OpenAI model name |
| `language` | `VARCHAR(8) NOT NULL CHECK (IN ('ru','en'))` | `'ru'` | report locale |
| `created_at` / `updated_at` | `TIMESTAMPTZ` | `NOW()` | trigger-managed |

### `chat_moderators`

Which Telegram users can moderate which chats.

| Column | Type | Notes |
|---|---|---|
| `chat_id` | `BIGINT REFERENCES chats(chat_id) ON DELETE CASCADE` | |
| `user_id` | `BIGINT NOT NULL` | Telegram user ID |
| `granted_at` | `TIMESTAMPTZ NOT NULL DEFAULT NOW()` | |
| `granted_by` | `BIGINT` | NULL when seeded by ops |
| | | `PRIMARY KEY (chat_id, user_id)` |

### `verified_users`

| Column | Type | Notes |
|---|---|---|
| `chat_id` | `BIGINT REFERENCES chats(chat_id) ON DELETE CASCADE` | |
| `user_id` | `BIGINT NOT NULL` | |
| `verified_at` | `TIMESTAMPTZ NOT NULL DEFAULT NOW()` | |
| | | `PRIMARY KEY (chat_id, user_id)` |

Verification is per-chat. Hot-path query: primary-key lookup.

### `captcha_challenges`

| Column | Type | Notes |
|---|---|---|
| `id` | `UUID PRIMARY KEY DEFAULT uuid_generate_v4()` | |
| `chat_id` | `BIGINT NOT NULL REFERENCES chats(chat_id) ON DELETE CASCADE` | |
| `user_id` | `BIGINT NOT NULL` | |
| `solution` | `VARCHAR(8) NOT NULL` | the 4 digits |
| `attempts_left` | `SMALLINT NOT NULL` | starts at `chat_config.captcha_attempts` |
| `telegram_message_id` | `BIGINT` | NULL until the photo is sent successfully |
| `expires_at` | `TIMESTAMPTZ NOT NULL` | |
| `created_at` | `TIMESTAMPTZ NOT NULL DEFAULT NOW()` | |
| | | `UNIQUE (chat_id, user_id)` — one outstanding challenge per `(chat,user)` |
| | | Index: `(expires_at)` for the expiry sweep |

The captcha **in-progress digit input** (the partial string typed between
button presses) and the **per-message callback meta** (owner_user_id +
uuid_short for the ownership check) are NOT stored here — they live in
Redis under `cap:input:{chat}:{user}` and `cap:meta:{chat}:{message}`,
TTL = challenge lifetime. See [captcha.md § State storage](captcha.md#state-storage).

### `spam_messages`

xxh3-64 hash → known spam signature.

| Column | Type | Notes |
|---|---|---|
| `xxh3_hash` | `BIGINT PRIMARY KEY` | of normalized body |
| `sample_body` | `TEXT NOT NULL` | first-seen body, truncated to 500 chars |
| `hit_count` | `BIGINT NOT NULL DEFAULT 1` | |
| `first_seen` | `TIMESTAMPTZ NOT NULL DEFAULT NOW()` | |
| `last_seen` | `TIMESTAMPTZ NOT NULL DEFAULT NOW()` | |
| | | Index: `(last_seen)` for the cleanup sweep |

### `moderation_actions`

Audit log. Append-only.

| Column | Type | Notes |
|---|---|---|
| `id` | `UUID PRIMARY KEY DEFAULT uuid_generate_v4()` | |
| `chat_id` | `BIGINT NOT NULL REFERENCES chats(chat_id) ON DELETE CASCADE` | |
| `target_user_id` | `BIGINT NOT NULL` | |
| `action` | `TEXT NOT NULL CHECK (action IN ('ban','unban','mute','unmute','delete','verify','unverify','captcha_expired','captcha_failed','kick'))` | M1 added the last three for captcha-pipeline outcomes |
| `actor_kind` | `TEXT NOT NULL CHECK (actor_kind IN ('bot','moderator'))` | |
| `actor_user_id` | `BIGINT` | NULL when `actor_kind='bot'` |
| `message_id` | `BIGINT` | Telegram message_id; NULL when not message-scoped |
| `reason` | `TEXT` | free-form (or JSON for spam) |
| `created_at` | `TIMESTAMPTZ NOT NULL DEFAULT NOW()` | |
| | | **`UNIQUE (chat_id, target_user_id, action, message_id)`** — idempotency anchor |
| | | Index: `(chat_id, created_at DESC)` for the audit-log read view |

### `report_messages`

Tracks each Telegram `message_id` posted by the daily-report flow so a re-run can delete-and-replace. Two rows per (`chat_id`, `report_date`) — one for the MarkdownV2 text message, one for the WebP chart photo — keyed by `kind`.

| Column | Type | Notes |
|---|---|---|
| `chat_id` | `BIGINT REFERENCES chats(chat_id) ON DELETE CASCADE` | |
| `report_date` | `DATE NOT NULL` | chat-local date the report was generated for |
| `kind` | `TEXT NOT NULL CHECK (kind IN ('daily_text','daily_photo'))` | future: `weekly_*`, `monthly_*` |
| `telegram_message_id` | `INTEGER NOT NULL` | matches teloxide `MessageId` |
| `generated_at` | `TIMESTAMPTZ NOT NULL DEFAULT NOW()` | |
| | | `PRIMARY KEY (chat_id, report_date, kind)` |
| | | Index: `(chat_id, report_date DESC)` for "today's report" lookup |

### `daily_stats`

Pre-aggregated daily counters per chat per metric. Updated via `models::daily_stats::increment` UPSERT (`INSERT ... ON CONFLICT DO UPDATE SET value = value + EXCLUDED.value`).

| Column | Type | Notes |
|---|---|---|
| `chat_id` | `BIGINT REFERENCES chats(chat_id) ON DELETE CASCADE` | |
| `date` | `DATE NOT NULL` | server-UTC date when the increment landed |
| `kind` | `TEXT NOT NULL` | see `Metric` enum: `messages_seen`, `messages_deleted`, `users_banned`, `users_verified`, `captcha_issued`, `captcha_solved`, `captcha_expired`, `openai_tokens_used` |
| `value` | `BIGINT NOT NULL` | accumulator |
| | | `PRIMARY KEY (chat_id, date, kind)` |
| | | Index: `(chat_id, date DESC)` |

Read from the dashboard's report view; written incrementally by the spam / captcha / moderation pipelines as `INSERT ... ON CONFLICT DO UPDATE SET value = value + EXCLUDED.value`.

### `chat_info_cache`

Cached `getChat` response per watched chat.

| Column | Type | Notes |
|---|---|---|
| `chat_id` | `BIGINT PRIMARY KEY REFERENCES chats(chat_id) ON DELETE CASCADE` | |
| `title` | `TEXT NOT NULL` | |
| `type` | `TEXT NOT NULL CHECK (type IN ('private','group','supergroup','channel'))` | |
| `description` | `TEXT` | |
| `members_count` | `INTEGER` | |
| `updated_at` | `TIMESTAMPTZ NOT NULL DEFAULT NOW()` | |

Refreshed every 6h by `chat_info_refresh` job.

### `allowed_messages` (optional, gated)

Only populated when `chat_config.log_allowed_messages = TRUE`. Used by the AI-summary pipeline and (eventually) for the dashboard's per-chat activity timeline.

| Column | Type | Notes |
|---|---|---|
| `chat_id` | `BIGINT REFERENCES chats(chat_id) ON DELETE CASCADE` | |
| `message_id` | `BIGINT NOT NULL` | |
| `user_id` | `BIGINT NOT NULL` | |
| `username` | `VARCHAR(64)` | |
| `kind` | `TEXT NOT NULL` | `'text'`, `'photo'`, ... |
| `length` | `INTEGER` | |
| `content` | `TEXT` | NULL for non-text |
| `created_at` | `TIMESTAMPTZ NOT NULL DEFAULT NOW()` | |
| | | `PRIMARY KEY (chat_id, message_id)` |
| | | Index: `(chat_id, created_at DESC)` |

Has its own retention sweep (deletes rows older than 14 days; aligns with `spam_messages` retention).

## Functions / Triggers

```sql
-- Auto-update updated_at columns
CREATE OR REPLACE FUNCTION update_updated_at()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Applied per table that has updated_at:
CREATE TRIGGER trg_chats_updated_at
    BEFORE UPDATE ON chats FOR EACH ROW EXECUTE FUNCTION update_updated_at();
-- ... same for chat_config, etc.
```

`update_updated_at` is defined once in the initial migration.

## Migrations

- File naming: `YYYYMMDDHHMMSS_description.sql` + matching `.down.sql`. Example: `20260501120000_initial_schema.sql`.
- Apply: `cargo sqlx migrate run` (or via `/db-migrate`).
- Refresh `.sqlx/`: `cargo sqlx prepare -- --all-targets`.
- Rules: see [rules/migrations.md](rules/migrations.md).

## Connection pool

Defined in `src/database/db.rs`:

- `max_connections = 50`
- `min_connections = 5`
- `acquire_timeout = 10s`
- `idle_timeout = 600s`
- `statement_timeout = 30s` (set once per pooled connection on connect via `SET statement_timeout` — session-scoped, sticks for the lifetime of the connection)

Wrapped in `Database { pool: PgPool }` and shared as `Arc<Database>` from `AppState`.

## Indexes — summary

The PRIMARY KEY / UNIQUE constraints above cover the hot lookups (verified-user check, spam-hash dedup, captcha challenge per user, moderation action idempotency). The non-PK indexes cover read-paths in the dashboard:

- `moderation_actions (chat_id, created_at DESC)` — audit log pagination.
- `daily_stats (chat_id, date DESC)` — report queries.
- `captcha_challenges (expires_at)` — expiry sweep.
- `spam_messages (last_seen)` — retention sweep.
- `allowed_messages (chat_id, created_at DESC)` — when enabled.

Add new indexes only with `EXPLAIN ANALYZE` evidence — see `.claude/skills/server/postgres-optimization/SKILL.md`.
