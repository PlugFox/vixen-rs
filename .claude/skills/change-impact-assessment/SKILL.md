---
name: change-impact-assessment
description: Before a non-trivial change, list affected systems — DB schema, API contract, Telegram handlers, jobs, TS types, docs. Catch ripple effects upfront, not after CI fails. Use when planning a feature, migration, refactor, or any cross-module touch.
---

# Change Impact Assessment (Vixen)

**Source:** vixen monorepo + foxic patterns.

## The questions

Run through this list before writing code. Each "yes" is a checkbox you owe.

1. **Database**: schema change? Migration up + down? Backfill needed for existing rows?
2. **API contract**: route added / changed / removed? Backwards-compatible? OpenAPI (`#[utoipa::path]`) updated?
3. **Telegram handlers**: new update type? Slash command list change? Watched-chats filter affected?
4. **Background jobs**: new job? Schedule change? Idempotency invariant intact under restart / retry?
5. **TypeScript types**: API response shape change? Need to update `website/src/features/*/types.ts` and any downstream stores?
6. **i18n strings**: new user-visible text? **Every** locale needs the key, not just `en`.
7. **Docs**: which `server/docs/*.md` / `website/docs/*.md` is now stale? Update in the same PR.
8. **Skills**: is there a SKILL.md describing the now-changed area? Update it (or note that you did).
9. **Permissions / config**: new env var? Update `.env.example` + `server/docs/config.md` + Compose.
10. **Deploy order**: does DB migrate first, then app? Or reversible-only changes? Document.

## Greppable check

```bash
# All Rust files referencing the changed symbol:
rg -l '<Symbol>' --type rust
# All TS files:
rg -l '<symbol>' --type ts
# All docs that mention it:
rg '<symbol>' server/docs website/docs docs
```

If a symbol shows up in 5 files, you have 5 places to think about.

## Breaking change protocol

- **Deprecate first** if possible: ship a new endpoint v2, leave v1 returning the same shape with a `Sunset` header for one release. Then remove.
- **Unavoidable break**: changelog entry under `Removed` (or `Changed` with **Breaking** note) with migration guidance for the operator.
- **Version bump**: minor for additive, major for breaking, patch for non-user-visible. See [`bump-version/SKILL.md`](../bump-version/SKILL.md).

## Vixen-specific impact zones

- `chat_config.spam_weights` shape change → every spam rule referencing the struct + serde fixtures + admin UI form.
- JWT claim shape change → website auth store + API middleware + every route that reads `Auth` extractor.
- `moderation_actions` schema → admin dashboard ledger view + public report aggregates + idempotency uniqueness key.
- Captcha asset version bump → in-flight challenges (rendered with old asset path); existing pending rows reference asset paths verbatim. Don't overwrite.
- Telegram bot ID rotation → `RedactedToken` cache + WebApp HMAC key derivation.
- Locale added → every UI string key + every existing locale file in `website/i18n/messages/`.

## Output template

State this before writing code:

> **Impact.**
> DB: 1 migration (`add_chat_lang`).
> API: `PATCH /chats/{id}/config` — new field `lang`, additive, backwards-compatible.
> Handlers: none.
> Jobs: none.
> TS types: `ChatConfigDto.lang?: string`.
> i18n: 1 new key (`chat.config.lang.label`) — needs en + ru + uk.
> Docs: `server/docs/database.md`, `server/docs/api.md`.
> Deploy: migrate first, then app (additive, safe).

Keep it tight. If the list is empty for a row, say "none" — don't omit.

## Related

- `plan-before-code` — combine with this assessment for the planning step.
- `code-review-self` — verify each impact row was actually addressed in the diff.
- `pr-description` — the impact list maps directly to PR's "What" + "Risk" sections.
