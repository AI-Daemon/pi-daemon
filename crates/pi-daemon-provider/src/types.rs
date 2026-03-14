//! Core types for the provider abstraction layer.
//!
//! These types define the request options and streaming events used by all
//! LLM provider implementations.

use std::pin::Pin;

use serde::{Deserialize, Serialize};
use tokio_stream::Stream;

use pi_daemon_types::message::{ContentBlock, StopReason, TokenUsage};

/// Options for a chat completion request.
///
/// These map to the common parameters shared across providers (Anthropic,
/// OpenAI, OpenRouter, Ollama). Provider-specific translation happens inside
/// each provider implementation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionOptions {
    /// Maximum number of tokens the model may generate.
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,

    /// Optional system prompt prepended to the conversation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,

    /// Sampling temperature (0.0–2.0 for most providers).
    /// `None` means use provider default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,

    /// Top-p (nucleus) sampling. `None` means use provider default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,

    /// Sequences that cause the model to stop generating.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stop_sequences: Vec<String>,

    /// Tool definitions available for the model to call.
    /// Each value is the JSON schema expected by the target provider.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<serde_json::Value>,
}

fn default_max_tokens() -> u32 {
    8192
}

impl Default for CompletionOptions {
    fn default() -> Self {
        Self {
            max_tokens: default_max_tokens(),
            system_prompt: None,
            temperature: None,
            top_p: None,
            stop_sequences: Vec::new(),
            tools: Vec::new(),
        }
    }
}

/// A single event emitted from a streaming completion.
///
/// The stream produces a sequence of these events:
///
/// 1. Zero or more `TextDelta` events carrying incremental text.
/// 2. Zero or more `ToolUse` events for tool call requests.
/// 3. Zero or more `ContentBlock` events for complete non-text blocks.
/// 4. Optionally a `Stop` event with the stop reason.
/// 5. Exactly one `Done` event with final token usage, or one `Error`.
#[derive(Debug, Clone)]
pub enum StreamEvent {
    /// An incremental text fragment from the model.
    TextDelta(String),

    /// The model wants to call a tool.
    ToolUse {
        /// Provider-assigned tool use ID for correlating results.
        id: String,
        /// Name of the tool to invoke.
        name: String,
        /// Parsed JSON arguments for the tool.
        input: serde_json::Value,
    },

    /// A complete content block (e.g. `ToolUse`, `ToolResult`).
    ///
    /// Providers accumulate partial tool-use JSON internally and emit a
    /// single `ContentBlock` once the block is fully received.
    ContentBlock(ContentBlock),

    /// The model stopped generating, along with the reason.
    Stop(StopReason),

    /// Final event — the completion is finished and token usage is available.
    Done(TokenUsage),

    /// An error occurred during streaming.
    Error(String),
}

/// A boxed, pinned, `Send`-able stream of [`StreamEvent`]s.
///
/// Every [`Provider`](super::provider::Provider) implementation returns this
/// type from its `complete` method, making it easy to consume with
/// `StreamExt` or forward into SSE / WebSocket connections.
pub type CompletionStream = Pin<Box<dyn Stream<Item = StreamEvent> + Send>>;

#[cfg(test)]
mod tests {
    use super::*;
    use pi_daemon_types::message::TokenUsage;

    #[test]
    fn test_completion_options_default() {
        let opts = CompletionOptions::default();
        assert_eq!(opts.max_tokens, 8192);
        assert!(opts.system_prompt.is_none());
        assert!(opts.temperature.is_none());
        assert!(opts.top_p.is_none());
        assert!(opts.stop_sequences.is_empty());
        assert!(opts.tools.is_empty());
    }

    #[test]
    fn test_completion_options_serialization() {
        let opts = CompletionOptions {
            max_tokens: 4096,
            system_prompt: Some("You are a helpful assistant.".to_string()),
            temperature: Some(0.7),
            top_p: None,
            stop_sequences: vec!["STOP".to_string()],
            tools: vec![serde_json::json!({
                "name": "get_weather",
                "description": "Get weather for a location",
                "input_schema": {
                    "type": "object",
                    "properties": {
                        "location": { "type": "string" }
                    },
                    "required": ["location"]
                }
            })],
        };

        let json = serde_json::to_string(&opts).unwrap();
        let roundtrip: CompletionOptions = serde_json::from_str(&json).unwrap();

        assert_eq!(roundtrip.max_tokens, 4096);
        assert_eq!(
            roundtrip.system_prompt.as_deref(),
            Some("You are a helpful assistant.")
        );
        assert_eq!(roundtrip.temperature, Some(0.7));
        assert!(roundtrip.top_p.is_none());
        assert_eq!(roundtrip.stop_sequences, vec!["STOP"]);
        assert_eq!(roundtrip.tools.len(), 1);
    }

