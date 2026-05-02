# Manual Moderation

Anything a human moderator does — through slash commands or the dashboard — flows through `src/services/moderation_service.rs`. Bot-driven actions (auto-bans from spam pipeline, auto-kicks from captcha expiry) flow through the same service, just with a different `actor_kind`.

## Surfaces

### Slash commands (in any watched chat)

| Command | Form | Effect |
|---|---|---|
| `/verify <user_id>` | reply or arg | Insert `verified_users` + `moderation_actions(action='verify', actor_kind='moderator')`. Bypasses any pending captcha. |
| `/ban` | reply | Ban the replied-to user. Optional `<reason>` from rest of command line. |
| `/ban <user_id>` | id-mode | Same as above, by id. |
| `/unban <user_id>` | id-mode | Lift the ban. |

`/verify` and `/ban`/`/unban` reject non-moderators with a localized message and no DB write.

### Dashboard (`/app/chats/{chat_id}/moderation`)

- **Action ledger** — paginated list of `moderation_actions` for this chat, with filters (action type, actor_kind, date range, target user id).
- **Manual ban** — input box for `user_id` + `reason`; POST to `/api/v1/chats/{chat_id}/moderation/ban`.
- **Manual unban** — input box for `user_id`; POST to `.../moderation/unban`.
- **Force verify** — input box for `user_id`; POST to `.../moderation/verify`.
- **Force unverify** — POST to `.../moderation/unverify` (requires confirmation; rare).

All endpoints validate that the JWT's `chat_ids` claim contains the requested `chat_id` (server-side) — the UI hides tabs for non-moderated chats but the API enforces.

## Permission check

`moderation_service::is_moderator(chat_id, user_id)`:

```sql
SELECT EXISTS(
  SELECT 1 FROM chat_moderators
  WHERE chat_id = $1 AND user_id = $2
);
```

Cached in Moka (5min TTL). Invalidated on `chat_moderators` write.

Adding moderators is **not** a runtime feature in v1 — `chat_moderators` is seeded by an ops script (`cargo run --bin seed-moderator -- --chat-id ... --user-id ...`) or by direct SQL. Future versions may add a self-service "add moderator" flow gated by an existing moderator's confirmation.

## Action ledger

`moderation_actions` schema highlights:

- `id UUID PRIMARY KEY DEFAULT uuid_generate_v4()`
- `chat_id BIGINT NOT NULL REFERENCES chats(chat_id) ON DELETE CASCADE`
- `target_user_id BIGINT NOT NULL`
- `action TEXT NOT NULL CHECK (action IN ('ban', 'unban', 'mute', 'unmute', 'delete', 'verify', 'unverify'))`
- `actor_kind TEXT NOT NULL CHECK (actor_kind IN ('bot', 'moderator'))`
- `actor_user_id BIGINT` — NULL when `actor_kind = 'bot'`
- `message_id BIGINT` — NULL when not message-scoped (e.g. manual ban without a referenced message)
- `reason TEXT` — free-form (or JSON for spam pipeline; see [spam-detection.md](spam-detection.md))
- `created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()`
- **UNIQUE (chat_id, target_user_id, action, message_id)** — idempotency anchor

The uniqueness key means re-processing the same operation (Telegram retry, bot restart mid-handler) does not double-action. The service catches the unique-violation and treats it as success.

## Concurrent moderator + bot

Race scenario: bot detects spam and starts the ban flow; a moderator simultaneously bans the same user from the dashboard.

1. First INSERT into `moderation_actions` succeeds — that one is recorded.
2. Second INSERT fails uniqueness. Service catches, treats as already-banned, skips the API call.
3. End state: one ledger entry, one `bot.ban_chat_member` call. Consistent.

If `actor_kind` differs between the two attempts (bot vs moderator), the recorded one is whichever landed first.

### id-mode (NULL message_id) atomicity

The unique key `(chat_id, target_user_id, action, message_id)` doesn't help
when `message_id IS NULL` — Postgres treats NULLs as distinct, so two
concurrent id-mode `/ban 12345` calls would both insert and both fire
`ban_chat_member`. `ModerationService::apply` handles this by opening a
transaction, taking `SELECT 1 FROM chats WHERE chat_id = $1 FOR UPDATE`,
running a behaviour check (`last action in (ban, unban) for this target ==
the action we're about to take?`), and only inserting + executing the bot
call when the behaviour differs. Concurrent id-mode attempts serialise on
the chat row lock; the second attempt's behaviour check sees the first's
`ban` and short-circuits to `Outcome::AlreadyApplied` without writing a
second row. Reply-mode bans (with a real `message_id`) skip the lock and
rely on the unique constraint, which Postgres serialises for free.

## Audit trail

The dashboard's audit-log search is the read view onto `moderation_actions`. Public-report page does NOT show this — the action ledger contains user IDs, which are PII.

A future "audit-log search" feature (see [docs/features.md](../../docs/features.md)) adds keyset-paginated filtering to make a year-long ledger usable.

## Failure modes

| Failure | Effect | Recovery |
|---|---|---|
| `bot.ban_chat_member` returns 403 (bot not admin) | Ledger row inserted, ban not applied | Surface in dashboard error toast; moderator promotes the bot manually |
| `bot.ban_chat_member` returns 400 (user not in chat) | Ledger row inserted with reason update; effectively no-op | Acceptable — recorded intent |
| Database down | Service errors before any API call | Dashboard returns 500; retry safely (uniqueness key) |

## Related

- Service: `src/services/moderation_service.rs`
- Routes: `src/api/routes_moderation.rs`
- Slash commands: `src/telegram/handlers/commands.rs`
- Dashboard: `website/src/features/moderation/`
- Schema: [database.md](database.md)
