//! Deterministic 480×270 lossless WebP captcha renderer.
//!
//! Rendered at **2× internal resolution (960×540)** for free anti-aliasing
//! on rotated digit edges, geometric shapes and line strokes; then
//! Lanczos3-downscaled to the final 480×270 output. The 16:9 output ratio
//! sits below Telegram's mobile preview crop threshold (~1.91:1) so the
//! captcha displays in full on iOS / Android / Desktop / Web without a
//! tap-to-expand.
//!
//! Lossless WebP is encoded via `image::codecs::webp::WebPEncoder` — no
//! standalone `webp` crate dependency. Lossless avoids the chroma
//! subsampling artefacts that `lossy` WebP produces on thin AA strokes.
//!
//! Layered composition — every layer writes into the same RGBA buffer
//! with proper alpha compositing:
//!
//! 1. Vertical pastel-or-deep gradient picked deterministically from
//!    [`PALETTES`].
//! 2. 18..22 large translucent **background shapes** (circle / rectangle
//!    / thick line) from the palette's accent set.
//! 3. 4 digit glyphs (`ab_glyph` outlines) with scale-jitter, rotation
//!    ±15°, position-jitter and per-digit accent colour. The font's own
//!    coverage values become alpha at the supersampled resolution so the
//!    Lanczos downscale produces clean edges in the final image.
//! 4. 30..40 quadratic Bézier "scribble" curves overlaid at low alpha
//!    in the palette's `noise` colour.
//! 5. 18..22 small translucent **foreground shapes** (whites / greys)
//!    laid over the digits to defeat naive OCR while staying readable.
//!
//! All randomness derives from `xxh3(challenge_id)` so the same UUID
//! always produces byte-identical output (snapshot test:
//! `tests::deterministic_for_same_inputs`). Adding new palettes or
//! re-ordering [`PALETTES`] changes the visual mapping for existing
//! UUIDs — that's allowed (no on-disk byte-pinning), but bear in mind
//! that pending challenges in flight will look different after the
//! deploy if their button-refresh re-renders them.

use ab_glyph::{Font, FontRef, PxScale};
use anyhow::{Result, anyhow, ensure};
use image::codecs::webp::WebPEncoder;
use image::imageops::FilterType;
use image::{ExtendedColorType, ImageBuffer, Rgba, RgbaImage};
use uuid::Uuid;
use xxhash_rust::xxh3::xxh3_64;

use super::fonts::Fonts;

pub const WIDTH: u32 = 480;
pub const HEIGHT: u32 = 270;

/// Supersampling factor for internal rasterisation — every pixel-level
/// op (gradient fill, glyph coverage, shape blend) runs at this
/// multiplier and the result is Lanczos3-downscaled at the end. 2×
/// gives effectively 4× area-sampled antialiasing on hard edges and
/// rotated glyphs while keeping the canvas at a reasonable ~2 MB.
const SUPER: u32 = 2;
const W_HI: u32 = WIDTH * SUPER;
const H_HI: u32 = HEIGHT * SUPER;
const DIGIT_COUNT: u32 = 4;

/// Render the captcha for the given challenge as lossless WebP bytes.
///
/// Pure CPU work; the caller wraps this in `tokio::task::spawn_blocking`
/// so the tokio runtime stays responsive.
pub fn render_webp(challenge_id: Uuid, solution: &str, fonts: &Fonts) -> Result<Vec<u8>> {
    ensure!(
        solution.chars().count() == DIGIT_COUNT as usize,
        "solution must be {DIGIT_COUNT} chars, got {}",
        solution.chars().count()
    );

    let mut rng = SeededRng::from_uuid(challenge_id);
    let palette = pick_palette(&mut rng);
    let mut canvas: RgbaImage = ImageBuffer::new(W_HI, H_HI);

    fill_gradient(&mut canvas, palette);
    draw_background_shapes(&mut canvas, &mut rng, palette);
    draw_digits(&mut canvas, &mut rng, palette, solution, &fonts.primary)?;
    draw_curves(&mut canvas, &mut rng, palette);
    draw_foreground_shapes(&mut canvas, &mut rng);

    // Lanczos3 is a 6-tap separable filter — sharp where it should be,
    // smooth where it shouldn't. Triangle / Nearest looked muddier on
    // the rotated glyph edges in dev tests.
    let final_img = image::imageops::resize(&canvas, WIDTH, HEIGHT, FilterType::Lanczos3);

    let mut bytes: Vec<u8> = Vec::with_capacity(16 * 1024);
    WebPEncoder::new_lossless(&mut bytes)
        .encode(final_img.as_raw(), WIDTH, HEIGHT, ExtendedColorType::Rgba8)
        .map_err(|e| anyhow!("WebP encode failed: {e}"))?;
    Ok(bytes)
}

