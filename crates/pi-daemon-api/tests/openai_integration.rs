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

    // Verify content is present (mock provider echoes the user message)
    let content = body["choices"][0]["message"]["content"].as_str().unwrap();
    assert!(!content.is_empty(), "Response content should not be empty");
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
        assert!(
            model["created"].is_number(),
            "Should have created timestamp"
        );
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
    let model_ids: Vec<&str> = models.iter().map(|m| m["id"].as_str().unwrap()).collect();

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
    let model_ids: Vec<&str> = models.iter().map(|m| m["id"].as_str().unwrap()).collect();

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
                assert_eq!(
                    owned_by, "anthropic",
                    "Claude models should be owned by anthropic"
                );
            }
            id if id.contains("gpt") => {
                assert_eq!(owned_by, "openai", "GPT models should be owned by openai");
            }
            id if id.contains("llama") => {
                assert_eq!(owned_by, "meta", "Llama models should be owned by meta");
            }
            id if id.starts_with("acme/") => {
                assert_eq!(
                    owned_by, "acme",
                    "Custom models should infer owner from prefix"
                );
            }
            _ => {
                // Other models (like default) can have various owners
                assert!(!owned_by.is_empty(), "All models should have an owner");
            }
        }
    }
}

#[tokio::test]
async fn test_models_endpoint_filters_invalid_model_names() {
    let server = FullTestServer::new().await;
    let client = server.client();

    // Register agents with various invalid model names
    let invalid_models = vec![
        ("empty-agent", ""),
        ("whitespace-agent", "   "),
        ("tab-agent", "\t\t"),
        ("newline-agent", "\n"),
        ("mixed-whitespace-agent", " \t\n "),
    ];

    for (name, model) in &invalid_models {
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
        assert_eq!(response.status(), 201); // Agent should register successfully
    }

    // Register an agent with a valid model for comparison
    let response = client
        .post_json(
            "/api/agents",
            &serde_json::json!({
                "name": "valid-agent",
                "kind": "api_client",
                "model": "valid-model-name"
            }),
        )
        .await;
    assert_eq!(response.status(), 201);

    // Get models list
    let response = client.get("/v1/models").await;
    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    let models = body["data"].as_array().unwrap();

    // Should not include any invalid model names, but should include valid ones
    let model_ids: Vec<&str> = models.iter().map(|m| m["id"].as_str().unwrap()).collect();

    // Should include valid model and default model
    assert!(model_ids.contains(&"valid-model-name"));
    assert!(model_ids.iter().any(|id| id.contains("claude"))); // Default model

    // Should not include any invalid models
    for id in &model_ids {
        assert!(!id.is_empty(), "No empty model IDs: {:?}", model_ids);
        assert!(
            !id.trim().is_empty(),
            "No whitespace-only model IDs: {:?}",
            model_ids
        );
        assert!(id.len() <= 256, "Model ID too long: {}", id);
    }
}

#[tokio::test]
async fn test_models_endpoint_handles_very_long_model_names() {
    let server = FullTestServer::new().await;
    let client = server.client();

    // Test with a very long model name (over 256 chars)
    let long_model = "x".repeat(300);
    let response = client
        .post_json(
            "/api/agents",
            &serde_json::json!({
                "name": "long-model-agent",
                "kind": "api_client",
                "model": long_model
            }),
        )
        .await;
    assert_eq!(response.status(), 201);

    // Test with exactly 256 chars (should be valid)
    let max_model = "y".repeat(256);
    let response = client
        .post_json(
            "/api/agents",
            &serde_json::json!({
                "name": "max-model-agent",
                "kind": "api_client",
                "model": max_model
            }),
        )
        .await;
    assert_eq!(response.status(), 201);

    // Get models list
    let response = client.get("/v1/models").await;
    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    let models = body["data"].as_array().unwrap();

    let model_ids: Vec<&str> = models.iter().map(|m| m["id"].as_str().unwrap()).collect();

    // Should include the 256-char model but not the 300-char model
    assert!(model_ids.iter().any(|id| id.len() == 256));
    assert!(!model_ids.iter().any(|id| id.len() == 300));
}

