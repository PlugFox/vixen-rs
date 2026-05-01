---
name: add-migration
description: Create a new SQLx migration for the vixen-rs Postgres schema, including .up.sql and .down.sql, then apply it and refresh the offline query cache. Use when the user asks to add a migration, change the DB schema, create a new table, alter columns, or add an index.
---

# Add SQLx Migration (Vixen server)

**Read first**: [server/docs/rules/migrations.md](../../../../server/docs/rules/migrations.md) — it is the source of truth for naming, idempotency, and the no-rewrite policy. Do not skip this step.

Also re-read the relevant schema doc in [server/docs/database.md](../../../../server/docs/database.md) to understand what exists today.

## Naming

`server/migrations/YYYYMMDDHHMMSS_short_snake_description.sql` and a matching `.down.sql`.

- Use the current UTC date for the prefix. Run `date -u +%Y%m%d%H%M%S` to avoid clock drift.
- Description: ≤4 words, snake_case, imperative (`add_chat_config_columns`, `drop_legacy_kv`).
- Never edit an existing applied migration — add a new one that fixes the issue.

## Vixen-specific column rules

- **Telegram IDs are `BIGINT NOT NULL`.** Never `INTEGER` (overflow at 2.1B users) and never nullable when they identify the row owner.
- **Foreign keys to `chats(chat_id)` use `ON DELETE CASCADE`** — deleting a watched chat purges its history.
- **All `created_at` / `updated_at` are `TIMESTAMPTZ DEFAULT NOW()`.** Never naive `TIMESTAMP`.
- **Per-chat config lives in a JSONB column** on the `chats` table for low-cardinality fields, or in a dedicated `chat_config` table when fields are queried individually.
- **Spam-message hashes are `BIGINT`** (xxh3-64 fits exactly).

## Structure

```sql
-- server/migrations/20260418120000_add_chat_clown_chance.up.sql
BEGIN;

ALTER TABLE chat_config
    ADD COLUMN IF NOT EXISTS clown_chance SMALLINT NOT NULL DEFAULT 0
        CHECK (clown_chance BETWEEN 0 AND 100);

COMMIT;
```

```sql
-- server/migrations/20260418120000_add_chat_clown_chance.down.sql
BEGIN;

ALTER TABLE chat_config DROP COLUMN IF EXISTS clown_chance;

COMMIT;
```

## Checklist

- `BEGIN; ... COMMIT;` wrapper unless the statement forbids it (`CREATE INDEX CONCURRENTLY`).
- `IF NOT EXISTS` / `IF EXISTS` for idempotency per the rules doc.
- `.down.sql` exists and is the inverse — reviewed, not empty.
- Foreign keys: `ON DELETE` decided explicitly; default to `CASCADE` for chat-owned data.
- Indexes on new foreign-key columns and on any column used in `WHERE`.
- Defaults on `NOT NULL` columns added to existing tables (backfill plan otherwise).
- Large data migrations (UPDATE over big tables) go in a separate migration, not mixed with schema DDL.

## After writing

1. Ensure Postgres is up (`/db-up` if not).
2. Apply: `/db-migrate` — runs `sqlx migrate run` and refreshes `.sqlx/` offline cache.
3. Update [server/docs/database.md](../../../../server/docs/database.md) with the new column/table/index.
4. If any model struct or query changed, run `/server-check` to catch breakage.
5. Commit migration files + `.sqlx/` changes + doc update in a single commit.

## Common mistakes

- Using `INTEGER` for a Telegram chat or user ID → silent corruption when negative supergroup IDs hit `i32::MIN`.
- Forgetting the down file → CI lints check for it.
- Using `NOW()` in defaults without timezone → use `TIMESTAMPTZ DEFAULT NOW()`.
- Adding a `NOT NULL` column without a default to a non-empty table → migration fails in prod.
- Renaming a column in place instead of adding new + backfilling + dropping old in two migrations.
