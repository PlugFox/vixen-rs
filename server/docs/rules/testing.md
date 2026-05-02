# Testing Rules

Read this file before adding tests.

## Three layers

| Layer | Where | When to use |
|---|---|---|
| **Unit** | `#[cfg(test)] mod tests { ... }` at the bottom of the source file | Pure functions: xxhash normalization, captcha digit generation, n-gram extraction, redaction helpers. No I/O. |
| **Integration** | `server/tests/*.rs`, each file is a separate test crate (`use vixen_server::*;`) | Anything that hits Postgres or the bot mock. |
| **End-to-end (golden flows)** | `server/tests/bot_flows.rs` (and similar) using `teloxide-tests::MockBot` | A handful of full-pipeline scenarios: new user joins → captcha → solves → verified; spam dedup catches a duplicate; daily report fires. |

## Async tests

```rust
#[tokio::test]
async fn handles_ok() { /* ... */ }

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn needs_multi_thread() { /* ... */ }
```

Default single-threaded is fine for 99% of cases.

## SQLx tests

Use `#[sqlx::test]` — auto-creates an isolated database per test, runs migrations, injects `PgPool`:

```rust
#[sqlx::test]
async fn verifies_user_atomically(pool: PgPool) -> sqlx::Result<()> {
    let chat_id = -100123_i64;   // negative supergroup ID
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

- **No shared global state between tests** — each gets a fresh DB.
- **Don't mock SQLx.** Hit real Postgres. The cost of a per-test DB is negligible compared to the cost of a mocked-but-broken query in prod.
- **Fixtures**: `#[sqlx::test(fixtures("watched_chat"))]` loads `server/tests/fixtures/watched_chat.sql` etc. Keep fixtures small and per-purpose.

## Spam-rule corpus tests

Every spam rule MUST have a corpus YAML at `server/tests/spam_corpus/<rule>.yaml` with positive AND negative samples. Minimum 5 of each.

```yaml
# server/tests/spam_corpus/cas_lookalike.yaml
positives:
  - "Заработай $1000 в день, пиши в личку @scammer"
  - "💰💰💰 EASY MONEY 💰💰💰 dm now"
  - "крипта легко зарабатывать сейчас гарантия"
  - "Make $$$ from home no skills required"
  - "хочешь работать удаленно? пиши в лс"

negatives:
  - "Сегодня в офисе купили торт"
  - "Проверьте мой PR пожалуйста"
  - "Слышал у вас дома собака?"
  - "В пятницу созвон в 18:00"
  - "Где такую футболку купить можно?"
```

The integration test in `tests/spam_pipeline.rs` walks the corpus and asserts the verdict. Adding a new rule without corpus tests is a P0 review block.

## Telegram-handler tests

Use `teloxide-tests::MockBot`:

```rust
let bot = MockBot::new(vec![]);
bot.dispatch_and_check_last_text(t("captcha.solve.success")).await;
```

Verify side-effects via the mock's recorded API calls + DB state. Avoid asserting on internal Telegram message IDs (they're synthetic).

## Mocking policy

Mock only at true system boundaries:

- **HTTP clients to external services** (CAS, OpenAI) — `wiremock` or an injected trait.
- **Telegram Bot API** — `teloxide-tests::MockBot`.
- **Time** — `tokio::time::pause()` + `tokio::time::advance()` for time-sensitive logic (captcha expiry, report scheduling).
- **Randomness** — inject a seeded RNG (especially for captcha digit generation; the deterministic property is testable).

Do **not** mock:

- SQLx queries.
- Internal services (CaptchaService, SpamService, ModerationService) — test them via real impls with `PgPool`.

## HTTP handler tests

```rust
let app = build_router(state);
let res = app.oneshot(
    Request::builder().uri("/api/v1/chats").method("GET")
        .header("authorization", format!("Bearer {}", test_jwt))
        .body(Body::empty())?
).await?;
assert_eq!(res.status(), StatusCode::OK);
```

Test JWTs come from a `mock_init_data(user_id, bot_token, ts)` helper + the real auth pipeline. Hand-crafted JWTs that bypass HMAC validation are forbidden — they would let the test pass while the real auth code is broken.

## Coverage

Not enforced in CI. Treat as informational. Aim for unit coverage of pure logic (normalization, redaction, captcha generation) and integration coverage of the pipelines (captcha, spam, moderation, reports). Don't chase coverage for getter/setter glue.

## Naming

`test_<module>_<scenario>_<expected>`:

- `test_captcha_solve_correct_marks_verified`
- `test_spam_dedup_catches_identical_text`
- `test_init_data_validation_rejects_expired_auth_date`

## Running

- `cargo test` — all tests.
- `cargo test --lib` — only unit tests (no DB needed; faster local feedback).
- `cargo test <name>` — filter by name substring.
- `cargo test -- --nocapture` — show `println!` / `dbg!` output.
- `cargo test -- --test-threads=1` — serialize if you suspect a flake; fix the flake, don't make this default.

## Reacting to a failing test

A red test is, by default, a real defect in the code under test — **not** an outdated test that needs to be "brought in line" with the current implementation.

1. Reproduce the failure locally, read the actual assertion, understand what behavior is being checked.
2. Decide which side is wrong. Default assumption: the code is wrong. Declaring the test wrong requires a concrete reason (the requirement changed, the fixture was always invalid, the assertion measures something untestable).
3. Code wrong → fix the code, keep the assertion. Test genuinely wrong → fix the test AND record in the commit message why the old assertion no longer holds.
4. **Forbidden**: weakening an assertion, removing a case, widening a tolerance, mocking out the failing path, commenting out a check, or marking the test `#[ignore]` to make CI green. If a test truly must be paused, open a tracking issue, link it from the `#[ignore]` attribute, and treat it as a bug — not a fix.
5. Same rule for `cargo clippy` / `cargo check` failures — fix the cause; do not paper over with `#[allow(...)]`, `unwrap()`, or type casts.

## Common mistakes

- Tests that pass locally and fail in CI — usually wall-clock assumptions (use `pause()`) or ordering assumptions (sort before comparing).
- `#[tokio::test]` with `block_on` inside — redundant.
- Forgetting `Result<(), Error>` return on integration tests — `?` doesn't propagate from a `()` test.
- Hand-crafted JWTs / signed initData with garbage signatures — defeats the test's purpose.
- Snapshot tests for captcha image bytes — they're huge and review-noisy. Test that `render(challenge_id)` is deterministic and that the solved digits decode correctly; don't pixel-diff the asset.
