/// Assert a JSON response matches expected status and contains a key.
///
/// This macro checks that the response is successful, parses the JSON,
/// and verifies that the specified key exists in the response.
/// Note: This consumes the response.
#[macro_export]
macro_rules! assert_json_ok {
    ($resp:expr, $key:expr) => {{
        assert!(
            $resp.status().is_success(),
            "Expected success, got {}",
            $resp.status()
        );
        let json: serde_json::Value = $resp.json().await.expect("Failed to parse JSON response");
        assert!(
            json.get($key).is_some(),
            "Expected key '{}' in response: {:?}",
            $key,
            json
        );
        json
    }};
}

/// Assert response status code matches expected value.
///
/// Only reads the response body on failure for the error message.
/// Note: This consumes the response.
#[macro_export]
macro_rules! assert_status {
    ($resp:expr, $status:expr) => {
        let status = $resp.status().as_u16();
        if status != $status {
            let body = $resp.text().await.unwrap_or_default();
            panic!(
                "Expected status {}, got {}. Response body: {:?}",
                $status, status, body
            );
        }
    };
}

/// Assert response has a specific header with the expected value.
///
/// # Example
/// ```ignore
/// let resp = client.get("/").await;
/// assert_header!(resp, "content-type", "text/html; charset=utf-8");
/// ```
#[macro_export]
macro_rules! assert_header {
    ($resp:expr, $header:expr, $expected:expr) => {{
        let header_val = $resp
            .headers()
            .get($header)
            .unwrap_or_else(|| panic!("Response missing header '{}'", $header))
            .to_str()
            .expect("Header value is not valid UTF-8");
        assert!(
            header_val.contains($expected),
            "Header '{}' expected to contain '{}', got '{}'",
            $header,
            $expected,
            header_val
        );
    }};
}

/// Assert a JSON response contains a subset of expected key-value pairs.
///
/// Takes a response and an expected `serde_json::Value` object. Verifies
/// that every key in `expected` exists in the response with the same value.
///
/// # Example
/// ```ignore
/// let resp = client.get("/api/status").await;
/// assert_json_contains!(resp, serde_json::json!({"status": "ok"}));
/// ```
#[macro_export]
macro_rules! assert_json_contains {
    ($resp:expr, $expected:expr) => {{
        let status = $resp.status();
        let json: serde_json::Value = $resp
            .json()
            .await
            .unwrap_or_else(|e| panic!("Failed to parse JSON (status {}): {}", status, e));
        let expected = $expected;
        if let serde_json::Value::Object(expected_map) = &expected {
            for (key, expected_val) in expected_map {
                let actual_val = json.get(key).unwrap_or_else(|| {
                    panic!(
                        "Expected key '{}' in response, got: {}",
                        key,
                        serde_json::to_string_pretty(&json).unwrap()
                    )
                });
                assert_eq!(
                    actual_val, expected_val,
                    "Key '{}': expected {}, got {}",
                    key, expected_val, actual_val
                );
            }
        } else {
            panic!("assert_json_contains! expects a JSON object as the expected value");
        }
        json
    }};
}

/// Assert an OpenAI-compatible chat completion response has the correct schema.
///
/// Validates id prefix, object type, created timestamp, choices array, and usage.
/// Returns the parsed JSON for further assertions.
///
/// # Example
/// ```ignore
/// let body: serde_json::Value = resp.json().await.unwrap();
/// let json = assert_openai_completion!(body);
/// assert_eq!(json["model"], "test-model");
/// ```
#[macro_export]
macro_rules! assert_openai_completion {
    ($json:expr) => {{
        let json = &$json;
        assert!(
            json["id"]
                .as_str()
                .expect("Missing 'id' field")
                .starts_with("chatcmpl-"),
            "id should start with 'chatcmpl-', got: {}",
            json["id"]
        );
        assert_eq!(
            json["object"], "chat.completion",
            "object should be 'chat.completion'"
        );
        assert!(json["created"].is_number(), "created should be a number");

        let choices = json["choices"]
            .as_array()
            .expect("choices should be an array");
        assert!(!choices.is_empty(), "choices should not be empty");

        let choice = &choices[0];
        assert_eq!(choice["message"]["role"], "assistant");
        assert!(
            choice["message"]["content"].is_string(),
            "message.content should be a string"
        );
        assert_eq!(choice["finish_reason"], "stop");

        let usage = &json["usage"];
        assert!(
            usage["prompt_tokens"].is_number(),
            "usage.prompt_tokens should be a number"
        );
        assert!(
            usage["completion_tokens"].is_number(),
            "usage.completion_tokens should be a number"
        );
        assert!(
            usage["total_tokens"].as_u64().expect("total_tokens") > 0,
            "total_tokens should be > 0"
        );
        json
    }};
}

/// Assert an event list contains expected event types in order.
///
/// Each event is expected to have a `payload.type` field. This macro
/// verifies the event types appear in the specified order (not necessarily
/// contiguous — other events may appear between them).
///
/// # Example
/// ```ignore
/// let events: Vec<serde_json::Value> = resp.json().await.unwrap();
/// assert_events_contain!(events, "System", "AgentRegistered", "AgentDisconnected");
/// ```
#[macro_export]
macro_rules! assert_events_contain {
    ($events:expr, $( $event_type:expr ),+ ) => {{
        let expected_types: Vec<&str> = vec![$( $event_type ),+];
        let mut expected_idx = 0;

        for event in &$events {
            let event_type = event["payload"]["type"]
                .as_str()
                .or_else(|| event["type"].as_str())
                .unwrap_or("");

            if expected_idx < expected_types.len() && event_type == expected_types[expected_idx] {
                expected_idx += 1;
            }
        }

        assert_eq!(
            expected_idx,
            expected_types.len(),
            "Expected event types {:?} in order, but only found {} of {} in events: {:?}",
            expected_types,
            expected_idx,
            expected_types.len(),
            $events.iter()
                .map(|e| e["payload"]["type"].as_str()
                    .or_else(|| e["type"].as_str())
                    .unwrap_or("unknown"))
                .collect::<Vec<_>>()
        );
    }};
}
