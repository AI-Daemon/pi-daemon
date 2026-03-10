//! Integration test: concurrent agent registration stress test

use pi_daemon_kernel::registry::AgentRegistry;
use pi_daemon_types::agent::{AgentId, AgentKind, AgentStatus};
use std::sync::Arc;
use tokio::task::JoinSet;

#[tokio::test]
async fn test_concurrent_agent_registration() {
    let registry = Arc::new(AgentRegistry::new());
    let mut tasks = JoinSet::new();

    // Spawn 10 concurrent tasks, each registering agents
    for task_id in 0..10 {
        let registry_clone = registry.clone();
        tasks.spawn(async move {
            let mut agent_ids = Vec::new();

            // Each task registers 5 agents
            for i in 0..5 {
                let agent_name = format!("task-{}-agent-{}", task_id, i);
                let id = registry_clone.register(
                    agent_name,
                    AgentKind::WebChat,
                    Some("test-model".to_string()),
                );
                agent_ids.push(id);
            }

            agent_ids
        });
    }

    // Collect all agent IDs from all tasks
    let mut all_agent_ids = Vec::new();
    while let Some(result) = tasks.join_next().await {
        let task_agent_ids = result.expect("Task should complete successfully");
        all_agent_ids.extend(task_agent_ids);
    }

    // Should have 50 total agents (10 tasks × 5 agents)
    assert_eq!(all_agent_ids.len(), 50);
    assert_eq!(registry.count(), 50);

    // All agent IDs should be unique
    let mut unique_ids = all_agent_ids.clone();
    unique_ids.sort_by_key(|id| id.0);
    unique_ids.dedup();
    assert_eq!(unique_ids.len(), 50, "All agent IDs should be unique");

    // All agents should be findable in registry
    for agent_id in &all_agent_ids {
        let agent = registry.get(agent_id);
        assert!(agent.is_some(), "Agent {} should be in registry", agent_id);

        let agent = agent.unwrap();
        assert_eq!(agent.kind, AgentKind::WebChat);
        assert_eq!(agent.status, AgentStatus::Idle);
        assert!(agent.name.starts_with("task-"));
    }

    // List should return all agents
    let listed_agents = registry.list();
    assert_eq!(listed_agents.len(), 50);
}

#[tokio::test]
async fn test_concurrent_agent_operations() {
    let registry = Arc::new(AgentRegistry::new());

    // Pre-register 20 agents
    let mut agent_ids = Vec::new();
    for i in 0..20 {
        let id = registry.register(format!("agent-{}", i), AgentKind::PiInstance, None);
        agent_ids.push(id);
    }

    let mut tasks = JoinSet::new();

    // Spawn concurrent operations
    for i in 0..10 {
        let registry_clone = registry.clone();
        let agent_id = agent_ids[i % agent_ids.len()].clone();

        match i % 4 {
            0 => {
                // Heartbeat task
                tasks.spawn(async move {
                    for _ in 0..5 {
                        let _ = registry_clone.heartbeat(&agent_id);
                        tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
                    }
                });
            }
            1 => {
                // Status update task
                tasks.spawn(async move {
                    let _ = registry_clone.set_status(&agent_id, AgentStatus::Active);
                    tokio::time::sleep(tokio::time::Duration::from_millis(2)).await;
                    let _ = registry_clone.set_status(&agent_id, AgentStatus::Idle);
                });
            }
            2 => {
                // Read task
                tasks.spawn(async move {
                    for _ in 0..10 {
                        let _ = registry_clone.get(&agent_id);
                        let _ = registry_clone.list();
                        tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
                    }
                });
            }
            3 => {
                // Find by name task
                tasks.spawn(async move {
                    let agent_name = format!("agent-{}", i % 20);
                    for _ in 0..5 {
                        let _ = registry_clone.find_by_name(&agent_name);
                        tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
                    }
                });
            }
            _ => unreachable!(),
        }
    }

    // Wait for all tasks to complete
    while let Some(result) = tasks.join_next().await {
        result.expect("All tasks should complete successfully");
    }

    // Registry should still be in a consistent state
    assert_eq!(registry.count(), 20);

    // All original agents should still exist
    for agent_id in &agent_ids {
        let agent = registry.get(agent_id);
        assert!(
            agent.is_some(),
            "Agent should still exist after concurrent operations"
        );
    }
}

