---
name: seed-test-chat
description: Bootstrap dev DB / integration tests with a fake watched chat, moderators, captcha challenges, spam messages, moderation actions. Use #[sqlx::test] fixtures or seed-dev binary.
---

# Seed Test Chat (Vixen server)

**Read first:**

- [server/docs/rules/testing.md](../../../../server/docs/rules/testing.md) — test fixtures, `#[sqlx::test]`.
- [server/docs/database.md](../../../../server/docs/database.md) — schema, UNIQUE constraints.

## Two paths

### Integration tests

`server/tests/fixtures/seed.rs` — small helpers, each returns the inserted row or id:

```rust
pub async fn seed_chat(pool: &PgPool, chat_id: i64, title: &str) -> Result<()> { ... }
pub async fn seed_moderator(pool: &PgPool, chat_id: i64, user_id: i64) -> Result<()> { ... }
pub async fn seed_verified_user(pool: &PgPool, chat_id: i64, user_id: i64) -> Result<()> { ... }
pub async fn seed_captcha_challenge(pool: &PgPool, chat_id: i64, user_id: i64) -> Result<Uuid> { ... }
pub async fn seed_spam_message(pool: &PgPool, chat_id: i64, hash: i64, text: &str) -> Result<()> { ... }
pub async fn seed_moderation_action(pool: &PgPool, chat_id: i64, target: i64, action: &str) -> Result<i64> { ... }
```

Use `#[sqlx::test]` for a fresh DB per test — never share state.

### Local dev DB

`cargo run --bin seed-dev -- --chat-id -1001234567890 --owner 42` (binary in `server/bin/seed-dev.rs`) inserts one chat with one moderator. Ready for manual testing against the real bot.

## Conventions

- **Telegram IDs are `i64`.** Use realistic supergroup IDs `-1001234567890` (always negative for supergroups, leading `-100`).
- **Time**: `NOW()` (SQL) or `Utc::now()` (Rust) — never local time.
- **Compile-time-checked inserts.** `sqlx::query!` so `.sqlx/` stays in sync. The fixtures are also a sanity check on schema drift.
- **Deterministic IDs across runs** make assertions readable; opaque UUIDs for challenges are fine.

## Gotchas

- **UNIQUE constraints.** `verified_users(chat_id, user_id)` and `captcha_challenges(chat_id, user_id)` will throw on re-seed. Use `INSERT ... ON CONFLICT DO NOTHING` or `DELETE` first when re-running locally.
- **No production IDs.** Don't seed real Telegram chat or user IDs — pick fake numbers.
- **Foreign keys.** Seed `chats` before `chat_moderators` / `verified_users`.

## Verification

- `cargo test integration`.
- `cargo run --bin seed-dev -- --chat-id -1001234567890 --owner 42`, then query the DB to confirm rows.

## Related

- `rust-testing` — `#[sqlx::test]` patterns, mocks.
- `add-migration` — schema changes that break fixtures.