#[tokio::test]
async fn test_models_endpoint_concurrent_access() {
    let server = FullTestServer::new().await;
    let client = server.client();

    // Test concurrent access to the models endpoint
    let mut handles = Vec::new();

    for i in 0..10 {
        let client = client.clone();
        handles.push(tokio::spawn(async move {
            let response = client.get("/v1/models").await;
            (
                i,
                response.status(),
                response.json::<serde_json::Value>().await,
            )
        }));
    }

    // All requests should succeed with consistent results
    let mut all_models = Vec::new();
    for handle in handles {
        let (i, status, body_result) = handle.await.unwrap();
        assert_eq!(status, 200, "Request {} should succeed", i);

        let body = body_result.unwrap();
        assert_eq!(body["object"], "list");
        assert!(body["data"].is_array());

        all_models.push(body["data"].as_array().unwrap().len());
    }

    // All concurrent requests should return the same number of models
    // (assuming no agents are being registered/unregistered during test)
    let first_count = all_models[0];
    for (i, count) in all_models.iter().enumerate() {
        assert_eq!(
            *count, first_count,
            "Request {} returned different model count",
            i
        );
    }
}

#[tokio::test]
async fn test_models_endpoint_with_special_characters() {
    let server = FullTestServer::new().await;
    let client = server.client();

    // Test with various special characters that should be handled gracefully
    let special_models = vec![
        ("dots-agent", "model.with.dots"),
        ("dashes-agent", "model-with-dashes"),
        ("underscores-agent", "model_with_underscores"),
        ("numbers-agent", "model123with456numbers"),
        ("slashes-agent", "company/model-name"),
    ];

    for (name, model) in &special_models {
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

    // Get models and verify all special character models are included
    let response = client.get("/v1/models").await;
    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    let models = body["data"].as_array().unwrap();

    let model_ids: Vec<&str> = models.iter().map(|m| m["id"].as_str().unwrap()).collect();

    // All special character models should be included
    for (_, expected_model) in &special_models {
        assert!(
            model_ids.contains(expected_model),
            "Should include model '{}' but found: {:?}",
            expected_model,
            model_ids
        );
    }
}

#[tokio::test]
async fn test_models_endpoint_error_resilience() {
    let server = FullTestServer::new().await;
    let client = server.client();

    // The models endpoint should never fail, even with edge case data
    let response = client.get("/v1/models").await;
    assert_eq!(response.status(), 200);

    // Should always return valid JSON structure
    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["object"], "list");
    assert!(body["data"].is_array());

    // Each model should have required fields
    let models = body["data"].as_array().unwrap();
    for model in models {
        assert!(model["id"].is_string(), "Model should have string id");
        assert_eq!(model["object"], "model");
        assert!(
            model["created"].is_number(),
            "Model should have created timestamp"
        );
        assert!(model["owned_by"].is_string(), "Model should have owned_by");
    }
}

#[tokio::test]
async fn test_models_endpoint_http_methods() {
    let server = FullTestServer::new().await;
    let client = server.client();

    // GET should work
    let response = client.get("/v1/models").await;
    assert_eq!(response.status(), 200);

    // POST should not be allowed (405 Method Not Allowed)
    let response = client.post_json("/v1/models", &serde_json::json!({})).await;
    assert_eq!(response.status(), 405);

    // PUT should not be allowed
    let response = client.put_json("/v1/models", &serde_json::json!({})).await;
    assert_eq!(response.status(), 405);

    // DELETE should not be allowed
    let response = client.delete("/v1/models").await;
    assert_eq!(response.status(), 405);
}

#[tokio::test]
async fn test_models_endpoint_with_no_models() {
    // Test with empty config (no default model, no API keys)
    let config = pi_daemon_types::config::DaemonConfig {
        default_model: "".to_string(), // No default model
        providers: pi_daemon_types::config::ProvidersConfig::default(), // No API keys
        ..Default::default()
    };

    let server = FullTestServer::with_config(config).await;
    let client = server.client();

    // Even with no models, endpoint should still work and return empty list
    let response = client.get("/v1/models").await;
    assert_eq!(response.status(), 200);

    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["object"], "list");

    let _models = body["data"].as_array().unwrap();
    // Should return a valid array, even if empty when no models are available
    // If we got here without panicking, the test passes - endpoint handled empty config gracefully
}

