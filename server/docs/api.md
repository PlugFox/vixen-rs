# API Reference

REST API for the dashboard, the public report, and ops-only endpoints.

Base path: `/api/v1` (versioned).

OpenAPI / Scalar UI is mounted at `/scalar` in dev. Production exposes `/api/v1/openapi.json` for the dashboard's typed client. The full enumeration of every endpoint lives in the OpenAPI doc — this file describes the **groups, conventions, and auth requirements**, not every route signature.

## Implementation status

| Endpoint                                           | Status        |
|----------------------------------------------------|---------------|
| `GET /health`, `GET /about`                        | M0 — shipped  |
| `POST /api/v1/auth/telegram/login`                 | M4 — shipped  |
| `GET /api/v1/auth/me`                              | M4 — shipped  |
| `POST /api/v1/auth/logout`                         | Pending (M5)  |
| `GET /api/v1/chats`                                | M4 — shipped  |
| `GET /api/v1/chats/{chat_id}/config`               | M4 — shipped  |
| `PATCH /api/v1/chats/{chat_id}/config`             | M4 — shipped  |
| `GET /api/v1/chats/{chat_id}` (detail)             | Pending (M5)  |
| `GET /api/v1/chats/{chat_id}/moderators`           | Pending (M5)  |
| `/api/v1/chats/{chat_id}/moderation/*`             | Pending (M5)  |
| `/api/v1/chats/{chat_id}/reports/*`                | Pending (M5)  |
| `/report/{chat_slug}` (public)                     | Pending (M6)  |
| `POST /admin/ping`                                 | M4 — shipped  |
| `GET /admin/db/health`, `/admin/jobs/*` (ops)      | Pending       |

## Conventions

### Success envelope

```json
{
  "status": "ok",
  "data": { ... }
}
```

### Error envelope

```json
{
  "status": "error",
  "error": {
    "code": "MODERATOR_REQUIRED",
    "message": "not a moderator of this chat"
  }
}
```

### Status codes

- `200` — success (GET, POST, PATCH).
- `204` — DELETE success (no body).
- `4xx` — client errors with a JSON error envelope.
- `5xx` — server errors with a JSON error envelope.

### Pagination

Cursor-based: `?cursor=<opaque>&limit=50` → response includes `{items, has_more, cursor}`.

Cursors encode `(created_at, id)` (or equivalent) as base64 JSON. Decoded server-side; clients treat as opaque.

### Telegram IDs

`chat_id` and `user_id` in paths and bodies are `i64`. The dashboard's TS uses `number` (safe up to 2^53, fits Telegram IDs comfortably) but exact-bit ops should use `bigint`.

## Endpoint groups

### Auth (`POST /auth/telegram/login`, `GET /auth/me`, `POST /auth/logout`)

Authentication is **Telegram-only**. See [auth.md](auth.md) for the algorithm.

- `POST /auth/telegram/login` — body is `{"init_data": "<raw URL-encoded string>"}`. Server validates HMAC (WebApp or Login Widget shape), mints a 1h HS256 JWT, returns `{token, expires_in, user, chat_ids}`. 401 on HMAC failure or `auth_date` older than `CONFIG_INIT_DATA_MAX_AGE_SECS` (24h default).
- `GET /auth/me` — returns the JWT claims (decoded server-side): `{user_id, username, first_name, last_name, chat_ids}`. Used by the dashboard on app boot.
- `POST /auth/logout` — client-side only (drop the JWT from memory). The endpoint will exist for symmetry and future revocation list support; not implemented in M4.

### Chats (`/chats/*`)

`webapp_auth_middleware`. JWT's `chat_ids` claim must contain the requested `chat_id`.

- `GET /chats` — list chats the moderator can manage. Returns the intersection of the JWT's `chat_ids` claim with the rows currently in `chats`. Metadata (title / type / member count) is best-effort from `chat_info_cache` — NULL when the bot hasn't yet seen the chat.
- `GET /chats/{chat_id}` — chat detail (title, type, members count, settings summary). _Pending M5._
- `GET /chats/{chat_id}/config` — full per-chat config DTO (captcha, spam, CAS, report, summary, language, …). `openai_api_key` is **never** echoed; instead the DTO carries `openai_api_key_set: bool`.
- `PATCH /chats/{chat_id}/config` — partial update. Absent field = leave alone; `null` = clear (only `openai_api_key` is nullable; `null` on a NOT NULL column is a 400). The write runs inside `SELECT … FOR UPDATE` + `UPDATE` and publishes `invalidate` on Redis `chat_config:{chat_id}` so every replica's Moka cache evicts the entry — bot picks up the new value within ~1s without restart.
- `GET /chats/{chat_id}/moderators` — list of `chat_moderators`. _Pending M5._

