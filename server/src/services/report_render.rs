//! MarkdownV2 renderer for [`ReportData`]. Pure function of the input.
//!
//! Output shape (per issue M3-02):
//!
//!   * Header — chat title (escaped) + date range.
//!   * Counts block — one line per metric, scaled bar in 8-step Unicode
//!     blocks (`▁▂▃▄▅▆▇█`).
//!   * Captcha block — issued / solved / expired (only if any > 0).
//!   * Moderation block — bans / deletes (only if any > 0).
//!   * Top phrases — escaped, truncated to 60 chars + `…`.
//!   * Sparkline — last-7-days messages, one row of block characters.
//!
//! Every section that produces zero data is omitted entirely so a quiet day
//! shrinks gracefully instead of emitting "0 / 0 / 0" rows.
//!
//! All user-derived strings (chat title, top-phrase samples) are escaped per
//! Telegram's MarkdownV2 spec — `_*[]()~\`>#+-=|{}.!` are the special set.
//! Numeric counters and dates are formatted by us, but we still pass them
//! through the escape helper because `.` is in the special set and a number
//! like `1.234` would otherwise need extra care.

use chrono::{DateTime, Datelike, Utc};

use crate::models::report::{DailyPoint, ReportData, TopPhrase};

/// Maximum top-phrase sample length in the rendered message. Long enough to
/// be informative, short enough that ten of them plus the rest of the report
/// stays under Telegram's 4096-char body limit.
const TOP_PHRASE_MAX_CHARS: usize = 60;

/// Width (in cells) of the in-line counts bar. Picked so that the longest
/// line in the counts block — Russian "Удалено" plus a 5-digit count plus
/// the bar — comfortably fits a phone-screen width without wrapping.
const COUNTS_BAR_WIDTH: usize = 7;

const BLOCK_CHARS: [char; 9] = [' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Ru,
    En,
}

impl Lang {
    pub fn from_db_str(s: &str) -> Self {
        match s {
            "en" => Self::En,
            _ => Self::Ru,
        }
    }
}

/// Drives the header banner. `Daily` for the scheduled report, `Today`
/// for `/stats` (chat-local day so far), `OnDemand` for `/report`. Affects
/// the title line only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeaderKind {
    Daily,
    Today,
    OnDemand,
}

/// Render `report` into a MarkdownV2-escaped message body. The result is
/// safe to pass directly to `bot.send_message(...).parse_mode(MarkdownV2)`
/// — every user-derived character is escaped.
pub fn render(report: &ReportData, lang: Lang, header: HeaderKind) -> String {
    let mut out = String::with_capacity(1024);
    push_header(&mut out, report, lang, header);
    push_counts(&mut out, report, lang);
    push_captcha(&mut out, report, lang);
    push_moderation(&mut out, report, lang);
    push_top_phrases(&mut out, report, lang);
    push_sparkline(&mut out, report, lang);
    // Trim a trailing newline if push_* left one — Telegram strips them but
    // it makes the snapshot tests slightly cleaner.
    while out.ends_with('\n') {
        out.pop();
    }
    out
}

fn push_header(out: &mut String, report: &ReportData, lang: Lang, header: HeaderKind) {
    let title = match (lang, header) {
        (Lang::Ru, HeaderKind::Daily) => "📊 *Ежедневный отчёт*",
        (Lang::Ru, HeaderKind::Today) => "📊 *Сводка за сегодня*",
        (Lang::Ru, HeaderKind::OnDemand) => "📊 *Отчёт по запросу*",
        (Lang::En, HeaderKind::Daily) => "📊 *Daily report*",
        (Lang::En, HeaderKind::Today) => "📊 *Today's snapshot*",
        (Lang::En, HeaderKind::OnDemand) => "📊 *On-demand report*",
    };
    out.push_str(title);
    out.push('\n');

    if let Some(t) = &report.chat_title {
        out.push_str(&escape(t));
        out.push('\n');
    }

    out.push_str(&escape(&format_window(report.from, report.to)));
    out.push_str("\n\n");
}

