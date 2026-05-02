# Vixen-rs

Telegram anti-spam bot. Single-tenant. Rust + PostgreSQL backend, SolidJS dashboard + public reports. Monorepo.

## Structure

- `server/` — Rust backend (Axum + SQLx + PostgreSQL + teloxide). The bot poller, REST API, background jobs, captcha generator, spam pipeline.
- `website/` — TypeScript frontend (SolidJS + Kobalte + Tailwind + Vite + bun). Moderator dashboard + public chat reports.
- `docker/` — Docker Compose (PostgreSQL, server, website).
- `.github/` — CI/CD workflows.

## Documentation

- Project-wide LLM conventions: [AGENTS.md](AGENTS.md)
- Server docs: [server/docs/](server/docs/)
- Server LLM rules: [server/docs/rules/](server/docs/rules/)
- Website docs: [website/docs/](website/docs/)
- Website LLM rules: [website/docs/rules/](website/docs/rules/)
- Cross-cutting docs: [docs/](docs/) (architecture, deployment, features, roadmap)

## Claude Code

This repo is pre-configured for Claude Code. Relevant files:

- [.claude/settings.json](.claude/settings.json) — shared permissions, deny-list (incl. bot-token leak prevention), Stop-hook. Source of truth for what commands run without approval.
- [.claude/commands/](.claude/commands/) — project slash commands:
  - `/server-check` — `cargo fmt && clippy && test && sqlx prepare --check` in [server/](server/)
  - `/website-check` — `bun run check && typecheck && build` in [website/](website/)
  - `/db-up` — start local Postgres
  - `/db-migrate` — apply SQLx migrations + refresh `.sqlx/` offline cache
  - `/bot-token` — verify the Telegram bot token via `getMe` (token never echoed)
  - `/tg-init-debug` — validate a Telegram WebApp `initData` payload during dev
- [.claude/skills/](.claude/skills/) — project skills, auto-loaded when the task matches the skill's description. Each skill references the relevant rules in `server/docs/rules/` or `website/docs/rules/`. Index: [.claude/skills/README.md](.claude/skills/README.md).
- [.claude/hooks/stop-reminder.sh](.claude/hooks/stop-reminder.sh) — Stop-hook that prints the right validation checklist based on which paths changed.
- [.mcp.json](.mcp.json) — MCP servers available in this repo:
  - **postgres** — read-only access to the local dev database. Use for schema inspection, EXPLAIN, verifying migrations without `docker compose exec`.
  - **github** — PRs, issues, workflow runs. Requires `GITHUB_PERSONAL_ACCESS_TOKEN` exported in your shell before launching Claude Code.

## Key Commands

```bash
# Infrastructure (Docker / PostgreSQL)
docker compose -f docker/docker-compose.yml up -d postgres

# Server (Rust / Axum / SQLx / teloxide)
cd server && cargo run                    # Run server: HTTP + bot poller (http://localhost:8000)
cd server && cargo test                   # Run tests
cd server && cargo fmt && cargo clippy    # Format + lint
cd server && sqlx migrate run             # Run DB migrations

# Website (TypeScript / SolidJS)
cd website && bun install                 # Install dependencies
cd website && bun run dev                 # Dev server (http://localhost:3000)
cd website && bun run build               # Production build
cd website && bun run check               # Biome lint + format check
cd website && bun run typecheck           # tsc --noEmit
```

## Conventions

- **Communication**: Russian with the user. English for all code, comments, docs, and commits.
- **Commits**: Conventional commits (feat, fix, refactor, docs, chore). See [.claude/skills/commit-message/SKILL.md](.claude/skills/commit-message/SKILL.md) for scopes.
- **Server**: Rust edition 2024, `cargo fmt` + `cargo clippy` before commits.
- **Website**: TypeScript strict, Biome for lint/format, bun as package manager.

## Critical Rules

