//! Business-logic services (captcha, spam, moderation, reports, summary).
//! Populated from M1 onwards — see `server/docs/architecture.md`.

pub mod captcha;
pub mod cas_client;
pub mod moderation_service;
pub mod spam;
