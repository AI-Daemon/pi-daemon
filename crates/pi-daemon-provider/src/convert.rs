//! Message conversion helpers between pi-daemon types and LLM API formats.

use pi_daemon_types::message::{ContentBlock, Message, MessageContent, Role};

/// Convert messages to Anthropic API format.
///
/// Returns `(system_prompt, messages)` — Anthropic takes system as a separate field.
pub fn to_anthropic_messages(
    messages: &[Message],
    system_prompt: Option<&str>,
) -> (Option<String>, Vec<serde_json::Value>) {
    let mut system = system_prompt.map(|s| s.to_string());
    let mut api_messages = Vec::new();

    for msg in messages {
        match msg.role {
            Role::System => {
                // Anthropic doesn't use system role in messages array.
                // Merge into the system prompt.
                if let Some(text) = extract_text_content(&msg.content) {
                    match &mut system {
                        Some(existing) => {
                            existing.push_str("\n\n");
                            existing.push_str(&text);
                        }
                        None => system = Some(text),
                    }
                }
            }
            Role::User | Role::Assistant => {
                let role = match msg.role {
                    Role::User => "user",
                    Role::Assistant => "assistant",
                    _ => unreachable!(),
                };
                let content = content_to_anthropic(&msg.content);
                api_messages.push(serde_json::json!({
                    "role": role,
                    "content": content,
                }));
            }
            Role::Tool => {
                // Tool results go as user messages in Anthropic format
                let content = content_to_anthropic(&msg.content);
                api_messages.push(serde_json::json!({
                    "role": "user",
                    "content": content,
                }));
            }
        }
    }

    (system, api_messages)
}

