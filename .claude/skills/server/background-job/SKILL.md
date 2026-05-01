---
name: background-job
description: Add a periodic background job (interval-driven or wall-clock scheduled) — tokio::spawn + CancellationToken + idempotency + tracing span. Use for daily reports, captcha expiry, cleanup.
---

# Background Job (Vixen server)

**Read first:**

- [server/docs/background-jobs.md](../../../../server/docs/background-jobs.md) — registered jobs, intervals.
- [server/docs/rules/background-jobs.md](../../../../server/docs/rules/background-jobs.md) — idempotency, cancellation, tracing.

## Skeleton

```rust
pub const NAME: &str = "jobname";
pub const INTERVAL: Duration = Duration::from_secs(60);

pub async fn run(state: AppState, shutdown: CancellationToken) -> anyhow::Result<()> {
    let mut tick = tokio::time::interval(INTERVAL);
    tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            biased;
            _ = shutdown.cancelled() => return Ok(()),
            _ = tick.tick() => {
                if let Err(e) = run_once(&state).await {
                    tracing::warn!(job = NAME, ?e, "iteration failed");
                }
            }
        }
    }
}
```

## Steps

1. Create `server/src/jobs/{job}.rs` with `NAME`, `INTERVAL`, `run`, and a private `run_once`.
2. Register in `server/src/jobs/mod.rs::spawn_all()`.
3. Wrap `run_once` with `#[tracing::instrument(skip(state), fields(job = NAME))]`.
4. For wall-clock fire (daily report at `report_hour`), use `tokio::time::sleep_until(next_fire)` and recompute `next_fire` after each tick.
5. Document in the job table in `server/docs/background-jobs.md`.

## Gotchas

- **Idempotency mandatory.** Use `INSERT ... ON CONFLICT DO NOTHING` or check `processed_at IS NULL` first. Job restarts must not double-act.
- **Always `select!` on `shutdown.cancelled()` at loop entry.** A bare `sleep(3600s)` blocks SIGTERM for an hour.
- **Return `Ok(())` on iteration failure.** A bubbled `Err` from the loop kills the job for the rest of the process lifetime — no retries.
- **Multi-replica future (M6 webhook):** wrap the first DB op of `run_once` in `pg_try_advisory_lock(hashtext(NAME)::int)` so only one replica runs the job per tick.
- **Startup jitter.** Add a small random delay (`tokio::time::sleep(Duration::from_millis(rand 0..1000))`) before the first tick to avoid all jobs firing at boot.
- **Telegram IDs `i64`** in any SQL the job touches.

## Verification

- `cargo test jobs`.
- Manual: trigger SIGTERM during a slow iteration; the job must exit within seconds.

## Related

- `transaction-discipline` — `FOR UPDATE` patterns inside `run_once`.
- `tracing-spans` — span hierarchy under the job span.
- `connection-pool-tuning` — long-running jobs and the SQLx pool.
