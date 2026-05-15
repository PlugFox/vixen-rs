# Setup Guide — Telegram bot, Login Widget, dashboard env

End-to-end walkthrough from "I have an empty repo" to "the dashboard renders
the chats list for a real moderator". Three actors talk to each other:

```
Telegram (BotFather, Bot API)
    │
    ├── /api/v1/auth/telegram/login   ← server validates initData
    │
website (SolidJS SPA)  ────  server (Rust, Axum, teloxide)
    │                              │
    └── runs at :3000 in dev       └── runs at :8000 in dev
```

Everything below is for a **dev** workflow. For production deploys see
[`../../docs/deployment.md`](../../docs/deployment.md) (lands in M8).

## 1. Create the Telegram bot

In Telegram, message [@BotFather](https://t.me/BotFather):

| Command | What you set | Result |
|---|---|---|
| `/newbot` | Display name, then `@username` | Bot token. Save it — `CONFIG_BOT_TOKEN`. |
| `/setprivacy` → choose bot → **Disable** | Privacy mode | Bot can read every message in groups. Required for the captcha + spam pipeline. |
| `/setdescription`, `/setabouttext`, `/setuserpic` | Cosmetics | Optional. |
| `/setcommands` | Command menu | Paste the table below for autocompletion in clients. |

Suggested `/setcommands` payload (one per line, `command - description`):

```
help - show help
status - show bot status in this chat
verify - manually verify a replied user (moderator)
ban - ban a replied user with optional reason (moderator)
unban - lift a ban by user_id (moderator)
stats - last 24h activity summary (moderator)
report - post the daily report now (moderator)
summary - AI summary of recent chat (moderator)
info - show user's moderation history (moderator)
```

## 2. Register the Login Widget domain

Telegram refuses to render the Login Widget on any domain not whitelisted by
BotFather:

| Command | Value | Result |
|---|---|---|
| `/setdomain` → choose bot → enter domain (no protocol, no path) | e.g. `dashboard.example.com` | Widget renders on `https://dashboard.example.com/*`. |

**Localhost does NOT work** — the widget script requires HTTPS plus a
registered domain. For dev you either:

- **Skip the widget locally** and test from a Telegram WebApp button
  instead (covered in §4 — only HTTPS required).
- **Tunnel** via [`ngrok`](https://ngrok.com/) /
  [`cloudflared`](https://developers.cloudflare.com/cloudflare-one/connections/connect-networks/)
  / [`localtunnel`](https://github.com/localtunnel/localtunnel):

  ```bash
  # ngrok
  ngrok http 3000
  # → https://random-hash.ngrok-free.app

  # cloudflared
  cloudflared tunnel --url http://localhost:3000
  # → https://random-words.trycloudflare.com
  ```

  Set the tunnel host as the Login Widget domain via `/setdomain` and
  open the dashboard at that HTTPS URL. Update `CONFIG_CORS_ORIGINS` to
  include it.

## 3. (Optional) Configure the WebApp menu button

This is what makes "Open dashboard" appear inside Telegram. The bot menu
button can point at any HTTPS URL — Telegram opens it in a WebView and
exposes `Telegram.WebApp.initData` to the page automatically.

| Command | Value | Result |
|---|---|---|
| `/setmenubutton` → choose bot → enter URL → button text | e.g. `https://dashboard.example.com/`, "Dashboard" | "Dashboard" appears in the bot's chat menu. |

Or, for an "inline" WebApp accessible by an `/start` deep-link, use
`/newapp` instead — Telegram walks you through registering it.

## 4. Add the bot to a chat and grant admin rights

1. Add the bot to the target chat as a regular member.
2. Promote to admin with at minimum:
   - **Delete messages** — to act on spam.
   - **Ban users** — for `/ban`, captcha-fail kicks, automatic spam bans.
   - **Restrict members** — for captcha-pending users.
   - Pin messages / Add new admins — optional, only if you want the bot to
     pin reports.
3. Grab the `chat_id`:
   - Easiest path: forward any message from the chat to
     [@userinfobot](https://t.me/userinfobot) — it prints both your user
     ID and the original chat ID.
   - Or watch the server log on startup: `tracing` prints every update with
     `chat_id` once polling starts.

   Supergroup IDs start with `-100…` and require an `i64` everywhere
   (CLAUDE.md hard rule).

## 5. Register yourself as a moderator

The dashboard's `chat_ids` claim is sourced from `chat_moderators`. Until
your `user_id` lands there, `/auth/telegram/login` returns an empty
`chat_ids` and the dashboard shows "you are not a moderator of any watched
chat".

For the first bootstrap, insert directly via SQL:

```sql
-- run against the dev database, e.g. via the MCP postgres tool or psql
INSERT INTO chats (chat_id) VALUES (-1001234567890)
ON CONFLICT DO NOTHING;
INSERT INTO chat_config (chat_id) VALUES (-1001234567890)
ON CONFLICT DO NOTHING;
INSERT INTO chat_moderators (chat_id, user_id, granted_by)
VALUES (-1001234567890, 4242, 4242)
ON CONFLICT DO NOTHING;
```

The server also auto-treats Telegram chat admins as moderators for slash
commands (via `ChatMemberAdministrator`), but the dashboard's `chat_ids`
claim derives ONLY from `chat_moderators` — every moderator who needs to
log in must have an explicit row.

## 6. Fill in server env

Copy and edit:

```bash
cp server/config/template.env server/.env
$EDITOR server/.env
```

Minimum required for the dashboard to be usable:

```bash
CONFIG_BOT_TOKEN=1234567890:AAEzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz
CONFIG_DATABASE_URL=postgresql://vixen:vixen_dev_password@localhost:5432/vixen
CONFIG_REDIS_URL=redis://localhost:6379
CONFIG_CHATS=-1001234567890
CONFIG_JWT_SECRET=$(openssl rand -hex 32)
CONFIG_ADMIN_SECRET=$(openssl rand -hex 32)
CONFIG_CORS_ORIGINS=http://localhost:3000
# If you tunnel via ngrok / cloudflared:
# CONFIG_CORS_ORIGINS=http://localhost:3000,https://random-hash.ngrok-free.app
```

Validation rules (`config::Config::validate`):

- `CONFIG_BOT_TOKEN` must match `^\d+:[\w-]{30,}$`.
- `CONFIG_CHATS` must list ≥1 ID.
- `CONFIG_CORS_ORIGINS` forbids wildcards.
- `CONFIG_JWT_SECRET` ≥ 32 bytes (and non-empty in any environment).
- `CONFIG_ADMIN_SECRET` non-empty in any environment (even dev).

## 7. Fill in website env

```bash
cp website/config/template.env website/.env
$EDITOR website/.env
```

```bash
VITE_BOT_USERNAME=vixen_test_bot
# Leave VITE_API_URL unset to use the Vite proxy in dev.
```

`VITE_BOT_USERNAME` must be the EXACT `@username` from step 1, **without**
the `@`. See [`env.md`](./env.md) for the full variable reference.

## 8. Boot the stack

```bash
# Infrastructure (Postgres + Redis).
docker compose -f docker/docker-compose.yml up -d

# Server (HTTP API on :8000 + bot poller).
cd server && cargo run

# Website (Vite dev server on :3000, proxies /api/* to :8000).
cd website && bun install
bun run i18n:gen    # regenerate src/shared/i18n/generated/ — gitignored
bun run dev
```

## 9. Sign in

### Browser mode (Login Widget)

1. Open `http://localhost:3000` (if you tunnel for HTTPS, use the tunnel URL
   that you registered via `/setdomain` in step 2).
2. The Login Widget renders. Click "Log in via Telegram" — Telegram shows a
   confirmation popup.
3. On success the widget invokes `window.onTelegramAuth(user)`. The
   dashboard composes a Login-Widget-shaped `initData` payload, submits it
   to `/api/v1/auth/telegram/login`, and renders the chats list.

### WebApp mode (inside Telegram)

1. Open the bot in Telegram.
2. Tap the menu button you configured in step 3 (or the `/start` deep-link
   if you registered an inline app).
3. Telegram opens the dashboard in a WebView with
   `Telegram.WebApp.initData` already set. The dashboard auto-submits it on
   mount — no widget click needed.

Inside WebApp mode the header is hidden, the Telegram BackButton drives the
router, and the theme tracks `Telegram.WebApp.colorScheme` automatically.

## 10. Troubleshooting

| Symptom | Likely cause |
|---|---|
| Login Widget doesn't render | `VITE_BOT_USERNAME` empty or wrong; domain not whitelisted via `/setdomain`. |
| `INVALID_INIT_DATA` on every login | `CONFIG_BOT_TOKEN` mismatch with the bot whose username is `VITE_BOT_USERNAME`. |
| `AUTH_DATE_EXPIRED` immediately | Server clock drift. Telegram requires `auth_date` within 24h (configurable via `CONFIG_INIT_DATA_MAX_AGE_SECS`). |
| Dashboard says "you are not a moderator" | No `chat_moderators` row for your user_id. See step 5. |
| CORS errors in the browser console | Your origin (or tunnel hostname) is missing from `CONFIG_CORS_ORIGINS`. |
| Bot doesn't see messages in a group | Privacy mode is on. Re-run `/setprivacy` → Disable. After toggling, **kick and re-add the bot** to the chat — the privacy flag only refreshes on join. |
| Captcha never fires for joins | Bot is not admin, or `chat_config.captcha_enabled = FALSE`. Check via `/scalar` at `http://localhost:8000/scalar`. |

## Related

- [`env.md`](./env.md) — full env reference.
- [`auth.md`](./auth.md) — auth flow internals (WebApp vs Login Widget HMAC).
- [`architecture.md`](./architecture.md) — directory layout, routing, state.
- Server config: [`../../server/config/template.env`](../../server/config/template.env), [`../../server/docs/config.md`](../../server/docs/config.md).
