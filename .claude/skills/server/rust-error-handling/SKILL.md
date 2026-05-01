---
name: rust-error-handling
description: Apply vixen-rs Rust error-handling conventions — AppError variants, thiserror, ? propagation, IntoResponse mapping, no unwrap/expect in production paths, Telegram error swallowing. Use when writing or refactoring error types, `Result` chains, `From` impls, or error mapping for HTTP responses.
---

# Rust Error Handling (Vixen server)

**Read first**: [server/docs/rules/error-handling.md](../../../../server/docs/rules/error-handling.md) — the canonical rules. This skill is a reminder, not a replacement.

## Core rules

- **One top-level `AppError` enum** (in [server/src/](../../../../server/src/) errors module) with `thiserror::Error` derives. Route handlers return `Result<_, AppError>`.
- **No `unwrap()` / `expect()`** in request- or update-handling code. Only tolerated in:
  - Tests.
  - `bin/server.rs` startup (with a descriptive `.expect("reason")`).
  - Constants proven infallible at compile time.
- **No `anyhow::Error` on public APIs**. `anyhow` is fine for scripts in `server/examples/` but not for library code reachable from HTTP handlers or Telegram handlers.
- **`?` over match** for propagation. Add `From` impls (or `#[from]` in `thiserror`) so `?` works across error types.
- **Map errors at boundaries**, not at the top of a function. A SQLx error becomes `AppError::Database` via `#[from] sqlx::Error`; a not-found row becomes `AppError::NotFound` via `.ok_or(...)`.

## IntoResponse

`AppError` must implement `IntoResponse` so Axum can turn it into an HTTP response. Vixen-specific codes (defined in [server/docs/rules/error-handling.md](../../../../server/docs/rules/error-handling.md)):

- `NotFound` → 404 with `{ "error": "not_found", "message": "..." }`.
- `Unauthorized` → 401 (missing/invalid JWT).
- `Forbidden` → 403 (`MODERATOR_REQUIRED`, `CHAT_NOT_WATCHED`).
- `BadRequest(String)` / validation → 400.
- `Conflict` → 409 (e.g. unique-constraint violations from SQLx).
- `InvalidInitData` / `InitDataExpired` → 401.
- `CaptchaExpired` / `CaptchaFailed` → typically don't surface as HTTP — they're Telegram flows.
- `BotApiError` → 502 (upstream Telegram failure) on the rare HTTP routes that proxy a Bot API call.
- `Internal(...)` → 500, log the underlying error with `tracing::error!`, never leak the message to the client.

Look at the existing variants before adding new ones — often the right variant already exists.

## Telegram-handler errors

`teloxide` handlers return `Result<()>`. Failure modes:

- **`teloxide::RequestError`** → log via `tracing::warn!` with throttling metadata, never panic. Wrap in `AppError::Telegram(#[from] teloxide::RequestError)` if it needs to bubble up to a service.
- **Never** `bot.send_message(...).await.unwrap()` — if Telegram is down, the handler must continue gracefully.
- **Never** propagate an error to the user via `bot.send_message` containing the raw error message — only localized, sanitized strings.
- A panic inside one handler must not stop the dispatcher. teloxide isolates handlers, but verify your handler signature returns `Result`.

## Logging

- `tracing::error!` at the boundary where the error becomes a 5xx. Below that, propagate silently.
- Include correlation fields (`chat_id`, `user_id`, `route`, `update_id`) in the span, not in the error message.
- Do not log request bodies, JWTs, bot tokens, or `initData`.

## Common mistakes

- `Result<_, Box<dyn Error>>` in public functions — use `AppError`.
- `.unwrap()` on `env::var` in hot paths — move to startup config loading via `clap`.
- Returning `StatusCode::INTERNAL_SERVER_ERROR` directly — construct an `AppError` so logging and shape stay consistent.
- Swallowing errors with `let _ = ...` — be explicit: either handle or propagate. Telegram fire-and-forget message sends should `let _ = bot.send_message(...).await.inspect_err(|e| tracing::warn!(?e, "failed to send"))` so failures are at least logged.

## Patterns

- Map `sqlx::Error` → `AppError::Database` via `#[from]` on the variant. Map `.fetch_one()` row-not-found early via `.ok_or(AppError::NotFound)?` at the row boundary, not buried inside business logic.
- Telegram send errors: don't `?` them silently. Use `.inspect_err(|e| tracing::warn!(?e, "telegram_send_failed"))` to log first; whether to propagate depends on whether the handler can degrade gracefully.
