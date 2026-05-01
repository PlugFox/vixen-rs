---
name: add-api-route
description: Add a new Axum HTTP route to the vixen-rs server following project conventions for auth, validation, error mapping, and OpenAPI docs. Use when the user asks to add an endpoint, add a route, expose a new API, or extend an existing handler.
---

# Add API Route (Vixen server)

**Read first**:

- [server/docs/rules/api-routes.md](../../../../server/docs/rules/api-routes.md) — route layout, middleware ordering, request/response shape.
- [server/docs/rules/error-handling.md](../../../../server/docs/rules/error-handling.md) — `AppError` variants, `IntoResponse` mapping.
- [server/docs/api.md](../../../../server/docs/api.md) — the public API reference that MUST be updated.
- [server/docs/auth.md](../../../../server/docs/auth.md) — when adding routes behind WebApp-initData auth.

## File layout

Routes live in [server/src/api/routes_*.rs](../../../../server/src/api/). Add the handler to the matching file; create a new `routes_<area>.rs` only if the area is genuinely new. Wire it into the router in [server/src/api/server.rs](../../../../server/src/api/server.rs).

## Checklist

- **Method + path** follow REST conventions. Plural collections (`/chats`), singular by id (`/chats/{chat_id}`). Telegram IDs in paths are `i64` — declare them as `Path<i64>`.
- **Auth**: pick the right middleware stack:
  - `webapp_auth_middleware` for moderator dashboard endpoints (validates JWT minted from initData; checks `chat_ids` claim).
  - `admin_secret_middleware` for ops/admin endpoints (constant-time compare against `CONFIG_ADMIN_SECRET`).
  - No middleware for the public report endpoints (`/api/v1/report/{slug}`) — those go through `pub_rate_limit_middleware`.
- **Extractors** in this order: `State`, path, query, `Json` body. Typed structs with `serde::Deserialize` + validation.
- **Response type**: always a concrete `serde::Serialize` struct, never a raw `Value`. Use `Json<T>` or `(StatusCode, Json<T>)`.
- **Errors**: return `Result<Json<T>, AppError>`. Add new `AppError` variants in the errors module rather than ad-hoc `StatusCode` returns.
- **SQLx queries**: use `query!`/`query_as!` macros (compile-time checked). Refresh `.sqlx/` via `/db-migrate` or `cargo sqlx prepare`.
- **Tracing**: `#[tracing::instrument(skip(state, body))]` — skip large/sensitive args.
- **OpenAPI**: every route needs `#[utoipa::path(...)]` so the website's typed client stays in sync.

## After writing

1. Update [server/docs/api.md](../../../../server/docs/api.md) with the route, auth requirement, request shape, response shape, error codes.
2. Add an integration test under `server/tests/` that hits the route via `axum::Router::oneshot`.
3. Run `/server-check`.
4. If the website consumes the route, update `website/src/features/{area}/api.ts` and `types.ts` — then `/website-check`.

## Security pitfalls

- **Authorization ≠ authentication**: `webapp_auth_middleware` only proves the JWT is valid. For chat-scoped endpoints, also check that the requested `chat_id` is in the JWT's `chat_ids` claim. Server-side double-check is mandatory — the JWT is just a UI hint.
- **IDOR via `chat_id` in path**: never trust the path. Re-verify the moderator's access to that chat in every handler.
- **Rate limits**: public endpoints without `pub_rate_limit_middleware` are a DoS vector.
- **Bot token / initData in logs**: never log raw `Authorization` headers, never log the JWT body, never include `initData` in any tracing field above debug level.
- **Input size**: set explicit `RequestBodyLimitLayer` on any upload-style routes (avatar, custom captcha asset upload if/when added).
