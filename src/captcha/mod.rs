use image::{Rgba, RgbaImage};
use rand::Rng;
use rusttype::{Font, Scale, point};
use serde::Serialize;
use tokio::sync::Mutex;
//use webp::Encoder;

/// Captcha entity representing a CAPTCHA image
#[derive(Debug, Serialize)]
pub struct Captcha {
    pub numbers: Vec<u8>,
    pub text: String,
    pub bytes: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

/// Captcha service for generating and serving CAPTCHA images
pub struct CaptchaService {
    mutex: Mutex<()>,
    length: usize, // Length of the CAPTCHA numbers
    width: u32,
    height: u32,
    font: Font<'static>,
}

impl Default for CaptchaService {
    fn default() -> Self {
        Self::new()
    }
}

impl CaptchaService {
    /// Creates a new instance of the CaptchaService
    pub fn new() -> Self {
        let font_data: &'static [u8] = include_bytes!("assets/DejaVuSans.ttf");
        let font = Font::try_from_bytes(font_data).expect("Failed to load font");
        Self {
            mutex: Mutex::new(()),
            length: 4,   // Default length of CAPTCHA numbers
            width: 480,  // Default width of CAPTCHA image
            height: 180, // Default height of CAPTCHA image
            font,
        }
    }
    /// Generates a new CAPTCHA image
    pub async fn generate(&self) -> Captcha {
        // Lock the mutex to ensure thread safety and generate only one CAPTCHA at a time
        let _lock = self.mutex.lock().await;

        // Set the dimensions of the CAPTCHA image
        let width = self.width;
        let height = self.height;
        let mut rng = rand::rng();

        // Generate random digits
        let digits_count: u8 = self.length as u8;
        let numbers: Vec<u8> = (0..digits_count).map(|_| rng.random_range(0..10)).collect();
        let text: String = numbers.iter().map(|&n| (n + b'0') as char).collect(); // Create a new image with transparent background
        let mut img = RgbaImage::new(width, height);
        // Initialize with transparent pixels
        for pixel in img.pixels_mut() {
            *pixel = Rgba([0, 0, 0, 0]); // Fully transparent
        }

        self.draw_gradient_background(&mut img, &mut rng);

        // Draw background geometric shapes
        self.draw_background_shapes(&mut img, &mut rng);

        // Draw the CAPTCHA text with rotation and offset
        self.draw_captcha_text(&mut img, &text, &mut rng);

        // Draw foreground geometric shapes (semi-transparent)
        self.draw_foreground_shapes(&mut img, &mut rng);

        // Encode to WebP format
        let mut bytes: Vec<u8> = Vec::new();
        image::codecs::webp::WebPEncoder::new_lossless(&mut bytes)
            .encode(&img, width, height, image::ExtendedColorType::Rgba8)
            .expect("Failed to encode image to WebP");

        Captcha {
            numbers,
            text,
            bytes,
            width,
            height,
        }
    }
    /// Generates a new CAPTCHA image with configurable background transparency
    pub async fn generate_with_transparency(&self, background_alpha: u8) -> Captcha {
        // Lock the mutex to ensure thread safety and generate only one CAPTCHA at a time
        let _lock = self.mutex.lock().await;

        // Set the dimensions of the CAPTCHA image
        let width = self.width;
        let height = self.height;
        let mut rng = rand::rng();

        // Generate random digits
        let digits_count: u8 = self.length as u8;
        let numbers: Vec<u8> = (0..digits_count).map(|_| rng.random_range(0..10)).collect();
        let text: String = numbers.iter().map(|&n| (n + b'0') as char).collect();

        // Create a new image with transparent background
        let mut img = RgbaImage::new(width, height);
        // Initialize with transparent pixels
        for pixel in img.pixels_mut() {
            *pixel = Rgba([0, 0, 0, 0]); // Fully transparent
        }

        self.draw_gradient_background_with_alpha(&mut img, &mut rng, background_alpha);

        // Draw background geometric shapes
        self.draw_background_shapes(&mut img, &mut rng);

        // Draw the CAPTCHA text with rotation and offset
        self.draw_captcha_text(&mut img, &text, &mut rng);

        // Draw foreground geometric shapes (semi-transparent)
        self.draw_foreground_shapes(&mut img, &mut rng);

        // Encode to WebP format
        let mut bytes: Vec<u8> = Vec::new();
        image::codecs::webp::WebPEncoder::new_lossless(&mut bytes)
            .encode(&img, width, height, image::ExtendedColorType::Rgba8)
            .expect("Failed to encode image to WebP");

        Captcha {
            numbers,
            text,
            bytes,
            width,
            height,
        }
    }

