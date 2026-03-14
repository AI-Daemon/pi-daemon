//! LLM provider clients — streaming completions from Anthropic, OpenAI, and OpenRouter.
//!
//! This crate provides a unified [`Provider`] trait for making streaming LLM completion
//! requests. Provider implementations are added incrementally.

pub mod convert;
pub mod provider;
pub mod sse;
pub mod types;

pub use provider::Provider;
pub use types::*;
