//! Thin Chat Completions client. Owns the retry-on-429 + Retry-After loop and
//! returns just the text + usage. The API key is **not** stored on the client
//! — `summary_service` resolves it per-chat (chat_config) and passes it in.
//!
//! Retry policy: up to 3 attempts on 429 / 5xx with exponential backoff,
//! optionally overridden by a `Retry-After` header. Anything else surfaces
//! to the caller as an error (no silent skip — the summary service's
//! `Skipped` outcome is for *policy* skips, not transport failures).

use std::time::Duration;

use anyhow::{Context, Result};
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

const MAX_RETRIES: u32 = 3;
const INITIAL_BACKOFF_MS: u64 = 500;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Serialize)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: String,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ChatRole {
    System,
    User,
    Assistant,
}

#[derive(Debug, Clone)]
pub struct ChatCompletion {
    pub content: String,
    pub total_tokens: u32,
}

#[derive(Clone)]
pub struct OpenAiClient {
    http: reqwest::Client,
    base_url: String,
}

impl OpenAiClient {
    pub fn new(base_url: String) -> Self {
        let http = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .expect("reqwest client builds");
        Self { http, base_url }
    }

    /// POST `/v1/chat/completions`. Retries on 429 + 5xx with exponential
    /// backoff (and `Retry-After` when present). Returns `(content,
    /// total_tokens)` on the first 2xx; any non-retryable status surfaces
    /// as an error.
    pub async fn chat(
        &self,
        api_key: &str,
        model: &str,
        messages: Vec<ChatMessage>,
        max_tokens: u32,
    ) -> Result<ChatCompletion> {
        let url = format!(
            "{}/v1/chat/completions",
            self.base_url.trim_end_matches('/')
        );
        let body = ChatCompletionsRequest {
            model: model.to_string(),
            messages,
            max_tokens,
            temperature: 0.4,
        };

        let mut last_err: Option<anyhow::Error> = None;
        for attempt in 0..MAX_RETRIES {
            let resp = self
                .http
                .post(&url)
                .bearer_auth(api_key)
                .json(&body)
                .send()
                .await;

            let resp = match resp {
                Ok(r) => r,
                Err(e) => {
                    warn!(attempt, error = %e, "openai POST failed");
                    last_err = Some(anyhow::Error::from(e).context("openai POST"));
                    sleep_with_backoff(attempt, None).await;
                    continue;
                }
            };

            let status = resp.status();
            if status.is_success() {
                let parsed: ChatCompletionsResponse =
                    resp.json().await.context("decode openai response")?;
                let content = parsed
                    .choices
                    .into_iter()
                    .next()
                    .map(|c| c.message.content)
                    .unwrap_or_default();
                let total_tokens = parsed.usage.map(|u| u.total_tokens).unwrap_or(0);
                return Ok(ChatCompletion {
                    content,
                    total_tokens,
                });
            }

            if !is_retryable(status) {
                let body_text = resp.text().await.unwrap_or_default();
                anyhow::bail!("openai non-retryable status {status}: {body_text}");
            }

            let retry_after = parse_retry_after(resp.headers().get("retry-after"));
            let body_text = resp.text().await.unwrap_or_default();
            warn!(
                attempt,
                %status,
                retry_after_ms = retry_after.map(|d| d.as_millis() as u64),
                body_preview = %body_text.chars().take(200).collect::<String>(),
                "openai retryable status",
            );
            last_err = Some(anyhow::anyhow!(
                "openai retryable status {status} (attempt {attempt})"
            ));
            sleep_with_backoff(attempt, retry_after).await;
        }
        Err(last_err
            .unwrap_or_else(|| anyhow::anyhow!("openai exhausted retries"))
            .context(format!("openai gave up after {MAX_RETRIES} attempts")))
    }
}

fn is_retryable(status: StatusCode) -> bool {
    status == StatusCode::TOO_MANY_REQUESTS || status.is_server_error()
}

async fn sleep_with_backoff(attempt: u32, retry_after: Option<Duration>) {
    let backoff = retry_after
        .unwrap_or_else(|| Duration::from_millis(INITIAL_BACKOFF_MS * (1u64 << attempt)));
    debug!(attempt, ?backoff, "sleeping before openai retry");
    tokio::time::sleep(backoff).await;
}

fn parse_retry_after(value: Option<&reqwest::header::HeaderValue>) -> Option<Duration> {
    let v = value?.to_str().ok()?;
    // RFC 9110: Retry-After is either delta-seconds or HTTP-date. We accept
    // delta-seconds only; HTTP-date is rare for OpenAI and not worth a parser.
    let secs: u64 = v.trim().parse().ok()?;
    Some(Duration::from_secs(secs.min(60)))
}

// ── wire types ─────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct ChatCompletionsRequest {
    model: String,
    messages: Vec<ChatMessage>,
    max_tokens: u32,
    temperature: f32,
}

#[derive(Debug, Deserialize)]
struct ChatCompletionsResponse {
    choices: Vec<ChatChoice>,
    usage: Option<ChatUsage>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatChoiceMessage,
}

#[derive(Debug, Deserialize)]
struct ChatChoiceMessage {
    content: String,
}

#[derive(Debug, Deserialize)]
struct ChatUsage {
    total_tokens: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retry_after_parses_delta_seconds() {
        let v = reqwest::header::HeaderValue::from_static("3");
        let d = parse_retry_after(Some(&v));
        assert_eq!(d, Some(Duration::from_secs(3)));
    }

    #[test]
    fn retry_after_caps_at_60s() {
        let v = reqwest::header::HeaderValue::from_static("3600");
        let d = parse_retry_after(Some(&v));
        assert_eq!(d, Some(Duration::from_secs(60)));
    }

    #[test]
    fn retry_after_rejects_http_date() {
        let v = reqwest::header::HeaderValue::from_static("Wed, 21 Oct 2026 07:28:00 GMT");
        assert_eq!(parse_retry_after(Some(&v)), None);
    }

    #[test]
    fn is_retryable_matches_429_and_5xx() {
        assert!(is_retryable(StatusCode::TOO_MANY_REQUESTS));
        assert!(is_retryable(StatusCode::INTERNAL_SERVER_ERROR));
        assert!(is_retryable(StatusCode::BAD_GATEWAY));
        assert!(!is_retryable(StatusCode::UNAUTHORIZED));
        assert!(!is_retryable(StatusCode::BAD_REQUEST));
    }
}