fn push_counts(out: &mut String, report: &ReportData, lang: Lang) {
    let labels = match lang {
        Lang::Ru => ["Сообщений", "Удалено", "Верифицировано", "Забанено"],
        Lang::En => ["Messages", "Deleted", "Verified", "Banned"],
    };
    let values = [
        report.messages_seen,
        report.messages_deleted,
        report.users_verified,
        report.users_banned,
    ];
    let max = values.iter().copied().max().unwrap_or(0).max(1);

    let label_pad = labels.iter().map(|l| visual_width(l)).max().unwrap_or(0);

    out.push_str("```\n");
    for (label, &value) in labels.iter().zip(values.iter()) {
        let bar = ascii_bar(value, max, COUNTS_BAR_WIDTH);
        let pad = " ".repeat(label_pad.saturating_sub(visual_width(label)));
        // Inside a fenced ```code block``` MarkdownV2 only requires escaping
        // backticks and backslashes. Labels are static; values are integers.
        out.push_str(&format!("{label}{pad}  {bar}  {value}\n"));
    }
    out.push_str("```\n");
}

fn push_captcha(out: &mut String, report: &ReportData, lang: Lang) {
    if report.captcha.total() == 0 {
        return;
    }
    let header = match lang {
        Lang::Ru => "*Капча*",
        Lang::En => "*Captcha*",
    };
    let labels = match lang {
        Lang::Ru => ["Выдано", "Решено", "Истекло"],
        Lang::En => ["Issued", "Solved", "Expired"],
    };
    out.push_str(header);
    out.push('\n');
    out.push_str("```\n");
    let values = [
        report.captcha.issued,
        report.captcha.solved,
        report.captcha.expired,
    ];
    let max = values.iter().copied().max().unwrap_or(0).max(1);
    let label_pad = labels.iter().map(|l| visual_width(l)).max().unwrap_or(0);
    for (label, &value) in labels.iter().zip(values.iter()) {
        let bar = ascii_bar(value, max, COUNTS_BAR_WIDTH);
        let pad = " ".repeat(label_pad.saturating_sub(visual_width(label)));
        out.push_str(&format!("{label}{pad}  {bar}  {value}\n"));
    }
    out.push_str("```\n");
}

fn push_moderation(out: &mut String, report: &ReportData, lang: Lang) {
    if report.users_banned == 0 && report.messages_deleted == 0 {
        return;
    }
    let _ = (out, report, lang);
    // The counts block already covers bans + deletes; this section becomes
    // material in M4 when reasons / actor breakdown is added. Stub out for
    // now — emitting an empty header would just be visual noise.
}

fn push_top_phrases(out: &mut String, report: &ReportData, lang: Lang) {
    if report.top_phrases.is_empty() {
        return;
    }
    let header = match lang {
        Lang::Ru => "*Частые фразы*",
        Lang::En => "*Top phrases*",
    };
    out.push_str(header);
    out.push('\n');
    for TopPhrase { text, hits } in &report.top_phrases {
        let truncated = truncate_chars(text, TOP_PHRASE_MAX_CHARS);
        let single_line = truncated.replace('\n', " ");
        out.push_str(&format!(
            "▌  {hits}× «{text}»\n",
            hits = escape(&hits.to_string()),
            text = escape(&single_line),
        ));
    }
    out.push('\n');
}

fn push_sparkline(out: &mut String, report: &ReportData, lang: Lang) {
    let header = match lang {
        Lang::Ru => "*7 дней*",
        Lang::En => "*Last 7 days*",
    };
    let max = report
        .last_7_days_messages
        .iter()
        .map(|p| p.messages)
        .max()
        .unwrap_or(0);
    if max == 0 {
        return;
    }
    out.push_str(header);
    out.push('\n');
    out.push_str("```\n");
    let line: String = report
        .last_7_days_messages
        .iter()
        .map(|p| spark_char(p.messages, max))
        .collect();
    out.push_str(&line);
    out.push('\n');
    // Day labels under the bars so a flat sparkline still anchors to dates.
    let labels: String = report
        .last_7_days_messages
        .iter()
        .map(|DailyPoint { date, .. }| {
            // 1-char weekday code (Mon=M, Tue=T, etc.) keeps the label row
            // the same width as the sparkline.
            match date.weekday().num_days_from_monday() {
                0 => 'M',
                1 => 'T',
                2 => 'W',
                3 => 'T',
                4 => 'F',
                5 => 'S',
                _ => 'S',
            }
        })
        .collect();
    out.push_str(&labels);
    out.push('\n');
    out.push_str("```\n");
}

