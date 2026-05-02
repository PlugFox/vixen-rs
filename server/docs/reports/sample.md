# Sample report output

This is the literal MarkdownV2 string that `report_render::render(...)` emits for a representative `ReportData`. Re-render via:

```rust
use vixen_server::models::report::{CaptchaCounts, DailyPoint, ReportData, TopPhrase};
use vixen_server::services::report_render::{render, HeaderKind, Lang};
use chrono::{NaiveDate, TimeZone, Utc, Duration};

let from = Utc.with_ymd_and_hms(2026, 5, 1, 17, 0, 0).unwrap();
let to   = Utc.with_ymd_and_hms(2026, 5, 2, 17, 0, 0).unwrap();
let start = NaiveDate::from_ymd_opt(2026, 4, 26).unwrap();
let last_7 = (0..7).map(|i: i64| DailyPoint {
    date: start + Duration::days(i),
    messages: [120, 180, 0, 320, 250, 90, 1240][i as usize],
}).collect();

let r = ReportData {
    chat_id: -1001234567890, from, to,
    chat_title: Some("Vixen test chat".into()),
    messages_seen: 1240, messages_deleted: 12,
    users_verified: 8, users_banned: 3,
    captcha: CaptchaCounts { issued: 11, solved: 8, expired: 3 },
    top_phrases: vec![
        TopPhrase { text: "buy now".into(),    hits: 7 },
        TopPhrase { text: "click here".into(), hits: 5 },
        TopPhrase { text: "best price".into(), hits: 3 },
    ],
    last_7_days_messages: last_7,
};
print!("{}", render(&r, Lang::Ru, HeaderKind::Daily));
```

## Raw MarkdownV2 (RU)

The block below is fenced with `~~~` so the inner triple-backtick fences in
the rendered MarkdownV2 (which Telegram uses for monospace blocks) survive
intact:

~~~
📊 *Ежедневный отчёт*
Vixen test chat
2026\-05\-01 17:00 → 2026\-05\-02 17:00 UTC

```
Сообщений       ███████  1240
Удалено         ▁        12
Верифицировано           8
Забанено                 3
```
*Капча*
```
Выдано   ███████  11
Решено   █████▁   8
Истекло  █▇       3
```
*Частые фразы*
▌  7× «buy now»
▌  5× «click here»
▌  3× «best price»

*7 дней*
```
▁▁ ▂▂▁█
SMTWTFS
```
~~~

## How Telegram renders it

- `📊 *Ежедневный отчёт*` — bold header.
- Counts and captcha blocks render in monospace via the fenced ```` ``` ```` blocks; the inline 8-step Unicode bars sit on the right and align by column.
- `*Частые фразы*` — bold; each line `▌  Nx «...»` reads as a quote-block visual without using MarkdownV2's `>` prefix (which would be quoted line-block style).
- `*7 дней*` — bold header above a 7-cell sparkline; the row underneath is a single-letter weekday code (M-T-W-T-F-S-S) so a flat day still anchors to a date.

Every dash, dot, and parenthesis is backslash-escaped (`\-`, `\.`, `\(`, `\)`) so MarkdownV2 doesn't interpret them as special.

## Width

The string above fits a phone screen without wrapping (≤ 32 monospace columns in the counts block; sparkline is 7 columns). Top phrases truncate to 60 visible chars + `…`.
