//! Daily-report chart renderer. Produces a 480×270 lossless WebP — same
//! geometry and encoder as the M1 captcha so the chat preview behaves
//! identically (16:9, no tap-to-expand, ≤ ~80 KB lossless).
//!
//! Two stacked panels:
//!
//! 1. Counts — bars for `messages_seen`, `messages_deleted`, `users_verified`,
//!    `users_banned`, `captcha.solved`, `captcha.expired`.
//! 2. Last-7-days — bar per calendar day of `messages_seen`, oldest first.
//!
//! Pure plotters → RGB pixel buffer → `image::codecs::webp::WebPEncoder`. No
//! standalone `webp` crate dependency, matches the captcha pipeline. CPU work
//! runs on a `spawn_blocking` thread so the bot poller stays responsive.

use std::sync::Once;

use anyhow::{Context, Result};
use image::ExtendedColorType;
use image::codecs::webp::WebPEncoder;
use plotters::prelude::*;
use plotters::style::register_font;

use crate::models::report::ReportData;

/// Bundled font for plotters' `ab_glyph` text backend. Re-uses the same TTF
/// the M1 captcha renderer ships with so we don't carry a second font copy
/// in the binary, and so a Docker / CI environment without system fonts
/// still produces a readable chart. Registration is process-global and
/// idempotent — the `Once` guards a re-entrant call from concurrent
/// `render` invocations.
const FONT_TTF: &[u8] = include_bytes!("../../assets/captcha/DejaVuSans.ttf");
static FONT_INIT: Once = Once::new();

fn ensure_font_registered() {
    FONT_INIT.call_once(|| {
        // Both "sans-serif" (plotters' default family alias) and the literal
        // family registration are useful: callers that pass `("sans-serif",
        // 12)` and callers that pass `("DejaVuSans", 12)` both resolve to
        // the same bytes. `register_font` returning `Err` would mean the
        // font bytes are corrupt — there is no recovery, panic with a
        // clear message instead of silently producing label-less charts.
        if register_font("sans-serif", plotters::style::FontStyle::Normal, FONT_TTF).is_err() {
            panic!("plotters register_font (sans-serif) failed: invalid TTF");
        }
    });
}

pub const WIDTH: u32 = 480;
pub const HEIGHT: u32 = 270;

/// Soft cap: large lossless WebPs are still well under this for the chart
/// shapes we emit (small palette, big flat areas). The caller asserts.
pub const MAX_BYTES: usize = 80 * 1024;

/// Render a chart as lossless WebP bytes. CPU-bound; callers spawn this on
/// a blocking thread so the tokio runtime keeps progressing.
pub fn render(report: &ReportData) -> Result<Vec<u8>> {
    ensure_font_registered();
    let mut rgb = vec![255u8; (WIDTH * HEIGHT * 3) as usize];
    {
        let backend = BitMapBackend::with_buffer(&mut rgb, (WIDTH, HEIGHT));
        let root = backend.into_drawing_area();
        root.fill(&WHITE).context("plotters fill bg")?;
        let panels = root.split_evenly((2, 1));
        draw_counts_panel(&panels[0], report).context("counts panel")?;
        draw_sparkline_panel(&panels[1], report).context("sparkline panel")?;
        root.present().context("plotters present")?;
    }

    let mut webp = Vec::with_capacity(MAX_BYTES);
    let encoder = WebPEncoder::new_lossless(&mut webp);
    encoder
        .encode(&rgb, WIDTH, HEIGHT, ExtendedColorType::Rgb8)
        .context("encode WebP")?;
    Ok(webp)
}