    /// Draw gradient background with transparency
    fn draw_gradient_background(&self, img: &mut RgbaImage, rng: &mut impl Rng) {
        let width = img.width();
        let height = img.height();

        // Choose random gradient colors with transparency (beautiful clean colors)
        let gradient_pairs = [
            (([135, 206, 235], 180), ([255, 182, 193], 180)), // Sky blue to light pink
            (([255, 218, 185], 180), ([255, 160, 122], 180)), // Peach to salmon
            (([221, 160, 221], 180), ([230, 230, 250], 180)), // Plum to lavender
            (([173, 216, 230], 180), ([255, 255, 224], 180)), // Light blue to light yellow
            (([255, 228, 225], 180), ([255, 218, 185], 180)), // Misty rose to peach
            (([240, 248, 255], 180), ([230, 230, 250], 180)), // Alice blue to lavender
        ];

        let ((start_color, start_alpha), (end_color, end_alpha)) =
            gradient_pairs[rng.random_range(0..gradient_pairs.len())];

        for y in 0..height {
            let ratio = y as f32 / height as f32;
            let r = (start_color[0] as f32 * (1.0 - ratio) + end_color[0] as f32 * ratio) as u8;
            let g = (start_color[1] as f32 * (1.0 - ratio) + end_color[1] as f32 * ratio) as u8;
            let b = (start_color[2] as f32 * (1.0 - ratio) + end_color[2] as f32 * ratio) as u8;
            let a = (start_alpha as f32 * (1.0 - ratio) + end_alpha as f32 * ratio) as u8;

            for x in 0..width {
                self.blend_pixel(img, x, y, [r, g, b, a]);
            }
        }
    }
    /// Draw gradient background with custom alpha
    fn draw_gradient_background_with_alpha(
        &self,
        img: &mut RgbaImage,
        rng: &mut impl Rng,
        custom_alpha: u8,
    ) {
        let width = img.width();
        let height = img.height();

        // Choose random gradient colors with custom transparency
        let gradient_pairs = [
            ([135, 206, 235], [255, 182, 193]), // Sky blue to light pink
            ([255, 218, 185], [255, 160, 122]), // Peach to salmon
            ([221, 160, 221], [230, 230, 250]), // Plum to lavender
            ([173, 216, 230], [255, 255, 224]), // Light blue to light yellow
            ([255, 228, 225], [255, 218, 185]), // Misty rose to peach
            ([240, 248, 255], [230, 230, 250]), // Alice blue to lavender
        ];

        let (start_color, end_color) = gradient_pairs[rng.random_range(0..gradient_pairs.len())];

        for y in 0..height {
            let ratio = y as f32 / height as f32;
            let r = (start_color[0] as f32 * (1.0 - ratio) + end_color[0] as f32 * ratio) as u8;
            let g = (start_color[1] as f32 * (1.0 - ratio) + end_color[1] as f32 * ratio) as u8;
            let b = (start_color[2] as f32 * (1.0 - ratio) + end_color[2] as f32 * ratio) as u8;

            for x in 0..width {
                self.blend_pixel(img, x, y, [r, g, b, custom_alpha]);
            }
        }
    }
    /// Draw background geometric shapes
    fn draw_background_shapes(&self, img: &mut RgbaImage, rng: &mut impl Rng) {
        // Beautiful clean colors for shapes
        let colors = [
            [255, 99, 132, 80],  // Red with transparency
            [54, 162, 235, 80],  // Blue with transparency
            [255, 205, 86, 80],  // Yellow with transparency
            [75, 192, 192, 80],  // Teal with transparency
            [153, 102, 255, 80], // Purple with transparency
            [255, 159, 64, 80],  // Orange with transparency
        ];

        // Draw 18-24 large background shapes
        for _ in 0..rng.random_range(18..=24) {
            let shape_type = rng.random_range(0..3);
            let color = colors[rng.random_range(0..colors.len())];

            match shape_type {
                0 => self.draw_circle(img, rng, color, false),
                1 => self.draw_rectangle(img, rng, color, false),
                _ => self.draw_line(img, rng, color, false),
            }
        }
    }

