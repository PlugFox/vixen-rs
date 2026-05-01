---
name: add-slash-command
description: Register a /slash_command (BotCommands derive) with permission check, dispatcher wiring, and i18n help text. Use for /verify, /ban, /unban, /stats, /help and similar.
---

# Add Slash Command (Vixen server)

**Read first:**

- [server/docs/bot.md](../../../../server/docs/bot.md) — slash-command table, dispatcher topology.
- [server/docs/rules/telegram-handlers.md](../../../../server/docs/rules/telegram-handlers.md) — handler conventions.

## Steps

1. Add a variant to the `Command` enum in `server/src/telegram/commands.rs` (with `description` attr — feeds the menu UI).
2. Implement `handle_{cmd}` in `server/src/telegram/handlers/commands.rs`.
3. Permission check via `state.moderation.is_moderator(chat_id, user_id).await?` **inside the handler**, before any side effect.
4. Register `dptree::case![Command::X]` in `dispatcher.rs` under the commands branch.
5. Update the slash-command table in `server/docs/bot.md`.
6. Push `setMyCommands` via the ops endpoint or one-shot CLI on next deploy.

## Skeleton

```rust
#[derive(BotCommands, Clone)]
#[command(rename_rule = "lowercase")]
pub enum Command {
    #[command(description = "Verify a user (reply or by id).")]
    Verify { user_id: Option<i64> },
    #[command(description = "Ban a user.")]
    Ban { user_id: Option<i64> },
}
```

## Gotchas

- **User ID is `i64`.** Always.
- **Permission check inside the handler, not the dispatcher.** `BotCommands` derive doesn't enforce moderator scope. Don't trust the message just because the dispatcher routed it.
- **Optional args** (`Option<i64>`) let `/verify` work both as a reply (no args) and with an explicit id. Resolve in this order: explicit arg → `msg.reply_to_message().from.id` → reject with help text.
- **`setMyCommands` is not automatic.** Add the new entry to the deploy-time push, or moderators won't see it in the Telegram menu.
- **`description` is mandatory for menu UX.** A missing description hides the command from the slash menu.
- **Reply-mode vs id-mode** is a single handler — branch on `args.user_id` then `msg.reply_to_message()`.

## Verification

- `cargo build` — derive macro errors out at compile time.
- Manual: type the command in a dev chat with both moderator and non-moderator accounts.

## Related

- `add-telegram-handler` — non-command updates.
- `per-chat-config` — commands that mutate config (transactional update).
