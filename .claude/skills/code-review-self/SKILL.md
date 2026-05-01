---
name: code-review-self
description: Self-review before push — read the diff as a reader, scan for vixen footguns (token leaks, PII, idempotency, captcha asset overwrites), run validation pipeline, no surprise files. Use before "git push", before opening a PR, or when the user says "ready to commit".
---

# Code Review (Self) (Vixen)

**Source:** vixen `review-pr` skill + standard self-review.

**Read first:** [`review-pr/SKILL.md`](../review-pr/SKILL.md), [`CLAUDE.md`](../../CLAUDE.md) Critical Rules.

## Before push, run

1. `git diff --staged` — read every line. If you don't, the reviewer will.
2. `/server-check` and/or `/website-check` — depending on what changed.
3. `git status` — no surprise files staged. Watch for `.env`, large binaries, `node_modules`, generated files.

## Read as a reviewer

- Variable names clear at the call site, not just the definition?
- Functions doing one thing? If a name is `do_foo_and_bar`, split it.
- Comments explain *why*, not *what*. The diff shows what.
- Error messages actionable — tell the operator how to fix, not just what failed.
- Tests cover the change. Diff a test file present in the staged set? If no, why not?

## Vixen footgun scan

Grep the diff for:

- `token`, `bot_token`, `TELEGRAM_BOT_TOKEN`, `OPENAI_API_KEY`, `JWT_SECRET` in any new `tracing::*!` / `println!` / `eprintln!` — must use `RedactedToken` newtype from `server/src/utils/redact.rs`.
- `init_data` / `initData` in any non-`debug!` log — initData carries user PII.
- Phone numbers, full names, message bodies in logs above `debug` level.
- New `INSERT INTO moderation_actions` without `ON CONFLICT (chat_id, target_user_id, action, message_id) DO NOTHING` — breaks idempotency.
- New file under `server/assets/captcha/` overwriting an existing path — captcha assets are immutable; bump the selector version instead.
- Per-chat config write outside a transaction — should be `SELECT ... FOR UPDATE` then `UPDATE`.
- `i32` where it should be `i64` (any Telegram ID — chat, user, message). Supergroup IDs overflow `i32`.
- i18n key added in `en` only — must be in *every* locale file under `website/i18n/messages/`.
- SolidJS `props.x` destructured (`const { x } = props`) — breaks reactivity.
- TypeScript `any`, `enum`, default exports outside `pages/`.

## Commit message check

- Conventional Commits format: `feat(scope):`, `fix(scope):`, `refactor(scope):`, `docs(scope):`, `chore(scope):`.
- Scope from the allowed list: `server`, `api`, `db`, `auth`, `bot`, `captcha`, `spam`, `jobs`, `reports`, `config`, `website`, `ui`, `i18n`, `docker`, `ci`, `deps`.
- Body explains *why*, not *what*. The diff shows what.
- One logical change per commit. Don't mix "rename function" with "add new feature."

## CHANGELOG

User-visible change → entry under `[Unreleased]` with `(server)` / `(website)` / `(infra)` tag, grouped under `Added` / `Changed` / `Fixed` / `Removed` / `Security`. See [`bump-version/SKILL.md`](../bump-version/SKILL.md).

Skip CHANGELOG only for trivial internal changes (formatting, comment tweaks, refactors with no observable effect).

## Sign-off

State explicitly before declaring ready:

> Ran `/server-check`, all green. No footguns: no token in logs, no overwritten captcha asset, idempotent insert. CHANGELOG updated under `[Unreleased]` (server).

If you can't say that sentence, you're not ready.

## Related

- `commit-message` — Conventional Commits format details.
- `review-pr` — what an external reviewer will check.
- `verifiable-goal` — proves the change works.
- `bump-version` — patch / minor / major + CHANGELOG.
