//! OpenRouter provider — OpenAI-compatible with extra routing headers.

use async_trait::async_trait;
use pi_daemon_types::error::DaemonError;
use pi_daemon_types::message::Message;

use crate::openai::OpenAIProvider;
use crate::provider::Provider;
use crate::types::{CompletionOptions, CompletionStream};

const DEFAULT_BASE_URL: &str = "https://openrouter.ai/api";

/// Client for the OpenRouter API.
///
/// Uses the same SSE format as OpenAI but adds `HTTP-Referer` and `X-Title` headers.
pub struct OpenRouterProvider {
    inner: OpenAIProvider,
}

impl OpenRouterProvider {
    /// Create a new OpenRouter provider.
    pub fn new(api_key: String, base_url: Option<String>) -> Result<Self, DaemonError> {
        let base_url = base_url
            .filter(|u| !u.is_empty())
            .unwrap_or_else(|| DEFAULT_BASE_URL.to_string());

        let inner = OpenAIProvider::with_headers(
            api_key,
            Some(base_url),
            vec![
                (
                    "HTTP-Referer".to_string(),
                    "https://pi-daemon.dev".to_string(),
                ),
                ("X-Title".to_string(), "pi-daemon".to_string()),
            ],
        )?;

        Ok(Self { inner })
    }
}

#[async_trait]
impl Provider for OpenRouterProvider {
    async fn complete(
        &self,
        model: &str,
        messages: Vec<Message>,
        options: CompletionOptions,
    ) -> Result<CompletionStream, DaemonError> {
        self.inner.complete(model, messages, options).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_openrouter_default_base_url() {
        let _p = OpenRouterProvider::new("test-key".into(), None).unwrap();
    }

    #[test]
    fn test_openrouter_custom_base_url() {
        let _p =
            OpenRouterProvider::new("key".into(), Some("https://custom.proxy/api".into())).unwrap();
    }
}
