//! OpenAI-compatible /v1/chat/completions endpoint
//!
//! Implements the OpenAI chat completions API so any OpenAI-compatible client
//! can connect to pi-daemon agents by pointing at the /v1/chat/completions endpoint.

use crate::state::AppState;
use axum::extract::State;
use axum::http::{header, StatusCode};
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use axum::response::IntoResponse;
use axum::Json;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::sync::Arc;
use uuid::Uuid;

// --- Request Types ---

/// OpenAI chat completion request.
#[derive(Debug, Deserialize)]
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<OaiMessage>,
    #[serde(default)]
    pub stream: bool,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub n: Option<u32>,
    pub stop: Option<OaiStop>,
    pub presence_penalty: Option<f32>,
    pub frequency_penalty: Option<f32>,
}

/// OpenAI message format.
#[derive(Debug, Deserialize)]
pub struct OaiMessage {
    pub role: String,
    #[serde(default)]
    pub content: OaiContent,
}

/// OpenAI content can be text, array of parts, or null.
#[derive(Debug, Deserialize, Default)]
#[serde(untagged)]
pub enum OaiContent {
    Text(String),
    Parts(Vec<OaiContentPart>),
    #[default]
    Null,
}

/// Content part for multimodal content.
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum OaiContentPart {
    #[serde(rename = "text")]
    Text { text: String },
    // Could add image_url, etc. in the future
}

/// Stop sequences can be string or array.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum OaiStop {
    String(String),
    Array(Vec<String>),
}

// --- Response Types (Non-Streaming) ---

/// OpenAI chat completion response.
#[derive(Debug, Serialize)]
pub struct ChatCompletionResponse {
    pub id: String,
    pub object: String,
    pub created: i64,
    pub model: String,
    pub choices: Vec<Choice>,
    pub usage: Usage,
}

/// Choice in completion response.
#[derive(Debug, Serialize)]
pub struct Choice {
    pub index: u32,
    pub message: ChoiceMessage,
    pub finish_reason: String,
}

/// Message in choice.
#[derive(Debug, Serialize)]
pub struct ChoiceMessage {
    pub role: String,
    pub content: String,
}

/// Token usage statistics.
#[derive(Debug, Serialize)]
pub struct Usage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

// --- Response Types (Streaming) ---

/// OpenAI streaming chunk.
#[derive(Debug, Serialize)]
pub struct ChatCompletionChunk {
    pub id: String,
    pub object: String,
    pub created: i64,
    pub model: String,
    pub choices: Vec<ChunkChoice>,
}

/// Choice in streaming chunk.
#[derive(Debug, Serialize)]
pub struct ChunkChoice {
    pub index: u32,
    pub delta: Delta,
    pub finish_reason: Option<String>,
}

/// Delta contains incremental changes.
#[derive(Debug, Serialize)]
pub struct Delta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
}

// --- Error Response ---

/// OpenAI-style error response.
#[derive(Debug, Serialize)]
pub struct ErrorResponse {
    pub error: ErrorDetails,
}

/// Error details.
#[derive(Debug, Serialize)]
pub struct ErrorDetails {
    pub message: String,
    pub r#type: String,
    pub param: Option<String>,
    pub code: Option<String>,
}

// --- Handler ---

/// POST /v1/chat/completions - OpenAI-compatible chat completions endpoint.
pub async fn chat_completions(
    State(state): State<Arc<AppState>>,
    Json(req): Json<ChatCompletionRequest>,
) -> impl IntoResponse {
    // Validate request
    if req.messages.is_empty() {
        return error_response(
            StatusCode::BAD_REQUEST,
            "invalid_request_error",
            "At least one message is required",
            Some("messages"),
        )
        .into_response();
    }

    // Generate completion ID and timestamp
    let completion_id = format!("chatcmpl-{}", Uuid::new_v4());
    let created = Utc::now().timestamp();

    // Resolve model to agent
    // For now, the model field is treated as an agent identifier
    let agent_identifier = req.model.clone();

    // Extract the most recent user message content
    let user_content = extract_user_content(&req.messages);

    if req.stream {
        // Streaming response using Server-Sent Events
        handle_streaming_request(
            completion_id,
            created,
            agent_identifier,
            user_content,
            state,
        )
        .await
        .into_response()
    } else {
        // Non-streaming response
        handle_non_streaming_request(
            completion_id,
            created,
            agent_identifier,
            user_content,
            state,
        )
        .await
        .into_response()
    }
}

