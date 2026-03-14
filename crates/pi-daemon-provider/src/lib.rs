//! LLM provider clients — streaming completions from Anthropic, OpenAI, and OpenRouter.
//!
//! This crate provides a unified [`Provider`] trait for making streaming LLM completion
//! requests, with concrete implementations for Anthropic, OpenAI, and OpenRouter APIs.
//!
//! Use [`ProviderRouter`] to automatically route model names to the correct provider.
//!
//! # Example
//!
//! ```rust,no_run
//! use pi_daemon_provider::{ProviderRouter, Provider, CompletionOptions};
//! use pi_daemon_types::config::ProvidersConfig;
//! use pi_daemon_types::message::{Message, MessageContent, Role};
//! use tokio_stream::StreamExt;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let config = ProvidersConfig {
//!     anthropic_api_key: "sk-ant-...".to_string(),
//!     ..Default::default()
//! };
//!
//! let router = ProviderRouter::from_config(&config)?;
//!
//! let messages = vec![Message {
//!     role: Role::User,
//!     content: MessageContent::Text("Hello!".to_string()),
//! }];
//!
//! let mut stream = router.complete("claude-sonnet-4-20250514", messages, CompletionOptions::default()).await?;
//!
//! while let Some(event) = stream.next().await {
//!     // Handle StreamEvent variants
//! }
//! # Ok(())
//! # }
//! ```

pub mod anthropic;
pub mod convert;
pub mod openai;
pub mod openrouter;
pub mod provider;
pub mod router;
pub mod sse;
pub mod types;

pub use anthropic::AnthropicProvider;
pub use openai::OpenAIProvider;
pub use openrouter::OpenRouterProvider;
pub use provider::Provider;
pub use router::ProviderRouter;
pub use types::*;
