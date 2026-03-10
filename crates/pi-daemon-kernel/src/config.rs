use pi_daemon_types::config::DaemonConfig;
use pi_daemon_types::error::{DaemonError, DaemonResult};
use std::path::PathBuf;
use tracing::{debug, info};

/// Get the pi-daemon home directory (~/.pi-daemon/).
pub fn daemon_home() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".pi-daemon")
}

/// Get the config file path (~/.pi-daemon/config.toml).
pub fn config_path() -> PathBuf {
    daemon_home().join("config.toml")
}

/// Get the daemon info file path (~/.pi-daemon/daemon.json).
pub fn daemon_info_path() -> PathBuf {
    daemon_home().join("daemon.json")
}

/// Load config from disk. Creates default config if file doesn't exist.
///
/// Environment variables override config file values:
/// - PI_DAEMON_LISTEN_ADDR
/// - PI_DAEMON_API_KEY
/// - PI_DAEMON_DEFAULT_MODEL
/// - ANTHROPIC_API_KEY
/// - OPENAI_API_KEY
/// - OPENROUTER_API_KEY
/// - GITHUB_TOKEN (or GH_TOKEN)
pub fn load_config() -> DaemonResult<DaemonConfig> {
    let path = config_path();

    let mut config = if path.exists() {
        debug!("Loading config from {}", path.display());
        let contents = std::fs::read_to_string(&path)
            .map_err(|e| DaemonError::Config(format!("Failed to read {}: {e}", path.display())))?;
        toml::from_str(&contents)
            .map_err(|e| DaemonError::Config(format!("Failed to parse {}: {e}", path.display())))?
    } else {
        // Create default config
        let config = DaemonConfig::default();
        save_config(&config)?;
        info!("Created default config at {}", path.display());
        config
    };

    // Environment variable overrides
    if let Ok(val) = std::env::var("PI_DAEMON_LISTEN_ADDR") {
        debug!("Overriding listen_addr from environment");
        config.listen_addr = val;
    }
    if let Ok(val) = std::env::var("PI_DAEMON_API_KEY") {
        debug!("Overriding api_key from environment");
        config.api_key = val;
    }
    if let Ok(val) = std::env::var("PI_DAEMON_DEFAULT_MODEL") {
        debug!("Overriding default_model from environment");
        config.default_model = val;
    }
    if let Ok(val) = std::env::var("ANTHROPIC_API_KEY") {
        debug!("Overriding anthropic_api_key from environment");
        config.providers.anthropic_api_key = val;
    }
    if let Ok(val) = std::env::var("OPENAI_API_KEY") {
        debug!("Overriding openai_api_key from environment");
        config.providers.openai_api_key = val;
    }
    if let Ok(val) = std::env::var("OPENROUTER_API_KEY") {
        debug!("Overriding openrouter_api_key from environment");
        config.providers.openrouter_api_key = val;
    }
    // GitHub token: check GITHUB_TOKEN first, then GH_TOKEN
    if let Ok(val) = std::env::var("GITHUB_TOKEN").or_else(|_| std::env::var("GH_TOKEN")) {
        debug!("Overriding github PAT from environment");
        config.github.personal_access_token = val;
    }

    // Set default URLs if empty
    if config.providers.anthropic_base_url.is_empty() {
        config.providers.anthropic_base_url = "https://api.anthropic.com".to_string();
    }
    if config.providers.openai_base_url.is_empty() {
        config.providers.openai_base_url = "https://api.openai.com".to_string();
    }
    if config.providers.ollama_base_url.is_empty() {
        config.providers.ollama_base_url = "http://localhost:11434".to_string();
    }
    if config.github.api_base_url.is_empty() {
        config.github.api_base_url = "https://api.github.com".to_string();
    }

    // Ensure data directory exists
    std::fs::create_dir_all(&config.data_dir)
        .map_err(|e| DaemonError::Config(format!("Failed to create data dir: {e}")))?;

    Ok(config)
}

