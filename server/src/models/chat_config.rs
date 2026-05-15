//! `chat_config` — per-chat tunables. Source of truth for captcha policy,
//! spam thresholds, report hour / timezone, OpenAI key + model, language.
//! Hot-reloaded via `Redis` pub/sub on `chat_config:{chat_id}`.

use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize};
use sqlx::FromRow;
use utoipa::ToSchema;

/// Full DB row, used by `ChatConfigService` callers and the dashboard API
/// alike. Note: `openai_api_key` is **never** serialized to the API — the
/// dashboard sees only `openai_api_key_set` in `ChatConfigDto`.
///
/// `Debug` is implemented manually to redact `openai_api_key` (any
/// `tracing::debug!(?cfg)` would otherwise leak the key — `derive(Debug)`
/// would print the raw `Some("sk-…")` form).
#[derive(Clone, FromRow)]
pub struct ChatConfig {
    pub chat_id: i64,
    pub captcha_enabled: bool,
    pub captcha_lifetime_secs: i32,
    pub captcha_attempts: i16,
    pub spam_enabled: bool,
    pub spam_threshold: f32,
    pub spam_weights: serde_json::Value,
    pub cas_enabled: bool,
    pub clown_chance: i16,
    pub log_allowed_messages: bool,
    pub report_hour: i16,
    pub timezone: String,
    pub summary_enabled: bool,
    pub summary_token_budget: i32,
    pub report_min_activity: i16,
    pub openai_api_key: Option<String>,
    pub openai_model: String,
    pub language: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Public view of a `ChatConfig`. `openai_api_key_set` replaces the raw key —
/// the secret never leaves the server.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ChatConfigDto {
    pub chat_id: i64,
    pub captcha_enabled: bool,
    pub captcha_lifetime_secs: i32,
    pub captcha_attempts: i16,
    pub spam_enabled: bool,
    pub spam_threshold: f32,
    pub spam_weights: serde_json::Value,
    pub cas_enabled: bool,
    pub clown_chance: i16,
    pub log_allowed_messages: bool,
    pub report_hour: i16,
    pub timezone: String,
    pub summary_enabled: bool,
    pub summary_token_budget: i32,
    pub report_min_activity: i16,
    /// `true` when an OpenAI API key is configured for this chat, `false` when
    /// `openai_api_key IS NULL`. The key itself is never exposed.
    pub openai_api_key_set: bool,
    pub openai_model: String,
    pub language: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl fmt::Debug for ChatConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Mask `openai_api_key`: `Some(_) -> "***redacted***"`, `None -> "None"`.
        // Every other field is non-secret and prints in full.
        let key_repr: &dyn fmt::Debug = if self.openai_api_key.is_some() {
            &"***redacted***"
        } else {
            &Option::<&str>::None
        };
        f.debug_struct("ChatConfig")
            .field("chat_id", &self.chat_id)
            .field("captcha_enabled", &self.captcha_enabled)
            .field("captcha_lifetime_secs", &self.captcha_lifetime_secs)
            .field("captcha_attempts", &self.captcha_attempts)
            .field("spam_enabled", &self.spam_enabled)
            .field("spam_threshold", &self.spam_threshold)
            .field("spam_weights", &self.spam_weights)
            .field("cas_enabled", &self.cas_enabled)
            .field("clown_chance", &self.clown_chance)
            .field("log_allowed_messages", &self.log_allowed_messages)
            .field("report_hour", &self.report_hour)
            .field("timezone", &self.timezone)
            .field("summary_enabled", &self.summary_enabled)
            .field("summary_token_budget", &self.summary_token_budget)
            .field("report_min_activity", &self.report_min_activity)
            .field("openai_api_key", key_repr)
            .field("openai_model", &self.openai_model)
            .field("language", &self.language)
            .field("created_at", &self.created_at)
            .field("updated_at", &self.updated_at)
            .finish()
    }
}

impl From<&ChatConfig> for ChatConfigDto {
    fn from(c: &ChatConfig) -> Self {
        Self {
            chat_id: c.chat_id,
            captcha_enabled: c.captcha_enabled,
            captcha_lifetime_secs: c.captcha_lifetime_secs,
            captcha_attempts: c.captcha_attempts,
            spam_enabled: c.spam_enabled,
            spam_threshold: c.spam_threshold,
            spam_weights: c.spam_weights.clone(),
            cas_enabled: c.cas_enabled,
            clown_chance: c.clown_chance,
            log_allowed_messages: c.log_allowed_messages,
            report_hour: c.report_hour,
            timezone: c.timezone.clone(),
            summary_enabled: c.summary_enabled,
            summary_token_budget: c.summary_token_budget,
            report_min_activity: c.report_min_activity,
            openai_api_key_set: c.openai_api_key.is_some(),
            openai_model: c.openai_model.clone(),
            language: c.language.clone(),
            created_at: c.created_at,
            updated_at: c.updated_at,
        }
    }
}

