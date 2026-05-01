---
description: Apply SQLx migrations and refresh offline query cache
allowed-tools: Bash, Read
---

Apply pending SQLx migrations against the local dev database, then refresh the offline query cache.

Prerequisites: Postgres is running (run `/db-up` first if not).

Before creating a new migration, read [server/docs/rules/migrations.md](../../server/docs/rules/migrations.md) for project conventions (down files, idempotency, no-rewrite-of-applied policy, BIGINT for Telegram IDs, ON DELETE CASCADE on `chats(chat_id)` foreign keys).

Steps:

1. `cd server && sqlx migrate run` with `DATABASE_URL="postgresql://vixen:vixen_dev_password@localhost:5432/vixen"`.
2. If step 1 applied at least one migration: `cd server && cargo sqlx prepare -- --all-targets` to refresh `.sqlx/` so CI with `SQLX_OFFLINE=true` keeps working.
3. Report: which migrations were applied, and whether `.sqlx/` changed (check via `git status server/.sqlx`).

If migration fails: show the error, do not attempt to rollback automatically. Ask the user before running `sqlx migrate revert`.
