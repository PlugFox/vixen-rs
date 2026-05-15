//! Authentication primitives — Telegram WebApp `initData` HMAC validation,
//! Telegram Login Widget validation, and JWT mint/decode for the dashboard.
//!
//! Two payload shapes are accepted on `/api/v1/auth/telegram/login`:
//!
//! 1. **WebApp `initData`**: URL-encoded query string with `auth_date`,
//!    `hash`, `user` (JSON-encoded), and optional `query_id`. Signing key is
//!    `HMAC_SHA256(key="WebAppData", msg=bot_token)`. See
//!    <https://core.telegram.org/bots/webapps#validating-data-received-via-the-mini-app>.
//!
//! 2. **Telegram Login Widget**: URL-encoded query string with flat fields
//!    (`id`, `first_name`, `last_name`, `username`, `photo_url`, `auth_date`,
//!    `hash`). Signing key is `SHA256(bot_token)` (note: not HMAC — that's
//!    the legacy Login Widget protocol).
//!
//! In both cases the signed payload is verified via `HMAC_SHA256(key=secret,
//! msg=data_check_string)` where `data_check_string` is the BTreeMap-sorted
//! list of `key=value` pairs joined by `\n`, excluding `hash`.
//!
//! Successful validation yields a [`TelegramIdentity`]. The auth route then
//! looks up `chat_moderators` for that user and mints a [`JwtClaims`] token.

use std::collections::BTreeMap;

use hmac::{Hmac, Mac};
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use subtle::ConstantTimeEq;
use utoipa::ToSchema;

type HmacSha256 = Hmac<Sha256>;

/// Validated Telegram identity. Returned by [`validate_init_data`] on a
/// successful HMAC check. `auth_date` is the unix-seconds timestamp the
/// signature was minted at — already checked against `max_age_secs` by the
/// validator.
#[derive(Debug, Clone)]
pub struct TelegramIdentity {
    pub user_id: i64,
    pub username: Option<String>,
    pub first_name: String,
    pub last_name: Option<String>,
    pub language_code: Option<String>,
    pub auth_date: u64,
    pub shape: InitDataShape,
}

/// Discriminates which of the two payload shapes the validator accepted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InitDataShape {
    WebApp,
    LoginWidget,
}

/// Internal dashboard JWT. HS256, 1h TTL by default. The signing secret is
/// `CONFIG_JWT_SECRET`, distinct from the bot token — rotating the bot token
/// does NOT invalidate live JWTs.
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct JwtClaims {
    /// Telegram user id.
    pub sub: i64,
    /// Expiry unix-seconds. `jsonwebtoken` enforces this on decode.
    pub exp: i64,
    /// Issued-at unix-seconds.
    pub iat: i64,
    /// Chats this user moderates at the time the token was minted. Frozen
    /// for the JWT's lifetime — added/removed moderator rights take effect
    /// on next login (1h default).
    pub chat_ids: Vec<i64>,
    /// Telegram display info — for the dashboard UI only. Authority always
    /// derives from `sub`/`chat_ids`, never from `tg`.
    pub tg: TgDisplay,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct TgDisplay {
    pub username: Option<String>,
    pub first_name: String,
    pub last_name: Option<String>,
}

#[derive(thiserror::Error, Debug)]
pub enum AuthError {
    #[error("initData is malformed")]
    Malformed,
    #[error("initData missing `hash` field")]
    MissingHash,
    #[error("initData missing `auth_date` field")]
    MissingAuthDate,
    #[error("initData missing user fields")]
    MissingUserFields,
    #[error("initData `hash` is not valid hex")]
    BadHashFormat,
    #[error("initData shape unrecognised (neither WebApp `user` nor Login Widget `id`)")]
    UnknownShape,
    #[error("initData HMAC mismatch")]
    HmacMismatch,
    #[error("initData `auth_date` older than {max_age_secs} seconds")]
    AuthDateExpired { max_age_secs: u64 },
    #[error("JWT encode failed: {0}")]
    JwtEncode(jsonwebtoken::errors::Error),
    #[error("JWT decode failed: {0}")]
    JwtDecode(jsonwebtoken::errors::Error),
    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),
}

