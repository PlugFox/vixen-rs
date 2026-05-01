# Vixen-rs Server — LLM Rules

## Mandatory Reads

- Migrations → [docs/rules/migrations.md](docs/rules/migrations.md)
- API routes → [docs/rules/api-routes.md](docs/rules/api-routes.md)
- Telegram handlers → [docs/rules/telegram-handlers.md](docs/rules/telegram-handlers.md)
- Background jobs → [docs/rules/background-jobs.md](docs/rules/background-jobs.md)
- Error handling → [docs/rules/error-handling.md](docs/rules/error-handling.md)
- Rust style → [docs/rules/rust.md](docs/rules/rust.md)
- Tests → [docs/rules/testing.md](docs/rules/testing.md)

## Quick Reference

- Entry point: `bin/server.rs` — loads config, init tracing, starts HTTP + bot poller + jobs with shared `CancellationToken`
- HTTP router + middleware stack: `src/api/server.rs`
- Telegram dispatcher (teloxide): `src/telegram/dispatcher.rs`
- Captcha pipeline orchestrator: `src/services/captcha_service.rs`
- Spam pipeline orchestrator: `src/services/spam_service.rs`
- Background-job registry: `src/jobs/mod.rs`
- ApiResult / response macros: `src/api/response.rs`
- Config struct (clap Parser): `src/config/mod.rs`
- Env template: `config/template.env`
- Bot-token redaction newtype: `src/utils/redact.rs`

## When Modifying

| Touched | Update |
|---|---|
| API endpoint | [docs/api.md](docs/api.md) |
| DB schema | [docs/database.md](docs/database.md) + new migration |
| Auth flow | [docs/auth.md](docs/auth.md) |
| Telegram handler | [docs/bot.md](docs/bot.md) (slash-command table if applicable) |
| Captcha pipeline / assets | [docs/captcha.md](docs/captcha.md) + `assets/captcha/CHANGELOG` |
| Spam detection | [docs/spam-detection.md](docs/spam-detection.md) + corpus YAML under `tests/spam_corpus/` |
| Background job | [docs/background-jobs.md](docs/background-jobs.md) |
| `CONFIG_*` env var | [docs/config.md](docs/config.md) |

After **any** user-visible change, append to root [`CHANGELOG.md`](../CHANGELOG.md) under `[Unreleased]` with `(server)` tag, and bump patch version in `Cargo.toml`.

## Key Conventions

- All SQL queries are SQLx compile-time checked. Run `cargo sqlx prepare -- --all-targets` after schema changes; commit `.sqlx/`.
- **Telegram IDs are always `i64` in code and `BIGINT` in SQL.** Never narrow to `i32`. Negative supergroup IDs (`-100…`) require signed 64-bit.
- **Transactions are mandatory for multi-step writes.** Captcha solve = DELETE challenge + INSERT verified_user in one tx. Per-chat config update = `SELECT ... FOR UPDATE` + `UPDATE`.
- **Cache TG-API calls only with a sensible TTL** (chat info 6h, CAS verdict 1h). Never cache user state — read fresh.
- **Document REST endpoints with `#[utoipa::path(...)]`.** Telegram handlers do NOT need utoipa — they aren't HTTP.
- **Return-type discipline**: route handlers return `ApiResult<T>`; services return `Result<T, E>`; Telegram handlers return `Result<()>` (errors logged, never panic up the dispatcher).
- **Separate DB models from API DTOs.** SQLx derives on the DB side, Serde derives on the API side, `From<Db> for Dto` in between.
- **Bot token redaction.** Use `RedactedToken` from `src/utils/redact.rs` for any `tracing::*!()` call where the token is in scope. Never the raw `String`.
- **Captcha output is deterministic per challenge ID.** Re-rendering a challenge for the same `(chat_id, user_id, nonce)` MUST produce byte-identical output (modulo asset upgrades). Tests assert this.
- **Graceful shutdown for every long-running task.** Polling worker, scheduler, and HTTP server all listen on the same `CancellationToken`. No `sleep > 5s` without a `tokio::select!` against the token.
- **Spam-rule additions need corpus tests.** Every new rule has positive + negative samples under `tests/spam_corpus/<rule>.yaml`. The integration test in `tests/spam_pipeline.rs` walks the corpus.

## Validation pipeline

`/server-check` runs:

1. `cargo fmt --all -- --check`
2. `cargo clippy --all-targets --all-features -- -D warnings`
3. `cargo test`
4. `cargo sqlx prepare --check -- --all-targets`

If you skip the fourth step locally, CI will fail when `.sqlx/` drifts from the live queries.
