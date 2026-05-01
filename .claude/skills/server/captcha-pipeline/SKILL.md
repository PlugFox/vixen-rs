---
name: captcha-pipeline
description: Add or modify a captcha mode (digits, picture-pick, etc.) — atomic image render + DB row + sendPhoto + restrictChatMember; deterministic per challenge_id.
---

# Captcha Pipeline (Vixen server)

**Read first:**

- [server/docs/captcha.md](../../../../server/docs/captcha.md) — modes, asset layout, lifecycle.
- [server/docs/rules/migrations.md](../../../../server/docs/rules/migrations.md) — schema changes.

## Pipeline

1. Generate solution with seeded RNG: `seed = xxh3_64(challenge_id)`. Same `challenge_id` → same solution → same image.
2. Render PNG via `image` + `ab_glyph` from assets in `server/assets/captcha/`. Run inside `tokio::task::spawn_blocking` — CPU-bound.
3. `INSERT INTO captcha_challenges (chat_id, user_id, challenge_id, solution, asset_version, expires_at, attempts_left)`.
4. `bot.send_photo(...)` with inline keyboard, then `bot.restrict_chat_member(...)` with `until_date = now + ttl`.
5. On solve callback (transaction): `SELECT ... FOR UPDATE` the challenge → verify → `DELETE` row → `INSERT verified_users` → lift restriction via `bot.restrict_chat_member` with full perms.

## Determinism

Render twice with the same `challenge_id` and assert byte equality. Required for the deterministic-render test suite.

## Files touched

- `server/src/services/captcha_service.rs` — solution gen + render.
- `server/src/telegram/handlers/captcha.rs` — issue + callback solve.
- `server/tests/captcha_determinism.rs` — byte-equality test.
- `server/docs/captcha.md` — mode table.

## Gotchas

- **Captcha assets in `server/assets/captcha/` are immutable.** Never overwrite an existing TTF/PNG. New look = new file with version suffix (`DejaVuSans-v2.ttf`) + bumped `asset_version` selector.
- **Pending challenges reference asset paths verbatim.** Overwriting a font breaks any in-flight user trying to re-render.
- **`until_date` for `restrict_chat_member` must be ≥ 30s in the future** (or `0` for permanent in some contexts) — Telegram silently treats values too close to "now" as permanent.
- **`attempts_left` decrement uses `SELECT ... FOR UPDATE`.** Without the row lock, two concurrent CallbackQueries from a tap-spam can both see `attempts_left = 1` and both decrement.
- **Expiry job uses `kick`, not `ban`.** Kicked users can rejoin and try again; banned users can't, which is the wrong UX for a failed captcha.
- **CPU work in `spawn_blocking`.** Don't render on the runtime's I/O threads.
- **All Telegram IDs are `i64`** in the schema (`chat_id BIGINT`, `user_id BIGINT`).

## Verification

- `cargo test captcha`.
- `cargo sqlx prepare --check`.
- Manual: `/verify` in dev chat, solve, re-issue with same input → identical image bytes.

## Related

- `add-migration` — `captcha_challenges` schema additions.
- `add-telegram-handler` — CallbackQuery answer-within-30s rule.
- `transaction-discipline` — `SELECT ... FOR UPDATE` patterns.