/// Validate a raw `initData` query string against `bot_token`.
///
/// `now` and `max_age_secs` are passed explicitly so the function stays pure:
/// callers fix the clock for tests via these arguments and don't depend on
/// `SystemTime::now()` inside.
pub fn validate_init_data(
    raw: &str,
    bot_token: &str,
    now: u64,
    max_age_secs: u64,
) -> Result<TelegramIdentity, AuthError> {
    let (params, hash) = parse_query(raw)?;
    let hash = hash.ok_or(AuthError::MissingHash)?;

    let shape = if params.contains_key("user") {
        InitDataShape::WebApp
    } else if params.contains_key("id") && params.contains_key("first_name") {
        InitDataShape::LoginWidget
    } else {
        return Err(AuthError::UnknownShape);
    };

    let dcs = build_data_check_string(&params);
    let expected = compute_expected_hash(shape, bot_token, &dcs);
    let provided = hex::decode(&hash).map_err(|_| AuthError::BadHashFormat)?;
    if expected.ct_eq(&provided).unwrap_u8() == 0 {
        return Err(AuthError::HmacMismatch);
    }

    let auth_date: u64 = params
        .get("auth_date")
        .ok_or(AuthError::MissingAuthDate)?
        .parse()
        .map_err(|_| AuthError::MissingAuthDate)?;
    if now.saturating_sub(auth_date) > max_age_secs {
        return Err(AuthError::AuthDateExpired { max_age_secs });
    }

    let identity = match shape {
        InitDataShape::WebApp => {
            let user_json = params.get("user").ok_or(AuthError::MissingUserFields)?;
            parse_webapp_user(user_json, auth_date)?
        }
        InitDataShape::LoginWidget => parse_login_widget_user(&params, auth_date)?,
    };
    Ok(identity)
}

/// Mint a dashboard JWT. Caller passes `now` so tests can pin time.
pub fn mint_jwt(
    identity: &TelegramIdentity,
    chat_ids: Vec<i64>,
    secret: &str,
    ttl_secs: i64,
    now: i64,
) -> Result<String, AuthError> {
    let claims = JwtClaims {
        sub: identity.user_id,
        exp: now + ttl_secs,
        iat: now,
        chat_ids,
        tg: TgDisplay {
            username: identity.username.clone(),
            first_name: identity.first_name.clone(),
            last_name: identity.last_name.clone(),
        },
    };
    let header = Header::new(Algorithm::HS256);
    let key = EncodingKey::from_secret(secret.as_bytes());
    encode(&header, &claims, &key).map_err(AuthError::JwtEncode)
}

/// Decode and validate a JWT (signature + `exp`).
pub fn decode_jwt(token: &str, secret: &str) -> Result<JwtClaims, AuthError> {
    let key = DecodingKey::from_secret(secret.as_bytes());
    let mut validation = Validation::new(Algorithm::HS256);
    validation.required_spec_claims.clear();
    validation.required_spec_claims.insert("exp".to_string());
    let data = decode::<JwtClaims>(token, &key, &validation).map_err(AuthError::JwtDecode)?;
    Ok(data.claims)
}

