//! Integration test: kernel lifecycle with agent registration and event history

use pi_daemon_kernel::PiDaemonKernel;
use pi_daemon_types::agent::{AgentId, AgentKind};
use pi_daemon_types::event::EventPayload;
use tokio::time::{timeout, Duration};

#[tokio::test]
async fn test_kernel_lifecycle_with_agent_and_events() {
    let kernel = PiDaemonKernel::new();
    let mut receiver = kernel.event_bus.subscribe_global();

    kernel.init().await;

    let startup_event = timeout(Duration::from_millis(100), receiver.recv())
        .await
        .expect("Should receive startup event")
        .expect("Should receive startup event");

    if let EventPayload::System { message } = startup_event.payload {
        assert_eq!(message, "Kernel started");
    } else {
        panic!("Expected System startup event");
    }

    let agent_id = kernel
        .register_agent(
            "test-lifecycle-agent".to_string(),
            AgentKind::WebChat,
            Some("test-model".to_string()),
        )
        .await;

    let reg_event = timeout(Duration::from_millis(100), receiver.recv())
        .await
        .expect("Should receive registration event")
        .expect("Should receive registration event");

    assert_eq!(reg_event.source, agent_id);
    if let EventPayload::AgentRegistered { name } = reg_event.payload {
        assert_eq!(name, "test-lifecycle-agent");
    } else {
        panic!("Expected AgentRegistered event");
    }

    let agent = kernel.registry.get(&agent_id).unwrap();
    assert_eq!(agent.name, "test-lifecycle-agent");
    assert_eq!(agent.kind, AgentKind::WebChat);
    assert_eq!(agent.model, Some("test-model".to_string()));

    kernel
        .unregister_agent(&agent_id, "lifecycle test complete".to_string())
        .await;

    let disc_event = timeout(Duration::from_millis(100), receiver.recv())
        .await
        .expect("Should receive disconnection event")
        .expect("Should receive disconnection event");

    assert_eq!(disc_event.source, agent_id);
    if let EventPayload::AgentDisconnected { reason } = disc_event.payload {
        assert_eq!(reason, "lifecycle test complete");
    } else {
        panic!("Expected AgentDisconnected event");
    }

    assert!(kernel.registry.get(&agent_id).is_none());

    let history = kernel.event_bus.history(10).await;
    assert_eq!(history.len(), 3);

    if let EventPayload::AgentDisconnected { .. } = history[0].payload {
        // Most recent event should be disconnection
    } else {
        panic!("Expected most recent event to be AgentDisconnected");
    }

    if let EventPayload::System { message } = &history[2].payload {
        assert_eq!(message, "Kernel started");
    } else {
        panic!("Expected oldest event to be System startup");
    }
}

#[tokio::test]
async fn test_multiple_agents_lifecycle() {
    let kernel = PiDaemonKernel::new();
    kernel.init().await;

    let agent1 = kernel
        .register_agent("agent-1".to_string(), AgentKind::PiInstance, None)
        .await;

    let agent2 = kernel
        .register_agent(
            "agent-2".to_string(),
            AgentKind::TerminalChat,
            Some("claude-haiku".to_string()),
        )
        .await;

    assert_eq!(kernel.registry.count(), 2);
    let agents = kernel.registry.list();
    assert_eq!(agents.len(), 2);

    let found1 = kernel.registry.find_by_name("agent-1");
    assert!(found1.is_some());
    assert_eq!(found1.unwrap().id, agent1);

    let found2 = kernel.registry.find_by_name("agent-2");
    assert!(found2.is_some());
    assert_eq!(found2.unwrap().id, agent2);

    kernel
        .unregister_agent(&agent1, "test complete".to_string())
        .await;

    assert_eq!(kernel.registry.count(), 1);
    assert!(kernel.registry.get(&agent1).is_none());
    assert!(kernel.registry.get(&agent2).is_some());

    kernel
        .unregister_agent(&agent2, "all done".to_string())
        .await;

    assert_eq!(kernel.registry.count(), 0);
}

#[tokio::test]
async fn test_event_history_persistence() {
    let kernel = PiDaemonKernel::new();
    kernel.init().await;

    for i in 0..5 {
        kernel
            .register_agent(format!("agent-{}", i), AgentKind::Hand, None)
            .await;
    }

    assert_eq!(kernel.registry.count(), 5);

    let history = kernel.event_bus.history(20).await;
    assert_eq!(history.len(), 6); // startup + 5 registrations

    let mut system_events = 0;
    let mut registration_events = 0;

    for event in history {
        match event.payload {
            EventPayload::System { .. } => system_events += 1,
            EventPayload::AgentRegistered { .. } => registration_events += 1,
            _ => panic!("Unexpected event type in history"),
        }
    }

    assert_eq!(system_events, 1);
    assert_eq!(registration_events, 5);
}

// ─── New edge case tests ─────────────────────────────────

#[tokio::test]
async fn test_duplicate_name_creates_distinct_agents() {
    let kernel = PiDaemonKernel::new();
    kernel.init().await;

    let id1 = kernel
        .register_agent("same-name".to_string(), AgentKind::WebChat, None)
        .await;
    let id2 = kernel
        .register_agent("same-name".to_string(), AgentKind::WebChat, None)
        .await;

    // Should create two distinct agents with different IDs
    assert_ne!(id1, id2);
    assert_eq!(kernel.registry.count(), 2);

    // Both should be retrievable
    assert!(kernel.registry.get(&id1).is_some());
    assert!(kernel.registry.get(&id2).is_some());
}

#[tokio::test]
async fn test_unregister_nonexistent_agent_is_safe() {
    let kernel = PiDaemonKernel::new();
    kernel.init().await;

    // Create a fake AgentId that was never registered
    let fake_id = AgentId::new();

    // Should not panic — should be a no-op
    kernel
        .unregister_agent(&fake_id, "never existed".to_string())
        .await;

    // Kernel should still be healthy
    assert_eq!(kernel.registry.count(), 0);

    // Should still be able to register new agents
    let real_id = kernel
        .register_agent("after-fake".to_string(), AgentKind::ApiClient, None)
        .await;
    assert!(kernel.registry.get(&real_id).is_some());
}

#[tokio::test]
async fn test_event_history_limit_respected() {
    let kernel = PiDaemonKernel::new();
    kernel.init().await;

    // Generate many events
    for i in 0..20 {
        kernel
            .register_agent(format!("agent-{}", i), AgentKind::Hand, None)
            .await;
    }

    // Requesting fewer than available should respect the limit
    let history_5 = kernel.event_bus.history(5).await;
    assert_eq!(history_5.len(), 5);

    let history_10 = kernel.event_bus.history(10).await;
    assert_eq!(history_10.len(), 10);

    // The 5-event history should be a subset of the 10-event history
    assert_eq!(history_5[0].source, history_10[0].source);
}

#[tokio::test]
async fn test_all_agent_kinds_register_successfully() {
    let kernel = PiDaemonKernel::new();
    kernel.init().await;

    let kinds = vec![
        AgentKind::PiInstance,
        AgentKind::WebChat,
        AgentKind::TerminalChat,
        AgentKind::ApiClient,
        AgentKind::Hand,
    ];

    for (i, kind) in kinds.into_iter().enumerate() {
        let id = kernel
            .register_agent(format!("agent-kind-{}", i), kind.clone(), None)
            .await;
        let agent = kernel
            .registry
            .get(&id)
            .expect("Agent should be registered");
        assert_eq!(agent.kind, kind);
    }

    assert_eq!(kernel.registry.count(), 5);
}
