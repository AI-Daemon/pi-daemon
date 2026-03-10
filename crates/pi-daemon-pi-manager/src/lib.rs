//! Managed Pi process lifecycle — discovery, install, spawn, health monitoring.
//!
//! The Pi Manager handles the full lifecycle of a managed Pi agent process:
//! 1. **Discovery** — find the `pi` binary on PATH, check version compatibility
//! 2. **Installation** — auto-install Pi via npm if not found
//! 3. **Spawning** — launch Pi as a child process with injected env and bridge extension
//! 4. **Health monitoring** — detect crashes and auto-restart with exponential backoff

pub mod config;
pub mod discovery;
pub mod health;
pub mod installer;
pub mod spawner;

use crate::config::PiConfig;
use crate::discovery::{PiDiscovery, PiDiscoveryError};
use crate::health::HealthMonitor;
use crate::spawner::ManagedPi;
use pi_daemon_kernel::PiDaemonKernel;
use pi_daemon_types::config::DaemonConfig;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{info, warn};

/// Status of the managed Pi process.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiStatus {
    /// Whether a managed Pi process is currently running.
    pub running: bool,
    /// PID of the managed Pi process (if running).
    pub pid: Option<u32>,
    /// Pi version string (if discovered).
    pub version: Option<String>,
    /// Uptime in seconds (if running).
    pub uptime_secs: Option<u64>,
    /// Total number of restarts since daemon start.
    pub restarts: u32,
    /// Timestamp of last crash (if any).
    pub last_crash: Option<String>,
    /// Path to the Pi binary.
    pub binary_path: Option<String>,
}

/// The Pi Manager — coordinates discovery, spawning, and health monitoring.
pub struct PiManager {
    /// Daemon config (for listen_addr, provider keys, etc.).
    daemon_config: DaemonConfig,
    /// Pi-specific config.
    pi_config: PiConfig,
    /// Reference to the kernel for agent registration.
    kernel: Arc<PiDaemonKernel>,
    /// The currently running managed Pi process (if any).
    managed_pi: Arc<Mutex<Option<ManagedPi>>>,
    /// Health monitor handle.
    health_monitor: Arc<Mutex<Option<HealthMonitor>>>,
    /// Discovery result (cached after first discovery).
    discovery: Arc<Mutex<Option<PiDiscovery>>>,
    /// Total restarts since daemon start.
    restart_count: Arc<std::sync::atomic::AtomicU32>,
    /// Timestamp of last crash.
    last_crash: Arc<Mutex<Option<chrono::DateTime<chrono::Utc>>>>,
}

impl PiManager {
    /// Create a new Pi Manager.
    pub fn new(
        daemon_config: DaemonConfig,
        pi_config: PiConfig,
        kernel: Arc<PiDaemonKernel>,
    ) -> Self {
        Self {
            daemon_config,
            pi_config,
            kernel,
            managed_pi: Arc::new(Mutex::new(None)),
            health_monitor: Arc::new(Mutex::new(None)),
            discovery: Arc::new(Mutex::new(None)),
            restart_count: Arc::new(std::sync::atomic::AtomicU32::new(0)),
            last_crash: Arc::new(Mutex::new(None)),
        }
    }

    /// Start the Pi Manager — discover Pi, optionally install, spawn if auto_start.
    ///
    /// Returns Ok(true) if a managed Pi was started, Ok(false) if degraded mode.
    pub async fn start(&self) -> Result<bool, String> {
        // Step 1: Discover Pi
        let discovery = match discovery::discover_pi(&self.pi_config).await {
            Ok(d) => {
                info!(
                    path = %d.path.display(),
                    version = %d.version,
                    "Pi discovered"
                );
                d
            }
            Err(PiDiscoveryError::NotFound) => {
                if self.pi_config.auto_install {
                    info!("Pi not found, attempting auto-install...");
                    match installer::install_pi().await {
                        Ok(()) => {
                            info!("Pi installed successfully, re-discovering...");
                            match discovery::discover_pi(&self.pi_config).await {
                                Ok(d) => d,
                                Err(e) => {
                                    warn!("Pi installed but discovery failed: {e}");
                                    return Ok(false);
                                }
                            }
                        }
                        Err(e) => {
                            warn!("Pi auto-install failed: {e}");
                            warn!("Run 'pi-daemon setup' to install Pi manually");
                            return Ok(false);
                        }
                    }
                } else {
                    warn!("Pi not found on PATH — run 'pi-daemon setup' to install");
                    return Ok(false);
                }
            }
            Err(PiDiscoveryError::VersionTooOld {
                found,
                required,
                path,
            }) => {
                warn!(
                    found = %found,
                    required = %required,
                    path = %path.display(),
                    "Pi version too old"
                );
                warn!("Run 'npm update -g @mariozechner/pi-coding-agent' to update");
                return Ok(false);
            }
            Err(e) => {
                warn!("Pi discovery failed: {e}");
                return Ok(false);
            }
        };

        // Cache the discovery result
        *self.discovery.lock().await = Some(discovery.clone());

        // Step 2: Spawn if auto_start
        if !self.pi_config.auto_start {
            info!("Pi discovered but auto_start is disabled");
            return Ok(false);
        }

        self.spawn_pi(&discovery).await
    }

