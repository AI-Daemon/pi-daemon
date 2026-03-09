//! Integration tests for test-utils crate functionality.

use pi_daemon_test_utils::{assert_json_ok, assert_status, TestClient, TestKernel, TestServer};

#[test]
fn test_kernel_isolation_and_cleanup() {
    let kernel1 = TestKernel::new();
    let kernel2 = TestKernel::new();

    // Each kernel should have a different directory
    assert_ne!(kernel1.data_dir, kernel2.data_dir);

    // Both directories should exist
    assert!(kernel1.data_dir.exists());
    assert!(kernel2.data_dir.exists());

    // Directories should be writable
    let test_file1 = kernel1.data_dir.join("test1.txt");
    std::fs::write(&test_file1, "test1").expect("Should be able to write to test dir");
    assert!(test_file1.exists());

    let test_file2 = kernel2.data_dir.join("test2.txt");
    std::fs::write(&test_file2, "test2").expect("Should be able to write to test dir");
    assert!(test_file2.exists());

    // Files should not interfere with each other
    assert!(!kernel1.data_dir.join("test2.txt").exists());
    assert!(!kernel2.data_dir.join("test1.txt").exists());
}

#[tokio::test]
async fn test_server_endpoints_respond_correctly() {
    let server = TestServer::new().await;
    let client = TestClient::new(&server.base_url);

    // Test health endpoint
    let health_resp = client.get("/api/health").await;
    assert_status!(health_resp, 200);

    let health_text = health_resp
        .text()
        .await
        .expect("Should get health response");
    let health_json: serde_json::Value =
        serde_json::from_str(&health_text).expect("Health response should be valid JSON");
    assert_eq!(health_json["status"], "ok");

    // Test status endpoint
    let status_resp = client.get("/api/status").await;
    assert_status!(status_resp, 200);

    let status_text = status_resp
        .text()
        .await
        .expect("Should get status response");
    let status_json: serde_json::Value =
        serde_json::from_str(&status_text).expect("Status response should be valid JSON");

    assert_eq!(status_json["status"], "ok");
    assert_eq!(status_json["version"], "test");
    assert!(status_json["uptime_secs"].is_number());
    assert!(status_json["agent_count"].is_number());
}

#[tokio::test]
async fn test_client_error_handling() {
    let server = TestServer::new().await;
    let client = TestClient::new(&server.base_url);

    // Test 404 endpoint
    let resp = client.get("/nonexistent").await;
    assert_eq!(resp.status().as_u16(), 404);
}

#[tokio::test]
async fn test_multiple_concurrent_servers() {
    let server1 = TestServer::new().await;
    let server2 = TestServer::new().await;

    // Each server should have a different port
    assert_ne!(server1.port, server2.port);

    let client1 = TestClient::new(&server1.base_url);
    let client2 = TestClient::new(&server2.base_url);

    // Both servers should respond independently
    let resp1 = client1.get("/api/health").await;
    let resp2 = client2.get("/api/health").await;

    assert_eq!(resp1.status().as_u16(), 200);
    assert_eq!(resp2.status().as_u16(), 200);
}

#[tokio::test]
async fn test_json_assertion_macros() {
    let server = TestServer::new().await;
    let client = TestClient::new(&server.base_url);

    let resp = client.get("/api/status").await;
    let json = assert_json_ok!(resp, "status");

    // Macro should return the parsed JSON
    assert_eq!(json["status"], "ok");
    assert_eq!(json["version"], "test");
}
