# Vixen-rs Claude Code Skills

Auto-loaded skills for Claude Code in this repo. Claude picks a skill when the user's request matches its `description` in the frontmatter — you don't invoke them by hand (except general slash commands like `/server-check`).

This file is the index. Each row names the skill, says what it does, and cites where the content came from. Own rules under [server/docs/rules/](../../server/docs/rules/) and [website/docs/rules/](../../website/docs/rules/) remain the canonical project conventions; external skills are adapted to those, not on top of them.

## Conventions for skills in this repo

- Frontmatter has `name` + `description` only. The description is a one-line trigger hint — Claude reads it to decide relevance.
- Body starts with a **Source** line citing the origin when content is adapted from an external skill, and a **Read first** list pointing at the canonical rules in `server/docs/rules/` or `website/docs/rules/`.
- Cross-references use relative markdown links.
- Each skill ends with a `## Related` section linking sibling skills and in-repo docs.
- Top-level skills are meta workflow (`verify-changes`, `commit-message`, `review-pr`, `bump-version`, `plan-before-code`, etc.). Server-specific live under `server/`, website-specific under `website/`, infra under `infra/`.
- Adding a new skill from an external source? Run through [find-external-skill](find-external-skill/SKILL.md) first.

## Index

### Meta — workflow & process