    #[test]
    fn test_completion_options_deserialization_defaults() {
        let json = r#"{}"#;
        let opts: CompletionOptions = serde_json::from_str(json).unwrap();
        assert_eq!(opts.max_tokens, 8192);
        assert!(opts.system_prompt.is_none());
        assert!(opts.stop_sequences.is_empty());
    }

    #[test]
    fn test_stream_event_text_delta() {
        let event = StreamEvent::TextDelta("Hello".to_string());
        if let StreamEvent::TextDelta(text) = &event {
            assert_eq!(text, "Hello");
        } else {
            panic!("Expected TextDelta variant");
        }
    }

    #[test]
    fn test_stream_event_tool_use() {
        let event = StreamEvent::ToolUse {
            id: "tool_1".to_string(),
            name: "get_weather".to_string(),
            input: serde_json::json!({"location": "London"}),
        };
        if let StreamEvent::ToolUse { id, name, input } = &event {
            assert_eq!(id, "tool_1");
            assert_eq!(name, "get_weather");
            assert_eq!(input["location"], "London");
        } else {
            panic!("Expected ToolUse variant");
        }
    }

    #[test]
    fn test_stream_event_content_block() {
        let block = ContentBlock::ToolUse {
            id: "tool_1".to_string(),
            name: "get_weather".to_string(),
            input: serde_json::json!({"location": "London"}),
        };
        let event = StreamEvent::ContentBlock(block);
        if let StreamEvent::ContentBlock(ContentBlock::ToolUse { id, name, .. }) = &event {
            assert_eq!(id, "tool_1");
            assert_eq!(name, "get_weather");
        } else {
            panic!("Expected ContentBlock(ToolUse) variant");
        }
    }

    #[test]
    fn test_stream_event_stop() {
        let event = StreamEvent::Stop(StopReason::EndTurn);
        assert!(matches!(event, StreamEvent::Stop(StopReason::EndTurn)));

        let event = StreamEvent::Stop(StopReason::ToolUse);
        assert!(matches!(event, StreamEvent::Stop(StopReason::ToolUse)));
    }

    #[test]
    fn test_stream_event_done() {
        let usage = TokenUsage {
            input_tokens: 100,
            output_tokens: 50,
            cache_read_tokens: Some(25),
            cache_creation_tokens: None,
        };
        let event = StreamEvent::Done(usage.clone());
        if let StreamEvent::Done(u) = &event {
            assert_eq!(u.input_tokens, 100);
            assert_eq!(u.output_tokens, 50);
            assert_eq!(u.cache_read_tokens, Some(25));
            assert!(u.cache_creation_tokens.is_none());
        } else {
            panic!("Expected Done variant");
        }
    }

    #[test]
    fn test_stream_event_error() {
        let event = StreamEvent::Error("API rate limit exceeded".to_string());
        if let StreamEvent::Error(msg) = &event {
            assert_eq!(msg, "API rate limit exceeded");
        } else {
            panic!("Expected Error variant");
        }
    }

    #[test]
    fn test_stream_event_variants() {
        // Verify all variants can be constructed
        let _delta = StreamEvent::TextDelta("hello".to_string());
        let _tool = StreamEvent::ToolUse {
            id: "t1".to_string(),
            name: "read".to_string(),
            input: serde_json::json!({"path": "foo.rs"}),
        };
        let _block = StreamEvent::ContentBlock(ContentBlock::Text {
            text: "hi".to_string(),
        });
        let _stop = StreamEvent::Stop(StopReason::MaxTokens);
        let _done = StreamEvent::Done(TokenUsage::default());
        let _err = StreamEvent::Error("oops".to_string());
    }
}
