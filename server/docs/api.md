# API Reference

REST API for the dashboard, the public report, and ops-only endpoints.

Base path: `/api/v1` (versioned).

OpenAPI / Scalar UI is mounted at `/scalar` in dev. Production exposes `/api/v1/openapi.json` for the dashboard's typed client. The full enumeration of every endpoint lives in the OpenAPI doc — this file describes the **groups, conventions, and auth requirements**, not every route signature.

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

- `POST /auth/telegram/login` — body is the raw `initData` string. Server validates HMAC, mints a JWT, returns `{token, user, chat_ids}`.
- `GET /auth/me` — returns the JWT payload (decoded server-side). Used by the dashboard on app start.
- `POST /auth/logout` — client-side only (drop the JWT from memory). The endpoint exists for symmetry and future revocation list support.

### Chats (`/chats/*`)

`webapp_auth_middleware`. JWT's `chat_ids` claim must contain the requested `chat_id`.

- `GET /chats` — list chats the moderator can manage.
- `GET /chats/{chat_id}` — chat detail (title, type, members count, settings summary).
- `GET /chats/{chat_id}/config` — full per-chat config (spam threshold, captcha enabled, report hour, AI summary, weights, ...).
- `PATCH /chats/{chat_id}/config` — partial update; transactional.
- `GET /chats/{chat_id}/moderators` — list of `chat_moderators`.

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

`admin_secret_middleware`. `X-Admin-Secret: <CONFIG_ADMIN_SECRET>`. **Never reachable from the dashboard.** Used by ops scripts.

- `GET /admin/db/health` — Postgres health.
- `GET /admin/jobs/status` — last-run timestamp per job.
- `POST /admin/jobs/{job_name}/run` — trigger a job out of band (e.g. force daily report).
- `GET /admin/sqlx/cache/check` — verifies `.sqlx/` matches live queries (CI-friendly).

### Health / About (`/health`, `/about`)

No auth.

- `GET /health` — `{"status":"ok"}` if DB pool is alive.
- `GET /about` — `{name, version, commit_hash, started_at}`. No secrets.

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
