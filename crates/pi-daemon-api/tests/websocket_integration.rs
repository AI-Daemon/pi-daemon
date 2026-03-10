//! WebSocket integration tests using real WebSocket connections

use futures::{SinkExt, StreamExt};
use pi_daemon_api::ws::{ClientMessage, ServerMessage};
use pi_daemon_test_utils::FullTestServer;
use pi_daemon_types::config::DaemonConfig;
use std::time::Duration;
use tokio_tungstenite::{connect_async, tungstenite::Message};

#[tokio::test]
async fn test_websocket_ping_pong() {
    let server = FullTestServer::new().await;
    let ws_url = server.ws_url("test-agent");

    let (ws_stream, _) = connect_async(&ws_url)
        .await
        .expect("Failed to connect to WebSocket");
    let (mut ws_sender, mut ws_receiver) = ws_stream.split();

    let ping_msg = ClientMessage::Ping;
    let ping_json = serde_json::to_string(&ping_msg).unwrap();
    ws_sender.send(Message::Text(ping_json)).await.unwrap();

    let response = ws_receiver.next().await.unwrap().unwrap();
    if let Message::Text(text) = response {
        let server_msg: ServerMessage = serde_json::from_str(&text).unwrap();
        assert!(matches!(server_msg, ServerMessage::Pong));
    } else {
        panic!("Expected text message, got: {:?}", response);
    }
}

#[tokio::test]
async fn test_websocket_chat_message() {
    let server = FullTestServer::new().await;
    let ws_url = server.ws_url("test-agent");

    let (ws_stream, _) = connect_async(&ws_url)
        .await
        .expect("Failed to connect to WebSocket");
    let (mut ws_sender, mut ws_receiver) = ws_stream.split();

    let chat_msg = ClientMessage::Message {
        content: "Hello, world!".to_string(),
    };
    let chat_json = serde_json::to_string(&chat_msg).unwrap();
    ws_sender.send(Message::Text(chat_json)).await.unwrap();

    let mut received_messages = Vec::new();

    let timeout = Duration::from_secs(2);
    let start = tokio::time::Instant::now();

    while received_messages.len() < 3 && start.elapsed() < timeout {
        if let Ok(Some(Ok(Message::Text(text)))) =
            tokio::time::timeout(Duration::from_millis(100), ws_receiver.next()).await
        {
            let server_msg: ServerMessage = serde_json::from_str(&text).unwrap();
            received_messages.push(server_msg);
        }
    }

    assert_eq!(received_messages.len(), 3);

    assert!(matches!(
        received_messages[0],
        ServerMessage::Typing { ref state, tool_name: None } if state == "start"
    ));

    assert!(matches!(
        received_messages[1],
        ServerMessage::Typing { ref state, tool_name: None } if state == "stop"
    ));

    if let ServerMessage::Response {
        content,
        input_tokens,
        output_tokens,
    } = &received_messages[2]
    {
        assert!(content.contains("Echo from agent test-agent"));
        assert!(content.contains("Hello, world!"));
        assert!(*input_tokens > 0);
        assert!(*output_tokens > 0);
    } else {
        panic!("Expected response message, got: {:?}", received_messages[2]);
    }
}

#[tokio::test]
async fn test_websocket_set_model() {
    let server = FullTestServer::new().await;
    let ws_url = server.ws_url("test-agent");

    let (ws_stream, _) = connect_async(&ws_url)
        .await
        .expect("Failed to connect to WebSocket");
    let (mut ws_sender, mut ws_receiver) = ws_stream.split();

    let model_msg = ClientMessage::SetModel {
        model: "claude-sonnet-4".to_string(),
    };
    let model_json = serde_json::to_string(&model_msg).unwrap();
    ws_sender.send(Message::Text(model_json)).await.unwrap();

    let timeout_result = tokio::time::timeout(Duration::from_millis(200), ws_receiver.next()).await;
    assert!(timeout_result.is_err());
}

