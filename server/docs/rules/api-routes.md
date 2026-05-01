# API Route Rules

Read this file before adding or modifying API endpoints.

## Adding a New Endpoint

### 1. Define the route handler in `src/api/routes_*.rs`

```rust
#[utoipa::path(
    post,
    path = "/api/v1/chats/{chat_id}/moderation/ban",
    request_body = BanUserRequest,
    responses(
        (status = 200, body = ModerationActionResponse),
        (status = 401, body = ApiError),
        (status = 403, body = ApiError),
        (status = 502, body = ApiError),
    ),
    security(("bearer_auth" = []))
)]
pub async fn ban_user(
    State(service): State<Arc<ModerationService>>,
    Extension(ctx): Extension<DashboardContext>,
    Path(chat_id): Path<i64>,
    Json(body): Json<BanUserRequest>,
) -> ApiResult<ModerationActionResponse> {
    if !ctx.can_moderate(chat_id) {
        return api_error!("MODERATOR_REQUIRED", "not a moderator of this chat", StatusCode::FORBIDDEN);
    }
    match service.ban_user(chat_id, body.user_id, body.reason).await {
        Ok(action) => api_success!(action.into()),
        Err(e) => api_error!("BOT_API_ERROR", e.to_string(), StatusCode::BAD_GATEWAY),
    }
}
```

Note `Path<i64>` — Telegram chat IDs are always `i64`.

### 2. Register in router (`src/api/server.rs`)

Add the route to the appropriate router group. Match the authentication and access level:

- **No auth**: public report endpoints (`GET /report/{chat_slug}`, `GET /report/{chat_slug}/chart.png`). Goes through `pub_rate_limit_middleware`.
- **Dashboard JWT** (`webapp_auth_middleware`): inserts `DashboardContext { user_id, chat_ids }` into extensions. Server-side double-check on `chat_id` is mandatory.
- **Admin secret** (`admin_secret_middleware`): inserts no extension; constant-time compare against `CONFIG_ADMIN_SECRET`. Used by ops scripts only.

### 3. Add OpenAPI schema types

```rust
#[derive(Deserialize, ToSchema)]
pub struct BanUserRequest {
    pub user_id: i64,
    pub reason: Option<String>,
}

#[derive(Serialize, ToSchema)]
pub struct ModerationActionResponse {
    pub id: Uuid,
    pub chat_id: i64,
    pub target_user_id: i64,
    pub action: String,   // "ban" | "unban" | "verify" | "unverify" | "delete"
    pub created_at: chrono::DateTime<chrono::Utc>,
}
```

Register schemas in the OpenAPI config in `server.rs`.

### 4. Update documentation

Update [`server/docs/api.md`](../api.md) with the new endpoint.

## Authentication Layers

| Layer | Inserts | Checks |
|---|---|---|
| `pub_rate_limit_middleware` | nothing | rate limit per IP |
| `webapp_auth_middleware` | `DashboardContext { user_id: i64, chat_ids: Vec<i64> }` | JWT signature + expiry; `chat_ids` claim derived from `chat_moderators` at mint time |
| `admin_secret_middleware` | nothing | `X-Admin-Secret` header constant-time eq `CONFIG_ADMIN_SECRET` |

The dashboard ALWAYS uses `webapp_auth_middleware`. The `admin_secret_middleware` is for `cargo run --bin admin-...` style ops tools, never reachable from the website.

## Server-side `chat_id` verification

The JWT's `chat_ids` claim is a **UI hint** — the dashboard uses it to hide tabs the user cannot view. The server MUST re-verify on every chat-scoped endpoint:

```rust
if !ctx.chat_ids.contains(&chat_id) {
    return api_error!("MODERATOR_REQUIRED", "not a moderator of this chat", StatusCode::FORBIDDEN);
}
```

Skipping this check is an IDOR bug.

## Response Conventions

- **200 OK**: GET, POST, PATCH success. Wrapped in `{"status":"ok","data":{...}}`.
- **204 No Content**: DELETE success. No body.
- **List endpoints**: return `{items: [], has_more: bool, cursor?: string}` for paginated results.
- Use `api_success!(data)` for success, `api_error!(code, message)` or `api_error!(code, message, status)` for errors.

## Pagination

Use cursor-based pagination for all list endpoints:

```rust
let (cursor_created_at, cursor_id) = decode_cursor(&cursor_str)?;
let items = sqlx::query_as!(
    ModerationAction,
    "SELECT ... FROM moderation_actions
     WHERE chat_id = $1 AND (created_at, id) < ($2, $3)
     ORDER BY created_at DESC, id DESC
     LIMIT $4",
    chat_id, cursor_created_at, cursor_id, limit + 1
).fetch_all(pool).await?;
```

## Validation

Validate input in the service layer or via an extractor. Never trust path / query / body without bounds.

- `chat_id`: must match the JWT's `chat_ids` whitelist (above).
- `user_id`: bounds-check (positive `i64`).
- Free-form text (reason, search query): max length, no nulls, no control chars.

## Checklist

- [ ] Route handler is thin (validation and logic in service layer)
- [ ] Handler returns `ApiResult<T>` using `api_success!()` / `api_error!()`
- [ ] OpenAPI annotations are complete (`#[utoipa::path(...)]`)
- [ ] Request/response types derive `ToSchema`
- [ ] Route is registered in `server.rs`
- [ ] Schemas are registered in OpenAPI config
- [ ] Authentication layer is correct (none / webapp / admin secret)
- [ ] Server-side `chat_id` whitelist check on chat-scoped endpoints
- [ ] [`server/docs/api.md`](../api.md) updated