/// Resolve the set of chats this user moderates at JWT-mint time.
/// `DISTINCT` defensively guards against any duplicate `(chat_id, user_id)`
/// rows even though the PK should forbid them.
pub async fn chats_for(pool: &PgPool, user_id: i64) -> Result<Vec<i64>, AuthError> {
    let rows = sqlx::query_scalar!(
        r#"SELECT DISTINCT chat_id FROM chat_moderators WHERE user_id = $1 ORDER BY chat_id"#,
        user_id
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

// ── internals ────────────────────────────────────────────────────────────

fn parse_query(raw: &str) -> Result<(BTreeMap<String, String>, Option<String>), AuthError> {
    if raw.is_empty() {
        return Err(AuthError::Malformed);
    }
    let mut params = BTreeMap::new();
    let mut hash = None;
    for (k, v) in url::form_urlencoded::parse(raw.as_bytes()) {
        if k == "hash" {
            hash = Some(v.into_owned());
        } else {
            params.insert(k.into_owned(), v.into_owned());
        }
    }
    Ok((params, hash))
}

fn build_data_check_string(params: &BTreeMap<String, String>) -> String {
    let mut out = String::new();
    for (i, (k, v)) in params.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        out.push_str(k);
        out.push('=');
        out.push_str(v);
    }
    out
}

fn compute_expected_hash(shape: InitDataShape, bot_token: &str, dcs: &str) -> Vec<u8> {
    let secret: Vec<u8> = match shape {
        InitDataShape::WebApp => {
            let mut m =
                HmacSha256::new_from_slice(b"WebAppData").expect("HMAC accepts any key length");
            m.update(bot_token.as_bytes());
            m.finalize().into_bytes().to_vec()
        }
        InitDataShape::LoginWidget => Sha256::digest(bot_token.as_bytes()).to_vec(),
    };
    let mut m = HmacSha256::new_from_slice(&secret).expect("HMAC accepts any key length");
    m.update(dcs.as_bytes());
    m.finalize().into_bytes().to_vec()
}

#[derive(Deserialize)]
struct WebAppUser {
    id: i64,
    first_name: String,
    #[serde(default)]
    last_name: Option<String>,
    #[serde(default)]
    username: Option<String>,
    #[serde(default)]
    language_code: Option<String>,
}

fn parse_webapp_user(json: &str, auth_date: u64) -> Result<TelegramIdentity, AuthError> {
    let u: WebAppUser = serde_json::from_str(json).map_err(|_| AuthError::MissingUserFields)?;
    Ok(TelegramIdentity {
        user_id: u.id,
        username: u.username,
        first_name: u.first_name,
        last_name: u.last_name,
        language_code: u.language_code,
        auth_date,
        shape: InitDataShape::WebApp,
    })
}

fn parse_login_widget_user(
    params: &BTreeMap<String, String>,
    auth_date: u64,
) -> Result<TelegramIdentity, AuthError> {
    let user_id: i64 = params
        .get("id")
        .ok_or(AuthError::MissingUserFields)?
        .parse()
        .map_err(|_| AuthError::MissingUserFields)?;
    let first_name = params
        .get("first_name")
        .ok_or(AuthError::MissingUserFields)?
        .clone();
    Ok(TelegramIdentity {
        user_id,
        username: params.get("username").cloned(),
        first_name,
        last_name: params.get("last_name").cloned(),
        language_code: params.get("language_code").cloned(),
        auth_date,
        shape: InitDataShape::LoginWidget,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test helper: sign a WebApp initData payload as Telegram would. Returns
    /// the URL-encoded query string ready to be POSTed.
    pub(crate) fn sign_webapp(user_json: &str, bot_token: &str, auth_date: u64) -> String {
        let mut params = BTreeMap::new();
        params.insert("auth_date".to_string(), auth_date.to_string());
        params.insert("user".to_string(), user_json.to_string());
        sign_with_shape(InitDataShape::WebApp, &params, bot_token)
    }

    /// Test helper: sign a Login Widget payload.
    pub(crate) fn sign_login_widget(
        id: i64,
        first_name: &str,
        bot_token: &str,
        auth_date: u64,
    ) -> String {
        let mut params = BTreeMap::new();
        params.insert("auth_date".to_string(), auth_date.to_string());
        params.insert("id".to_string(), id.to_string());
        params.insert("first_name".to_string(), first_name.to_string());
        sign_with_shape(InitDataShape::LoginWidget, &params, bot_token)
    }

    fn sign_with_shape(
        shape: InitDataShape,
        params: &BTreeMap<String, String>,
        bot_token: &str,
    ) -> String {
        let dcs = build_data_check_string(params);
        let hash = hex::encode(compute_expected_hash(shape, bot_token, &dcs));
        let mut q = url::form_urlencoded::Serializer::new(String::new());
        for (k, v) in params {
            q.append_pair(k, v);
        }
        q.append_pair("hash", &hash);
        q.finish()
    }

    const TOKEN: &str = "12345:abcdefghijklmnopqrstuvwxyz_0-9";

    #[test]
    fn webapp_happy_path() {
        let user = r#"{"id":42,"first_name":"Mike","username":"plugfox"}"#;
        let raw = sign_webapp(user, TOKEN, 1_714_560_000);
        let id = validate_init_data(&raw, TOKEN, 1_714_560_500, 86_400).unwrap();
        assert_eq!(id.user_id, 42);
        assert_eq!(id.first_name, "Mike");
        assert_eq!(id.username.as_deref(), Some("plugfox"));
        assert_eq!(id.shape, InitDataShape::WebApp);
    }

    #[test]
    fn webapp_rejects_tampered_hash() {
        let user = r#"{"id":42,"first_name":"Mike"}"#;
        let mut raw = sign_webapp(user, TOKEN, 1_714_560_000);
        // Flip the last hex digit of the hash.
        let last = raw.pop().unwrap();
        let replacement = if last == '0' { '1' } else { '0' };
        raw.push(replacement);
        let err = validate_init_data(&raw, TOKEN, 1_714_560_500, 86_400).unwrap_err();
        assert!(matches!(
            err,
            AuthError::HmacMismatch | AuthError::BadHashFormat
        ));
    }

    #[test]
    fn webapp_rejects_expired_auth_date() {
        let user = r#"{"id":42,"first_name":"Mike"}"#;
        let raw = sign_webapp(user, TOKEN, 1_714_000_000);
        let err = validate_init_data(&raw, TOKEN, 1_714_000_000 + 86_400 + 1, 86_400).unwrap_err();
        assert!(matches!(err, AuthError::AuthDateExpired { .. }));
    }

    #[test]
    fn webapp_rejects_wrong_bot_token() {
        let user = r#"{"id":42,"first_name":"Mike"}"#;
        let raw = sign_webapp(user, TOKEN, 1_714_560_000);
        let err = validate_init_data(
            &raw,
            "99999:wrongtokenwrongtokenwrongtoken12",
            1_714_560_500,
            86_400,
        )
        .unwrap_err();
        assert!(matches!(err, AuthError::HmacMismatch));
    }

    #[test]
    fn login_widget_happy_path() {
        let raw = sign_login_widget(7, "Mike", TOKEN, 1_714_560_000);
        let id = validate_init_data(&raw, TOKEN, 1_714_560_500, 86_400).unwrap();
        assert_eq!(id.user_id, 7);
        assert_eq!(id.first_name, "Mike");
        assert_eq!(id.shape, InitDataShape::LoginWidget);
    }

    #[test]
    fn login_widget_rejects_tampered_hash() {
        let mut raw = sign_login_widget(7, "Mike", TOKEN, 1_714_560_000);
        let last = raw.pop().unwrap();
        let replacement = if last == '0' { '1' } else { '0' };
        raw.push(replacement);
        let err = validate_init_data(&raw, TOKEN, 1_714_560_500, 86_400).unwrap_err();
        assert!(matches!(
            err,
            AuthError::HmacMismatch | AuthError::BadHashFormat
        ));
    }

    #[test]
    fn unknown_shape_rejected() {
        let raw = "auth_date=1714560000&hash=00";
        let err = validate_init_data(raw, TOKEN, 1_714_560_500, 86_400).unwrap_err();
        assert!(matches!(err, AuthError::UnknownShape));
    }

    #[test]
    fn missing_hash_rejected() {
        let raw = "auth_date=1714560000&id=7&first_name=Mike";
        let err = validate_init_data(raw, TOKEN, 1_714_560_500, 86_400).unwrap_err();
        assert!(matches!(err, AuthError::MissingHash));
    }

    fn sample_identity(user_id: i64) -> TelegramIdentity {
        TelegramIdentity {
            user_id,
            username: Some("plugfox".into()),
            first_name: "Mike".into(),
            last_name: None,
            language_code: Some("ru".into()),
            auth_date: 1_714_560_000,
            shape: InitDataShape::WebApp,
        }
    }

    #[test]
    fn jwt_mint_decode_roundtrip() {
        let id = sample_identity(42);
        let secret = "x".repeat(32);
        let now = chrono::Utc::now().timestamp();
        let token = mint_jwt(&id, vec![-100, -101], &secret, 3600, now).unwrap();
        let claims = decode_jwt(&token, &secret).unwrap();
        assert_eq!(claims.sub, 42);
        assert_eq!(claims.chat_ids, vec![-100, -101]);
        assert_eq!(claims.tg.username.as_deref(), Some("plugfox"));
        assert_eq!(claims.exp - now, 3600);
    }

    #[test]
    fn jwt_rejects_wrong_secret() {
        let id = sample_identity(1);
        let now = chrono::Utc::now().timestamp();
        let token = mint_jwt(&id, vec![], &"x".repeat(32), 3600, now).unwrap();
        let err = decode_jwt(&token, &"y".repeat(32)).unwrap_err();
        assert!(matches!(err, AuthError::JwtDecode(_)));
    }

    #[test]
    fn jwt_rejects_expired() {
        let id = sample_identity(1);
        let secret = "x".repeat(32);
        // exp is several minutes in the past — well past the 60s default leeway.
        let now = chrono::Utc::now().timestamp();
        let token = mint_jwt(&id, vec![], &secret, -3600, now).unwrap();
        let err = decode_jwt(&token, &secret).unwrap_err();
        assert!(matches!(err, AuthError::JwtDecode(_)));
    }

    #[test]
    fn data_check_string_is_sorted_by_key() {
        let mut p = BTreeMap::new();
        p.insert("z".to_string(), "1".to_string());
        p.insert("a".to_string(), "2".to_string());
        p.insert("m".to_string(), "3".to_string());
        assert_eq!(build_data_check_string(&p), "a=2\nm=3\nz=1");
    }
}
