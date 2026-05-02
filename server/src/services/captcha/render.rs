//! Deterministic 480×180 lossless WebP captcha renderer.
//!
//! All randomness derives from `xxh3(challenge_id)`; the same UUID + solution
//! pair always produces byte-identical output. Regenerating after a Telegram
//! retry yields the same image, and the snapshot test in
//! `tests/captcha/render.rs` pins the bytes for a fixed seed.

use ab_glyph::{Font, FontRef, PxScale, ScaleFont};
use anyhow::{Result, anyhow, ensure};
use image::{ImageBuffer, Rgba};
use uuid::Uuid;
use xxhash_rust::xxh3::xxh3_64;

use super::fonts::Fonts;

pub const WIDTH: u32 = 480;
pub const HEIGHT: u32 = 180;
const DIGIT_COUNT: u32 = 4;

/// Render the captcha for the given challenge as lossless WebP bytes.
///
/// The function is pure — no I/O, no globals — so the caller is free to wrap it
/// in `tokio::task::spawn_blocking`.
pub fn render_webp(challenge_id: Uuid, solution: &str, fonts: &Fonts) -> Result<Vec<u8>> {
    ensure!(
        solution.chars().count() == DIGIT_COUNT as usize,
        "solution must be {DIGIT_COUNT} chars, got {}",
        solution.chars().count()
    );

    let mut rng = SeededRng::from_uuid(challenge_id);
    let palette = pick_palette(&mut rng);
    let mut canvas: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::new(WIDTH, HEIGHT);

    fill_gradient(&mut canvas, palette);
    draw_curves(&mut canvas, &mut rng, palette);
    draw_dots(&mut canvas, &mut rng, palette);
    draw_digits(&mut canvas, &mut rng, palette, solution, &fonts.primary)?;

    encode_lossless_webp(&canvas)
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

#[derive(Clone, Copy)]
struct Palette {
    bg_top: [u8; 3],
    bg_bottom: [u8; 3],
    digit_colors: [[u8; 3]; 4],
    noise: [u8; 3],
}

const PALETTES: &[Palette] = &[
    Palette {
        bg_top: [22, 33, 62],
        bg_bottom: [70, 30, 90],
        digit_colors: [
            [255, 215, 130],
            [240, 130, 110],
            [255, 235, 200],
            [200, 230, 255],
        ],
        noise: [180, 180, 220],
    },
    Palette {
        bg_top: [12, 60, 70],
        bg_bottom: [10, 100, 80],
        digit_colors: [
            [255, 240, 150],
            [200, 255, 220],
            [255, 200, 220],
            [180, 220, 255],
        ],
        noise: [200, 230, 220],
    },
    Palette {
        bg_top: [50, 25, 35],
        bg_bottom: [110, 50, 60],
        digit_colors: [
            [255, 235, 210],
            [255, 200, 170],
            [240, 255, 220],
            [200, 220, 255],
        ],
        noise: [230, 200, 200],
    },
];

fn pick_palette(rng: &mut SeededRng) -> Palette {
    PALETTES[(rng.next_u64() as usize) % PALETTES.len()]
}

// ── Background ────────────────────────────────────────────────────────────

fn fill_gradient(canvas: &mut ImageBuffer<Rgba<u8>, Vec<u8>>, palette: Palette) {
    let h = HEIGHT as f32;
    for y in 0..HEIGHT {
        let t = y as f32 / (h - 1.0);
        let r = lerp_u8(palette.bg_top[0], palette.bg_bottom[0], t);
        let g = lerp_u8(palette.bg_top[1], palette.bg_bottom[1], t);
        let b = lerp_u8(palette.bg_top[2], palette.bg_bottom[2], t);
        for x in 0..WIDTH {
            canvas.put_pixel(x, y, Rgba([r, g, b, 255]));
        }
    }
}

fn lerp_u8(a: u8, b: u8, t: f32) -> u8 {
    let v = a as f32 + (b as f32 - a as f32) * t;
    v.clamp(0.0, 255.0) as u8
}

// ── Noise: thin curves + dot field ────────────────────────────────────────

fn draw_curves(canvas: &mut ImageBuffer<Rgba<u8>, Vec<u8>>, rng: &mut SeededRng, palette: Palette) {
    let count = rng.range_u32(30, 51);
    for _ in 0..count {
        let x0 = rng.range_f32(0.0, WIDTH as f32);
        let y0 = rng.range_f32(0.0, HEIGHT as f32);
        let x1 = rng.range_f32(0.0, WIDTH as f32);
        let y1 = rng.range_f32(0.0, HEIGHT as f32);
        let cx = rng.range_f32(0.0, WIDTH as f32);
        let cy = rng.range_f32(0.0, HEIGHT as f32);
        // Quadratic Bézier sampled in fixed steps for stable output.
        let steps = 80;
        for i in 0..=steps {
            let t = i as f32 / steps as f32;
            let it = 1.0 - t;
            let x = it * it * x0 + 2.0 * it * t * cx + t * t * x1;
            let y = it * it * y0 + 2.0 * it * t * cy + t * t * y1;
            blend_pixel(canvas, x as i32, y as i32, palette.noise, 70);
        }
    }
}

fn draw_dots(canvas: &mut ImageBuffer<Rgba<u8>, Vec<u8>>, rng: &mut SeededRng, palette: Palette) {
    let count = rng.range_u32(220, 320);
    for _ in 0..count {
        let x = rng.range_i32(0, WIDTH as i32);
        let y = rng.range_i32(0, HEIGHT as i32);
        blend_pixel(canvas, x, y, palette.noise, 100);
    }
}

// ── Digits ────────────────────────────────────────────────────────────────

fn draw_digits(
    canvas: &mut ImageBuffer<Rgba<u8>, Vec<u8>>,
    rng: &mut SeededRng,
    palette: Palette,
    solution: &str,
    font: &FontRef<'static>,
) -> Result<()> {
    let cell_w = WIDTH as f32 / DIGIT_COUNT as f32;
    let center_y = HEIGHT as f32 / 2.0;

    for (i, c) in solution.chars().enumerate() {
        let scale_px = rng.range_f32(108.0, 132.0);
        let angle_deg = rng.range_f32(-15.0, 15.0);
        let dx_jitter = rng.range_f32(-6.0, 6.0);
        let dy_jitter = rng.range_f32(-10.0, 10.0);
        let color = palette.digit_colors[i % palette.digit_colors.len()];

        let px = cell_w * (i as f32 + 0.5) + dx_jitter;
        let py = center_y + dy_jitter;

        rasterize_digit(canvas, font, c, scale_px, angle_deg, px, py, color)?;
    }

    Ok(())
}

/// Rasterize a single glyph onto `canvas` at `(cx, cy)` (the glyph's centre)
/// with a rotation of `angle_deg`. Coverage from `ab_glyph` is blended onto
/// the existing pixel; the colour is `digit_color` (RGB, full alpha).
#[allow(clippy::too_many_arguments)]
fn rasterize_digit(
    canvas: &mut ImageBuffer<Rgba<u8>, Vec<u8>>,
    font: &FontRef<'static>,
    c: char,
    scale_px: f32,
    angle_deg: f32,
    cx: f32,
    cy: f32,
    digit_color: [u8; 3],
) -> Result<()> {
    let scale = PxScale::from(scale_px);
    let scaled = font.as_scaled(scale);
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

    // Centre offset inside the glyph buffer — we want the glyph centred on cx/cy.
    let half_w = glyph_w as f32 / 2.0;
    let half_h = scaled.ascent() / 2.0; // visual centre, not bbox centre
    let _ = half_h; // suppress unused warning if future tweaks remove it
    let half_h = (glyph_h as f32) / 2.0;

    let theta = angle_deg.to_radians();
    let (sin_t, cos_t) = theta.sin_cos();

    // Determine the canvas bounding box that the rotated glyph can touch.
    let r = (half_w * half_w + half_h * half_h).sqrt().ceil() as i32 + 2;
    let x_min = (cx as i32 - r).max(0);
    let y_min = (cy as i32 - r).max(0);
    let x_max = (cx as i32 + r).min(WIDTH as i32 - 1);
    let y_max = (cy as i32 + r).min(HEIGHT as i32 - 1);

    for y in y_min..=y_max {
        for x in x_min..=x_max {
            // Inverse-rotate this canvas pixel back into the glyph's local frame.
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

fn blend_pixel(
    canvas: &mut ImageBuffer<Rgba<u8>, Vec<u8>>,
    x: i32,
    y: i32,
    color: [u8; 3],
    alpha: u8,
) {
    if x < 0 || y < 0 || x >= WIDTH as i32 || y >= HEIGHT as i32 {
        return;
    }
    let p = canvas.get_pixel_mut(x as u32, y as u32);
    let a = alpha as u32;
    let inv = 255 - a;
    p.0[0] = ((color[0] as u32 * a + p.0[0] as u32 * inv) / 255) as u8;
    p.0[1] = ((color[1] as u32 * a + p.0[1] as u32 * inv) / 255) as u8;
    p.0[2] = ((color[2] as u32 * a + p.0[2] as u32 * inv) / 255) as u8;
    // alpha stays 255: the captcha is opaque
}

// ── WebP encode ───────────────────────────────────────────────────────────

fn encode_lossless_webp(canvas: &ImageBuffer<Rgba<u8>, Vec<u8>>) -> Result<Vec<u8>> {
    let encoder = webp::Encoder::from_rgba(canvas.as_raw(), WIDTH, HEIGHT);
    let memory = encoder.encode_lossless();
    Ok(memory.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fonts() -> Fonts {
        Fonts::load().expect("load fonts")
    }

    #[test]
    fn renders_within_size_budget() {
        let id = Uuid::nil();
        let bytes = render_webp(id, "1234", &fonts()).expect("render");
        assert!(!bytes.is_empty(), "non-empty");
        assert!(
            bytes.len() <= 30_000,
            "WebP must fit in 30 KB, got {}",
            bytes.len()
        );
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
}
