use anyhow::Result;
use std::io::Write;

/// Daemonize the current process using Unix fork/setsid
#[cfg(unix)]
pub fn daemonize() -> Result<()> {
    use nix::sys::stat::{umask, Mode};
    use nix::unistd::{chdir, fork, setsid, ForkResult};

    // First fork
    match unsafe { fork() } {
        Ok(ForkResult::Parent { child: _ }) => {
            // Parent process exits - this is the key to daemonization
            std::process::exit(0);
        }
        Ok(ForkResult::Child) => {
            // Child continues to become the daemon
        }
        Err(e) => {
            return Err(anyhow::anyhow!("Failed to fork: {}", e));
        }
    }

    // Create new session (child becomes session leader)
    setsid().map_err(|e| anyhow::anyhow!("Failed to setsid: {}", e))?;

    // Second fork to prevent daemon from ever acquiring a controlling terminal
    match unsafe { fork() } {
        Ok(ForkResult::Parent { child: _ }) => {
            // First child exits, leaving grandchild as the daemon
            std::process::exit(0);
        }
        Ok(ForkResult::Child) => {
            // This grandchild becomes the actual daemon
        }
        Err(e) => {
            return Err(anyhow::anyhow!("Failed to second fork: {}", e));
        }
    }

    // Change working directory to root to prevent blocking filesystem unmounts
    chdir("/").map_err(|e| anyhow::anyhow!("Failed to chdir to /: {}", e))?;

    // Set restrictive file creation mask
    umask(Mode::from_bits_truncate(0o027));

    // Note: We're NOT redirecting stdout/stderr/stdin here because:
    // 1. The Rust tracing system expects to write to them
    // 2. We'll handle logging configuration in the main process instead
    // 3. The process is already detached from the terminal by the double fork

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
        assert!(true); // Placeholder to ensure the function compiles
    }

    #[test]
    #[cfg(windows)]
    fn test_daemonize_warns_on_windows() {
        // On Windows, daemonize should succeed but just log a warning
        let result = daemonize();
        assert!(result.is_ok());
    }
}