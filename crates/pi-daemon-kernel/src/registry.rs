use chrono::Utc;
use dashmap::DashMap;
use pi_daemon_types::agent::{AgentEntry, AgentId, AgentKind, AgentStatus};
use pi_daemon_types::error::{DaemonError, DaemonResult};
use tracing::info;

/// Concurrent agent registry. Thread-safe via DashMap.
pub struct AgentRegistry {
    agents: DashMap<AgentId, AgentEntry>,
}

impl AgentRegistry {
    pub fn new() -> Self {
        Self {
            agents: DashMap::new(),
        }
    }

    /// Register a new agent. Returns the assigned AgentId.
    pub fn register(&self, name: String, kind: AgentKind, model: Option<String>) -> AgentId {
        let id = AgentId::new();
        let now = Utc::now();
        let entry = AgentEntry {
            id: id.clone(),
            name: name.clone(),
            kind,
            status: AgentStatus::Idle,
            registered_at: now,
            last_heartbeat: now,
            model,
            current_session: None,
        };
        self.agents.insert(id.clone(), entry);
        info!(agent_id = %id, name = %name, "Agent registered");
        id
    }

    /// Unregister an agent.
    pub fn unregister(&self, id: &AgentId) -> DaemonResult<()> {
        self.agents
            .remove(id)
            .map(|_| {
                info!(agent_id = %id, "Agent unregistered");
            })
            .ok_or_else(|| DaemonError::AgentNotFound(id.to_string()))
    }

    /// Get a clone of an agent entry.
    pub fn get(&self, id: &AgentId) -> Option<AgentEntry> {
        self.agents.get(id).map(|entry| entry.clone())
    }

    /// List all registered agents.
    pub fn list(&self) -> Vec<AgentEntry> {
        self.agents
            .iter()
            .map(|entry| entry.value().clone())
            .collect()
    }

    /// Update agent status.
    pub fn set_status(&self, id: &AgentId, status: AgentStatus) -> DaemonResult<()> {
        self.agents
            .get_mut(id)
            .map(|mut entry| {
                entry.status = status;
            })
            .ok_or_else(|| DaemonError::AgentNotFound(id.to_string()))
    }

    /// Record a heartbeat from an agent.
    pub fn heartbeat(&self, id: &AgentId) -> DaemonResult<()> {
        self.agents
            .get_mut(id)
            .map(|mut entry| {
                entry.last_heartbeat = Utc::now();
            })
            .ok_or_else(|| DaemonError::AgentNotFound(id.to_string()))
    }

    /// Get the number of registered agents.
    pub fn count(&self) -> usize {
        self.agents.len()
    }

    /// Find an agent by name.
    pub fn find_by_name(&self, name: &str) -> Option<AgentEntry> {
        self.agents
            .iter()
            .find(|entry| entry.value().name == name)
            .map(|entry| entry.value().clone())
    }
}

impl Default for AgentRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pi_daemon_types::agent::AgentKind;

    #[test]
    fn test_register_agent_appears_in_list() {
        let registry = AgentRegistry::new();
        let id = registry.register(
            "test-agent".to_string(),
            AgentKind::WebChat,
            Some("claude-sonnet-4".to_string()),
        );

        let agents = registry.list();
        assert_eq!(agents.len(), 1);
        assert_eq!(agents[0].id, id);
        assert_eq!(agents[0].name, "test-agent");
        assert_eq!(agents[0].kind, AgentKind::WebChat);
        assert_eq!(agents[0].model, Some("claude-sonnet-4".to_string()));
        assert_eq!(agents[0].status, AgentStatus::Idle);
    }

    #[test]
    fn test_register_then_unregister() {
        let registry = AgentRegistry::new();
        let id = registry.register("test".to_string(), AgentKind::PiInstance, None);

        // Verify it exists
        assert_eq!(registry.count(), 1);
        assert!(registry.get(&id).is_some());

        // Unregister
        let result = registry.unregister(&id);
        assert!(result.is_ok());

        // Verify it's gone
        assert_eq!(registry.count(), 0);
        assert!(registry.get(&id).is_none());
    }

    #[test]
    fn test_find_by_name() {
        let registry = AgentRegistry::new();
        let id1 = registry.register("alice".to_string(), AgentKind::WebChat, None);
        let _id2 = registry.register("bob".to_string(), AgentKind::PiInstance, None);

        let alice = registry.find_by_name("alice");
        assert!(alice.is_some());
        assert_eq!(alice.unwrap().id, id1);

        let charlie = registry.find_by_name("charlie");
        assert!(charlie.is_none());
    }

    #[test]
    fn test_heartbeat_updates_timestamp() {
        let registry = AgentRegistry::new();
        let id = registry.register("test".to_string(), AgentKind::TerminalChat, None);

        let original_time = registry.get(&id).unwrap().last_heartbeat;

        // Wait a tiny bit to ensure timestamp difference
        std::thread::sleep(std::time::Duration::from_millis(1));

        let result = registry.heartbeat(&id);
        assert!(result.is_ok());

        let updated_time = registry.get(&id).unwrap().last_heartbeat;
        assert!(updated_time > original_time);
    }

    #[test]
    fn test_set_status_changes_status() {
        let registry = AgentRegistry::new();
        let id = registry.register("test".to_string(), AgentKind::Hand, None);

        // Initially idle
        assert_eq!(registry.get(&id).unwrap().status, AgentStatus::Idle);

        // Change to active
        let result = registry.set_status(&id, AgentStatus::Active);
        assert!(result.is_ok());
        assert_eq!(registry.get(&id).unwrap().status, AgentStatus::Active);

        // Change to error
        let result = registry.set_status(&id, AgentStatus::Error("test error".to_string()));
        assert!(result.is_ok());
        assert_eq!(
            registry.get(&id).unwrap().status,
            AgentStatus::Error("test error".to_string())
        );
    }

    #[test]
    fn test_unregister_nonexistent_returns_error() {
        let registry = AgentRegistry::new();
        let fake_id = AgentId::new();

        let result = registry.unregister(&fake_id);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), DaemonError::AgentNotFound(_)));
    }

    #[test]
    fn test_heartbeat_nonexistent_returns_error() {
        let registry = AgentRegistry::new();
        let fake_id = AgentId::new();

        let result = registry.heartbeat(&fake_id);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), DaemonError::AgentNotFound(_)));
    }

    #[test]
    fn test_set_status_nonexistent_returns_error() {
        let registry = AgentRegistry::new();
        let fake_id = AgentId::new();

        let result = registry.set_status(&fake_id, AgentStatus::Active);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), DaemonError::AgentNotFound(_)));
    }
}
