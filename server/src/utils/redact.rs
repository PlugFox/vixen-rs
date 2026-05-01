//! Tracing-only redaction wrappers for raw secret strings. Use them in
//! `tracing::*!()` field expressions when the caller has access to the raw
//! `&str` but must not leak it to logs.
//!
//! These are *display* helpers — distinct from the *owning* secret newtypes in
//! `crate::config::secrets` (`BotToken`, `JwtSecret`, …). The newtypes redact to
//! `***redacted***`; these wrappers preserve the public correlator (e.g. the
//! bot ID before the colon) so log lines remain searchable.

use std::fmt;

/// Wraps a borrowed Telegram bot token. `Display` prints `<id>:****` if the
/// string contains `:`, otherwise `****`.
pub struct RedactedToken<'a>(pub &'a str);

impl fmt::Display for RedactedToken<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0.split_once(':') {
            Some((id, _)) if !id.is_empty() => write!(f, "{id}:****"),
            _ => f.write_str("****"),
        }
    }
}

impl fmt::Debug for RedactedToken<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_with_colon_keeps_id() {
        assert_eq!(
            format!("{}", RedactedToken("12345:abcdefXYZ")),
            "12345:****"
        );
    }

    #[test]
    fn token_format_ends_with_redaction_marker() {
        // Per #23: the formatted string ends with 4 obscured chars only.
        let s = format!("{}", RedactedToken("12345:abcdefXYZ"));
        assert!(s.ends_with("****"), "got {s:?}");
    }

    #[test]
    fn token_without_colon_fully_redacts() {
        assert_eq!(format!("{}", RedactedToken("plainstring")), "****");
    }

    #[test]
    fn empty_id_falls_back() {
        assert_eq!(format!("{}", RedactedToken(":secret")), "****");
    }

    #[test]
    fn debug_matches_display() {
        let t = RedactedToken("123:abc");
        assert_eq!(format!("{t:?}"), format!("{t}"));
    }

    #[test]
    fn empty_string_redacts() {
        assert_eq!(format!("{}", RedactedToken("")), "****");
    }
}
