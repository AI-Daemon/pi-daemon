//! Pi binary discovery — find the `pi` binary, check version compatibility.

use crate::config::PiConfig;
use std::fmt;
use std::path::PathBuf;
use tokio::process::Command;
use tracing::debug;

/// Result of discovering the Pi binary.
#[derive(Debug, Clone)]
pub struct PiDiscovery {
    /// Absolute path to the pi binary.
    pub path: PathBuf,
    /// Version string (e.g., "0.56.2").
    pub version: String,
}

/// Errors during Pi discovery.
#[derive(Debug)]
pub enum PiDiscoveryError {
    /// Pi binary not found on PATH or at configured path.
    NotFound,
    /// Pi binary found but version is below minimum.
    VersionTooOld {
        found: String,
        required: String,
        path: PathBuf,
    },
    /// Could not execute pi or parse version output.
    VersionCheckFailed(String),
}

impl fmt::Display for PiDiscoveryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound => write!(f, "Pi binary not found on PATH"),
            Self::VersionTooOld {
                found,
                required,
                path,
            } => write!(
                f,
                "Pi at {} is version {found}, need >= {required}",
                path.display()
            ),
            Self::VersionCheckFailed(msg) => write!(f, "Pi version check failed: {msg}"),
        }
    }
}

/// Discover the Pi binary and verify version compatibility.
pub async fn discover_pi(config: &PiConfig) -> Result<PiDiscovery, PiDiscoveryError> {
    // Step 1: Find the binary
    let pi_path = find_pi_binary(config)?;
    debug!(path = %pi_path.display(), "Found pi binary");

    // Step 2: Get version
    let version = get_pi_version(&pi_path).await?;
    debug!(version = %version, "Pi version detected");

    // Step 3: Check version compatibility
    if !config.min_version.is_empty() {
        check_version_compat(&version, &config.min_version, &pi_path)?;
    }

    Ok(PiDiscovery {
        path: pi_path,
        version,
    })
}

/// Find the pi binary on PATH or at configured path.
fn find_pi_binary(config: &PiConfig) -> Result<PathBuf, PiDiscoveryError> {
    if !config.binary_path.is_empty() {
        // User specified an explicit path
        let path = PathBuf::from(&config.binary_path);
        if path.exists() {
            return Ok(path);
        }
        return Err(PiDiscoveryError::NotFound);
    }

    // Search PATH using which
    which_pi().ok_or(PiDiscoveryError::NotFound)
}

/// Find `pi` on PATH by checking common locations.
fn which_pi() -> Option<PathBuf> {
    // Use the which logic: check PATH entries
    if let Ok(path) = std::env::var("PATH") {
        for dir in path.split(':') {
            let candidate = PathBuf::from(dir).join("pi");
            if candidate.exists() && candidate.is_file() {
                return Some(candidate);
            }
        }
    }

    // Check common locations as fallback
    let common_paths = ["/usr/local/bin/pi", "/usr/bin/pi"];

    for path in &common_paths {
        let p = PathBuf::from(path);
        if p.exists() {
            return Some(p);
        }
    }

    None
}

/// Run `pi --version` and parse the version string.
pub async fn get_pi_version(pi_path: &PathBuf) -> Result<String, PiDiscoveryError> {
    let output = Command::new(pi_path)
        .arg("--version")
        .env("PI_NON_INTERACTIVE", "1")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()
        .await
        .map_err(|e| PiDiscoveryError::VersionCheckFailed(format!("Failed to execute pi: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(PiDiscoveryError::VersionCheckFailed(format!(
            "pi --version exited with {}: {stderr}",
            output.status
        )));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_version_output(&stdout)
}

/// Parse version from `pi --version` output.
/// Handles formats like "0.56.2", "pi 0.56.2", "pi-coding-agent 0.56.2"
fn parse_version_output(output: &str) -> Result<String, PiDiscoveryError> {
    // Try each line for a semver-like pattern
    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // Try to find a version-like pattern (digits.digits.digits)
        for word in line.split_whitespace() {
            let word = word.trim_start_matches('v');
            if is_semver_like(word) {
                return Ok(word.to_string());
            }
        }

        // If the whole line is a version
        let trimmed = line.trim_start_matches('v');
        if is_semver_like(trimmed) {
            return Ok(trimmed.to_string());
        }
    }

    Err(PiDiscoveryError::VersionCheckFailed(format!(
        "Could not parse version from output: {output}"
    )))
}

