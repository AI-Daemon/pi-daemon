//! Integration tests for LLM providers using mock HTTP servers.
//!
//! These tests spin up a local axum server that mimics the SSE responses from
//! Anthropic and OpenAI, so we can verify streaming logic without real API keys.

use axum::{extract::Request, response::IntoResponse, routing::post, Router};
use futures::StreamExt;
use pi_daemon_provider::{
    AnthropicProvider, CompletionOptions, OpenAIProvider, OpenRouterProvider, Provider,
    ProviderRouter, StreamEvent,
};
use pi_daemon_types::config::ProvidersConfig;
use pi_daemon_types::message::{Message, MessageContent, Role};
use std::net::SocketAddr;
use tokio::net::TcpListener;

/// Start a mock server and return its base URL.
async fn start_mock_server(app: Router) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr: SocketAddr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    format!("http://{addr}")
}

/// Build an SSE response body from raw SSE text.
fn sse_response(body: &str) -> impl IntoResponse {
    (
        axum::http::StatusCode::OK,
        [("content-type", "text/event-stream")],
        body.to_string(),
    )
}

fn user_message(text: &str) -> Vec<Message> {
    vec![Message {
        role: Role::User,
        content: MessageContent::Text(text.to_string()),
    }]
}

// ---------------------------------------------------------------------------
// Anthropic streaming tests
// ---------------------------------------------------------------------------

fn mock_anthropic_sse() -> &'static str {
    concat!(
        "event: message_start\n",
        "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_01\",\"type\":\"message\",\"role\":\"assistant\",\"content\":[],\"model\":\"claude-sonnet-4-20250514\",\"usage\":{\"input_tokens\":25,\"output_tokens\":0}}}\n\n",
        "event: content_block_start\n",
        "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n",
        "event: content_block_delta\n",
        "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Hello\"}}\n\n",
        "event: content_block_delta\n",
        "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\" world!\"}}\n\n",
        "event: content_block_stop\n",
        "data: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
        "event: message_delta\n",
        "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":12}}\n\n",
        "event: message_stop\n",
        "data: {\"type\":\"message_stop\"}\n\n",
    )
}

#[tokio::test]
async fn test_anthropic_streaming_text() {
    let app = Router::new().route(
        "/v1/messages",
        post(|| async { sse_response(mock_anthropic_sse()) }),
    );
    let base_url = start_mock_server(app).await;

    let provider = AnthropicProvider::new("test-key".into(), Some(base_url)).unwrap();
    let mut stream = provider
        .complete(
            "claude-sonnet-4-20250514",
            user_message("Hi"),
            CompletionOptions::default(),
        )
        .await
        .unwrap();

    let mut text = String::new();
    let mut got_done = false;

    while let Some(event) = stream.next().await {
        match event {
            StreamEvent::TextDelta(t) => text.push_str(&t),
            StreamEvent::Done(usage) => {
                assert_eq!(usage.input_tokens, 25);
                assert_eq!(usage.output_tokens, 12);
                got_done = true;
            }
            StreamEvent::Error(e) => panic!("Unexpected error: {e}"),
            _ => {}
        }
    }

    assert_eq!(text, "Hello world!");
    assert!(got_done, "Should have received Done event");
}

fn mock_anthropic_tool_use_sse() -> &'static str {
    concat!(
        "event: message_start\n",
        "data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_02\",\"type\":\"message\",\"role\":\"assistant\",\"content\":[],\"model\":\"claude-sonnet-4-20250514\",\"usage\":{\"input_tokens\":30,\"output_tokens\":0}}}\n\n",
        "event: content_block_start\n",
        "data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"tool_use\",\"id\":\"toolu_01\",\"name\":\"get_weather\",\"input\":{}}}\n\n",
        "event: content_block_delta\n",
        "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"city\\\": \"}}\n\n",
        "event: content_block_delta\n",
        "data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"\\\"London\\\"}\"}}\n\n",
        "event: content_block_stop\n",
        "data: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
        "event: message_delta\n",
        "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"tool_use\"},\"usage\":{\"output_tokens\":8}}\n\n",
        "event: message_stop\n",
        "data: {\"type\":\"message_stop\"}\n\n",
    )
}

