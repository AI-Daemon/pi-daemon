use crate::client::TestClient;
use crate::mock_provider::MockProvider;
use axum::{routing::get, Router};
use pi_daemon_api::state::AppState;
use pi_daemon_kernel::PiDaemonKernel;
use pi_daemon_types::config::DaemonConfig;
use std::sync::Arc;
use tokio::net::TcpListener;

/// Test server configuration for integration testing.
///
/// Provides a lightweight HTTP server with basic endpoints for testing
/// API interactions and WebSocket functionality.
pub struct TestServer {
    /// Base URL including port (e.g., `http://127.0.0.1:12345`).
    pub base_url: String,
    /// The port the server is listening on.
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

/// Full API server backed by a real `PiDaemonKernel`.
///
/// This is the primary test helper for integration tests. It boots a complete
/// pi-daemon API server (HTTP + WebSocket) on a random port with a real kernel,
/// event bus, and agent registry.
///
/// # Example
/// ```ignore
/// let server = FullTestServer::new().await;
/// let client = server.client();
///
/// let resp = client.get("/api/health").await;
/// assert_eq!(resp.status().as_u16(), 200);
///
/// // WebSocket URL helper
/// let ws_url = server.ws_url("my-agent");
/// ```
pub struct FullTestServer {
    /// Base URL including port (e.g., `http://127.0.0.1:12345`).
    pub base_url: String,
    /// The port the server is listening on.
    pub port: u16,
    /// Shared application state (access to kernel, config, etc.).
    pub state: Arc<AppState>,
}

impl FullTestServer {
    /// Boot a full pi-daemon API server with a real kernel on a random port.
    ///
    /// Uses default `DaemonConfig` with a [`MockProvider`] injected so that
    /// `/v1/chat/completions` works without real API keys.
    pub async fn new() -> Self {
        let config = DaemonConfig {
            listen_addr: "127.0.0.1:0".to_string(),
            ..Default::default()
        };
        Self::with_config(config).await
    }

    /// Boot a full pi-daemon API server with custom configuration.
    ///
    /// A [`MockProvider`] is injected so chat completions work in tests.
    /// The `listen_addr` field in the config is ignored — a random port is always used.
    pub async fn with_config(mut config: DaemonConfig) -> Self {
        config.listen_addr = "127.0.0.1:0".to_string();

        let kernel = Arc::new(PiDaemonKernel::new());
        kernel.init().await;

        // Inject mock provider so /v1/chat/completions works without real API keys
        let mock = Arc::new(MockProvider::new());
        let state = Arc::new(AppState::with_provider(kernel, config, mock));

        let (router, state) = pi_daemon_api::server::build_router_with_state(state);

        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("Failed to bind to random port");
        let addr = listener.local_addr().expect("Failed to get local address");
        let port = addr.port();

        tokio::spawn(async move {
            axum::serve(
                listener,
                router.into_make_service_with_connect_info::<std::net::SocketAddr>(),
            )
            .await
            .expect("FullTestServer failed");
        });

        // Give server a moment to start
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        Self {
            base_url: format!("http://127.0.0.1:{}", port),
            port,
            state,
        }
    }

    /// Get a pre-configured `TestClient` pointing at this server.
    pub fn client(&self) -> TestClient {
        TestClient::new(&self.base_url)
    }

    /// Get a WebSocket URL for a given agent name.
    ///
    /// Returns a URL like `ws://127.0.0.1:PORT/ws/AGENT_NAME`.
    pub fn ws_url(&self, agent_name: &str) -> String {
        format!("ws://127.0.0.1:{}/ws/{}", self.port, agent_name)
    }

    /// Get a WebSocket URL with an API key query parameter.
    ///
    /// Returns a URL like `ws://127.0.0.1:PORT/ws/AGENT_NAME?api_key=KEY`.
    pub fn ws_url_with_key(&self, agent_name: &str, api_key: &str) -> String {
        format!(
            "ws://127.0.0.1:{}/ws/{}?api_key={}",
            self.port, agent_name, api_key
        )
    }
}
