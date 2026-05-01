---
name: plan-before-code
description: Before non-trivial code — challenge approach, ask unknowns, propose alternatives, list edge cases, get explicit sign-off. Skip only for trivial fixes (typos, renames). Use when the user asks for a feature, refactor, schema change, or anything touching public surface.
---

# Plan Before Code (Vixen)

**Source:** [`CLAUDE.md`](../../CLAUDE.md) "Before Writing Code" + [multica-ai/andrej-karpathy-skills](https://github.com/multica-ai/andrej-karpathy-skills).

**Read first:** [`CLAUDE.md`](../../CLAUDE.md) "Before Writing Code" section.

## When to skip

Trivial fixes — just do it:

- Typos, doc-comment edits, one-line value changes.
- Simple renames (with the IDE / `cargo fix` / `replace_all`).
- Reformatting that `cargo fmt` / `bun run check` produces.

## When mandatory

Anything else — and especially:

- New feature, new module, new route, new handler, new job.
- Schema change, migration, new column, new index.
- Refactor that touches more than one file.
- Public API touch (REST, JWT shape, Telegram callback payload).
- Security-sensitive code (auth, redaction, captcha, permissions).
- Anything that could break a running deploy.

## Five steps before any code

1. **Challenge the approach.** Bluntly. Point out flaws, missed edges, security gaps, concurrency holes. "This races on N parallel webhook deliveries" is more useful than "looks good."
2. **Ask about unknowns.** If anything is ambiguous, ask. Do not guess. "Should this delete prior `moderation_actions` rows or append?" is a real question — pick wrong and you ship a bug.
3. **Propose alternatives.** If a simpler / more robust path exists, name it with one-sentence reasoning. "We can avoid the new column by reusing `kind` + an enum variant — same info, no migration."
4. **List edge cases.** Concurrent access, empty inputs, large payloads, permission gaps, migration rollbacks, Telegram API outages, CAS API timeouts, captcha asset upgrades, clock skew on `auth_date`.
5. **Wait for sign-off.** Do not write code until the user explicitly approves. "Ok" / "yes" / "go" — that's the gate.

## Vixen-specific edge-case checklist

Run through these for any non-trivial change:

- Telegram IDs are `i64` everywhere? Negative supergroup IDs (`-100…`) handled?
- Idempotency: re-processing the same message (xxhash dup, retry, restart) safe? Check `moderation_actions` ledger before acting.
- Per-chat config write: wrapped in a transaction with `SELECT ... FOR UPDATE` on the chat row?
- New log call: any path that could surface `bot_token`, `initData`, phone, full name, message body? Use `RedactedToken`.
- Captcha asset: are you overwriting an existing file in `server/assets/captcha/` (forbidden) or adding a new versioned file?
- Migration: `.down.sql` present? `BIGINT NOT NULL` for TG IDs? Index on FK columns?
- WebApp auth: `auth_date` < 24h check? HMAC algorithm matches the surface (WebApp vs Login Widget)?
- i18n: every locale gets the new key, not just `en`?

## Anti-patterns

- Diving into code as the first response to "add X."
- Assuming the user wants the most-comprehensive solution. Ask about scope.
- Hiding objections behind hedged language ("you might want to consider..."). Say it.
- Listing 20 edge cases as a stalling tactic. Pick the 3-5 that actually bite.

## Output template

> **Plan.**
> What I'm doing: ...
> What I'm assuming: ...
> Open questions: ...
> Edge cases: ...
> Alternatives considered: ...
> Sign off?

Keep it ≤ 15 lines. Long plans are skipped.

## Related

- `verifiable-goal` — convert the agreed plan to a check that fails before, passes after.
- `change-impact-assessment` — list affected systems before coding.
- `code-review-self` — read your own diff before pushing.