#[tokio::test]
async fn test_models_endpoint_load_performance() {
    let server = FullTestServer::new().await;
    let client = server.client();

    // Register several agents to make the response more realistic
    for i in 0..5 {
        let response = client
            .post_json(
                "/api/agents",
                &serde_json::json!({
                    "name": format!("load-test-agent-{}", i),
                    "kind": "api_client",
                    "model": format!("test-model-{}", i)
                }),
            )
            .await;
        assert_eq!(response.status(), 201);
    }

    // Test with 50 concurrent requests to simulate realistic load
    let mut handles = Vec::new();
    let start = std::time::Instant::now();

    for i in 0..50 {
        let client = client.clone();
        handles.push(tokio::spawn(async move {
            let response = client.get("/v1/models").await;
            (
                i,
                response.status(),
                response.text().await.unwrap_or_default().len(),
            )
        }));
    }

    let mut results = Vec::new();
    for handle in handles {
        results.push(handle.await.unwrap());
    }

    let elapsed = start.elapsed();

    // All requests should succeed
    for (i, status, body_len) in results {
        assert_eq!(status, 200, "Request {} should succeed", i);
        assert!(
            body_len > 50,
            "Request {} should return substantial JSON",
            i
        );
    }

    // Should complete 50 requests in reasonable time (under 1 second for local test)
    assert!(
        elapsed.as_millis() < 1000,
        "50 concurrent requests took too long: {:?}",
        elapsed
    );
}

#[tokio::test]
async fn test_models_endpoint_openai_spec_compliance() {
    let server = FullTestServer::new().await;
    let client = server.client();

    let response = client.get("/v1/models").await;
    assert_eq!(response.status(), 200);

    // Verify content-type header
    assert_eq!(
        response.headers().get("content-type").unwrap(),
        "application/json"
    );

    let body: serde_json::Value = response.json().await.unwrap();

    // Top-level object validation per OpenAI spec
    assert_eq!(
        body["object"].as_str().unwrap(),
        "list",
        "object field must be 'list'"
    );
    assert!(body["data"].is_array(), "data field must be array");

    // Each model must conform to OpenAI model object spec
    let models = body["data"].as_array().unwrap();
    for (i, model) in models.iter().enumerate() {
        // Required fields per OpenAI API spec
        assert!(
            model["id"].is_string(),
            "Model {} missing required 'id' field",
            i
        );
        assert_eq!(
            model["object"].as_str().unwrap(),
            "model",
            "Model {} 'object' field must be 'model'",
            i
        );
        assert!(
            model["created"].is_number(),
            "Model {} missing required 'created' field",
            i
        );
        assert!(
            model["owned_by"].is_string(),
            "Model {} missing required 'owned_by' field",
            i
        );

        // Field format validation
        let id = model["id"].as_str().unwrap();
        let owned_by = model["owned_by"].as_str().unwrap();
        let created = model["created"].as_i64().unwrap();

        assert!(!id.is_empty(), "Model {} id cannot be empty", i);
        assert!(!owned_by.is_empty(), "Model {} owned_by cannot be empty", i);
        assert!(
            created > 0,
            "Model {} created timestamp must be positive",
            i
        );
        assert!(
            created < i64::MAX,
            "Model {} created timestamp must be valid",
            i
        );
    }
}

#[tokio::test]
async fn test_models_endpoint_with_configured_providers() {
    // Create a config with some provider API keys configured
    let config = pi_daemon_types::config::DaemonConfig {
        providers: pi_daemon_types::config::ProvidersConfig {
            anthropic_api_key: "test-anthropic-key".to_string(),
            openai_api_key: "test-openai-key".to_string(),
            ..Default::default()
        },
        ..Default::default()
    };

    let server = FullTestServer::with_config(config).await;
    let client = server.client();

    let response = client.get("/v1/models").await;
    assert_eq!(response.status(), 200);
    let body: serde_json::Value = response.json().await.unwrap();
    let models = body["data"].as_array().unwrap();

    let model_ids: Vec<&str> = models.iter().map(|m| m["id"].as_str().unwrap()).collect();

    // Should include well-known models from configured providers
    let has_anthropic = model_ids.iter().any(|id| id.contains("claude"));
    let has_openai = model_ids.iter().any(|id| id.contains("gpt"));

    assert!(
        has_anthropic,
        "Should include Anthropic models when API key configured: {:?}",
        model_ids
    );
    assert!(
        has_openai,
        "Should include OpenAI models when API key configured: {:?}",
        model_ids
    );

    // Verify model ownership is correctly inferred
    for model in models {
        let id = model["id"].as_str().unwrap();
        let owned_by = model["owned_by"].as_str().unwrap();

        if id.contains("claude") {
            assert_eq!(
                owned_by, "anthropic",
                "Claude models should be owned by anthropic"
            );
        } else if id.contains("gpt") {
            assert_eq!(owned_by, "openai", "GPT models should be owned by openai");
        }
    }
}

// ─── Issue #236: LLM Provider Wiring Tests ─────────────────────────────────

