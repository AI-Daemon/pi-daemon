//! API integration tests using actual HTTP requests

use pi_daemon_test_utils::FullTestServer;

#[tokio::test]
async fn test_api_health_endpoint() {
    let server = FullTestServer::new().await;
    let client = server.client();

    let response = client.get("/api/health").await;

    assert_eq!(response.status(), 200);

    let json: serde_json::Value = response.json().await.unwrap();
    assert_eq!(json["status"], "ok");
}

#[tokio::test]
async fn test_api_status_endpoint() {
    let server = FullTestServer::new().await;
    let client = server.client();

    let response = client.get("/api/status").await;

    assert_eq!(response.status(), 200);

    let json: serde_json::Value = response.json().await.unwrap();
    assert!(json["version"].is_string());
    assert!(json["uptime_secs"].is_number());
    assert!(json["agent_count"].is_number());
    assert_eq!(json["agent_count"], 0); // No agents registered initially
}

#[tokio::test]
async fn test_agent_crud_lifecycle() {
    let server = FullTestServer::new().await;
    let client = server.client();

    // 1. GET /api/agents should return empty list initially
    let response = client.get("/api/agents").await;
    assert_eq!(response.status(), 200);
    let agents: serde_json::Value = response.json().await.unwrap();
    assert_eq!(agents.as_array().unwrap().len(), 0);

    // 2. POST /api/agents to create an agent
    let create_request = serde_json::json!({
        "name": "test-agent",
        "kind": "web_chat",
        "model": "claude-sonnet-4"
    });

    let response = client.post_json("/api/agents", &create_request).await;
    assert_eq!(response.status(), 201);

    let created: serde_json::Value = response.json().await.unwrap();
    let agent_id = created["agent_id"].as_str().unwrap();
    let agent_name = created["name"].as_str().unwrap();
    assert_eq!(agent_name, "test-agent");

    // 3. GET /api/agents should now return 1 agent
    let response = client.get("/api/agents").await;
    assert_eq!(response.status(), 200);
    let agents: serde_json::Value = response.json().await.unwrap();
    assert_eq!(agents.as_array().unwrap().len(), 1);

    // 4. GET /api/agents/:id should return the specific agent
    let response = client.get(&format!("/api/agents/{agent_id}")).await;
    assert_eq!(response.status(), 200);

    let agent: serde_json::Value = response.json().await.unwrap();
    assert_eq!(agent["name"], "test-agent");
    assert_eq!(agent["kind"], "web_chat");
    assert_eq!(agent["model"], "claude-sonnet-4");

    // 5. POST heartbeat
    let response = client
        .post_json(
            &format!("/api/agents/{agent_id}/heartbeat"),
            &serde_json::json!({}),
        )
        .await;
    assert_eq!(response.status(), 200);

    // 6. DELETE /api/agents/:id to unregister
    let response = client.delete(&format!("/api/agents/{agent_id}")).await;
    assert_eq!(response.status(), 204);

    // 7. GET /api/agents should return empty list again
    let response = client.get("/api/agents").await;
    assert_eq!(response.status(), 200);
    let agents: serde_json::Value = response.json().await.unwrap();
    assert_eq!(agents.as_array().unwrap().len(), 0);

    // 8. GET /api/agents/:id should return 404
    let response = client.get(&format!("/api/agents/{agent_id}")).await;
    assert_eq!(response.status(), 404);
}

#[tokio::test]
async fn test_api_events_endpoint() {
    let server = FullTestServer::new().await;
    let client = server.client();

    // Register an agent to generate some events
    let create_request = serde_json::json!({
        "name": "event-test-agent",
        "kind": "terminal_chat",
        "model": null
    });

    client.post_json("/api/agents", &create_request).await;

    // Get events
    let response = client.get("/api/events").await;

    assert_eq!(response.status(), 200);

    let events: serde_json::Value = response.json().await.unwrap();
    let events_array = events.as_array().unwrap();

    // Should have at least 2 events: kernel startup + agent registration
    assert!(events_array.len() >= 2);

    // Check the most recent event is AgentRegistered
    let most_recent = &events_array[0];
    assert_eq!(most_recent["payload"]["type"], "AgentRegistered");
    assert_eq!(most_recent["payload"]["name"], "event-test-agent");
}

