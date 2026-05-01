---
name: review-pr
description: Review a pull request (or uncommitted diff) against vixen-rs conventions — server rules in server/docs/rules/ and website rules in website/docs/rules/. Use when the user asks to review a PR, review changes, or asks "what do you think of this diff".
---

# PR Review (Vixen)

## Scope

Review focuses on **correctness and convention adherence**, not style nits (Biome and `cargo fmt` handle formatting).

## Gather the diff

- Current branch vs `master`: `git diff master...HEAD` and `git log master..HEAD --oneline`.
- Specific PR: `gh pr view <n> --json title,body,files` + `gh pr diff <n>`.
- MCP GitHub: `mcp__github__get_pull_request_files` for a structured file list.

## Read the rules first

Before commenting, skim the rules that apply to the changed files:

- Any `server/**.rs` → [server/CLAUDE.md](../../../server/CLAUDE.md), [server/docs/rules/rust.md](../../../server/docs/rules/rust.md).
- `server/migrations/**.sql` → [server/docs/rules/migrations.md](../../../server/docs/rules/migrations.md).
- `server/src/api/routes_*.rs` → [server/docs/rules/api-routes.md](../../../server/docs/rules/api-routes.md).
- `server/src/telegram/**` → [server/docs/rules/telegram-handlers.md](../../../server/docs/rules/telegram-handlers.md).
- `server/src/jobs/**` → [server/docs/rules/background-jobs.md](../../../server/docs/rules/background-jobs.md).
- `server/src/**` error types → [server/docs/rules/error-handling.md](../../../server/docs/rules/error-handling.md).
- `website/src/**/*.tsx` → [website/docs/rules/solidjs.md](../../../website/docs/rules/solidjs.md) + [components.md](../../../website/docs/rules/components.md).
- `website/src/**/*.ts` → [website/docs/rules/typescript.md](../../../website/docs/rules/typescript.md).
- Any UI text → verify i18n keys exist in [website/i18n/messages/](../../../website/i18n/messages/) for **every** locale.

## Checklist

- **Title & commits** follow Conventional Commits.
- **Schema changes** come with a matching update in `server/docs/{api,database}.md` and a fresh `.sqlx/`.
- **New API routes** are listed in `server/docs/api.md` and decorated with `#[utoipa::path(...)]`.
- **New Telegram handlers** are registered in the dispatcher, documented in `server/docs/bot.md`'s slash-command table, and respect the watched-chats filter.
- **New background jobs** are idempotent, respect `CancellationToken`, and have an `info_span!("job", name=...)` wrapper.
- **SolidJS**: no prop destructuring, no `useEffect`, `<Show>/<For>/<Switch>` used.
- **TypeScript**: no `any`, no TS enums, kebab-case filenames, named exports (except `pages/`).
- **No hardcoded user-facing text** — all strings come from i18n; every locale has the new key.
- **Migrations**: have a `.down.sql`, are idempotent where the rule requires it, use `BIGINT` for Telegram IDs.

## Vixen-specific extras (must check)

- **No bot token in any added log statement.** Search the diff for `tracing::*!.*token`, `bot_token`, `TELEGRAM_BOT_TOKEN`. Tokens go through `RedactedToken`.
- **No raw initData logged at info+.** initData carries user PII; only debug-level logs may include it.
- **No raw user PII in logs.** Phone numbers, full names, message bodies — opt-in only.
- **New spam rule** has corpus tests under `server/tests/spam_corpus/<rule>.yaml` (positive + negative samples).
- **Captcha asset paths** never overwrite an existing file in `server/assets/captcha/`. Adding a new look = new file + bumped selector.
- **Per-chat config writes** are wrapped in a transaction.

## Output format

Structure the review as:

1. **Summary** (1–2 sentences): what the PR does and overall verdict.
2. **Must fix**: blocking issues with `file:line` refs.
3. **Should fix**: non-blocking improvements.
4. **Nits**: optional style/polish.

Be direct. Call out bad ideas. Silence on a flaw is worse than bluntness.
