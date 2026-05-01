use std::sync::Arc;

use vixen::captcha::CaptchaService;

// Generate captcha.webp image
// cargo run --bin captcha
#[tokio::main]
async fn main() {
    let service = Arc::new(CaptchaService::new());
    let cap = service.generate_with_transparency(0).await; // 0..255 - 0 is transparent, 255 is opaque
    std::fs::write("captcha.webp", &cap.bytes).expect("Failed to write file");
    println!("captcha text: {}", cap.text);
}
