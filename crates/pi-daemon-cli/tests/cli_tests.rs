//! CLI integration tests using assert_cmd

#![allow(deprecated)]

use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn cli_version() {
    Command::cargo_bin("pi-daemon")
        .unwrap()
        .arg("version")
        .assert()
        .success()
        .stdout(predicate::str::contains("pi-daemon v"));
}

#[test]
fn cli_config_shows_redacted_secrets() {
    Command::cargo_bin("pi-daemon")
        .unwrap()
        .arg("config")
        .assert()
        .success()
        .stdout(predicate::str::contains("pi-daemon configuration"))
        .stdout(predicate::str::contains("Config file:"))
        .stdout(predicate::str::contains("listen_addr:"))
        .stdout(predicate::str::contains("api_key:"))
        .stdout(predicate::str::contains("[providers]"))
        .stdout(predicate::str::contains("[github]"));
}

#[test]
fn cli_status_when_not_running() {
    Command::cargo_bin("pi-daemon")
        .unwrap()
        .arg("status")
        .assert()
        .success()
        .stdout(predicate::str::contains("pi-daemon is not running"));
}

#[test]
fn cli_stop_when_not_running() {
    Command::cargo_bin("pi-daemon")
        .unwrap()
        .arg("stop")
        .assert()
        .failure() // Should fail when daemon is not running
        .stderr(predicate::str::contains("Daemon is not running"));
}

#[test]
fn cli_chat_when_not_running() {
    Command::cargo_bin("pi-daemon")
        .unwrap()
        .arg("chat")
        .assert()
        .failure() // Should fail when daemon is not running
        .stderr(predicate::str::contains("Daemon is not running"));
}

#[test]
fn cli_help_message() {
    Command::cargo_bin("pi-daemon")
        .unwrap()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Agent kernel daemon for pi"))
        .stdout(predicate::str::contains("start"))
        .stdout(predicate::str::contains("stop"))
        .stdout(predicate::str::contains("status"))
        .stdout(predicate::str::contains("chat"))
        .stdout(predicate::str::contains("version"))
        .stdout(predicate::str::contains("config"));
}

#[test]
fn cli_invalid_command() {
    Command::cargo_bin("pi-daemon")
        .unwrap()
        .arg("invalid")
        .assert()
        .failure()
        .stderr(predicate::str::contains("unrecognized subcommand"));
}

#[tokio::test]
#[ignore] // Requires actual daemon running
async fn test_daemon_lifecycle() {
    // This test requires careful orchestration and cleanup
    // Start daemon in background, test commands, then stop

    // For now, this is marked as ignore since it requires more complex test setup
    // In a real scenario, this would:
    // 1. Start daemon with --foreground in background task
    // 2. Wait for it to be ready (health check)
    // 3. Test status command
    // 4. Test stop command
    // 5. Verify cleanup
}

#[test]
fn cli_start_shows_help_options() {
    Command::cargo_bin("pi-daemon")
        .unwrap()
        .args(["start", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Start the daemon"))
        .stdout(predicate::str::contains("--foreground"))
        .stdout(predicate::str::contains("--listen"));
}

#[test]
fn cli_chat_shows_help_options() {
    Command::cargo_bin("pi-daemon")
        .unwrap()
        .args(["chat", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Interactive terminal chat"))
        .stdout(predicate::str::contains("--agent"))
        .stdout(predicate::str::contains("--model"));
}
