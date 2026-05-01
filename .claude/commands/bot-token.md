---
description: Verify the Telegram bot token by calling getMe
allowed-tools: Bash
---

Sanity-check the configured Telegram bot token via the Bot API `getMe` endpoint, without ever logging the token itself.

Prerequisites: `TELEGRAM_BOT_TOKEN` is exported in the current shell (e.g. via `direnv` or `source .env.local`). Do **not** echo, print, or otherwise surface the token value.

Steps:

1. Verify the var is set without printing it: `[ -n "${TELEGRAM_BOT_TOKEN:-}" ] || { echo "TELEGRAM_BOT_TOKEN not set"; exit 1; }`
2. Pipe the response straight through `jq`, never assigning the URL to a variable that could land in shell history:
   ```bash
   curl -fsS "https://api.telegram.org/bot${TELEGRAM_BOT_TOKEN}/getMe" | jq '.result | {username, id, can_read_all_group_messages, is_premium}'
   ```
3. Print the `result` fields in a 4-line table.

Error mapping (do **not** include the token in any error output):

- HTTP 401 → "token invalid or revoked"
- HTTP 404 → "bot does not exist (token typo?)"
- network error → "no connectivity to api.telegram.org"

If anything goes wrong, show only the HTTP status code and a one-line diagnosis.