#[tokio::test]
async fn test_websocket_malformed_message() {
    let server = FullTestServer::new().await;
    let ws_url = server.ws_url("test-agent");

    let (ws_stream, _) = connect_async(&ws_url)
        .await
        .expect("Failed to connect to WebSocket");
    let (mut ws_sender, mut ws_receiver) = ws_stream.split();

    // Send malformed JSON
    ws_sender
        .send(Message::Text("invalid json".to_string()))
        .await
        .unwrap();

    // Connection should remain open — send a valid ping to confirm
    let ping_msg = ClientMessage::Ping;
    let ping_json = serde_json::to_string(&ping_msg).unwrap();
    ws_sender.send(Message::Text(ping_json)).await.unwrap();

    let response = ws_receiver.next().await.unwrap().unwrap();
    if let Message::Text(text) = response {
        let server_msg: ServerMessage = serde_json::from_str(&text).unwrap();
        assert!(matches!(server_msg, ServerMessage::Pong));
    } else {
        panic!("Expected text message, got: {:?}", response);
    }
}

#[tokio::test]
async fn test_websocket_auth_required() {
    let config = DaemonConfig {
        api_key: "test-secret-key".to_string(),
        ..Default::default()
    };

    let server = FullTestServer::with_config(config).await;

    // Test 1: No API key — should fail
    let ws_url_no_auth = server.ws_url("test-agent");
    let result = connect_async(&ws_url_no_auth).await;
    assert!(
        result.is_err() || {
            if let Ok((_ws_stream, response)) = result {
                response.status() == 401 || {
                    drop(_ws_stream);
                    false
                }
            } else {
                true
            }
        }
    );

    // Test 2: Wrong API key — should fail
    let ws_url_wrong = server.ws_url_with_key("test-agent", "wrong-key");
    let result = connect_async(&ws_url_wrong).await;
    assert!(
        result.is_err() || {
            if let Ok((_ws_stream, response)) = result {
                response.status() == 401
            } else {
                true
            }
        }
    );

    // Test 3: Correct API key — should succeed
    let ws_url_correct = server.ws_url_with_key("test-agent", "test-secret-key");
    let result = connect_async(&ws_url_correct).await;
    assert!(result.is_ok());

    if let Ok((ws_stream, _response)) = result {
        let (mut ws_sender, mut ws_receiver) = ws_stream.split();

        let ping_msg = ClientMessage::Ping;
        let ping_json = serde_json::to_string(&ping_msg).unwrap();
        ws_sender.send(Message::Text(ping_json)).await.unwrap();

        let response = ws_receiver.next().await.unwrap().unwrap();
        if let Message::Text(text) = response {
            let server_msg: ServerMessage = serde_json::from_str(&text).unwrap();
            assert!(matches!(server_msg, ServerMessage::Pong));
        }
    }
}

#[tokio::test]
async fn test_websocket_connection_limit() {
    let server = FullTestServer::new().await;

    let mut connections = Vec::new();
    for i in 0..5 {
        let ws_url = server.ws_url(&format!("test-agent-{i}"));
        let result = connect_async(&ws_url).await;
        assert!(result.is_ok(), "Connection {i} should succeed");
        connections.push(result.unwrap());
    }

    // 6th connection — may be rejected or cause oldest to drop
    let ws_url_6th = server.ws_url("test-agent-6");
    let result = connect_async(&ws_url_6th).await;

    if let Ok((ws_stream, response)) = result {
        if response.status() == 429 {
            // Expected: rate limited
        } else {
            // Connection accepted — verify it works
            let (mut ws_sender, mut ws_receiver) = ws_stream.split();
            let ping_msg = ClientMessage::Ping;
            let ping_json = serde_json::to_string(&ping_msg).unwrap();
            ws_sender.send(Message::Text(ping_json)).await.unwrap();

            let response =
                tokio::time::timeout(Duration::from_millis(500), ws_receiver.next()).await;

            if let Ok(Some(Ok(Message::Text(text)))) = response {
                let server_msg: ServerMessage = serde_json::from_str(&text).unwrap();
                assert!(matches!(server_msg, ServerMessage::Pong));
            }
        }
    }

    // Clean up
    for (ws, _) in connections {
        drop(ws);
    }
}

