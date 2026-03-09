use axum::{routing::get, Router};
use tokio::net::TcpListener;

/// Test server configuration for integration testing.
///
/// Provides a lightweight HTTP server with basic endpoints for testing
/// API interactions and WebSocket functionality.
pub struct TestServer {
    pub base_url: String,
    pub port: u16,
}

impl TestServer {
    /// Start a test HTTP server on a random port with basic endpoints.
    ///
    /// The server includes /api/health and /api/status endpoints for testing.
    /// Returns immediately after starting the server in the background.
    pub async fn new() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("Failed to bind to random port");

        let addr = listener.local_addr().expect("Failed to get local address");
        let port = addr.port();
        let base_url = format!("http://127.0.0.1:{}", port);

        // Create a simple router for testing
        let router = Router::new()
            .route("/api/health", get(health_check))
            .route("/api/status", get(status_check));

        // Spawn the server
        tokio::spawn(async move {
            axum::serve(listener, router).await.expect("Server failed");
        });

        // Give the server a moment to start
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        Self { base_url, port }
    }
}

/// Health check endpoint that returns OK status.
async fn health_check() -> &'static str {
    r#"{"status":"ok"}"#
}

/// Status endpoint with basic daemon information.
async fn status_check() -> &'static str {
    r#"{"status":"ok","version":"test","uptime_secs":0,"agent_count":0}"#
}
