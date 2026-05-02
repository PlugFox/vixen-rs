//! Spam pipeline: normalize → xxh3-64 dedup → CAS lookup → n-gram phrase
//! match (weighted score) → Allow / Delete / Ban verdict. The handler dispatches
//! verdicts through `ModerationService::apply` so the ledger stays the single
//! source of truth.
//!
//! See `server/docs/spam-detection.md`.

pub mod dedup;
pub mod normalize;
pub mod phrases;
pub mod service;