// ── helpers ───────────────────────────────────────────────────────────────

fn ascii_bar(value: i64, max: i64, width: usize) -> String {
    let max = max.max(1) as f64;
    let v = value.max(0) as f64;
    let total_eighths = ((v / max) * (width as f64) * 8.0).round() as usize;
    let full = (total_eighths / 8).min(width);
    let remainder = total_eighths - full * 8;
    let mut s = String::with_capacity(width);
    for _ in 0..full {
        s.push('█');
    }
    if full < width {
        s.push(BLOCK_CHARS[remainder]);
        for _ in (full + 1)..width {
            s.push(BLOCK_CHARS[0]);
        }
    }
    s
}

fn spark_char(value: i64, max: i64) -> char {
    if max <= 0 {
        return BLOCK_CHARS[0];
    }
    let scaled = ((value.max(0) as f64) / (max as f64)) * 8.0;
    let idx = scaled.round().clamp(0.0, 8.0) as usize;
    // Sparkline reads better without the ' ' (idx 0) — for a non-zero day,
    // even one message should produce a visible mark.
    if value > 0 && idx == 0 {
        return BLOCK_CHARS[1];
    }
    BLOCK_CHARS[idx]
}

/// Visual width — counts Unicode scalars, not bytes. Russian labels are
/// 1 column per char, same as ASCII; emoji would skew this but neither
/// label list contains emoji.
fn visual_width(s: &str) -> usize {
    s.chars().count()
}

fn truncate_chars(s: &str, max: usize) -> String {
    let mut out = String::with_capacity(max);
    for (i, c) in s.chars().enumerate() {
        if i >= max {
            out.push('…');
            return out;
        }
        out.push(c);
    }
    out
}

fn format_window(from: DateTime<Utc>, to: DateTime<Utc>) -> String {
    // ISO-ish without sub-second precision. The renderer is locale-free —
    // dates always render as `YYYY-MM-DD HH:MM` UTC. Per-chat-tz formatting
    // would require carrying the tz through and adds little value: the
    // report fires at the chat's local-evening hour, so the absolute UTC
    // window is unambiguous in context.
    format!(
        "{} → {} UTC",
        from.format("%Y-%m-%d %H:%M"),
        to.format("%Y-%m-%d %H:%M"),
    )
}

