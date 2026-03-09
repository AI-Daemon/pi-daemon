//! API integration tests using actual HTTP requests

use pi_daemon_api::server::build_router;
use pi_daemon_kernel::PiDaemonKernel;
use pi_daemon_types::config::DaemonConfig;
use std::sync::Arc;
use tokio::net::TcpListener;

async fn start_test_server() -> (String, Arc<pi_daemon_api::state::AppState>) {
    let kernel = Arc::new(PiDaemonKernel::new());
    kernel.init().await;

    let config = DaemonConfig {
        listen_addr: "127.0.0.1:0".to_string(),
        ..Default::default()
    };

    let (router, state) = build_router(kernel, config);

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        axum::serve(
            listener,
            router.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .await
        .unwrap();
    });

    (format!("http://{addr}"), state)
}

#[tokio::test]
async fn test_api_health_endpoint() {
    let (base_url, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let response = client
        .get(format!("{base_url}/api/health"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    let json: serde_json::Value = response.json().await.unwrap();
    assert_eq!(json["status"], "ok");
}

#[tokio::test]
async fn test_api_status_endpoint() {
    let (base_url, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    let response = client
        .get(format!("{base_url}/api/status"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    let json: serde_json::Value = response.json().await.unwrap();
    assert!(json["version"].is_string());
    assert!(json["uptime_secs"].is_number());
    assert!(json["agent_count"].is_number());
    assert_eq!(json["agent_count"], 0); // No agents registered initially
}

#[tokio::test]
async fn test_agent_crud_lifecycle() {
    let (base_url, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    // 1. GET /api/agents should return empty list initially
    let response = client
        .get(format!("{base_url}/api/agents"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let agents: serde_json::Value = response.json().await.unwrap();
    assert_eq!(agents.as_array().unwrap().len(), 0);

    // 2. POST /api/agents to create an agent
    let create_request = serde_json::json!({
        "name": "test-agent",
        "kind": "web_chat",
        "model": "claude-sonnet-4"
    });

    let response = client
        .post(format!("{base_url}/api/agents"))
        .json(&create_request)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 201);

    let created: serde_json::Value = response.json().await.unwrap();
    let agent_id = created["agent_id"].as_str().unwrap();
    let agent_name = created["name"].as_str().unwrap();
    assert_eq!(agent_name, "test-agent");

    // 3. GET /api/agents should now return 1 agent
    let response = client
        .get(format!("{base_url}/api/agents"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let agents: serde_json::Value = response.json().await.unwrap();
    assert_eq!(agents.as_array().unwrap().len(), 1);

    // 4. GET /api/agents/:id should return the specific agent
    let response = client
        .get(format!("{base_url}/api/agents/{agent_id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    let agent: serde_json::Value = response.json().await.unwrap();
    assert_eq!(agent["name"], "test-agent");
    assert_eq!(agent["kind"], "web_chat");
    assert_eq!(agent["model"], "claude-sonnet-4");

    // 5. POST heartbeat
    let response = client
        .post(format!("{base_url}/api/agents/{agent_id}/heartbeat"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);

    // 6. DELETE /api/agents/:id to unregister
    let response = client
        .delete(format!("{base_url}/api/agents/{agent_id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 204);

    // 7. GET /api/agents should return empty list again
    let response = client
        .get(format!("{base_url}/api/agents"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 200);
    let agents: serde_json::Value = response.json().await.unwrap();
    assert_eq!(agents.as_array().unwrap().len(), 0);

    // 8. GET /api/agents/:id should return 404
    let response = client
        .get(format!("{base_url}/api/agents/{agent_id}"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 404);
}

#[tokio::test]
async fn test_api_events_endpoint() {
    let (base_url, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    // Register an agent to generate some events
    let create_request = serde_json::json!({
        "name": "event-test-agent",
        "kind": "terminal_chat",
        "model": null
    });

    client
        .post(format!("{base_url}/api/agents"))
        .json(&create_request)
        .send()
        .await
        .unwrap();

    // Get events
    let response = client
        .get(format!("{base_url}/api/events"))
        .send()
        .await
        .unwrap();

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
    let (base_url, _state) = start_test_server().await;
    let client = reqwest::Client::new();

    // GET with invalid UUID
    let response = client
        .get(format!("{base_url}/api/agents/invalid-uuid"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 400);

    // DELETE with invalid UUID
    let response = client
        .delete(format!("{base_url}/api/agents/invalid-uuid"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 400);

    // Heartbeat with invalid UUID
    let response = client
        .post(format!("{base_url}/api/agents/invalid-uuid/heartbeat"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 400);
}
