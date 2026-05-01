# Daily Reports

A summary of the previous 24h is posted into each watched chat at `CONFIG_REPORT_HOUR` (default 17:00). Optional AI-generated narrative summary is appended when OpenAI is configured.

## Aggregates

For each chat, the previous-24h window:

- `messages_seen` — count of `allowed_messages` rows (if `chat_config.log_allowed_messages = true`); otherwise extracted from `daily_stats(... 'message')` running counter.
- `messages_deleted` — `moderation_actions WHERE action = 'delete'`.
- `users_verified` — `moderation_actions WHERE action = 'verify'` (incl. captcha solves).
- `users_banned` — `moderation_actions WHERE action = 'ban'`.
- `captcha_attempts` — `daily_stats(... 'captcha_attempt')`.
- `top_spam_phrases` — top-5 `spam_messages` by `hit_count` updated in the window (sample text + count).

Stored as `daily_stats(chat_id, date, kind, value BIGINT)` for cheap re-aggregation.

## Chart

`plotters` renders a stacked bar chart, 24 hourly buckets:

- Bar 1: `allowed_messages` (greenish)
- Bar 2: `moderation_actions WHERE action = 'delete'` (red)
- Bar 3: `moderation_actions WHERE action = 'ban'` (dark red)
- Bar 4: captcha attempts (blue)

Output: PNG, 1200×630 (also serves as OG image for the public report).

`spawn_blocking` for the rendering — plotters is CPU work.

## Posting

```rust
let chart_png = report_service::render_chart(chat_id, &stats).await?;
let caption = report_service::format_caption(&stats, &locale);
let msg = bot.send_photo(chat_id, InputFile::memory(chart_png).file_name("report.png"))
    .caption(caption)
    .parse_mode(ParseMode::MarkdownV2)
    .await?;
sqlx::query!(
    "INSERT INTO report_messages (chat_id, telegram_message_id, generated_at, kind)
     VALUES ($1, $2, NOW(), 'daily')
     ON CONFLICT (chat_id, kind) DO UPDATE SET telegram_message_id = $2, generated_at = NOW()
     RETURNING (xmax = 0) AS inserted",
    chat_id, msg.id.0
).fetch_one(&pool).await?;
```

If a previous day's report message ID is in `report_messages`, delete it first (`bot.delete_message(chat_id, prev_msg_id)`) — keeps the chat clean. The dashboard can also trigger a "redo report" that does this on demand.

## Caption format

Locale-aware (RU / EN), MarkdownV2:

```
📊 *Vixen — Daily Report*  ({date})

Messages: {messages_seen} · Deleted: {messages_deleted} · Captcha attempts: {captcha_attempts}
Verified: {users_verified} · Banned: {users_banned}

Top users:
1. {name1} — {count1}
2. ...

(AI summary, if enabled, appended below)
```

User mentions in "Top users" are escaped properly per MarkdownV2 (the helper is in `utils/escape.rs`).

## Optional OpenAI summary

Gated by `CONFIG_OPENAI_KEY` AND `chat_config.summary_enabled = true` AND `daily_stats(... 'openai_tokens')` < `chat_config.summary_token_budget`.

Pipeline:

1. Pull last-24h `allowed_messages.content` from the chat (limit ~2000 messages).
2. **Sanitize**: strip @mentions (`@\w+`), strip phone-like sequences (`+?\d{7,}`), strip URLs (replace with `[link]`).
3. POST to OpenAI Chat Completions API (`gpt-4o-mini` default, configurable):
   ```
   model: CONFIG_OPENAI_MODEL
   messages: [
     {"role": "system", "content": "Summarize this chat into 3-5 short bullet points in {locale}. Focus on topics, not individuals."},
     {"role": "user", "content": "<sanitized messages>"}
   ]
   max_tokens: 500
   ```
4. Append the response text to the report caption (or as a separate message if it doesn't fit).
5. Increment `daily_stats(... 'openai_tokens')` by the API's reported token usage.

## Per-chat token budget

`chat_config.summary_token_budget INTEGER NOT NULL DEFAULT 50000` (per day). When today's accumulated token usage hits the budget, the summary is skipped for the rest of the day and a one-line note is added to the caption ("AI summary skipped — daily budget reached"). Resets at chat-local midnight.

This is a hard cap — there's no "soft warning" path. If the budget is too tight, moderators raise it in the dashboard.

## Scheduling

`daily_report` job per chat. Wakes up at chat-local `CONFIG_REPORT_HOUR` (NOT a single global tick — different chats can have different report hours and timezones in `chat_config.timezone`).

The job uses `chrono::Local` math + `tokio::time::sleep_until` to fire on a wall-clock schedule. `tokio::time::interval` would drift.

Idempotency: the job checks `report_messages WHERE chat_id = $1 AND kind = 'daily' AND generated_at::date = current_date` before firing. A re-run on the same day no-ops unless the `--force-redo` admin endpoint is used.

## On-demand "regenerate report"

`POST /api/v1/chats/{chat_id}/reports/regenerate` (moderator-only) deletes the current `report_messages` row and re-runs the same flow immediately. Dashboard exposes a "Redo today's report" button.

## Public report

`GET /report/{chat_slug}` and `GET /report/{chat_slug}/chart.png` are unauthenticated and serve the **same data** as the in-chat report, with usernames stripped:

- "Top users" section omitted entirely.
- Top spam phrases shown as categories, not raw text (e.g. "investment scam: 12 hits") — adds a redaction step.
- Chart and counts unchanged.

`chat_slug` is derived from `chats.slug` (a short URL-safe identifier set per-chat by a moderator). Chats without a slug have no public report.

## Failure modes

| Failure | Effect | Recovery |
|---|---|---|
| Aggregation query slow | Job tick goes long | Add an index, partition `daily_stats` by date if it grows |
| `bot.send_photo` fails (Telegram down) | No message posted, no `report_messages` row written | Job re-tries on next day's tick; missed day shows nothing |
| OpenAI API errors / timeouts | Summary skipped, caption noted | Token budget unchanged; report still posts |
| OpenAI budget exhausted mid-day | Summary skipped | Dashboard surfaces budget usage; moderator raises if needed |

## Related

- Service: `src/services/report_service.rs` + `src/services/summary_service.rs`
- Job: `src/jobs/{daily_report,summary_generation}.rs`
- Routes: `src/api/routes_reports.rs` (auth) + `src/api/routes_public.rs` (public)
- Dashboard: `website/src/features/reports/`
- Public page: `website/src/pages/public-report.tsx`
