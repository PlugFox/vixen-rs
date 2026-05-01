---
name: rust-async-tokio
description: Write correct async Rust on Tokio in the vixen-rs server — cancel-safety, spawn vs spawn_blocking, select!, timeouts, graceful shutdown across the bot poller, HTTP server, and background jobs. Use when writing async fns, background tasks, channels, or anything touching tokio::spawn / select! / timeout.
---

# Rust Async / Tokio (Vixen server)

**Source:** [Tokio docs](https://tokio.rs/tokio/topics) + foxic-derived conventions.

Vixen runs three long-lived async citizens in one process: the **HTTP server** (`axum::serve`), the **Telegram dispatcher** (teloxide polling), and the **background-job runner**. They all share one `Arc<AppState>` and one shutdown channel.

## Core rules

- **`tokio::spawn` for async work, `spawn_blocking` for CPU-heavy or blocking sync calls.** Blocking the executor (`std::fs`, `std::thread::sleep`, image rendering, font loading) starves every other task. CAPTCHA image generation should happen inside `spawn_blocking` if it ever measures > ~100µs (it almost certainly will).
- **Never hold a `std::sync::Mutex` / `parking_lot::Mutex` across `.await`.** Either drop the guard before awaiting, use `tokio::sync::Mutex`, or restructure. Violating this deadlocks under load.
- **Prefer message passing over shared state.** `tokio::sync::mpsc` + a single owner task is simpler than locks around shared data.
- **Every `.await` is a potential cancellation point.** Code between `.await` points runs to completion; code across them can be dropped mid-way.

## Cancel-safety

A future is *cancel-safe* if dropping it mid-`.await` leaves no broken invariants. This matters inside `tokio::select!`: the losing branch is dropped.

- Cancel-safe: `mpsc::Receiver::recv`, `broadcast::Receiver::recv`, `oneshot::Receiver`, `tokio::time::sleep`, `CancellationToken::cancelled()`, most I/O reads returning a fresh buffer.
- **Not** cancel-safe: anything that consumes input incrementally, custom state machines, partially-filled `read_exact`.
- If a branch is not cancel-safe, don't put the raw future in `select!` — wrap it in a spawned task and select on its `JoinHandle` / channel instead.

| Operation | Safe to drop mid-await? |
|---|---|
| `mpsc::Receiver::recv` | Yes |
| `tokio::time::sleep` | Yes |
| `CancellationToken::cancelled` | Yes |
| `Mutex::lock` | Yes |
| Custom state-machine reads (partial bytes) | NO — restart from scratch on next poll |
| In-progress writes to a connection | NO — connection state may be torn |
| Inside an active `sqlx::Transaction` | NO — leave the transaction open and the conn poisoned |

Rule: in `tokio::select!`, every branch must be cancel-safe. If unsure, wrap the unsafe operation in a `tokio::task::spawn` and `select!` on its `JoinHandle`.

## `select!`

```rust
tokio::select! {
    biased;                                   // shutdown must win
    _ = shutdown.cancelled() => break,
    msg = rx.recv() => handle(msg).await,
    _ = tokio::time::sleep(timeout) => warn_stall(),
}
```

- Use `biased;` when ordering matters (shutdown must win).
- Don't select on `&mut future` that is not cancel-safe.

## Timeouts

- Wrap external I/O (Postgres connect, Combot CAS, OpenAI, Telegram API) with `tokio::time::timeout(dur, fut)` at the boundary — not buried deep.
- Return `AppError::Timeout` (or map to the existing variant) on `Elapsed`; log at the boundary.
- Telegram long-polling has its own 30s timeout — don't wrap it in another shorter one.

## Graceful shutdown

- Single `tokio_util::sync::CancellationToken` passed into every long-running task: HTTP server (`axum::serve(...).with_graceful_shutdown(token.cancelled())`), the teloxide dispatcher (`Dispatcher::dispatch_with_listener` + a custom listener that respects the token), and every background job loop.
- `bin/server.rs` listens for `SIGTERM` / `SIGINT` via `tokio::signal`, calls `token.cancel()`, then `join!`s outstanding `JoinHandle`s with an outer timeout (e.g. 30s) before `process::exit`.
- Don't rely on drop order — explicitly signal, then wait.

## Background-job loops

```rust
async fn run(state: AppState, shutdown: CancellationToken) -> Result<()> {
    let mut interval = tokio::time::interval(JOB_INTERVAL);
    loop {
        tokio::select! {
            biased;
            _ = shutdown.cancelled() => return Ok(()),
            _ = interval.tick() => {
                if let Err(e) = do_one_pass(&state).await {
                    tracing::warn!(?e, "job iteration failed");
                    // do NOT return Err — keep the loop alive
                }
            }
        }
    }
}
```

A panic inside `do_one_pass` should be caught by the dispatcher — but defensively, wrap risky inner work in `tokio::task::spawn` + `JoinHandle::await` if you suspect panics.

## Common mistakes

- `tokio::spawn(async move { blocking_call() })` — use `spawn_blocking`.
- `let _ = tokio::spawn(...)` — you dropped the `JoinHandle`; errors silently vanish. Either `.await` it, store it, or explicitly `drop()` with a comment saying fire-and-forget is intentional.
- `Arc<Mutex<T>>` wrapped around something that needs `.await` — switch to `tokio::sync::Mutex` or an actor pattern.
- Busy-looping with `tokio::task::yield_now()` instead of waiting on a real event.
- Sleeping > 5s without a `tokio::select!` against shutdown — graceful shutdown will hang.

## Related

- Errors at boundaries: see `rust-error-handling` skill.
- DB connections: SQLx pool is already shared via `AppState`; don't wrap it in another lock.
