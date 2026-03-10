//! Pi auto-installation via npm.

use tokio::process::Command;
use tracing::{debug, info, warn};

/// Error from Pi installation.
#[derive(Debug)]
pub struct InstallError(pub String);

impl std::fmt::Display for InstallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Check if Node.js is available.
pub async fn check_node() -> Result<String, InstallError> {
    let output = Command::new("node")
        .arg("--version")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .await
        .map_err(|e| {
            InstallError(format!(
                "Node.js not found: {e}. Install Node.js >= 18 first."
            ))
        })?;

    if !output.status.success() {
        return Err(InstallError(
            "Node.js --version failed. Install Node.js >= 18.".to_string(),
        ));
    }

    let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
    debug!(version = %version, "Node.js detected");
    Ok(version)
}

/// Check if npm is available.
pub async fn check_npm() -> Result<String, InstallError> {
    let output = Command::new("npm")
        .arg("--version")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .await
        .map_err(|e| InstallError(format!("npm not found: {e}")))?;

    if !output.status.success() {
        return Err(InstallError("npm --version failed".to_string()));
    }

    let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
    debug!(version = %version, "npm detected");
    Ok(version)
}

/// Install Pi globally via npm.
pub async fn install_pi() -> Result<(), InstallError> {
    // Pre-flight checks
    check_node().await?;
    check_npm().await?;

    info!("Installing Pi via npm...");

    let output = Command::new("npm")
        .args(["install", "-g", "@mariozechner/pi-coding-agent"])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .await
        .map_err(|e| InstallError(format!("npm install failed: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        warn!(
            stderr = %stderr,
            stdout = %stdout,
            "npm install failed"
        );
        return Err(InstallError(format!(
            "npm install -g @mariozechner/pi-coding-agent failed: {stderr}"
        )));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    info!(output = %stdout.trim(), "Pi installed successfully");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_check_node_available() {
        // Node.js should be available in our environment
        let result = check_node().await;
        if let Ok(version) = result {
            assert!(version.starts_with('v') || version.chars().next().unwrap().is_ascii_digit());
        }
        // Don't fail if node isn't installed — test environments vary
    }

    #[tokio::test]
    async fn test_check_npm_available() {
        let result = check_npm().await;
        if let Ok(version) = result {
            assert!(!version.is_empty());
        }
    }
}
