//! WebSocket streaming chat handler
//!
//! Real-time bidirectional WebSocket connection between clients and agents.
//! Messages stream token-by-token from LLMs with typing indicators and tool updates.

use crate::state::AppState;
use axum::extract::ws::{Message, WebSocket};
use axum::extract::{ConnectInfo, Path, Query, State, WebSocketUpgrade};
use axum::response::IntoResponse;
use dashmap::DashMap;
use futures::{SinkExt, StreamExt};
use pi_daemon_types::agent::AgentEntry;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, info, warn};

/// Max WebSocket connections per IP address.
const MAX_WS_PER_IP: usize = 5;

/// Idle timeout — close connection after 30 min of no client messages.
const WS_IDLE_TIMEOUT: Duration = Duration::from_secs(30 * 60);

/// Read timeout — close connection if no data received within this period.
/// This catches abrupt disconnects where the TCP connection drops without
/// a WebSocket close frame.
const WS_READ_TIMEOUT: Duration = Duration::from_secs(60);

/// Text delta debounce: flush buffer after this many ms.
const DEBOUNCE_MS: u64 = 100;

/// Text delta debounce: flush buffer when it exceeds this many chars.
const DEBOUNCE_CHARS: usize = 200;

/// Interval for sending WebSocket ping frames to detect dead connections.
const WS_PING_INTERVAL: Duration = Duration::from_secs(15);

// --- Protocol types ---

/// Messages from client to server.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
pub enum ClientMessage {
    /// Send a chat message.
    Message { content: String },
    /// Set the LLM model for this session.
    SetModel { model: String },
    /// Ping (keepalive).
    Ping,
}

/// Messages from server to client.
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
pub enum ServerMessage {
    /// Agent is thinking or using tools.
    Typing {
        state: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        tool_name: Option<String>,
    },
    /// Streaming text delta (partial response).
    TextDelta { content: String },
    /// Complete response (sent after all deltas).
    Response {
        content: String,
        input_tokens: u32,
        output_tokens: u32,
    },
    /// Error message.
    Error { content: String },
    /// Agent list updated (broadcast when agents register/unregister).
    AgentsUpdated { agents: Vec<AgentEntry> },
    /// Pong (response to ping).
    Pong,
}

/// Query params for WebSocket auth (when API key is configured).
#[derive(Deserialize)]
pub struct WsAuthQuery {
    pub api_key: Option<String>,
}

/// Per-IP connection counter. Shared across all WebSocket upgrades.
pub type ConnectionTracker = Arc<DashMap<std::net::IpAddr, usize>>;

/// Create a new connection tracker (store in AppState).
pub fn new_connection_tracker() -> ConnectionTracker {
    Arc::new(DashMap::new())
}

/// RAII guard that decrements the per-IP connection count when dropped.
/// This ensures cleanup happens on all exit paths: normal close, error,
/// panic, or task cancellation.
struct ConnectionGuard {
    tracker: ConnectionTracker,
    ip: std::net::IpAddr,
}

impl ConnectionGuard {
    fn new(tracker: ConnectionTracker, ip: std::net::IpAddr) -> Self {
        Self { tracker, ip }
    }
}

impl Drop for ConnectionGuard {
    fn drop(&mut self) {
        if let Some(mut entry) = self.tracker.get_mut(&self.ip) {
            *entry = entry.saturating_sub(1);
            if *entry == 0 {
                drop(entry); // Release the lock before removing
                self.tracker.remove(&self.ip);
            }
        }
        info!(ip = %self.ip, "WebSocket connection guard dropped — connection count decremented");
    }
}

/// WebSocket upgrade handler.
///
/// Route: GET /ws/:agent_id?api_key=xxx
pub async fn ws_upgrade(
    ws: WebSocketUpgrade,
    Path(agent_id): Path<String>,
    Query(auth): Query<WsAuthQuery>,
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
) -> impl IntoResponse {
    // Auth check
    if !state.config.api_key.is_empty() {
        let key_ok = auth.api_key.as_deref() == Some(&state.config.api_key);
        if !key_ok {
            return (axum::http::StatusCode::UNAUTHORIZED, "Invalid API key").into_response();
        }
    }

    // Per-IP connection limit check
    let ip = addr.ip();
    let mut connections = state.connection_tracker.entry(ip).or_insert(0);

    if *connections >= MAX_WS_PER_IP {
        warn!(ip = %ip, current = *connections, max = MAX_WS_PER_IP, "WebSocket connection limit exceeded");
        return (
            axum::http::StatusCode::TOO_MANY_REQUESTS,
            "Too many WebSocket connections from this IP",
        )
            .into_response();
    }

    // Increment connection count
    *connections += 1;
    let connection_count = *connections;
    drop(connections); // Release the lock

    info!(ip = %ip, agent_id = %agent_id, connections = connection_count, "WebSocket connection starting");

    // Create the RAII guard — this will decrement the count when dropped,
    // regardless of how the handler exits (normal, error, panic, etc.)
    let guard = ConnectionGuard::new(state.connection_tracker.clone(), ip);

    ws.on_upgrade(move |socket| async move {
        // Move the guard into this future — it will be dropped when the future completes
        let _guard = guard;
        handle_websocket(socket, agent_id, state, addr).await;
        info!(ip = %ip, "WebSocket connection cleaned up");
    })
    .into_response()
}

