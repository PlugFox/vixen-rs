---
name: serde-strict-deserialization
description: Add serde::Deserialize for API request bodies — deny_unknown_fields, custom validators via deserialize_with, semantic error messages. Catch contract drift early.
---

# Serde Strict Deserialization (Vixen server)

**Source:** [Serde field attributes](https://serde.rs/field-attrs.html).

**Read first:**

- [server/docs/rules/api-routes.md](../../../../server/docs/rules/api-routes.md).
- [server/docs/rules/error-handling.md](../../../../server/docs/rules/error-handling.md).

## Pattern

```rust
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateConfigRequest {
    #[serde(deserialize_with = "deserialize_threshold")]
    pub spam_threshold: u8,
    pub report_hour: Option<u8>,
}

fn deserialize_threshold<'de, D: Deserializer<'de>>(d: D) -> Result<u8, D::Error> {
    let v = u8::deserialize(d)?;
    if v > 100 { return Err(de::Error::custom("threshold must be 0..=100")); }
    Ok(v)
}
```

## Why `deny_unknown_fields`

- Catches API contract drift: client sends `spamThreshold` (camelCase) instead of `spam_threshold` → loud failure, not silent ignore.
- Required for every request struct in vixen's API.
- Costs nothing at runtime; catches bugs at the boundary.

## Custom validators

- `deserialize_with` runs at parse time; no separate validation pass.
- Return `de::Error::custom("...")` for human-readable error.
- Combines cleanly with `deny_unknown_fields` — both fire if applicable.
- Reach for it for ranges, regex shapes, enum-from-string with validation.

## Numeric ranges

- Use the smallest type that fits (`u8` for percentages, not `u32`).
- For `i64` Telegram IDs: no extra validator needed; serde rejects out-of-range values automatically.
- Never `u64` for Telegram IDs — supergroup IDs `-100…` are negative.

## Optional vs required

- `Option<T>` = optional, `None` if missing.
- `T` = required, parse error if missing.
- `#[serde(default)]` = required-from-shape but defaults if missing — rarely correct for API DTOs.

## Errors out

- Map serde errors to `AppError::BadRequest` in the extractor; never let raw serde messages leak to the client (they include line/column from JSON — useful internally, verbose externally).
- Use `axum::extract::Json<T>` rejection handling to wrap errors uniformly.
- Log the raw error at `debug!`; return a sanitized message to the client.

## Gotchas

- `deny_unknown_fields` + flattened untagged enums = brittle. Test the combination thoroughly; serde's interaction is order-sensitive.
- `#[serde(rename_all = "camelCase")]` is fine for external APIs (Combot CAS); vixen's own API stays `snake_case` for SQL parity.
- Don't `#[serde(default)]` on required fields — silently masks missing data.
- `#[serde(skip_deserializing)]` is the wrong tool for "compute on the server"; use a separate output type instead.

## Verification

- `cargo test deserialize`.
- Property test: send unknown field → 400 `BadRequest`; send out-of-range → 400 with `custom` message; send missing required → 400 explaining which field.

## Related

- `add-api-route`, `rust-error-handling`.
