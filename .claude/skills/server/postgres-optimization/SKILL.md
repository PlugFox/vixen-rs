---
name: postgres-optimization
description: Tune Postgres queries and schema in the vixen-rs server — read EXPLAIN (ANALYZE, BUFFERS), add the right indexes, spot N+1, batch writes, use partial/covering/expression indexes, avoid row locks on hot paths. Use when a query is slow, when adding a new query on a non-trivial table, or when reviewing migrations that touch indexes.
---

# Postgres Optimization (Vixen server)

**Source:** [Postgres docs on performance tips](https://www.postgresql.org/docs/current/performance-tips.html) + foxic-derived rules.

## Investigate before optimizing

- **Always start with `EXPLAIN (ANALYZE, BUFFERS, VERBOSE)`** on a realistic dataset. "It's slow" is not a diagnosis.
  ```sql
  EXPLAIN (ANALYZE, BUFFERS) SELECT ...;
  ```
- Read top-down. Large `Seq Scan` on a filtered table, large `Rows Removed by Filter`, or a `Nested Loop` with a high outer row count → index-missing smell.
- `Buffers: shared read=N` means cold cache disk reads; `shared hit=N` is memory.
- `actual rows` vs `estimated rows` drift by >10× → stats are stale (`ANALYZE <table>`) or the predicate is hard to estimate.
- Use the `postgres` MCP tool (read-only local DB) for quick EXPLAINs without `docker compose exec`.

## Indexing

- **Match the WHERE / JOIN / ORDER BY columns**, and in that column order. A composite `(chat_id, created_at DESC)` serves `WHERE chat_id = $1 ORDER BY created_at DESC` as an index-only scan if `created_at` is covered.
- **Partial indexes** for hot-but-narrow predicates: `CREATE INDEX ... ON moderation_actions (chat_id, created_at DESC) WHERE actor_kind = 'bot';`. Smaller, faster, cheaper to maintain.
- **Expression indexes** for computed predicates: `CREATE INDEX ON spam_messages (xxh3_hash);` (already part of the natural key, but worth making explicit if joining on it).
- **Covering (`INCLUDE`)** to avoid heap lookups: `CREATE INDEX ... ON verified_users (chat_id) INCLUDE (user_id, verified_at);`.
- **`CREATE INDEX CONCURRENTLY`** in production migrations (must be in its own migration, not inside a `BEGIN`).
- **Avoid over-indexing.** Every index costs writes, WAL, and vacuum time. Drop indexes with `pg_stat_user_indexes.idx_scan = 0`.

## Vixen-specific hot paths

- **Verified-user lookup** (`SELECT 1 FROM verified_users WHERE chat_id = $1 AND user_id = $2`) runs on every message — must be a primary-key hit. Composite PK `(chat_id, user_id)` does this for free.
- **Captcha expiry sweep** runs every 60s — needs an index on `captcha_challenges (expires_at)` (a partial index on `expires_at < now()` is overkill; plain btree works).
- **Daily report aggregation** scans 24h of `moderation_actions` and 24h of `verified_users` — composite index `(chat_id, created_at DESC)` on both.
- **Spam-hash dedup** (`SELECT 1 FROM spam_messages WHERE xxh3_hash = $1`) — primary-key hit.

## Common bad patterns

- **N+1**: `for id in ids { SELECT ... WHERE id = $1 }`. Fix: `WHERE id = ANY($1::bigint[])` or `JOIN` with an in-list CTE.
- **`SELECT *`** in SQLx macros — you fetch columns you don't need. Name columns.
- **`OFFSET` pagination** on large sets — O(N) scan. Use keyset pagination: `WHERE (created_at, id) < ($1, $2) ORDER BY created_at DESC, id DESC LIMIT 50`. Especially relevant for moderation-action lists in the dashboard.
- **`COUNT(*)` on huge tables** for UI badges — expensive exact count. Use `pg_class.reltuples` estimate, or skip the count entirely.
- **`LIKE '%foo%'`** can't use a btree. For spam-pattern fuzzy match consider `pg_trgm` + `gin`, but normalize+hash beats fuzzy match here.
- **`NOT IN (subquery)`** with nullable columns returns unexpected rows. Use `NOT EXISTS`.
- **Functions on indexed columns**: `WHERE date(created_at) = $1` bypasses an index. Rewrite as range: `created_at >= $1 AND created_at < $1 + interval '1 day'`.

## Writes / locks

- **Batch inserts** with `INSERT ... VALUES (...), (...), (...)` or `UNNEST($1::bigint[], $2::text[])`. One round trip.
- **`ON CONFLICT ... DO NOTHING`** for the action ledger — atomic, idempotent. The pattern for spam_messages: `INSERT ... ON CONFLICT (xxh3_hash) DO UPDATE SET hit_count = spam_messages.hit_count + 1, last_seen = NOW()`.
- **Avoid long-held row locks** in update handlers. `SELECT ... FOR UPDATE` on `chat_config` is fine because the row is small and writes are infrequent; on `moderation_actions` it would be a deadlock magnet.
- **Bulk updates**: `UPDATE t SET ... WHERE id = ANY($1::uuid[])` instead of a loop.

## Migrations

- Long `ALTER TABLE` can lock the table. For big tables: add nullable column → backfill in batches → add NOT NULL / check constraint with `NOT VALID` then `VALIDATE CONSTRAINT` → done.
- `CREATE INDEX CONCURRENTLY` — own migration file, no transaction wrapper. See [server/docs/rules/migrations.md](../../../../server/docs/rules/migrations.md).

## Locking strategy

- `SELECT ... FOR UPDATE` — strongest; blocks readers + writers; use only when DELETE or PK modification is part of the same transaction.
- `SELECT ... FOR NO KEY UPDATE` — vixen default for UPDATE-then-COMMIT; doesn't block FK references.
- `SELECT ... FOR SHARE` — readers; prevents concurrent UPDATE while you verify invariants.
- `NOWAIT` — fail fast instead of waiting on a contended row (good for user-facing handlers).
- `SKIP LOCKED` — for queue-style workers picking the next free row.

See `transaction-discipline` skill for the surrounding transactional patterns.

## Related

- Query syntax + SQLx macros: see `sqlx-query` skill.
- Schema changes: see `add-migration` skill.
- Schema reference: [server/docs/database.md](../../../../server/docs/database.md).
