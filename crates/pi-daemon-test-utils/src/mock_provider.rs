//! Mock LLM provider for integration testing.
//!
//! Returns canned streaming responses without hitting any real API.
//! The mock echoes the last user message content, producing realistic
//! `StreamEvent` sequences including `TextDelta`, `Stop`, and `Done` events.

use async_trait::async_trait;
use pi_daemon_provider::{CompletionOptions, CompletionStream, Provider, StreamEvent};
use pi_daemon_types::error::DaemonError;
use pi_daemon_types::message::{Message, MessageContent, Role, StopReason, TokenUsage};

/// A mock provider that echoes user messages.
///
/// Produces a realistic stream: TextDelta chunks → Stop → Done(usage).
pub struct MockProvider;

impl MockProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Default for MockProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Provider for MockProvider {
    async fn complete(
        &self,
        model: &str,
        messages: Vec<Message>,
        _options: CompletionOptions,
    ) -> Result<CompletionStream, DaemonError> {
        // Extract the last user message for the echo response
        let user_content = messages
            .iter()
            .rev()
            .find(|m| m.role == Role::User)
            .map(|m| match &m.content {
                MessageContent::Text(text) => text.clone(),
                MessageContent::Blocks(_) => "[blocks]".to_string(),
            })
            .unwrap_or_default();

        let response_text = format!("Mock response from '{model}': {user_content}");
        let input_tokens = estimate_tokens(&user_content);
        let output_tokens = estimate_tokens(&response_text);

        let stream = async_stream::stream! {
            // Emit text in small chunks (like a real LLM).
            // Chunk by chars to avoid splitting multi-byte UTF-8 sequences.
            let chars: Vec<char> = response_text.chars().collect();
            for chunk in chars.chunks(10) {
                let chunk_text: String = chunk.iter().collect();
                yield StreamEvent::TextDelta(chunk_text);
            }

            yield StreamEvent::Stop(StopReason::EndTurn);

            yield StreamEvent::Done(TokenUsage {
                input_tokens,
                output_tokens,
                cache_read_tokens: None,
                cache_creation_tokens: None,
            });
        };

        Ok(Box::pin(stream))
    }
}

fn estimate_tokens(text: &str) -> u32 {
    (text.len() as f32 / 4.0).ceil() as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio_stream::StreamExt;

    #[tokio::test]
    async fn test_mock_provider_streams_response() {
        let provider = MockProvider::new();
        let messages = vec![Message {
            role: Role::User,
            content: MessageContent::Text("Hello world".to_string()),
        }];

        let mut stream = provider
            .complete("test-model", messages, CompletionOptions::default())
            .await
            .unwrap();

        let mut text = String::new();
        let mut got_stop = false;
        let mut got_done = false;

        while let Some(event) = stream.next().await {
            match event {
                StreamEvent::TextDelta(t) => text.push_str(&t),
                StreamEvent::Stop(_) => got_stop = true,
                StreamEvent::Done(usage) => {
                    got_done = true;
                    assert!(usage.input_tokens > 0);
                    assert!(usage.output_tokens > 0);
                }
                _ => {}
            }
        }

        assert!(text.contains("Mock response from 'test-model'"));
        assert!(text.contains("Hello world"));
        assert!(got_stop);
        assert!(got_done);
    }
}
