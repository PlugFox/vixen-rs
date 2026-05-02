# Telegram Bot Anatomy

teloxide-based dispatcher inside the same Rust process as the HTTP server. Polling in v1; webhook-ready abstraction for later (see [deployment.md](../../docs/deployment.md)).

## Dispatcher structure

```
Update
  └─ filter: chat_id ∈ CONFIG_CHATS  (watched-chats filter)
      ├─ branch: ChatMemberUpdated   → handle_chat_member_update
      ├─ branch: MyChatMember        → handle_my_chat_member
      ├─ branch: Message
      │   ├─ branch: command parsed   → handle_command (Help / Status / Verify / Ban / Unban / Stats)
      │   └─ branch: any other        → handle_message (spam pipeline)
      ├─ branch: EditedMessage       → handle_edited_message (re-runs spam check, log only)
      └─ branch: CallbackQuery
          └─ filter: data starts with "vc:"  → handle_captcha_callback
```

Built in `src/telegram/dispatcher.rs` using `dptree::case!` and `dptree::filter`.

## Watched-chats filter

The dispatcher's first node drops every update where `chat_id` is not in `CONFIG_CHATS`:

```rust
fn watched_chats_filter(update: Update, config: Arc<Config>) -> bool {
    let Some(chat_id) = chat_id_of(&update) else { return false };
    config.chats.contains(&chat_id)
}
```

This is the **single source of truth** for "is this chat ours". Don't re-check inside individual handlers — it's noisy and a missed check becomes a leak.

## Update-type routing

| Update | Handler | Purpose |
|---|---|---|
| `Message` (command) | `handle_command` | Slash-command dispatch — see table below. |
| `Message` (text/media) | `message_gate::handle` (then M2 spam pipeline) | Verified or admin → bypass. Unverified non-admin → delete the message; if no live captcha row, issue + post a fresh photo. **No restrict, no kick.** |
| `EditedMessage` | `handle_edited_message` | Re-run spam pipeline against the new content; log differential. |
| `ChatMemberUpdated` | `member_update::handle` | New non-admin joiner → issue captcha (no restrict). Owner/admin transitions and departures → no-op. |
| `MyChatMember` | `handle_my_chat_member` | Bot added to a chat (warn if not in `CONFIG_CHATS`) / removed from a chat (log). |
| `CallbackQuery` (`vc:*` data) | `captcha::handle` | User input on captcha digit-pad. Always answers within 30s; ownership-checked against the per-message Redis meta row. |

## Slash commands

| Command | Who can call | What it does |
|---|---|---|
| `/start` | anyone | Sends a one-line greeting + link to public report. In groups, reduced output. |
| `/help` | anyone | Lists available commands localized to chat language. |
| `/status` | anyone | "Vixen is watching this chat. {N} verified users today, {M} actions." Counts come from `daily_stats`. |
| `/verify <user_id>` or `/verify` (reply) | moderator | Force-verify a user without captcha. Records `moderation_actions` row with `actor_kind = 'moderator'`. |
| `/ban` (reply) or `/ban <user_id>` | moderator | Ban a user. Optional reason as remaining args. |
| `/unban <user_id>` | moderator | Lift a ban. |
| `/stats` | moderator | Inline summary of last 24h: messages, captchas, bans, spam hits, top phrases. |

Permission check is `is_moderator(chat_id, user_id)` against `chat_moderators`. Non-moderator gets a localized "not allowed" reply.

When you add a slash command, you MUST register it both in `Command` (in `src/telegram/commands.rs`) AND in this table.

## Captcha callback data

CallbackQuery `data` field carries the digit input encoded as `vc:<challenge_short>:<op>` where `<challenge_short>` is the first 8 hex characters of the challenge UUID. Ops: `0`–`9`, `bs` (backspace), `rf` (refresh).

Handler decodes, applies to `captcha_challenges.attempts_left` and the per-press digit buffer in Redis (`cap:input:{chat_id}:{user_id}`, TTL = challenge lifetime), updates the message caption with a length-only mask (`●●○○`) so the actual digits never leak through Telegram's update API, and on full input either solves or fails. Ownership is enforced via the per-message Redis meta key (`cap:meta:{chat_id}:{message_id}`): a callback whose presser does not match `meta.owner_user_id` gets a "this isn't your captcha" toast and the captcha is not touched.

## Polling vs webhook

- **v1 (polling)**: `Dispatcher::dispatch_with_listener` against a long-poll listener. Single process. No public ingress required.
- **Future (webhook)**: `routes_telegram_webhook.rs` decodes the `Update` and feeds the same dispatcher tree via a `mpsc` channel. Telegram POSTs with `X-Telegram-Bot-Api-Secret-Token` header validated server-side.

The split is in `src/telegram/dispatcher.rs::run` (`if config.mode == Polling { ... } else { ... }`). Handlers don't know which mode they're in.

## Error policy

- Handlers return `Result<()>`. `?` propagates to the dispatcher; teloxide acks the update and logs.
- A panic crashes the dispatcher's task — never panic.
- `bot.send_*().await?` in a handler is fine; if you want graceful degradation (don't fail the whole handler because a status reply timed out), use `let _ = bot.send_message(...).await.inspect_err(|e| tracing::warn!(?e, ...));`.
- Errors are logged, never echoed to the user as raw text.

## Throttling

`teloxide::adaptors::Throttle` is wrapped around the `Bot` handle once at construction:

```rust
let bot = Bot::new(token).throttle(Limits::default());
```

This respects:
- Per-chat: 1 message / second for groups, 30 / second for private.
- Global: 30 messages / second.

Outbound message bursts (e.g. report posting to all watched chats simultaneously) get queued automatically.

## Restrict / ban / unban

Always go through `ModerationService` so the action is recorded in `moderation_actions` BEFORE the API call. The uniqueness key `(chat_id, target_user_id, action, message_id)` makes retries safe.

```rust
state.moderation.ban(chat_id, user_id, reason, ActorKind::Bot, message_id).await?;
```

Direct `bot.ban_chat_member()` calls bypassing the service are forbidden — they break the audit log.