#[tokio::test]
async fn test_invalid_agent_id_handling() {
    let server = FullTestServer::new().await;
    let client = server.client();

    // GET with invalid UUID
    let response = client.get("/api/agents/invalid-uuid").await;
    assert_eq!(response.status(), 400);

    // DELETE with invalid UUID
    let response = client.delete("/api/agents/invalid-uuid").await;
    assert_eq!(response.status(), 400);

    // Heartbeat with invalid UUID
    let response = client
        .post_json("/api/agents/invalid-uuid/heartbeat", &serde_json::json!({}))
        .await;
    assert_eq!(response.status(), 400);
}

// ─── New edge case tests ─────────────────────────────────

#[tokio::test]
async fn test_double_delete_agent_is_idempotent() {
    let server = FullTestServer::new().await;
    let client = server.client();

    // Register an agent
    let resp = client
        .post_json(
            "/api/agents",
            &serde_json::json!({"name": "double-del", "kind": "api_client"}),
        )
        .await;
    let created: serde_json::Value = resp.json().await.unwrap();
    let agent_id = created["agent_id"].as_str().unwrap();

    // First delete should succeed
    let resp = client.delete(&format!("/api/agents/{agent_id}")).await;
    assert_eq!(resp.status(), 204);

    // Second delete should not return 500 (idempotent — 204 is acceptable)
    let resp = client.delete(&format!("/api/agents/{agent_id}")).await;
    assert!(
        !resp.status().is_server_error(),
        "Double delete should not cause a server error, got {}",
        resp.status()
    );

    // Agent should definitely not be found via GET
    let resp = client.get(&format!("/api/agents/{agent_id}")).await;
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn test_heartbeat_nonexistent_agent_returns_404() {
    let server = FullTestServer::new().await;
    let client = server.client();

    // Valid UUID format but agent doesn't exist
    let fake_id = "00000000-0000-0000-0000-000000000000";
    let resp = client
        .post_json(
            &format!("/api/agents/{fake_id}/heartbeat"),
            &serde_json::json!({}),
        )
        .await;
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn test_concurrent_agent_register_delete_consistency() {
    let server = FullTestServer::new().await;
    let client = server.client();

    // Register 20 agents concurrently
    let mut handles = Vec::new();
    for i in 0..20 {
        let c = client.clone();
        handles.push(tokio::spawn(async move {
            c.post_json(
                "/api/agents",
                &serde_json::json!({
                    "name": format!("concurrent-{i}"),
                    "kind": "api_client"
                }),
            )
            .await
        }));
    }

    let results = futures::future::join_all(handles).await;
    // All should succeed with 201
    for result in &results {
        let resp = result.as_ref().expect("task should not panic");
        assert_eq!(resp.status(), 201);
    }

    // Verify final count is exactly 20
    let resp = client.get("/api/agents").await;
    let agents: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(agents.as_array().unwrap().len(), 20);
}

#[tokio::test]
async fn test_concurrent_http_requests_all_succeed() {
    let server = FullTestServer::new().await;
    let client = server.client();

    // 50 concurrent GET requests
    let responses = client.get_concurrent("/api/status", 50).await;
    assert_eq!(responses.len(), 50);
    for resp in responses {
        assert_eq!(resp.status(), 200, "All concurrent requests should succeed");
    }
}

#[tokio::test]
async fn test_health_response_time_under_threshold() {
    let server = FullTestServer::new().await;
    let client = server.client();

    let start = std::time::Instant::now();
    let resp = client.get("/api/health").await;
    let duration = start.elapsed();

    assert_eq!(resp.status(), 200);
    assert!(
        duration.as_millis() < 200,
        "Health check took {}ms, expected <200ms",
        duration.as_millis()
    );
}