#[tokio::test]
async fn test_anthropic_streaming_tool_use() {
    let app = Router::new().route(
        "/v1/messages",
        post(|| async { sse_response(mock_anthropic_tool_use_sse()) }),
    );
    let base_url = start_mock_server(app).await;

    let provider = AnthropicProvider::new("test-key".into(), Some(base_url)).unwrap();
    let mut stream = provider
        .complete(
            "claude-sonnet-4-20250514",
            user_message("What's the weather in London?"),
            CompletionOptions::default(),
        )
        .await
        .unwrap();

    let mut got_tool = false;
    let mut got_done = false;

    while let Some(event) = stream.next().await {
        match event {
            StreamEvent::ToolUse { id, name, input } => {
                assert_eq!(id, "toolu_01");
                assert_eq!(name, "get_weather");
                assert_eq!(input["city"], "London");
                got_tool = true;
            }
            StreamEvent::Done(usage) => {
                assert_eq!(usage.input_tokens, 30);
                assert_eq!(usage.output_tokens, 8);
                got_done = true;
            }
            StreamEvent::Error(e) => panic!("Unexpected error: {e}"),
            _ => {}
        }
    }

    assert!(got_tool, "Should have received ToolUse event");
    assert!(got_done, "Should have received Done event");
}

// ---------------------------------------------------------------------------
// OpenAI streaming tests
// ---------------------------------------------------------------------------

fn mock_openai_sse() -> &'static str {
    concat!(
        "data: {\"id\":\"chatcmpl-01\",\"object\":\"chat.completion.chunk\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":\"\"},\"finish_reason\":null}]}\n\n",
        "data: {\"id\":\"chatcmpl-01\",\"object\":\"chat.completion.chunk\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Hello\"},\"finish_reason\":null}]}\n\n",
        "data: {\"id\":\"chatcmpl-01\",\"object\":\"chat.completion.chunk\",\"choices\":[{\"index\":0,\"delta\":{\"content\":\" there!\"},\"finish_reason\":null}]}\n\n",
        "data: {\"id\":\"chatcmpl-01\",\"object\":\"chat.completion.chunk\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}\n\n",
        "data: {\"id\":\"chatcmpl-01\",\"object\":\"chat.completion.chunk\",\"choices\":[],\"usage\":{\"prompt_tokens\":10,\"completion_tokens\":5,\"total_tokens\":15}}\n\n",
        "data: [DONE]\n\n",
    )
}

#[tokio::test]
async fn test_openai_streaming_text() {
    let app = Router::new().route(
        "/v1/chat/completions",
        post(|| async { sse_response(mock_openai_sse()) }),
    );
    let base_url = start_mock_server(app).await;

    let provider = OpenAIProvider::new("test-key".into(), Some(base_url)).unwrap();
    let mut stream = provider
        .complete("gpt-4o", user_message("Hi"), CompletionOptions::default())
        .await
        .unwrap();

    let mut text = String::new();
    let mut got_done = false;

    while let Some(event) = stream.next().await {
        match event {
            StreamEvent::TextDelta(t) => text.push_str(&t),
            StreamEvent::Done(usage) => {
                assert_eq!(usage.input_tokens, 10);
                assert_eq!(usage.output_tokens, 5);
                got_done = true;
            }
            StreamEvent::Error(e) => panic!("Unexpected error: {e}"),
            _ => {}
        }
    }

    assert_eq!(text, "Hello there!");
    assert!(got_done, "Should have received Done event");
}

