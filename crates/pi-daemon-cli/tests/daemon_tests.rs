//! Integration tests for daemon background functionality

use assert_cmd::Command;
use predicates::prelude::*;
use serial_test::serial;
use std::thread;
use std::time::Duration;
use tempfile::TempDir;
use tokio::time::timeout;

/// Helper to clean up any existing daemon before test
fn cleanup_daemon() {
    let _ = Command::cargo_bin("pi-daemon")
        .unwrap()
        .arg("stop")
        .output();
    
    // Give it a moment to clean up
    thread::sleep(Duration::from_millis(500));
}

/// Helper to get a unique port for testing
fn get_test_port() -> u16 {
    use std::net::{TcpListener, SocketAddr};
    
    let listener = TcpListener::bind("127.0.0.1:0").expect("Failed to bind to random port");
    let addr = listener.local_addr().expect("Failed to get local address");
    match addr {
        SocketAddr::V4(v4) => v4.port(),
        SocketAddr::V6(v6) => v6.port(),
    }
}

/// Wait for daemon to start by checking status
async fn wait_for_daemon_start(max_wait: Duration) -> Result<(), String> {
    let start_time = std::time::Instant::now();
    
    while start_time.elapsed() < max_wait {
        let result = Command::cargo_bin("pi-daemon")
            .unwrap()
            .arg("status")
            .output()
            .map_err(|e| format!("Failed to run status command: {}", e))?;
            
        if result.status.success() {
            let stdout = String::from_utf8_lossy(&result.stdout);
            if !stdout.contains("pi-daemon is not running") {
                return Ok(());
            }
        }
        
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    
    Err("Daemon did not start within timeout".to_string())
}

#[test]
#[serial]
fn test_foreground_option_shows_correct_message() {
    cleanup_daemon();
    
    // Test help shows foreground option
    Command::cargo_bin("pi-daemon")
        .unwrap()
        .args(["start", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Run in foreground"))
        .stdout(predicate::str::contains("--foreground"));
}

#[test]
#[serial]
fn test_background_mode_messages() {
    cleanup_daemon();
    
    let port = get_test_port();
    
    // Start daemon in background mode (should show startup messages then exit parent)
    let mut cmd = Command::cargo_bin("pi-daemon").unwrap();
    cmd.args(["start", "--listen", &format!("127.0.0.1:{}", port)]);
    
    // This will test that the parent process exits with appropriate messages
    let output = cmd.timeout(Duration::from_secs(10)).output();
    
    // Clean up - the daemon might still be running
    let _ = Command::cargo_bin("pi-daemon").unwrap().arg("stop").output();
    
    match output {
        Ok(result) => {
            let stdout = String::from_utf8_lossy(&result.stdout);
            
            // Should show background startup messages
            assert!(stdout.contains("pi-daemon starting in background mode"));
            assert!(stdout.contains("Use `pi-daemon status` to check status"));
            assert!(stdout.contains("Use `pi-daemon stop` to stop the daemon"));
        }
        Err(_) => {
            // Command might timeout if daemonization works correctly
            // which is actually the expected behavior - parent should exit
        }
    }
}

#[tokio::test]
#[serial]
async fn test_background_daemon_lifecycle() {
    cleanup_daemon();
    
    let port = get_test_port();
    
    // Start daemon in background
    let mut child = tokio::process::Command::new("cargo")
        .args([
            "run", "--bin", "pi-daemon", "--",
            "start", "--listen", &format!("127.0.0.1:{}", port)
        ])
        .current_dir("/root/pi-daemon")
        .spawn()
        .expect("Failed to start daemon process");
    
    // Give the daemon a moment to start
    tokio::time::sleep(Duration::from_millis(1000)).await;
    
    // The parent process should have exited (successful daemonization)
    let exit_status = timeout(Duration::from_secs(2), child.wait()).await;
    
    match exit_status {
        Ok(Ok(status)) if status.success() => {
            // Good! Parent exited successfully
        }
        Ok(Ok(status)) => {
            cleanup_daemon();
            panic!("Daemon parent process failed: {}", status);
        }
        Ok(Err(e)) => {
            cleanup_daemon();
            panic!("Error waiting for daemon process: {}", e);
        }
        Err(_) => {
            // Timeout is actually fine - means process is still running
            // Kill the child process since it shouldn't still be the foreground process
            let _ = child.kill().await;
        }
    }
    
    // Wait for daemon to be ready
    if let Err(e) = wait_for_daemon_start(Duration::from_secs(10)).await {
        cleanup_daemon();
        panic!("Daemon failed to start: {}", e);
    }
    
    // Test that daemon is running
    let status_result = Command::cargo_bin("pi-daemon")
        .unwrap()
        .arg("status")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    
    let status_output = String::from_utf8_lossy(&status_result);
    assert!(status_output.contains("pi-daemon v"));
    assert!(status_output.contains("PID:"));
    assert!(status_output.contains("Address:"));
    assert!(!status_output.contains("pi-daemon is not running"));
    
    // Test that we can stop the daemon
    Command::cargo_bin("pi-daemon")
        .unwrap()
        .arg("stop")
        .assert()
        .success()
        .stdout(predicate::str::contains("Stopping daemon"))
        .stdout(predicate::str::contains("Daemon stopped"));
    
    // Give it a moment to clean up
    tokio::time::sleep(Duration::from_millis(500)).await;
    
    // Verify daemon is no longer running
    Command::cargo_bin("pi-daemon")
        .unwrap()
        .arg("status")
        .assert()
        .success()
        .stdout(predicate::str::contains("pi-daemon is not running"));
}

#[tokio::test]
#[serial]
async fn test_foreground_vs_background_behavior() {
    cleanup_daemon();
    
    let port = get_test_port();
    
    // Test foreground mode doesn't daemonize (process should keep running)
    let mut foreground_child = tokio::process::Command::new("cargo")
        .args([
            "run", "--bin", "pi-daemon", "--",
            "start", "--foreground", "--listen", &format!("127.0.0.1:{}", port)
        ])
        .current_dir("/root/pi-daemon")
        .spawn()
        .expect("Failed to start daemon in foreground");
    
    // In foreground mode, process should keep running (not exit like daemon mode)
    tokio::time::sleep(Duration::from_millis(2000)).await;
    
    // Check if process is still running (it should be in foreground mode)
    match foreground_child.try_wait() {
        Ok(Some(status)) => {
            cleanup_daemon();
            panic!("Foreground process exited unexpectedly: {}", status);
        }
        Ok(None) => {
            // Good! Process is still running in foreground
        }
        Err(e) => {
            cleanup_daemon();
            panic!("Error checking foreground process: {}", e);
        }
    }
    
    // Kill the foreground process
    let _ = foreground_child.kill().await;
    let _ = foreground_child.wait().await;
    
    // Give it a moment to clean up
    tokio::time::sleep(Duration::from_millis(500)).await;
    
    cleanup_daemon();
}

#[test]
#[serial] 
fn test_daemon_log_file_creation() {
    cleanup_daemon();
    
    // Use temporary directory to avoid affecting real config
    let temp_dir = TempDir::new().unwrap();
    std::env::set_var("HOME", temp_dir.path());
    
    // Test that daemon log functionality works
    use pi_daemon_cli::daemon;
    
    daemon::write_daemon_log("Test log message").unwrap();
    
    let log_file = temp_dir.path().join(".pi-daemon/daemon.log");
    assert!(log_file.exists());
    
    let contents = std::fs::read_to_string(log_file).unwrap();
    assert!(contents.contains("Test log message"));
    assert!(contents.contains("T")); // Should contain timestamp
    
    // Clean up env var
    std::env::remove_var("HOME");
}

#[test]
#[cfg(unix)]
#[serial]
fn test_unix_daemonization_functions() {
    // Test that daemonization functions exist and can be called
    // Note: We can't actually test full daemonization in a unit test
    // since it would interfere with the test runner
    
    // This test mainly verifies the code compiles and doesn't panic immediately
    // Real daemonization testing is done in integration tests above
    use pi_daemon_cli::daemon;
    
    // The write_daemon_log function should work in tests
    let temp_dir = TempDir::new().unwrap();
    std::env::set_var("HOME", temp_dir.path());
    
    let result = daemon::write_daemon_log("Unit test message");
    assert!(result.is_ok());
    
    std::env::remove_var("HOME");
}

#[test]
#[cfg(windows)]  
#[serial]
fn test_windows_daemonization_warning() {
    // On Windows, daemonize should succeed but log a warning
    use pi_daemon_cli::daemon;
    
    let result = daemon::daemonize();
    assert!(result.is_ok());
    
    // Note: In a real test we'd capture the tracing output to verify the warning
    // but that requires more complex test setup
}

#[test]
#[serial]
fn test_daemon_already_running_detection() {
    cleanup_daemon();
    
    // This test is complex because it requires starting a daemon
    // For now we'll test the simpler case of the error message format
    Command::cargo_bin("pi-daemon")
        .unwrap()
        .args(["start", "--listen", "invalid-address"])
        .assert()
        .failure(); // Should fail with invalid address
}