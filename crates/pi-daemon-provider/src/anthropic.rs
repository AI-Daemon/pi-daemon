//! Anthropic provider — streaming completions from the Claude API.

use async_trait::async_trait;
use futures::StreamExt;
use pi_daemon_types::error::DaemonError;
use pi_daemon_types::message::{Message, TokenUsage};
use tracing::{debug, warn};

use crate::convert::to_anthropic_messages;
use crate::provider::Provider;
use crate::sse;
use crate::types::{CompletionOptions, CompletionStream, StreamEvent};

const DEFAULT_BASE_URL: &str = "https://api.anthropic.com";
const ANTHROPIC_VERSION: &str = "2023-06-01";
const REQUEST_TIMEOUT_SECS: u64 = 120;
const MAX_RETRIES: u32 = 3;

/// Client for the Anthropic Messages API.
pub struct AnthropicProvider {
    client: reqwest::Client,
    api_key: String,
    base_url: String,
}

impl AnthropicProvider {
    /// Create a new Anthropic provider.
    ///
    /// Uses the given `base_url` or falls back to `https://api.anthropic.com`.
    pub fn new(api_key: String, base_url: Option<String>) -> Result<Self, DaemonError> {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(REQUEST_TIMEOUT_SECS))
            .build()
            .map_err(|e| DaemonError::Config(format!("Failed to build HTTP client: {e}")))?;

        let base_url = base_url
            .filter(|u| !u.is_empty())
            .unwrap_or_else(|| DEFAULT_BASE_URL.to_string())
            .trim_end_matches('/')
            .to_string();

        Ok(Self {
            client,
            api_key,
            base_url,
        })
    }

    /// Build the request body for the Anthropic Messages API.
    fn build_body(
        &self,
        model: &str,
        messages: &[Message],
        options: &CompletionOptions,
    ) -> serde_json::Value {
        let (system, api_messages) =
            to_anthropic_messages(messages, options.system_prompt.as_deref());

        let mut body = serde_json::json!({
            "model": model,
            "messages": api_messages,
            "max_tokens": options.max_tokens,
            "stream": true,
        });

        if let Some(system) = system {
            body["system"] = serde_json::Value::String(system);
        }
        if let Some(temp) = options.temperature {
            body["temperature"] = serde_json::json!(temp);
        }
        if let Some(top_p) = options.top_p {
            body["top_p"] = serde_json::json!(top_p);
        }
        if !options.stop_sequences.is_empty() {
            body["stop_sequences"] = serde_json::json!(options.stop_sequences);
        }

        body
    }

    /// Send the request with retry logic for transient failures (429, 5xx).
    async fn send_with_retry(
        &self,
        body: &serde_json::Value,
    ) -> Result<reqwest::Response, DaemonError> {
        let url = format!("{}/v1/messages", self.base_url);

        for attempt in 0..MAX_RETRIES {
            let resp = self
                .client
                .post(&url)
                .header("x-api-key", &self.api_key)
                .header("anthropic-version", ANTHROPIC_VERSION)
                .header("content-type", "application/json")
                .json(body)
                .send()
                .await
                .map_err(|e| DaemonError::Api(format!("Anthropic request failed: {e}")))?;

            let status = resp.status();

            if status.is_success() {
                return Ok(resp);
            }

            // Retry on rate-limit or server errors
            if (status == reqwest::StatusCode::TOO_MANY_REQUESTS || status.is_server_error())
                && attempt < MAX_RETRIES - 1
            {
                let delay = std::time::Duration::from_millis(500 * 2u64.pow(attempt));
                warn!(
                    status = %status,
                    attempt = attempt + 1,
                    delay_ms = delay.as_millis(),
                    "Anthropic transient error, retrying"
                );
                tokio::time::sleep(delay).await;
                continue;
            }

            // Non-retryable error — read body for details
            let error_body = match resp.text().await {
                Ok(body) => body,
                Err(e) => {
                    warn!("Failed to read Anthropic error response body: {e}");
                    format!("<unreadable: {e}>")
                }
            };
            return Err(DaemonError::Api(format!(
                "Anthropic API error {status}: {error_body}"
            )));
        }

        Err(DaemonError::Api("Anthropic: max retries exceeded".into()))
    }
}