// ── Deterministic PRNG ────────────────────────────────────────────────────

struct SeededRng(u64);

impl SeededRng {
    fn from_uuid(id: Uuid) -> Self {
        let h = xxh3_64(id.as_bytes());
        Self(if h == 0 { 1 } else { h })
    }

    /// xorshift64 — fixed iteration order, identical output for identical seeds.
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    fn next_unit(&mut self) -> f32 {
        ((self.next_u64() >> 40) as f32) / (1u32 << 24) as f32
    }

    fn range_f32(&mut self, lo: f32, hi: f32) -> f32 {
        lo + self.next_unit() * (hi - lo)
    }

    fn range_i32(&mut self, lo: i32, hi: i32) -> i32 {
        let span = (hi - lo) as u64;
        lo + (self.next_u64() % span) as i32
    }

    fn range_u32(&mut self, lo: u32, hi: u32) -> u32 {
        lo + (self.next_u64() % (hi - lo) as u64) as u32
    }
}

// ── Palettes ──────────────────────────────────────────────────────────────

/// One self-contained look. `digit_colors` are picked by index modulo
/// length, so palettes with 4 entries assign one colour per digit; with
/// 3 entries the first digit's colour repeats on the fourth.
#[derive(Clone, Copy)]
struct Palette {
    bg_top: [u8; 3],
    bg_bottom: [u8; 3],
    digit_colors: &'static [[u8; 3]],
    /// Translucent overlay shapes on top of the gradient (under the
    /// digits). Alpha is baked into the colour for tighter call sites.
    shape_colors: &'static [[u8; 4]],
    /// Bézier-curve overlay colour (alpha applied at draw site).
    noise: [u8; 3],
}

const PALETTES: &[Palette] = &[
    // 0. Twilight — deep navy → magenta with warm digit accents.
    Palette {
        bg_top: [22, 33, 62],
        bg_bottom: [70, 30, 90],
        digit_colors: &[
            [255, 215, 130],
            [240, 130, 110],
            [255, 235, 200],
            [200, 230, 255],
        ],
        shape_colors: &[
            [255, 99, 132, 70],
            [54, 162, 235, 70],
            [255, 205, 86, 70],
            [153, 102, 255, 70],
        ],
        noise: [180, 180, 220],
    },
    // 1. Forest — teal → moss with cream / salmon digits.
    Palette {
        bg_top: [12, 60, 70],
        bg_bottom: [10, 100, 80],
        digit_colors: &[
            [255, 240, 150],
            [200, 255, 220],
            [255, 200, 220],
            [180, 220, 255],
        ],
        shape_colors: &[
            [75, 192, 192, 70],
            [255, 205, 86, 70],
            [255, 159, 64, 70],
            [54, 162, 235, 60],
        ],
        noise: [200, 230, 220],
    },
    // 2. Plum — maroon → rose with cream digits.
    Palette {
        bg_top: [50, 25, 35],
        bg_bottom: [110, 50, 60],
        digit_colors: &[
            [255, 235, 210],
            [255, 200, 170],
            [240, 255, 220],
            [200, 220, 255],
        ],
        shape_colors: &[
            [221, 160, 221, 70],
            [255, 182, 193, 70],
            [255, 218, 185, 70],
            [230, 230, 250, 70],
        ],
        noise: [230, 200, 200],
    },
    // 3. Sky pastel — cerulean → peach with deep digits (light theme).
    Palette {
        bg_top: [70, 130, 180],
        bg_bottom: [240, 200, 160],
        digit_colors: &[[40, 30, 60], [80, 30, 50], [60, 25, 70], [30, 60, 90]],
        shape_colors: &[
            [255, 255, 255, 60],
            [200, 200, 220, 60],
            [173, 216, 230, 70],
            [255, 218, 185, 70],
        ],
        noise: [60, 60, 90],
    },
    // 4. Lavender mist — lilac → cream with deep purple digits.
    Palette {
        bg_top: [150, 120, 200],
        bg_bottom: [240, 230, 220],
        digit_colors: &[[60, 30, 90], [40, 60, 110], [80, 30, 80], [30, 50, 70]],
        shape_colors: &[
            [221, 160, 221, 60],
            [255, 182, 193, 60],
            [173, 216, 230, 60],
            [255, 255, 255, 50],
        ],
        noise: [120, 100, 150],
    },
    // 5. Coral — peach → coral with deep navy digits.
    Palette {
        bg_top: [255, 175, 140],
        bg_bottom: [240, 110, 110],
        digit_colors: &[[30, 30, 70], [40, 20, 60], [70, 30, 50], [30, 50, 90]],
        shape_colors: &[
            [255, 240, 200, 70],
            [255, 205, 170, 70],
            [255, 255, 255, 60],
            [200, 220, 240, 60],
        ],
        noise: [80, 50, 70],
    },
];

