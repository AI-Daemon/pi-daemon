//! Health monitoring — detect crashes and auto-restart with exponential backoff.

use crate::config::PiConfig;
use crate::discovery::PiDiscovery;
use crate::spawner;
use pi_daemon_kernel::PiDaemonKernel;
use pi_daemon_types::config::DaemonConfig;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tracing::{error, info, warn};

/// Minimum backoff delay (1 second).
const MIN_BACKOFF_SECS: u64 = 1;
/// Maximum backoff delay (30 seconds).
const MAX_BACKOFF_SECS: u64 = 30;
/// If the process runs longer than this, reset the backoff.
const STABLE_RUN_SECS: u64 = 60;
/// How often to poll the child process (seconds).
const POLL_INTERVAL_SECS: u64 = 5;

/// Health monitor for a managed Pi process.
pub struct HealthMonitor {
    /// Handle to the monitoring task (kept alive to prevent task abort on drop).
    _task_handle: Arc<Mutex<Option<JoinHandle<()>>>>,
    /// Cancellation token.
    cancel: tokio_util::sync::CancellationToken,
}

impl HealthMonitor {
    /// Create a new health monitor.
    pub fn new(
        managed_pi: Arc<Mutex<Option<spawner::ManagedPi>>>,
        discovery: Arc<Mutex<Option<PiDiscovery>>>,
        daemon_config: DaemonConfig,
        pi_config: PiConfig,
        kernel: Arc<PiDaemonKernel>,
        restart_count: Arc<AtomicU32>,
        last_crash: Arc<Mutex<Option<chrono::DateTime<chrono::Utc>>>>,
    ) -> Self {
        let cancel = tokio_util::sync::CancellationToken::new();
        let cancel_clone = cancel.clone();

        let task_handle = tokio::spawn(async move {
            health_loop(
                managed_pi,
                discovery,
                daemon_config,
                pi_config,
                kernel,
                restart_count,
                last_crash,
                cancel_clone,
            )
            .await;
        });

        Self {
            _task_handle: Arc::new(Mutex::new(Some(task_handle))),
            cancel,
        }
    }

    /// Start monitoring (already started in `new`, this is a no-op placeholder).
    pub fn start(&self) {
        // Monitoring task is already running from new()
    }

    /// Stop the health monitor.
    pub fn stop(&self) {
        self.cancel.cancel();
    }
}

/// The main health monitoring loop.
#[allow(clippy::too_many_arguments)]
async fn health_loop(
    managed_pi: Arc<Mutex<Option<spawner::ManagedPi>>>,
    discovery: Arc<Mutex<Option<PiDiscovery>>>,
    daemon_config: DaemonConfig,
    pi_config: PiConfig,
    kernel: Arc<PiDaemonKernel>,
    restart_count: Arc<AtomicU32>,
    last_crash: Arc<Mutex<Option<chrono::DateTime<chrono::Utc>>>>,
    cancel: tokio_util::sync::CancellationToken,
) {
    let mut current_backoff = MIN_BACKOFF_SECS;

    loop {
        // Wait before checking
        tokio::select! {
            _ = tokio::time::sleep(std::time::Duration::from_secs(POLL_INTERVAL_SECS)) => {}
            _ = cancel.cancelled() => {
                info!("Health monitor cancelled");
                return;
            }
        }

        // Check if the managed Pi process is still running
        let mut lock = managed_pi.lock().await;
        let exit_status = if let Some(ref mut pi) = *lock {
            if pi.was_intentionally_stopped() {
                // Intentional stop — don't restart
                continue;
            }
            pi.try_wait().await
        } else {
            continue;
        };

        if let Some(status) = exit_status {
            // Process exited unexpectedly
            let uptime = lock.as_ref().map(|p| p.uptime_secs()).unwrap_or(0);

            warn!(
                exit_status = %status,
                uptime_secs = uptime,
                "Managed Pi process exited unexpectedly"
            );

            // Record the crash
            *last_crash.lock().await = Some(chrono::Utc::now());
            restart_count.fetch_add(1, Ordering::Relaxed);

            // Unregister the dead agent
            if let Some(pi) = lock.as_ref() {
                kernel
                    .unregister_agent(pi.agent_id(), format!("Crashed: {status}"))
                    .await;
            }

            // Drop the old managed Pi
            *lock = None;
            drop(lock);

            // Reset backoff if it ran long enough
            if uptime >= STABLE_RUN_SECS {
                current_backoff = MIN_BACKOFF_SECS;
            }

            // Wait with backoff before restart
            info!(
                backoff_secs = current_backoff,
                "Restarting managed Pi after backoff"
            );

            tokio::select! {
                _ = tokio::time::sleep(std::time::Duration::from_secs(current_backoff)) => {}
                _ = cancel.cancelled() => {
                    info!("Health monitor cancelled during backoff");
                    return;
                }
            }

            // Respawn
            let disc = discovery.lock().await.clone();
            if let Some(disc) = disc {
                match spawner::spawn_pi(&disc, &daemon_config, &pi_config, &kernel).await {
                    Ok(new_pi) => {
                        info!(
                            pid = new_pi.pid(),
                            restarts = restart_count.load(Ordering::Relaxed),
                            "Managed Pi restarted successfully"
                        );
                        *managed_pi.lock().await = Some(new_pi);
                        // Increase backoff for next crash (exponential)
                        current_backoff = (current_backoff * 2).min(MAX_BACKOFF_SECS);
                    }
                    Err(e) => {
                        error!(
                            error = %e,
                            "Failed to restart managed Pi — will retry"
                        );
                        // Increase backoff on failure too
                        current_backoff = (current_backoff * 2).min(MAX_BACKOFF_SECS);
                    }
                }
            } else {
                error!("Cannot restart managed Pi — no discovery result cached");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backoff_constants() {
        let min: u64 = MIN_BACKOFF_SECS;
        let max: u64 = MAX_BACKOFF_SECS;
        let stable: u64 = STABLE_RUN_SECS;
        let poll: u64 = POLL_INTERVAL_SECS;
        assert!(min >= 1);
        assert!(max >= min);
        assert!(max <= 60);
        assert!(stable >= 30);
        assert!(poll >= 1);
    }

    #[test]
    fn test_exponential_backoff_calculation() {
        let mut backoff = MIN_BACKOFF_SECS;
        let sequence: Vec<u64> = (0..8)
            .map(|_| {
                let current = backoff;
                backoff = (backoff * 2).min(MAX_BACKOFF_SECS);
                current
            })
            .collect();

        // Should grow: 1, 2, 4, 8, 16, 30, 30, 30
        assert_eq!(sequence[0], 1);
        assert_eq!(sequence[1], 2);
        assert_eq!(sequence[2], 4);
        assert_eq!(sequence[3], 8);
        assert_eq!(sequence[4], 16);
        assert_eq!(sequence[5], 30); // capped
        assert_eq!(sequence[6], 30);
    }
}