/// Handle an established WebSocket connection.
async fn handle_websocket(
    socket: WebSocket,
    agent_id: String,
    state: Arc<AppState>,
    addr: SocketAddr,
) {
    let (mut sender, mut receiver) = socket.split();

    info!(addr = %addr, agent_id = %agent_id, "WebSocket connected");

    // Text delta buffer for debouncing
    let mut text_buffer = TextDeltaBuffer::new();
    let mut flush_interval = tokio::time::interval(Duration::from_secs(1));

    // Idle timeout: pinned sleep that resets when the client sends a message
    let idle_deadline = tokio::time::sleep(WS_IDLE_TIMEOUT);
    tokio::pin!(idle_deadline);

    // Server-initiated ping interval to detect dead connections
    let mut ping_interval = tokio::time::interval(WS_PING_INTERVAL);
    // The first tick completes immediately; skip it so we don't ping right away
    ping_interval.tick().await;

    // Track when we last heard *anything* from the client (data or pong)
    let mut last_client_activity = tokio::time::Instant::now();

    loop {
        tokio::select! {
            // Receive from client
            msg = receiver.next() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        // Reset idle timeout on any client message
                        idle_deadline.as_mut().reset(tokio::time::Instant::now() + WS_IDLE_TIMEOUT);
                        last_client_activity = tokio::time::Instant::now();

                        if let Err(e) = handle_client_message(&text, &mut sender, &state, &agent_id).await {
                            warn!(error = %e, "Error handling client message");
                        }
                    }
                    Some(Ok(Message::Pong(_))) => {
                        // Client responded to our ping — connection is alive
                        last_client_activity = tokio::time::Instant::now();
                        debug!(addr = %addr, "Received pong from client");
                    }
                    Some(Ok(Message::Ping(data))) => {
                        // Client sent a ping — respond with pong (axum may auto-reply,
                        // but explicit handling is safer)
                        last_client_activity = tokio::time::Instant::now();
                        let _ = sender.send(Message::Pong(data)).await;
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        info!(addr = %addr, "WebSocket disconnected by client");
                        break;
                    }
                    Some(Ok(msg)) => {
                        debug!(message = ?msg, "Received non-text WebSocket message");
                        last_client_activity = tokio::time::Instant::now();
                    }
                    Some(Err(e)) => {
                        warn!(error = %e, addr = %addr, "WebSocket receive error — closing connection");
                        break;
                    }
                }
            }

            // Text buffer flush timer
            _ = flush_interval.tick() => {
                if text_buffer.should_flush() {
                    if let Some(content) = text_buffer.try_flush() {
                        let msg = ServerMessage::TextDelta { content };
                        if let Ok(json) = serde_json::to_string(&msg) {
                            let _ = sender.send(Message::Text(json.into())).await;
                        }
                    }
                }
            }

            // Server-initiated ping to detect dead connections
            _ = ping_interval.tick() => {
                // Check if we've heard from the client recently
                if last_client_activity.elapsed() > WS_READ_TIMEOUT {
                    warn!(
                        addr = %addr,
                        elapsed_secs = last_client_activity.elapsed().as_secs(),
                        "WebSocket read timeout — no client activity, closing connection"
                    );
                    let _ = sender.send(Message::Close(Some(axum::extract::ws::CloseFrame {
                        code: axum::extract::ws::close_code::NORMAL,
                        reason: "Read timeout — no client activity".into(),
                    }))).await;
                    break;
                }

                // Send a ping frame to probe the connection
                if let Err(e) = sender.send(Message::Ping(vec![].into())).await {
                    warn!(error = %e, addr = %addr, "Failed to send WebSocket ping — closing connection");
                    break;
                }
                debug!(addr = %addr, "Sent ping to WebSocket client");
            }

            // Global idle timeout (no client messages for 30 min)
            _ = &mut idle_deadline => {
                info!(addr = %addr, "WebSocket idle timeout — no messages for 30 minutes");
                let _ = sender.send(Message::Close(Some(axum::extract::ws::CloseFrame {
                    code: axum::extract::ws::close_code::NORMAL,
                    reason: "Idle timeout".into(),
                }))).await;
                break;
            }
        }
    }
}

