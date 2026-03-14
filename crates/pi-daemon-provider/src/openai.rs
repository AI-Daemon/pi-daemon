//! OpenAI provider — streaming completions from the OpenAI Chat API.

use async_trait::async_trait;
use futures::StreamExt;
use pi_daemon_types::error::DaemonError;
use pi_daemon_types::message::{Message, TokenUsage};
use tracing::{debug, warn};

use crate::convert::to_openai_messages;
use crate::provider::Provider;
use crate::sse;
use crate::types::{CompletionOptions, CompletionStream, StreamEvent};

const DEFAULT_BASE_URL: &str = "https://api.openai.com";
const REQUEST_TIMEOUT_SECS: u64 = 120;
const MAX_RETRIES: u32 = 3;

/// Client for the OpenAI Chat Completions API.
pub struct OpenAIProvider {
    client: reqwest::Client,
    api_key: String,
    base_url: String,
    /// Extra headers to send with every request (used by OpenRouter).
    extra_headers: Vec<(String, String)>,
}

impl OpenAIProvider {
    /// Create a new OpenAI provider.
    pub fn new(api_key: String, base_url: Option<String>) -> Result<Self, DaemonError> {
        Self::with_headers(api_key, base_url, vec![])
    }

    /// Create an OpenAI-compatible provider with extra headers.
    pub(crate) fn with_headers(
        api_key: String,
        base_url: Option<String>,
        extra_headers: Vec<(String, String)>,
    ) -> Result<Self, DaemonError> {
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
            extra_headers,
        })
    }

    /// Build the request body for the Chat Completions API.
    fn build_body(
        &self,
        model: &str,
        messages: &[Message],
        options: &CompletionOptions,
    ) -> serde_json::Value {
        let api_messages = to_openai_messages(messages, options.system_prompt.as_deref());

        let mut body = serde_json::json!({
            "model": model,
            "messages": api_messages,
            "max_completion_tokens": options.max_tokens,
            "stream": true,
            "stream_options": { "include_usage": true },
        });

        if let Some(temp) = options.temperature {
            body["temperature"] = serde_json::json!(temp);
        }
        if let Some(top_p) = options.top_p {
            body["top_p"] = serde_json::json!(top_p);
        }
        if !options.stop_sequences.is_empty() {
            body["stop"] = serde_json::json!(options.stop_sequences);
        }

        body
    }

    /// Send the request with retry logic for transient failures.
    async fn send_with_retry(
        &self,
        body: &serde_json::Value,
    ) -> Result<reqwest::Response, DaemonError> {
        let url = format!("{}/v1/chat/completions", self.base_url);

        for attempt in 0..MAX_RETRIES {
            let mut req = self
                .client
                .post(&url)
                .header("authorization", format!("Bearer {}", self.api_key))
                .header("content-type", "application/json");

            for (key, value) in &self.extra_headers {
                req = req.header(key.as_str(), value.as_str());
            }

            let resp = req
                .json(body)
                .send()
                .await
                .map_err(|e| DaemonError::Api(format!("OpenAI request failed: {e}")))?;

            let status = resp.status();

            if status.is_success() {
                return Ok(resp);
            }

            if (status == reqwest::StatusCode::TOO_MANY_REQUESTS || status.is_server_error())
                && attempt < MAX_RETRIES - 1
            {
                let delay = std::time::Duration::from_millis(500 * 2u64.pow(attempt));
                warn!(
                    status = %status,
                    attempt = attempt + 1,
                    delay_ms = delay.as_millis(),
                    "OpenAI transient error, retrying"
                );
                tokio::time::sleep(delay).await;
                continue;
            }

            let error_body = match resp.text().await {
                Ok(body) => body,
                Err(e) => {
                    warn!("Failed to read OpenAI error response body: {e}");
                    format!("<unreadable: {e}>")
                }
            };
            return Err(DaemonError::Api(format!(
                "OpenAI API error {status}: {error_body}"
            )));
        }

        Err(DaemonError::Api("OpenAI: max retries exceeded".into()))
    }
}

