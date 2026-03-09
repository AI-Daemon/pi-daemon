/// HTTP client helper for testing API endpoints.
///
/// Provides convenient methods for making HTTP requests during integration
/// tests with built-in error handling suitable for test environments.
pub struct TestClient {
    pub base_url: String,
    pub client: reqwest::Client,
}

impl TestClient {
    /// Create a new test client for the given base URL.
    ///
    /// The client is configured with reasonable timeouts for testing.
    pub fn new(base_url: &str) -> Self {
        Self {
            base_url: base_url.to_string(),
            client: reqwest::Client::new(),
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
}
