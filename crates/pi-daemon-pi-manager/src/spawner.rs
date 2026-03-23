//! Pi process spawning — launch Pi as a managed child process with injected env and bridge.

use crate::config::PiConfig;
use crate::discovery::PiDiscovery;
use pi_daemon_kernel::PiDaemonKernel;
use pi_daemon_types::agent::{AgentId, AgentKind};
use pi_daemon_types::config::DaemonConfig;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use tokio::process::{Child, Command};
use tracing::{debug, info};

/// A managed Pi process with its metadata.
pub struct ManagedPi {
    /// The child process handle.
    child: Child,
    /// PID of the child process.
    pid: u32,
    /// Agent ID registered in the kernel.
    agent_id: AgentId,
    /// When this process was spawned.
    started_at: Instant,
    /// Whether the process has been intentionally killed.
    intentionally_stopped: bool,
}

impl ManagedPi {
    /// Get the PID.
    pub fn pid(&self) -> u32 {
        self.pid
    }

    /// Get the agent ID.
    pub fn agent_id(&self) -> &AgentId {
        &self.agent_id
    }

    /// Get uptime in seconds.
    pub fn uptime_secs(&self) -> u64 {
        self.started_at.elapsed().as_secs()
    }

    /// Check if the process is still running.
    pub fn is_running(&self) -> bool {
        // We can't call try_wait on &self (needs &mut), so check via /proc
        // This is a best-effort check — the health monitor does the real polling
        let pid_path = format!("/proc/{}", self.pid);
        std::path::Path::new(&pid_path).exists()
    }

    /// Check if the process has exited (non-blocking).
    pub async fn try_wait(&mut self) -> Option<std::process::ExitStatus> {
        match self.child.try_wait() {
            Ok(Some(status)) => Some(status),
            Ok(None) => None, // still running
            Err(_) => None,
        }
    }

    /// Was this process intentionally stopped?
    pub fn was_intentionally_stopped(&self) -> bool {
        self.intentionally_stopped
    }

    /// Kill the managed Pi process and unregister from kernel.
    pub async fn kill(&mut self, kernel: &PiDaemonKernel) -> Result<(), String> {
        self.intentionally_stopped = true;
        let _ = self.child.kill().await;
        let _ = self.child.wait().await;
        kernel
            .unregister_agent(&self.agent_id, "Managed Pi stopped".to_string())
            .await;
        info!(pid = self.pid, "Managed Pi process killed");
        Ok(())
    }
}

/// Spawn a managed Pi process.
pub async fn spawn_pi(
    discovery: &PiDiscovery,
    daemon_config: &DaemonConfig,
    pi_config: &PiConfig,
    kernel: &Arc<PiDaemonKernel>,
) -> Result<ManagedPi, String> {
    let mut cmd = Command::new(&discovery.path);

    // Set working directory
    let working_dir = resolve_working_dir(&pi_config.working_directory);
    if working_dir.exists() {
        cmd.current_dir(&working_dir);
    }

    // Inject environment variables — API keys flow from daemon config
    cmd.env(
        "PI_DAEMON_URL",
        format!("http://{}", daemon_config.listen_addr),
    );
    cmd.env("PI_NON_INTERACTIVE", "1");

    if !daemon_config.providers.anthropic_api_key.is_empty() {
        cmd.env(
            "ANTHROPIC_API_KEY",
            &daemon_config.providers.anthropic_api_key,
        );
    }
    if !daemon_config.providers.openai_api_key.is_empty() {
        cmd.env("OPENAI_API_KEY", &daemon_config.providers.openai_api_key);
    }
    if !daemon_config.providers.openrouter_api_key.is_empty() {
        cmd.env(
            "OPENROUTER_API_KEY",
            &daemon_config.providers.openrouter_api_key,
        );
    }
    if !daemon_config.github.personal_access_token.is_empty() {
        cmd.env("GITHUB_TOKEN", &daemon_config.github.personal_access_token);
    }

    // Inject bridge extension path
    let bridge_path = find_bridge_extension();
    if let Some(ref bridge) = bridge_path {
        debug!(path = %bridge.display(), "Injecting bridge extension");
        // Pi accepts extensions via --extension flag or PI_EXTENSIONS env var
        cmd.env("PI_EXTENSIONS", bridge.display().to_string());
    }

    // Add extra flags from config
    for flag in &pi_config.extra_flags {
        cmd.arg(flag);
    }

    // Pipe stdout/stderr for logging
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());
    // Provide /dev/null stdin so Pi doesn't wait for input
    cmd.stdin(std::process::Stdio::null());

    // Spawn the process
    let child = cmd
        .spawn()
        .map_err(|e| format!("Failed to spawn pi: {e}"))?;

    let pid = child
        .id()
        .ok_or_else(|| "Failed to get PID of spawned pi process".to_string())?;

    // Register the managed Pi in the kernel's agent registry
    let agent_id = kernel
        .register_agent(
            format!("pi-managed-{pid}"),
            AgentKind::PiInstance,
            Some(daemon_config.default_model.clone()),
        )
        .await;

    info!(
        pid = pid,
        agent_id = %agent_id,
        binary = %discovery.path.display(),
        version = %discovery.version,
        bridge = ?bridge_path.as_ref().map(|p| p.display().to_string()),
        "Managed Pi process spawned"
    );

    Ok(ManagedPi {
        child,
        pid,
        agent_id,
        started_at: Instant::now(),
        intentionally_stopped: false,
    })
}

/// Resolve the working directory, expanding `~` to home dir.
fn resolve_working_dir(dir: &str) -> PathBuf {
    if dir == "~" || dir.starts_with("~/") {
        if let Some(home) = dirs::home_dir() {
            if dir == "~" {
                return home;
            }
            return home.join(&dir[2..]);
        }
    }
    PathBuf::from(dir)
}

/// Find the bridge extension directory.
/// Looks in: the daemon's own extensions dir, npm global install, cwd.
fn find_bridge_extension() -> Option<PathBuf> {
    // Check relative to the current executable
    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            // If running from the repo: <repo>/target/release/pi-daemon
            // Bridge is at: <repo>/extensions/pi-daemon-bridge/
            let repo_bridge = exe_dir
                .join("../../extensions/pi-daemon-bridge")
                .canonicalize();
            if let Ok(path) = repo_bridge {
                if path.join("package.json").exists() {
                    return Some(path);
                }
            }
        }
    }

    // Check standard locations
    let candidates = [
        // In the daemon's data directory
        dirs::home_dir().map(|h| h.join(".pi-daemon/extensions/pi-daemon-bridge")),
        // In the current working directory (dev mode)
        Some(PathBuf::from("extensions/pi-daemon-bridge")),
    ];

    for candidate in candidates.iter().flatten() {
        if candidate.join("package.json").exists() {
            return candidate.canonicalize().ok();
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_working_dir_tilde() {
        let result = resolve_working_dir("~");
        // Should resolve to actual home directory
        if let Some(home) = dirs::home_dir() {
            assert_eq!(result, home);
        }
    }

    #[test]
    fn test_resolve_working_dir_tilde_subdir() {
        let result = resolve_working_dir("~/projects");
        if let Some(home) = dirs::home_dir() {
            assert_eq!(result, home.join("projects"));
        }
    }

    #[test]
    fn test_resolve_working_dir_absolute() {
        let result = resolve_working_dir("/tmp/test");
        assert_eq!(result, PathBuf::from("/tmp/test"));
    }

    #[test]
    fn test_resolve_working_dir_relative() {
        let result = resolve_working_dir("relative/path");
        assert_eq!(result, PathBuf::from("relative/path"));
    }
}