fn pick_palette(rng: &mut SeededRng) -> Palette {
    PALETTES[(rng.next_u64() as usize) % PALETTES.len()]
}

// ── Background gradient ───────────────────────────────────────────────────

fn fill_gradient(canvas: &mut RgbaImage, palette: Palette) {
    let h = H_HI as f32;
    for y in 0..H_HI {
        let t = y as f32 / (h - 1.0);
        let r = lerp_u8(palette.bg_top[0], palette.bg_bottom[0], t);
        let g = lerp_u8(palette.bg_top[1], palette.bg_bottom[1], t);
        let b = lerp_u8(palette.bg_top[2], palette.bg_bottom[2], t);
        let row = Rgba([r, g, b, 255]);
        for x in 0..W_HI {
            canvas.put_pixel(x, y, row);
        }
    }
}

fn lerp_u8(a: u8, b: u8, t: f32) -> u8 {
    let v = a as f32 + (b as f32 - a as f32) * t;
    v.clamp(0.0, 255.0) as u8
}

// ── Background / foreground shapes ────────────────────────────────────────

fn draw_background_shapes(canvas: &mut RgbaImage, rng: &mut SeededRng, palette: Palette) {
    let count = rng.range_u32(18, 23);
    for _ in 0..count {
        let kind = rng.next_u64() % 3;
        let color = palette.shape_colors[(rng.next_u64() as usize) % palette.shape_colors.len()];
        match kind {
            0 => draw_circle(canvas, rng, color, false),
            1 => draw_rectangle(canvas, rng, color, false),
            _ => draw_line(canvas, rng, color, false),
        }
    }
}

/// Light overlay shapes on top of the digits — they break up large
/// monochrome glyph fills (a common naive-OCR signal) without making
/// the digits unreadable.
fn draw_foreground_shapes(canvas: &mut RgbaImage, rng: &mut SeededRng) {
    const FG_COLORS: &[[u8; 4]] = &[
        [255, 255, 255, 50],
        [240, 240, 240, 60],
        [200, 200, 200, 50],
    ];
    let count = rng.range_u32(18, 23);
    for _ in 0..count {
        let kind = rng.next_u64() % 3;
        let color = FG_COLORS[(rng.next_u64() as usize) % FG_COLORS.len()];
        match kind {
            0 => draw_circle(canvas, rng, color, true),
            1 => draw_rectangle(canvas, rng, color, true),
            _ => draw_line(canvas, rng, color, true),
        }
    }
}

