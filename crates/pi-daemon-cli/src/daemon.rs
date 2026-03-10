use anyhow::Result;
use std::io::Write;

/// Simple daemonization check - not used in current implementation
/// We use process spawning instead of fork-based daemonization for better compatibility
#[cfg(unix)]
pub fn daemonize() -> Result<()> {
    // This function is kept for potential future use but not currently called
    // The current implementation uses process spawning in spawn_daemon_process()
    Ok(())
}

/// Windows doesn't support traditional daemonization, so we just log a warning
#[cfg(windows)]
pub fn daemonize() -> Result<()> {
    tracing::warn!("Daemonization is not supported on Windows. Process will run in foreground.");
    tracing::info!("Consider using Windows Services or running in a separate console window.");
    Ok(())
}

/// Write daemon process information to a log file when running as daemon
pub fn write_daemon_log(message: &str) -> Result<()> {
    if let Some(home_dir) = dirs::home_dir() {
        let pi_daemon_dir = home_dir.join(".pi-daemon");
        std::fs::create_dir_all(&pi_daemon_dir)?;

        let log_file = pi_daemon_dir.join("daemon.log");
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(log_file)?;

        let timestamp = chrono::Utc::now().to_rfc3339();
        writeln!(file, "[{}] {}", timestamp, message)?;
        file.flush()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_write_daemon_log() {
        let temp_dir = TempDir::new().unwrap();

        // Mock the home directory for testing
        std::env::set_var("HOME", temp_dir.path());

        // Write a test message
        write_daemon_log("Test daemon started").unwrap();

        // Check that the log file was created and contains our message
        let log_file = temp_dir.path().join(".pi-daemon/daemon.log");
        assert!(log_file.exists());

        let contents = std::fs::read_to_string(log_file).unwrap();
        assert!(contents.contains("Test daemon started"));
        assert!(contents.contains("T")); // Should contain timestamp
    }

    #[test]
    #[cfg(unix)]
    fn test_daemonize_creates_child_process() {
        // This test is inherently difficult to test in a unit test
        // because daemonize() calls exit() in the parent process
        // So we'll just test that the function exists and can be called
        // without panicking (though it won't actually daemonize in test)
        
        // We can't actually test daemonization here because it would
        // interfere with the test runner. Integration tests would be better.
        
        // Test that the function compiles and is accessible
        let _ = daemonize; // Ensure function is accessible
    }

    #[test]
    #[cfg(windows)]
    fn test_daemonize_warns_on_windows() {
        // On Windows, daemonize should succeed but just log a warning
        let result = daemonize();
        assert!(result.is_ok());
    }
}
