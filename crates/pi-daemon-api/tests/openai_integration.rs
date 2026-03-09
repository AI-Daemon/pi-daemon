//! OpenAI-compatible API integration tests
//!
//! These are contract tests - if they break, every OpenAI-compatible client breaks.

use pi_daemon_api::server::build_router;
use pi_daemon_kernel::PiDaemonKernel;
use pi_daemon_types::config::DaemonConfig;
use std::sync::Arc;
use tokio::net::TcpListener;

async fn start_test_server() -> String {
    let kernel = Arc::new(PiDaemonKernel::new());
    kernel.init().await;

    let config = DaemonConfig {
        listen_addr: "127.0.0.1:0".to_string(),
        ..Default::default()
    };

    let (router, _state) = build_router(kernel, config);

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(
            listener,
            router.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .await
        .unwrap();
    });

    // Give the server a moment to start
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    format!("http://127.0.0.1:{}", addr.port())
}

#[tokio::test]
async fn test_non_streaming_chat_completion() {
    let base_url = start_test_server().await;
    let client = reqwest::Client::new();

    let request_body = serde_json::json!({
        "model": "test-model",
        "messages": [
            {"role": "user", "content": "Hello, how are you?"}
        ],
        "stream": false
    });

    let response = client
        .post(format!("{base_url}/v1/chat/completions"))
        .header("content-type", "application/json")
        .json(&request_body)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);
    assert_eq!(
        response.headers().get("content-type").unwrap(),
        "application/json"
    );

    let body: serde_json::Value = response.json().await.unwrap();

    // Verify OpenAI response schema
    assert!(body["id"].is_string());
    assert!(body["id"].as_str().unwrap().starts_with("chatcmpl-"));
    assert_eq!(body["object"], "chat.completion");
    assert!(body["created"].is_number());
    assert_eq!(body["model"], "test-model");

    // Verify choices array
    let choices = body["choices"].as_array().unwrap();
    assert_eq!(choices.len(), 1);

    let choice = &choices[0];
    assert_eq!(choice["index"], 0);
    assert_eq!(choice["message"]["role"], "assistant");
    assert!(choice["message"]["content"].is_string());
    assert!(choice["message"]["content"]
        .as_str()
        .unwrap()
        .contains("Echo"));
    assert_eq!(choice["finish_reason"], "stop");

    // Verify usage object
    let usage = &body["usage"];
    assert!(usage["prompt_tokens"].is_number());
    assert!(usage["completion_tokens"].is_number());
    assert!(usage["total_tokens"].is_number());
    assert!(usage["total_tokens"].as_u64().unwrap() > 0);
}

#[tokio::test]
async fn test_streaming_chat_completion() {
    let base_url = start_test_server().await;
    let client = reqwest::Client::new();

    let request_body = serde_json::json!({
        "model": "test-model",
        "messages": [
            {"role": "user", "content": "Tell me a joke"}
        ],
        "stream": true
    });

    let response = client
        .post(format!("{base_url}/v1/chat/completions"))
        .header("content-type", "application/json")
        .json(&request_body)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    let content_type = response
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap();
    // Axum SSE sets this to text/event-stream
    assert!(content_type.contains("text/event-stream") || content_type.contains("text/plain"));

    let body = response.text().await.unwrap();
    let lines: Vec<&str> = body.lines().collect();

    // Verify SSE format
    assert!(!lines.is_empty());

    // Filter out empty lines (valid in SSE format)
    let data_lines: Vec<&str> = lines
        .iter()
        .filter(|line| !line.is_empty())
        .copied()
        .collect();

    // Each non-empty line should start with "data: "
    for line in &data_lines {
        assert!(
            line.starts_with("data: "),
            "Line should start with 'data: ': {}",
            line
        );
    }

    // Last data line should be "data: [DONE]"
    assert_eq!(data_lines.last().unwrap(), &"data: [DONE]");

    // Parse and verify chunks
    let mut has_role_chunk = false;
    let mut has_content_chunks = false;
    let mut has_final_chunk = false;

    for line in lines {
        if line.starts_with("data: {") {
            let json_str = &line[6..]; // Remove "data: " prefix
            let chunk: serde_json::Value = serde_json::from_str(json_str).unwrap();

            // Verify chunk schema
            assert!(chunk["id"].is_string());
            assert!(chunk["id"].as_str().unwrap().starts_with("chatcmpl-"));
            assert_eq!(chunk["object"], "chat.completion.chunk");
            assert!(chunk["created"].is_number());
            assert_eq!(chunk["model"], "test-model");

            let choices = chunk["choices"].as_array().unwrap();
            assert_eq!(choices.len(), 1);

            let choice = &choices[0];
            assert_eq!(choice["index"], 0);

            let delta = &choice["delta"];
            if delta["role"].is_string() {
                assert_eq!(delta["role"], "assistant");
                has_role_chunk = true;
            }

            if delta["content"].is_string() {
                has_content_chunks = true;
            }

            if choice["finish_reason"].is_string() {
                assert_eq!(choice["finish_reason"], "stop");
                has_final_chunk = true;
            }
        }
    }

    // Verify we got all expected chunk types
    assert!(has_role_chunk, "Should have role chunk");
    assert!(has_content_chunks, "Should have content chunks");
    assert!(
        has_final_chunk,
        "Should have final chunk with finish_reason"
    );
}

