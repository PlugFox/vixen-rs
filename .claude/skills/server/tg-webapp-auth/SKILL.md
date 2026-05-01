---
name: tg-webapp-auth
description: Validate Telegram WebApp initData (HMAC-SHA256 with key=HMAC_SHA256('WebAppData',bot_token)), mint JWT with chat_ids whitelist, reject auth_date > 24h.
---

# Telegram WebApp Auth (Vixen server)

**Read first:**

- [server/docs/auth.md](../../../../server/docs/auth.md) — initData flow, JWT claims.
- [Telegram WebApp validation spec](https://core.telegram.org/bots/webapps#validating-data-received-via-the-mini-app).

## Algorithm

1. URL-decode `initData`; parse into key-value pairs; remove `hash`.
2. Sort pairs lexicographically by key; join `key=value` with `\n` → data-check-string.
3. `secret_key = HMAC_SHA256(key="WebAppData", message=BOT_TOKEN)`.
4. `expected_hash = hex(HMAC_SHA256(key=secret_key, message=data_check_string))`.
5. Constant-time compare `expected_hash` vs `hash` (use `subtle::ConstantTimeEq`).
6. Reject if `auth_date < now_unix - 86400` (24h).
7. Parse `user` JSON (`user.id` is `i64`); look up `chat_moderators`; collect `chat_ids`.
8. Mint JWT (HS256, `CONFIG_JWT_SECRET`, **not** bot token) with `{sub, exp: now+1h, chat_ids, tg: {username, first_name}}`.

## Files

- `server/src/services/auth_service.rs` — validator + JWT mint.
- `server/src/api/routes_auth.rs` — POST `/auth/telegram/webapp`.

## Gotchas

- **HMAC secret is NOT the bot token.** It's `HMAC_SHA256(key="WebAppData", message=bot_token)`. Login Widget uses `SHA256(bot_token)` — different algorithm. Don't confuse the two.
- **User ID is `i64`.** Always.
- **Constant-time compare** prevents timing-oracle leaks of the expected hash.
- **JWT secret ≠ bot token.** Bot token rotation must not invalidate active sessions.
- **Clock skew tolerance.** A few seconds is fine; reject only when `auth_date > 24h` old, per spec.
- **Never log raw `initData` above debug level.** Carries `user.first_name`, `user.username`, possibly `user.phone_number` in some flows.
- **Use `RedactedToken`** from `src/utils/redact.rs` if you must log a token-shaped string for debugging.

## Verification

- `cargo test auth_service` — known-good and known-bad fixtures.
- `/tg-init-debug` slash command for ad-hoc validation in dev.

## Related

- `telegram-login-widget` — browser-mode counterpart (different secret).
- `add-api-route` — `routes_auth.rs` wiring.