| Skill | Purpose | Source |
|---|---|---|
| [verify-changes](verify-changes/SKILL.md) | Picks the right validation pipeline (`/server-check`, `/website-check`) based on what files changed. | Foxic-native (adapted). |
| [commit-message](commit-message/SKILL.md) | Writes Conventional Commit messages matching repo style; breaking-change footer; co-author rules. | Foxic-native. |
| [bump-version](bump-version/SKILL.md) | Bumps patch version in `server/Cargo.toml` / `website/package.json` after user-visible changes. | Foxic-native. |
| [review-pr](review-pr/SKILL.md) | Reviews a PR / diff against server + website rules + vixen-specific checks (token redaction, no PII, idempotency, captcha asset overwrites). | Foxic-native. |
| [plan-before-code](plan-before-code/SKILL.md) | Before non-trivial code: challenge approach, ask unknowns, propose alternatives, list edge cases, get sign-off. | [CLAUDE.md "Before Writing Code"](../../CLAUDE.md) + [karpathy-skills](https://github.com/multica-ai/andrej-karpathy-skills). |
| [verifiable-goal](verifiable-goal/SKILL.md) | Convert any task to a test that fails before / passes after. Don't declare done without verification. | [CLAUDE.md "After Writing Code"](../../CLAUDE.md). |
| [code-review-self](code-review-self/SKILL.md) | Self-review before push: read diff, scan vixen footguns (tokens, PII, idempotency, asset overwrites), run validation. | Foxic-native. |
| [debug-systematically](debug-systematically/SKILL.md) | Reproduce → bisect → root cause → fix → verify. No "add more error handling" as a guess. | Standard debug discipline. |
| [change-impact-assessment](change-impact-assessment/SKILL.md) | Before non-trivial change: list affected DB / API / handlers / jobs / TS types / docs. Catch ripples upfront. | Vixen monorepo + [GSD](https://github.com/gsd-build/get-shit-done). |
| [pr-description](pr-description/SKILL.md) | Writes the PR body — Why / What / Risk / Test plan; uses `gh pr create --body` with HEREDOC. | Foxic + standard PR practice. |
| [find-external-skill](find-external-skill/SKILL.md) | Evaluates a 3rd-party Claude skill before importing — source trust, duplicate check, rule extraction, vixen-fit. | Vixen meta. |

### Server (Rust / Axum / SQLx / Postgres / teloxide)

#### Foundations (apply to any server work)

| Skill | Purpose | Source |
|---|---|---|
| [server/add-api-route](server/add-api-route/SKILL.md) | Scaffolds a new Axum route per [server/docs/rules/api-routes.md](../../server/docs/rules/api-routes.md). | Foxic-native. |
| [server/add-migration](server/add-migration/SKILL.md) | Creates a SQLx migration per [server/docs/rules/migrations.md](../../server/docs/rules/migrations.md). | Foxic-native. |
| [server/sqlx-query](server/sqlx-query/SKILL.md) | Writes compile-time-checked SQLx queries; `.sqlx/` discipline; keyset pagination; `ON CONFLICT` for the moderation ledger. | Foxic-native. |
| [server/rust-error-handling](server/rust-error-handling/SKILL.md) | Applies `AppError` / `thiserror` / `IntoResponse`; map `sqlx::Error` via `#[from]`; Telegram-send `inspect_err`. | Foxic-native. |
| [server/rust-async-tokio](server/rust-async-tokio/SKILL.md) | Correct async Rust on Tokio — cancel-safety table, `select!`, timeouts, graceful shutdown. | [Tokio docs](https://tokio.rs/tokio/topics) + foxic. |
| [server/rust-testing](server/rust-testing/SKILL.md) | Unit + integration tests; `#[sqlx::test(fixtures("..."))]`; spam-rule corpus tests. | Foxic + [SQLx testing](https://docs.rs/sqlx/latest/sqlx/attr.test.html). |
| [server/postgres-optimization](server/postgres-optimization/SKILL.md) | EXPLAIN, indexes, N+1; lock strength `FOR UPDATE` / `FOR NO KEY UPDATE` / `SHARE` / `NOWAIT` / `SKIP LOCKED`. | Foxic + Postgres docs. |
| [server/transaction-discipline](server/transaction-discipline/SKILL.md) | Use SQLx transactions for multi-statement writes; lock-strength choice; async-drop pitfalls; idempotency via `ON CONFLICT`. | [Postgres explicit locking](https://www.postgresql.org/docs/current/explicit-locking.html) + RFD 400. |
| [server/tracing-spans](server/tracing-spans/SKILL.md) | Structured tracing — `#[instrument(skip(state, body))]`, fields not message text, redaction discipline, level rules. | [Tokio tracing](https://tokio.rs/tokio/topics/tracing). |
| [server/connection-pool-tuning](server/connection-pool-tuning/SKILL.md) | Tune sqlx `PgPool` (size, idle, lifetime, acquire); diagnose pool exhaustion; managed-DB notes. | [SQLx PoolOptions](https://docs.rs/sqlx/latest/sqlx/pool/struct.PoolOptions.html). |
| [server/serde-strict-deserialization](server/serde-strict-deserialization/SKILL.md) | API request DTOs — `deny_unknown_fields`, `deserialize_with` validators, semantic error messages. | [Serde field attrs](https://serde.rs/field-attrs.html). |

#### Vixen subsystems

| Skill | Purpose | Source |
|---|---|---|
| [server/add-telegram-handler](server/add-telegram-handler/SKILL.md) | Adds a teloxide update handler routed through the dispatcher tree. | [server/docs/rules/telegram-handlers.md](../../server/docs/rules/telegram-handlers.md) + [Bot API](https://core.telegram.org/bots/api). |
| [server/add-slash-command](server/add-slash-command/SKILL.md) | Registers a `/slash_command` (BotCommands derive) with permission check + i18n help text. | [teloxide BotCommands](https://docs.rs/teloxide). |
| [server/captcha-pipeline](server/captcha-pipeline/SKILL.md) | Atomic captcha solution + image render + DB row + sendPhoto + restrictChatMember; deterministic; asset immutability. | [server/docs/captcha.md](../../server/docs/captcha.md). |
| [server/spam-rule](server/spam-rule/SKILL.md) | New spam-detection rule (normalize → score → action); corpus tests under `server/tests/spam_corpus/`. | [server/docs/spam-detection.md](../../server/docs/spam-detection.md). |
| [server/background-job](server/background-job/SKILL.md) | Periodic `tokio::spawn` job — `CancellationToken`, idempotency, `tracing::instrument`. | [server/docs/rules/background-jobs.md](../../server/docs/rules/background-jobs.md). |
| [server/tg-webapp-auth](server/tg-webapp-auth/SKILL.md) | Validates Telegram WebApp `initData` HMAC; mints JWT with `chat_ids`; rejects `auth_date > 24h`. | [Telegram WebApp spec](https://core.telegram.org/bots/webapps#validating-data-received-via-the-mini-app). |
| [server/per-chat-config](server/per-chat-config/SKILL.md) | Adds a per-chat configurable knob — migration → struct → API DTO → transactional update. | [server/docs/database.md](../../server/docs/database.md). |
| [server/seed-test-chat](server/seed-test-chat/SKILL.md) | Bootstraps dev DB / integration tests with a fake watched chat, moderators, captcha challenges. | [server/docs/rules/testing.md](../../server/docs/rules/testing.md). |

### Website (SolidJS / Kobalte / Tailwind / Vite / bun)

#### Foundations

| Skill | Purpose | Source |
|---|---|---|
| [website/add-solid-component](website/add-solid-component/SKILL.md) | Writes SolidJS components per project rules (no destructuring, CVA, Kobalte, refs in `onMount`, `onCleanup` for listeners). | Foxic-native. |
| [website/add-feature-module](website/add-feature-module/SKILL.md) | Scaffolds a feature folder under `src/features/`. | Foxic-native. |
| [website/add-i18n-string](website/add-i18n-string/SKILL.md) | Adds an i18n key + translation; RU plural cases (one/few/many/other); ICU brace escapes. | Foxic-native. |
| [website/typescript-discriminated-union](website/typescript-discriminated-union/SKILL.md) | Models API responses, ledger entries, modal states with discriminated unions; exhaustive `switch`. | [TS narrowing](https://www.typescriptlang.org/docs/handbook/2/narrowing.html#discriminated-unions). |
| [website/solid-resource-pattern](website/solid-resource-pattern/SKILL.md) | `createResource` discipline — loading/error/refetch, granular `.state`, no `createSignal+createEffect` for fetching. | [SolidJS createResource](https://docs.solidjs.com/reference/basic-reactivity/createresource). |
| [website/solid-async-cleanup](website/solid-async-cleanup/SKILL.md) | `onCleanup` for intervals, listeners, fetch aborts, Telegram WebApp `BackButton.onClick` and theme handlers. | [SolidJS onCleanup](https://docs.solidjs.com/reference/lifecycle/oncleanup). |

#### UI patterns

| Skill | Purpose | Source |
|---|---|---|
| [website/tailwind-styling](website/tailwind-styling/SKILL.md) | CVA vs inline utilities, `@theme` tokens, OKLCH, no `!important`, no arbitrary values. | [Tailwind v4](https://tailwindcss.com/docs/theme) + [cva.style](https://cva.style/docs). |
| [website/design-tokens-system](website/design-tokens-system/SKILL.md) | Govern OKLCH tokens — semantic naming, light+dark pairs, contrast validation, status colors for the ledger. | [Tailwind v4 @theme](https://tailwindcss.com/docs/theme) + [InclusiveColors](https://www.inclusivecolors.com/). |
| [website/typography-scale](website/typography-scale/SKILL.md) | Fixed type scale; `tabular-nums` for ledger columns; tracking rules; RU vs EN sizing test. | [impeccable.style](https://impeccable.style/) + [Tailwind font-size](https://tailwindcss.com/docs/font-size). |
| [website/interaction-states-kobalte](website/interaction-states-kobalte/SKILL.md) | Map Kobalte `data-*` attributes to Tailwind variants; precedence disabled > active > hover; `focus-visible` always. | [Kobalte docs](https://kobalte.dev/docs/core/overview/introduction). |
| [website/design-anti-patterns](website/design-anti-patterns/SKILL.md) | Anti-attractor catalogue: no purple gradients, no nested cards, no pure-black OLED bg, motion 150–250ms. | [impeccable.style](https://impeccable.style/) (Gallery of Shame). |
| [website/responsive-breakpoints-telegram](website/responsive-breakpoints-telegram/SKILL.md) | Mobile-first — Telegram WebApp viewport floor ~320px; container queries; no fixed widths above viewport. | [Tailwind responsive](https://tailwindcss.com/docs/responsive-design). |
| [website/ui-critique](website/ui-critique/SKILL.md) | UI review: Nielsen heuristics, cognitive load, P0–P3 severity; mandatory empty/loading/error coverage. | [impeccable.style](https://impeccable.style/) + [NN/g 10 heuristics](https://www.nngroup.com/articles/ten-usability-heuristics/). |
| [website/ui-accessibility](website/ui-accessibility/SKILL.md) | Semantic HTML, ARIA, keyboard flows, focus-visible, WCAG AA contrast, 44×44 touch targets, skip link. | [WCAG 2.2](https://www.w3.org/TR/WCAG22/) + [Kobalte a11y](https://kobalte.dev/docs/core/overview/introduction) + [WAI-ARIA APG](https://www.w3.org/WAI/ARIA/apg/patterns/). |
| [website/form-error-ux](website/form-error-ux/SKILL.md) | Inline validation — `aria-invalid` + `aria-describedby`, focus first invalid, Kobalte `Form.Field`, server error mapping. | [Smashing accessible forms](https://www.smashingmagazine.com/2023/02/guide-accessible-form-validation/) + [WebAIM](https://webaim.org/techniques/formvalidation/). |
| [website/loading-empty-error-states](website/loading-empty-error-states/SKILL.md) | Five mandatory states: loading (skeleton), empty (CTA), error (retry), partial/stale, success. | [UX Writing Hub](https://uxwritinghub.com/empty-state-examples/). |
| [website/seo-meta](website/seo-meta/SKILL.md) | Title, OG, Twitter, JSON-LD, sitemap, robots — for the public report pages. | [Google Search Central](https://developers.google.com/search/docs). |

#### Vixen-specific

| Skill | Purpose | Source |
|---|---|---|
| [website/telegram-login-widget](website/telegram-login-widget/SKILL.md) | Embed Telegram Login Widget script (browser auth mode), capture callback, POST signed payload, JWT in memory. | [Telegram Login Widget](https://core.telegram.org/widgets/login). |

### Infra

| Skill | Purpose | Source |
|---|---|---|
| [docker-multi-stage](docker-multi-stage/SKILL.md) | Multi-stage Dockerfiles for Rust + bun: cache mounts, distroless/alpine runtime, non-root, healthcheck, SQLX_OFFLINE. | Foxic + Docker docs. |
| [infra/github-actions-workflow](infra/github-actions-workflow/SKILL.md) | Writes `.github/workflows/*.yml` — server-ci, website-ci, build-server, build-website. Concurrency, caching, secrets. | [GH Actions docs](https://docs.github.com/en/actions). |

## Companion documents

- [CLAUDE.md](../../CLAUDE.md) — project-wide engineering instructions. The "Before / After Writing Code" sections incorporate guidance from [multica-ai/andrej-karpathy-skills](https://github.com/multica-ai/andrej-karpathy-skills).
- [AGENTS.md](../../AGENTS.md) — domain map for any LLM agent working in this repo.
- [server/CLAUDE.md](../../server/CLAUDE.md), [website/CLAUDE.md](../../website/CLAUDE.md) — per-component rule pyramids.
