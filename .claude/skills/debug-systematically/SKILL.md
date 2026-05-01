---
name: debug-systematically
description: Reproduce → bisect → root cause → fix → verify. Don't speculate, don't "add more error handling" as a guess. Use when something is broken, when a test fails intermittently, when the user says "this isn't working" or "investigate why".
---

# Debug Systematically (Vixen)

**Source:** standard debugging discipline.

## The five steps

1. **Reproduce locally.** Exact command, exact error, exact input. If you can't reproduce, you can't fix — you can only guess.
2. **Read the error fully.** Stack trace, error code, log context. Don't react to the symptom message; trace to the root.
3. **Bisect.** What was the last known-good state? `git bisect` for commit-level; comment out half a function for line-level. Narrow until the offending change is isolated.
4. **Form a hypothesis.** State it explicitly: "I think it's X because Y." Then test it — don't apply a "fix" without confirming the cause.
5. **Verify the fix.** Re-run the exact reproduction. Run the broader test suite. "Looks fine to me" is not verification.

## Vixen-specific debugging

- **`pool timed out` / connection exhaustion** → check for long-running transactions that hold a connection across an `await` (`bot.send_*().await` inside a `tx`). Inspect `pg_stat_activity` for blocked queries.
- **`SQLX_OFFLINE=true` build fail** → run `/db-migrate` to refresh `.sqlx/`; commit it. Don't hand-edit JSON files in `.sqlx/`.
- **Captcha solve fails for legit user** → check determinism: render the same `challenge_id` twice, bytes identical? If not, the asset path or seed is non-deterministic.
- **Telegram webhook flapping / `update_id` gaps** → 30s timeout on CallbackQuery answer (TG drops it); or watched-chats filter mismatch sending the bot updates it ignores.
- **Spam rule false positives** → check normalization output via `cargo test <rule> -- --nocapture`; corpus YAML mismatch (sample tagged positive but rule says no, or vice versa).
- **WebApp 401 loop** → `auth_date` clock skew (server vs client); HMAC algorithm wrong (WebApp uses `key = HMAC("WebAppData", bot_token)`, Login Widget differs); JWT secret rotated without restart.
- **`<For>` not updating** → destructured props or stale signal access. Use `() => list()` not `{list()}`.
- **Migration ran but query fails** → `.sqlx/` cache is stale. `cargo sqlx prepare` to regenerate.

## Anti-patterns

- Adding `try/catch` (Rust: `Result` matching) "to make the error go away" without understanding the cause. The error was telling you something.
- Adding more logging without a hypothesis. Logs are tools to test a hypothesis, not solutions.
- "It worked before, must be the recent change" — without bisecting. Recent changes are *correlated*, not necessarily causal.
- Restarting the process as the fix. Restart cleared transient state; the bug is still there for next time.
- Committing the "fix" before confirming it actually fixes the reported case.

## Tools

- **Server logs**: `tracing` JSON logs at `logs/vixen-server.log` (7d rotation). Pipe through `jq` for structured filtering.
- **DB live state**: `pg_stat_activity` for queries in flight; `pg_locks` for blockers. Use the `postgres` MCP server (read-only).
- **Max verbosity**: `RUST_LOG=vixen=trace,sqlx=warn cargo run` — trace level for Vixen modules without drowning in sqlx noise.
- **Network**: DevTools Network tab for website fetch issues; `curl -v` for API.
- **Bisect**: `git bisect start && git bisect bad HEAD && git bisect good <known-good-sha>` then run the repro on each step.

## Output for a debug session

State the problem, the hypothesis, the test, the result. Example:

> **Problem.** `/api/chats` returns 500 in prod, fine in dev.
> **Hypothesis.** Migration `20260420_add_chat_lang.sql` ran in dev but not prod.
> **Test.** `psql -c "SELECT lang FROM chats LIMIT 1"` in prod.
> **Result.** Column missing. Confirmed.
> **Fix.** `sqlx migrate run` on prod. Verified `/api/chats` now 200.

## Related

- `code-review-self` — most "bugs" are caught by reading the diff.
- `verifiable-goal` — bugfix = failing test, then fix, then passing test.
- `verify-changes` — run the pipeline after the fix.
