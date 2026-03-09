//! Shared test utilities for pi-daemon.
//!
//! This crate provides common testing infrastructure including:
//! - `TestKernel` for isolated test environments
//! - `TestServer` for HTTP API testing
//! - `TestClient` for making test requests
//! - Assertion macros for JSON and HTTP responses

pub mod client;
pub mod kernel;
pub mod macros;
pub mod server;

// Re-export main types for convenience
pub use client::TestClient;
pub use kernel::TestKernel;
pub use server::TestServer;

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

    #[test]
    fn test_client_constructs_with_valid_url() {
        let client = TestClient::new("http://localhost:3000");
        assert_eq!(client.base_url, "http://localhost:3000");
    }
}
