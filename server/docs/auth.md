# Authentication

Two parallel auth schemes:

1. **Telegram WebApp `initData`** — for the dashboard. Every moderator authenticates via their Telegram identity.
2. **Admin shared secret** — for ops scripts hitting `/admin/*`. Never used by the dashboard.

Google OAuth, username/password, and external SSO are **not** supported and are not on the roadmap.

## Telegram WebApp initData flow

### High level

1. The dashboard runs in two modes:
   - **WebApp container**: Telegram opens the dashboard via the bot's "Open dashboard" inline button. `Telegram.WebApp.initData` is available immediately as a signed query-string-shaped payload.
   - **Browser**: User opens the dashboard URL directly. The Telegram Login Widget renders, user signs in via Telegram, the widget posts a flat-fields payload to a callback URL. The website composes an `initData`-shaped string from those fields and submits it the same way.
2. Dashboard `POST /api/v1/auth/telegram/login` with the raw signed `initData` string in the body.
3. Server validates the HMAC, looks up `chat_moderators` for the verified `user_id`, mints a JWT (HS256, 1h expiry) with claims:
   ```
   sub: <telegram_user_id>  (i64)
   exp: <unix_seconds>
   chat_ids: [<i64>, <i64>, ...]   // chats this user moderates
   tg: { username, first_name, last_name }   // for UI display
   ```
4. Dashboard stores the JWT **in memory** (not localStorage — initData re-submission is cheap on refresh). All subsequent requests carry `Authorization: Bearer <jwt>`.
5. Server-side double-check on every chat-scoped endpoint: `if !ctx.chat_ids.contains(&path_chat_id) { return Forbidden; }`.

### initData HMAC algorithm (per Telegram spec)

[core.telegram.org/bots/webapps#validating-data-received-via-the-mini-app](https://core.telegram.org/bots/webapps#validating-data-received-via-the-mini-app):

1. Parse the URL-encoded `initData` into a sorted-by-key list of `key=value` pairs, **excluding** `hash`.
2. Join into the data-check-string:
   ```
   auth_date=1714560000\nquery_id=...\nuser={"id":...,"first_name":...}
   ```
3. Compute `secret_key = HMAC_SHA256(key="WebAppData", message=bot_token)`.
4. Compute `expected_hash = hex(HMAC_SHA256(key=secret_key, message=data_check_string))`.
5. Constant-time compare against the `hash` field from the original payload (`subtle::ConstantTimeEq`).
6. Reject if `auth_date` is older than 24 hours (`CONFIG_INIT_DATA_MAX_AGE_SECS`, default 86400).

The bot token is loaded once at startup into a `RedactedToken` — never echoed.

### Telegram Login Widget (browser mode)

Login Widget produces a slightly different shape than WebApp `initData`:

- Fields are flat (`id`, `first_name`, `last_name`, `username`, `photo_url`, `auth_date`).
- Hash is computed against the same kind of sorted data-check-string but with `secret_key = SHA256(bot_token)` (note: not HMAC; for backwards-compat with the original Login Widget protocol).

`auth_service.rs` handles both: it detects which shape the payload is and uses the matching algorithm. Both end up minting the same internal JWT.

### JWT mint

```rust
let claims = InternalClaims {
    sub: tg_user_id,
    exp: (Utc::now() + Duration::hours(1)).timestamp(),
    chat_ids: moderators::chats_for(tg_user_id, &pool).await?,
    tg: TgIdentity { username, first_name, last_name },
};
let token = jsonwebtoken::encode(&Header::new(Algorithm::HS256), &claims, &encoding_key)?;
```

`encoding_key` derives from `CONFIG_JWT_SECRET` (separate from the bot token — rotating the bot token does NOT invalidate JWTs unless we explicitly rotate `CONFIG_JWT_SECRET`).

JWTs are **not** stored server-side (no revocation list in v1). Logout = client drops the token from memory. To force-revoke all sessions, rotate `CONFIG_JWT_SECRET` and redeploy.

### Token lifetime + refresh

- 1h expiry (`CONFIG_JWT_TTL_SECS`).
- No refresh tokens. On 401, the dashboard re-submits initData (in WebApp mode, `Telegram.WebApp.initData` is always fresh; in browser mode, the user re-clicks the Login Widget).
- This is the same flow on cold start — there's nothing to migrate.

## Admin shared secret

`/admin/*` endpoints check:

```rust
let header = headers.get("X-Admin-Secret").and_then(|v| v.to_str().ok()).unwrap_or("");
if !subtle::ConstantTimeEq::ct_eq(header.as_bytes(), config.admin_secret.as_bytes()).into() {
    return Err(AppError::Unauthorized);
}
```

Constant-time compare prevents timing oracles. The secret is `CONFIG_ADMIN_SECRET` (env-only, never in git, never logged). Rotate by updating the env and redeploying — there are no persistent admin sessions.

## Threat model

| Threat | Mitigation |
|---|---|
| Bot token leak | Only used as HMAC key for `initData` and signs nothing else. Rotation via @BotFather; new token = new HMAC chain = old initData payloads invalid. JWTs are not directly affected because they sign with `CONFIG_JWT_SECRET`. |
| `CONFIG_JWT_SECRET` leak | All current JWTs become forgeable. Rotate the secret, all moderators re-login. |
| `CONFIG_ADMIN_SECRET` leak | Anyone can hit `/admin/*`. Rotate, redeploy. |
| Stolen `initData` payload | Valid for ≤ 24h (`auth_date` window). Replay attacks within that window are possible — the user needs to physically re-trigger Telegram login to get a fresh payload. Mitigated by the 1h JWT TTL: even if an attacker minted a JWT from a stolen initData, they can't refresh past 24h. |
| Session fixation | JWT in memory, not localStorage; not in cookies; `SameSite` not relevant. |
| CSRF | All state-changing endpoints require `Authorization: Bearer ...` — not a cookie — so CSRF is structurally impossible. |
| IDOR via `chat_id` in path | Server-side `chat_ids` whitelist check on every chat-scoped endpoint (see [rules/api-routes.md](rules/api-routes.md)). |
| Bot token in logs | `RedactedToken` newtype; deny-listed in `.claude/settings.json` so Claude can't accidentally print it. |
| `initData` in logs | Only at debug level; never at info+. |

## Rotating the bot token

1. Re-issue via @BotFather.
2. Update `CONFIG_BOT_TOKEN` in the deployment.
3. Redeploy.
4. All current moderator sessions stay valid (JWTs were signed with `CONFIG_JWT_SECRET`, not the bot token). The next initData submission against the new token works on first load — no migration needed.

To kill all sessions instead, rotate `CONFIG_JWT_SECRET` simultaneously.

## Related

- Service: `src/services/auth_service.rs`
- Routes: `src/api/routes_auth.rs` + `src/api/webapp_auth_middleware.rs`
- Test fixture: `mock_init_data(user_id, bot_token, ts)` in `tests/`
- Skills: `.claude/skills/server/tg-webapp-auth/SKILL.md` (M4) + `.claude/skills/website/telegram-login-widget/SKILL.md` (M4)
- Website-side flow: [`website/docs/auth.md`](../../website/docs/auth.md)
