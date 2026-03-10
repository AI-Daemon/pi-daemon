//! OpenAI-compatible API endpoints
//!
//! Implements the OpenAI API so any OpenAI-compatible client can connect to pi-daemon 
//! agents. Includes /v1/chat/completions and /v1/models endpoints.

use crate::state::AppState;
use axum::extract::State;
use axum::http::{header, StatusCode};
use axum::response::sse::{Event as SseEvent, KeepAlive, Sse};
use axum::response::IntoResponse;
use axum::Json;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::convert::Infallible;
use std::sync::Arc;
use tracing::debug;
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

// --- Models Response Types ---

/// OpenAI models list response.
#[derive(Debug, Serialize)]
pub struct ModelsResponse {
    pub object: String,
    pub data: Vec<ModelInfo>,
}

/// Information about a specific model.
#[derive(Debug, Serialize)]
pub struct ModelInfo {
    pub id: String,
    pub object: String,
    pub created: i64,
    pub owned_by: String,
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

/// GET /v1/models - OpenAI-compatible models list endpoint.
pub async fn models(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let models = discover_available_models(&state).await;
    debug!("Models endpoint: returning {} models", models.len());
    
    Json(ModelsResponse {
        object: "list".to_string(),
        data: models,
    })
}

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

/// Discover available models from multiple sources.
/// 
/// Note: This function performs in-memory operations only and is fast enough
/// that caching is not currently needed. If performance becomes an issue with
/// hundreds of agents, consider adding a TTL cache.
async fn discover_available_models(state: &AppState) -> Vec<ModelInfo> {
    let mut models = Vec::new();
    let mut seen_models = HashSet::new();
    let created_timestamp = Utc::now().timestamp();

    // 1. Add the default model from configuration
    let default_model = &state.config.default_model;
    if is_valid_model_name(default_model) && seen_models.insert(default_model.clone()) {
        models.push(ModelInfo {
            id: default_model.clone(),
            object: "model".to_string(),
            created: created_timestamp,
            owned_by: infer_model_owner(default_model),
        });
    }

    // 2. Add models from currently registered agents
    let registered_agents = state.kernel.registry.list();
    for agent in registered_agents {
        if let Some(model) = agent.model {
            if is_valid_model_name(&model) && seen_models.insert(model.clone()) {
                models.push(ModelInfo {
                    id: model.clone(),
                    object: "model".to_string(),
                    created: created_timestamp,
                    owned_by: infer_model_owner(&model),
                });
            }
        }
    }

    // 3. Add well-known models based on configured providers
    add_provider_models(state, &mut models, &mut seen_models, created_timestamp);

    models
}

/// Validate that a model name is reasonable for inclusion in the models list.
fn is_valid_model_name(model: &str) -> bool {
    // Check for empty, whitespace-only, or unreasonably long model names
    let trimmed = model.trim();
    !trimmed.is_empty() && trimmed.len() <= 256 && !trimmed.chars().all(|c| c.is_whitespace())
}

/// Infer the model owner/provider from the model name.
fn infer_model_owner(model_name: &str) -> String {
    let model_lower = model_name.to_lowercase();
    
    if model_lower.contains("claude") || model_lower.contains("anthropic") {
        "anthropic".to_string()
    } else if model_lower.contains("gpt") || model_lower.contains("openai") {
        "openai".to_string()
    } else if model_lower.contains("llama") || model_lower.contains("mistral") || model_lower.contains("codellama") {
        "meta".to_string()
    } else if model_lower.contains("gemini") || model_lower.contains("palm") {
        "google".to_string()
    } else if model_lower.contains("titan") {
        "amazon".to_string()
    } else {
        // For unknown models, try to extract owner from model name patterns like "company/model"
        if let Some(slash_idx) = model_name.find('/') {
            model_name[..slash_idx].to_string()
        } else {
            "unknown".to_string()
        }
    }
}

/// Add well-known models based on configured providers.
fn add_provider_models(
    state: &AppState,
    models: &mut Vec<ModelInfo>,
    seen_models: &mut HashSet<String>,
    created_timestamp: i64,
) {
    let providers = &state.config.providers;

    // Anthropic models if API key is configured
    if !providers.anthropic_api_key.is_empty() {
        let anthropic_models = vec![
            "claude-3-5-sonnet-20241022",
            "claude-3-5-haiku-20241022", 
            "claude-3-opus-20240229",
            "claude-3-sonnet-20240229",
            "claude-3-haiku-20240307",
        ];
        
        for model in anthropic_models {
            if seen_models.insert(model.to_string()) {
                models.push(ModelInfo {
                    id: model.to_string(),
                    object: "model".to_string(),
                    created: created_timestamp,
                    owned_by: "anthropic".to_string(),
                });
            }
        }
    }

    // OpenAI models if API key is configured
    if !providers.openai_api_key.is_empty() {
        let openai_models = vec![
            "gpt-4o",
            "gpt-4o-mini", 
            "gpt-4-turbo",
            "gpt-4",
            "gpt-3.5-turbo",
        ];
        
        for model in openai_models {
            if seen_models.insert(model.to_string()) {
                models.push(ModelInfo {
                    id: model.to_string(),
                    object: "model".to_string(),
                    created: created_timestamp,
                    owned_by: "openai".to_string(),
                });
            }
        }
    }

    // Note: We could add more providers (Ollama local models discovery, 
    // OpenRouter model listing, etc.) in the future by making HTTP calls
    // to their respective APIs, but for now we'll stick to well-known models
    // to keep the implementation performant and reliable.
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

    #[test]
    fn test_models_response_serialization() {
        let response = ModelsResponse {
            object: "list".to_string(),
            data: vec![
                ModelInfo {
                    id: "claude-3-5-sonnet-20241022".to_string(),
                    object: "model".to_string(),
                    created: 1700000000,
                    owned_by: "anthropic".to_string(),
                },
                ModelInfo {
                    id: "gpt-4".to_string(),
                    object: "model".to_string(), 
                    created: 1700000000,
                    owned_by: "openai".to_string(),
                },
            ],
        };

        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains("\"object\":\"list\""));
        assert!(json.contains("claude-3-5-sonnet-20241022"));
        assert!(json.contains("gpt-4"));
        assert!(json.contains("anthropic"));
        assert!(json.contains("openai"));
    }