/// Save config to disk.
pub fn save_config(config: &DaemonConfig) -> DaemonResult<()> {
    let path = config_path();
    std::fs::create_dir_all(daemon_home())
        .map_err(|e| DaemonError::Config(format!("Failed to create config dir: {e}")))?;

    let contents = create_config_toml(config)?;

    std::fs::write(&path, contents)
        .map_err(|e| DaemonError::Config(format!("Failed to write {}: {e}", path.display())))?;

    Ok(())
}

/// Create a TOML config file with comments.
fn create_config_toml(config: &DaemonConfig) -> DaemonResult<String> {
    let mut toml = String::new();

    toml.push_str("# pi-daemon configuration\n\n");
    toml.push_str("# HTTP server listen address\n");
    toml.push_str(&format!("listen_addr = \"{}\"\n\n", config.listen_addr));
    toml.push_str("# API key for authenticating requests (empty = no auth)\n");
    toml.push_str(&format!("api_key = \"{}\"\n\n", config.api_key));
    toml.push_str("# Default LLM model\n");
    toml.push_str(&format!("default_model = \"{}\"\n\n", config.default_model));
    toml.push_str("# Data directory path (will be created if it doesn't exist)\n");
    toml.push_str(&format!("data_dir = \"{}\"\n\n", config.data_dir.display()));

    toml.push_str("[providers]\n");
    toml.push_str("# Anthropic\n");
    toml.push_str(&format!(
        "anthropic_api_key = \"{}\"\n",
        config.providers.anthropic_api_key
    ));
    toml.push_str(&format!(
        "anthropic_base_url = \"{}\"\n",
        config.providers.anthropic_base_url
    ));
    toml.push_str("# OpenAI\n");
    toml.push_str(&format!(
        "openai_api_key = \"{}\"\n",
        config.providers.openai_api_key
    ));
    toml.push_str(&format!(
        "openai_base_url = \"{}\"\n",
        config.providers.openai_base_url
    ));
    toml.push_str("# OpenRouter\n");
    toml.push_str(&format!(
        "openrouter_api_key = \"{}\"\n",
        config.providers.openrouter_api_key
    ));
    toml.push_str("# Ollama (local)\n");
    toml.push_str(&format!(
        "ollama_base_url = \"{}\"\n\n",
        config.providers.ollama_base_url
    ));

    toml.push_str("[github]\n");
    toml.push_str("# Personal Access Token — needed for private repo access\n");
    toml.push_str("# Scopes: repo, read:org\n");
    toml.push_str("# Set via config or GITHUB_TOKEN / GH_TOKEN env var\n");
    toml.push_str(&format!(
        "personal_access_token = \"{}\"\n",
        config.github.personal_access_token
    ));
    toml.push_str(&format!(
        "api_base_url = \"{}\"\n",
        config.github.api_base_url
    ));
    toml.push_str(&format!(
        "default_owner = \"{}\"\n\n",
        config.github.default_owner
    ));

    toml.push_str("[pi]\n");
    toml.push_str("# Managed Pi agent configuration\n");
    toml.push_str("# Path to the pi binary (empty = auto-discover on $PATH)\n");
    toml.push_str(&format!("binary_path = \"{}\"\n", config.pi.binary_path));
    toml.push_str("# Minimum Pi version required\n");
    toml.push_str(&format!("min_version = \"{}\"\n", config.pi.min_version));
    toml.push_str("# Auto-install Pi via npm if not found\n");
    toml.push_str(&format!("auto_install = {}\n", config.pi.auto_install));
    toml.push_str("# Spawn a managed Pi agent on daemon start\n");
    toml.push_str(&format!("auto_start = {}\n", config.pi.auto_start));
    toml.push_str("# Number of managed Pi instances\n");
    toml.push_str(&format!("pool_size = {}\n", config.pi.pool_size));
    toml.push_str("# Working directory for managed Pi\n");
    toml.push_str(&format!(
        "working_directory = \"{}\"\n",
        config.pi.working_directory
    ));

    Ok(toml)
}