#[tokio::test]
async fn test_model_resolution() {
    let base_url = start_test_server().await;
    let client = reqwest::Client::new();

    let request_body = serde_json::json!({
        "model": "custom-agent-name",
        "messages": [
            {"role": "user", "content": "Hello"}
        ],
        "stream": false
    });

    let response = client
        .post(format!("{base_url}/v1/chat/completions"))
        .header("content-type", "application/json")
        .json(&request_body)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["model"], "custom-agent-name");

    // Response should contain the model name in content
    let content = body["choices"][0]["message"]["content"].as_str().unwrap();
    assert!(content.contains("custom-agent-name"));
}

#[tokio::test]
async fn test_message_content_extraction() {
    let base_url = start_test_server().await;
    let client = reqwest::Client::new();

    // Test with complex message structure
    let request_body = serde_json::json!({
        "model": "test",
        "messages": [
            {"role": "system", "content": "You are a helpful assistant"},
            {"role": "user", "content": "What is 2+2?"},
            {"role": "assistant", "content": "4"},
            {"role": "user", "content": "And 3+3?"}
        ],
        "stream": false
    });

    let response = client
        .post(format!("{base_url}/v1/chat/completions"))
        .header("content-type", "application/json")
        .json(&request_body)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    let body: serde_json::Value = response.json().await.unwrap();
    let content = body["choices"][0]["message"]["content"].as_str().unwrap();

    // Should use the last user message
    assert!(content.contains("And 3+3?"));
}

#[tokio::test]
async fn test_multipart_content() {
    let base_url = start_test_server().await;
    let client = reqwest::Client::new();

    let request_body = serde_json::json!({
        "model": "test",
        "messages": [
            {
                "role": "user",
                "content": [
                    {"type": "text", "text": "Describe this image"},
                    {"type": "text", "text": "Also tell me about colors"}
                ]
            }
        ],
        "stream": false
    });

    let response = client
        .post(format!("{base_url}/v1/chat/completions"))
        .header("content-type", "application/json")
        .json(&request_body)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    let body: serde_json::Value = response.json().await.unwrap();
    let content = body["choices"][0]["message"]["content"].as_str().unwrap();

    // Should combine text parts
    assert!(content.contains("Describe this image"));
    assert!(content.contains("Also tell me about colors"));
}

