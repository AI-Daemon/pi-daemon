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
use pi_daemon_provider::{CompletionOptions, Provider, StreamEvent};
use pi_daemon_types::message::{Message, MessageContent, Role};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::convert::Infallible;
use std::sync::Arc;
use tokio_stream::StreamExt;
use tracing::{debug, error};
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
    // Validate request: messages must be non-empty
    if req.messages.is_empty() {
        return error_response(
            StatusCode::BAD_REQUEST,
            "invalid_request_error",
            "At least one message is required",
            Some("messages"),
        )
        .into_response();
    }

    // Validate parameter ranges
    if let Some(temp) = req.temperature {
        if !(0.0..=2.0).contains(&temp) {
            return error_response(
                StatusCode::BAD_REQUEST,
                "invalid_request_error",
                "temperature must be between 0 and 2",
                Some("temperature"),
            )
            .into_response();
        }
    }
    if let Some(top_p) = req.top_p {
        if !(0.0..=1.0).contains(&top_p) {
            return error_response(
                StatusCode::BAD_REQUEST,
                "invalid_request_error",
                "top_p must be between 0 and 1",
                Some("top_p"),
            )
            .into_response();
        }
    }
    if let Some(max_tokens) = req.max_tokens {
        if max_tokens == 0 {
            return error_response(
                StatusCode::BAD_REQUEST,
                "invalid_request_error",
                "max_tokens must be greater than 0",
                Some("max_tokens"),
            )
            .into_response();
        }
    }

    // Check that a provider is available
    let provider = match &state.provider {
        Some(p) => p.as_ref(),
        None => {
            return error_response(
                StatusCode::BAD_REQUEST,
                "invalid_request_error",
                &format!("Model '{}' is not available: no LLM provider configured", req.model),
                Some("model"),
            )
            .into_response();
        }
    };

    // Generate completion ID and timestamp
    let completion_id = format!("chatcmpl-{}", Uuid::new_v4());
    let created = Utc::now().timestamp();

    // Convert OpenAI messages to provider format (full conversation context)
    let (system_prompt, messages) = convert_oai_messages(&req.messages);

    // Build completion options
    let mut options = CompletionOptions {
        system_prompt,
        ..Default::default()
    };
    if let Some(max_tokens) = req.max_tokens {
        options.max_tokens = max_tokens;
    }
    if let Some(temp) = req.temperature {
        options.temperature = Some(temp as f64);
    }
    if let Some(top_p) = req.top_p {
        options.top_p = Some(top_p as f64);
    }
    if let Some(stop) = &req.stop {
        options.stop_sequences = match stop {
            OaiStop::String(s) => vec![s.clone()],
            OaiStop::Array(a) => a.clone(),
        };
    }

    let model = req.model.clone();

    if req.stream {
        handle_streaming_request(completion_id, created, model, messages, options, provider)
            .await
            .into_response()
    } else {
        handle_non_streaming_request(completion_id, created, model, messages, options, provider)
            .await
            .into_response()
    }
}

/// Handle streaming chat completion request.
async fn handle_streaming_request(
    completion_id: String,
    created: i64,
    model: String,
    messages: Vec<Message>,
    options: CompletionOptions,
    provider: &dyn Provider,
) -> impl IntoResponse {
    // Start the provider stream before entering the SSE generator.
    // If the provider call itself fails, return an HTTP error immediately.
    let provider_stream = match provider.complete(&model, messages, options).await {
        Ok(s) => s,
        Err(e) => {
            error!("Provider error for model '{}': {}", model, e);
            return error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "server_error",
                &format!("LLM provider error: {e}"),
                None,
            )
            .into_response();
        }
    };

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

        // Stream real provider events
        let mut provider_stream = provider_stream;
        while let Some(event) = provider_stream.next().await {
            match event {
                StreamEvent::TextDelta(text) => {
                    let content_chunk = ChatCompletionChunk {
                        id: completion_id.clone(),
                        object: "chat.completion.chunk".to_string(),
                        created,
                        model: model.clone(),
                        choices: vec![ChunkChoice {
                            index: 0,
                            delta: Delta {
                                role: None,
                                content: Some(text),
                            },
                            finish_reason: None,
                        }],
                    };

                    yield Ok(SseEvent::default().data(
                        serde_json::to_string(&content_chunk).unwrap()
                    ));
                }
                StreamEvent::Stop(_) => {
                    // Emit final chunk with finish reason
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
                }
                StreamEvent::Done(_usage) => {
                    // Usage arrives in Done event; streaming format doesn't include it
                    // in individual chunks per OpenAI spec. Just terminate.
                }
                StreamEvent::Error(msg) => {
                    error!("Stream error: {}", msg);
                    // Can't change HTTP status mid-stream, just stop
                    break;
                }
                // ToolUse, ContentBlock — not relevant for chat completions text streaming
                _ => {}
            }
        }

        // End stream with [DONE]
        yield Ok(SseEvent::default().data("[DONE]"));
    };

    // SSE response with proper headers for proxies/buffering (fixes #211)
    (
        [
            (header::CACHE_CONTROL, "no-cache"),
            (
                header::HeaderName::from_static("x-accel-buffering"),
                "no",
            ),
        ],
        Sse::new(stream).keep_alive(KeepAlive::default()),
    )
        .into_response()
}

