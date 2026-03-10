//! OpenAI-compatible API integration tests
//!
//! These are contract tests - if they break, every OpenAI-compatible client breaks.

use pi_daemon_test_utils::{assert_openai_completion, FullTestServer};

#[tokio::test]
async fn test_non_streaming_chat_completion() {
    let server = FullTestServer::new().await;
    let client = server.client();

    let request_body = serde_json::json!({
        "model": "test-model",
        "messages": [
            {"role": "user", "content": "Hello, how are you?"}
        ],
        "stream": false
    });

    let response = client
        .post_json("/v1/chat/completions", &request_body)
        .await;

    assert_eq!(response.status(), 200);
    assert_eq!(
        response.headers().get("content-type").unwrap(),
        "application/json"
    );

    let body: serde_json::Value = response.json().await.unwrap();

    // Use the shared assertion macro for schema validation
    assert_openai_completion!(body);
    assert_eq!(body["model"], "test-model");

    // Verify content is present
    assert!(body["choices"][0]["message"]["content"]
        .as_str()
        .unwrap()
        .contains("Echo"));
}

#[tokio::test]
async fn test_streaming_chat_completion() {
    let server = FullTestServer::new().await;
    let client = server.client();

    let request_body = serde_json::json!({
        "model": "test-model",
        "messages": [
            {"role": "user", "content": "Tell me a joke"}
        ],
        "stream": true
    });

    let response = client
        .post_json("/v1/chat/completions", &request_body)
        .await;

    assert_eq!(response.status(), 200);

    let content_type = response
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(content_type.contains("text/event-stream") || content_type.contains("text/plain"));

    let body = response.text().await.unwrap();
    let lines: Vec<&str> = body.lines().collect();

    assert!(!lines.is_empty());

    let data_lines: Vec<&str> = lines
        .iter()
        .filter(|line| !line.is_empty())
        .copied()
        .collect();

    for line in &data_lines {
        assert!(
            line.starts_with("data: "),
            "Line should start with 'data: ': {}",
            line
        );
    }

    assert_eq!(data_lines.last().unwrap(), &"data: [DONE]");

    // Parse and verify chunks
    let mut has_role_chunk = false;
    let mut has_content_chunks = false;
    let mut has_final_chunk = false;

    for line in lines {
        if line.starts_with("data: {") {
            let json_str = &line[6..];
            let chunk: serde_json::Value = serde_json::from_str(json_str).unwrap();

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

    assert!(has_role_chunk, "Should have role chunk");
    assert!(has_content_chunks, "Should have content chunks");
    assert!(
        has_final_chunk,
        "Should have final chunk with finish_reason"
    );
}

#[tokio::test]
async fn test_model_resolution() {
    let server = FullTestServer::new().await;
    let client = server.client();

    let body = client
        .post_json_expect(
            "/v1/chat/completions",
            &serde_json::json!({
                "model": "custom-agent-name",
                "messages": [{"role": "user", "content": "Hello"}],
                "stream": false
            }),
            200,
        )
        .await;

    assert_eq!(body["model"], "custom-agent-name");
    let content = body["choices"][0]["message"]["content"].as_str().unwrap();
    assert!(content.contains("custom-agent-name"));
}

#[tokio::test]
async fn test_message_content_extraction() {
    let server = FullTestServer::new().await;
    let client = server.client();

    let body = client
        .post_json_expect(
            "/v1/chat/completions",
            &serde_json::json!({
                "model": "test",
                "messages": [
                    {"role": "system", "content": "You are a helpful assistant"},
                    {"role": "user", "content": "What is 2+2?"},
                    {"role": "assistant", "content": "4"},
                    {"role": "user", "content": "And 3+3?"}
                ],
                "stream": false
            }),
            200,
        )
        .await;

    let content = body["choices"][0]["message"]["content"].as_str().unwrap();
    assert!(content.contains("And 3+3?"));
}

#[tokio::test]
async fn test_multipart_content() {
    let server = FullTestServer::new().await;
    let client = server.client();

    let body = client
        .post_json_expect(
            "/v1/chat/completions",
            &serde_json::json!({
                "model": "test",
                "messages": [{
                    "role": "user",
                    "content": [
                        {"type": "text", "text": "Describe this image"},
                        {"type": "text", "text": "Also tell me about colors"}
                    ]
                }],
                "stream": false
            }),
            200,
        )
        .await;

    let content = body["choices"][0]["message"]["content"].as_str().unwrap();
    assert!(content.contains("Describe this image"));
    assert!(content.contains("Also tell me about colors"));
}

#[tokio::test]
async fn test_empty_messages_error() {
    let server = FullTestServer::new().await;
    let client = server.client();

    let response = client
        .post_json(
            "/v1/chat/completions",
            &serde_json::json!({
                "model": "test",
                "messages": [],
                "stream": false
            }),
        )
        .await;

    assert_eq!(response.status(), 400);

    let body: serde_json::Value = response.json().await.unwrap();
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
    let server = FullTestServer::new().await;
    let client = server.client();

    let body = client
        .post_json_expect(
            "/v1/chat/completions",
            &serde_json::json!({
                "model": "test",
                "messages": [{"role": "user", "content": "Hello"}],
                "max_tokens": 100,
                "temperature": 0.7,
                "top_p": 0.9,
                "stop": ["END"],
                "stream": false
            }),
            200,
        )
        .await;

    assert_eq!(body["object"], "chat.completion");
}

#[tokio::test]
async fn test_malformed_request() {
    let server = FullTestServer::new().await;
    let client = server.client();

    let response = client
        .post_raw(
            "/v1/chat/completions",
            r#"{"invalid": "json", "missing": "required fields"}"#,
            "application/json",
        )
        .await;

    assert_eq!(response.status(), 422);
}

#[tokio::test]
async fn test_unique_completion_ids() {
    let server = FullTestServer::new().await;
    let client = server.client();

    let request_body = serde_json::json!({
        "model": "test",
        "messages": [{"role": "user", "content": "Hello"}],
        "stream": false
    });

    let mut completion_ids = std::collections::HashSet::new();

    for _ in 0..5 {
        let body = client
            .post_json_expect("/v1/chat/completions", &request_body, 200)
            .await;
        let id = body["id"].as_str().unwrap();
        assert!(
            completion_ids.insert(id.to_string()),
            "Completion ID should be unique: {}",
            id
        );
    }
}

#[tokio::test]
async fn test_openai_client_compatibility() {
    let server = FullTestServer::new().await;
    let client = server.client();

    let body = client
        .post_json_expect(
            "/v1/chat/completions",
            &serde_json::json!({
                "model": "gpt-3.5-turbo",
                "messages": [
                    {"role": "system", "content": "You are a helpful assistant."},
                    {"role": "user", "content": "Say hello"}
                ],
                "temperature": 0.7,
                "max_tokens": 150
            }),
            200,
        )
        .await;

    // Verify all fields that OpenAI clients expect
    assert_openai_completion!(body);
    assert_eq!(body["model"], "gpt-3.5-turbo");
}

// ─── New edge case tests ─────────────────────────────────

#[tokio::test]
async fn test_unicode_content_handling() {
    let server = FullTestServer::new().await;
    let client = server.client();

    let body = client
        .post_json_expect(
            "/v1/chat/completions",
            &serde_json::json!({
                "model": "test",
                "messages": [{"role": "user", "content": "こんにちは 🌍 مرحبا"}],
                "stream": false
            }),
            200,
        )
        .await;

    let content = body["choices"][0]["message"]["content"].as_str().unwrap();
    // Echo mode should reflect the unicode content back
    assert!(content.contains("こんにちは"));
}

#[tokio::test]
async fn test_empty_content_string_accepted() {
    let server = FullTestServer::new().await;
    let client = server.client();

    // Empty string content should be accepted (not rejected)
    let response = client
        .post_json(
            "/v1/chat/completions",
            &serde_json::json!({
                "model": "test",
                "messages": [{"role": "user", "content": ""}],
                "stream": false
            }),
        )
        .await;

    // Should either succeed or give a clear error, but NOT a 500
    assert!(
        !response.status().is_server_error(),
        "Empty content should not cause server error, got {}",
        response.status()
    );
}

#[tokio::test]
async fn test_token_counts_are_plausible() {
    let server = FullTestServer::new().await;
    let client = server.client();

    let body = client
        .post_json_expect(
            "/v1/chat/completions",
            &serde_json::json!({
                "model": "test",
                "messages": [{"role": "user", "content": "Hello world"}],
                "stream": false
            }),
            200,
        )
        .await;

    let usage = &body["usage"];
    let prompt_tokens = usage["prompt_tokens"].as_u64().unwrap();
    let completion_tokens = usage["completion_tokens"].as_u64().unwrap();
    let total_tokens = usage["total_tokens"].as_u64().unwrap();

    // Tokens should be positive and plausible
    assert!(prompt_tokens > 0, "prompt_tokens should be > 0");
    assert!(completion_tokens > 0, "completion_tokens should be > 0");
    assert_eq!(
        total_tokens,
        prompt_tokens + completion_tokens,
        "total should equal prompt + completion"
    );
    // Should not be absurdly high for a simple message
    assert!(
        total_tokens < 10000,
        "token count seems unrealistically high"
    );
}

#[tokio::test]
async fn test_error_response_has_type_field() {
    let server = FullTestServer::new().await;
    let client = server.client();

    let response = client
        .post_json(
            "/v1/chat/completions",
            &serde_json::json!({
                "model": "test",
                "messages": [],
                "stream": false
            }),
        )
        .await;

    assert_eq!(response.status(), 400);
    let body: serde_json::Value = response.json().await.unwrap();

    // OpenAI error format must include these fields
    assert!(
        body["error"]["message"].is_string(),
        "error.message required"
    );
    assert!(body["error"]["type"].is_string(), "error.type required");
}

#[tokio::test]
async fn test_streaming_chunk_ids_are_consistent() {
    let server = FullTestServer::new().await;
    let client = server.client();

    let response = client
        .post_json(
            "/v1/chat/completions",
            &serde_json::json!({
                "model": "test",
                "messages": [{"role": "user", "content": "hello"}],
                "stream": true
            }),
        )
        .await;

    let body = response.text().await.unwrap();
    let mut chunk_ids: Vec<String> = Vec::new();

    for line in body.lines() {
        if line.starts_with("data: {") {
            let json_str = &line[6..];
            let chunk: serde_json::Value = serde_json::from_str(json_str).unwrap();
            chunk_ids.push(chunk["id"].as_str().unwrap().to_string());
        }
    }

    // All chunks in one stream should share the same completion ID
    assert!(!chunk_ids.is_empty(), "Should have at least one chunk");
    let first_id = &chunk_ids[0];
    for id in &chunk_ids {
        assert_eq!(
            id, first_id,
            "All streaming chunks should share the same ID"
        );
    }
}

#[tokio::test]
async fn test_models_endpoint_basic() {
    let server = FullTestServer::new().await;
    let client = server.client();

    let response = client.get("/v1/models").await;
    assert_eq!(response.status(), 200);
    assert_eq!(
        response.headers().get("content-type").unwrap(),
        "application/json"
    );

    let body: serde_json::Value = response.json().await.unwrap();

    // Verify OpenAI models list schema
    assert_eq!(body["object"], "list");
    assert!(body["data"].is_array());

    let models = body["data"].as_array().unwrap();
    assert!(!models.is_empty(), "Should return at least one model");

    // Verify each model follows OpenAI schema
    for model in models {
        assert!(model["id"].is_string(), "Model should have string id");
        assert_eq!(model["object"], "model");
        assert!(model["created"].is_number(), "Should have created timestamp");
        assert!(model["owned_by"].is_string(), "Should have owned_by");

        let model_id = model["id"].as_str().unwrap();
        let owned_by = model["owned_by"].as_str().unwrap();
        assert!(!model_id.is_empty(), "Model ID should not be empty");
        assert!(!owned_by.is_empty(), "owned_by should not be empty");
    }
}

#[tokio::test]
async fn test_models_endpoint_includes_default_model() {
    let server = FullTestServer::new().await;
    let client = server.client();

    let response = client.get("/v1/models").await;
    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    let models = body["data"].as_array().unwrap();

    // Should include the default model from config (claude-sonnet-4-20250514)
    let model_ids: Vec<&str> = models
        .iter()
        .map(|m| m["id"].as_str().unwrap())
        .collect();

    assert!(
        model_ids.iter().any(|id: &&str| id.contains("claude")),
        "Should include a Claude model (default model): {:?}",
        model_ids
    );
}

#[tokio::test]
async fn test_models_endpoint_includes_agent_models() {
    let server = FullTestServer::new().await;
    let client = server.client();

    // Register an agent with a specific model
    let register_response = client
        .post_json(
            "/api/agents",
            &serde_json::json!({
                "name": "test-agent",
                "kind": "api_client",
                "model": "custom-test-model"
            }),
        )
        .await;
    assert_eq!(register_response.status(), 201);

    // Get models list
    let response = client.get("/v1/models").await;
    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    let models = body["data"].as_array().unwrap();

    // Should include the agent's custom model
    let model_ids: Vec<&str> = models
        .iter()
        .map(|m| m["id"].as_str().unwrap())
        .collect();

    assert!(
        model_ids.contains(&"custom-test-model"),
        "Should include agent's model: {:?}",
        model_ids
    );
}

#[tokio::test]
async fn test_models_endpoint_deduplicates() {
    let server = FullTestServer::new().await;
    let client = server.client();

    // Register multiple agents with the same model
    for i in 1..=3 {
        let response = client
            .post_json(
                "/api/agents",
                &serde_json::json!({
                    "name": format!("agent-{}", i),
                    "kind": "api_client", 
                    "model": "duplicate-model"
                }),
            )
            .await;
        assert_eq!(response.status(), 201);
    }

    // Get models list  
    let response = client.get("/v1/models").await;
    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    let models = body["data"].as_array().unwrap();

    // Count occurrences of duplicate-model
    let duplicate_count = models
        .iter()
        .filter(|m| m["id"].as_str().unwrap() == "duplicate-model")
        .count();

    assert_eq!(
        duplicate_count, 1,
        "Should only include duplicate-model once, found {} times",
        duplicate_count
    );
}

#[tokio::test]
async fn test_models_endpoint_model_ownership_inference() {
    let server = FullTestServer::new().await;
    let client = server.client();

    // Register agents with different model types
    let test_models = vec![
        ("claude-agent", "claude-3-opus"),
        ("gpt-agent", "gpt-4"),
        ("llama-agent", "llama-2-7b"),
        ("custom-agent", "acme/custom-model"),
    ];

    for (name, model) in &test_models {
        let response = client
            .post_json(
                "/api/agents",
                &serde_json::json!({
                    "name": name,
                    "kind": "api_client",
                    "model": model
                }),
            )
            .await;
        assert_eq!(response.status(), 201);
    }

    // Get models and verify ownership inference
    let response = client.get("/v1/models").await;
    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    let models = body["data"].as_array().unwrap();

    for model in models {
        let id = model["id"].as_str().unwrap();
        let owned_by = model["owned_by"].as_str().unwrap();

        match id {
            id if id.contains("claude") => {
                assert_eq!(owned_by, "anthropic", "Claude models should be owned by anthropic");
            }
            id if id.contains("gpt") => {
                assert_eq!(owned_by, "openai", "GPT models should be owned by openai");
            }
            id if id.contains("llama") => {
                assert_eq!(owned_by, "meta", "Llama models should be owned by meta");
            }
            id if id.starts_with("acme/") => {
                assert_eq!(owned_by, "acme", "Custom models should infer owner from prefix");
            }
            _ => {
                // Other models (like default) can have various owners
                assert!(!owned_by.is_empty(), "All models should have an owner");
            }
        }
    }
}
