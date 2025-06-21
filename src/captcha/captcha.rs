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

        // Generate 6 random digits
        let digits_count: u8 = self.length as u8;
        let numbers: Vec<u8> = (0..digits_count).map(|_| rng.random_range(0..10)).collect();
        let text: String = numbers.iter().map(|&n| (n + b'0') as char).collect();

        // Create a new image with white background
        let mut img = RgbaImage::new(width, height);
        for pixel in img.pixels_mut() {
            *pixel = Rgba([255, 255, 255, 255]);
        }

        // Add random noise to the image as colored pixels
        for _ in 0..1000 {
            let x = rng.random_range(0..width);
            let y = rng.random_range(0..height);
            let color = Rgba([
                rng.random_range(0..=255),
                rng.random_range(0..=255),
                rng.random_range(0..=255),
                255,
            ]);
            img.put_pixel(x, y, color);
        }

        // Draw the CAPTCHA text on the image
        let scale = Scale::uniform(50.0);
        let start_x = 20.0;
        let start_y = 70.0;
        for (i, ch) in text.chars().enumerate() {
            let glyph = self
                .font
                .glyph(ch)
                .scaled(scale)
                .positioned(point(start_x + i as f32 * 25.0, start_y));
            if let Some(bb) = glyph.pixel_bounding_box() {
                glyph.draw(|gx, gy, v| {
                    let x = bb.min.x + gx as i32;
                    let y = bb.min.y + gy as i32;
                    if x >= 0 && x < width as i32 && y >= 0 && y < height as i32 {
                        let pixel = img.get_pixel_mut(x as u32, y as u32);
                        let alpha = (v * 255.0) as u8;
                        *pixel = Rgba([0, 0, 0, alpha]);
                    }
                });
            }
        }

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
}
