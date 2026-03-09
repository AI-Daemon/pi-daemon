use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::agent::AgentId;

/// Unique event identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EventId(pub Uuid);

impl EventId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for EventId {
    fn default() -> Self {
        Self::new()
    }
}

/// Who an event is targeted at.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EventTarget {
    /// Send to a specific agent.
    Agent(AgentId),
    /// Broadcast to all listeners.
    Broadcast,
}

/// Event payload — what happened.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum EventPayload {
    /// Agent registered with the kernel.
    AgentRegistered { name: String },
    /// Agent disconnected.
    AgentDisconnected { reason: String },
    /// Agent status changed.
    AgentStatusChanged { old: String, new: String },
    /// New chat message from user.
    UserMessage { content: String },
    /// Agent produced a response.
    AgentResponse { content: String },
    /// Agent started using a tool.
    ToolStarted { tool_name: String },
    /// Agent finished using a tool.
    ToolCompleted { tool_name: String, success: bool },
    /// System-level event (startup, shutdown, config change).
    System { message: String },
}

/// An event on the event bus.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub id: EventId,
    pub source: AgentId,
    pub target: EventTarget,
    pub payload: EventPayload,
    pub timestamp: DateTime<Utc>,
}

impl Event {
    pub fn new(source: AgentId, target: EventTarget, payload: EventPayload) -> Self {
        Self {
            id: EventId::new(),
            source,
            target,
            payload,
            timestamp: Utc::now(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::AgentId;

    #[test]
    fn test_event_id_new_is_unique() {
        let id1 = EventId::new();
        let id2 = EventId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_event_target_agent_serialization() {
        let agent_id = AgentId::new();
        let target = EventTarget::Agent(agent_id.clone());

        let json = serde_json::to_string(&target).unwrap();
        let roundtrip: EventTarget = serde_json::from_str(&json).unwrap();

        if let EventTarget::Agent(id) = roundtrip {
            assert_eq!(id, agent_id);
        } else {
            panic!("Expected Agent variant");
        }
    }

    #[test]
    fn test_event_target_broadcast_serialization() {
        let target = EventTarget::Broadcast;

        let json = serde_json::to_string(&target).unwrap();
        let roundtrip: EventTarget = serde_json::from_str(&json).unwrap();

        assert!(matches!(roundtrip, EventTarget::Broadcast));
    }

    #[test]
    fn test_event_payload_agent_registered_serialization() {
        let payload = EventPayload::AgentRegistered {
            name: "test-agent".to_string(),
        };

        let json = serde_json::to_string(&payload).unwrap();
        let roundtrip: EventPayload = serde_json::from_str(&json).unwrap();

        if let EventPayload::AgentRegistered { name } = roundtrip {
            assert_eq!(name, "test-agent");
        } else {
            panic!("Expected AgentRegistered variant");
        }
    }

    #[test]
    fn test_event_payload_tool_completed_serialization() {
        let payload = EventPayload::ToolCompleted {
            tool_name: "test_tool".to_string(),
            success: true,
        };

        let json = serde_json::to_string(&payload).unwrap();
        let roundtrip: EventPayload = serde_json::from_str(&json).unwrap();

        if let EventPayload::ToolCompleted { tool_name, success } = roundtrip {
            assert_eq!(tool_name, "test_tool");
            assert!(success);
        } else {
            panic!("Expected ToolCompleted variant");
        }
    }

    #[test]
    fn test_event_new_sets_timestamp() {
        let agent_id = AgentId::new();
        let before = Utc::now();

        let event = Event::new(
            agent_id.clone(),
            EventTarget::Broadcast,
            EventPayload::System {
                message: "test".to_string(),
            },
        );

        let after = Utc::now();

        assert!(event.timestamp >= before);
        assert!(event.timestamp <= after);
        assert_eq!(event.source, agent_id);
        assert!(matches!(event.target, EventTarget::Broadcast));
        assert!(matches!(event.payload, EventPayload::System { .. }));
    }

    #[test]
    fn test_event_serialization() {
        let agent_id = AgentId::new();
        let event = Event::new(
            agent_id.clone(),
            EventTarget::Broadcast,
            EventPayload::UserMessage {
                content: "Hello!".to_string(),
            },
        );

        let json = serde_json::to_string(&event).unwrap();
        let roundtrip: Event = serde_json::from_str(&json).unwrap();

        assert_eq!(event.id, roundtrip.id);
        assert_eq!(event.source, roundtrip.source);
        assert_eq!(event.timestamp, roundtrip.timestamp);

        if let EventPayload::UserMessage { content } = roundtrip.payload {
            assert_eq!(content, "Hello!");
        } else {
            panic!("Expected UserMessage payload");
        }
    }
}
