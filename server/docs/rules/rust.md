# Rust Conventions

## File Organization

- **Models** (`src/models/`): Separate DB model (SQLx derives) and API DTO (Serde derives) in the same file. Implement `From<DbModel>` for DTO conversions.
- **Services** (`src/services/`): Business logic only, no HTTP / Telegram concerns. Accept `&PgPool`, `&Database`, or `&mut PgConnection` (for transactions). Return `Result<T, E>`.
- **Routes** (`src/api/routes_*.rs`): Thin HTTP layer. Extract auth from extensions, call services, return `ApiResult<T>`. Register routes in `src/api/server.rs`.
- **Telegram handlers** (`src/telegram/handlers/*.rs`): Thin teloxide layer. Extract context from injected dependencies, call services, return `Result<()>`. Register in `src/telegram/dispatcher.rs`.
- **Jobs** (`src/jobs/*.rs`): One file per periodic task. Implement `async fn run(state, shutdown) -> Result<()>` + a `JobConfig`. Register in `src/jobs/mod.rs`.

## Models Pattern

```rust
// DB model (for SQLx queries)
#[derive(sqlx::FromRow)]
pub struct CaptchaChallenge {
    pub id: Uuid,
    pub chat_id: i64,                  // Telegram chat ID — i64
    pub user_id: i64,                  // Telegram user ID — i64
    pub solution: String,
    pub attempts_left: i16,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

// API DTO (for JSON responses to the dashboard)
#[derive(Serialize, ToSchema)]
pub struct CaptchaChallengeResponse {
    pub id: Uuid,
    pub chat_id: i64,
    pub user_id: i64,
    pub attempts_left: i16,
    pub expires_at: chrono::DateTime<chrono::Utc>,
    // NOTE: solution is intentionally NOT exposed
}

impl From<CaptchaChallenge> for CaptchaChallengeResponse {
    fn from(c: CaptchaChallenge) -> Self { /* ... */ }
}
```

## Service Pattern

```rust
pub async fn solve_challenge(
    pool: &PgPool,
    chat_id: i64,
    user_id: i64,
    submitted: &str,
) -> Result<SolveOutcome> {
    let mut tx = pool.begin().await?;
    let challenge = sqlx::query_as!(
        CaptchaChallenge,
        "SELECT ... FROM captcha_challenges WHERE chat_id = $1 AND user_id = $2 FOR UPDATE",
        chat_id, user_id
    ).fetch_optional(&mut *tx).await?
        .ok_or(AppError::CaptchaExpired)?;

    if challenge.solution != submitted {
        // decrement attempts, possibly fail
    } else {
        sqlx::query!("DELETE FROM captcha_challenges WHERE id = $1", challenge.id).execute(&mut *tx).await?;
        sqlx::query!("INSERT INTO verified_users (chat_id, user_id) VALUES ($1, $2)", chat_id, user_id).execute(&mut *tx).await?;
        tx.commit().await?;
        return Ok(SolveOutcome::Solved);
    }
    // ...
}
```

Services should not reference HTTP types (`StatusCode`, `Json`) or teloxide types (`Bot`, `Message`). The route or handler converts service results into the appropriate response type.

## Route Pattern

```rust
#[utoipa::path(
    post,
    path = "/api/v1/chats/{chat_id}/moderation/ban",
    request_body = BanUserRequest,
    responses(
        (status = 200, body = ModerationActionResponse),
        (status = 403, body = ApiError),
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

## Telegram-Handler Pattern

```rust
pub async fn handle_captcha_callback(
    bot: Bot,
    q: CallbackQuery,
    state: AppState,
) -> Result<()> {
    let Some(data) = q.data.as_deref() else { return Ok(()); };
    let Some((chat_id, user_id)) = decode_captcha_callback(data) else { return Ok(()); };

    let outcome = state.captcha.handle_input(chat_id, user_id, data).await?;
    bot.answer_callback_query(q.id).text(outcome.toast()).await?;
    // ... update message via editMessageCaption or editMessageMedia
    Ok(())
}
```

## Transactions

Use transactions for multi-step operations:

```rust
let mut tx = pool.begin().await?;
sqlx::query!("DELETE FROM captcha_challenges WHERE id = $1", id).execute(&mut *tx).await?;
sqlx::query!("INSERT INTO verified_users (chat_id, user_id, verified_at) VALUES ($1, $2, NOW())", chat, user).execute(&mut *tx).await?;
tx.commit().await?;
```

## Bot-token redaction

```rust
use crate::utils::redact::RedactedToken;

let token = RedactedToken::new(env::var("CONFIG_BOT_TOKEN")?);
tracing::info!(bot = %token, "starting poller");   // logs as bot=xxxxxx:****
```

Never call `tracing::*!` with the raw token in scope. There's no exception.

## Error Handling

See `rules/error-handling.md`. Key points:
- Route handlers return `ApiResult<T>`, services return `Result<T, E>`, telegram handlers return `Result<()>`.
- Use `api_success!()`, `api_error!()` macros at the HTTP boundary.
- Never panic in request handlers, services, or telegram handlers.

## Naming

- Files: `snake_case.rs`
- Structs/Enums: `PascalCase`
- Functions/variables: `snake_case`
- Constants: `SCREAMING_SNAKE_CASE`
- Route handlers: HTTP-action-named (`create_chat_config`, `list_moderation_actions`, `ban_user`)
- Telegram handlers: `handle_<purpose>` (`handle_new_member`, `handle_captcha_callback`)

## Formatting

Run before every commit:
```bash
cargo fmt --all
cargo clippy --all-targets --all-features -- -D warnings
```
