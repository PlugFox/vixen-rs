---
name: per-chat-config
description: Add a per-chat configurable knob (spam_threshold, report_hour, captcha_difficulty) — migration → ChatConfig struct → API DTO → transactional update endpoint.
---

# Per-Chat Config (Vixen server)

**Read first:**

- [server/docs/database.md](../../../../server/docs/database.md) — `chat_config` schema.
- [server/docs/config.md](../../../../server/docs/config.md) — knob registry.
- [server/docs/rules/api-routes.md](../../../../server/docs/rules/api-routes.md) — endpoint conventions.
- [server/docs/rules/migrations.md](../../../../server/docs/rules/migrations.md) — schema bump rules.

## Steps

1. **Migration**: `ALTER TABLE chat_config ADD COLUMN {name} {type} NOT NULL DEFAULT ...` plus matching `.down.sql`. If the column is `NOT NULL` and existing rows must take a non-default value, backfill with an `UPDATE` in the same migration.
2. Update `ChatConfig` in `server/src/models/chat_config.rs` and DTO in `server/src/api/dto.rs`.
3. Expose `PATCH /api/v1/chats/{chat_id}/config` in `server/src/api/routes_config.rs`. Validate the moderator's chat scope server-side — don't trust the JWT's `chat_ids` claim alone.
4. Inside the handler:

```rust
let mut tx = state.pool.begin().await?;
let cfg: ChatConfig = sqlx::query_as!(ChatConfig,
    "SELECT * FROM chat_config WHERE chat_id = $1 FOR UPDATE", chat_id)
    .fetch_one(&mut *tx).await?;
sqlx::query!("UPDATE chat_config SET {name} = $1 WHERE chat_id = $2", value, chat_id)
    .execute(&mut *tx).await?;
tx.commit().await?;
```

5. If cache-dependent (e.g., `spam_weights` cached in Moka), invalidate by chat_id key after commit.
6. Update `server/docs/database.md` (schema row) and `server/docs/config.md` (knob table).

## Gotchas

- **Transaction is not optional.** Concurrent moderator changes race otherwise — read-modify-write outside a transaction loses writes.
- **Server-side moderator check is mandatory.** `chat_id` in the path is attacker-controlled; the JWT only proves the user is *some* moderator somewhere.
- **`NOT NULL` + existing rows** requires a `DEFAULT` in the ALTER or a backfill `UPDATE` in the same migration.
- **Validate value range** (`spam_threshold ∈ 0..=100`, `report_hour ∈ 0..=23`) before `UPDATE`. Return `AppError::BadRequest` with a useful message.
- **Cache invalidation is manual.** Document in `server/docs/config.md` which knobs are cached and which aren't.
- **Telegram IDs `i64`.** `chat_id BIGINT` everywhere.

## Verification

- `cargo sqlx prepare --check`.
- `cargo test config_update` — happy path + concurrent-write test.

## Related

- `add-migration` — schema-bump conventions.
- `add-api-route` — middleware stack and OpenAPI.
- `transaction-discipline` — `FOR UPDATE` patterns.
