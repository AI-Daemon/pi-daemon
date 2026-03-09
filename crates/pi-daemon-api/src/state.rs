use pi_daemon_kernel::PiDaemonKernel;
use pi_daemon_types::config::DaemonConfig;
use std::sync::Arc;
use std::time::Instant;

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
}

impl AppState {
    pub fn new(kernel: Arc<PiDaemonKernel>, config: DaemonConfig) -> Self {
        Self {
            kernel,
            started_at: Instant::now(),
            config,
            shutdown_notify: Arc::new(tokio::sync::Notify::new()),
        }
    }
}
