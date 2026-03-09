use crate::event_bus::EventBus;
use crate::registry::AgentRegistry;
use pi_daemon_types::agent::{AgentId, AgentKind};
use pi_daemon_types::event::{Event, EventPayload, EventTarget};

/// System agent ID — used as the source for system-level events.
fn system_agent_id() -> AgentId {
    AgentId(uuid::Uuid::nil())
}

/// The main pi-daemon kernel — coordinates all subsystems.
pub struct PiDaemonKernel {
    /// Agent registry.
    pub registry: AgentRegistry,
    /// Event bus.
    pub event_bus: EventBus,
    /// Kernel start time.
    pub started_at: chrono::DateTime<chrono::Utc>,
}

impl PiDaemonKernel {
    /// Create a new kernel.
    pub fn new() -> Self {
        // Async initialization will be done in init()
        Self {
            registry: AgentRegistry::new(),
            event_bus: EventBus::new(),
            started_at: chrono::Utc::now(),
        }
    }

    /// Async initialization — publish startup event.
    pub async fn init(&self) {
        self.event_bus
            .publish(Event::new(
                system_agent_id(),
                EventTarget::Broadcast,
                EventPayload::System {
                    message: "Kernel started".to_string(),
                },
            ))
            .await;
    }

    /// Register an agent and publish event.
    pub async fn register_agent(
        &self,
        name: String,
        kind: AgentKind,
        model: Option<String>,
    ) -> AgentId {
        let id = self.registry.register(name.clone(), kind, model);
        self.event_bus
            .publish(Event::new(
                id.clone(),
                EventTarget::Broadcast,
                EventPayload::AgentRegistered { name },
            ))
            .await;
        id
    }

    /// Unregister an agent and publish event.
    pub async fn unregister_agent(&self, id: &AgentId, reason: String) {
        let _ = self.registry.unregister(id);
        self.event_bus.remove_agent_channel(id);
        self.event_bus
            .publish(Event::new(
                id.clone(),
                EventTarget::Broadcast,
                EventPayload::AgentDisconnected { reason },
            ))
            .await;
    }

    /// Get uptime in seconds.
    pub fn uptime_secs(&self) -> i64 {
        (chrono::Utc::now() - self.started_at).num_seconds()
    }
}

impl Default for PiDaemonKernel {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pi_daemon_types::agent::AgentKind;
    use pi_daemon_types::event::EventPayload;
    use tokio::time::{timeout, Duration};

    #[tokio::test]
    async fn test_register_agent_returns_valid_id_and_appears_in_registry() {
        let kernel = PiDaemonKernel::new();

        let id = kernel
            .register_agent(
                "test-agent".to_string(),
                AgentKind::WebChat,
                Some("claude-sonnet-4".to_string()),
            )
            .await;

        // Verify agent is in registry
        let agent = kernel.registry.get(&id);
        assert!(agent.is_some());

        let agent = agent.unwrap();
        assert_eq!(agent.id, id);
        assert_eq!(agent.name, "test-agent");
        assert_eq!(agent.kind, AgentKind::WebChat);
        assert_eq!(agent.model, Some("claude-sonnet-4".to_string()));
    }

    #[tokio::test]
    async fn test_register_agent_publishes_event() {
        let kernel = PiDaemonKernel::new();
        let mut receiver = kernel.event_bus.subscribe_global();

        let id = kernel
            .register_agent("test-agent".to_string(), AgentKind::PiInstance, None)
            .await;

        // Should receive registration event
        let event = timeout(Duration::from_millis(100), receiver.recv())
            .await
            .expect("Should receive event within timeout")
            .expect("Should receive event");

        assert_eq!(event.source, id);
        if let EventPayload::AgentRegistered { name } = event.payload {
            assert_eq!(name, "test-agent");
        } else {
            panic!("Expected AgentRegistered event");
        }
    }

    #[tokio::test]
    async fn test_unregister_agent_removes_from_registry_and_cleans_up() {
        let kernel = PiDaemonKernel::new();
        let mut receiver = kernel.event_bus.subscribe_global();

        // Register an agent first
        let id = kernel
            .register_agent("test-agent".to_string(), AgentKind::TerminalChat, None)
            .await;

        // Clear the registration event
        let _ = timeout(Duration::from_millis(10), receiver.recv()).await;

        // Subscribe to agent-specific channel to verify cleanup
        let _agent_receiver = kernel.event_bus.subscribe_agent(&id);
        assert!(kernel.event_bus.has_agent_channel(&id));

        // Unregister
        kernel
            .unregister_agent(&id, "test disconnect".to_string())
            .await;

        // Verify agent is removed from registry
        assert!(kernel.registry.get(&id).is_none());
        assert_eq!(kernel.registry.count(), 0);

        // Verify agent channel is cleaned up
        assert!(!kernel.event_bus.has_agent_channel(&id));

        // Should receive disconnection event
        let event = timeout(Duration::from_millis(100), receiver.recv())
            .await
            .expect("Should receive event within timeout")
            .expect("Should receive event");

        assert_eq!(event.source, id);
        if let EventPayload::AgentDisconnected { reason } = event.payload {
            assert_eq!(reason, "test disconnect");
        } else {
            panic!("Expected AgentDisconnected event");
        }
    }

    #[tokio::test]
    async fn test_uptime_secs_returns_non_negative() {
        let kernel = PiDaemonKernel::new();

        let uptime = kernel.uptime_secs();
        assert!(uptime >= 0);

        // Wait a bit and check again
        tokio::time::sleep(Duration::from_millis(10)).await;
        let uptime2 = kernel.uptime_secs();
        assert!(uptime2 >= uptime);
    }

    #[tokio::test]
    async fn test_init_publishes_startup_event() {
        let kernel = PiDaemonKernel::new();
        let mut receiver = kernel.event_bus.subscribe_global();

        kernel.init().await;

        let event = timeout(Duration::from_millis(100), receiver.recv())
            .await
            .expect("Should receive event within timeout")
            .expect("Should receive event");

        assert_eq!(event.source, system_agent_id());
        if let EventPayload::System { message } = event.payload {
            assert_eq!(message, "Kernel started");
        } else {
            panic!("Expected System event");
        }
    }

    #[test]
    fn test_system_agent_id_is_nil_uuid() {
        let id = system_agent_id();
        assert_eq!(id.0, uuid::Uuid::nil());
    }
}
