---
name: find-external-skill
description: Evaluate a 3rd-party Claude skill before importing — source trust, duplicate check, rule extraction, vixen-fit. Reject vague triggers and off-scope content. Use when the user shares a skill URL, asks "should we adopt this skill", or proposes importing from a marketplace.
---

# Find External Skill (Vixen)

**Source:** vixen meta + foxic discipline.

## The mandate

Imported skills must add a *rule*, not duplicate one. **Skill quality over quantity.** A repo with 50 vague skills is worse than 15 sharp ones.

## Source trust ranking

1. `anthropics/skills` — Anthropic-curated. High trust.
2. Established orgs — Vercel Labs, Tokio, large OSS projects with active maintainers. Medium-high trust.
3. Community marketplaces — `skills.sh`, `bencium-marketplace`, `accesslint-marketplace`. Medium trust; verify per-skill.
4. Random repos — only if recent (last 6 months), starred (>50), and the rule is genuinely useful. Treat as "inspiration to rewrite," not "import as-is."

## Duplicate check

Before adopting, grep:

```bash
rg -l '<concept>' .claude/skills server/docs/rules website/docs/rules
```

If 80%+ of the rule already exists somewhere, **skip the import**. Or merge a single bullet into the existing skill instead of creating a duplicate.

## Trigger quality (frontmatter `description`)

- **Reject** vague triggers: "use when needed", "for help", "anytime you're not sure".
- **Accept** concrete triggers: "When adding an Axum route", "When the user asks to write a commit message", "When editing migrations under server/migrations/".

The trigger must answer: "what user phrasing makes Claude pick this skill?"

## Body extract rule

Skills are rules + checklists, not essays. If a source skill is 2000 words of theory, rewrite down to:

- 5-10 bullet checklist.
- 1-3 short code examples (vixen-relevant).
- Gotchas / anti-patterns section.
- One canonical verification command.

Aim for ≤ 120 lines. Beyond that, you're writing a doc, not a skill.

## Vixen-fit check

- **Stack match**: Rust + Axum + sqlx + teloxide / SolidJS + Kobalte + Tailwind + bun. Reject skills written for Next.js, Express, Django, Vue, React.
- **Scale match**: single-tenant, single-process, Docker Compose. Reject Kubernetes, Kafka, multi-region patterns.
- **Hard rule compatibility**: honors vixen invariants — Telegram IDs are `i64`, captcha assets immutable, redaction discipline (`RedactedToken`), idempotent moderation actions, transactional per-chat config writes. If the skill teaches a pattern that violates these, reject or rewrite.

## Adapt, don't copy

- Replace generic examples with vixen-specific ones (`chat_config`, `moderation_actions`, `captcha_challenges`).
- Strip framework-specific phrasing ("in your Next.js app...").
- Cite the source URL on the body's `**Source:**` line — required for attribution and re-checking.

## Where to file

- Server-specific → `.claude/skills/server/<name>/SKILL.md`.
- Website-specific → `.claude/skills/website/<name>/SKILL.md`.
- Generic meta / cross-cutting → `.claude/skills/<name>/SKILL.md`.
- Infra (CI, Docker, deploy) → `.claude/skills/infra/<name>/SKILL.md`.

## After importing

- Add a row to [`.claude/skills/README.md`](../../skills/README.md) — name, trigger, area.
- Cite source URL on the body's `**Source:**` line.
- Verify against [`CLAUDE.md`](../../CLAUDE.md) Critical Rules — re-read, don't assume.
- Run a sample: ask "would this skill activate on the right phrase?" If unclear, sharpen the description.

## Reject signals

- Source skill mentions tools we don't have (Storybook, Cypress, Playwright in v1, GraphQL, tRPC).
- Source skill is marketing for a SaaS — "use our hosted X for...".
- Source skill is auto-generated boilerplate without specific rules ("write good code", "follow best practices").
- Description doesn't name a concrete trigger phrase or file path.
- Body is essay-style with no checklist.

## Related

- `plan-before-code` — adoption is itself a non-trivial change; plan first.
- `import-skill` (sister meta-skill, when added) — the mechanical "do the import" steps.