/// MarkdownV2 escape per Telegram spec. The full special set is
/// `_*[]()~\`>#+-=|{}.!` — every occurrence must be backslash-prefixed.
pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '_' | '*' | '[' | ']' | '(' | ')' | '~' | '`' | '>' | '#' | '+' | '-' | '=' | '|'
            | '{' | '}' | '.' | '!' | '\\' => {
                out.push('\\');
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::report::{CaptchaCounts, DailyPoint, TopPhrase};
    use chrono::{Duration, NaiveDate, TimeZone};

    fn fixture() -> ReportData {
        let from = Utc.with_ymd_and_hms(2026, 5, 1, 17, 0, 0).unwrap();
        let to = Utc.with_ymd_and_hms(2026, 5, 2, 17, 0, 0).unwrap();
        let start = NaiveDate::from_ymd_opt(2026, 4, 26).unwrap();
        let last_7 = (0..7)
            .map(|i: i64| DailyPoint {
                date: start + Duration::days(i),
                messages: [3, 5, 0, 12, 7, 2, 9][i as usize],
            })
            .collect();
        ReportData {
            chat_id: -1001,
            from,
            to,
            chat_title: Some("Vixen test chat".into()),
            messages_seen: 120,
            messages_deleted: 4,
            users_verified: 2,
            users_banned: 1,
            captcha: CaptchaCounts {
                issued: 5,
                solved: 3,
                expired: 2,
            },
            top_phrases: vec![
                TopPhrase {
                    text: "buy now".into(),
                    hits: 7,
                },
                TopPhrase {
                    text: "click here".into(),
                    hits: 3,
                },
            ],
            last_7_days_messages: last_7,
        }
    }

    #[test]
    fn escape_all_special_chars() {
        let s = escape("a.b_c*d[e]f(g)h~i`j>k#l+m-n=o|p{q}r!s\\t");
        assert_eq!(
            s,
            "a\\.b\\_c\\*d\\[e\\]f\\(g\\)h\\~i\\`j\\>k\\#l\\+m\\-n\\=o\\|p\\{q\\}r\\!s\\\\t"
        );
    }

    #[test]
    fn ascii_bar_full_when_value_equals_max() {
        let bar = ascii_bar(10, 10, 5);
        assert_eq!(bar, "█████");
    }

    #[test]
    fn ascii_bar_empty_when_value_is_zero() {
        let bar = ascii_bar(0, 10, 5);
        assert_eq!(bar, "     ");
    }

    #[test]
    fn ascii_bar_partial() {
        // 25% of 5 columns = 1.25 cells = 1 full + 2-eighths (10/8 = 1.25)
        let bar = ascii_bar(25, 100, 5);
        // 25/100 * 5 * 8 = 10 eighths → full=1, remainder=2 → '█▂   '
        assert_eq!(bar.chars().next().unwrap(), '█');
    }

    #[test]
    fn render_is_deterministic() {
        let a = render(&fixture(), Lang::Ru, HeaderKind::Daily);
        let b = render(&fixture(), Lang::Ru, HeaderKind::Daily);
        assert_eq!(a, b);
    }

    #[test]
    fn render_includes_header_and_top_phrases() {
        let s = render(&fixture(), Lang::Ru, HeaderKind::Daily);
        assert!(s.contains("Ежедневный отчёт"));
        assert!(s.contains("Vixen test chat"));
        assert!(s.contains("buy now"));
        assert!(s.contains("click here"));
    }

    #[test]
    fn render_omits_captcha_when_zero() {
        let mut r = fixture();
        r.captcha = CaptchaCounts::default();
        let s = render(&r, Lang::Ru, HeaderKind::Daily);
        assert!(!s.contains("Капча"));
    }

    #[test]
    fn render_omits_top_phrases_when_empty() {
        let mut r = fixture();
        r.top_phrases.clear();
        let s = render(&r, Lang::Ru, HeaderKind::Daily);
        assert!(!s.contains("Частые фразы"));
    }

    #[test]
    fn render_omits_sparkline_on_quiet_week() {
        let mut r = fixture();
        for p in &mut r.last_7_days_messages {
            p.messages = 0;
        }
        let s = render(&r, Lang::Ru, HeaderKind::Daily);
        assert!(!s.contains("7 дней"));
    }

    #[test]
    fn english_locale_renders_english_strings() {
        let s = render(&fixture(), Lang::En, HeaderKind::Daily);
        assert!(s.contains("Daily report"));
        assert!(s.contains("Messages"));
        assert!(s.contains("Top phrases"));
    }

    #[test]
    fn truncates_long_phrases() {
        let mut r = fixture();
        r.top_phrases = vec![TopPhrase {
            text: "a".repeat(120),
            hits: 1,
        }];
        let s = render(&r, Lang::Ru, HeaderKind::Daily);
        assert!(s.contains('…'));
    }

    #[test]
    fn header_kind_changes_title() {
        let r = fixture();
        let daily = render(&r, Lang::Ru, HeaderKind::Daily);
        let today = render(&r, Lang::Ru, HeaderKind::Today);
        let on_demand = render(&r, Lang::Ru, HeaderKind::OnDemand);
        assert!(daily.contains("Ежедневный отчёт"));
        assert!(!daily.contains("Сводка за сегодня"));
        assert!(today.contains("Сводка за сегодня"));
        assert!(!today.contains("Ежедневный отчёт"));
        assert!(on_demand.contains("Отчёт по запросу"));
    }

    #[test]
    fn chat_title_special_chars_are_escaped() {
        let mut r = fixture();
        r.chat_title = Some("acme [admins] · v1.2 (beta)!".to_string());
        let s = render(&r, Lang::Ru, HeaderKind::Daily);
        // Each MarkdownV2 special char must be backslash-prefixed.
        assert!(s.contains(r"\[admins\]"));
        assert!(s.contains(r"v1\.2"));
        assert!(s.contains(r"\(beta\)"));
        assert!(s.contains('!'));
        assert!(s.contains(r"\!"));
    }

    #[test]
    fn top_phrase_with_special_chars_is_escaped() {
        let mut r = fixture();
        r.top_phrases = vec![TopPhrase {
            text: "click_here.now (free!)".into(),
            hits: 9,
        }];
        let s = render(&r, Lang::Ru, HeaderKind::Daily);
        assert!(s.contains(r"click\_here\.now"));
        assert!(s.contains(r"\(free\!\)"));
    }

    #[test]
    fn top_phrase_newlines_collapse_to_space() {
        let mut r = fixture();
        r.top_phrases = vec![TopPhrase {
            text: "line1\nline2\nline3".into(),
            hits: 1,
        }];
        let s = render(&r, Lang::Ru, HeaderKind::Daily);
        // No raw newline inside the phrase line — Telegram would render it
        // as two list rows otherwise.
        let phrase_line = s
            .lines()
            .find(|l| l.starts_with("▌"))
            .expect("phrase line present");
        assert!(!phrase_line.contains('\n'));
        assert!(phrase_line.contains("line1 line2 line3"));
    }

    /// Extract the 7-char sparkline row + day-of-week label row from the
    /// rendered output. Located by anchoring on the `*7 дней*` heading and
    /// reading the next two non-fence lines, so a future header tweak in the
    /// counts block won't accidentally match here.
    fn sparkline_rows(rendered: &str) -> (String, String) {
        let mut iter = rendered.lines();
        for line in iter.by_ref() {
            if line.contains("*7 дней*") || line.contains("*Last 7 days*") {
                break;
            }
        }
        // Skip the opening ``` fence.
        let _ = iter.next();
        let bars = iter.next().unwrap_or("").to_string();
        let labels = iter.next().unwrap_or("").to_string();
        (bars, labels)
    }

    #[test]
    fn sparkline_emits_visible_mark_for_smallest_nonzero() {
        let mut r = fixture();
        for (i, p) in r.last_7_days_messages.iter_mut().enumerate() {
            p.messages = if i == 0 { 1 } else { 1000 };
        }
        let s = render(&r, Lang::Ru, HeaderKind::Daily);
        let (bars, _) = sparkline_rows(&s);
        // The smallest non-zero day should still produce a visible block —
        // never collapse to a space (which would visually skip the day).
        assert!(bars.starts_with('▁'), "got {bars:?}");
    }

    #[test]
    fn sparkline_label_row_is_seven_chars() {
        let r = fixture();
        let s = render(&r, Lang::Ru, HeaderKind::Daily);
        let (bars, labels) = sparkline_rows(&s);
        // The day-of-week labels under the sparkline must match the bar
        // count exactly so they line up in monospace.
        assert_eq!(labels.chars().count(), 7, "got {labels:?}");
        assert_eq!(bars.chars().count(), 7, "got {bars:?}");
    }

    #[test]
    fn lang_from_db_str_falls_back_to_ru() {
        assert!(matches!(Lang::from_db_str("ru"), Lang::Ru));
        assert!(matches!(Lang::from_db_str("en"), Lang::En));
        // Unknown / mistyped value collapses to RU rather than panicking.
        assert!(matches!(Lang::from_db_str(""), Lang::Ru));
        assert!(matches!(Lang::from_db_str("fr"), Lang::Ru));
    }

    #[test]
    fn render_strips_trailing_blank_lines() {
        let r = fixture();
        let s = render(&r, Lang::Ru, HeaderKind::Daily);
        assert!(!s.ends_with('\n'));
    }

    #[test]
    fn ascii_bar_value_above_max_clamps_to_full() {
        let bar = ascii_bar(150, 100, 5);
        assert_eq!(bar, "█████");
    }

    #[test]
    fn ascii_bar_negative_value_renders_empty() {
        let bar = ascii_bar(-3, 10, 5);
        assert_eq!(bar, "     ");
    }

    #[test]
    fn render_omits_chat_title_line_when_unknown() {
        let mut r = fixture();
        r.chat_title = None;
        let s = render(&r, Lang::Ru, HeaderKind::Daily);
        assert!(!s.contains("Vixen test chat"));
        // Header → date line → counts: no orphan blank.
        assert!(s.contains("UTC"));
    }
}