    /// Spawn a managed Pi instance.
    async fn spawn_pi(&self, discovery: &PiDiscovery) -> Result<bool, String> {
        let managed = spawner::spawn_pi(
            discovery,
            &self.daemon_config,
            &self.pi_config,
            &self.kernel,
        )
        .await
        .map_err(|e| format!("Failed to spawn Pi: {e}"))?;

        info!(
            pid = managed.pid(),
            agent_id = %managed.agent_id(),
            "Managed Pi agent spawned"
        );

        // Store the managed process
        *self.managed_pi.lock().await = Some(managed);

        // Start health monitoring
        let monitor = HealthMonitor::new(
            self.managed_pi.clone(),
            self.discovery.clone(),
            self.daemon_config.clone(),
            self.pi_config.clone(),
            self.kernel.clone(),
            self.restart_count.clone(),
            self.last_crash.clone(),
        );
        monitor.start();
        *self.health_monitor.lock().await = Some(monitor);

        Ok(true)
    }

    /// Get current status of the managed Pi.
    pub async fn status(&self) -> PiStatus {
        let managed = self.managed_pi.lock().await;
        let discovery = self.discovery.lock().await;
        let last_crash = self.last_crash.lock().await;

        let (running, pid, uptime_secs) = match managed.as_ref() {
            Some(m) => (m.is_running(), Some(m.pid()), Some(m.uptime_secs())),
            None => (false, None, None),
        };

        PiStatus {
            running,
            pid,
            version: discovery.as_ref().map(|d| d.version.clone()),
            uptime_secs,
            restarts: self
                .restart_count
                .load(std::sync::atomic::Ordering::Relaxed),
            last_crash: last_crash.as_ref().map(|t| t.to_rfc3339()),
            binary_path: discovery.as_ref().map(|d| d.path.display().to_string()),
        }
    }

    /// Stop the managed Pi process.
    pub async fn stop(&self) -> Result<(), String> {
        // Stop health monitor first
        if let Some(monitor) = self.health_monitor.lock().await.take() {
            monitor.stop();
        }

        // Kill the managed Pi process
        if let Some(mut managed) = self.managed_pi.lock().await.take() {
            info!(pid = managed.pid(), "Stopping managed Pi");
            managed
                .kill(&self.kernel)
                .await
                .map_err(|e| format!("Failed to stop Pi: {e}"))?;
        }

        Ok(())
    }

    /// Restart the managed Pi process.
    pub async fn restart(&self) -> Result<bool, String> {
        self.stop().await?;

        let discovery = self.discovery.lock().await.clone();
        match discovery {
            Some(d) => self.spawn_pi(&d).await,
            None => Err("Pi not discovered — cannot restart".to_string()),
        }
    }

    /// Manually start the managed Pi (if not already running).
    pub async fn start_pi(&self) -> Result<bool, String> {
        if self.managed_pi.lock().await.is_some() {
            return Err("Managed Pi is already running".to_string());
        }

        let discovery = self.discovery.lock().await.clone();
        match discovery {
            Some(d) => self.spawn_pi(&d).await,
            None => {
                // Try fresh discovery
                match discovery::discover_pi(&self.pi_config).await {
                    Ok(d) => {
                        *self.discovery.lock().await = Some(d.clone());
                        self.spawn_pi(&d).await
                    }
                    Err(e) => Err(format!("Pi not found: {e}")),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pi_daemon_types::config::DaemonConfig;

    #[test]
    fn test_pi_status_serialization() {
        let status = PiStatus {
            running: true,
            pid: Some(12345),
            version: Some("0.56.2".to_string()),
            uptime_secs: Some(120),
            restarts: 2,
            last_crash: Some("2026-03-10T10:00:00Z".to_string()),
            binary_path: Some("/usr/local/bin/pi".to_string()),
        };

        let json = serde_json::to_string(&status).unwrap();
        let parsed: PiStatus = serde_json::from_str(&json).unwrap();

        assert!(parsed.running);
        assert_eq!(parsed.pid, Some(12345));
        assert_eq!(parsed.version, Some("0.56.2".to_string()));
        assert_eq!(parsed.restarts, 2);
    }

    #[test]
    fn test_pi_status_not_running() {
        let status = PiStatus {
            running: false,
            pid: None,
            version: None,
            uptime_secs: None,
            restarts: 0,
            last_crash: None,
            binary_path: None,
        };

        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("\"running\":false"));
    }

    #[tokio::test]
    async fn test_pi_manager_status_when_not_started() {
        let config = DaemonConfig::default();
        let pi_config = PiConfig::default();
        let kernel = Arc::new(PiDaemonKernel::new());

        let manager = PiManager::new(config, pi_config, kernel);
        let status = manager.status().await;

        assert!(!status.running);
        assert_eq!(status.pid, None);
        assert_eq!(status.restarts, 0);
    }
}
