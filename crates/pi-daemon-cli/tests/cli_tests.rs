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

// ─── New tests ───────────────────────────────────────────

#[test]
fn cli_version_format_is_semver() {
    let output = Command::cargo_bin("pi-daemon")
        .unwrap()
        .arg("version")
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    // Should contain a version like "pi-daemon v0.1.0"
    assert!(
        stdout.contains("v0.") || stdout.contains("v1."),
        "Version should be semver-like, got: {}",
        stdout
    );
}

#[test]
fn cli_all_subcommands_have_help() {
    // Every subcommand should respond to --help without errors
    for subcmd in &["start", "stop", "status", "chat", "config", "version"] {
        let result = Command::cargo_bin("pi-daemon")
            .unwrap()
            .args([subcmd, "--help"])
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{} --help should succeed, got: {}",
            subcmd,
            String::from_utf8_lossy(&result.stderr)
        );
    }
}

#[test]
fn cli_config_does_not_leak_real_keys() {
    let output = Command::cargo_bin("pi-daemon")
        .unwrap()
        .arg("config")
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should NOT contain real API key patterns
    assert!(
        !stdout.contains("sk-ant-"),
        "Config output should not contain Anthropic keys"
    );
    assert!(
        !stdout.contains("sk-proj-"),
        "Config output should not contain OpenAI keys"
    );
    assert!(
        !stdout.contains("ghp_"),
        "Config output should not contain GitHub PATs"
    );
}
