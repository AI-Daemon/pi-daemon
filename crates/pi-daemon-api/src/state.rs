use crate::ws::ConnectionTracker;
use pi_daemon_kernel::PiDaemonKernel;
use pi_daemon_pi_manager::PiManager;
use pi_daemon_types::config::DaemonConfig;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Mutex;

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
    /// Managed Pi process manager (set after daemon startup).
    pub pi_manager: Mutex<Option<Arc<PiManager>>>,
}

impl AppState {
    pub fn new(kernel: Arc<PiDaemonKernel>, config: DaemonConfig) -> Self {
        Self {
            kernel,
            started_at: Instant::now(),
            config,
            shutdown_notify: Arc::new(tokio::sync::Notify::new()),
            connection_tracker: crate::ws::new_connection_tracker(),
            pi_manager: Mutex::new(None),
        }
    }

    /// Set the Pi manager after it's been initialized.
    pub async fn set_pi_manager(&self, manager: Arc<PiManager>) {
        *self.pi_manager.lock().await = Some(manager);
    }
}
