# Telegram Handler Rules

Read this file before adding or modifying a teloxide handler.

## Where handlers live

`src/telegram/handlers/*.rs`. One file per concern (`captcha.rs`, `member_update.rs`, `commands.rs`, `messages.rs`). Each handler is `pub async fn handle_X(...) -> Result<()>` and is wired into the dispatcher tree in `src/telegram/dispatcher.rs`.

## Mandatory invariants

1. **Watched-chats filter runs before everything.** The dispatcher tree starts with `dptree::filter(|update: Update, state: AppState| watched(update.chat_id(), &state.config.chats))`. Handlers for non-watched chats are never reached. Do not re-check inside individual handlers — the filter is the single source of truth.
2. **Verified-user check runs before any anti-spam path.** A verified user's message bypasses the spam pipeline (modulo `clown_chance`). Re-issuing a captcha to a verified user is a bug.
3. **Never `bot.send_*().await.unwrap()`.** If Telegram is down, log and return — the handler must continue gracefully so the dispatcher acks the update.
4. **CallbackQuery handlers must always answer ≤30s.** Telegram retries the update if you don't. Even an empty `bot.answer_callback_query(q.id).await?` counts.
5. **Idempotency.** Telegram retries updates on transient failure (network blip, bot restart). Handlers MUST tolerate replay: check the action ledger / DB state before acting. Use `(message_id, chat_id)` or `callback_id` as the dedup key.
6. **Errors via `?`, never panic.** A panic crashes the dispatcher's task. teloxide isolates each update, but a panic in a shared dependency (PgPool, captcha service) propagates further than you want.

## Handler signature

```rust
pub async fn handle_captcha_callback(
    bot: Bot,
    q: CallbackQuery,
    state: AppState,
) -> Result<()> {
    // ...
}
```

The dispatcher injects everything from `AppState`. Don't reach for globals — accept what you need as parameters.

## Slash commands

Use teloxide's `BotCommands` derive in `src/telegram/commands.rs`:

```rust
#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase", description = "Vixen bot commands")]
pub enum Command {
    #[command(description = "show help")]
    Help,
    #[command(description = "show bot status in this chat")]
    Status,
    #[command(description = "manually verify a user (admin)")]
    Verify { user_id: i64 },
    #[command(description = "ban a user (admin)")]
    Ban,
    #[command(description = "unban a user (admin)")]
    Unban { user_id: i64 },
    #[command(description = "show today's stats")]
    Stats,
}
```

Permission check happens inside the handler:

```rust
pub async fn handle_ban_command(bot: Bot, msg: Message, state: AppState) -> Result<()> {
    if !state.moderation.is_moderator(msg.chat.id.0, msg.from.as_ref().unwrap().id.0).await? {
        let _ = bot.send_message(msg.chat.id, t("commands.not-moderator")).await;
        return Ok(());
    }
    // ...
}
```

When you add a slash command, register it both in the `Command` enum AND in [`docs/bot.md`](../bot.md)'s command table.

## Throttling

Use `teloxide::adaptors::Throttle` once at construction time, not per-call:

```rust
let bot = Bot::new(token).throttle(Limits::default());
```

The Throttle adapter respects Telegram's per-chat (1 msg/sec for groups) and global (30 msg/sec) limits. Don't add ad-hoc `tokio::time::sleep` between sends.

## Restricting / banning users

- `bot.restrict_chat_member(chat_id, user_id, ChatPermissions::default())` — silences the user (used during captcha).
- `bot.ban_chat_member(chat_id, user_id)` with optional `until_date` — actual ban.
- `bot.unban_chat_member(chat_id, user_id)` — undo ban (requires `only_if_banned: true` so it doesn't unban-then-rejoin a recent leaver).

Always wrap the call in the moderation service so the action is recorded in `moderation_actions` BEFORE the API call. If the API call fails, log and let the next retry catch it (uniqueness key prevents double-action).

## Tests

Use `teloxide-tests::MockBot`:

```rust
#[tokio::test]
async fn captcha_solve_marks_verified() {
    let pool = setup_test_db().await;
    let bot = MockBot::new(vec![]);
    // simulate the chain of updates
    // assert DB state and bot's recorded API calls
}
```

See [`rules/testing.md`](testing.md) for the full pattern.

## Common mistakes

- Calling `bot.send_message(...).await?` directly — the `?` bubbles the error to the dispatcher; if you wanted graceful degradation, use `let _ = bot.send_message(...).await.inspect_err(|e| tracing::warn!(?e, "send failed"));`.
- Assuming `msg.from` is always `Some` — channel messages can have `from = None`.
- Forgetting `answer_callback_query` on a CallbackQuery handler — Telegram retries the update.
- Re-issuing a captcha to an already-verified user — check `verified_users` first.
- Using `Update::filter_message().branch(...)` without a watched-chats filter higher up — every handler then has to check itself, and one omission becomes a security hole.
