# Observability

Tracing-driven. Two sinks: human-readable console + machine-readable JSON file with daily rotation. Metrics endpoint is planned for after M6.

## Setup

`src/telemetry/mod.rs::init(&config)`:

```rust
let console_layer = tracing_subscriber::fmt::layer()
    .with_target(false)
    .with_writer(std::io::stdout)
    .with_filter(EnvFilter::new(&config.log_level));

let file_appender = tracing_appender::rolling::daily(&config.log_dir, "vixen-server.log");
let (file_writer, _guard) = tracing_appender::non_blocking(file_appender);
let json_layer = tracing_subscriber::fmt::layer()
    .json()
    .with_writer(file_writer)
    .with_filter(EnvFilter::new("info"));

tracing_subscriber::registry().with(console_layer).with(json_layer).init();
```

The `_guard` is held for the lifetime of the process — drop it and the file writer flushes/exits.

## Span conventions

Every meaningful unit of work has a span. The fields are stable so prod-log queries don't drift:

### HTTP requests

```
span!("request", method, route, request_id, user_id?)
  └─ event: "request received"
  └─ event: "service call complete" (per service)
  └─ event: "request completed" (status, latency_ms)
```

The `request_id` is generated in `request_id_middleware` (UUIDv4) and propagated to every nested span.

### Telegram updates

```
span!("update", update_id, chat_id, user_id?)
  └─ event: "handler matched" (handler_name)
  └─ event: "service call complete"
  └─ event: "handler completed" (latency_ms)
```

`update_id` lets you correlate the bot's view of an update with Telegram's.

### Background jobs

```
span!("job", name)
  └─ event: "iteration started" (now)
  └─ event: "iteration failed" (level=warn, error)
  └─ event: "iteration completed" (latency_ms)
```

Grep `job=daily_report` to see all daily-report runs.

### External API calls

```
span!("cas", user_id)
span!("openai", chat_id, model)
span!("bot_api", method)
```

Each carries `latency_ms` on completion. Outage diagnosis = `cas` + `error` + `latency_ms > $timeout`.

## Redaction

Two layers:

- **Owning newtypes** in `src/config/secrets.rs` — `BotToken`, `JwtSecret`, `AdminSecret`, `OpenAiKey`. Their `Display`/`Debug` always prints `***redacted***` (no public correlator). Use whenever the value is held in a struct (`Config`, service state).
- **Borrowed wrapper** in `src/utils/redact.rs::RedactedToken<'a>(pub &'a str)` — preserves the bot-id correlator before the colon so log lines stay searchable. Use at tracing call sites where the raw `&str` is in scope (e.g. inside `Bot::new(token.expose())` paths).

```rust
// Borrowed wrapper: keeps the searchable id, redacts the secret half.
pub struct RedactedToken<'a>(pub &'a str);

impl std::fmt::Display for RedactedToken<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.0.split_once(':') {
            Some((id, _)) if !id.is_empty() => write!(f, "{id}:****"),
            _ => f.write_str("****"),
        }
    }
}
```

Use in any `tracing` field: `tracing::info!(bot = %RedactedToken(raw), "starting poller")` → logs `bot=12345:****`. For owning struct fields: `tracing::info!(bot = %config.bot_token, ...)` → logs `bot=***redacted***` (no correlator, but safe).

`RedactedJwt` (logs `eyJh***`) and `RedactedInitData` (logs `len=...,user_id=...`) follow the same borrowed-wrapper pattern; populated from M2 onwards.

## What NOT to log at info+

- Raw bot token, raw JWT, raw `initData` — only at `debug` (and even then, prefer the redacted variant).
- Raw message bodies (chat content) — opt-in only via `chat_config.log_allowed_messages`, and even then never at info+.
- User PII fields (`first_name`, `last_name`, `username`, phone, email) — only at debug for diagnosis.
- SQL query strings with bound parameters — leaks user-supplied content.

CI grep would catch the obvious cases; `.claude/skills/server/review-pr/SKILL.md` calls these out as a P0 review blocker.

## Log structure (JSON sink)

Each line:

```json
{
  "timestamp": "2026-05-01T17:00:00.123Z",
  "level": "INFO",
  "fields": {"message": "request completed", "status": 200, "latency_ms": 12},
  "target": "vixen_server::api",
  "span": {"name": "request", "method": "GET", "route": "/api/v1/chats", "request_id": "...", "user_id": 12345}
}
```

Pipe through `jq` for ad-hoc analysis:

```bash
jq -r 'select(.span.name=="job" and .span.fields.name=="daily_report") | "\(.timestamp) \(.fields.message) chat_id=\(.span.fields.chat_id // "n/a")"' logs/vixen-server.log.2026-05-01
```

## Metrics (planned, not in M0)

Will expose `/admin/metrics` for Prometheus scraping. Counters / gauges:

- `vixen_http_requests_total{route, status}`
- `vixen_http_request_duration_seconds{route}`
- `vixen_bot_updates_total{chat_id, kind}`
- `vixen_bot_handler_duration_seconds{handler}`
- `vixen_jobs_runs_total{name, outcome}`
- `vixen_jobs_last_success_timestamp_seconds{name}`
- `vixen_db_pool_acquire_duration_seconds`
- `vixen_db_connections_in_use`
- `vixen_cas_requests_total{outcome}`
- `vixen_openai_tokens_total{chat_id}`

Gated by `CONFIG_METRICS_ENABLED` (default `false` in dev, `true` in prod). Behind `admin_secret_middleware` so it's not publicly scrapable.

## Health endpoint

`GET /health`:

```json
{ "status": "ok", "checks": { "db": "ok", "redis": "ok" } }
```

Returns 200 if every probe (Postgres acquire + `SELECT 1`, Redis `PING`) succeeds, 503 with `"status":"degraded"` and the failing component flipped to `"down"` otherwise. Used by load balancer / docker compose `condition: service_healthy`.

## Tracing in tests

Tests use `tracing_subscriber::fmt::try_init()` with a level controlled by `RUST_LOG`. Default is `error` to keep test output clean; set `RUST_LOG=info,vixen_server=debug` to see what the test is actually doing.

`tokio::test` doesn't share state across tests, so re-init is safe (the `try_init` no-ops on second call within the same process — fine for cargo test's per-test sub-process model).
