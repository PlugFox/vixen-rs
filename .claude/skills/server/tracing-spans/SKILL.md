---
name: tracing-spans
description: Add structured tracing spans + fields to handlers, jobs, services. Redact tokens / initData / PII. Use #[instrument(skip(state, body))] for sensitive args.
---

# Tracing Spans (Vixen server)

**Source:** [Tokio tracing docs](https://tokio.rs/tokio/topics/tracing) + vixen `RedactedToken` newtype in [server/src/utils/redact.rs](../../../../server/src/utils/redact.rs).

**Read first:**

- [server/docs/observability.md](../../../../server/docs/observability.md).

## Span discipline

- **Per-request span**: `request_id`, `route`, `user_id` (if authenticated).
- **Per-update span**: `update_id`, `chat_id`, `user_id`.
- **Per-job span**: `job` (name), `iteration` (counter).
- Use `#[tracing::instrument]` to auto-create the span around a function. Spans nest automatically across `.await`.

## Skeleton

```rust
#[tracing::instrument(skip(state, body), fields(chat_id, user_id))]
async fn handle_message(state: AppState, body: Message) -> Result<()> {
    tracing::Span::current().record("chat_id", body.chat.id.0);
    tracing::Span::current().record("user_id", body.from.as_ref().map(|u| u.id.0));
    // ...
}
```

- Declare empty `fields(...)` up front; fill them with `Span::current().record(...)` once values are known.
- `skip(state, body)` keeps the `AppState` and untrusted payloads out of the span attributes.

## Log fields, not message text

```rust
// GOOD
tracing::info!(chat_id, user_id, action = "verify", "captcha_solved");
// BAD
tracing::info!("captcha solved for user {} in chat {}", user_id, chat_id);
```

- Structured fields are queryable in JSON logs; interpolated strings are not.
- The first text argument is the static event name — keep it short and stable.

## Redaction (non-negotiable)

- **Bot token** → wrap in `RedactedToken`; its `Display` impl prints `bot:...***`. Never `{bot_token}` raw.
- **`initData`** → `debug!` only; never `info!` or higher. CLAUDE.md mandate.
- **Phone numbers, full names, message bodies** → opt-in only, behind an explicit `pii = true` flag.
- **JWT body** → never logged. JWT id (`jti`) is OK as a correlation key.

## Levels

- `error!` — irrecoverable; alert.
- `warn!` — recoverable; investigate (CAS timeout, captcha asset missing).
- `info!` — user-visible action (verify, ban, report sent).
- `debug!` — developer-only; raw initData, request body shapes.
- `trace!` — heavy; off in prod.

## Skip large or sensitive args

- `#[tracing::instrument(skip(state, body))]` — `state` is huge, `body` may carry PII.
- `skip_all` for handlers that take many sensitive args; record fields explicitly via `Span::current().record(...)`.

## Gotchas

- `tracing::Span::current()` outside an instrumented function returns a no-op span — recording fields silently does nothing.
- `#[instrument]` is async-aware; the span enters and exits across each `.await`.
- Don't `tracing::info!("token: {}", bot_token)` — even debug-level is risky for tokens; always go through `RedactedToken`.
- Console subscriber + JSON file rotation (7d) configured in `bin/server.rs`. Don't add a second subscriber.

## Verification

- `cargo build` — `instrument` macro errors are compile-time.
- Manual: trigger handler, inspect log file at `logs/vixen-server.log` for required fields.
- Grep the diff for `bot_token`, `init_data`, `phone` in any `tracing::` macro at `info!` level or above.

## Related

- `rust-error-handling`, `add-api-route`, `add-telegram-handler`, `background-job`.
