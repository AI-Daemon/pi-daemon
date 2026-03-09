use dashmap::DashMap;
use pi_daemon_types::agent::AgentId;
use pi_daemon_types::event::{Event, EventTarget};
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tracing::debug;

/// Maximum events retained in the history ring buffer.
const HISTORY_SIZE: usize = 1000;

/// Broadcast channel capacity.
const CHANNEL_CAPACITY: usize = 1024;

/// Central event bus for all inter-agent and system communication.
pub struct EventBus {
    /// Global broadcast channel.
    sender: broadcast::Sender<Event>,
    /// Per-agent channels for targeted events.
    agent_channels: DashMap<AgentId, broadcast::Sender<Event>>,
    /// Event history ring buffer (most recent HISTORY_SIZE events).
    history: Arc<RwLock<VecDeque<Event>>>,
}

impl EventBus {
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(CHANNEL_CAPACITY);
        Self {
            sender,
            agent_channels: DashMap::new(),
            history: Arc::new(RwLock::new(VecDeque::with_capacity(HISTORY_SIZE))),
        }
    }

    /// Publish an event. Routes to specific agent or broadcasts to all.
    pub async fn publish(&self, event: Event) {
        debug!(event_id = %event.id, source = %event.source, "Publishing event");

        // Store in history ring buffer
        {
            let mut history = self.history.write().await;
            if history.len() >= HISTORY_SIZE {
                history.pop_front();
            }
            history.push_back(event.clone());
        }

        // Route to target
        match &event.target {
            EventTarget::Agent(agent_id) => {
                if let Some(sender) = self.agent_channels.get(agent_id) {
                    let _ = sender.send(event);
                }
            }
            EventTarget::Broadcast => {
                let _ = self.sender.send(event);
            }
        }
    }

    /// Subscribe to the global broadcast channel.
    pub fn subscribe_global(&self) -> broadcast::Receiver<Event> {
        self.sender.subscribe()
    }

    /// Subscribe to events for a specific agent.
    /// Creates the per-agent channel if it doesn't exist.
    pub fn subscribe_agent(&self, agent_id: &AgentId) -> broadcast::Receiver<Event> {
        self.agent_channels
            .entry(agent_id.clone())
            .or_insert_with(|| broadcast::channel(CHANNEL_CAPACITY).0)
            .subscribe()
    }

    /// Remove per-agent channel (call when agent unregisters).
    pub fn remove_agent_channel(&self, agent_id: &AgentId) {
        self.agent_channels.remove(agent_id);
    }

    /// Get recent event history.
    pub async fn history(&self, limit: usize) -> Vec<Event> {
        let history = self.history.read().await;
        history.iter().rev().take(limit).cloned().collect()
    }

    /// Check if an agent channel exists (for testing).
    pub fn has_agent_channel(&self, agent_id: &AgentId) -> bool {
        self.agent_channels.contains_key(agent_id)
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pi_daemon_types::event::{EventPayload, EventTarget};
    use tokio::time::{timeout, Duration};

    #[tokio::test]
    async fn test_publish_broadcast_event_subscriber_receives() {
        let bus = EventBus::new();
        let mut receiver = bus.subscribe_global();

        let agent_id = AgentId::new();
        let event = Event::new(
            agent_id.clone(),
            EventTarget::Broadcast,
            EventPayload::System {
                message: "test".to_string(),
            },
        );

        bus.publish(event.clone()).await;

        let received = timeout(Duration::from_millis(100), receiver.recv())
            .await
            .expect("Should receive within timeout")
            .expect("Should receive event");

        assert_eq!(received.id, event.id);
        assert_eq!(received.source, event.source);
    }

    #[tokio::test]
    async fn test_publish_targeted_event_only_target_receives() {
        let bus = EventBus::new();

        let target_agent = AgentId::new();
        let other_agent = AgentId::new();

        let mut target_receiver = bus.subscribe_agent(&target_agent);
        let mut other_receiver = bus.subscribe_agent(&other_agent);
        let mut global_receiver = bus.subscribe_global();

        let event = Event::new(
            AgentId::new(),
            EventTarget::Agent(target_agent.clone()),
            EventPayload::UserMessage {
                content: "hello".to_string(),
            },
        );

        bus.publish(event.clone()).await;

        // Target agent should receive
        let received = timeout(Duration::from_millis(100), target_receiver.recv())
            .await
            .expect("Target should receive within timeout")
            .expect("Target should receive event");
        assert_eq!(received.id, event.id);

        // Other agent should not receive (timeout)
        let other_result = timeout(Duration::from_millis(50), other_receiver.recv()).await;
        assert!(
            other_result.is_err(),
            "Other agent should not receive targeted event"
        );

        // Global should not receive (timeout)
        let global_result = timeout(Duration::from_millis(50), global_receiver.recv()).await;
        assert!(
            global_result.is_err(),
            "Global should not receive targeted event"
        );
    }

    #[tokio::test]
    async fn test_history_stores_events_up_to_limit() {
        let bus = EventBus::new();
        let agent_id = AgentId::new();

        // Publish events up to the limit
        for i in 0..5 {
            let event = Event::new(
                agent_id.clone(),
                EventTarget::Broadcast,
                EventPayload::System {
                    message: format!("event {}", i),
                },
            );
            bus.publish(event).await;
        }

        let history = bus.history(10).await;
        assert_eq!(history.len(), 5);

        // Events should be in reverse order (newest first)
        if let EventPayload::System { message } = &history[0].payload {
            assert_eq!(message, "event 4");
        } else {
            panic!("Expected System event");
        }

        if let EventPayload::System { message } = &history[4].payload {
            assert_eq!(message, "event 0");
        } else {
            panic!("Expected System event");
        }
    }

    #[tokio::test]
    async fn test_history_evicts_oldest_when_full() {
        let bus = EventBus::new();
        let agent_id = AgentId::new();

        // Fill beyond HISTORY_SIZE (using small test size)
        // We'll test with more than HISTORY_SIZE events
        let test_events = HISTORY_SIZE + 10;

        for i in 0..test_events {
            let event = Event::new(
                agent_id.clone(),
                EventTarget::Broadcast,
                EventPayload::System {
                    message: format!("event {}", i),
                },
            );
            bus.publish(event).await;
        }

        let history = bus.history(HISTORY_SIZE + 20).await;

        // Should only have HISTORY_SIZE events
        assert_eq!(history.len(), HISTORY_SIZE);

        // Newest event should be the last one we sent
        if let EventPayload::System { message } = &history[0].payload {
            assert_eq!(message, &format!("event {}", test_events - 1));
        } else {
            panic!("Expected System event");
        }

        // Oldest event should be the (total - HISTORY_SIZE)th event
        if let EventPayload::System { message } = &history[HISTORY_SIZE - 1].payload {
            assert_eq!(message, &format!("event {}", test_events - HISTORY_SIZE));
        } else {
            panic!("Expected System event");
        }
    }

    #[tokio::test]
    async fn test_remove_agent_channel() {
        let bus = EventBus::new();
        let agent_id = AgentId::new();

        // Create a subscription (which creates the channel)
        let _receiver = bus.subscribe_agent(&agent_id);

        // Verify channel exists
        assert!(bus.has_agent_channel(&agent_id));

        // Remove it
        bus.remove_agent_channel(&agent_id);

        // Verify it's gone
        assert!(!bus.has_agent_channel(&agent_id));
    }

    #[tokio::test]
    async fn test_history_limit() {
        let bus = EventBus::new();
        let agent_id = AgentId::new();

        // Publish 10 events
        for i in 0..10 {
            let event = Event::new(
                agent_id.clone(),
                EventTarget::Broadcast,
                EventPayload::System {
                    message: format!("event {}", i),
                },
            );
            bus.publish(event).await;
        }

        // Request only 3 events
        let history = bus.history(3).await;
        assert_eq!(history.len(), 3);

        // Should get the 3 most recent
        if let EventPayload::System { message } = &history[0].payload {
            assert_eq!(message, "event 9");
        } else {
            panic!("Expected System event");
        }
    }
}
