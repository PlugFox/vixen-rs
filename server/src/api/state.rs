//! Shared application state passed to every handler via `axum::extract::State`.

use std::sync::Arc;

use crate::config::Config;
use crate::database::{Database, Redis};

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub db: Arc<Database>,
    pub redis: Arc<Redis>,
}