    #[test]
    fn test_is_valid_model_name() {
        // Valid model names
        assert!(is_valid_model_name("gpt-4"));
        assert!(is_valid_model_name("claude-3-sonnet"));
        assert!(is_valid_model_name("company/model-name"));
        assert!(is_valid_model_name("a")); // Single character is ok
        
        // Invalid model names
        assert!(!is_valid_model_name("")); // Empty
        assert!(!is_valid_model_name("   ")); // Whitespace only
        assert!(!is_valid_model_name("\t\n  ")); // Various whitespace
        assert!(!is_valid_model_name(&"x".repeat(257))); // Too long (>256 chars)
        
        // Edge cases
        assert!(is_valid_model_name(" valid-model ")); // Trimmed, so valid
        assert!(is_valid_model_name("model-with-123")); // Numbers ok
        assert!(is_valid_model_name("model.with.dots")); // Dots ok
    }

    #[test]
    fn test_infer_model_owner() {
        assert_eq!(infer_model_owner("claude-3-sonnet"), "anthropic");
        assert_eq!(infer_model_owner("gpt-4"), "openai");
        assert_eq!(infer_model_owner("llama-2-7b"), "meta");
        assert_eq!(infer_model_owner("gemini-pro"), "google");
        assert_eq!(infer_model_owner("anthropic/claude-3-opus"), "anthropic");
        assert_eq!(infer_model_owner("openai/gpt-4"), "openai");
        assert_eq!(infer_model_owner("custom-company/special-model"), "custom-company");
        assert_eq!(infer_model_owner("unknown-model"), "unknown");
    }
}
