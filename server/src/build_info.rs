//! Compile-time build metadata. Populated by `build.rs`.

pub const NAME: &str = env!("CARGO_PKG_NAME");
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const GIT_HASH: &str = env!("VIXEN_GIT_HASH");
pub const BUILD_DATE: &str = env!("VIXEN_BUILD_DATE");
pub const RUST_VERSION: &str = env!("VIXEN_RUST_VERSION");
pub const BUILD_PROFILE: &str = env!("VIXEN_BUILD_PROFILE");
pub const BUILD_TARGET: &str = env!("VIXEN_BUILD_TARGET");

#[cfg(debug_assertions)]
pub const IS_DEV: bool = true;
#[cfg(not(debug_assertions))]
pub const IS_DEV: bool = false;