/// PATCH payload. Absent field = leave value untouched. For non-nullable
/// columns an explicit `null` is **rejected** at deserialization time so
/// client typos don't silently no-op (e.g. `{"captcha_enabled": null}` is
/// never a valid value — caller meant to either set a bool or omit the
/// field). For `openai_api_key` (the only NULLable column) `null` clears the
/// key; absent leaves the stored key intact — the [`Option<Option<T>>`]
/// indirection captures the three-state semantics.
#[derive(Debug, Default, Clone, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct ChatConfigPatch {
    #[serde(default, deserialize_with = "deserialize_present_non_null")]
    pub captcha_enabled: Option<bool>,
    #[serde(default, deserialize_with = "deserialize_present_non_null")]
    pub captcha_lifetime_secs: Option<i32>,
    #[serde(default, deserialize_with = "deserialize_present_non_null")]
    pub captcha_attempts: Option<i16>,
    #[serde(default, deserialize_with = "deserialize_present_non_null")]
    pub spam_enabled: Option<bool>,
    #[serde(default, deserialize_with = "deserialize_present_non_null")]
    pub spam_threshold: Option<f32>,
    #[serde(default, deserialize_with = "deserialize_present_non_null")]
    pub spam_weights: Option<serde_json::Value>,
    #[serde(default, deserialize_with = "deserialize_present_non_null")]
    pub cas_enabled: Option<bool>,
    #[serde(default, deserialize_with = "deserialize_present_non_null")]
    pub clown_chance: Option<i16>,
    #[serde(default, deserialize_with = "deserialize_present_non_null")]
    pub log_allowed_messages: Option<bool>,
    #[serde(default, deserialize_with = "deserialize_present_non_null")]
    pub report_hour: Option<i16>,
    #[serde(default, deserialize_with = "deserialize_present_non_null")]
    pub timezone: Option<String>,
    #[serde(default, deserialize_with = "deserialize_present_non_null")]
    pub summary_enabled: Option<bool>,
    #[serde(default, deserialize_with = "deserialize_present_non_null")]
    pub summary_token_budget: Option<i32>,
    #[serde(default, deserialize_with = "deserialize_present_non_null")]
    pub report_min_activity: Option<i16>,
    /// The only nullable field — `Option<Option<T>>` distinguishes
    /// "absent" from explicit `null`.
    #[serde(default, deserialize_with = "deserialize_optional_nullable")]
    pub openai_api_key: Option<Option<String>>,
    #[serde(default, deserialize_with = "deserialize_present_non_null")]
    pub openai_model: Option<String>,
    #[serde(default, deserialize_with = "deserialize_present_non_null")]
    pub language: Option<String>,
}

/// `Some(value)` if a non-null value is present, `Err` if `null` is given.
/// Combined with `#[serde(default)]`, an absent field yields `None` without
/// hitting this function. Used on every non-nullable patch field so a
/// `{"field": null}` payload is rejected with a useful error rather than
/// silently treated as omitted.
fn deserialize_present_non_null<'de, T, D>(deserializer: D) -> Result<Option<T>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    if value.is_null() {
        return Err(serde::de::Error::custom(
            "null is not a valid value for this field; omit it instead to leave it unchanged",
        ));
    }
    T::deserialize(value)
        .map(Some)
        .map_err(serde::de::Error::custom)
}

/// Distinguishes `{ "field": null }` (returns `Some(None)`) from omission
/// (returns `None`). Required for the three-state PATCH semantics on
/// nullable columns — currently only `openai_api_key`.
fn deserialize_optional_nullable<'de, T, D>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

impl ChatConfigPatch {
    /// Returns `true` when the patch carries no field — caller can skip the DB write.
    pub fn is_empty(&self) -> bool {
        self.captcha_enabled.is_none()
            && self.captcha_lifetime_secs.is_none()
            && self.captcha_attempts.is_none()
            && self.spam_enabled.is_none()
            && self.spam_threshold.is_none()
            && self.spam_weights.is_none()
            && self.cas_enabled.is_none()
            && self.clown_chance.is_none()
            && self.log_allowed_messages.is_none()
            && self.report_hour.is_none()
            && self.timezone.is_none()
            && self.summary_enabled.is_none()
            && self.summary_token_budget.is_none()
            && self.report_min_activity.is_none()
            && self.openai_api_key.is_none()
            && self.openai_model.is_none()
            && self.language.is_none()
    }

