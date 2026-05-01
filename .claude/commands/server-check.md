---
description: Run server pre-commit checks (fmt, clippy, test, sqlx prepare check)
allowed-tools: Bash
---

Run the full server validation pipeline and report results.

Execute in order, stop on first failure:

1. `cd server && cargo fmt --all -- --check` — if this fails, run `cargo fmt --all` and commit the diff separately.
2. `cd server && cargo clippy --all-targets --all-features -- -D warnings`
3. `cd server && cargo test`
4. `cd server && cargo sqlx prepare --check -- --all-targets` — fails if `.sqlx/` is out of sync with the live queries; fix by running `/db-migrate` (which refreshes `.sqlx/`) or `cargo sqlx prepare -- --all-targets` directly.

If any step fails: show the failing output (tail only, not the full log), identify the root cause, and propose a fix. Do not auto-fix without confirmation unless it's a pure formatting issue.

If all pass: report `server-check: OK` with a one-line summary (test count, warnings count).
