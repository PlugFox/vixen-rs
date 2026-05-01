---
name: pr-description
description: Write the PR body — Why / What / Risk / Test plan. Skip pleasantries. Link issue. Tag CHANGELOG entry. Use 'gh pr create --body' with HEREDOC. Use when the user asks to open a PR, create a pull request, or write a PR description.
---

# PR Description (Vixen)

**Source:** foxic + standard PR practice.

## Title

Conventional Commits format. Under 70 chars. The body has details, not the title.

- `feat(captcha): add picture-pick mode`
- `fix(spam): close timing window in dedup cache`
- `refactor(api): extract auth middleware`
- `docs(server): document chat config transactional write`

Scopes: `server`, `api`, `db`, `auth`, `bot`, `captcha`, `spam`, `jobs`, `reports`, `config`, `website`, `ui`, `i18n`, `docker`, `ci`, `deps`.

## Body sections (in this order)

- **Why** (1-3 sentences): the problem this solves; link to issue if any (`Closes #42`).
- **What** (3-7 bullets): the changes, in user-facing language. Not "renamed `foo` to `bar`" — "rename callback type for clarity."
- **Risk**: what can break? Backwards-compat? Migration order? Performance impact? "None known" is acceptable when true.
- **Test plan**: what was verified locally; what reviewer should re-check. Concrete commands and steps, not "tested it."
- **CHANGELOG**: confirm `[Unreleased]` was updated with `(component)` tag, or note "internal-only, no CHANGELOG entry."

## Skip

- "This PR..." preamble. Start with content, not metadata.
- Emoji in titles unless explicitly requested.
- "Generated with Claude Code" footers in PR bodies (commits are fine; the PR description is for humans).
- Restating the diff line-by-line. Reviewers can read.

## gh CLI pattern

Use HEREDOC for body — preserves Markdown formatting and avoids shell quoting hell.

```bash
gh pr create --title "feat(captcha): add picture-pick mode" --body "$(cat <<'EOF'
## Why

Some users find digit captchas frustrating; picture-pick lowers friction for legit users while keeping the spam-bot solve rate similar. Closes #87.

## What

- New `ChallengeKind::PicturePick` variant + asset set under `server/assets/captcha/picture/v1/`.
- Solver UI in the callback handler (single `pick_*` action with index).
- `chat_config.captcha_kind` column; defaults to `digits`; opt-in per chat via `PATCH /chats/{id}/config`.
- Admin dashboard toggle.

## Risk

- In-flight digit challenges unaffected — `ChallengeKind` discriminates on solve.
- New assets are immutable — never overwriting existing files; v1 path is new.
- Migration is additive with default; safe to deploy app before app uses the column.

## Test plan

- `cargo test captcha::picture_pick` — green.
- `cargo sqlx prepare --check` — green.
- Manual TG bot test in dev chat — works on iOS, Android, web client.
- Admin toggle round-trips through API — verified in browser.

CHANGELOG: entry added under `[Unreleased]` `Added` `(server)` and `(website)`.
EOF
)"
```

## Linking issues

- `Closes #42` / `Fixes #42` in the body auto-closes on merge. Put it in **Why**, not the title.
- Cross-repo: `Closes plugfox/related-repo#42`.
- "Related to #N" if the PR touches but doesn't fully resolve another issue.

## After opening

- Return the PR URL to the user.
- Don't auto-merge. Let the user (or CI gates) decide.
- If review comments require non-trivial changes, run [`plan-before-code`](../plan-before-code/SKILL.md) again before pushing.

## Related

- `commit-message` — Conventional Commits scope reference.
- `verifiable-goal` — populates the Test Plan section.
- `change-impact-assessment` — populates the What / Risk sections.
