//! Provider routing — model name → provider dispatch.

use async_trait::async_trait;
use pi_daemon_types::config::ProvidersConfig;
use pi_daemon_types::error::DaemonError;
use pi_daemon_types::message::Message;

use crate::anthropic::AnthropicProvider;
use crate::openai::OpenAIProvider;
use crate::openrouter::OpenRouterProvider;
use crate::provider::Provider;
use crate::types::{CompletionOptions, CompletionStream};

/// Routes completion requests to the appropriate provider based on model name.
pub struct ProviderRouter {
    anthropic: Option<AnthropicProvider>,
    openai: Option<OpenAIProvider>,
    openrouter: Option<OpenRouterProvider>,
}

impl ProviderRouter {
    /// Build a router from the daemon's provider configuration.
    ///
    /// Only providers with non-empty API keys are initialized.
    /// Returns an error if a provider's HTTP client fails to build.
    pub fn from_config(config: &ProvidersConfig) -> Result<Self, DaemonError> {
        let anthropic = if !config.anthropic_api_key.is_empty() {
            let base_url = if config.anthropic_base_url.is_empty() {
                None
            } else {
                Some(config.anthropic_base_url.clone())
            };
            Some(AnthropicProvider::new(
                config.anthropic_api_key.clone(),
                base_url,
            )?)
        } else {
            None
        };

        let openai = if !config.openai_api_key.is_empty() {
            let base_url = if config.openai_base_url.is_empty() {
                None
            } else {
                Some(config.openai_base_url.clone())
            };
            Some(OpenAIProvider::new(
                config.openai_api_key.clone(),
                base_url,
            )?)
        } else {
            None
        };

        let openrouter = if !config.openrouter_api_key.is_empty() {
            Some(OpenRouterProvider::new(
                config.openrouter_api_key.clone(),
                None,
            )?)
        } else {
            None
        };

        Ok(Self {
            anthropic,
            openai,
            openrouter,
        })
    }

    /// Determine which provider handles the given model.
    ///
    /// Routing rules:
    /// - `claude-*` → Anthropic
    /// - `gpt-*`, `o1-*`, `o3-*`, `o4-*` → OpenAI
    /// - Everything else → OpenRouter (fallback)
    pub fn route(&self, model: &str) -> Result<&dyn Provider, DaemonError> {
        if model.starts_with("claude-") {
            self.anthropic
                .as_ref()
                .map(|p| p as &dyn Provider)
                .ok_or_else(|| {
                    DaemonError::Config(format!(
                        "Model '{model}' requires Anthropic, but no API key is configured"
                    ))
                })
        } else if model.starts_with("gpt-")
            || model.starts_with("o1-")
            || model.starts_with("o3-")
            || model.starts_with("o4-")
        {
            self.openai
                .as_ref()
                .map(|p| p as &dyn Provider)
                .ok_or_else(|| {
                    DaemonError::Config(format!(
                        "Model '{model}' requires OpenAI, but no API key is configured"
                    ))
                })
        } else {
            // Fallback: try OpenRouter, then OpenAI, then Anthropic
            self.openrouter
                .as_ref()
                .map(|p| p as &dyn Provider)
                .or_else(|| self.openai.as_ref().map(|p| p as &dyn Provider))
                .or_else(|| self.anthropic.as_ref().map(|p| p as &dyn Provider))
                .ok_or_else(|| {
                    DaemonError::Config(format!("No provider configured for model '{model}'"))
                })
        }
    }

    /// Check whether any provider is available.
    pub fn has_providers(&self) -> bool {
        self.anthropic.is_some() || self.openai.is_some() || self.openrouter.is_some()
    }
}

#[async_trait]
impl Provider for ProviderRouter {
    async fn complete(
        &self,
        model: &str,
        messages: Vec<Message>,
        options: CompletionOptions,
    ) -> Result<CompletionStream, DaemonError> {
        let provider = self.route(model)?;
        provider.complete(model, messages, options).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pi_daemon_types::config::ProvidersConfig;

    fn config_with_anthropic() -> ProvidersConfig {
        ProvidersConfig {
            anthropic_api_key: "sk-ant-test".to_string(),
            ..Default::default()
        }
    }

    fn config_with_openai() -> ProvidersConfig {
        ProvidersConfig {
            openai_api_key: "sk-openai-test".to_string(),
            ..Default::default()
        }
    }

    fn config_with_all() -> ProvidersConfig {
        ProvidersConfig {
            anthropic_api_key: "sk-ant-test".to_string(),
            openai_api_key: "sk-openai-test".to_string(),
            openrouter_api_key: "sk-or-test".to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn test_route_claude_to_anthropic() {
        let router = ProviderRouter::from_config(&config_with_anthropic()).unwrap();
        assert!(router.route("claude-sonnet-4-20250514").is_ok());
    }

    #[test]
    fn test_route_gpt_to_openai() {
        let router = ProviderRouter::from_config(&config_with_openai()).unwrap();
        assert!(router.route("gpt-4o").is_ok());
    }

    #[test]
    fn test_route_o1_to_openai() {
        let router = ProviderRouter::from_config(&config_with_openai()).unwrap();
        assert!(router.route("o1-preview").is_ok());
    }

    #[test]
    fn test_route_o3_to_openai() {
        let router = ProviderRouter::from_config(&config_with_openai()).unwrap();
        assert!(router.route("o3-mini").is_ok());
    }

    #[test]
    fn test_route_unknown_to_openrouter_fallback() {
        let router = ProviderRouter::from_config(&config_with_all()).unwrap();
        assert!(router.route("deepseek-coder-v2").is_ok());
    }

    #[test]
    fn test_route_missing_provider_error() {
        let router = ProviderRouter::from_config(&ProvidersConfig::default()).unwrap();
        match router.route("claude-sonnet-4-20250514") {
            Err(e) => assert!(e.to_string().contains("Anthropic")),
            Ok(_) => panic!("Expected error for missing Anthropic key"),
        }
    }

    #[test]
    fn test_route_missing_openai_error() {
        let router = ProviderRouter::from_config(&ProvidersConfig::default()).unwrap();
        match router.route("gpt-4o") {
            Err(e) => assert!(e.to_string().contains("OpenAI")),
            Ok(_) => panic!("Expected error for missing OpenAI key"),
        }
    }

    #[test]
    fn test_route_no_providers_error() {
        let router = ProviderRouter::from_config(&ProvidersConfig::default()).unwrap();
        match router.route("llama-3.1-70b") {
            Err(e) => assert!(e.to_string().contains("No provider configured")),
            Ok(_) => panic!("Expected error for no providers"),
        }
    }

    #[test]
    fn test_has_providers() {
        let empty = ProviderRouter::from_config(&ProvidersConfig::default()).unwrap();
        assert!(!empty.has_providers());

        let with_key = ProviderRouter::from_config(&config_with_anthropic()).unwrap();
        assert!(with_key.has_providers());
    }

    #[test]
    fn test_from_config_custom_base_urls() {
        let config = ProvidersConfig {
            anthropic_api_key: "key".to_string(),
            anthropic_base_url: "https://proxy.example.com".to_string(),
            openai_api_key: "key".to_string(),
            openai_base_url: "https://openai-proxy.example.com".to_string(),
            ..Default::default()
        };
        let router = ProviderRouter::from_config(&config).unwrap();
        assert!(router.has_providers());
        assert!(router.route("claude-sonnet-4-20250514").is_ok());
        assert!(router.route("gpt-4o").is_ok());
    }
}