#[async_trait]
impl Provider for AnthropicProvider {
    async fn complete(
        &self,
        model: &str,
        messages: Vec<Message>,
        options: CompletionOptions,
    ) -> Result<CompletionStream, DaemonError> {
        let body = self.build_body(model, &messages, &options);
        debug!(model, "Sending Anthropic streaming request");

        let response = self.send_with_retry(&body).await?;
        let event_stream = sse::parse_sse(response);

        let stream = async_stream::stream! {
            // State for accumulating tool use blocks
            let mut tool_id: Option<String> = None;
            let mut tool_name: Option<String> = None;
            let mut tool_input_json = String::new();

            // Token usage accumulator
            let mut usage = TokenUsage::default();

            futures::pin_mut!(event_stream);

            while let Some(sse_result) = event_stream.next().await {
                let sse_event = match sse_result {
                    Ok(e) => e,
                    Err(e) => {
                        yield StreamEvent::Error(e);
                        return;
                    }
                };

                let event_type = sse_event.event.as_str();

                match event_type {
                    "content_block_start" => {
                        if let Ok(data) = serde_json::from_str::<serde_json::Value>(&sse_event.data) {
                            if let Some(cb) = data.get("content_block") {
                                if cb.get("type").and_then(|t| t.as_str()) == Some("tool_use") {
                                    tool_id = cb.get("id").and_then(|v| v.as_str()).map(|s| s.to_string());
                                    tool_name = cb.get("name").and_then(|v| v.as_str()).map(|s| s.to_string());
                                    tool_input_json.clear();
                                }
                            }
                        }
                    }
                    "content_block_delta" => {
                        if let Ok(data) = serde_json::from_str::<serde_json::Value>(&sse_event.data) {
                            if let Some(delta) = data.get("delta") {
                                let delta_type = delta.get("type").and_then(|t| t.as_str()).unwrap_or("");

                                match delta_type {
                                    "text_delta" => {
                                        if let Some(text) = delta.get("text").and_then(|t| t.as_str()) {
                                            if !text.is_empty() {
                                                yield StreamEvent::TextDelta(text.to_string());
                                            }
                                        }
                                    }
                                    "input_json_delta" => {
                                        if let Some(partial) = delta.get("partial_json").and_then(|t| t.as_str()) {
                                            tool_input_json.push_str(partial);
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                    "content_block_stop" => {
                        // If we were accumulating a tool use, emit it now
                        if let (Some(id), Some(name)) = (tool_id.take(), tool_name.take()) {
                            let input: serde_json::Value = serde_json::from_str(&tool_input_json)
                                .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
                            tool_input_json.clear();
                            yield StreamEvent::ToolUse { id, name, input };
                        }
                    }
                    "message_delta" => {
                        if let Ok(data) = serde_json::from_str::<serde_json::Value>(&sse_event.data) {
                            if let Some(u) = data.get("usage") {
                                usage.output_tokens = u.get("output_tokens")
                                    .and_then(|v| v.as_u64())
                                    .unwrap_or(0) as u32;
                            }
                        }
                    }
                    "message_start" => {
                        if let Ok(data) = serde_json::from_str::<serde_json::Value>(&sse_event.data) {
                            if let Some(msg) = data.get("message") {
                                if let Some(u) = msg.get("usage") {
                                    usage.input_tokens = u.get("input_tokens")
                                        .and_then(|v| v.as_u64())
                                        .unwrap_or(0) as u32;
                                    usage.cache_read_tokens = u.get("cache_read_input_tokens")
                                        .and_then(|v| v.as_u64())
                                        .map(|v| v as u32);
                                    usage.cache_creation_tokens = u.get("cache_creation_input_tokens")
                                        .and_then(|v| v.as_u64())
                                        .map(|v| v as u32);
                                }
                            }
                        }
                    }
                    "message_stop" => {
                        yield StreamEvent::Done(usage.clone());
                    }
                    "error" => {
                        if let Ok(data) = serde_json::from_str::<serde_json::Value>(&sse_event.data) {
                            let msg = data.get("error")
                                .and_then(|e| e.get("message"))
                                .and_then(|m| m.as_str())
                                .unwrap_or("Unknown Anthropic error");
                            yield StreamEvent::Error(msg.to_string());
                        } else {
                            yield StreamEvent::Error(sse_event.data);
                        }
                    }
                    _ => {
                        // ping, etc. — ignore
                    }
                }
            }
        };

        Ok(Box::pin(stream))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_anthropic_provider_default_base_url() {
        let p = AnthropicProvider::new("test-key".into(), None).unwrap();
        assert_eq!(p.base_url, DEFAULT_BASE_URL);
        assert_eq!(p.api_key, "test-key");
    }

    #[test]
    fn test_anthropic_provider_custom_base_url() {
        let p = AnthropicProvider::new("key".into(), Some("https://proxy.example.com/".into()))
            .unwrap();
        assert_eq!(p.base_url, "https://proxy.example.com");
    }

    #[test]
    fn test_anthropic_provider_empty_base_url_uses_default() {
        let p = AnthropicProvider::new("key".into(), Some("".into())).unwrap();
        assert_eq!(p.base_url, DEFAULT_BASE_URL);
    }

    #[test]
    fn test_build_body_basic() {
        let p = AnthropicProvider::new("key".into(), None).unwrap();
        let messages = vec![Message {
            role: pi_daemon_types::message::Role::User,
            content: pi_daemon_types::message::MessageContent::Text("Hello".into()),
        }];
        let options = CompletionOptions::default();

        let body = p.build_body("claude-sonnet-4-20250514", &messages, &options);
        assert_eq!(body["model"], "claude-sonnet-4-20250514");
        assert_eq!(body["stream"], true);
        assert_eq!(body["max_tokens"], 8192);
        assert!(body.get("system").is_none());
    }

    #[test]
    fn test_build_body_with_system() {
        let p = AnthropicProvider::new("key".into(), None).unwrap();
        let messages = vec![Message {
            role: pi_daemon_types::message::Role::User,
            content: pi_daemon_types::message::MessageContent::Text("Hello".into()),
        }];
        let options = CompletionOptions {
            system_prompt: Some("You are helpful.".into()),
            ..Default::default()
        };

        let body = p.build_body("claude-sonnet-4-20250514", &messages, &options);
        assert_eq!(body["system"], "You are helpful.");
    }

    /// Helper: start an axum server that always returns the given status and body.
    async fn start_mock_server(status: axum::http::StatusCode, body: &'static str) -> String {
        use axum::{routing::post, Router};

        let app = Router::new().route(
            "/v1/messages",
            post(move || async move { (status, body.to_string()) }),
        );

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        format!("http://{addr}")
    }

    #[tokio::test]
    async fn test_send_with_retry_non_retryable_api_error() {
        let base = start_mock_server(
            axum::http::StatusCode::BAD_REQUEST,
            r#"{"error":{"message":"invalid model"}}"#,
        )
        .await;

        let provider = AnthropicProvider::new("test-key".into(), Some(base)).unwrap();
        let body = serde_json::json!({"model": "bad-model", "stream": true});

        let err = provider.send_with_retry(&body).await.unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("400"), "expected 400 in: {msg}");
        assert!(msg.contains("invalid model"), "expected body in: {msg}");
    }

    #[tokio::test]
    async fn test_send_with_retry_unauthorized_error() {
        let base = start_mock_server(
            axum::http::StatusCode::UNAUTHORIZED,
            r#"{"error":{"message":"invalid api key"}}"#,
        )
        .await;

        let provider = AnthropicProvider::new("bad-key".into(), Some(base)).unwrap();
        let body = serde_json::json!({"model": "claude-sonnet-4-20250514", "stream": true});

        let err = provider.send_with_retry(&body).await.unwrap_err();
        let msg = err.to_string();
        assert!(
            matches!(err, DaemonError::Api(_)),
            "expected DaemonError::Api, got: {err:?}"
        );
        assert!(msg.contains("401"), "expected 401 in: {msg}");
    }

    #[tokio::test]
    async fn test_send_with_retry_forbidden_error() {
        let base = start_mock_server(axum::http::StatusCode::FORBIDDEN, "Forbidden").await;

        let provider = AnthropicProvider::new("key".into(), Some(base)).unwrap();
        let body = serde_json::json!({"model": "claude-sonnet-4-20250514", "stream": true});

        let err = provider.send_with_retry(&body).await.unwrap_err();
        assert!(matches!(err, DaemonError::Api(_)));
        assert!(err.to_string().contains("403"));
    }

    #[tokio::test]
    async fn test_send_with_retry_success_returns_ok() {
        let base = start_mock_server(axum::http::StatusCode::OK, "{}").await;

        let provider = AnthropicProvider::new("key".into(), Some(base)).unwrap();
        let body = serde_json::json!({"model": "claude-sonnet-4-20250514", "stream": true});

        let resp = provider.send_with_retry(&body).await;
        assert!(resp.is_ok(), "expected Ok, got: {resp:?}");
    }

    #[tokio::test]
    async fn test_send_with_retry_connection_refused() {
        // Point at a port with nothing listening
        let provider =
            AnthropicProvider::new("key".into(), Some("http://127.0.0.1:1".into())).unwrap();
        let body = serde_json::json!({"model": "m", "stream": true});

        let err = provider.send_with_retry(&body).await.unwrap_err();
        assert!(
            matches!(err, DaemonError::Api(_)),
            "expected DaemonError::Api, got: {err:?}"
        );
        assert!(
            err.to_string().contains("request failed"),
            "expected connection error in: {}",
            err
        );
    }
}
