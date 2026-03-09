use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Unique agent identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AgentId(pub Uuid);

impl AgentId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for AgentId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for AgentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Unique session identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(pub Uuid);

impl SessionId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for SessionId {
    fn default() -> Self {
        Self::new()
    }
}

/// The kind of agent connected to the kernel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentKind {
    /// A pi TUI instance connected via the bridge extension.
    PiInstance,
    /// A webchat session from the browser.
    WebChat,
    /// A terminal chat session via `pi-daemon chat`.
    TerminalChat,
    /// An API client using /v1/chat/completions.
    ApiClient,
    /// An autonomous Hand.
    Hand,
}

/// Current agent status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    /// Agent is registered but idle.
    Idle,
    /// Agent is actively processing (thinking/tool use).
    Active,
    /// Agent is sleeping (scheduled, waiting for next run).
    Sleeping,
    /// Agent is paused by user.
    Paused,
    /// Agent has disconnected.
    Disconnected,
    /// Agent encountered an error.
    Error(String),
}

/// An agent registered with the kernel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentEntry {
    /// Unique agent ID.
    pub id: AgentId,
    /// Human-readable name (e.g., "pi-main", "researcher", "webchat-abc123").
    pub name: String,
    /// What kind of agent this is.
    pub kind: AgentKind,
    /// Current status.
    pub status: AgentStatus,
    /// When the agent registered with the kernel.
    pub registered_at: DateTime<Utc>,
    /// When the agent last sent a heartbeat.
    pub last_heartbeat: DateTime<Utc>,
    /// The model this agent is using (e.g., "claude-sonnet-4-20250514").
    pub model: Option<String>,
    /// Current session ID (if in an active conversation).
    pub current_session: Option<SessionId>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_id_new_is_unique() {
        let id1 = AgentId::new();
        let id2 = AgentId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_agent_id_display() {
        let id = AgentId::new();
        let display = format!("{}", id);
        assert_eq!(display, id.0.to_string());
    }

    #[test]
    fn test_session_id_new_is_unique() {
        let id1 = SessionId::new();
        let id2 = SessionId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_agent_kind_serialization() {
        let kind = AgentKind::PiInstance;
        let json = serde_json::to_string(&kind).unwrap();
        assert_eq!(json, "\"pi_instance\"");

        let kind2: AgentKind = serde_json::from_str(&json).unwrap();
        assert_eq!(kind, kind2);
    }

    #[test]
    fn test_agent_status_serialization() {
        let status = AgentStatus::Active;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, "\"active\"");

        let status_with_error = AgentStatus::Error("test error".to_string());
        let json2 = serde_json::to_string(&status_with_error).unwrap();
        let roundtrip: AgentStatus = serde_json::from_str(&json2).unwrap();
        assert_eq!(status_with_error, roundtrip);
    }

    #[test]
    fn test_agent_entry_serialization() {
        let agent_id = AgentId::new();
        let session_id = SessionId::new();
        let now = Utc::now();

        let entry = AgentEntry {
            id: agent_id.clone(),
            name: "test-agent".to_string(),
            kind: AgentKind::WebChat,
            status: AgentStatus::Idle,
            registered_at: now,
            last_heartbeat: now,
            model: Some("claude-sonnet-4".to_string()),
            current_session: Some(session_id.clone()),
        };

        let json = serde_json::to_string(&entry).unwrap();
        let roundtrip: AgentEntry = serde_json::from_str(&json).unwrap();

        assert_eq!(entry.id, roundtrip.id);
        assert_eq!(entry.name, roundtrip.name);
        assert_eq!(entry.kind, roundtrip.kind);
        assert_eq!(entry.status, roundtrip.status);
        assert_eq!(entry.model, roundtrip.model);
        assert_eq!(entry.current_session, roundtrip.current_session);
    }
}