// ─── New edge case tests ─────────────────────────────────

#[tokio::test]
async fn test_websocket_rapid_ping_flood() {
    let server = FullTestServer::new().await;
    let ws_url = server.ws_url("flood-agent");

    let (ws_stream, _) = connect_async(&ws_url)
        .await
        .expect("Failed to connect to WebSocket");
    let (mut ws_sender, mut ws_receiver) = ws_stream.split();

    // Send 50 rapid pings
    let ping_json = serde_json::to_string(&ClientMessage::Ping).unwrap();
    for _ in 0..50 {
        ws_sender
            .send(Message::Text(ping_json.clone()))
            .await
            .unwrap();
    }

    // Collect pongs with a timeout
    let mut pong_count = 0;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);

    while pong_count < 50 && tokio::time::Instant::now() < deadline {
        if let Ok(Some(Ok(Message::Text(text)))) =
            tokio::time::timeout(Duration::from_millis(200), ws_receiver.next()).await
        {
            let server_msg: ServerMessage = serde_json::from_str(&text).unwrap();
            if matches!(server_msg, ServerMessage::Pong) {
                pong_count += 1;
            }
        }
    }

    assert_eq!(pong_count, 50, "Should receive 50 pongs for 50 pings");
}

#[tokio::test]
async fn test_websocket_close_frame_is_clean() {
    let server = FullTestServer::new().await;
    let ws_url = server.ws_url("close-test");

    let (ws_stream, _) = connect_async(&ws_url)
        .await
        .expect("Failed to connect to WebSocket");
    let (mut ws_sender, _ws_receiver) = ws_stream.split();

    // Send a close frame — should not cause server panic
    ws_sender.close().await.unwrap();

    // Server should handle the close gracefully — verify by connecting again
    tokio::time::sleep(Duration::from_millis(100)).await;

    let ws_url2 = server.ws_url("close-test-2");
    let result = connect_async(&ws_url2).await;
    assert!(
        result.is_ok(),
        "Server should still accept connections after a client closes"
    );
}

#[tokio::test]
async fn test_websocket_multiple_agents_independent() {
    let server = FullTestServer::new().await;

    // Connect two agents
    let (ws1, _) = connect_async(server.ws_url("agent-a"))
        .await
        .expect("Failed to connect agent-a");
    let (ws2, _) = connect_async(server.ws_url("agent-b"))
        .await
        .expect("Failed to connect agent-b");

    let (mut tx1, mut rx1) = ws1.split();
    let (mut tx2, mut rx2) = ws2.split();

    // Send ping on agent-a
    let ping_json = serde_json::to_string(&ClientMessage::Ping).unwrap();
    tx1.send(Message::Text(ping_json.clone())).await.unwrap();

    // Agent-a should get pong
    let resp = rx1.next().await.unwrap().unwrap();
    if let Message::Text(text) = resp {
        let msg: ServerMessage = serde_json::from_str(&text).unwrap();
        assert!(matches!(msg, ServerMessage::Pong));
    }

    // Agent-b should NOT have received anything (independent connections)
    let timeout_result = tokio::time::timeout(Duration::from_millis(100), rx2.next()).await;
    assert!(
        timeout_result.is_err(),
        "Agent-b should not receive agent-a's pong"
    );

    // Send ping on agent-b
    tx2.send(Message::Text(ping_json)).await.unwrap();
    let resp = rx2.next().await.unwrap().unwrap();
    if let Message::Text(text) = resp {
        let msg: ServerMessage = serde_json::from_str(&text).unwrap();
        assert!(matches!(msg, ServerMessage::Pong));
    }
}
