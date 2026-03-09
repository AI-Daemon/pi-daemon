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
