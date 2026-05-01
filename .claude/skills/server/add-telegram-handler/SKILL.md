---
name: add-telegram-handler
description: Add a new teloxide update handler (Message, EditedMessage, CallbackQuery, ChatMemberUpdated, MyChatMember) routed through the dispatcher tree. Use when adding bot reactions to a Telegram event.
---

# Add Telegram Handler (Vixen server)

**Read first:**

- [server/docs/bot.md](../../../../server/docs/bot.md) — dispatcher tree, branches, watched-chats filter.
- [server/docs/rules/telegram-handlers.md](../../../../server/docs/rules/telegram-handlers.md) — handler conventions.
- [server/docs/rules/error-handling.md](../../../../server/docs/rules/error-handling.md) — `AppError`, `inspect_err` for graceful degradation.

## File layout

- Handler: `server/src/telegram/handlers/{name}.rs`.
- Wiring: branch added in `server/src/telegram/dispatcher.rs`.
- Tests: `server/tests/telegram_{name}.rs` or `#[cfg(test)] mod tests`.

## Signature

```rust
#[tracing::instrument(skip(bot, state), fields(chat_id = %update.chat.id, user_id))]
pub async fn handle_x(bot: Bot, update: Message, state: AppState) -> Result<()> {
    // ...
    Ok(())
}
```

## Steps

1. Create `handlers/{name}.rs` with one `pub async fn` per update kind.
2. Wire the dispatcher branch in `dispatcher.rs` under the matching `Update::filter_*` chain.
3. Wrap with `#[tracing::instrument]`; record `chat_id`, `user_id`, message kind.
4. Write tests using a mocked teloxide `Bot` or DB-backed integration tests.

## Gotchas

- **Watched-chats filter is the single source of truth** — set once at the top of the dispatcher tree. Don't re-check `chat_id` membership inside the handler.
- **CallbackQuery must `bot.answer_callback_query(q.id)` within ~30s**, even with no text. Telegram greys out the button forever otherwise.
- **`bot.send_*().await` shouldn't kill the handler**. Use `.inspect_err(|e| tracing::warn!(?e, "send failed"))?` (or swallow with `let _ = ...`) where graceful degradation matters more than the bubble-up.
- **Verify-user check first.** Hit the Moka cache (`state.verified.get((chat_id, user_id))`, 10min TTL) before issuing a captcha — avoids re-challenging an already-verified user.
- **Telegram IDs are `i64`.** Never `i32` (supergroup IDs `-100…` overflow), never `u64`.
- **No `.unwrap()` on `bot.*()` calls.** Network failures aren't bugs.

## Verification

- `cargo test --lib telegram::handlers`.
- `cargo clippy --all-targets -- -D warnings`.
- Manual: send the trigger event in a dev chat and watch the tracing span.

## Related

- `add-slash-command` — `/`-prefixed commands with `BotCommands` derive.
- `captcha-pipeline` — captcha-issue + solve flow.
- `spam-rule` — message-classification cascade.
- `tracing-spans` — how to wire spans + structured fields.