/// Check if a string looks like a semver version (x.y.z).
fn is_semver_like(s: &str) -> bool {
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() < 2 || parts.len() > 4 {
        return false;
    }
    parts.iter().all(|p| p.parse::<u32>().is_ok())
}

/// Compare found version against required minimum.
fn check_version_compat(
    found: &str,
    required: &str,
    path: &std::path::Path,
) -> Result<(), PiDiscoveryError> {
    let found_parts = parse_semver_parts(found);
    let required_parts = parse_semver_parts(required);

    if found_parts < required_parts {
        return Err(PiDiscoveryError::VersionTooOld {
            found: found.to_string(),
            required: required.to_string(),
            path: path.to_path_buf(),
        });
    }

    Ok(())
}

/// Parse "x.y.z" into (major, minor, patch) tuple for comparison.
fn parse_semver_parts(version: &str) -> (u32, u32, u32) {
    let parts: Vec<u32> = version.split('.').filter_map(|p| p.parse().ok()).collect();

    (
        parts.first().copied().unwrap_or(0),
        parts.get(1).copied().unwrap_or(0),
        parts.get(2).copied().unwrap_or(0),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_version_output_bare_version() {
        assert_eq!(parse_version_output("0.56.2").unwrap(), "0.56.2");
    }

    #[test]
    fn test_parse_version_output_with_prefix() {
        assert_eq!(parse_version_output("v0.56.2").unwrap(), "0.56.2");
    }

    #[test]
    fn test_parse_version_output_with_name() {
        assert_eq!(parse_version_output("pi 0.56.2").unwrap(), "0.56.2");
    }

    #[test]
    fn test_parse_version_output_multiline() {
        let output = "[GraphitiBridge] something\n0.56.2\n";
        assert_eq!(parse_version_output(output).unwrap(), "0.56.2");
    }

    #[test]
    fn test_parse_version_output_no_version() {
        assert!(parse_version_output("no version here").is_err());
    }

    #[test]
    fn test_is_semver_like() {
        assert!(is_semver_like("0.56.2"));
        assert!(is_semver_like("1.0.0"));
        assert!(is_semver_like("0.1"));
        assert!(!is_semver_like("hello"));
        assert!(!is_semver_like("0"));
        assert!(!is_semver_like(""));
    }

    #[test]
    fn test_parse_semver_parts() {
        assert_eq!(parse_semver_parts("0.56.2"), (0, 56, 2));
        assert_eq!(parse_semver_parts("1.0.0"), (1, 0, 0));
        assert_eq!(parse_semver_parts("0.56"), (0, 56, 0));
    }

    #[test]
    fn test_check_version_compat_ok() {
        let path = PathBuf::from("/usr/local/bin/pi");
        assert!(check_version_compat("0.56.2", "0.56.0", &path).is_ok());
        assert!(check_version_compat("1.0.0", "0.56.0", &path).is_ok());
        assert!(check_version_compat("0.56.0", "0.56.0", &path).is_ok());
    }

    #[test]
    fn test_check_version_compat_too_old() {
        let path = PathBuf::from("/usr/local/bin/pi");
        assert!(check_version_compat("0.55.9", "0.56.0", &path).is_err());
        assert!(check_version_compat("0.1.0", "0.56.0", &path).is_err());
    }

    #[test]
    fn test_find_pi_binary_explicit_path() {
        // Non-existent path should fail
        let config = PiConfig {
            binary_path: "/nonexistent/pi".to_string(),
            ..Default::default()
        };
        assert!(find_pi_binary(&config).is_err());
    }

    #[test]
    fn test_find_pi_binary_on_path() {
        // This tests the actual system — pi should be available in CI/dev
        let config = PiConfig::default();
        let result = find_pi_binary(&config);
        // Don't assert success since pi may not be installed in all test environments
        if let Ok(path) = result {
            assert!(path.exists());
        }
    }
}