fn mock_openai_tool_call_sse() -> &'static str {
    concat!(
        "data: {\"id\":\"chatcmpl-02\",\"object\":\"chat.completion.chunk\",\"choices\":[{\"index\":0,\"delta\":{\"role\":\"assistant\",\"content\":null,\"tool_calls\":[{\"index\":0,\"id\":\"call_abc\",\"type\":\"function\",\"function\":{\"name\":\"get_weather\",\"arguments\":\"\"}}]},\"finish_reason\":null}]}\n\n",
        "data: {\"id\":\"chatcmpl-02\",\"object\":\"chat.completion.chunk\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"{\\\"city\\\"\"}}]},\"finish_reason\":null}]}\n\n",
        "data: {\"id\":\"chatcmpl-02\",\"object\":\"chat.completion.chunk\",\"choices\":[{\"index\":0,\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\": \\\"London\\\"}\"}}]},\"finish_reason\":null}]}\n\n",
        "data: {\"id\":\"chatcmpl-02\",\"object\":\"chat.completion.chunk\",\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"tool_calls\"}]}\n\n",
        "data: {\"id\":\"chatcmpl-02\",\"object\":\"chat.completion.chunk\",\"choices\":[],\"usage\":{\"prompt_tokens\":15,\"completion_tokens\":10,\"total_tokens\":25}}\n\n",
        "data: [DONE]\n\n",
    )
}

#[tokio::test]
async fn test_openai_streaming_tool_call() {
    let app = Router::new().route(
        "/v1/chat/completions",
        post(|| async { sse_response(mock_openai_tool_call_sse()) }),
    );
    let base_url = start_mock_server(app).await;

    let provider = OpenAIProvider::new("test-key".into(), Some(base_url)).unwrap();
    let mut stream = provider
        .complete(
            "gpt-4o",
            user_message("Weather in London?"),
            CompletionOptions::default(),
        )
        .await
        .unwrap();

    let mut got_tool = false;
    let mut got_done = false;

    while let Some(event) = stream.next().await {
        match event {
            StreamEvent::ToolUse { id, name, input } => {
                assert_eq!(id, "call_abc");
                assert_eq!(name, "get_weather");
                assert_eq!(input["city"], "London");
                got_tool = true;
            }
            StreamEvent::Done(usage) => {
                assert_eq!(usage.input_tokens, 15);
                assert_eq!(usage.output_tokens, 10);
                got_done = true;
            }
            StreamEvent::Error(e) => panic!("Unexpected error: {e}"),
            _ => {}
        }
    }

    assert!(got_tool, "Should have received ToolUse event");
    assert!(got_done, "Should have received Done event");
}

// ---------------------------------------------------------------------------
// OpenRouter tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_openrouter_streaming() {
    // OpenRouter uses the same SSE format as OpenAI
    let app = Router::new().route(
        "/v1/chat/completions",
        post(|| async { sse_response(mock_openai_sse()) }),
    );
    let base_url = start_mock_server(app).await;

    let provider = OpenRouterProvider::new("test-key".into(), Some(base_url)).unwrap();
    let mut stream = provider
        .complete(
            "deepseek/deepseek-coder",
            user_message("Hi"),
            CompletionOptions::default(),
        )
        .await
        .unwrap();

    let mut text = String::new();
    let mut got_done = false;

    while let Some(event) = stream.next().await {
        match event {
            StreamEvent::TextDelta(t) => text.push_str(&t),
            StreamEvent::Done(usage) => {
                assert_eq!(usage.input_tokens, 10);
                assert_eq!(usage.output_tokens, 5);
                got_done = true;
            }
            StreamEvent::Error(e) => panic!("Unexpected error: {e}"),
            _ => {}
        }
    }

    assert_eq!(text, "Hello there!");
    assert!(got_done, "Should have received Done event");
}

