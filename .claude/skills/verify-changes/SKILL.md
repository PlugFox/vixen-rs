---
name: verify-changes
description: Run the correct validation pipeline (server and/or website) before declaring work done. Use before reporting task completion, when the user asks to "check", "validate", "make sure it builds", or when preparing a commit.
---

# Verify Changes (Vixen)

CLAUDE.md is explicit: **do not consider a task done until verified.** This skill picks the right checks based on what changed.

## Decide what to run

Check `git status` and `git diff --name-only` (or the list of files just edited):

| Touched paths | Run |
|---|---|
| `server/**` (any Rust, SQL, config) | `/server-check` |
| `server/migrations/**` | `/db-migrate` first (refreshes `.sqlx/`), then `/server-check` |
| `website/**` (any TS/TSX, CSS, JSON) | `/website-check` |
| `website/i18n/**` | `/website-check` plus a manual diff that the same key appears in every locale file |
| Both areas | both check commands, server first |
| `docker/**`, `.github/**`, root-level docs only | skip — no validation pipeline for these |

## Procedure

1. Invoke the matching slash command(s). Do not run the individual subcommands manually — the slash command has the canonical sequence (incl. `cargo sqlx prepare --check` for server).
2. If a check fails, **stop**. Show the failing output (tail only, not the whole log), identify the root cause, propose a fix.
3. Do not mark the task complete until every check is green.
4. For UI changes, after `/website-check` is green, sanity-check the flow manually (Playwright is not yet wired into this repo).

## What "green" looks like

- `server-check: OK` with a one-line test/warning count.
- `website-check: OK` with a bundle size line.

## Common footguns

- `cargo test` alone is not sufficient — the slash command intentionally also runs `fmt --check`, `clippy -D warnings`, and `sqlx prepare --check`.
- `bun run typecheck` passing does not mean the UI works — for behavior changes, exercise the flow manually.
- If `.sqlx/` changed, add it to the commit or CI with `SQLX_OFFLINE=true` will fail.
- If you added an i18n key in one locale only, the typed surface generation will produce missing-key errors at TS check time.
- After `cd server && cargo sqlx prepare --check`: if it failed, run `/db-migrate` (which regenerates `.sqlx/`), confirm the new files are STAGED, and re-run the check before commit. Missing `.sqlx/` updates → CI fails with `SQLX_OFFLINE=true` cannot find query.
- For async / concurrency changes (new background job, new shared state, new `tokio::select!`), `cargo test` alone is insufficient — also stress-test by manually triggering concurrent calls or shutdown mid-iteration; `tokio_test` and `loom` (for non-async lock testing) are the next levels of rigor.
