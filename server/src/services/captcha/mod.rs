//! Captcha pipeline: deterministic WebP renderer, digit-pad keyboard, and the
//! service that orchestrates challenge issuance / solving / expiry. See
//! `server/docs/captcha.md` for the state machine and atomicity contract.

pub mod fonts;
pub mod keyboard;
pub mod render;
pub mod service;
pub mod state;

pub use fonts::Fonts;
pub use keyboard::{OP_BACKSPACE, OP_REFRESH, ParsedCallback, digit_pad, parse_callback, short_id};
pub use render::render_webp;
pub use service::{CaptchaService, IssuedChallenge, Outcome, solution_for};
pub use state::{CaptchaState, MetaPayload, VERIFIED_CACHE_TTL_SECS};
