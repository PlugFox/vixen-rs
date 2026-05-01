# Background-Job Rules

Read this file before adding or modifying a periodic task.

## Anatomy of a job

```rust
// src/jobs/captcha_expiry.rs
use tokio_util::sync::CancellationToken;
use crate::AppState;

pub const NAME: &str = "captcha_expiry";
pub const INTERVAL: std::time::Duration = std::time::Duration::from_secs(60);
pub const JITTER_SECS: u64 = 5;

pub async fn run(state: AppState, shutdown: CancellationToken) -> anyhow::Result<()> {
    let mut interval = tokio::time::interval(INTERVAL);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            biased;
            _ = shutdown.cancelled() => return Ok(()),
            _ = interval.tick() => {
                if let Err(e) = do_one_pass(&state).await {
                    tracing::warn!(job = NAME, ?e, "iteration failed");
                    // do NOT return Err — keep the loop alive
                }
            }
        }
    }
}

#[tracing::instrument(skip(state), fields(job = NAME))]
async fn do_one_pass(state: &AppState) -> anyhow::Result<()> {
    // ... idempotent work ...
    Ok(())
}
```

Register it in `src/jobs/mod.rs`:

```rust
pub fn spawn_all(state: AppState, shutdown: CancellationToken) -> Vec<tokio::task::JoinHandle<()>> {
    vec![
        spawn_named(captcha_expiry::NAME, captcha_expiry::run(state.clone(), shutdown.clone())),
        spawn_named(daily_report::NAME, daily_report::run(state.clone(), shutdown.clone())),
        // ...
    ]
}
```

## Mandatory invariants

1. **Idempotency.** Jobs run on overlap (e.g. clock skew, restart-mid-tick). Always assume the same operation may execute twice. Use `INSERT ... ON CONFLICT DO NOTHING` or `processed_at IS NULL` filters or the `moderation_actions` uniqueness key.
2. **Cancellation responsiveness.** The job loop MUST `select!` against `shutdown.cancelled()` at every iteration. Sleeps longer than 5s inside `do_one_pass` should themselves `select!` against shutdown.
3. **Panic isolation.** A panic inside `do_one_pass` is caught by `tokio::spawn`'s task boundary, but the job loop dies. Either wrap risky inner calls in `std::panic::catch_unwind` (rarely) or trust that `do_one_pass` returns `Result` and the loop survives `Err`s.
4. **Logging.** Every job has a `tracing::instrument` span with `job = NAME`. This is how you grep prod logs ("show me everything from `daily_report` on this date").
5. **Jitter.** When N replicas exist (post-webhook era), all interval-driven jobs fire on the same wall-clock tick. Add `+ random_jitter(JITTER_SECS)` to the first sleep to spread load.

## Job inventory (planned)

| Job | Interval | What it does |
|---|---|---|
| `captcha_expiry` | 60s | Sweep `captcha_challenges WHERE expires_at < NOW()`, kick the user (not ban), delete row. |
| `spam_messages_cleanup` | 24h | Delete `spam_messages WHERE last_seen < NOW() - INTERVAL '14 days'`. |
| `chat_info_refresh` | 6h | Call `getChat` for each watched chat, refresh `chat_info_cache`. |
| `daily_report` | per chat at `CONFIG_REPORT_HOUR` chat-local time | Aggregate, render PNG chart, send via bot, record `report_messages` row. |
| `summary_generation` | gated, fires after `daily_report` if OpenAI is enabled | Sanitize day's messages → POST to OpenAI → append summary as caption. |

## Scheduling on a cron-like time (per-chat report)

`tokio::time::interval` drifts and ignores wall-clock. For a "every day at 17:00 chat-local" schedule:

- Compute `next_fire = next_local_hour(chat.tz, CONFIG_REPORT_HOUR)`.
- `tokio::time::sleep_until(next_fire)`.
- After firing, recompute `next_fire` for the next day.

Don't reach for `tokio-cron-scheduler` — for a single-tenant bot with a handful of chats, the cost outweighs the benefit. Hand-rolled with `chrono` + `sleep_until` is ~30 lines.

## Multi-instance safety (post-webhook era)

When the bot eventually runs N replicas behind a webhook load balancer, only **one** replica should run scheduler-driven jobs. Use a Postgres advisory lock at job start:

```rust
let acquired: bool = sqlx::query_scalar!("SELECT pg_try_advisory_lock(hashtext('captcha_expiry')::int)")
    .fetch_one(&state.pool).await?;
if !acquired {
    // another replica is running this job; sleep and retry
    return Ok(());
}
```

This is **not** in v1 (single instance). It is the migration path for M6.

## Testing

Run `do_one_pass` against a fixture DB without spinning up the scheduler. The loop itself is too thin to need direct testing.

```rust
#[sqlx::test]
async fn captcha_expiry_kicks_expired(pool: PgPool) {
    seed_expired_challenge(&pool).await;
    let state = test_state(pool);
    captcha_expiry::do_one_pass(&state).await.unwrap();
    assert!(challenges_for_user(&state, ...).is_empty());
}
```

## Common mistakes

- `tokio::time::sleep(Duration::from_secs(60)).await` instead of `interval.tick()` — the loop drifts by the duration of `do_one_pass` each iteration.
- Returning `Err` from the loop — kills the job for the rest of the process lifetime.
- Forgetting the shutdown branch in `select!` — `SIGTERM` hangs for the duration of the longest sleep.
- A non-idempotent `do_one_pass` — restart during a tick double-actions.
- Adding a job without a `tracing::instrument` span — debugging prod becomes a guessing game.
