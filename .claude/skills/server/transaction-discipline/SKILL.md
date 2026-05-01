---
name: transaction-discipline
description: Use SQLx transactions for multi-statement writes (captcha solve, per-chat config, moderation ledger). FOR UPDATE / FOR NO KEY UPDATE / SKIP LOCKED choices and async drop pitfalls.
---

# Transaction Discipline (Vixen server)

**Source:** [Postgres explicit locking](https://www.postgresql.org/docs/current/explicit-locking.html) + vixen [CLAUDE.md](../../../../CLAUDE.md) critical rules.

**Read first:**

- [server/docs/rules/migrations.md](../../../../server/docs/rules/migrations.md).
- [server/docs/database.md](../../../../server/docs/database.md).

## When a transaction is mandatory

- **Captcha solve** — DELETE challenge + INSERT verified_user + UPDATE chat member status. Three statements, single atomic outcome.
- **Per-chat config write** — `SELECT ... FOR UPDATE` on the chat row + UPDATE. No read-modify-write outside a transaction.
- **Moderation action** — INSERT ledger row + side-effect (ban/kick) reconciliation. The ledger insert + the "did we already act?" check share one tx.

## Skeleton

```rust
let mut tx = pool.begin().await?;
sqlx::query!("SELECT spam_weights FROM chat_config WHERE chat_id = $1 FOR UPDATE", chat_id)
    .fetch_one(&mut *tx).await?;
sqlx::query!("UPDATE chat_config SET spam_weights = $1 WHERE chat_id = $2", weights, chat_id)
    .execute(&mut *tx).await?;
tx.commit().await?;
```

- Pass `&mut *tx` to executors (NOT `&mut tx` — that's a `&mut Transaction`, won't deref).
- Drop without commit → automatic rollback. No `unwrap`-then-leak hazard.

## Lock strength

- `FOR UPDATE` — strongest; blocks readers/writers. Use only when you'll DELETE or change the primary key.
- `FOR NO KEY UPDATE` — default for vixen UPDATE-then-COMMIT patterns; doesn't block FK references.
- `FOR SHARE` — readers; prevents concurrent UPDATE while you verify invariants.
- `NOWAIT` — fail immediately if the row is locked (use for user-facing requests).
- `SKIP LOCKED` — gold for queue-style workers (`SELECT ... FROM jobs WHERE ... FOR UPDATE SKIP LOCKED LIMIT 10`).

## Idempotency on conflict

```rust
sqlx::query!(
    "INSERT INTO moderation_actions (chat_id, target_user_id, action, message_id, ...) \
     VALUES ($1, $2, $3, $4, ...) \
     ON CONFLICT (chat_id, target_user_id, action, message_id) DO NOTHING",
    chat_id, user_id, action, msg_id, ...
).execute(&mut *tx).await?;
```

- Re-runs on retry are O(1); no double-bans, no double-deletes.
- Spam pipeline must check the ledger BEFORE the side-effect (see CLAUDE.md "Spam detection is idempotent").

## Async drop / panic safety

- Tokio has no async Drop. If a task is cancelled mid-transaction, the connection returns to the pool dirty; sqlx aborts cleanly but the next user must not assume it.
- For panic safety: wrap risky inner work in `tokio::task::spawn` and `await` the `JoinHandle` so panics surface as `JoinError` and log via tracing.
- Never `?` past `tx.commit()` without thinking — early returns abort the whole tx.

## Gotchas

- **Long transactions starve the pool.** Keep them under a few hundred ms. Never `.await` external HTTP inside.
- **Don't `bot.send_*().await` inside a transaction** — Telegram API may stall 30s while you hold a row lock. Externalize the side-effect or split the unit of work.
- **`serializable` isolation is overkill** for vixen; default `read committed` + explicit locks is correct.
- **Read-only multi-statement** queries don't need `pool.begin()` — issue them on a single conn via `pool.acquire()` if consistency across statements matters.

## Verification

- `cargo test transaction --lib`.
- `EXPLAIN (ANALYZE, BUFFERS)` to confirm the lock acquired matches what you intended (use postgres MCP).
- Stress test: hammer the endpoint with 50 concurrent writers; check no rows lost, no duplicates.

## Related

- `add-migration`, `sqlx-query`, `postgres-optimization`, `connection-pool-tuning`.
