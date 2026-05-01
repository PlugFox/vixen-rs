---
name: bump-version
description: Bump the patch version in server/Cargo.toml or website/package.json after completing a major change. Use when the user asks to bump/release a version, or when CLAUDE.md's rule about version bumps after major changes applies.
---

# Bump Version (Vixen)

The root `CLAUDE.md` requires bumping the patch version after completing a user-visible change.

## Which file

- **Server change** (anything under [server/](../../../server/)) → [server/Cargo.toml](../../../server/Cargo.toml), field `[package].version`.
- **Website change** (anything under [website/](../../../website/)) → [website/package.json](../../../website/package.json), field `"version"`.
- **Both** → bump both independently.

## Rules

- Use **semver patch** bump by default: `0.3.12` → `0.3.13`.
- Bump **minor** (`0.3.12` → `0.4.0`) only if the user says "minor" or the change is a user-visible feature.
- Bump **major** only on explicit request.
- After editing `Cargo.toml`, run `cargo check` (offline is fine: `SQLX_OFFLINE=true cargo check`) so `Cargo.lock` picks up the new version.
- After editing `package.json`, no regeneration needed (Bun doesn't pin the package itself in `bun.lock`).
- Do **not** tag or push — leave that to the user.
- Add a matching entry to [CHANGELOG.md](../../../CHANGELOG.md) under `[Unreleased]` with the `(server)` / `(website)` / `(infra)` tag.

## Procedure

1. Read the current version.
2. Edit it in place with `Edit` (preserve exact formatting — the TOML is `taplo`-formatted).
3. For Cargo, run `cd server && SQLX_OFFLINE=true cargo check` to refresh `Cargo.lock`.
4. Append the changelog entry (component-tagged).
5. Include the bump in the same commit as the feature change, or as a dedicated `chore: bump server to 0.3.13` commit — ask the user which they prefer if unclear.