    /// Draw CAPTCHA text with rotation and offset
    fn draw_captcha_text(&self, img: &mut RgbaImage, text: &str, rng: &mut impl Rng) {
        let width = img.width();
        let height = img.height();
        let digits_count = text.len();

        // Calculate font size and spacing
        let font_size = (height as f32 * 0.4).min(width as f32 / digits_count as f32 * 0.8);
        let scale = Scale::uniform(font_size);

        let total_text_width = width as f32 * 0.8;
        let char_spacing = total_text_width / digits_count as f32;
        let start_x = (width as f32 - total_text_width) / 2.0;

        for (i, ch) in text.chars().enumerate() {
            // Random offset and rotation for each character
            let x_offset = rng.random_range(-25.0..=25.0);
            let y_offset = rng.random_range(-25.0..=25.0);
            let rotation = rng.random_range(-0.45..=0.45); // Rotation in radians

            let base_x = start_x + i as f32 * char_spacing + char_spacing * 0.1;
            let base_y = height as f32 * 0.6;

            // Create multiple text colors for better visibility
            let text_colors = [
                [40, 40, 40], // Dark gray
                [80, 80, 80], // Medium gray
                [60, 60, 60], // Gray
            ];
            let text_color = text_colors[rng.random_range(0..text_colors.len())];

            self.draw_rotated_char(
                img,
                ch,
                base_x + x_offset,
                base_y + y_offset,
                rotation,
                scale,
                text_color,
            );
        }
    }

    /// Draw foreground geometric shapes (semi-transparent)
    fn draw_foreground_shapes(&self, img: &mut RgbaImage, rng: &mut impl Rng) {
        let colors = [
            [255, 255, 255, 40], // White with low transparency
            [200, 200, 200, 50], // Light gray with low transparency
            [180, 180, 180, 45], // Gray with low transparency
        ];

        // Draw 18-24 foreground shapes
        for _ in 0..rng.random_range(18..=24) {
            let shape_type = rng.random_range(0..3);
            let color = colors[rng.random_range(0..colors.len())];

            match shape_type {
                0 => self.draw_circle(img, rng, color, true),
                1 => self.draw_rectangle(img, rng, color, true),
                _ => self.draw_line(img, rng, color, true),
            }
        }
    }

    /// Draw a circle
    fn draw_circle(
        &self,
        img: &mut RgbaImage,
        rng: &mut impl Rng,
        color: [u8; 4],
        is_foreground: bool,
    ) {
        let width = img.width() as i32;
        let height = img.height() as i32;
        let base_radius = height.min(width);

        let radius = if is_foreground {
            rng.random_range(16 + base_radius / 8..=16 + base_radius / 4)
        } else {
            rng.random_range(16 + base_radius / 4..=16 + base_radius / 2)
        };

        let center_x = rng.random_range(0..width);
        let center_y = rng.random_range(0..height);

        for y in (center_y - radius).max(0)..(center_y + radius).min(height) {
            for x in (center_x - radius).max(0)..(center_x + radius).min(width) {
                let dx = x - center_x;
                let dy = y - center_y;
                if dx * dx + dy * dy <= radius * radius {
                    self.blend_pixel(img, x as u32, y as u32, color);
                }
            }
        }
    }

    /// Draw a rectangle
    fn draw_rectangle(
        &self,
        img: &mut RgbaImage,
        rng: &mut impl Rng,
        color: [u8; 4],
        is_foreground: bool,
    ) {
        let width = img.width() as i32;
        let height = img.height() as i32;

        let rect_width = if is_foreground {
            rng.random_range(width / 8..=width / 4)
        } else {
            rng.random_range(width / 4..=width / 2)
        };

        let rect_height = if is_foreground {
            rng.random_range(height / 8..=height / 4)
        } else {
            rng.random_range(height / 4..=height / 2)
        };

        let start_x = rng.random_range(0..width - rect_width);
        let start_y = rng.random_range(0..height - rect_height);

        for y in start_y..(start_y + rect_height).min(height) {
            for x in start_x..(start_x + rect_width).min(width) {
                self.blend_pixel(img, x as u32, y as u32, color);
            }
        }
    }

