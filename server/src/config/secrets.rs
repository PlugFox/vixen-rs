//! Secret newtypes — wrap sensitive strings so they never leak via `Display`,
//! `Debug` or `tracing` field formatting. Call `.expose()` only at audited
//! sites (bot client constructor, JWT signer, HMAC keying).

use std::fmt;
use std::str::FromStr;

macro_rules! secret_newtype {
    ($name:ident, $doc:expr) => {
        #[doc = $doc]
        #[derive(Clone, serde::Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            pub fn new(s: impl Into<String>) -> Self {
                Self(s.into())
            }
            /// Borrow the raw secret. ONLY for audited call sites.
            pub fn expose(&self) -> &str {
                &self.0
            }
            /// Length in bytes — safe to log; useful for `JwtSecret >= 32` checks.
            pub fn len(&self) -> usize {
                self.0.len()
            }
            pub fn is_empty(&self) -> bool {
                self.0.is_empty()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("***redacted***")
            }
        }

        impl fmt::Debug for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}(***redacted***)", stringify!($name))
            }
        }

        impl From<String> for $name {
            fn from(s: String) -> Self {
                Self(s)
            }
        }

        impl FromStr for $name {
            type Err = std::convert::Infallible;
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Ok(Self(s.to_owned()))
            }
        }
    };
}

secret_newtype!(BotToken, "Telegram bot token from @BotFather.");
secret_newtype!(
    JwtSecret,
    "HS256 signing secret for dashboard JWTs (≥32 bytes in prod)."
);
secret_newtype!(AdminSecret, "Shared bearer for /admin/* endpoints.");
secret_newtype!(
    OpenAiKey,
    "OpenAI API key for the daily-report summary feature."
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_redacts() {
        let t = BotToken::new("12345:abcdefXYZsecretvaluehere1234567890");
        assert_eq!(format!("{t}"), "***redacted***");
        assert_eq!(format!("{t:?}"), "BotToken(***redacted***)");
    }

    #[test]
    fn expose_returns_raw() {
        let s = JwtSecret::new("super-secret-32-byte-jwt-signing-key");
        assert_eq!(s.expose(), "super-secret-32-byte-jwt-signing-key");
        assert!(s.len() >= 32);
    }

    #[test]
    fn parses_from_env_string() {
        let parsed: AdminSecret = "topsecret".parse().expect("infallible");
        assert_eq!(parsed.expose(), "topsecret");
    }
}