### Moderation (`/chats/{chat_id}/moderation/*`)

`webapp_auth_middleware`. See [moderation.md](moderation.md) for the action ledger semantics.

- `GET /chats/{chat_id}/moderation/actions?cursor=...&limit=50&action=ban&actor_kind=moderator` — paginated action ledger with filters.
- `POST /chats/{chat_id}/moderation/ban` — `{user_id, reason}`.
- `POST /chats/{chat_id}/moderation/unban` — `{user_id}`.
- `POST /chats/{chat_id}/moderation/verify` — `{user_id}`.
- `POST /chats/{chat_id}/moderation/unverify` — `{user_id}` — rare, requires explicit confirmation client-side.
- `GET /chats/{chat_id}/moderation/verified?cursor=...` — list verified users.

### Reports (auth) (`/chats/{chat_id}/reports/*`)

`webapp_auth_middleware`. See [reports.md](reports.md).

- `GET /chats/{chat_id}/reports/today` — current-day aggregates + chart URL.
- `GET /chats/{chat_id}/reports/{date}` — historical day.
- `POST /chats/{chat_id}/reports/regenerate` — re-run today's report (delete + re-post in chat).

### Public (`/report/*`, `/sitemap.xml`)

No auth. `pub_rate_limit_middleware` (~60 req/min per IP).

- `GET /report/{chat_slug}` — redacted aggregates (no usernames, no message bodies).
- `GET /report/{chat_slug}/chart.png` — the daily chart as PNG. Cached `max-age=3600`.
- `GET /sitemap.xml` — lists every public-report slug. Cached 24h.

### Admin (`/admin/*`)

`admin_secret_middleware`. `X-Admin-Secret: <CONFIG_ADMIN_SECRET>` compared in constant time against `SHA-256` digests of both sides — length isn't leaked through early-exit timing. **Never reachable from the dashboard.** Used by ops scripts.

When `CONFIG_ADMIN_SECRET` is unset the middleware refuses every request with 503; config validation enforces presence in prod.

- `POST /admin/ping` — smoke test: returns `{status: "ok", received_at: <iso>}`. Confirms the middleware chain is wired and the secret matches.
- `GET /admin/db/health` — Postgres health. _Pending._
- `GET /admin/jobs/status` — last-run timestamp per job. _Pending._
- `POST /admin/jobs/{job_name}/run` — trigger a job out of band (e.g. force daily report). _Pending._
- `GET /admin/sqlx/cache/check` — verifies `.sqlx/` matches live queries (CI-friendly). _Pending._

### Health / About (`/health`, `/about`)

No auth.

- `GET /health` — `{"status":"ok"|"degraded","checks":{"db":"ok"|"down","redis":"ok"|"down"}}`. Returns 200 if every check is `ok`, 503 otherwise.
- `GET /about` — `{name, version, commit_hash, built_at, rust_version, profile, target}`. No secrets.

### OpenAPI (`/scalar`, `/api/v1/openapi.json`)

- `/scalar` — interactive Scalar UI (dev only by default; gated by `CONFIG_OPENAPI_UI` in prod).
- `/api/v1/openapi.json` — the spec, used by the dashboard's typed client generator.

## Validation

Input is validated in the service layer (or extractor) — never trust path / query / body raw:

- `chat_id`: must be in JWT's `chat_ids`.
- `user_id`: positive `i64`.
- Free text: max length, no nulls, no control chars.
- Slug (`chat_slug`): lowercase, `[a-z0-9-]{3,64}`, regex-validated.

## Rate limiting

- Public endpoints: 60 req/min per IP (Tower's `governor` middleware).
- Admin endpoints: 10 req/sec (low; ops only).
- Authenticated dashboard: no per-user rate limit in v1 (one moderator can't realistically DoS themselves; revisit if abuse appears).

## CORS

`CONFIG_CORS_ORIGINS` is a comma-separated list of allowed origins. Defaults to dashboard's URL in prod, `http://localhost:3000` in dev. Wildcards (`*`) are forbidden — explicit origins only.

## Versioning

`/api/v1/*` is the only version today. Breaking changes either add a v2 mount alongside or — preferred — extend the existing surface in a back-compat way (new optional fields, new endpoints, never re-typing existing fields).

## Related

- Auth: [auth.md](auth.md)
- Schema: [database.md](database.md)
- Rules: [rules/api-routes.md](rules/api-routes.md)
- Skill: `.claude/skills/server/add-api-route/SKILL.md`