/// Handle a client message and send appropriate response.
async fn handle_client_message(
    text: &str,
    sender: &mut futures::stream::SplitSink<WebSocket, Message>,
    _state: &AppState,
    agent_id: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let client_msg: ClientMessage = serde_json::from_str(text).map_err(|e| {
        warn!(error = %e, text = %text, "Invalid WebSocket message format");
        format!("Invalid message format: {e}")
    })?;

    match client_msg {
        ClientMessage::Message { content } => {
            // TODO: Route to LLM agent loop (Phase 1.8 / bridge)
            // For now, echo back with typing indicators

            // Send typing start
            let typing_msg = ServerMessage::Typing {
                state: "start".to_string(),
                tool_name: None,
            };
            let json = serde_json::to_string(&typing_msg)?;
            sender.send(Message::Text(json.into())).await?;

            // Simulate processing delay
            tokio::time::sleep(Duration::from_millis(500)).await;

            // Send typing stop
            let typing_msg = ServerMessage::Typing {
                state: "stop".to_string(),
                tool_name: None,
            };
            let json = serde_json::to_string(&typing_msg)?;
            sender.send(Message::Text(json.into())).await?;

            // Send response
            let resp = ServerMessage::Response {
                content: format!("Echo from agent {agent_id}: {content}"),
                input_tokens: content.split_whitespace().count() as u32,
                output_tokens: (content.split_whitespace().count() + 5) as u32,
            };
            let json = serde_json::to_string(&resp)?;
            sender.send(Message::Text(json.into())).await?;
        }

        ClientMessage::SetModel { model } => {
            debug!(model = %model, agent_id = %agent_id, "Model set for WebSocket session");
            // TODO: Store model preference for this session
        }

        ClientMessage::Ping => {
            let pong = ServerMessage::Pong;
            let json = serde_json::to_string(&pong)?;
            sender.send(Message::Text(json.into())).await?;
        }
    }

    Ok(())
}

/// Accumulates text deltas and flushes when buffer exceeds size or time limits.
pub struct TextDeltaBuffer {
    buffer: String,
    last_push: tokio::time::Instant,
}

impl Default for TextDeltaBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl TextDeltaBuffer {
    /// Create a new empty buffer.
    pub fn new() -> Self {
        Self {
            buffer: String::new(),
            last_push: tokio::time::Instant::now(),
        }
    }

    /// Push text into buffer. Returns Some(chunk) if ready to flush immediately.
    pub fn push(&mut self, text: &str) -> Option<String> {
        self.buffer.push_str(text);
        self.last_push = tokio::time::Instant::now();

        if self.buffer.len() >= DEBOUNCE_CHARS {
            Some(self.flush())
        } else {
            None
        }
    }

    /// Check if buffer should be flushed due to time.
    pub fn should_flush(&self) -> bool {
        !self.buffer.is_empty() && self.last_push.elapsed() >= Duration::from_millis(DEBOUNCE_MS)
    }

    /// Try to flush buffer if conditions are met.
    pub fn try_flush(&mut self) -> Option<String> {
        if self.should_flush() {
            Some(self.flush())
        } else {
            None
        }
    }

    /// Flush and return buffer contents.
    pub fn flush(&mut self) -> String {
        std::mem::take(&mut self.buffer)
    }

