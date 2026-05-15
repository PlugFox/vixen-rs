//! Opaque cursor for keyset pagination. The cursor is a base64-encoded JSON
//! tuple — the client treats it as a black box and round-trips it verbatim.
//!
//! Two pagination shapes are needed by the M5 moderation API:
//!
//! 1. `(created_at, id)` for `moderation_actions` — tied rows in the
//!    `created_at DESC` order are broken by `id DESC`.
//! 2. `(verified_at, user_id)` for `verified_users` — tied rows in the
//!    `verified_at DESC` order are broken by `user_id DESC`.
//!
//! Both shapes serialise via [`encode`] / [`decode`] generic over `T: Serialize
//! + Deserialize`. The JSON is the truth — base64 is purely transport.

use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Serialize, de::DeserializeOwned};

#[derive(Debug, thiserror::Error)]
pub enum CursorError {
    #[error("cursor is not valid base64")]
    BadBase64,
    #[error("cursor JSON did not decode to the expected shape")]
    BadJson,
}

/// Encode a tuple-like value into an opaque cursor string. The result is
/// safe for query-string use (URL-safe base64, no padding).
pub fn encode<T: Serialize>(value: &T) -> String {
    let json = serde_json::to_vec(value).expect("cursor value is always serialisable");
    URL_SAFE_NO_PAD.encode(json)
}

/// Decode an opaque cursor. Returns `CursorError` for any malformed input —
/// route handlers map this to `400 Bad Request`.
pub fn decode<T: DeserializeOwned>(s: &str) -> Result<T, CursorError> {
    let bytes = URL_SAFE_NO_PAD
        .decode(s.as_bytes())
        .map_err(|_| CursorError::BadBase64)?;
    serde_json::from_slice(&bytes).map_err(|_| CursorError::BadJson)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, TimeZone, Utc};
    use uuid::Uuid;

    #[test]
    fn roundtrip_datetime_uuid() {
        let ts: DateTime<Utc> = Utc.timestamp_opt(1_780_000_000, 123_456_789).unwrap();
        let id = Uuid::parse_str("123e4567-e89b-12d3-a456-426614174000").unwrap();
        let cursor = encode(&(ts, id));
        let (decoded_ts, decoded_id): (DateTime<Utc>, Uuid) = decode(&cursor).unwrap();
        assert_eq!(decoded_ts, ts);
        assert_eq!(decoded_id, id);
    }

    #[test]
    fn roundtrip_datetime_i64() {
        let ts: DateTime<Utc> = Utc.timestamp_opt(1_780_000_001, 0).unwrap();
        let id = -1_001_234_567_890_i64;
        let cursor = encode(&(ts, id));
        let (decoded_ts, decoded_id): (DateTime<Utc>, i64) = decode(&cursor).unwrap();
        assert_eq!(decoded_ts, ts);
        assert_eq!(decoded_id, id);
    }

    #[test]
    fn rejects_garbage() {
        let err = decode::<(DateTime<Utc>, i64)>("not!valid!base64").unwrap_err();
        assert!(matches!(err, CursorError::BadBase64));
    }

    #[test]
    fn rejects_wrong_shape() {
        let json_only_one = serde_json::to_vec(&("hello",)).unwrap();
        let bad_cursor = URL_SAFE_NO_PAD.encode(json_only_one);
        let err = decode::<(DateTime<Utc>, i64)>(&bad_cursor).unwrap_err();
        assert!(matches!(err, CursorError::BadJson));
    }
}