#[async_trait]
impl Provider for OpenAIProvider {
    async fn complete(
        &self,
        model: &str,
        messages: Vec<Message>,
        options: CompletionOptions,
    ) -> Result<CompletionStream, DaemonError> {
        let body = self.build_body(model, &messages, &options);
        debug!(model, "Sending OpenAI streaming request");

        let response = self.send_with_retry(&body).await?;
        let event_stream = sse::parse_sse(response);

        let stream = async_stream::stream! {
            // State for accumulating tool calls (keyed by index)
            let mut tool_calls: std::collections::HashMap<u32, (String, String, String)> =
                std::collections::HashMap::new(); // index -> (id, name, arguments)

            let mut usage = TokenUsage::default();
            let mut got_usage = false;

            futures::pin_mut!(event_stream);

            while let Some(sse_result) = event_stream.next().await {
                let sse_event = match sse_result {
                    Ok(e) => e,
                    Err(e) => {
                        yield StreamEvent::Error(e);
                        return;
                    }
                };

                // OpenAI sends `data: [DONE]` as the final message
                if sse_event.data.trim() == "[DONE]" {
                    // Flush any remaining tool calls
                    for (_, (id, name, args)) in tool_calls.drain() {
                        let input: serde_json::Value = serde_json::from_str(&args)
                            .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
                        yield StreamEvent::ToolUse { id, name, input };
                    }
                    if !got_usage {
                        yield StreamEvent::Done(usage.clone());
                    }
                    return;
                }

                let data: serde_json::Value = match serde_json::from_str(&sse_event.data) {
                    Ok(d) => d,
                    Err(_) => continue,
                };

                // Check for usage (sent as a separate chunk with include_usage)
                if let Some(u) = data.get("usage").filter(|u| !u.is_null()) {
                    usage.input_tokens = u.get("prompt_tokens")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0) as u32;
                    usage.output_tokens = u.get("completion_tokens")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0) as u32;

                    // Flush any remaining tool calls before Done
                    for (_, (id, name, args)) in tool_calls.drain() {
                        let input: serde_json::Value = serde_json::from_str(&args)
                            .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
                        yield StreamEvent::ToolUse { id, name, input };
                    }

                    got_usage = true;
                    yield StreamEvent::Done(usage.clone());
                    continue;
                }

                // Process choices
                if let Some(choices) = data.get("choices").and_then(|c| c.as_array()) {
                    for choice in choices {
                        if let Some(delta) = choice.get("delta") {
                            // Text content
                            if let Some(content) = delta.get("content").and_then(|c| c.as_str()) {
                                if !content.is_empty() {
                                    yield StreamEvent::TextDelta(content.to_string());
                                }
                            }

                            // Tool calls (streamed in chunks)
                            if let Some(tcs) = delta.get("tool_calls").and_then(|t| t.as_array()) {
                                for tc in tcs {
                                    let index = tc.get("index")
                                        .and_then(|i| i.as_u64())
                                        .unwrap_or(0) as u32;

                                    let entry = tool_calls.entry(index).or_insert_with(|| {
                                        (String::new(), String::new(), String::new())
                                    });

                                    if let Some(id) = tc.get("id").and_then(|i| i.as_str()) {
                                        entry.0 = id.to_string();
                                    }
                                    if let Some(func) = tc.get("function") {
                                        if let Some(name) = func.get("name").and_then(|n| n.as_str()) {
                                            entry.1 = name.to_string();
                                        }
                                        if let Some(args) = func.get("arguments").and_then(|a| a.as_str()) {
                                            entry.2.push_str(args);
                                        }
                                    }
                                }
                            }
                        }
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
    fn test_openai_provider_default_base_url() {
        let p = OpenAIProvider::new("test-key".into(), None).unwrap();
        assert_eq!(p.base_url, DEFAULT_BASE_URL);
        assert_eq!(p.api_key, "test-key");
        assert!(p.extra_headers.is_empty());
    }

    #[test]
    fn test_openai_provider_custom_base_url() {
        let p =
            OpenAIProvider::new("key".into(), Some("https://proxy.example.com/".into())).unwrap();
        assert_eq!(p.base_url, "https://proxy.example.com");
    }

    #[test]
    fn test_build_body_basic() {
        let p = OpenAIProvider::new("key".into(), None).unwrap();
        let messages = vec![Message {
            role: pi_daemon_types::message::Role::User,
            content: pi_daemon_types::message::MessageContent::Text("Hello".into()),
        }];
        let options = CompletionOptions::default();

        let body = p.build_body("gpt-4o", &messages, &options);
        assert_eq!(body["model"], "gpt-4o");
        assert_eq!(body["stream"], true);
        assert_eq!(body["max_completion_tokens"], 8192);
        assert_eq!(body["stream_options"]["include_usage"], true);
    }

    #[test]
    fn test_build_body_with_system() {
        let p = OpenAIProvider::new("key".into(), None).unwrap();
        let messages = vec![Message {
            role: pi_daemon_types::message::Role::User,
            content: pi_daemon_types::message::MessageContent::Text("Hello".into()),
        }];
        let options = CompletionOptions {
            system_prompt: Some("You are helpful.".into()),
            ..Default::default()
        };

        let body = p.build_body("gpt-4o", &messages, &options);
        // System should be the first message
        let msgs = body["messages"].as_array().unwrap();
        assert_eq!(msgs[0]["role"], "system");
        assert_eq!(msgs[0]["content"], "You are helpful.");
    }

    /// Helper: start an axum server that always returns the given status and body.
    async fn start_mock_server(status: axum::http::StatusCode, body: &'static str) -> String {
        use axum::{routing::post, Router};

        let app = Router::new().route(
            "/v1/chat/completions",
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

        let provider = OpenAIProvider::new("test-key".into(), Some(base)).unwrap();
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

        let provider = OpenAIProvider::new("bad-key".into(), Some(base)).unwrap();
        let body = serde_json::json!({"model": "gpt-4o", "stream": true});

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

        let provider = OpenAIProvider::new("key".into(), Some(base)).unwrap();
        let body = serde_json::json!({"model": "gpt-4o", "stream": true});

        let err = provider.send_with_retry(&body).await.unwrap_err();
        assert!(matches!(err, DaemonError::Api(_)));
        assert!(err.to_string().contains("403"));
    }

    #[tokio::test]
    async fn test_send_with_retry_success_returns_ok() {
        let base = start_mock_server(axum::http::StatusCode::OK, "{}").await;

        let provider = OpenAIProvider::new("key".into(), Some(base)).unwrap();
        let body = serde_json::json!({"model": "gpt-4o", "stream": true});

        let resp = provider.send_with_retry(&body).await;
        assert!(resp.is_ok(), "expected Ok, got: {resp:?}");
    }

    #[tokio::test]
    async fn test_send_with_retry_connection_refused() {
        // Point at a port with nothing listening
        let provider =
            OpenAIProvider::new("key".into(), Some("http://127.0.0.1:1".into())).unwrap();
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
