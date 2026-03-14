use crate::ws::ConnectionTracker;
use pi_daemon_kernel::PiDaemonKernel;
use pi_daemon_provider::{Provider, ProviderRouter};
use pi_daemon_types::config::DaemonConfig;
use std::sync::Arc;
use std::time::Instant;
use tracing::warn;

/// Shared application state, passed to all route handlers via Axum's State extractor.
pub struct AppState {
    /// The kernel — owns agent registry, event bus, etc.
    pub kernel: Arc<PiDaemonKernel>,
    /// When the daemon started (for uptime calculation).
    pub started_at: Instant,
    /// Daemon configuration.
    pub config: DaemonConfig,
    /// Shutdown signal — notify to trigger graceful shutdown.
    pub shutdown_notify: Arc<tokio::sync::Notify>,
    /// WebSocket connection tracker (per-IP connection limits).
    pub connection_tracker: ConnectionTracker,
    /// LLM provider for chat completions.
    /// `None` if no provider API keys are configured (and no mock injected).
    pub provider: Option<Arc<dyn Provider>>,
}

impl AppState {
    pub fn new(kernel: Arc<PiDaemonKernel>, config: DaemonConfig) -> Self {
        let provider: Option<Arc<dyn Provider>> =
            match ProviderRouter::from_config(&config.providers) {
                Ok(router) if router.has_providers() => Some(Arc::new(router)),
                Ok(_) => {
                    warn!("No LLM provider API keys configured — /v1/chat/completions will return errors");
                    None
                }
                Err(e) => {
                    warn!("Failed to initialize provider router: {e}");
                    None
                }
            };

        Self {
            kernel,
            started_at: Instant::now(),
            config,
            shutdown_notify: Arc::new(tokio::sync::Notify::new()),
            connection_tracker: crate::ws::new_connection_tracker(),
            provider,
        }
    }

    /// Create AppState with an injected provider (for testing).
    pub fn with_provider(
        kernel: Arc<PiDaemonKernel>,
        config: DaemonConfig,
        provider: Arc<dyn Provider>,
    ) -> Self {
        Self {
            kernel,
            started_at: Instant::now(),
            config,
            shutdown_notify: Arc::new(tokio::sync::Notify::new()),
            connection_tracker: crate::ws::new_connection_tracker(),
            provider: Some(provider),
        }
    }
}
