//! Integration test: kernel lifecycle with agent registration and event history

use pi_daemon_kernel::PiDaemonKernel;
use pi_daemon_types::agent::AgentKind;
use pi_daemon_types::event::EventPayload;
use tokio::time::{timeout, Duration};

#[tokio::test]
async fn test_kernel_lifecycle_with_agent_and_events() {
    let kernel = PiDaemonKernel::new();
    let mut receiver = kernel.event_bus.subscribe_global();

    // Initialize kernel
    kernel.init().await;

    // Should receive startup event
    let startup_event = timeout(Duration::from_millis(100), receiver.recv())
        .await
        .expect("Should receive startup event")
        .expect("Should receive startup event");

    if let EventPayload::System { message } = startup_event.payload {
        assert_eq!(message, "Kernel started");
    } else {
        panic!("Expected System startup event");
    }

    // Register an agent
    let agent_id = kernel
        .register_agent(
            "test-lifecycle-agent".to_string(),
            AgentKind::WebChat,
            Some("test-model".to_string()),
        )
        .await;

    // Should receive registration event
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

    // Verify agent is in registry
    let agent = kernel.registry.get(&agent_id).unwrap();
    assert_eq!(agent.name, "test-lifecycle-agent");
    assert_eq!(agent.kind, AgentKind::WebChat);
    assert_eq!(agent.model, Some("test-model".to_string()));

    // Unregister the agent
    kernel
        .unregister_agent(&agent_id, "lifecycle test complete".to_string())
        .await;

    // Should receive disconnection event
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

    // Verify agent is removed from registry
    assert!(kernel.registry.get(&agent_id).is_none());

    // Verify event history contains our events
    let history = kernel.event_bus.history(10).await;
    assert_eq!(history.len(), 3); // startup + register + unregister

    // Events should be in reverse chronological order
    if let EventPayload::AgentDisconnected { .. } = history[0].payload {
        // Most recent event should be disconnection
    } else {
        panic!("Expected most recent event to be AgentDisconnected");
    }

    if let EventPayload::System { message } = &history[2].payload {
        // Oldest event should be startup
        assert_eq!(message, "Kernel started");
    } else {
        panic!("Expected oldest event to be System startup");
    }
}

#[tokio::test]
async fn test_multiple_agents_lifecycle() {
    let kernel = PiDaemonKernel::new();
    kernel.init().await;

    // Register multiple agents
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

    // Verify both are registered
    assert_eq!(kernel.registry.count(), 2);
    let agents = kernel.registry.list();
    assert_eq!(agents.len(), 2);

    // Verify we can find by name
    let found1 = kernel.registry.find_by_name("agent-1");
    assert!(found1.is_some());
    assert_eq!(found1.unwrap().id, agent1);

    let found2 = kernel.registry.find_by_name("agent-2");
    assert!(found2.is_some());
    assert_eq!(found2.unwrap().id, agent2);

    // Unregister first agent
    kernel
        .unregister_agent(&agent1, "test complete".to_string())
        .await;

    // Second agent should still be there
    assert_eq!(kernel.registry.count(), 1);
    assert!(kernel.registry.get(&agent1).is_none());
    assert!(kernel.registry.get(&agent2).is_some());

    // Unregister second agent
    kernel
        .unregister_agent(&agent2, "all done".to_string())
        .await;

    // No agents left
    assert_eq!(kernel.registry.count(), 0);
}

#[tokio::test]
async fn test_event_history_persistence() {
    let kernel = PiDaemonKernel::new();
    kernel.init().await;

    // Generate several events
    for i in 0..5 {
        kernel
            .register_agent(format!("agent-{}", i), AgentKind::Hand, None)
            .await;
    }

    // All agents should be registered
    assert_eq!(kernel.registry.count(), 5);

    // History should contain startup + 5 registration events = 6 total
    let history = kernel.event_bus.history(20).await;
    assert_eq!(history.len(), 6);

    // Verify event types in history
    let mut system_events = 0;
    let mut registration_events = 0;

    for event in history {
        match event.payload {
            EventPayload::System { .. } => system_events += 1,
            EventPayload::AgentRegistered { .. } => registration_events += 1,
            _ => panic!("Unexpected event type in history"),
        }
    }

    assert_eq!(system_events, 1); // startup event
    assert_eq!(registration_events, 5); // 5 agent registrations
}
