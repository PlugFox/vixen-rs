---
description: Validate a Telegram WebApp initData payload (HMAC, expiry, decoded fields)
allowed-tools: Bash, Read
---

Debug a Telegram WebApp `initData` payload during development — useful when the website's `POST /api/v1/auth/telegram/login` is rejecting a session you believe should work.

Prerequisites:
- `TELEGRAM_BOT_TOKEN` is exported (used as the HMAC key, never printed).
- The example binary `tg-init-validate` exists in `server/examples/`. **Until M4 lands the auth pipeline, this command is documentation-only — see [server/docs/auth.md](../../server/docs/auth.md) for the algorithm.**

Steps (once the binary exists):

1. Take the raw initData query string as the first argument or from stdin. Example:
   ```
   /tg-init-debug "user=%7B...%7D&auth_date=1714560000&hash=abc123..."
   ```
2. Run `cd server && cargo run --example tg-init-validate -- "$INIT_DATA"`. The example performs:
   - Parse the URL-encoded `initData` into a sorted-by-key data-check-string.
   - Compute `secret_key = HMAC_SHA256("WebAppData", bot_token)`.
   - Compute `expected_hash = HMAC_SHA256(secret_key, data_check_string)`.
   - Constant-time compare against the `hash` field.
   - Check `auth_date` is within the configured window (default 24h).
3. Print: `hash valid: Y/N`, `auth_date age: <seconds>s`, decoded user fields (`id`, `username`, `first_name`, `language_code`, `is_premium`).

The example reads `TELEGRAM_BOT_TOKEN` from env directly — never accept the token as an argument and never log it.

Reference: [Telegram Mini Apps — Validating data received via the Mini App](https://core.telegram.org/bots/webapps#validating-data-received-via-the-mini-app).