/// Convert messages to OpenAI API format.
pub fn to_openai_messages(
    messages: &[Message],
    system_prompt: Option<&str>,
) -> Vec<serde_json::Value> {
    let mut api_messages = Vec::new();

    if let Some(system) = system_prompt {
        api_messages.push(serde_json::json!({
            "role": "system",
            "content": system,
        }));
    }

    for msg in messages {
        let role = match msg.role {
            Role::System => "system",
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::Tool => "tool",
        };

        match &msg.content {
            MessageContent::Text(text) => {
                api_messages.push(serde_json::json!({
                    "role": role,
                    "content": text,
                }));
            }
            MessageContent::Blocks(blocks) => {
                if msg.role == Role::Tool {
                    // Each tool result becomes its own message
                    for block in blocks {
                        if let ContentBlock::ToolResult {
                            tool_use_id,
                            content,
                            ..
                        } = block
                        {
                            api_messages.push(serde_json::json!({
                                "role": "tool",
                                "tool_call_id": tool_use_id,
                                "content": content,
                            }));
                        }
                    }
                } else if msg.role == Role::Assistant {
                    // Assistant messages may have text + tool_calls
                    let mut text_parts = Vec::new();
                    let mut tool_calls = Vec::new();

                    for block in blocks {
                        match block {
                            ContentBlock::Text { text } => {
                                text_parts.push(text.clone());
                            }
                            ContentBlock::ToolUse { id, name, input } => {
                                tool_calls.push(serde_json::json!({
                                    "id": id,
                                    "type": "function",
                                    "function": {
                                        "name": name,
                                        "arguments": input.to_string(),
                                    }
                                }));
                            }
                            _ => {}
                        }
                    }

                    let mut obj = serde_json::json!({ "role": "assistant" });
                    if !text_parts.is_empty() {
                        obj["content"] = serde_json::Value::String(text_parts.join(""));
                    }
                    if !tool_calls.is_empty() {
                        obj["tool_calls"] = serde_json::Value::Array(tool_calls);
                    }

                    api_messages.push(obj);
                } else {
                    // User or system with blocks — concatenate text
                    let text: String = blocks
                        .iter()
                        .filter_map(|b| match b {
                            ContentBlock::Text { text } => Some(text.as_str()),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                        .join("");

                    api_messages.push(serde_json::json!({
                        "role": role,
                        "content": text,
                    }));
                }
            }
        }
    }

    api_messages
}

fn content_to_anthropic(content: &MessageContent) -> serde_json::Value {
    match content {
        MessageContent::Text(text) => serde_json::Value::String(text.clone()),
        MessageContent::Blocks(blocks) => {
            let api_blocks: Vec<serde_json::Value> = blocks
                .iter()
                .map(|block| match block {
                    ContentBlock::Text { text } => {
                        serde_json::json!({ "type": "text", "text": text })
                    }
                    ContentBlock::ToolUse { id, name, input } => {
                        serde_json::json!({
                            "type": "tool_use",
                            "id": id,
                            "name": name,
                            "input": input,
                        })
                    }
                    ContentBlock::ToolResult {
                        tool_use_id,
                        content,
                        is_error,
                    } => {
                        serde_json::json!({
                            "type": "tool_result",
                            "tool_use_id": tool_use_id,
                            "content": content,
                            "is_error": is_error,
                        })
                    }
                })
                .collect();
            serde_json::Value::Array(api_blocks)
        }
    }
}

fn extract_text_content(content: &MessageContent) -> Option<String> {
    match content {
        MessageContent::Text(text) => Some(text.clone()),
        MessageContent::Blocks(blocks) => {
            let texts: Vec<&str> = blocks
                .iter()
                .filter_map(|b| match b {
                    ContentBlock::Text { text } => Some(text.as_str()),
                    _ => None,
                })
                .collect();
            if texts.is_empty() {
                None
            } else {
                Some(texts.join(""))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pi_daemon_types::message::{ContentBlock, Message, MessageContent, Role};

    #[test]
    fn test_to_anthropic_system_extracted() {
        let messages = vec![
            Message {
                role: Role::System,
                content: MessageContent::Text("You are helpful.".to_string()),
            },
            Message {
                role: Role::User,
                content: MessageContent::Text("Hello".to_string()),
            },
        ];

        let (system, api_msgs) = to_anthropic_messages(&messages, None);
        assert_eq!(system.unwrap(), "You are helpful.");
        assert_eq!(api_msgs.len(), 1);
        assert_eq!(api_msgs[0]["role"], "user");
    }

    #[test]
    fn test_to_anthropic_system_merged_with_option() {
        let messages = vec![Message {
            role: Role::System,
            content: MessageContent::Text("Be concise.".to_string()),
        }];

        let (system, _) = to_anthropic_messages(&messages, Some("You are helpful."));
        let s = system.unwrap();
        assert!(s.contains("You are helpful."));
        assert!(s.contains("Be concise."));
    }

    #[test]
    fn test_to_anthropic_tool_result_as_user() {
        let messages = vec![Message {
            role: Role::Tool,
            content: MessageContent::Blocks(vec![ContentBlock::ToolResult {
                tool_use_id: "t1".to_string(),
                content: "result data".to_string(),
                is_error: false,
            }]),
        }];

        let (_, api_msgs) = to_anthropic_messages(&messages, None);
        assert_eq!(api_msgs.len(), 1);
        assert_eq!(api_msgs[0]["role"], "user");
    }

    #[test]
    fn test_to_openai_system_prepended() {
        let messages = vec![Message {
            role: Role::User,
            content: MessageContent::Text("Hi".to_string()),
        }];

        let api_msgs = to_openai_messages(&messages, Some("Be helpful"));
        assert_eq!(api_msgs.len(), 2);
        assert_eq!(api_msgs[0]["role"], "system");
        assert_eq!(api_msgs[0]["content"], "Be helpful");
        assert_eq!(api_msgs[1]["role"], "user");
    }

    #[test]
    fn test_to_openai_assistant_with_tool_calls() {
        let messages = vec![Message {
            role: Role::Assistant,
            content: MessageContent::Blocks(vec![
                ContentBlock::Text {
                    text: "Let me check.".to_string(),
                },
                ContentBlock::ToolUse {
                    id: "call_1".to_string(),
                    name: "get_weather".to_string(),
                    input: serde_json::json!({"city": "London"}),
                },
            ]),
        }];

        let api_msgs = to_openai_messages(&messages, None);
        assert_eq!(api_msgs.len(), 1);
        assert_eq!(api_msgs[0]["role"], "assistant");
        assert_eq!(api_msgs[0]["content"], "Let me check.");
        assert!(api_msgs[0]["tool_calls"].is_array());
    }

    #[test]
    fn test_to_openai_tool_result() {
        let messages = vec![Message {
            role: Role::Tool,
            content: MessageContent::Blocks(vec![ContentBlock::ToolResult {
                tool_use_id: "call_1".to_string(),
                content: "Sunny, 20C".to_string(),
                is_error: false,
            }]),
        }];

        let api_msgs = to_openai_messages(&messages, None);
        assert_eq!(api_msgs.len(), 1);
        assert_eq!(api_msgs[0]["role"], "tool");
        assert_eq!(api_msgs[0]["tool_call_id"], "call_1");
        assert_eq!(api_msgs[0]["content"], "Sunny, 20C");
    }
}
