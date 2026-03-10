/// HTTP client helper for testing API endpoints.
///
/// Provides convenient methods for making HTTP requests during integration
/// tests with built-in error handling suitable for test environments.
///
/// # Example
/// ```ignore
/// let client = TestClient::new("http://127.0.0.1:4200");
/// let resp = client.get("/api/health").await;
/// assert_eq!(resp.status().as_u16(), 200);
/// ```
#[derive(Clone)]
pub struct TestClient {
    /// The base URL this client targets.
    pub base_url: String,
    /// The inner reqwest client.
    pub client: reqwest::Client,
}

impl TestClient {
    /// Create a new test client for the given base URL.
    ///
    /// The client is configured with reasonable timeouts for testing.
    pub fn new(base_url: &str) -> Self {
        Self {
            base_url: base_url.to_string(),
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .expect("Failed to build reqwest client"),
        }
    }

    /// Send a GET request to the specified path.
    ///
    /// Path is relative to the base URL. Panics on failure for test convenience.
    pub async fn get(&self, path: &str) -> reqwest::Response {
        self.client
            .get(format!("{}{}", self.base_url, path))
            .send()
            .await
            .expect("Failed to send GET request")
    }

    /// Send a POST request with JSON body.
    ///
    /// Panics on failure for test convenience.
    pub async fn post_json(&self, path: &str, body: &serde_json::Value) -> reqwest::Response {
        self.client
            .post(format!("{}{}", self.base_url, path))
            .json(body)
            .send()
            .await
            .expect("Failed to send POST request")
    }

    /// Send a PUT request with JSON body.
    ///
    /// Panics on failure for test convenience.
    pub async fn put_json(&self, path: &str, body: &serde_json::Value) -> reqwest::Response {
        self.client
            .put(format!("{}{}", self.base_url, path))
            .json(body)
            .send()
            .await
            .expect("Failed to send PUT request")
    }

    /// Send a PATCH request with JSON body.
    ///
    /// Panics on failure for test convenience.
    pub async fn patch_json(&self, path: &str, body: &serde_json::Value) -> reqwest::Response {
        self.client
            .patch(format!("{}{}", self.base_url, path))
            .json(body)
            .send()
            .await
            .expect("Failed to send PATCH request")
    }

    /// Send a POST request with raw string body and custom content-type.
    ///
    /// Useful for testing malformed request handling.
    pub async fn post_raw(&self, path: &str, body: &str, content_type: &str) -> reqwest::Response {
        self.client
            .post(format!("{}{}", self.base_url, path))
            .header("content-type", content_type)
            .body(body.to_string())
            .send()
            .await
            .expect("Failed to send raw POST request")
    }

    /// Send a DELETE request to the specified path.
    ///
    /// Panics on failure for test convenience.
    pub async fn delete(&self, path: &str) -> reqwest::Response {
        self.client
            .delete(format!("{}{}", self.base_url, path))
            .send()
            .await
            .expect("Failed to send DELETE request")
    }

    /// Send N concurrent GET requests and return all responses.
    ///
    /// Useful for load testing and concurrency validation.
    pub async fn get_concurrent(&self, path: &str, count: usize) -> Vec<reqwest::Response> {
        let mut handles = Vec::with_capacity(count);
        for _ in 0..count {
            let client = self.clone();
            let path = path.to_string();
            handles.push(tokio::spawn(async move { client.get(&path).await }));
        }

        let mut responses = Vec::with_capacity(count);
        for handle in handles {
            responses.push(handle.await.expect("Concurrent GET task panicked"));
        }
        responses
    }

    /// POST JSON and assert the expected status, returning parsed JSON body.
    ///
    /// Convenience method that combines request + status assertion + JSON parse.
    pub async fn post_json_expect(
        &self,
        path: &str,
        body: &serde_json::Value,
        expected_status: u16,
    ) -> serde_json::Value {
        let resp = self.post_json(path, body).await;
        let status = resp.status().as_u16();
        assert_eq!(
            status, expected_status,
            "Expected status {expected_status}, got {status}"
        );
        resp.json().await.expect("Failed to parse JSON response")
    }
}
