//! Shared test utilities for pi-daemon.
//!
//! This crate provides common testing infrastructure including:
//! - `FullTestServer` for API integration testing with a real kernel
//! - `TestServer` for lightweight endpoint testing
//! - `TestKernel` for isolated test environments
//! - `TestClient` for making test requests with assertion helpers
//! - Assertion macros for JSON, HTTP, OpenAI, and event responses

pub mod client;
pub mod kernel;
pub mod macros;
pub mod server;

// Re-export main types for convenience
pub use client::TestClient;
pub use kernel::TestKernel;
pub use server::{FullTestServer, TestServer};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kernel_creates_isolated_environment() {
        let kernel = TestKernel::new();
        assert!(kernel.data_dir.exists());
        assert!(kernel.data_dir.is_dir());

        // Verify it's a temporary directory (should be under /tmp or similar)
        let temp_root = std::env::temp_dir();
        assert!(kernel.data_dir.starts_with(temp_root));
    }

    #[tokio::test]
    async fn test_server_binds_and_responds() {
        let server = TestServer::new().await;

        // Verify server properties
        assert!(server.base_url.starts_with("http://127.0.0.1:"));
        assert!(server.port > 0);

        // Verify we can make a request
        let client = TestClient::new(&server.base_url);
        let resp = client.get("/api/health").await;
        assert_eq!(resp.status().as_u16(), 200);
    }

    #[tokio::test]
    async fn test_full_server_boots_real_kernel() {
        let server = FullTestServer::new().await;

        assert!(server.base_url.starts_with("http://127.0.0.1:"));
        assert!(server.port > 0);

        // Should respond to real API endpoints
        let client = server.client();
        let resp = client.get("/api/health").await;
        assert_eq!(resp.status().as_u16(), 200);

        let json: serde_json::Value = resp.json().await.expect("health should be JSON");
        assert_eq!(json["status"], "ok");
    }

    #[tokio::test]
    async fn test_full_server_with_config() {
        let config = pi_daemon_types::config::DaemonConfig {
            api_key: "test-key-123".to_string(),
            ..Default::default()
        };

        let server = FullTestServer::with_config(config).await;
        let client = server.client();

        // Health endpoint should still work (no auth on health)
        let resp = client.get("/api/health").await;
        assert_eq!(resp.status().as_u16(), 200);
    }

    #[tokio::test]
    async fn test_full_server_ws_url() {
        let server = FullTestServer::new().await;
        let url = server.ws_url("test-agent");
        assert!(url.starts_with("ws://127.0.0.1:"));
        assert!(url.ends_with("/ws/test-agent"));
    }

    #[test]
    fn test_client_constructs_with_valid_url() {
        let client = TestClient::new("http://localhost:3000");
        assert_eq!(client.base_url, "http://localhost:3000");
    }
}
