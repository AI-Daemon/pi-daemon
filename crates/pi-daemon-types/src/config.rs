use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Top-level daemon configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DaemonConfig {
    /// HTTP listen address (default: "127.0.0.1:4200").
    pub listen_addr: String,

    /// API key for authenticating HTTP/WebSocket requests.
    /// Empty string means no authentication required.
    pub api_key: String,

    /// Default LLM model to use (e.g., "claude-sonnet-4-20250514").
    pub default_model: String,

    /// Data directory for sessions, memory, logs.
    /// Defaults to ~/.pi-daemon/data/
    pub data_dir: PathBuf,

    /// LLM provider configuration.
    pub providers: ProvidersConfig,

    /// GitHub configuration.
    pub github: GitHubConfig,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        let data_dir = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".pi-daemon")
            .join("data");
        Self {
            listen_addr: "127.0.0.1:4200".to_string(),
            api_key: String::new(),
            default_model: "claude-sonnet-4-20250514".to_string(),
            data_dir,
            providers: ProvidersConfig::default(),
            github: GitHubConfig::default(),
        }
    }
}

/// LLM provider API keys and settings.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ProvidersConfig {
    /// Anthropic API key.
    pub anthropic_api_key: String,
    /// Anthropic base URL (default: <https://api.anthropic.com>).
    pub anthropic_base_url: String,
    /// OpenAI API key.
    pub openai_api_key: String,
    /// OpenAI base URL (default: <https://api.openai.com>).
    pub openai_base_url: String,
    /// OpenRouter API key (routes to multiple providers).
    pub openrouter_api_key: String,
    /// Ollama base URL for local models (default: <http://localhost:11434>).
    pub ollama_base_url: String,
}

/// GitHub integration configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct GitHubConfig {
    /// Personal Access Token for private repo access.
    /// Scopes needed: repo, read:org
    pub personal_access_token: String,
    /// GitHub API base URL (default: <https://api.github.com>).
    /// Override for GitHub Enterprise.
    pub api_base_url: String,
    /// Default organization/user for repo operations.
    pub default_owner: String,
}

/// Daemon runtime info written to daemon.json.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonInfo {
    pub pid: u32,
    pub listen_addr: String,
    pub started_at: String,
    pub version: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_daemon_config_default_values() {
        let config = DaemonConfig::default();
        assert_eq!(config.listen_addr, "127.0.0.1:4200");
        assert_eq!(config.api_key, "");
        assert_eq!(config.default_model, "claude-sonnet-4-20250514");
        assert!(config.data_dir.to_string_lossy().contains(".pi-daemon"));
        assert!(config.data_dir.to_string_lossy().contains("data"));
    }

    #[test]
    fn test_providers_config_default() {
        let config = ProvidersConfig::default();
        assert_eq!(config.anthropic_api_key, "");
        assert_eq!(config.openai_api_key, "");
        assert_eq!(config.openrouter_api_key, "");
        assert_eq!(config.anthropic_base_url, "");
        assert_eq!(config.openai_base_url, "");
        assert_eq!(config.ollama_base_url, "");
    }

    #[test]
    fn test_github_config_default() {
        let config = GitHubConfig::default();
        assert_eq!(config.personal_access_token, "");
        assert_eq!(config.api_base_url, "");
        assert_eq!(config.default_owner, "");
    }

    #[test]
    fn test_daemon_config_serialization() {
        let config = DaemonConfig::default();
        let toml_str = toml::to_string(&config).unwrap();

        // Verify it contains expected sections
        assert!(toml_str.contains("listen_addr"));
        assert!(toml_str.contains("[providers]"));
        assert!(toml_str.contains("[github]"));

        // Verify roundtrip
        let parsed: DaemonConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.listen_addr, config.listen_addr);
        assert_eq!(parsed.default_model, config.default_model);
    }

    #[test]
    fn test_daemon_info_serialization() {
        let info = DaemonInfo {
            pid: 1234,
            listen_addr: "127.0.0.1:4200".to_string(),
            started_at: "2026-03-09T05:30:00Z".to_string(),
            version: "0.1.0".to_string(),
        };

        let json = serde_json::to_string(&info).unwrap();
        let parsed: DaemonInfo = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.pid, 1234);
        assert_eq!(parsed.listen_addr, "127.0.0.1:4200");
        assert_eq!(parsed.version, "0.1.0");
    }
}