#[tokio::test]
async fn test_empty_messages_error() {
    let base_url = start_test_server().await;
    let client = reqwest::Client::new();

    let request_body = serde_json::json!({
        "model": "test",
        "messages": [],
        "stream": false
    });

    let response = client
        .post(format!("{base_url}/v1/chat/completions"))
        .header("content-type", "application/json")
        .json(&request_body)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 400);

    let body: serde_json::Value = response.json().await.unwrap();

    // Verify error format
    assert!(body["error"].is_object());
    assert!(body["error"]["message"].is_string());
    assert_eq!(body["error"]["type"], "invalid_request_error");
    assert!(body["error"]["message"]
        .as_str()
        .unwrap()
        .contains("At least one message"));
}

#[tokio::test]
async fn test_optional_parameters() {
    let base_url = start_test_server().await;
    let client = reqwest::Client::new();

    let request_body = serde_json::json!({
        "model": "test",
        "messages": [
            {"role": "user", "content": "Hello"}
        ],
        "max_tokens": 100,
        "temperature": 0.7,
        "top_p": 0.9,
        "stop": ["END"],
        "stream": false
    });

    let response = client
        .post(format!("{base_url}/v1/chat/completions"))
        .header("content-type", "application/json")
        .json(&request_body)
        .send()
        .await
        .unwrap();

    // Should accept all optional parameters without error
    assert_eq!(response.status(), 200);

    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["object"], "chat.completion");
}

#[tokio::test]
async fn test_malformed_request() {
    let base_url = start_test_server().await;
    let client = reqwest::Client::new();

    let request_body = r#"{"invalid": "json", "missing": "required fields"}"#;

    let response = client
        .post(format!("{base_url}/v1/chat/completions"))
        .header("content-type", "application/json")
        .body(request_body)
        .send()
        .await
        .unwrap();

    // Should return error for malformed request
    assert_eq!(response.status(), 422);
}

#[tokio::test]
async fn test_unique_completion_ids() {
    let base_url = start_test_server().await;
    let client = reqwest::Client::new();

    let request_body = serde_json::json!({
        "model": "test",
        "messages": [
            {"role": "user", "content": "Hello"}
        ],
        "stream": false
    });

    let mut completion_ids = std::collections::HashSet::new();

    // Make multiple requests
    for _ in 0..5 {
        let response = client
            .post(format!("{base_url}/v1/chat/completions"))
            .header("content-type", "application/json")
            .json(&request_body)
            .send()
            .await
            .unwrap();

        assert_eq!(response.status(), 200);

        let body: serde_json::Value = response.json().await.unwrap();
        let id = body["id"].as_str().unwrap();

        // Each completion should have unique ID
        assert!(
            completion_ids.insert(id.to_string()),
            "Completion ID should be unique: {}",
            id
        );
    }
}

#[tokio::test]
async fn test_openai_client_compatibility() {
    // Test basic structure that would work with OpenAI client libraries
    let base_url = start_test_server().await;
    let client = reqwest::Client::new();

    let request_body = serde_json::json!({
        "model": "gpt-3.5-turbo",  // Common OpenAI model name
        "messages": [
            {"role": "system", "content": "You are a helpful assistant."},
            {"role": "user", "content": "Say hello"}
        ],
        "temperature": 0.7,
        "max_tokens": 150
    });

    let response = client
        .post(format!("{base_url}/v1/chat/completions"))
        .header("content-type", "application/json")
        .json(&request_body)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    let body: serde_json::Value = response.json().await.unwrap();

    // Verify all fields that OpenAI clients expect
    assert!(body.get("id").is_some());
    assert_eq!(body["object"], "chat.completion");
    assert!(body.get("created").is_some());
    assert_eq!(body["model"], "gpt-3.5-turbo");
    assert!(body["choices"].is_array());
    assert!(body["usage"].is_object());

    let choice = &body["choices"][0];
    assert!(choice.get("index").is_some());
    assert!(choice.get("message").is_some());
    assert!(choice.get("finish_reason").is_some());

    let message = &choice["message"];
    assert!(message.get("role").is_some());
    assert!(message.get("content").is_some());

    let usage = &body["usage"];
    assert!(usage.get("prompt_tokens").is_some());
    assert!(usage.get("completion_tokens").is_some());
    assert!(usage.get("total_tokens").is_some());
}