fn draw_circle(canvas: &mut RgbaImage, rng: &mut SeededRng, color: [u8; 4], small: bool) {
    let w = W_HI as i32;
    let h = H_HI as i32;
    let base = w.min(h);
    let radius = if small {
        rng.range_i32(20 + base / 16, 30 + base / 8)
    } else {
        rng.range_i32(40 + base / 8, 60 + base / 4)
    };
    let cx = rng.range_i32(0, w);
    let cy = rng.range_i32(0, h);
    let r2 = radius * radius;
    let y_min = (cy - radius).max(0);
    let y_max = (cy + radius).min(h - 1);
    let x_min = (cx - radius).max(0);
    let x_max = (cx + radius).min(w - 1);
    let rgb = [color[0], color[1], color[2]];
    let alpha = color[3];
    for y in y_min..=y_max {
        for x in x_min..=x_max {
            let dx = x - cx;
            let dy = y - cy;
            if dx * dx + dy * dy <= r2 {
                blend_pixel(canvas, x, y, rgb, alpha);
            }
        }
    }
}

fn draw_rectangle(canvas: &mut RgbaImage, rng: &mut SeededRng, color: [u8; 4], small: bool) {
    let w = W_HI as i32;
    let h = H_HI as i32;
    let rw = if small {
        rng.range_i32(w / 16, w / 8)
    } else {
        rng.range_i32(w / 8, w / 4)
    };
    let rh = if small {
        rng.range_i32(h / 16, h / 8)
    } else {
        rng.range_i32(h / 8, h / 4)
    };
    let sx = rng.range_i32(0, (w - rw).max(1));
    let sy = rng.range_i32(0, (h - rh).max(1));
    let rgb = [color[0], color[1], color[2]];
    let alpha = color[3];
    for y in sy..(sy + rh).min(h) {
        for x in sx..(sx + rw).min(w) {
            blend_pixel(canvas, x, y, rgb, alpha);
        }
    }
}

fn draw_line(canvas: &mut RgbaImage, rng: &mut SeededRng, color: [u8; 4], small: bool) {
    let w = W_HI as i32;
    let h = H_HI as i32;
    let x0 = rng.range_i32(0, w);
    let y0 = rng.range_i32(0, h);
    let x1 = rng.range_i32(0, w);
    let y1 = rng.range_i32(0, h);
    let thickness = if small {
        rng.range_i32(2, 6)
    } else {
        rng.range_i32(6, 14)
    };
    draw_thick_line(canvas, x0, y0, x1, y1, thickness, color);
}