    /// Validate every present field against the same CHECK constraints PG
    /// enforces — failing here returns a 400 with a useful message instead of
    /// a 500 from the DB.
    pub fn validate(&self) -> Result<(), PatchValidationError> {
        if let Some(v) = self.captcha_lifetime_secs
            && v <= 0
        {
            return Err(PatchValidationError::OutOfRange {
                field: "captcha_lifetime_secs",
                detail: "must be > 0",
            });
        }
        if let Some(v) = self.captcha_attempts
            && v <= 0
        {
            return Err(PatchValidationError::OutOfRange {
                field: "captcha_attempts",
                detail: "must be > 0",
            });
        }
        if let Some(v) = self.spam_threshold
            && v < 0.0
        {
            return Err(PatchValidationError::OutOfRange {
                field: "spam_threshold",
                detail: "must be >= 0",
            });
        }
        if let Some(v) = self.clown_chance
            && !(0..=100).contains(&v)
        {
            return Err(PatchValidationError::OutOfRange {
                field: "clown_chance",
                detail: "must be 0..=100",
            });
        }
        if let Some(v) = self.report_hour
            && !(0..=23).contains(&v)
        {
            return Err(PatchValidationError::OutOfRange {
                field: "report_hour",
                detail: "must be 0..=23",
            });
        }
        if let Some(v) = self.summary_token_budget
            && v <= 0
        {
            return Err(PatchValidationError::OutOfRange {
                field: "summary_token_budget",
                detail: "must be > 0",
            });
        }
        if let Some(v) = self.report_min_activity
            && v < 0
        {
            return Err(PatchValidationError::OutOfRange {
                field: "report_min_activity",
                detail: "must be >= 0",
            });
        }
        if let Some(tz) = &self.timezone {
            // chat_config.timezone is VARCHAR(64); reject longer input here so
            // the route returns 400 with a useful message instead of bubbling
            // a generic 500 up from the Postgres "value too long" error.
            if tz.len() > 64 {
                return Err(PatchValidationError::BadValue {
                    field: "timezone",
                    detail: "must be ≤ 64 bytes (matches the chat_config.timezone column)",
                });
            }
            if tz.parse::<chrono_tz::Tz>().is_err() {
                return Err(PatchValidationError::BadValue {
                    field: "timezone",
                    detail: "must be a valid IANA timezone (e.g. 'UTC', 'Europe/Moscow')",
                });
            }
        }
        if let Some(lang) = &self.language
            && lang != "ru"
            && lang != "en"
        {
            return Err(PatchValidationError::BadValue {
                field: "language",
                detail: "must be 'ru' or 'en'",
            });
        }
        if let Some(model) = &self.openai_model {
            if model.is_empty() {
                return Err(PatchValidationError::BadValue {
                    field: "openai_model",
                    detail: "must not be empty",
                });
            }
            // chat_config.openai_model is VARCHAR(64); same rationale as
            // timezone above.
            if model.len() > 64 {
                return Err(PatchValidationError::BadValue {
                    field: "openai_model",
                    detail: "must be ≤ 64 bytes (matches the chat_config.openai_model column)",
                });
            }
        }
        Ok(())
    }
}