    /// Check if buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    /// Get current buffer length.
    pub fn len(&self) -> usize {
        self.buffer.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn test_text_delta_buffer_char_threshold() {
        let mut buffer = TextDeltaBuffer::new();

        // Push text below threshold
        let result = buffer.push("Hello");
        assert!(result.is_none());
        assert_eq!(buffer.len(), 5);

        // Push text to exceed threshold
        let large_text = "A".repeat(DEBOUNCE_CHARS);
        let result = buffer.push(&large_text);
        assert!(result.is_some());
        assert_eq!(result.unwrap(), format!("Hello{large_text}"));
        assert!(buffer.is_empty());
    }

    #[tokio::test]
    async fn test_text_delta_buffer_time_threshold() {
        let mut buffer = TextDeltaBuffer::new();

        // Push some text
        let result = buffer.push("Hello");
        assert!(result.is_none());

        // Immediately check - should not flush
        assert!(!buffer.should_flush());

        // Wait for debounce time
        tokio::time::sleep(Duration::from_millis(DEBOUNCE_MS + 50)).await;

        // Should now be ready to flush
        assert!(buffer.should_flush());

        let flushed = buffer.try_flush();
        assert_eq!(flushed, Some("Hello".to_string()));
        assert!(buffer.is_empty());
    }

    #[test]
    fn test_text_delta_buffer_empty() {
        let buffer = TextDeltaBuffer::new();
        assert!(buffer.is_empty());
        assert_eq!(buffer.len(), 0);
        assert!(!buffer.should_flush()); // Empty buffer should not flush
    }

    #[test]
    fn test_client_message_deserialization() {
        let ping = r#"{"type": "ping"}"#;
        let msg: ClientMessage = serde_json::from_str(ping).unwrap();
        assert!(matches!(msg, ClientMessage::Ping));

        let message = r#"{"type": "message", "content": "Hello world"}"#;
        let msg: ClientMessage = serde_json::from_str(message).unwrap();
        assert!(matches!(msg, ClientMessage::Message { content } if content == "Hello world"));

        let set_model = r#"{"type": "set_model", "model": "claude-sonnet-4"}"#;
        let msg: ClientMessage = serde_json::from_str(set_model).unwrap();
        assert!(matches!(msg, ClientMessage::SetModel { model } if model == "claude-sonnet-4"));
    }

    #[test]
    fn test_server_message_serialization() {
        let pong = ServerMessage::Pong;
        let json = serde_json::to_string(&pong).unwrap();
        assert_eq!(json, r#"{"type":"pong"}"#);

        let typing = ServerMessage::Typing {
            state: "start".to_string(),
            tool_name: None,
        };
        let json = serde_json::to_string(&typing).unwrap();
        assert_eq!(json, r#"{"type":"typing","state":"start"}"#);

        let typing_with_tool = ServerMessage::Typing {
            state: "tool".to_string(),
            tool_name: Some("bash".to_string()),
        };
        let json = serde_json::to_string(&typing_with_tool).unwrap();
        assert_eq!(
            json,
            r#"{"type":"typing","state":"tool","tool_name":"bash"}"#
        );

        let text_delta = ServerMessage::TextDelta {
            content: "Hello".to_string(),
        };
        let json = serde_json::to_string(&text_delta).unwrap();
        assert_eq!(json, r#"{"type":"text_delta","content":"Hello"}"#);

        let response = ServerMessage::Response {
            content: "Full response".to_string(),
            input_tokens: 10,
            output_tokens: 15,
        };
        let json = serde_json::to_string(&response).unwrap();
        assert_eq!(
            json,
            r#"{"type":"response","content":"Full response","input_tokens":10,"output_tokens":15}"#
        );

        let error = ServerMessage::Error {
            content: "Something went wrong".to_string(),
        };
        let json = serde_json::to_string(&error).unwrap();
        assert_eq!(json, r#"{"type":"error","content":"Something went wrong"}"#);
    }

    #[test]
    fn test_connection_tracker_creation() {
        let tracker = new_connection_tracker();
        assert!(tracker.is_empty());
    }

    #[test]
    fn test_connection_guard_decrements_on_drop() {
        let tracker = new_connection_tracker();
        let ip: std::net::IpAddr = "127.0.0.1".parse().unwrap();

        // Simulate incrementing the counter (as ws_upgrade does)
        tracker.insert(ip, 3);

        // Create guard and drop it
        {
            let _guard = ConnectionGuard::new(tracker.clone(), ip);
        }

        // Count should be decremented
        assert_eq!(*tracker.get(&ip).unwrap(), 2);
    }

    #[test]
    fn test_connection_guard_removes_entry_at_zero() {
        let tracker = new_connection_tracker();
        let ip: std::net::IpAddr = "127.0.0.1".parse().unwrap();

        // Set count to 1
        tracker.insert(ip, 1);

        // Create guard and drop it
        {
            let _guard = ConnectionGuard::new(tracker.clone(), ip);
        }

        // Entry should be removed entirely
        assert!(tracker.get(&ip).is_none());
        assert!(tracker.is_empty());
    }

    #[test]
    fn test_connection_guard_handles_missing_entry() {
        let tracker = new_connection_tracker();
        let ip: std::net::IpAddr = "127.0.0.1".parse().unwrap();

        // Don't insert anything — guard should handle gracefully
        {
            let _guard = ConnectionGuard::new(tracker.clone(), ip);
        }

        // Should not panic, tracker should still be empty
        assert!(tracker.is_empty());
    }
}
