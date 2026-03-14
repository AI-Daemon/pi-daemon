//! LLM provider clients — streaming completions from Anthropic, OpenAI, and OpenRouter.
//!
//! This crate provides a unified [`Provider`] trait for making streaming LLM completion
//! requests, with concrete implementations for Anthropic, OpenAI, and OpenRouter APIs.

pub mod anthropic;
pub mod convert;
pub mod openai;
pub mod openrouter;
pub mod provider;
pub mod sse;
pub mod types;

pub use anthropic::AnthropicProvider;
pub use openai::OpenAIProvider;
pub use openrouter::OpenRouterProvider;
pub use provider::Provider;
pub use types::*;