#[derive(thiserror::Error, Debug)]
pub enum PatchValidationError {
    #[error("field `{field}` out of range: {detail}")]
    OutOfRange {
        field: &'static str,
        detail: &'static str,
    },
    #[error("field `{field}` invalid: {detail}")]
    BadValue {
        field: &'static str,
        detail: &'static str,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patch_distinguishes_null_from_absent_for_nullable() {
        let absent: ChatConfigPatch = serde_json::from_str(r#"{}"#).unwrap();
        assert!(absent.openai_api_key.is_none());

        let null: ChatConfigPatch = serde_json::from_str(r#"{"openai_api_key": null}"#).unwrap();
        assert_eq!(null.openai_api_key, Some(None));

        let set: ChatConfigPatch =
            serde_json::from_str(r#"{"openai_api_key": "sk-test"}"#).unwrap();
        assert_eq!(set.openai_api_key, Some(Some("sk-test".to_string())));
    }

    #[test]
    fn patch_rejects_unknown_fields() {
        let err = serde_json::from_str::<ChatConfigPatch>(r#"{"unknown_field": 1}"#);
        assert!(err.is_err());
    }

    #[test]
    fn patch_rejects_null_for_non_nullable_fields() {
        // `captcha_enabled` is NOT NULL in PG and not nullable in the patch
        // contract — `null` must be rejected at deserialization time, not
        // silently treated as "leave it alone".
        for field in [
            "captcha_enabled",
            "captcha_lifetime_secs",
            "captcha_attempts",
            "spam_enabled",
            "spam_threshold",
            "spam_weights",
            "cas_enabled",
            "clown_chance",
            "log_allowed_messages",
            "report_hour",
            "timezone",
            "summary_enabled",
            "summary_token_budget",
            "report_min_activity",
            "openai_model",
            "language",
        ] {
            let json = format!(r#"{{"{field}": null}}"#);
            let err = serde_json::from_str::<ChatConfigPatch>(&json);
            assert!(
                err.is_err(),
                "null for non-nullable `{field}` must error; got {err:?}"
            );
        }

        // openai_api_key remains nullable — `null` is a valid "clear" value.
        let ok = serde_json::from_str::<ChatConfigPatch>(r#"{"openai_api_key": null}"#).unwrap();
        assert_eq!(ok.openai_api_key, Some(None));
    }

    #[test]
    fn patch_rejects_too_long_openai_model() {
        let p = ChatConfigPatch {
            openai_model: Some("a".repeat(65)),
            ..Default::default()
        };
        assert!(matches!(
            p.validate(),
            Err(PatchValidationError::BadValue {
                field: "openai_model",
                ..
            })
        ));
    }

    #[test]
    fn patch_rejects_too_long_timezone() {
        let p = ChatConfigPatch {
            timezone: Some("a".repeat(65)),
            ..Default::default()
        };
        assert!(matches!(
            p.validate(),
            Err(PatchValidationError::BadValue {
                field: "timezone",
                ..
            })
        ));
    }

    #[test]
    fn validate_catches_bad_ranges() {
        let p = ChatConfigPatch {
            report_hour: Some(24),
            ..Default::default()
        };
        assert!(matches!(
            p.validate(),
            Err(PatchValidationError::OutOfRange { .. })
        ));

        let p = ChatConfigPatch {
            language: Some("fr".into()),
            ..Default::default()
        };
        assert!(matches!(
            p.validate(),
            Err(PatchValidationError::BadValue { .. })
        ));

        let p = ChatConfigPatch {
            timezone: Some("Not/A_Zone".into()),
            ..Default::default()
        };
        assert!(matches!(
            p.validate(),
            Err(PatchValidationError::BadValue { .. })
        ));
    }

    #[test]
    fn empty_patch_detected() {
        let p = ChatConfigPatch::default();
        assert!(p.is_empty());

        let p = ChatConfigPatch {
            spam_enabled: Some(false),
            ..Default::default()
        };
        assert!(!p.is_empty());
    }

    #[test]
    fn debug_redacts_openai_api_key() {
        let now = Utc::now();
        let cfg = ChatConfig {
            chat_id: -100,
            captcha_enabled: true,
            captcha_lifetime_secs: 60,
            captcha_attempts: 5,
            spam_enabled: true,
            spam_threshold: 1.0,
            spam_weights: serde_json::json!({}),
            cas_enabled: true,
            clown_chance: 0,
            log_allowed_messages: false,
            report_hour: 17,
            timezone: "UTC".into(),
            summary_enabled: false,
            summary_token_budget: 50_000,
            report_min_activity: 20,
            openai_api_key: Some("sk-DO-NOT-LEAK-this-secret".into()),
            openai_model: "gpt-4o-mini".into(),
            language: "ru".into(),
            created_at: now,
            updated_at: now,
        };
        let dbg = format!("{cfg:?}");
        assert!(
            !dbg.contains("sk-DO-NOT-LEAK"),
            "Debug must redact openai_api_key; got: {dbg}"
        );
        assert!(
            dbg.contains("***redacted***"),
            "Debug should mark the key as redacted; got: {dbg}"
        );

        // When the key is absent the field should still appear, just as None.
        let cfg_no_key = ChatConfig {
            openai_api_key: None,
            ..cfg
        };
        let dbg = format!("{cfg_no_key:?}");
        assert!(dbg.contains("openai_api_key"));
        assert!(!dbg.contains("***redacted***"));
    }

    #[test]
    fn dto_masks_openai_key() {
        let now = Utc::now();
        let mut cfg = ChatConfig {
            chat_id: -100,
            captcha_enabled: true,
            captcha_lifetime_secs: 60,
            captcha_attempts: 5,
            spam_enabled: true,
            spam_threshold: 1.0,
            spam_weights: serde_json::json!({}),
            cas_enabled: true,
            clown_chance: 0,
            log_allowed_messages: false,
            report_hour: 17,
            timezone: "UTC".into(),
            summary_enabled: false,
            summary_token_budget: 50_000,
            report_min_activity: 20,
            openai_api_key: Some("sk-leak-me-not".into()),
            openai_model: "gpt-4o-mini".into(),
            language: "ru".into(),
            created_at: now,
            updated_at: now,
        };
        let dto = ChatConfigDto::from(&cfg);
        assert!(dto.openai_api_key_set);
        let json = serde_json::to_string(&dto).unwrap();
        assert!(!json.contains("sk-leak-me-not"));

        cfg.openai_api_key = None;
        let dto = ChatConfigDto::from(&cfg);
        assert!(!dto.openai_api_key_set);
    }
}