#[tokio::test]
async fn test_no_provider_returns_model_not_available() {
    let server = FullTestServer::without_provider().await;
    let client = server.client();

    let response = client
        .post_json(
            "/v1/chat/completions",
            &serde_json::json!({
                "model": "claude-3-5-sonnet-20241022",
                "messages": [{"role": "user", "content": "Hello"}],
                "stream": false
            }),
        )
        .await;

    assert_eq!(response.status(), 400);
    let body: serde_json::Value = response
        .json()
        .await
        .expect("error response should be valid JSON");
    assert_eq!(body["error"]["type"], "invalid_request_error");
    assert!(
        body["error"]["message"]
            .as_str()
            .expect("error message should be a string")
            .contains("not available"),
        "Error message should mention 'not available': {}",
        body["error"]["message"]
    );
}

#[tokio::test]
async fn test_temperature_validation_out_of_range() {
    let server = FullTestServer::new().await;
    let client = server.client();

    // temperature > 2 should fail
    let response = client
        .post_json(
            "/v1/chat/completions",
            &serde_json::json!({
                "model": "test",
                "messages": [{"role": "user", "content": "Hello"}],
                "temperature": 2.5,
                "stream": false
            }),
        )
        .await;
    assert_eq!(response.status(), 400);
    let body: serde_json::Value = response
        .json()
        .await
        .expect("validation error should be valid JSON");
    assert!(
        body["error"]["message"]
            .as_str()
            .expect("error message should be a string")
            .contains("temperature"),
        "Error should mention temperature: {}",
        body["error"]["message"]
    );

    // temperature < 0 should fail
    let response = client
        .post_json(
            "/v1/chat/completions",
            &serde_json::json!({
                "model": "test",
                "messages": [{"role": "user", "content": "Hello"}],
                "temperature": -0.1,
                "stream": false
            }),
        )
        .await;
    assert_eq!(response.status(), 400);
}

#[tokio::test]
async fn test_top_p_validation_out_of_range() {
    let server = FullTestServer::new().await;
    let client = server.client();

    let response = client
        .post_json(
            "/v1/chat/completions",
            &serde_json::json!({
                "model": "test",
                "messages": [{"role": "user", "content": "Hello"}],
                "top_p": 1.5,
                "stream": false
            }),
        )
        .await;
    assert_eq!(response.status(), 400);
    let body: serde_json::Value = response
        .json()
        .await
        .expect("validation error should be valid JSON");
    assert!(
        body["error"]["message"]
            .as_str()
            .expect("error message should be a string")
            .contains("top_p"),
        "Error should mention top_p: {}",
        body["error"]["message"]
    );
}

#[tokio::test]
async fn test_max_tokens_validation_zero() {
    let server = FullTestServer::new().await;
    let client = server.client();

    let response = client
        .post_json(
            "/v1/chat/completions",
            &serde_json::json!({
                "model": "test",
                "messages": [{"role": "user", "content": "Hello"}],
                "max_tokens": 0,
                "stream": false
            }),
        )
        .await;
    assert_eq!(response.status(), 400);
    let body: serde_json::Value = response
        .json()
        .await
        .expect("validation error should be valid JSON");
    assert!(
        body["error"]["message"]
            .as_str()
            .expect("error message should be a string")
            .contains("max_tokens"),
        "Error should mention max_tokens: {}",
        body["error"]["message"]
    );
}

#[tokio::test]
async fn test_valid_parameter_boundaries_accepted() {
    let server = FullTestServer::new().await;
    let client = server.client();

    // Boundary values should be accepted
    let body = client
        .post_json_expect(
            "/v1/chat/completions",
            &serde_json::json!({
                "model": "test",
                "messages": [{"role": "user", "content": "Hello"}],
                "temperature": 0.0,
                "top_p": 0.0,
                "max_tokens": 1,
                "stream": false
            }),
            200,
        )
        .await;
    assert_eq!(body["object"], "chat.completion");

    let body = client
        .post_json_expect(
            "/v1/chat/completions",
            &serde_json::json!({
                "model": "test",
                "messages": [{"role": "user", "content": "Hello"}],
                "temperature": 2.0,
                "top_p": 1.0,
                "stream": false
            }),
            200,
        )
        .await;
    assert_eq!(body["object"], "chat.completion");
}

