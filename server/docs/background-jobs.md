# Background Jobs

Internal scheduler in `src/jobs/`. One `tokio::spawn` per registered job. Shared `CancellationToken` for graceful shutdown.

**Not** a generic queue. Single-tenant doesn't justify the ops cost of Postgres-backed queues (no advisory locks, no retry tables, no dead-letter queue). When we ever shard horizontally (post-webhook M6+), we add a `pg_try_advisory_lock` at job start to enforce single-writer semantics.

## Registry

`src/jobs/mod.rs::spawn_all(state, shutdown) -> Vec<JoinHandle<()>>` spawns each job. `bin/server.rs` collects the handles and `join_all`s them on shutdown.

## Job inventory

| Job | Interval | Purpose | Notes |
|---|---|---|---|
| [`captcha_expiry`](#captcha_expiry) | 60s | Sweep expired captcha rows; kick the user. | Idempotent. Cheap. |
| [`spam_cleanup`](#spam_cleanup) | 24h | Drop `spam_messages` rows older than 14 days. | Idempotent. |
| [`chat_info_refresh`](#chat_info_refresh) | 6h | Re-fetch `getChat` for each watched chat into `chat_info_cache`. | Hits Telegram API; throttled. |
| [`daily_report`](#daily_report) | per-chat at `chat_config.report_hour` | Aggregate, render PNG, send via bot. | Wall-clock scheduled. |
| [`summary_generation`](#summary_generation) | gated, fires after `daily_report` if OpenAI is enabled | Sanitize chat content → POST to OpenAI → append to report caption. | Per-chat token budget. |

## Job pattern

See [rules/background-jobs.md](rules/background-jobs.md) for the full contract. Skeleton:

```rust
pub const NAME: &str = "captcha_expiry";
pub const INTERVAL: Duration = Duration::from_secs(60);

pub async fn run(state: AppState, shutdown: CancellationToken) -> anyhow::Result<()> {
    let mut interval = tokio::time::interval(INTERVAL);
    interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            biased;
            _ = shutdown.cancelled() => return Ok(()),
            _ = interval.tick() => {
                if let Err(e) = do_one_pass(&state).await {
                    tracing::warn!(job = NAME, ?e, "iteration failed");
                }
            }
        }
    }
}

#[tracing::instrument(skip(state), fields(job = NAME))]
async fn do_one_pass(state: &AppState) -> anyhow::Result<()> { /* ... */ Ok(()) }
```

## captcha_expiry

```sql
DELETE FROM captcha_challenges
WHERE expires_at < NOW()
RETURNING chat_id, user_id, telegram_message_id;
```

For each returned row:

1. `bot.delete_message(chat_id, telegram_message_id)` — remove the captcha image (skip if NULL).
2. `bot.unban_chat_member(chat_id, user_id, only_if_banned=false)` — lift any restrict.
3. `bot.kick_chat_member(chat_id, user_id)` then `bot.unban_chat_member(...)` — remove from chat without banning (so they can rejoin).

If any Telegram call fails, log and continue. Next tick will not retry the user (the row is already deleted) — this is intentional: we don't want a bot hiccup to leave a hard-banned-by-accident state.

## spam_cleanup

```sql
DELETE FROM spam_messages
WHERE last_seen < NOW() - INTERVAL '14 days';
```

That's it. No side effects.

## chat_info_refresh

For each `chat_id` in `CONFIG_CHATS`:

```rust
let info = bot.get_chat(chat_id).await?;
sqlx::query!(
    "INSERT INTO chat_info_cache (chat_id, title, type, description, members_count, updated_at)
     VALUES ($1, $2, $3, $4, $5, NOW())
     ON CONFLICT (chat_id) DO UPDATE
     SET title = EXCLUDED.title, type = EXCLUDED.type, description = EXCLUDED.description,
         members_count = EXCLUDED.members_count, updated_at = NOW()",
    chat_id, info.title, info.kind_str(), info.description, info.members_count
).execute(&state.pool).await?;
```

Throttled: 1 request per second between chats (don't burst the Bot API for ops data).

## daily_report

Wall-clock scheduled. The job loop computes:

```rust
let next_fire = next_local_time(chat_config.timezone, chat_config.report_hour);
tokio::select! {
    biased;
    _ = shutdown.cancelled() => return Ok(()),
    _ = tokio::time::sleep_until(next_fire.into()) => {
        if let Err(e) = generate_and_post_report(state, chat_id).await {
            tracing::warn!(job = NAME, chat_id, ?e, "daily report failed");
        }
    }
}
```

`generate_and_post_report` is idempotent on a per-day basis: it checks `report_messages WHERE chat_id = $1 AND kind = 'daily' AND generated_at::date = current_date`. If a row exists, the job no-ops (a previous tick already posted today, or a moderator triggered `regenerate`).

Side effects: `report_service::aggregate` + `plotters` render + `bot.send_photo` + `report_messages` upsert. See [reports.md](reports.md).

## summary_generation

Fires immediately after `daily_report` succeeds, gated by:

- `CONFIG_OPENAI_KEY` set
- `chat_config.summary_enabled`
- `daily_stats(... 'openai_tokens')` < `chat_config.summary_token_budget`

Pipeline:

1. Pull last-24h `allowed_messages.content` (limit 2000 messages).
2. Sanitize (strip @mentions, phone-like sequences, URLs).
3. POST to OpenAI Chat Completions.
4. Append response to the existing report caption (or post as a separate message).
5. `INSERT ... ON CONFLICT` to `daily_stats(... 'openai_tokens')` += response token usage.

Failure is silent (logged at warn): the report still has its base caption. Token budget exhaustion → skip with a one-line caption note.

## Multi-instance safety (planned)

When the bot eventually runs N replicas behind a webhook load balancer, only **one** replica should run scheduler-driven jobs. The pattern:

```rust
let acquired: bool = sqlx::query_scalar!(
    "SELECT pg_try_advisory_lock(hashtext($1)::int)", NAME
).fetch_one(&state.pool).await?;
if !acquired { return Ok(()); }   // another replica owns this tick
defer! { sqlx::query!("SELECT pg_advisory_unlock(hashtext($1)::int)", NAME).execute(...).await; }
```

Not in v1. Documented here so the migration is clear when M6 lands.

## Observability

Every job iteration emits one `tracing::info_span!("job", name = NAME)` span. Grep prod logs for `job=daily_report` to see one chat's report run, including aggregation timing and bot-API call latency.

A planned `/admin/jobs/status` endpoint exposes last-run timestamp + last-error per job. Not in M0.

## Failure modes

| Failure | Effect | Recovery |
|---|---|---|
| Job iteration `Err` | Logged at `warn`, loop continues | Next tick retries |
| Job task panics | Loop dies; the job stops until restart | Wrap risky calls in `Result`; or watch panics in `JoinHandle` and re-spawn (overkill for v1) |
| Postgres unavailable mid-iteration | Iteration `Err`s; loop continues | Auto |
| Telegram API outage during `daily_report` | Today's report not posted; `report_messages` not written | Tomorrow's tick will not back-fill (intentional — we don't post yesterday's report a day late) |
| OpenAI API outage during `summary_generation` | Summary skipped, base report still posted | Auto — silent degradation |

## Related

- Code: `src/jobs/`
- Rule: [rules/background-jobs.md](rules/background-jobs.md)
- Skill: `.claude/skills/server/background-job/SKILL.md` (added in M2)
