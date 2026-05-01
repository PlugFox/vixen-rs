# Error Handling Rules

Read this file before implementing error handling.

## ApiResult

All route handlers return `ApiResult<T>` defined in `src/api/response.rs`. This is an enum with three variants:

```rust
pub enum ApiResult<T> {
    Success(T),
    Error { code: String, message: String, status: StatusCode },
    File { content: Vec<u8>, filename: String, content_type: String },
}
```

## Response Format

**Success:**
```json
{
  "status": "ok",
  "data": { ... }
}
```

**Error:**
```json
{
  "status": "error",
  "error": {
    "code": "ERROR_CODE",
    "message": "Human-readable description"
  }
}
```

## Macros

```rust
api_success!(data)
api_error!("ERROR_CODE", "message")
api_error!("ERROR_CODE", "message", StatusCode::NOT_FOUND)
```

## Error Codes

| Code | HTTP | When to use |
|------|------|-------------|
| UNAUTHORIZED | 401 | Missing JWT |
| INVALID_TOKEN | 401 | JWT decode/validation failed |
| INVALID_INIT_DATA | 401 | Telegram WebApp `initData` HMAC check failed |
| INIT_DATA_EXPIRED | 401 | `auth_date` older than 24h |
| FORBIDDEN | 403 | Generic permission failure |
| MODERATOR_REQUIRED | 403 | User is not in `chat_moderators` for the requested chat |
| CHAT_NOT_WATCHED | 403 | Requested chat is not in `CONFIG_CHATS` |
| NOT_FOUND | 404 | Resource doesn't exist |
| CONFLICT | 409 | Duplicate uniqueness key |
| VALIDATION_ERROR | 400 | Invalid input data |
| CAPTCHA_EXPIRED | 410 | Challenge no longer valid (rare HTTP surface; mostly Telegram-side) |
| CAPTCHA_FAILED | 422 | Wrong solution; attempts decremented |
| BOT_API_ERROR | 502 | Upstream Telegram failure on a route that proxies a Bot API call |
| DATABASE_ERROR | 500 | DB query failed |

## Usage Patterns

### In route handlers

```rust
pub async fn get_chat(
    State(service): State<Arc<ChatService>>,
    Extension(ctx): Extension<DashboardContext>,
    Path(chat_id): Path<i64>,
) -> ApiResult<ChatResponse> {
    if !ctx.can_view(chat_id) {
        return api_error!("MODERATOR_REQUIRED", "not a moderator of this chat", StatusCode::FORBIDDEN);
    }
    match service.get_chat(chat_id).await {
        Ok(chat) => api_success!(ChatResponse::from(chat)),
        Err(_) => api_error!("NOT_FOUND", "chat not found", StatusCode::NOT_FOUND),
    }
}
```

### In services (return Result)

Services return `Result<T, E>`, not `ApiResult`. The route handler converts service errors to `ApiResult`. Use `thiserror`-derived error enums per service or fold into a shared `AppError`.

### Telegram handlers

Telegram handlers return `Result<()>`. Failures are **logged**, never bubbled up to the chat as raw error text:

```rust
pub async fn handle_new_member(bot: Bot, msg: Message, state: AppState) -> Result<()> {
    if let Err(e) = state.captcha.issue_challenge(msg.chat.id.0, user.id.0).await {
        tracing::warn!(?e, chat_id = msg.chat.id.0, user_id = user.id.0, "failed to issue captcha");
        // Optionally try a localized "something went wrong, please try again" message,
        // but never echo the error.
    }
    Ok(())
}
```

A `?` early return inside a teloxide handler is fine — teloxide's dispatcher catches the error, logs it, and ack's the update. A panic, however, would crash the dispatcher's task — so never panic.

### Conflict detection

```rust
if existing.is_some() {
    return api_error!("CONFLICT", "user already verified", StatusCode::CONFLICT);
}
```

## Rules

- Route handlers return `ApiResult<T>`, services return `Result<T, E>`, telegram handlers return `Result<()>`.
- Use specific error codes (table above), not generic strings.
- Error messages should be useful but not leak internals (no stack traces, SQL, file paths, bot tokens, raw initData).
- Log internal errors at `error!`; client / handler errors at `warn!`.
- Never `unwrap()` in request handlers, services, or telegram handlers. Allowed only in `bin/server.rs` startup with `.expect("reason")` and in tests.
- Never expose the bot token, JWT contents, or `initData` in any error message.