#[tokio::test]
async fn test_concurrent_register_unregister() {
    let registry = Arc::new(AgentRegistry::new());
    let mut tasks = JoinSet::new();

    // Spawn tasks that register and then unregister agents
    for task_id in 0..5 {
        let registry_clone = registry.clone();
        tasks.spawn(async move {
            let mut registered_ids = Vec::new();

            // Register 3 agents
            for i in 0..3 {
                let id = registry_clone.register(
                    format!("temp-task-{}-agent-{}", task_id, i),
                    AgentKind::Hand,
                    None,
                );
                registered_ids.push(id);
            }

            // Small delay
            tokio::time::sleep(tokio::time::Duration::from_millis(5)).await;

            // Unregister 2 of the 3 agents
            for id in registered_ids.iter().take(2) {
                let _ = registry_clone.unregister(id);
            }

            // Return the ID of the remaining agent
            registered_ids[2].clone()
        });
    }

    // Collect remaining agent IDs
    let mut remaining_ids = Vec::new();
    while let Some(result) = tasks.join_next().await {
        let remaining_id = result.expect("Task should complete");
        remaining_ids.push(remaining_id);
    }

    // Should have 5 remaining agents (1 per task)
    assert_eq!(registry.count(), 5);
    assert_eq!(remaining_ids.len(), 5);

    // Verify the remaining agents are the correct ones
    for id in &remaining_ids {
        let agent = registry.get(id);
        assert!(agent.is_some());
        assert!(agent.unwrap().name.contains("agent-2")); // The third agent (index 2)
    }
}

// ─── New edge case tests ─────────────────────────────────

#[tokio::test]
async fn test_registry_count_linearizability() {
    let registry = Arc::new(AgentRegistry::new());

    // Register N agents sequentially, assert count is accurate at every step
    let mut ids = Vec::new();
    for i in 0..10 {
        let id = registry.register(format!("linear-{}", i), AgentKind::WebChat, None);
        ids.push(id);
        assert_eq!(
            registry.count(),
            i + 1,
            "Count should be {} after {} registrations",
            i + 1,
            i + 1
        );
    }

    // Delete every other agent, check count
    for (i, id) in ids.iter().enumerate() {
        if i % 2 == 0 {
            let _ = registry.unregister(id);
        }
    }

    // 5 of 10 should remain
    assert_eq!(registry.count(), 5);

    // list() should agree with count()
    assert_eq!(registry.list().len(), registry.count());
}

#[tokio::test]
async fn test_registry_find_by_name_returns_none_for_missing() {
    let registry = AgentRegistry::new();
    registry.register("exists".to_string(), AgentKind::PiInstance, None);

    assert!(registry.find_by_name("exists").is_some());
    assert!(registry.find_by_name("does-not-exist").is_none());
    assert!(registry.find_by_name("").is_none());
}

#[tokio::test]
async fn test_registry_unregister_nonexistent_is_safe() {
    let registry = AgentRegistry::new();
    let fake_id = AgentId::new();

    // Should not panic
    let _ = registry.unregister(&fake_id);

    // Registry should remain functional
    let id = registry.register("after-fake".to_string(), AgentKind::Hand, None);
    assert!(registry.get(&id).is_some());
    assert_eq!(registry.count(), 1);
}

#[tokio::test]
async fn test_concurrent_mixed_operations_consistency() {
    let registry = Arc::new(AgentRegistry::new());
    let mut tasks = JoinSet::new();

    // 20 tasks doing mixed operations concurrently
    for task_id in 0..20 {
        let reg = registry.clone();
        tasks.spawn(async move {
            // Register
            let id = reg.register(
                format!("mixed-{}", task_id),
                AgentKind::ApiClient,
                Some("model".to_string()),
            );
            // Heartbeat
            let _ = reg.heartbeat(&id);
            // Status update
            let _ = reg.set_status(&id, AgentStatus::Active);
            // Read
            let _ = reg.get(&id);
            let _ = reg.list();
            // Back to idle
            let _ = reg.set_status(&id, AgentStatus::Idle);
            id
        });
    }

    let mut ids = Vec::new();
    while let Some(result) = tasks.join_next().await {
        ids.push(result.expect("Task should complete"));
    }

    // All 20 should exist and be in Idle status
    assert_eq!(registry.count(), 20);
    for id in &ids {
        let agent = registry.get(id).expect("Agent should exist");
        assert_eq!(agent.status, AgentStatus::Idle);
    }
}