- Before substantial work, read [AGENTS.md](AGENTS.md), then the per-component CLAUDE.md ([server/CLAUDE.md](server/CLAUDE.md) or [website/CLAUDE.md](website/CLAUDE.md)).
- **Telegram chat IDs and user IDs are `i64` / `BIGINT`.** Never `i32` (overflows on supergroup IDs `-100…`), never `u64`. This is non-negotiable across the whole stack. Telegram message IDs (`teloxide::types::MessageId`) are the exception — they fit in `i32` / `INTEGER` per teloxide's type, so the columns `moderation_actions.message_id` and `captcha_challenges.telegram_message_id` are `INTEGER`.
- **Captcha assets in `server/assets/captcha/` are immutable.** Do not edit existing files. To change a captcha look, add a new asset file and bump the version selector. Existing pending challenges reference asset paths verbatim — overwriting breaks deterministic re-rendering tests.
- **Never log bot tokens, raw `initData`, or user PII to public logs.** Use the `RedactedToken` newtype from `server/src/utils/redact.rs` for any `tracing` call that might surface a token. Phone numbers, full names, message bodies are opt-in only.
- **Telegram WebApp `initData` is HMAC-validated server-side on every request.** HMAC-SHA256 with `key = HMAC_SHA256("WebAppData", bot_token)` per Telegram spec; reject `auth_date` older than 24h. The website never trusts `Telegram.WebApp.initDataUnsafe` — it always submits the raw signed `initData` string.
- **Per-chat config writes are transactional.** `SELECT ... FOR UPDATE` on the chat row plus an `UPDATE`. No read-modify-write outside a transaction.
- **Spam detection is idempotent.** Re-processing the same message (xxhash duplicate, retry after restart) must not double-ban or double-delete. Always check the `moderation_actions` ledger (uniqueness key `(chat_id, target_user_id, action, message_id)`) before acting.
- When modifying API or database schema, update the corresponding file in [server/docs/](server/docs/).
- Before writing migrations → [server/docs/rules/migrations.md](server/docs/rules/migrations.md).
- Before adding API routes → [server/docs/rules/api-routes.md](server/docs/rules/api-routes.md).
- Before adding Telegram handlers → [server/docs/rules/telegram-handlers.md](server/docs/rules/telegram-handlers.md).
- Before adding background jobs → [server/docs/rules/background-jobs.md](server/docs/rules/background-jobs.md).
- Before writing SolidJS components → [website/docs/rules/solidjs.md](website/docs/rules/solidjs.md).
- Before creating UI components → [website/docs/rules/components.md](website/docs/rules/components.md).
- Before writing TypeScript code → [website/docs/rules/typescript.md](website/docs/rules/typescript.md).
- After completing any user-visible change (feature, fix, perf, security, breaking change), add an entry to [CHANGELOG.md](CHANGELOG.md) under `[Unreleased]` and bump the patch version in `server/Cargo.toml` or `website/package.json`. Skip CHANGELOG only for trivial internal-only changes (formatting, comment tweaks). Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) — group entries under `Added` / `Changed` / `Fixed` / `Removed` / `Security` and tag each with `(server)` / `(website)` / `(infra)`.

## Before Writing Code

For trivial fixes (typos, one-line changes, simple renames), skip discussion and just do it.

For anything non-trivial, do NOT start implementation until all open questions are resolved. First:

1. **Challenge the approach** — point out flaws, missed edge cases, and security risks. Be direct, not polite.
2. **Ask about unknowns** — if anything is ambiguous, ask. Do not guess or assume.
3. **Propose alternatives** — if there is a simpler or more robust way, say so and explain why.
4. **List edge cases** — enumerate what can break: concurrent access, empty inputs, large payloads, permission gaps, migration rollbacks, Telegram API outages, CAS API timeouts, captcha-asset upgrades.
5. **Wait for confirmation** — do not write code until the user explicitly approves the plan.

Do only what was asked. Do not refactor surrounding code, add comments to code you did not change, or introduce abstractions for hypothetical future needs. **Every changed line should trace directly to the user's request** — if you cannot point at the sentence that motivates a line, remove it.

**Simplicity first.** If you write 200 lines and the same result fits in 50, rewrite it. No speculative abstractions, no error-handling for situations that cannot occur, no configuration knobs nobody asked for.

Be blunt. Point out bad ideas. Disagree when you have a reason. The goal is a correct implementation, not a fast one.

## After Writing Code

Convert the task into a **verifiable goal** before declaring it done. "Add validation" → "tests for invalid inputs fail before the change and pass after." "Fix the bug" → "a test reproduces it, and now passes." For multi-step work, break into steps with a verifiable checkpoint per step.

Then run the validation pipeline:

- **Server**: `cargo fmt`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`, `cargo sqlx prepare --check`.
- **Website**: `bun run check`, `bun run typecheck`, `bun run build`.

If tests or build fail, fix the issue before reporting completion.

**Never silence a failing test, lint, or type error to make it pass.** A red signal is, by default, evidence that the production code is broken — not that the check needs to be loosened. Before touching the test/check:

1. Reproduce the failure and read what it actually asserts.
2. Decide which side is wrong: the code under test, or the test (stale assertion, wrong fixture, flaky timing). The burden of proof is on declaring the test wrong, and that proof must be specific (which assertion, why it no longer reflects desired behavior).
3. If the code is wrong → fix the code. If the test is genuinely wrong (outdated, badly written, asserts behavior that was deliberately changed) → fix the test AND state in the commit message why the old assertion no longer holds.
4. Never weaken an assertion, delete a case, loosen a tolerance, comment out a check, or slap `#[ignore]` / `it.skip` / `xit` on a test just to get CI green. If a test must be temporarily disabled, open a tracking issue, link it from the test, and treat it as a bug.
5. Same rule for lints and types — fix the underlying problem; do not cast to `any`, add `#[allow(...)]` / `// biome-ignore` / `// @ts-ignore` to make the diagnostic disappear.

The "Before / After Writing Code" guidance above incorporates formulations from [multica-ai/andrej-karpathy-skills](https://github.com/multica-ai/andrej-karpathy-skills). Available project skills are indexed in [.claude/skills/README.md](.claude/skills/README.md).