/// Handle streaming chat completion request.
async fn handle_streaming_request(
    completion_id: String,
    created: i64,
    model: String,
    user_content: String,
    _state: Arc<AppState>,
) -> impl IntoResponse {
    let stream = async_stream::stream! {
        // First chunk: role
        let role_chunk = ChatCompletionChunk {
            id: completion_id.clone(),
            object: "chat.completion.chunk".to_string(),
            created,
            model: model.clone(),
            choices: vec![ChunkChoice {
                index: 0,
                delta: Delta {
                    role: Some("assistant".to_string()),
                    content: None,
                },
                finish_reason: None,
            }],
        };

        yield Ok::<_, Infallible>(SseEvent::default().data(
            serde_json::to_string(&role_chunk).unwrap()
        ));

        // TODO: Wire to actual LLM agent loop via WebSocket or direct agent communication
        // For now, provide an echo response in chunks to demonstrate streaming
        let response_text = format!("Echo from model '{model}': {user_content}");

        // Stream response in chunks
        for chunk in response_text.chars().collect::<Vec<_>>().chunks(3) {
            let chunk_text: String = chunk.iter().collect();

            let content_chunk = ChatCompletionChunk {
                id: completion_id.clone(),
                object: "chat.completion.chunk".to_string(),
                created,
                model: model.clone(),
                choices: vec![ChunkChoice {
                    index: 0,
                    delta: Delta {
                        role: None,
                        content: Some(chunk_text),
                    },
                    finish_reason: None,
                }],
            };

            yield Ok(SseEvent::default().data(
                serde_json::to_string(&content_chunk).unwrap()
            ));

            // Small delay to simulate real streaming
            tokio::time::sleep(std::time::Duration::from_millis(30)).await;
        }

        // Final chunk with finish reason
        let final_chunk = ChatCompletionChunk {
            id: completion_id.clone(),
            object: "chat.completion.chunk".to_string(),
            created,
            model: model.clone(),
            choices: vec![ChunkChoice {
                index: 0,
                delta: Delta {
                    role: None,
                    content: None,
                },
                finish_reason: Some("stop".to_string()),
            }],
        };

        yield Ok(SseEvent::default().data(
            serde_json::to_string(&final_chunk).unwrap()
        ));

        // End stream with [DONE]
        yield Ok(SseEvent::default().data("[DONE]"));
    };

    Sse::new(stream)
        .keep_alive(KeepAlive::default())
        .into_response()
}

/// Handle non-streaming chat completion request.
async fn handle_non_streaming_request(
    completion_id: String,
    created: i64,
    model: String,
    user_content: String,
    _state: Arc<AppState>,
) -> impl IntoResponse {
    // TODO: Wire to actual LLM agent loop
    // For now, provide an echo response
    let response_text = format!("Echo from model '{model}': {user_content}");

    let response = ChatCompletionResponse {
        id: completion_id,
        object: "chat.completion".to_string(),
        created,
        model,
        choices: vec![Choice {
            index: 0,
            message: ChoiceMessage {
                role: "assistant".to_string(),
                content: response_text.clone(),
            },
            finish_reason: "stop".to_string(),
        }],
        usage: Usage {
            prompt_tokens: estimate_tokens(&user_content),
            completion_tokens: estimate_tokens(&response_text),
            total_tokens: estimate_tokens(&user_content) + estimate_tokens(&response_text),
        },
    };

    Json(response).into_response()
}