    /// Draw a line
    fn draw_line(
        &self,
        img: &mut RgbaImage,
        rng: &mut impl Rng,
        color: [u8; 4],
        is_foreground: bool,
    ) {
        let width = img.width() as i32;
        let height = img.height() as i32;

        let x1 = rng.random_range(0..width);
        let y1 = rng.random_range(0..height);
        let x2 = rng.random_range(0..width);
        let y2 = rng.random_range(0..height);

        let thickness = if is_foreground {
            rng.random_range(1..=4)
        } else {
            rng.random_range(4..=8)
        };

        self.draw_thick_line(img, x1, y1, x2, y2, thickness, color);
    }

    /// Draw a thick line using Bresenham's algorithm
    #[allow(clippy::too_many_arguments)]
    fn draw_thick_line(
        &self,
        img: &mut RgbaImage,
        x0: i32,
        y0: i32,
        x1: i32,
        y1: i32,
        thickness: i32,
        color: [u8; 4],
    ) {
        let dx = (x1 - x0).abs();
        let dy = (y1 - y0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx - dy;
        let mut x = x0;
        let mut y = y0;

        loop {
            // Draw thick point
            for dy in -thickness / 2..=thickness / 2 {
                for dx in -thickness / 2..=thickness / 2 {
                    let px = x + dx;
                    let py = y + dy;
                    if px >= 0 && px < img.width() as i32 && py >= 0 && py < img.height() as i32 {
                        self.blend_pixel(img, px as u32, py as u32, color);
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

    /// Draw rotated character
    #[allow(clippy::too_many_arguments)]
    fn draw_rotated_char(
        &self,
        img: &mut RgbaImage,
        ch: char,
        x: f32,
        y: f32,
        rotation: f32,
        scale: Scale,
        color: [u8; 3],
    ) {
        let glyph = self.font.glyph(ch).scaled(scale).positioned(point(x, y));

        if let Some(bb) = glyph.pixel_bounding_box() {
            glyph.draw(|gx, gy, v| {
                if v > 0.1 {
                    // Apply rotation
                    let rel_x = (gx as f32) - (bb.width() as f32 / 2.0);
                    let rel_y = (gy as f32) - (bb.height() as f32 / 2.0);

                    let cos_r = rotation.cos();
                    let sin_r = rotation.sin();

                    let rotated_x = rel_x * cos_r - rel_y * sin_r;
                    let rotated_y = rel_x * sin_r + rel_y * cos_r;

                    let final_x = bb.min.x as f32 + rotated_x + (bb.width() as f32 / 2.0);
                    let final_y = bb.min.y as f32 + rotated_y + (bb.height() as f32 / 2.0);

                    let px = final_x as i32;
                    let py = final_y as i32;

                    if px >= 0 && px < img.width() as i32 && py >= 0 && py < img.height() as i32 {
                        let alpha = (v * 255.0) as u8;
                        self.blend_pixel(
                            img,
                            px as u32,
                            py as u32,
                            [color[0], color[1], color[2], alpha],
                        );
                    }
                }
            });
        }
    }
    /// Blend pixel with alpha (proper alpha compositing)
    fn blend_pixel(&self, img: &mut RgbaImage, x: u32, y: u32, color: [u8; 4]) {
        if x >= img.width() || y >= img.height() {
            return;
        }

        let pixel = img.get_pixel_mut(x, y);
        let src_alpha = color[3] as f32 / 255.0;
        let dst_alpha = pixel[3] as f32 / 255.0;

        // Alpha compositing formula: out_alpha = src_alpha + dst_alpha * (1 - src_alpha)
        let out_alpha = src_alpha + dst_alpha * (1.0 - src_alpha);

        if out_alpha > 0.0 {
            // Premultiplied alpha blending
            let new_r = ((color[0] as f32 * src_alpha
                + pixel[0] as f32 * dst_alpha * (1.0 - src_alpha))
                / out_alpha) as u8;
            let new_g = ((color[1] as f32 * src_alpha
                + pixel[1] as f32 * dst_alpha * (1.0 - src_alpha))
                / out_alpha) as u8;
            let new_b = ((color[2] as f32 * src_alpha
                + pixel[2] as f32 * dst_alpha * (1.0 - src_alpha))
                / out_alpha) as u8;
            let new_a = (out_alpha * 255.0) as u8;

            *pixel = Rgba([new_r, new_g, new_b, new_a]);
        } else {
            *pixel = Rgba([color[0], color[1], color[2], color[3]]);
        }
    }
}
