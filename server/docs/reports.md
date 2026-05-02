# Daily Reports (M3)

Each watched chat receives a daily report at the configured hour: a MarkdownV2 message with pseudographics + counts + top phrases, plus a WebP chart message with an optional AI summary as caption.

The report is **per-chat**: every tunable lives in `chat_config` and can differ across chats.

## Pipeline

```
daily_report job (5-min ticker)
  ├─► fetch chat_config (report_hour, timezone, report_min_activity, language, summary_enabled)
  ├─► is current chat-local time within ±5min of report_hour ? continue : skip
  ├─► already_posted_today(chat_id, report_date) ? skip
  ├─► report_service.aggregate(chat_id, day_window_utc(report_date))
  │     ├─► daily_stats SUM (messages_seen, captcha_*, openai_tokens_used)
  │     ├─► moderation_actions COUNT (delete, ban, verify) for the window
  │     ├─► spam_messages ORDER BY hit_count DESC LIMIT 10  (top phrases)
  │     └─► daily_stats[messages_seen] for last 7 days       (sparkline)
  ├─► report.messages_seen < min_activity ? skip
  ├─► delete prior report_messages (best-effort) + DROP rows for today
  ├─► bot.send_message(MarkdownV2)               → record(daily_text)
  └─► bot.send_photo(WebP)
        caption = summary_service.summarize(...) if summary_enabled, else None
        record(daily_photo)
```

## Aggregator (`services/report_service.rs`)

`aggregate(chat_id, from, to) -> ReportData` — pure-DB, one SQL per metric, no N+1.

- `messages_seen`, `captcha_{issued,solved,expired}` — `SUM(value)` on `daily_stats(chat_id, date, kind)`.
- `messages_deleted`, `users_banned`, `users_verified` — `COUNT(*)` on `moderation_actions` keyed by `(chat_id, action, [from, to))`.
- `top_phrases` — `spam_messages` joined to the window via `last_seen`, ordered by `hit_count DESC, last_seen DESC`, limit 10.
- `last_7_days_messages` — last 7 calendar days of `messages_seen`, oldest first, missing days zero-padded.

The aggregator also resolves `chat_title` from `chat_info_cache`.

> Note: `top_phrases` is **global** — `spam_messages` is xxh3-keyed, not chat-scoped. Until M6 ships the redaction helper, the renderer emits raw match samples. The audit-only `moderation_actions.reason` JSON keeps full detail.

## Renderer (`services/report_render.rs`)

`render(report, lang, header) -> String` — pure function, MarkdownV2-escaped output. Sections:

- **Header** — chat title (escaped) + ISO-ish UTC window.
- **Counts block** — `Сообщений / Удалено / Верифицировано / Забанено`, each with an inline 7-cell bar (8-step Unicode blocks `▁▂▃▄▅▆▇█`).
- **Captcha block** — issued / solved / expired (omitted when total = 0).
- **Top phrases** — rendered iff non-empty, samples truncated to 60 chars.
- **7-day sparkline** — one line of block characters, day-of-week row underneath. Omitted when the entire week is zero.

`HeaderKind` switches the title:

| Kind | Title (RU) | Used by |
|---|---|---|
| `Daily` | "Ежедневный отчёт" | `daily_report` job |
| `Last24h` | "Сводка за 24 часа" | `/stats` |
| `OnDemand` | "Отчёт по запросу" | `/report` |

`Lang::{Ru, En}` chooses the locale; `chat_config.language` is the source.

A worked sample is in [`reports/sample.md`](reports/sample.md).

## Chart (`services/chart_service.rs`)

`render(report) -> Vec<u8>` — 480×270 lossless WebP, same geometry as the M1 captcha so Telegram doesn't tap-to-expand. Two stacked panels:

1. **Counts** — bars for messages / deleted / verified / banned / captcha solved / expired.
2. **Last 7 days** — daily messages_seen.

Plotters' `BitMapBackend` writes RGB into an in-memory buffer; the buffer is encoded with `image::codecs::webp::WebPEncoder::new_lossless`. CPU-bound — callers wrap `render` in `spawn_blocking`. The bundled DejaVuSans is registered with plotters' `ab_glyph` backend at first call so charts render identically in dev, CI, and Docker without relying on system fonts.

Asserts `bytes.len() <= 80 KiB` in the test harness; in practice we see a few KB.

## Summary (`services/summary_service.rs`)

