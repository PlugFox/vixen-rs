//! Business-logic services (captcha, spam, moderation, reports, summary).
//! Populated from M1 onwards — see `server/docs/architecture.md`.

pub mod captcha;
pub mod cas_client;
pub mod chart_service;
pub mod moderation_service;
pub mod openai_client;
pub mod report_render;
pub mod report_service;
pub mod spam;
pub mod summary_service;
