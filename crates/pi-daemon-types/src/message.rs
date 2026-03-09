use serde::{Deserialize, Serialize};

/// Message role in a conversation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
    Tool,
}

/// A content block within a message.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        content: String,
        is_error: bool,
    },
}

/// The content of a message — either a simple string or structured blocks.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    Text(String),
    Blocks(Vec<ContentBlock>),
}

/// A conversation message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: MessageContent,
}

/// Why the model stopped generating.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopReason {
    EndTurn,
    MaxTokens,
    ToolUse,
    StopSequence,
}

/// Token usage for a completion.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub cache_read_tokens: Option<u32>,
    pub cache_creation_tokens: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_role_serialization() {
        let role = Role::User;
        let json = serde_json::to_string(&role).unwrap();
        assert_eq!(json, "\"user\"");

        let roundtrip: Role = serde_json::from_str(&json).unwrap();
        assert_eq!(role, roundtrip);
    }

    #[test]
    fn test_content_block_text_serialization() {
        let block = ContentBlock::Text {
            text: "Hello world".to_string(),
        };
        let json = serde_json::to_string(&block).unwrap();
        let roundtrip: ContentBlock = serde_json::from_str(&json).unwrap();

        if let ContentBlock::Text { text } = roundtrip {
            assert_eq!(text, "Hello world");
        } else {
            panic!("Expected Text variant");
        }
    }

    #[test]
    fn test_content_block_tool_use_serialization() {
        let block = ContentBlock::ToolUse {
            id: "tool_123".to_string(),
            name: "get_weather".to_string(),
            input: serde_json::json!({"location": "London"}),
        };

        let json = serde_json::to_string(&block).unwrap();
        let roundtrip: ContentBlock = serde_json::from_str(&json).unwrap();

        if let ContentBlock::ToolUse { id, name, input } = roundtrip {
            assert_eq!(id, "tool_123");
            assert_eq!(name, "get_weather");
            assert_eq!(input["location"], "London");
        } else {
            panic!("Expected ToolUse variant");
        }
    }

    #[test]
    fn test_message_content_text_untagged() {
        // Test that simple string deserializes as Text variant
        let json = "\"Hello world\"";
        let content: MessageContent = serde_json::from_str(json).unwrap();

        if let MessageContent::Text(text) = content {
            assert_eq!(text, "Hello world");
        } else {
            panic!("Expected Text variant");
        }
    }

    #[test]
    fn test_message_content_blocks_untagged() {
        // Test that array deserializes as Blocks variant
        let json = r#"[{"type": "text", "text": "Hello"}]"#;
        let content: MessageContent = serde_json::from_str(json).unwrap();

        if let MessageContent::Blocks(blocks) = content {
            assert_eq!(blocks.len(), 1);
            if let ContentBlock::Text { text } = &blocks[0] {
                assert_eq!(text, "Hello");
            } else {
                panic!("Expected Text block");
            }
        } else {
            panic!("Expected Blocks variant");
        }
    }

    #[test]
    fn test_message_serialization() {
        let message = Message {
            role: Role::Assistant,
            content: MessageContent::Text("Hello!".to_string()),
        };

        let json = serde_json::to_string(&message).unwrap();
        let roundtrip: Message = serde_json::from_str(&json).unwrap();

        assert_eq!(message.role, roundtrip.role);
        if let (MessageContent::Text(orig), MessageContent::Text(round)) =
            (message.content, roundtrip.content)
        {
            assert_eq!(orig, round);
        } else {
            panic!("Content mismatch");
        }
    }

    #[test]
    fn test_stop_reason_serialization() {
        let reason = StopReason::ToolUse;
        let json = serde_json::to_string(&reason).unwrap();
        assert_eq!(json, "\"tool_use\"");

        let roundtrip: StopReason = serde_json::from_str(&json).unwrap();
        assert_eq!(reason, roundtrip);
    }

    #[test]
    fn test_token_usage_default() {
        let usage = TokenUsage::default();
        assert_eq!(usage.input_tokens, 0);
        assert_eq!(usage.output_tokens, 0);
        assert_eq!(usage.cache_read_tokens, None);
        assert_eq!(usage.cache_creation_tokens, None);
    }

    #[test]
    fn test_token_usage_serialization() {
        let usage = TokenUsage {
            input_tokens: 100,
            output_tokens: 50,
            cache_read_tokens: Some(25),
            cache_creation_tokens: None,
        };

        let json = serde_json::to_string(&usage).unwrap();
        let roundtrip: TokenUsage = serde_json::from_str(&json).unwrap();

        assert_eq!(usage.input_tokens, roundtrip.input_tokens);
        assert_eq!(usage.output_tokens, roundtrip.output_tokens);
        assert_eq!(usage.cache_read_tokens, roundtrip.cache_read_tokens);
        assert_eq!(usage.cache_creation_tokens, roundtrip.cache_creation_tokens);
    }
}