#[tokio::test]
async fn test_full_conversation_context_sent() {
    let server = FullTestServer::new().await;
    let client = server.client();

    // The mock provider echoes the last user message.
    // With full context, "What is 3+3?" should be in the response.
    let body = client
        .post_json_expect(
            "/v1/chat/completions",
            &serde_json::json!({
                "model": "test",
                "messages": [
                    {"role": "system", "content": "You are a math tutor"},
                    {"role": "user", "content": "What is 2+2?"},
                    {"role": "assistant", "content": "4"},
                    {"role": "user", "content": "What is 3+3?"}
                ],
                "stream": false
            }),
            200,
        )
        .await;

    let content = body["choices"][0]["message"]["content"]
        .as_str()
        .expect("response content should be a string");
    // Mock echoes the last user message
    assert!(
        content.contains("3+3"),
        "Response should contain the last user message: {}",
        content
    );
}

#[tokio::test]
async fn test_streaming_has_sse_headers() {
    let server = FullTestServer::new().await;
    let client = server.client();

    let response = client
        .post_json(
            "/v1/chat/completions",
            &serde_json::json!({
                "model": "test",
                "messages": [{"role": "user", "content": "Hello"}],
                "stream": true
            }),
        )
        .await;

    assert_eq!(response.status(), 200);

    // Verify SSE-related headers (fixes #211)
    let cache_control = response
        .headers()
        .get("cache-control")
        .expect("Should have Cache-Control header")
        .to_str()
        .expect("Cache-Control should be valid UTF-8");
    assert!(
        cache_control.contains("no-cache"),
        "Cache-Control should contain no-cache, got: {}",
        cache_control
    );

    let x_accel = response
        .headers()
        .get("x-accel-buffering")
        .expect("Should have X-Accel-Buffering header")
        .to_str()
        .expect("X-Accel-Buffering should be valid UTF-8");
    assert_eq!(x_accel, "no", "X-Accel-Buffering should be 'no'");
}

#[tokio::test]
async fn test_real_token_usage_in_response() {
    let server = FullTestServer::new().await;
    let client = server.client();

    let body = client
        .post_json_expect(
            "/v1/chat/completions",
            &serde_json::json!({
                "model": "test",
                "messages": [{"role": "user", "content": "Hello world, this is a test message"}],
                "stream": false
            }),
            200,
        )
        .await;

    let usage = &body["usage"];
    let prompt_tokens = usage["prompt_tokens"]
        .as_u64()
        .expect("prompt_tokens should be a number");
    let completion_tokens = usage["completion_tokens"]
        .as_u64()
        .expect("completion_tokens should be a number");
    let total_tokens = usage["total_tokens"]
        .as_u64()
        .expect("total_tokens should be a number");

    // Token counts come from the provider's Done event, not estimate_tokens()
    assert!(prompt_tokens > 0, "prompt_tokens should be > 0");
    assert!(completion_tokens > 0, "completion_tokens should be > 0");
    assert_eq!(
        total_tokens,
        prompt_tokens + completion_tokens,
        "total should equal prompt + completion"
    );
}

#[tokio::test]
async fn test_stop_sequences_forwarded() {
    let server = FullTestServer::new().await;
    let client = server.client();

    // String stop sequence
    let body = client
        .post_json_expect(
            "/v1/chat/completions",
            &serde_json::json!({
                "model": "test",
                "messages": [{"role": "user", "content": "Hello"}],
                "stop": "END",
                "stream": false
            }),
            200,
        )
        .await;
    assert_eq!(body["object"], "chat.completion");

    // Array stop sequences
    let body = client
        .post_json_expect(
            "/v1/chat/completions",
            &serde_json::json!({
                "model": "test",
                "messages": [{"role": "user", "content": "Hello"}],
                "stop": ["END", "STOP"],
                "stream": false
            }),
            200,
        )
        .await;
    assert_eq!(body["object"], "chat.completion");
}

#[tokio::test]
async fn test_streaming_mock_provider_text_content() {
    let server = FullTestServer::new().await;
    let client = server.client();

    let response = client
        .post_json(
            "/v1/chat/completions",
            &serde_json::json!({
                "model": "test",
                "messages": [{"role": "user", "content": "Tell me something"}],
                "stream": true
            }),
        )
        .await;

    assert_eq!(response.status(), 200);
    let body = response
        .text()
        .await
        .expect("streaming response should have a text body");

    // Collect all text deltas from chunks
    let mut full_text = String::new();
    for line in body.lines() {
        if line.starts_with("data: {") {
            let json_str = &line[6..];
            let chunk: serde_json::Value =
                serde_json::from_str(json_str).expect("SSE chunk should be valid JSON");
            if let Some(content) = chunk["choices"][0]["delta"]["content"].as_str() {
                full_text.push_str(content);
            }
        }
    }

    // Reassembled text should contain the user message echoed back
    assert!(
        full_text.contains("Tell me something"),
        "Streamed text should contain user message: {}",
        full_text
    );
}
