---
name: sqlx-query
description: Write a compile-time-checked SQLx query for the vixen-rs Postgres database using query!/query_as! macros, keep the .sqlx/ offline cache in sync, and avoid N+1 patterns. Use when adding or modifying a DB query, struct, or anywhere `sqlx::query` appears.
---

# SQLx Query (Vixen server)

## Prefer macros

- `sqlx::query!` — untyped rows, ad-hoc.
- `sqlx::query_as!(Type, "...")` — returns `Type` directly, avoids manual `row.get()`.
- `sqlx::query_scalar!` — single column, single value.

Compile-time checking needs either a live DB (`DATABASE_URL`) or the offline cache in `server/.sqlx/`.

## Offline cache

CI runs with `SQLX_OFFLINE=true`. If `.sqlx/` is stale, the build fails. After changing any query:

```bash
cd server && cargo sqlx prepare -- --all-targets
git add .sqlx
```

Or just run `/db-migrate` — it also refreshes `.sqlx/` when migrations change.

## Write queries as parameterized SQL

- **Never** `format!()` user input into the SQL string. Use `$1, $2, ...` placeholders.
- Type annotations when the macro can't infer: `SELECT id AS "id!: Uuid" FROM ...`.
- For nullable columns: `SELECT col AS "col?" FROM ...` yields `Option<T>`.
- For arrays of Telegram IDs: `$1::bigint[]` and pass `&Vec<i64>`.

## Vixen specifics

- **Telegram IDs**: bind as `i64`, never narrow to `i32`.
- **xxh3 spam hashes**: bind as `i64` (Postgres has no unsigned 64-bit type — xxh3-64 fits in `BIGINT` because Postgres `BIGINT` is signed 64-bit; reinterpret bits via `u64::cast_signed()` / `i64::cast_unsigned()` consistently).
- **Per-chat config**: when reading JSONB settings, use `SELECT settings->>'clown_chance' AS "clown_chance: i16"` with explicit type annotation.

## Transactions

Use `sqlx::Transaction` for multi-statement writes:

```rust
let mut tx = pool.begin().await?;
sqlx::query!("DELETE FROM captcha_challenges WHERE id = $1", id).execute(&mut *tx).await?;
sqlx::query!("INSERT INTO verified_users (chat_id, user_id) VALUES ($1, $2)", chat, user).execute(&mut *tx).await?;
tx.commit().await?;
```

Passing `&mut *tx` to the executor slot is the idiomatic way — `&mut tx` won't compile.

**Mandatory transactions in vixen**:
- Captcha solve: delete challenge + insert verified_user must be atomic.
- Per-chat config update: `SELECT ... FOR UPDATE` then `UPDATE`.
- Moderation action: insert action ledger row + side-effect (ban call etc.) — at minimum the ledger insert should be guarded by a uniqueness key so retries are safe.

## Avoid N+1

- Fetch parents + children in one query with `JOIN` + `json_agg` or two queries + in-memory merge.
- Do **not** loop with `for id in ids { query!("SELECT ... WHERE id = $1", id) }`.
- Use `= ANY($1::bigint[])` for batched Telegram-ID lookups.

## Typical pitfalls

- Integer overflow: `COUNT(*)` returns `i64`; cast or store appropriately.
- `RETURNING` on INSERT is cheaper than a follow-up SELECT.
- Use `.fetch_optional()` for "may or may not exist", then `.ok_or(AppError::NotFound)?`.
- `.fetch_all()` + then `.len()` is wasteful — use `query_scalar!` with `COUNT(*)`.
- Don't put raw query strings in error messages (leaks schema).

## Performance patterns

- Avoid `SELECT *` in macros — name columns explicitly so the result struct matches and fetches stay lean.
- Batch reads via `WHERE id = ANY($1::bigint[])` to replace N+1 lookups (e.g., resolving multiple `verified_users` in one query).
- Keyset pagination for moderation-action lists: `WHERE (created_at, id) < ($1::timestamptz, $2::uuid) ORDER BY created_at DESC, id DESC LIMIT 50`. Skip OFFSET — it gets slow on big tables.
- Idempotent inserts to the moderation ledger:
  ```rust
  sqlx::query!(
      "INSERT INTO moderation_actions (chat_id, target_user_id, action, message_id, ...) \
       VALUES ($1, $2, $3, $4, ...) \
       ON CONFLICT (chat_id, target_user_id, action, message_id) DO NOTHING",
      chat_id, target_user_id, action, message_id, ...
  ).execute(&mut *tx).await?;
  ```

## Related

- Error mapping: see `rust-error-handling` skill.
- New tables/columns: see `add-migration` skill.
- Schema reference: [server/docs/database.md](../../../../server/docs/database.md).
