---
name: commit-message
description: Write a git commit message for vixen-rs following the project's Conventional Commits convention. Use when the user asks to commit, create a commit, or write a commit message.
---

# Commit Message (Vixen)

Follow [Conventional Commits](https://www.conventionalcommits.org/) with the scopes used in this repo.

## Format

```
<type>(<scope>): <subject>

<body — optional, wraps at 72 chars>
```

## Types

`feat`, `fix`, `refactor`, `docs`, `chore`, `test`, `perf`, `style`, `build`, `ci`.

## Scopes (use when obvious)

- Server: `server`, `api`, `db`, `auth`, `bot`, `captcha`, `spam`, `jobs`, `reports`, `config`, `moderation`.
- Website: `website`, `ui`, `auth`, `chats`, `moderation`, `reports`, `settings`, `i18n`.
- Infra: `docker`, `ci`, `deps`.

## Rules

- **Subject**: imperative mood, lowercase, no trailing period, ≤72 chars. Example: `feat(captcha): add digit-pad refresh button`.
- **Language**: English only in commits (even though we chat in Russian).
- **One logical change per commit**. If the diff spans unrelated areas, split it.
- **Body** (optional): explain the *why*, not the *what*. The diff shows the what.
- **Breaking changes**: add `!` after scope (`feat(api)!: ...`) and a `BREAKING CHANGE:` footer.

## Breaking changes

- Mark a breaking change with `!` after scope: `feat(api)!: rename /chats/list to /chats/index`.
- Add a `BREAKING CHANGE:` footer paragraph explaining the migration path.
- Bump major (or minor pre-1.0) — see `bump-version` skill.

## Before committing

1. Check `git status` / `git diff --staged` — make sure only relevant files are staged.
2. Never stage `.env*`, `.secrets/`, secrets, `node_modules/`, or `target/` artifacts.
3. Never commit logs that contain a bot token or a raw `initData` payload — `git diff --staged | grep -i 'bot[0-9]\+:'` and `grep -i 'auth_date='` should both be empty.
4. If the commit touches `server/` or `website/`, the user should have run the corresponding `/server-check` or `/website-check` first. If they haven't and the change is non-trivial, mention it before committing.
5. If the commit touches `server/migrations/` and `.sqlx/` is not staged, that's almost certainly a mistake — flag it.

## Co-author trailer

Only add `Co-Authored-By: Claude ...` when the user has explicitly asked you to make the commit. Do not add it for commits written by the user themselves.

- Add `Co-Authored-By: ...` ONLY when the user explicitly asks for the commit. Don't preempt — silent author injection is noise.