/// Handle non-streaming chat completion request.
async fn handle_non_streaming_request(
    completion_id: String,
    created: i64,
    model: String,
    messages: Vec<Message>,
    options: CompletionOptions,
    provider: &dyn Provider,
) -> impl IntoResponse {
    // Call the LLM provider
    let mut stream = match provider.complete(&model, messages, options).await {
        Ok(s) => s,
        Err(e) => {
            error!("Provider error for model '{}': {}", model, e);
            return error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "server_error",
                &format!("LLM provider error: {e}"),
                None,
            )
            .into_response();
        }
    };

    // Collect the full response from the stream
    let mut response_text = String::new();
    let mut usage = Usage {
        prompt_tokens: 0,
        completion_tokens: 0,
        total_tokens: 0,
    };

    while let Some(event) = stream.next().await {
        match event {
            StreamEvent::TextDelta(text) => {
                response_text.push_str(&text);
            }
            StreamEvent::Done(token_usage) => {
                usage.prompt_tokens = token_usage.input_tokens;
                usage.completion_tokens = token_usage.output_tokens;
                usage.total_tokens = token_usage.input_tokens + token_usage.output_tokens;
            }
            StreamEvent::Error(msg) => {
                error!("Provider stream error: {}", msg);
                return error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "server_error",
                    &format!("LLM provider error: {msg}"),
                    None,
                )
                .into_response();
            }
            _ => {}
        }
    }

    let response = ChatCompletionResponse {
        id: completion_id,
        object: "chat.completion".to_string(),
        created,
        model,
        choices: vec![Choice {
            index: 0,
            message: ChoiceMessage {
                role: "assistant".to_string(),
                content: response_text,
            },
            finish_reason: "stop".to_string(),
        }],
        usage,
    };

    Json(response).into_response()
}

// --- Message Conversion ---

/// Convert OpenAI-format messages to provider Message format.
///
/// Returns `(system_prompt, messages)`. System messages are extracted as a
/// separate system prompt (required by Anthropic), and all other messages
/// are converted to the provider's `Message` type with full conversation context.
fn convert_oai_messages(oai_messages: &[OaiMessage]) -> (Option<String>, Vec<Message>) {
    let mut system_parts = Vec::new();
    let mut messages = Vec::new();

    for msg in oai_messages {
        let content_text = extract_message_content(&msg.content);

        match msg.role.as_str() {
            "system" => {
                system_parts.push(content_text);
            }
            "user" => {
                messages.push(Message {
                    role: Role::User,
                    content: MessageContent::Text(content_text),
                });
            }
            "assistant" => {
                messages.push(Message {
                    role: Role::Assistant,
                    content: MessageContent::Text(content_text),
                });
            }
            "tool" => {
                messages.push(Message {
                    role: Role::Tool,
                    content: MessageContent::Text(content_text),
                });
            }
            _ => {
                // Unknown role — treat as user
                debug!("Unknown message role '{}', treating as user", msg.role);
                messages.push(Message {
                    role: Role::User,
                    content: MessageContent::Text(content_text),
                });
            }
        }
    }

    let system_prompt = if system_parts.is_empty() {
        None
    } else {
        Some(system_parts.join("\n\n"))
    };

    (system_prompt, messages)
}

/// Extract text content from an OaiContent value.
fn extract_message_content(content: &OaiContent) -> String {
    match content {
        OaiContent::Text(text) => text.clone(),
        OaiContent::Parts(parts) => parts
            .iter()
            .map(|part| match part {
                OaiContentPart::Text { text } => text.clone(),
            })
            .collect::<Vec<_>>()
            .join("\n"),
        OaiContent::Null => String::new(),
    }
}

