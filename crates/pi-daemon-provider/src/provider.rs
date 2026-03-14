//! The core Provider trait.

use async_trait::async_trait;
use pi_daemon_types::error::DaemonError;
use pi_daemon_types::message::Message;

use crate::types::{CompletionOptions, CompletionStream};

/// Trait for LLM provider clients.
#[async_trait]
pub trait Provider: Send + Sync {
    /// Send a completion request and return a stream of events.
    async fn complete(
        &self,
        model: &str,
        messages: Vec<Message>,
        options: CompletionOptions,
    ) -> Result<CompletionStream, DaemonError>;
}