/// Write daemon runtime info (PID, addr) so CLI commands can find the running daemon.
pub fn write_daemon_info(info: &pi_daemon_types::config::DaemonInfo) -> DaemonResult<()> {
    let path = daemon_info_path();

    // Ensure the directory exists
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| DaemonError::Config(format!("Failed to create daemon info dir: {e}")))?;
    }

    let json = serde_json::to_string_pretty(info)
        .map_err(|e| DaemonError::Config(format!("Failed to serialize daemon info: {e}")))?;
    std::fs::write(&path, json)
        .map_err(|e| DaemonError::Config(format!("Failed to write daemon info: {e}")))?;
    debug!("Wrote daemon info to {}", path.display());
    Ok(())
}

/// Read daemon info (used by CLI to find running daemon).
pub fn read_daemon_info() -> DaemonResult<pi_daemon_types::config::DaemonInfo> {
    let path = daemon_info_path();
    let contents = std::fs::read_to_string(&path).map_err(|e| {
        DaemonError::Config(format!("Daemon not running ({}): {e}", path.display()))
    })?;
    serde_json::from_str(&contents)
        .map_err(|e| DaemonError::Config(format!("Invalid daemon info: {e}")))
}

/// Remove daemon info file (on shutdown).
pub fn remove_daemon_info() {
    let path = daemon_info_path();
    if path.exists() {
        let _ = std::fs::remove_file(&path);
        debug!("Removed daemon info file");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::env;
    use tempfile::TempDir;

    fn with_temp_home<F>(f: F)
    where
        F: FnOnce(&TempDir),
    {
        let temp_dir = TempDir::new().unwrap();

        // Save all original environment variables
        let original_vars = vec![
            ("HOME", env::var("HOME").ok()),
            (
                "PI_DAEMON_LISTEN_ADDR",
                env::var("PI_DAEMON_LISTEN_ADDR").ok(),
            ),
            ("PI_DAEMON_API_KEY", env::var("PI_DAEMON_API_KEY").ok()),
            (
                "PI_DAEMON_DEFAULT_MODEL",
                env::var("PI_DAEMON_DEFAULT_MODEL").ok(),
            ),
            ("ANTHROPIC_API_KEY", env::var("ANTHROPIC_API_KEY").ok()),
            ("OPENAI_API_KEY", env::var("OPENAI_API_KEY").ok()),
            ("OPENROUTER_API_KEY", env::var("OPENROUTER_API_KEY").ok()),
            ("GITHUB_TOKEN", env::var("GITHUB_TOKEN").ok()),
            ("GH_TOKEN", env::var("GH_TOKEN").ok()),
        ];

        // Clear all config-related environment variables
        env::remove_var("PI_DAEMON_LISTEN_ADDR");
        env::remove_var("PI_DAEMON_API_KEY");
        env::remove_var("PI_DAEMON_DEFAULT_MODEL");
        env::remove_var("ANTHROPIC_API_KEY");
        env::remove_var("OPENAI_API_KEY");
        env::remove_var("OPENROUTER_API_KEY");
        env::remove_var("GITHUB_TOKEN");
        env::remove_var("GH_TOKEN");

        // Override the home directory for this test
        env::set_var("HOME", temp_dir.path());

        f(&temp_dir);

        // Restore all original environment variables
        for (var_name, original_value) in original_vars {
            if let Some(value) = original_value {
                env::set_var(var_name, value);
            } else {
                env::remove_var(var_name);
            }
        }
    }

    #[test]
    #[serial]
    fn test_load_config_creates_default_when_missing() {
        with_temp_home(|_temp| {
            let config = load_config().unwrap();
            assert_eq!(config.listen_addr, "127.0.0.1:4200");
            assert_eq!(config.default_model, "claude-sonnet-4-20250514");

            // Config file should have been created
            let path = config_path();
            assert!(path.exists());
        });
    }

    #[test]
    #[serial]
    fn test_config_roundtrip() {
        with_temp_home(|_temp| {
            let config = DaemonConfig {
                listen_addr: "0.0.0.0:8080".to_string(),
                api_key: "test-key".to_string(),
                providers: pi_daemon_types::config::ProvidersConfig {
                    anthropic_api_key: "sk-test".to_string(),
                    ..Default::default()
                },
                ..Default::default()
            };

            save_config(&config).unwrap();

            // Verify the file was created and contains our values
            let config_file = config_path();
            assert!(config_file.exists());
            let contents = std::fs::read_to_string(&config_file).unwrap();
            assert!(contents.contains("0.0.0.0:8080"));

            let loaded = load_config().unwrap();

            assert_eq!(loaded.listen_addr, "0.0.0.0:8080");
            assert_eq!(loaded.api_key, "test-key");
            assert_eq!(loaded.providers.anthropic_api_key, "sk-test");
        });
    }

    #[test]
    #[serial]
    fn test_environment_variable_overrides() {
        with_temp_home(|_temp| {
            // Create a base config file first
            let base_config = DaemonConfig::default();
            save_config(&base_config).unwrap();

            // Set environment variables
            env::set_var("PI_DAEMON_LISTEN_ADDR", "192.168.1.100:9000");
            env::set_var("ANTHROPIC_API_KEY", "sk-env-test");
            env::set_var("GITHUB_TOKEN", "ghp_env_test");

            let config = load_config().unwrap();

            assert_eq!(config.listen_addr, "192.168.1.100:9000");
            assert_eq!(config.providers.anthropic_api_key, "sk-env-test");
            assert_eq!(config.github.personal_access_token, "ghp_env_test");
        });
    }

    #[test]
    #[serial]
    fn test_daemon_info_roundtrip() {
        with_temp_home(|_temp| {
            let info = pi_daemon_types::config::DaemonInfo {
                pid: 12345,
                listen_addr: "127.0.0.1:4200".to_string(),
                started_at: "2026-03-09T05:30:00Z".to_string(),
                version: "0.1.0".to_string(),
            };

            write_daemon_info(&info).unwrap();

            // Verify the file exists
            let info_path = daemon_info_path();
            assert!(info_path.exists());

            let loaded = read_daemon_info().unwrap();

            assert_eq!(loaded.pid, 12345);
            assert_eq!(loaded.listen_addr, "127.0.0.1:4200");
            assert_eq!(loaded.version, "0.1.0");

            remove_daemon_info();
            assert!(!info_path.exists());
            assert!(read_daemon_info().is_err());
        });
    }

    #[test]
    #[serial]
    fn test_default_urls_are_set() {
        with_temp_home(|_temp| {
            let config = load_config().unwrap();
            assert_eq!(
                config.providers.anthropic_base_url,
                "https://api.anthropic.com"
            );
            assert_eq!(config.providers.openai_base_url, "https://api.openai.com");
            assert_eq!(config.providers.ollama_base_url, "http://localhost:11434");
            assert_eq!(config.github.api_base_url, "https://api.github.com");
        });
    }

    #[test]
    #[serial]
    fn test_config_toml_has_comments() {
        let config = DaemonConfig::default();
        let toml = create_config_toml(&config).unwrap();

        assert!(toml.contains("# pi-daemon configuration"));
        assert!(toml.contains("# HTTP server listen address"));
        assert!(toml.contains("# Anthropic"));
        assert!(toml.contains("# Personal Access Token"));
        assert!(toml.contains("[providers]"));
        assert!(toml.contains("[github]"));

        // Verify it's valid TOML
        let _: DaemonConfig = toml::from_str(&toml).unwrap();
    }
}
