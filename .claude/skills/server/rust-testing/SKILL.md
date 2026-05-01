---
name: rust-testing
description: Write Rust tests for the vixen-rs server — unit, integration, SQLx tests with per-test isolated DBs, teloxide-tests for bot handlers, and mocks only at true system boundaries. Use when adding tests, fixtures, or when the user asks to "test X" or "add a test for Y".
---

# Rust Testing (Vixen server)

**Source:** adapted from [SQLx testing docs](https://docs.rs/sqlx/latest/sqlx/attr.test.html), [Tokio test docs](https://docs.rs/tokio/latest/tokio/attr.test.html), and [teloxide-tests](https://docs.rs/teloxide-tests).

## Where tests live

- **Unit tests**: `#[cfg(test)] mod tests { ... }` at the bottom of the file under test. Test private fns and small pure helpers (xxhash normalization, captcha digit generation, n-gram extraction).
- **Integration tests**: `server/tests/*.rs`. Each file is a separate crate — import the server as a library (`use vixen_server::...`).
- **DB-touching tests**: integration tests with `#[sqlx::test]`.
- **Bot-handler tests**: integration tests using `teloxide-tests` mock bot.

## Async tests

```rust
#[tokio::test]
async fn handles_ok() { /* ... */ }

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn needs_multi_thread() { /* ... */ }
```

Default is single-threaded — fine for 99% of cases.

## SQLx tests

`#[sqlx::test]` auto-creates an isolated database per test, runs migrations from `server/migrations/`, and injects a `PgPool`:

```rust
#[sqlx::test]
async fn verifies_user_atomically(pool: PgPool) -> sqlx::Result<()> {
    let chat_id = 42_i64;
    let user_id = 1001_i64;
    issue_challenge(&pool, chat_id, user_id, "1234").await?;
    solve_challenge(&pool, chat_id, user_id, "1234").await?;

    let verified: bool = sqlx::query_scalar!(
        "SELECT EXISTS(SELECT 1 FROM verified_users WHERE chat_id = $1 AND user_id = $2)",
        chat_id, user_id
    ).fetch_one(&pool).await?;
    assert!(verified);
    Ok(())
}
```

- **No shared global state between tests** — each gets a fresh DB. Don't hand-roll `TRUNCATE` or per-test cleanup.
- **Don't mock SQLx.** If a test touches DB, it hits a real Postgres.
- **Fixtures**: `#[sqlx::test(fixtures("watched_chat"))]` loads `server/tests/fixtures/watched_chat.sql` etc. Keep fixtures small and per-file.
- `#[sqlx::test]` gives a fresh per-test DB; combine with file-local fixtures: `#[sqlx::test(fixtures("watched_chat", "moderator"))]` auto-loads `tests/fixtures/watched_chat.sql` + `tests/fixtures/moderator.sql` before the body.

## Spam-rule corpus tests

Every spam rule MUST have a corpus YAML at `server/tests/spam_corpus/<rule>.yaml` with positive and negative samples. The integration test in `server/tests/spam_pipeline.rs` walks the corpus and asserts the verdict:

```yaml
# server/tests/spam_corpus/cas_lookalike.yaml
positives:
  - "Заработай $1000 в день, пиши в личку @scammer"
  - "💰💰💰 EASY MONEY 💰💰💰 dm now"
negatives:
  - "Сегодня в офисе купили торт"
  - "Проверьте мой PR пожалуйста"
```

When adding a new rule, add at least 5 positive + 5 negative samples. Maintain the YAML at `server/tests/spam_corpus/<rule>.yaml` with ≥5 positive + ≥5 negative samples — the pipeline test walks the corpus and asserts the verdict for each entry. Without the corpus the rule silently rots when the normalize pipeline changes.

## Telegram-handler tests

Use `teloxide-tests::MockBot`:

```rust
let bot = MockBot::new(vec![]);
bot.dispatch_and_check_last_text("hello").await;
```

Verify handler side-effects via the mocked bot's recorded API calls + DB state. Avoid asserting on internal Telegram message IDs (they're synthetic).

## Mocking policy

Mock only at true system boundaries:

- **HTTP clients to external services** (Combot CAS, OpenAI) — use `wiremock` or an injected trait.
- **Telegram Bot API** — `teloxide-tests::MockBot`.
- **Time** — `tokio::time::pause()` + `tokio::time::advance()` for timing-sensitive logic.
- **Randomness** — inject a seeded RNG (especially for captcha digit generation).

Do **not** mock:

- SQLx queries.
- Internal services (CaptchaService, SpamService) — test them via their real impls with test DB.
- `serde_json`, UUID, filesystem inside `tempdir()`.

## HTTP handler tests

Use `axum::Router` + `tower::ServiceExt::oneshot`:

```rust
let app = build_router(state);
let res = app.oneshot(
    Request::builder().uri("/api/v1/chats").method("GET")
        .header("authorization", format!("Bearer {}", test_jwt))
        .body(Body::empty())?
).await?;
assert_eq!(res.status(), StatusCode::OK);
```

Test JWTs come from a `mock_init_data(user_id, bot_token, ts)` helper + the real auth pipeline — not hand-crafted.

## Common mistakes

- Testing implementation details instead of public contract.
- Tests that pass locally and fail in CI — usually due to ordering assumptions (sort results before comparing) or wall-clock time (use `pause()`).
- Forgetting `-- --nocapture` when you want `println!`/`dbg!` output during test runs.
- `#[tokio::test]` with `block_on` inside — redundant.

## Running

- `cargo test` — all tests.
- `cargo test --lib` — only unit tests (no DB needed).
- `cargo test <name>` — filter by name substring.

## Related

- `/server-check` runs `cargo test` as part of the pre-commit pipeline.
- DB schema: [server/docs/database.md](../../../../server/docs/database.md).
- Errors: see `rust-error-handling` skill.
