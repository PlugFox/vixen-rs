//! Shared application state passed to every handler via `axum::extract::State`
//! and to the teloxide dispatcher via `dptree::deps!`.

use std::sync::Arc;

use crate::config::Config;
use crate::database::{Database, Redis};
use crate::services::captcha::CaptchaService;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub db: Arc<Database>,
    pub redis: Arc<Redis>,
    pub captcha: Arc<CaptchaService>,
}