`summarize(chat_id, from, to, language) -> SummaryOutcome::{Generated{text, tokens_used} | Skipped{reason}}`.

Per-chat:
- `chat_config.openai_api_key` — NULL → `Skipped(NoApiKey)`.
- `chat_config.summary_enabled` — FALSE → `Skipped(Disabled)`.
- `chat_config.openai_model` — defaults to `gpt-4o-mini`.
- `chat_config.summary_token_budget` — when `daily_stats('openai_tokens_used')` for today ≥ budget, → `Skipped(BudgetExhausted{used, budget})`.
- `allowed_messages` — empty → `Skipped(NoMessages)`. Filled only when `chat_config.log_allowed_messages = TRUE`.

Sanitisation runs on every message body before the OpenAI POST:
- `https?://...` → `[link]`
- emails → `[email]`
- `+?\d[\d\s().-]{6,}\d` → `[phone]`
- `@username` → `[user]`

The OpenAI client (`services/openai_client.rs`) retries on 429 / 5xx with exponential backoff (initial 500 ms, doubled per attempt; up to 3 attempts). A `Retry-After` header in seconds overrides the backoff (capped at 60s). Total token usage is recorded into `daily_stats('openai_tokens_used')` on success.

## Replace-on-redo

`report_messages` schema:

```sql
CREATE TABLE report_messages (
    chat_id             BIGINT      NOT NULL REFERENCES chats(chat_id) ON DELETE CASCADE,
    report_date         DATE        NOT NULL,
    kind                TEXT        NOT NULL CHECK (kind IN ('daily_text', 'daily_photo')),
    telegram_message_id INTEGER     NOT NULL,
    generated_at        TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (chat_id, report_date, kind)
);
```

Re-running on the same `report_date` is the dashboard "redo report" path:

1. `prior_today(chat_id, report_date)` returns the existing pair.
2. Bot `delete_message` each (best-effort — failure on either is logged).
3. `delete_for_day` clears the rows.
4. Send fresh text + photo, `record` each.

This keeps the chat free of historical "v1, v2, v3" pairs and matches the
moderator's mental model of "today's report".

## /stats / /report / /summary

Slash commands (`telegram/handlers/commands.rs`):

| Command | Window | Cooldown | Posts | Notes |
|---|---|---|---|---|
| `/stats`   | last 24h | 60s | text | moderator-only |
| `/report`  | today (chat-local) | — | text + chart + summary caption | moderator-only; replaces today's prior pair |
| `/summary` | last 24h | 60s | text | moderator-only; replies with skip reason if OpenAI key missing or disabled |

Cooldowns use `SET NX EX` on Redis key `cmd:{stats,summary}:{chat_id}`.

## Failure modes

| Failure | Effect | Recovery |
|---|---|---|
| `bot.send_message` fails | report_messages row not written → next 5-min tick re-fires | Idempotent by design |
| `bot.send_photo` fails after text sent | Text stays, photo retried next tick (text replaced) | Idempotent by `(chat_id, report_date, kind)` UPSERT |
| OpenAI 429 / 5xx within retry budget | Backoff + retry up to 3 attempts, `Retry-After` honoured | Surfaces error if exhausted; report still posts without summary caption |
| OpenAI budget exhausted mid-day | `Skipped(BudgetExhausted)`; report posts without caption | Moderator raises `chat_config.summary_token_budget` |
| Invalid `chat_config.timezone` | Job logs `warn` and skips that chat for the tick | Fix the IANA string in chat_config |

## Related

- Service: [`src/services/report_service.rs`](../src/services/report_service.rs)
- Renderer: [`src/services/report_render.rs`](../src/services/report_render.rs)
- Chart: [`src/services/chart_service.rs`](../src/services/chart_service.rs)
- Summary: [`src/services/summary_service.rs`](../src/services/summary_service.rs) + [`src/services/openai_client.rs`](../src/services/openai_client.rs)
- Job: [`src/jobs/daily_report.rs`](../src/jobs/daily_report.rs) — see also [`docs/rules/background-jobs.md`](rules/background-jobs.md)
- Models: [`src/models/daily_stats.rs`](../src/models/daily_stats.rs), [`src/models/report_message.rs`](../src/models/report_message.rs), [`src/models/report.rs`](../src/models/report.rs)
- Sample output: [`reports/sample.md`](reports/sample.md)
