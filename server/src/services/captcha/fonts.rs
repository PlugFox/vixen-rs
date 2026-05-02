//! Embedded TrueType fonts for the captcha renderer. Fonts are bundled at
//! compile time via `include_bytes!` so the binary needs no filesystem access
//! at runtime — and any swap is forced through `assets/captcha/CHANGELOG`.

use ab_glyph::FontRef;
use anyhow::{Context, Result};

const PRIMARY_TTF: &[u8] = include_bytes!("../../../assets/captcha/DejaVuSans.ttf");

#[derive(Clone)]
pub struct Fonts {
    pub primary: FontRef<'static>,
}

impl Fonts {
    pub fn load() -> Result<Self> {
        let primary = FontRef::try_from_slice(PRIMARY_TTF).context("load DejaVuSans.ttf")?;
        Ok(Self { primary })
    }
}