// --- Models Discovery (unchanged) ---

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
    } else if model_lower.contains("llama")
        || model_lower.contains("mistral")
        || model_lower.contains("codellama")
    {
        "meta".to_string()
    } else if model_lower.contains("gemini") || model_lower.contains("palm") {
        "google".to_string()
    } else if model_lower.contains("titan") {
        "amazon".to_string()
    } else {
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
}

/// Create error response in OpenAI format.
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
    fn test_convert_oai_messages_full_conversation() {
        let oai = vec![
            OaiMessage {
                role: "system".to_string(),
                content: OaiContent::Text("You are helpful.".to_string()),
            },
            OaiMessage {
                role: "user".to_string(),
                content: OaiContent::Text("Hello".to_string()),
            },
            OaiMessage {
                role: "assistant".to_string(),
                content: OaiContent::Text("Hi there!".to_string()),
            },
            OaiMessage {
                role: "user".to_string(),
                content: OaiContent::Text("How are you?".to_string()),
            },
        ];

        let (system, messages) = convert_oai_messages(&oai);
        assert_eq!(system.unwrap(), "You are helpful.");
        assert_eq!(messages.len(), 3); // user, assistant, user (no system)
        assert_eq!(messages[0].role, Role::User);
        assert_eq!(messages[1].role, Role::Assistant);
        assert_eq!(messages[2].role, Role::User);
    }

    #[test]
    fn test_convert_oai_messages_multiple_system() {
        let oai = vec![
            OaiMessage {
                role: "system".to_string(),
                content: OaiContent::Text("Rule 1".to_string()),
            },
            OaiMessage {
                role: "system".to_string(),
                content: OaiContent::Text("Rule 2".to_string()),
            },
            OaiMessage {
                role: "user".to_string(),
                content: OaiContent::Text("Hello".to_string()),
            },
        ];

        let (system, messages) = convert_oai_messages(&oai);
        assert_eq!(system.unwrap(), "Rule 1\n\nRule 2");
        assert_eq!(messages.len(), 1);
    }

    #[test]
    fn test_convert_oai_messages_no_system() {
        let oai = vec![OaiMessage {
            role: "user".to_string(),
            content: OaiContent::Text("Hello".to_string()),
        }];

        let (system, messages) = convert_oai_messages(&oai);
        assert!(system.is_none());
        assert_eq!(messages.len(), 1);
    }

    #[test]
    fn test_convert_oai_messages_parts_content() {
        let oai = vec![OaiMessage {
            role: "user".to_string(),
            content: OaiContent::Parts(vec![
                OaiContentPart::Text {
                    text: "Part 1".to_string(),
                },
                OaiContentPart::Text {
                    text: "Part 2".to_string(),
                },
            ]),
        }];

        let (_, messages) = convert_oai_messages(&oai);
        if let MessageContent::Text(text) = &messages[0].content {
            assert_eq!(text, "Part 1\nPart 2");
        } else {
            panic!("Expected Text content");
        }
    }

    #[test]
    fn test_extract_message_content_null() {
        let content = OaiContent::Null;
        assert_eq!(extract_message_content(&content), "");
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
        assert!(is_valid_model_name("gpt-4"));
        assert!(is_valid_model_name("claude-3-sonnet"));
        assert!(is_valid_model_name("company/model-name"));
        assert!(is_valid_model_name("a"));

        assert!(!is_valid_model_name(""));
        assert!(!is_valid_model_name("   "));
        assert!(!is_valid_model_name("\t\n  "));
        assert!(!is_valid_model_name(&"x".repeat(257)));

        assert!(is_valid_model_name(" valid-model "));
        assert!(is_valid_model_name("model-with-123"));
        assert!(is_valid_model_name("model.with.dots"));
    }

    #[test]
    fn test_infer_model_owner() {
        assert_eq!(infer_model_owner("claude-3-sonnet"), "anthropic");
        assert_eq!(infer_model_owner("gpt-4"), "openai");
        assert_eq!(infer_model_owner("llama-2-7b"), "meta");
        assert_eq!(infer_model_owner("gemini-pro"), "google");
        assert_eq!(infer_model_owner("anthropic/claude-3-opus"), "anthropic");
        assert_eq!(infer_model_owner("openai/gpt-4"), "openai");
        assert_eq!(
            infer_model_owner("custom-company/special-model"),
            "custom-company"
        );
        assert_eq!(infer_model_owner("unknown-model"), "unknown");
    }
}