fn draw_thick_line(
    canvas: &mut RgbaImage,
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
    thickness: i32,
    color: [u8; 4],
) {
    let w = W_HI as i32;
    let h = H_HI as i32;
    let dx = (x1 - x0).abs();
    let dy = (y1 - y0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx - dy;
    let mut x = x0;
    let mut y = y0;
    let half = thickness / 2;
    let rgb = [color[0], color[1], color[2]];
    let alpha = color[3];
    loop {
        for ty in -half..=half {
            for tx in -half..=half {
                let px = x + tx;
                let py = y + ty;
                if px >= 0 && px < w && py >= 0 && py < h {
                    blend_pixel(canvas, px, py, rgb, alpha);
                }
            }
        }
        if x == x1 && y == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 > -dy {
            err -= dy;
            x += sx;
        }
        if e2 < dx {
            err += dx;
            y += sy;
        }
    }
}

// ── Bezier curves (organic noise) ─────────────────────────────────────────

fn draw_curves(canvas: &mut RgbaImage, rng: &mut SeededRng, palette: Palette) {
    let count = rng.range_u32(30, 41);
    let rgb = palette.noise;
    for _ in 0..count {
        let x0 = rng.range_f32(0.0, W_HI as f32);
        let y0 = rng.range_f32(0.0, H_HI as f32);
        let x1 = rng.range_f32(0.0, W_HI as f32);
        let y1 = rng.range_f32(0.0, H_HI as f32);
        let cx = rng.range_f32(0.0, W_HI as f32);
        let cy = rng.range_f32(0.0, H_HI as f32);
        // Sampled denser at supersampled resolution so the downscale
        // smooths the polyline into a continuous stroke.
        let steps = 120;
        for i in 0..=steps {
            let t = i as f32 / steps as f32;
            let it = 1.0 - t;
            let x = it * it * x0 + 2.0 * it * t * cx + t * t * x1;
            let y = it * it * y0 + 2.0 * it * t * cy + t * t * y1;
            blend_pixel(canvas, x as i32, y as i32, rgb, 70);
        }
    }
}

// ── Digits ────────────────────────────────────────────────────────────────

fn draw_digits(
    canvas: &mut RgbaImage,
    rng: &mut SeededRng,
    palette: Palette,
    solution: &str,
    font: &FontRef<'static>,
) -> Result<()> {
    let cell_w = W_HI as f32 / DIGIT_COUNT as f32;
    let center_y = H_HI as f32 / 2.0;

    for (i, c) in solution.chars().enumerate() {
        // All sizing scales linearly with SUPER so layouts match the
        // pre-supersampling design (216..264 px @ 2× == 108..132 @ 1×).
        let scale_px = rng.range_f32(216.0, 264.0);
        let angle_deg = rng.range_f32(-15.0, 15.0);
        let dx_jitter = rng.range_f32(-12.0, 12.0);
        let dy_jitter = rng.range_f32(-20.0, 20.0);
        let color = palette.digit_colors[i % palette.digit_colors.len()];
        let px = cell_w * (i as f32 + 0.5) + dx_jitter;
        let py = center_y + dy_jitter;
        rasterize_digit(canvas, font, c, scale_px, angle_deg, px, py, color)?;
    }

    Ok(())
}

/// Rasterise a single glyph onto `canvas` at `(cx, cy)` (the glyph's
/// centre) with `angle_deg` rotation. `ab_glyph`'s coverage values (a
/// 0..1 alpha mask) become the per-pixel alpha here, then survive the
/// downscale as a soft anti-aliased edge.
#[allow(clippy::too_many_arguments)]
fn rasterize_digit(
    canvas: &mut RgbaImage,
    font: &FontRef<'static>,
    c: char,
    scale_px: f32,
    angle_deg: f32,
    cx: f32,
    cy: f32,
    digit_color: [u8; 3],
) -> Result<()> {
    let scale = PxScale::from(scale_px);
    let glyph_id = font.glyph_id(c);
    let glyph = glyph_id.with_scale(scale);
    let outlined = font
        .outline_glyph(glyph)
        .ok_or_else(|| anyhow!("no outline for glyph '{c}'"))?;

    let bounds = outlined.px_bounds();
    let glyph_w = bounds.width().ceil() as i32;
    let glyph_h = bounds.height().ceil() as i32;
    if glyph_w <= 0 || glyph_h <= 0 {
        return Ok(());
    }

    // Coverage buffer for the glyph in its own local space.
    let mut cover = vec![0u8; (glyph_w * glyph_h) as usize];
    outlined.draw(|gx, gy, c| {
        let idx = (gy as i32) * glyph_w + (gx as i32);
        if let Some(slot) = cover.get_mut(idx as usize) {
            *slot = (c.clamp(0.0, 1.0) * 255.0) as u8;
        }
    });

    let half_w = glyph_w as f32 / 2.0;
    let half_h = glyph_h as f32 / 2.0;
    let theta = angle_deg.to_radians();
    let (sin_t, cos_t) = theta.sin_cos();

    // Bounding box of the rotated glyph on canvas (over-estimate by the
    // diagonal, the cheap branch is faster than a tight per-corner test).
    let r = (half_w * half_w + half_h * half_h).sqrt().ceil() as i32 + 2;
    let x_min = (cx as i32 - r).max(0);
    let y_min = (cy as i32 - r).max(0);
    let x_max = (cx as i32 + r).min(W_HI as i32 - 1);
    let y_max = (cy as i32 + r).min(H_HI as i32 - 1);

    for y in y_min..=y_max {
        for x in x_min..=x_max {
            // Inverse-rotate this canvas pixel back into the glyph's
            // local frame, then sample the coverage mask.
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            let lx = cos_t * dx + sin_t * dy + half_w;
            let ly = -sin_t * dx + cos_t * dy + half_h;
            if lx < 0.0 || ly < 0.0 {
                continue;
            }
            let ix = lx as i32;
            let iy = ly as i32;
            if ix >= glyph_w || iy >= glyph_h {
                continue;
            }
            let alpha = cover[(iy * glyph_w + ix) as usize];
            if alpha == 0 {
                continue;
            }
            blend_pixel(canvas, x, y, digit_color, alpha);
        }
    }

    Ok(())
}

// ── Pixel blend ───────────────────────────────────────────────────────────

/// Source-over alpha compositing onto an opaque canvas. The captcha is
/// always opaque (gradient fill writes alpha=255), so the formula
/// reduces to a straight `alpha`-weighted lerp; we never need to track
/// the destination's alpha.
fn blend_pixel(canvas: &mut RgbaImage, x: i32, y: i32, color: [u8; 3], alpha: u8) {
    if x < 0 || y < 0 || x >= W_HI as i32 || y >= H_HI as i32 {
        return;
    }
    let p = canvas.get_pixel_mut(x as u32, y as u32);
    let a = alpha as u32;
    let inv = 255 - a;
    p.0[0] = ((color[0] as u32 * a + p.0[0] as u32 * inv) / 255) as u8;
    p.0[1] = ((color[1] as u32 * a + p.0[1] as u32 * inv) / 255) as u8;
    p.0[2] = ((color[2] as u32 * a + p.0[2] as u32 * inv) / 255) as u8;
    // alpha stays whatever it was — gradient fills it to 255 up front.
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fonts() -> Fonts {
        Fonts::load().expect("load fonts")
    }

    #[test]
    fn renders_within_size_budget() {
        let f = fonts();
        // Sample a handful of seeds — the gradient + per-palette
        // shape counts vary the file size meaningfully (60..130 KB
        // observed). 150 KB is the worst-case ceiling with comfortable
        // headroom; well under the 5 MB Telegram photo limit.
        let mut max_size = 0;
        for seed in [0u128, 0x42, 0xdead_beef, 0xff00ff00, 0x0123_4567_89ab_cdef] {
            let bytes = render_webp(Uuid::from_u128(seed), "1234", &f).expect("render");
            assert!(!bytes.is_empty(), "non-empty (seed=0x{seed:x})");
            assert!(
                bytes.len() <= 150_000,
                "WebP must fit in 150 KB (seed=0x{seed:x}), got {}",
                bytes.len()
            );
            max_size = max_size.max(bytes.len());
        }
        eprintln!("captcha worst-case WebP size across 5 seeds: {max_size} bytes");
    }

    #[test]
    fn deterministic_for_same_inputs() {
        let id = Uuid::from_u128(0x42);
        let f = fonts();
        let a = render_webp(id, "0429", &f).expect("render a");
        let b = render_webp(id, "0429", &f).expect("render b");
        assert_eq!(a, b, "same inputs must produce identical bytes");
    }

    #[test]
    fn different_seeds_diverge() {
        let f = fonts();
        let a = render_webp(Uuid::from_u128(1), "0000", &f).expect("a");
        let b = render_webp(Uuid::from_u128(2), "0000", &f).expect("b");
        assert_ne!(a, b, "distinct UUIDs must visibly differ");
    }

    #[test]
    fn rejects_wrong_length_solution() {
        let f = fonts();
        assert!(render_webp(Uuid::nil(), "12", &f).is_err());
        assert!(render_webp(Uuid::nil(), "12345", &f).is_err());
    }

    #[test]
    fn output_dimensions_match_constants() {
        let bytes = render_webp(Uuid::from_u128(7), "1234", &fonts()).expect("render");
        // Decode header to verify we're actually emitting 480×270.
        let img = image::load_from_memory_with_format(&bytes, image::ImageFormat::WebP)
            .expect("decode webp");
        assert_eq!(img.width(), WIDTH);
        assert_eq!(img.height(), HEIGHT);
    }
}
