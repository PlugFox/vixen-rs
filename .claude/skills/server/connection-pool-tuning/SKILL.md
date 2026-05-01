---
name: connection-pool-tuning
description: Tune sqlx PgPool size, idle_timeout, max_lifetime, acquire_timeout. Diagnose pool exhaustion and 'pool timed out' errors.
---

# Connection Pool Tuning (Vixen server)

**Source:** [SQLx PoolOptions docs](https://docs.rs/sqlx/latest/sqlx/pool/struct.PoolOptions.html).

**Read first:**

- [server/docs/database.md](../../../../server/docs/database.md).

## Defaults for vixen (single-process)

- `min_connections = 2` — pre-warm; cuts first-request latency.
- `max_connections = 10` — single-tenant doesn't need more; Postgres `max_connections` (default 100) is comfortably above.
- `idle_timeout = 600s` — close idle conns after 10 min (cost saver on managed DB).
- `max_lifetime = 30 min` — retire conns periodically; prevents long-lived bugs (TCP wedge, server upgrade).
- `acquire_timeout = 5s` — bound wait time so pool exhaustion fails fast instead of cascading.
- `test_before_acquire = true` — ping before returning; catches stale conns after network blips.

## Skeleton

```rust
let pool = sqlx::postgres::PgPoolOptions::new()
    .min_connections(2)
    .max_connections(10)
    .idle_timeout(Duration::from_secs(600))
    .max_lifetime(Duration::from_secs(1800))
    .acquire_timeout(Duration::from_secs(5))
    .test_before_acquire(true)
    .connect(&database_url).await?;
```

## Diagnose pool exhaustion

- **Symptom**: `error: pool timed out while waiting for an open connection`.
- **Causes**: long-running transactions, blocking syscalls inside async, leaked conns (forgot to drop).
- **Tools**:
  - Postgres-side: `SELECT * FROM pg_stat_activity WHERE application_name LIKE 'vixen%';` — see what each conn is doing.
  - sqlx-side: `pool.size()` (total open) and `pool.num_idle()` (free).
  - Add a `/health/pool` endpoint that exposes both for ops.

## Common bugs

- `let _conn = pool.acquire().await?` — unused holding; drop explicitly or scope-bound.
- `.fetch_all()` returning 100k rows — fix with keyset pagination, not a bigger pool.
- HTTP call inside a DB transaction — externalize or split. The conn is held the entire tx.

## Long-running transactions

- Hold conn for the whole transaction; if you `bot.send_*().await` inside, you've held it 30s+.
- **Rule:** no external I/O inside `pool.begin() ... tx.commit()`.
- See `transaction-discipline` skill.

## Managed DB notes

- **Neon / Supabase** pause idle conns after ~5 min — set `idle_timeout` lower than their pause threshold to avoid cold-start latency on next use.
- **PgBouncer** in transaction mode breaks `LISTEN/NOTIFY` and prepared statements; use session mode or skip the pooler. SQLx prepared-statement cache assumes session-pinning.

## Verification

- `cargo test --lib pool` (sanity).
- Load test: hit `/health` 50 concurrent for 60s; pool size should stay ≤ max, idle should recover.
- Watch `pg_stat_activity.state_change` to confirm conns return to `idle`, not stuck `idle in transaction`.

## Related

- `transaction-discipline`, `postgres-optimization`, `tracing-spans`.
