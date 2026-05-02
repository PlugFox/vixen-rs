//! Shared application state passed to every handler via `axum::extract::State`
//! and to the teloxide dispatcher via `dptree::deps!`.

use std::sync::Arc;

use crate::config::Config;
use crate::database::{Database, Redis};
use crate::services::captcha::{CaptchaService, CaptchaState};

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub db: Arc<Database>,
    pub redis: Arc<Redis>,
    pub captcha: Arc<CaptchaService>,
    /// Ephemeral captcha state in Redis: in-progress digit input,
    /// callback meta (owner + uuid_short keyed by message), and the
    /// `is_verified` cache. PG owns the durable challenge row and
    /// verified-user ledger; this is the UI scratchpad alongside it.
    pub captcha_state: Arc<CaptchaState>,
}