/// Extract user content from messages array.
fn extract_user_content(messages: &[OaiMessage]) -> String {
    messages
        .iter()
        .rev()
        .find(|m| m.role == "user")
        .map(|m| match &m.content {
            OaiContent::Text(text) => text.clone(),
            OaiContent::Parts(parts) => parts
                .iter()
                .map(|part| match part {
                    OaiContentPart::Text { text } => text.clone(),
                })
                .collect::<Vec<_>>()
                .join("\n"),
            OaiContent::Null => String::new(),
        })
        .unwrap_or_default()
}

/// Simple token estimation (rough approximation: ~4 chars per token).
fn estimate_tokens(text: &str) -> u32 {
    (text.len() as f32 / 4.0).ceil() as u32
}

/// Create error response.
fn error_response(
    status: StatusCode,
    error_type: &str,
    message: &str,
    param: Option<&str>,
) -> impl IntoResponse {
    let error = ErrorResponse {
        error: ErrorDetails {
            message: message.to_string(),
            r#type: error_type.to_string(),
            param: param.map(String::from),
            code: None,
        },
    };

    (
        status,
        [(header::CONTENT_TYPE, "application/json")],
        Json(error),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_user_content_text() {
        let messages = vec![
            OaiMessage {
                role: "system".to_string(),
                content: OaiContent::Text("You are helpful".to_string()),
            },
            OaiMessage {
                role: "user".to_string(),
                content: OaiContent::Text("Hello world".to_string()),
            },
        ];

        assert_eq!(extract_user_content(&messages), "Hello world");
    }

    #[test]
    fn test_extract_user_content_parts() {
        let messages = vec![OaiMessage {
            role: "user".to_string(),
            content: OaiContent::Parts(vec![OaiContentPart::Text {
                text: "Hello from parts".to_string(),
            }]),
        }];

        assert_eq!(extract_user_content(&messages), "Hello from parts");
    }

    #[test]
    fn test_extract_user_content_empty() {
        let messages = vec![OaiMessage {
            role: "system".to_string(),
            content: OaiContent::Text("System message only".to_string()),
        }];

        assert_eq!(extract_user_content(&messages), "");
    }

    #[test]
    fn test_estimate_tokens() {
        assert_eq!(estimate_tokens(""), 0);
        assert_eq!(estimate_tokens("test"), 1); // 4 chars -> 1 token
        assert_eq!(estimate_tokens("hello world"), 3); // 11 chars -> 3 tokens
    }

    #[test]
    fn test_chat_completion_request_deserialization() {
        let json = r#"{
            "model": "test-model",
            "messages": [
                {"role": "user", "content": "Hello"}
            ],
            "stream": true,
            "max_tokens": 100
        }"#;

        let req: ChatCompletionRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.model, "test-model");
        assert_eq!(req.messages.len(), 1);
        assert!(req.stream);
        assert_eq!(req.max_tokens, Some(100));
    }

    #[test]
    fn test_chat_completion_response_serialization() {
        let response = ChatCompletionResponse {
            id: "chatcmpl-123".to_string(),
            object: "chat.completion".to_string(),
            created: 1700000000,
            model: "test".to_string(),
            choices: vec![Choice {
                index: 0,
                message: ChoiceMessage {
                    role: "assistant".to_string(),
                    content: "Hello!".to_string(),
                },
                finish_reason: "stop".to_string(),
            }],
            usage: Usage {
                prompt_tokens: 5,
                completion_tokens: 2,
                total_tokens: 7,
            },
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("chat.completion"));
        assert!(json.contains("Hello!"));
    }

    #[test]
    fn test_streaming_chunk_serialization() {
        let chunk = ChatCompletionChunk {
            id: "chatcmpl-123".to_string(),
            object: "chat.completion.chunk".to_string(),
            created: 1700000000,
            model: "test".to_string(),
            choices: vec![ChunkChoice {
                index: 0,
                delta: Delta {
                    role: Some("assistant".to_string()),
                    content: None,
                },
                finish_reason: None,
            }],
        };

        let json = serde_json::to_string(&chunk).unwrap();
        assert!(json.contains("chat.completion.chunk"));
        assert!(json.contains("assistant"));
        assert!(!json.contains("\"content\"")); // Should be omitted when None
    }
}