// ---------------------------------------------------------------------------
// Error handling tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_anthropic_api_error() {
    let app = Router::new().route(
        "/v1/messages",
        post(|| async {
            (
                axum::http::StatusCode::UNAUTHORIZED,
                r#"{"error":{"message":"Invalid API key"}}"#,
            )
        }),
    );
    let base_url = start_mock_server(app).await;

    let provider = AnthropicProvider::new("bad-key".into(), Some(base_url)).unwrap();
    let result = provider
        .complete(
            "claude-sonnet-4-20250514",
            user_message("Hi"),
            CompletionOptions::default(),
        )
        .await;

    match result {
        Err(e) => assert!(
            e.to_string().contains("401"),
            "Error should contain 401: {e}"
        ),
        Ok(_) => panic!("Expected error for 401 response"),
    }
}

#[tokio::test]
async fn test_openai_api_error() {
    let app = Router::new().route(
        "/v1/chat/completions",
        post(|| async {
            (
                axum::http::StatusCode::FORBIDDEN,
                r#"{"error":{"message":"Forbidden"}}"#,
            )
        }),
    );
    let base_url = start_mock_server(app).await;

    let provider = OpenAIProvider::new("bad-key".into(), Some(base_url)).unwrap();
    let result = provider
        .complete("gpt-4o", user_message("Hi"), CompletionOptions::default())
        .await;

    match result {
        Err(e) => assert!(
            e.to_string().contains("403"),
            "Error should contain 403: {e}"
        ),
        Ok(_) => panic!("Expected error for 403 response"),
    }
}

// ---------------------------------------------------------------------------
// Retry tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_anthropic_retry_on_429() {
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    let count = Arc::new(AtomicU32::new(0));
    let count_clone = count.clone();

    let app = Router::new().route(
        "/v1/messages",
        post(move |_req: Request| {
            let count = count_clone.clone();
            async move {
                let n = count.fetch_add(1, Ordering::SeqCst);
                if n < 2 {
                    (
                        axum::http::StatusCode::TOO_MANY_REQUESTS,
                        "rate limited".to_string(),
                    )
                        .into_response()
                } else {
                    sse_response(mock_anthropic_sse()).into_response()
                }
            }
        }),
    );

    let base_url = start_mock_server(app).await;
    let provider = AnthropicProvider::new("test-key".into(), Some(base_url)).unwrap();
    let mut stream = provider
        .complete(
            "claude-sonnet-4-20250514",
            user_message("Hi"),
            CompletionOptions::default(),
        )
        .await
        .unwrap();

    let mut text = String::new();
    while let Some(event) = stream.next().await {
        if let StreamEvent::TextDelta(t) = event {
            text.push_str(&t);
        }
    }

    assert_eq!(text, "Hello world!");
    assert_eq!(count.load(Ordering::SeqCst), 3, "Should have retried twice");
}

// ---------------------------------------------------------------------------
// Router integration tests
// ---------------------------------------------------------------------------

#[test]
fn test_router_routes_claude_to_anthropic() {
    let config = ProvidersConfig {
        anthropic_api_key: "sk-ant-test".to_string(),
        ..Default::default()
    };
    let router = ProviderRouter::from_config(&config).unwrap();
    assert!(router.route("claude-sonnet-4-20250514").is_ok());
    // Only Anthropic key configured — OpenAI models should fail
    assert!(router.route("gpt-4o").is_err());
}

#[test]
fn test_router_routes_gpt_to_openai() {
    let config = ProvidersConfig {
        openai_api_key: "sk-openai-test".to_string(),
        ..Default::default()
    };
    let router = ProviderRouter::from_config(&config).unwrap();
    assert!(router.route("gpt-4o").is_ok());
    assert!(router.route("o3-mini").is_ok());
    // Only OpenAI key configured — Anthropic models should fail
    assert!(router.route("claude-sonnet-4-20250514").is_err());
}

#[test]
fn test_router_missing_key_errors() {
    let router = ProviderRouter::from_config(&ProvidersConfig::default()).unwrap();
    assert!(router.route("claude-sonnet-4-20250514").is_err());
    assert!(router.route("gpt-4o").is_err());
    assert!(router.route("some-model").is_err());
}