fn draw_counts_panel<DB: DrawingBackend>(
    area: &DrawingArea<DB, plotters::coord::Shift>,
    report: &ReportData,
) -> Result<()>
where
    DB::ErrorType: 'static,
{
    let bars: [(&str, i64, RGBColor); 6] = [
        ("Msg", report.messages_seen, RGBColor(46, 134, 222)),
        ("Del", report.messages_deleted, RGBColor(238, 90, 36)),
        ("Ver", report.users_verified, RGBColor(16, 172, 132)),
        ("Ban", report.users_banned, RGBColor(192, 57, 43)),
        ("CapOk", report.captcha.solved, RGBColor(72, 219, 251)),
        ("CapExp", report.captcha.expired, RGBColor(165, 94, 234)),
    ];

    let max_value = bars.iter().map(|(_, v, _)| *v).max().unwrap_or(0).max(1);

    let mut chart = ChartBuilder::on(area)
        .margin(6)
        .caption("Counts", ("sans-serif", 14).into_font())
        .x_label_area_size(18)
        .y_label_area_size(28)
        .build_cartesian_2d(0..bars.len(), 0i64..(max_value + max_value / 5 + 1))
        .map_err(|e| anyhow::anyhow!("counts chart build: {e}"))?;

    chart
        .configure_mesh()
        .disable_x_mesh()
        .disable_y_mesh()
        .x_labels(bars.len())
        .x_label_formatter(&|idx| {
            bars.get(*idx)
                .map(|(label, _, _)| (*label).to_string())
                .unwrap_or_default()
        })
        .y_labels(4)
        .label_style(("sans-serif", 10).into_font())
        .draw()
        .map_err(|e| anyhow::anyhow!("counts mesh: {e}"))?;

    chart
        .draw_series(bars.iter().enumerate().map(|(i, (_, value, color))| {
            let mut bar = Rectangle::new([(i, 0i64), (i + 1, *value)], color.filled());
            bar.set_margin(0, 0, 4, 4);
            bar
        }))
        .map_err(|e| anyhow::anyhow!("counts draw_series: {e}"))?;

    Ok(())
}

fn draw_sparkline_panel<DB: DrawingBackend>(
    area: &DrawingArea<DB, plotters::coord::Shift>,
    report: &ReportData,
) -> Result<()>
where
    DB::ErrorType: 'static,
{
    let points = &report.last_7_days_messages;
    let max_value = points.iter().map(|p| p.messages).max().unwrap_or(0).max(1);
    let n = points.len();

    let mut chart = ChartBuilder::on(area)
        .margin(6)
        .caption("Last 7 days · messages", ("sans-serif", 14).into_font())
        .x_label_area_size(18)
        .y_label_area_size(28)
        .build_cartesian_2d(0..n, 0i64..(max_value + max_value / 5 + 1))
        .map_err(|e| anyhow::anyhow!("sparkline build: {e}"))?;

    chart
        .configure_mesh()
        .disable_x_mesh()
        .disable_y_mesh()
        .x_labels(n)
        .x_label_formatter(&|idx| {
            points
                .get(*idx)
                .map(|p| p.date.format("%m/%d").to_string())
                .unwrap_or_default()
        })
        .y_labels(4)
        .label_style(("sans-serif", 10).into_font())
        .draw()
        .map_err(|e| anyhow::anyhow!("sparkline mesh: {e}"))?;

    chart
        .draw_series(points.iter().enumerate().map(|(i, p)| {
            let mut bar = Rectangle::new(
                [(i, 0i64), (i + 1, p.messages)],
                RGBColor(46, 134, 222).filled(),
            );
            bar.set_margin(0, 0, 4, 4);
            bar
        }))
        .map_err(|e| anyhow::anyhow!("sparkline draw_series: {e}"))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::report::{CaptchaCounts, DailyPoint, ReportData};
    use chrono::{Duration, NaiveDate, TimeZone, Utc};

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
            chat_title: Some("Test".into()),
            messages_seen: 120,
            messages_deleted: 4,
            users_verified: 2,
            users_banned: 1,
            captcha: CaptchaCounts {
                issued: 5,
                solved: 3,
                expired: 2,
            },
            top_phrases: vec![],
            last_7_days_messages: last_7,
        }
    }

    #[test]
    fn renders_under_max_bytes() {
        let bytes = render(&fixture()).expect("render");
        assert!(
            bytes.len() <= MAX_BYTES,
            "WebP bytes={} > MAX_BYTES={}",
            bytes.len(),
            MAX_BYTES
        );
    }

    #[test]
    fn renders_webp_magic() {
        let bytes = render(&fixture()).expect("render");
        // RIFF....WEBP
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WEBP");
    }

    #[test]
    fn renders_zero_data_without_panic() {
        let mut r = fixture();
        r.messages_seen = 0;
        r.messages_deleted = 0;
        r.users_verified = 0;
        r.users_banned = 0;
        r.captcha = CaptchaCounts::default();
        for p in &mut r.last_7_days_messages {
            p.messages = 0;
        }
        let bytes = render(&r).expect("render");
        assert!(bytes.len() > 100);
    }
}
