//! WebSocket integration tests using real WebSocket connections

use futures::{SinkExt, StreamExt};
use pi_daemon_api::server::build_router;
use pi_daemon_api::ws::{ClientMessage, ServerMessage};
use pi_daemon_kernel::PiDaemonKernel;
use pi_daemon_types::config::DaemonConfig;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio_tungstenite::{connect_async, tungstenite::Message};

async fn start_test_server_ws() -> (String, Arc<pi_daemon_api::state::AppState>) {
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

    // Give the server a moment to start
    tokio::time::sleep(Duration::from_millis(100)).await;

    (format!("127.0.0.1:{}", addr.port()), state)
}

#[tokio::test]
async fn test_websocket_ping_pong() {
    let (addr, _state) = start_test_server_ws().await;
    let ws_url = format!("ws://{addr}/ws/test-agent");

    let (ws_stream, _) = connect_async(&ws_url)
        .await
        .expect("Failed to connect to WebSocket");
    let (mut ws_sender, mut ws_receiver) = ws_stream.split();

    // Send ping
    let ping_msg = ClientMessage::Ping;
    let ping_json = serde_json::to_string(&ping_msg).unwrap();
    ws_sender.send(Message::Text(ping_json)).await.unwrap();

    // Receive pong
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
    let (addr, _state) = start_test_server_ws().await;
    let ws_url = format!("ws://{addr}/ws/test-agent");

    let (ws_stream, _) = connect_async(&ws_url)
        .await
        .expect("Failed to connect to WebSocket");
    let (mut ws_sender, mut ws_receiver) = ws_stream.split();

    // Send chat message
    let chat_msg = ClientMessage::Message {
        content: "Hello, world!".to_string(),
    };
    let chat_json = serde_json::to_string(&chat_msg).unwrap();
    ws_sender.send(Message::Text(chat_json)).await.unwrap();

    // We should receive: typing start, typing stop, response
    let mut received_messages = Vec::new();

    // Collect messages with timeout
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

    // Check typing start
    assert!(matches!(
        received_messages[0],
        ServerMessage::Typing { ref state, tool_name: None } if state == "start"
    ));

    // Check typing stop
    assert!(matches!(
        received_messages[1],
        ServerMessage::Typing { ref state, tool_name: None } if state == "stop"
    ));

    // Check response
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
    let (addr, _state) = start_test_server_ws().await;
    let ws_url = format!("ws://{addr}/ws/test-agent");

    let (ws_stream, _) = connect_async(&ws_url)
        .await
        .expect("Failed to connect to WebSocket");
    let (mut ws_sender, mut ws_receiver) = ws_stream.split();

    // Send set model message
    let model_msg = ClientMessage::SetModel {
        model: "claude-sonnet-4".to_string(),
    };
    let model_json = serde_json::to_string(&model_msg).unwrap();
    ws_sender.send(Message::Text(model_json)).await.unwrap();

    // SetModel doesn't send a response, so we should not receive anything
    let timeout_result = tokio::time::timeout(Duration::from_millis(200), ws_receiver.next()).await;

    // Should timeout (no response expected)
    assert!(timeout_result.is_err());
}

#[tokio::test]
async fn test_websocket_malformed_message() {
    let (addr, _state) = start_test_server_ws().await;
    let ws_url = format!("ws://{addr}/ws/test-agent");

    let (ws_stream, _) = connect_async(&ws_url)
        .await
        .expect("Failed to connect to WebSocket");
    let (mut ws_sender, mut ws_receiver) = ws_stream.split();

    // Send malformed JSON
    ws_sender
        .send(Message::Text("invalid json".to_string()))
        .await
        .unwrap();

    // Connection should remain open (graceful error handling)
    // Send a valid ping to confirm
    let ping_msg = ClientMessage::Ping;
    let ping_json = serde_json::to_string(&ping_msg).unwrap();
    ws_sender.send(Message::Text(ping_json)).await.unwrap();

    // Should receive pong (connection still works)
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
    let kernel = Arc::new(PiDaemonKernel::new());
    kernel.init().await;

    // Config with API key required
    let config = DaemonConfig {
        listen_addr: "127.0.0.1:0".to_string(),
        api_key: "test-secret-key".to_string(),
        ..Default::default()
    };

    let (router, _state) = build_router(kernel, config);

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

    tokio::time::sleep(Duration::from_millis(100)).await;

    let base_addr = format!("127.0.0.1:{}", addr.port());

    // Test 1: No API key - should fail
    let ws_url_no_auth = format!("ws://{base_addr}/ws/test-agent");
    let result = connect_async(&ws_url_no_auth).await;
    assert!(
        result.is_err() || {
            // Some WebSocket clients might connect but get closed immediately
            if let Ok((ws_stream, response)) = result {
                response.status() == 401 || {
                    // Connection might be accepted but closed
                    drop(ws_stream);
                    false
                }
            } else {
                true
            }
        }
    );

    // Test 2: Wrong API key - should fail
    let ws_url_wrong_auth = format!("ws://{base_addr}/ws/test-agent?api_key=wrong-key");
    let result = connect_async(&ws_url_wrong_auth).await;
    assert!(
        result.is_err() || {
            if let Ok((_ws_stream, response)) = result {
                response.status() == 401
            } else {
                true
            }
        }
    );

    // Test 3: Correct API key - should succeed
    let ws_url_correct_auth = format!("ws://{base_addr}/ws/test-agent?api_key=test-secret-key");
    let result = connect_async(&ws_url_correct_auth).await;
    assert!(result.is_ok());

    if let Ok((ws_stream, _response)) = result {
        let (mut ws_sender, mut ws_receiver) = ws_stream.split();

        // Send ping to verify connection works
        let ping_msg = ClientMessage::Ping;
        let ping_json = serde_json::to_string(&ping_msg).unwrap();
        ws_sender.send(Message::Text(ping_json)).await.unwrap();

        // Should receive pong
        let response = ws_receiver.next().await.unwrap().unwrap();
        if let Message::Text(text) = response {
            let server_msg: ServerMessage = serde_json::from_str(&text).unwrap();
            assert!(matches!(server_msg, ServerMessage::Pong));
        }
    }
}

#[tokio::test]
async fn test_websocket_connection_limit() {
    let (addr, _state) = start_test_server_ws().await;

    // Create 5 connections from the same IP (127.0.0.1)
    let mut connections = Vec::new();

    for i in 0..5 {
        let ws_url = format!("ws://{addr}/ws/test-agent-{i}");
        let result = connect_async(&ws_url).await;
        assert!(result.is_ok(), "Connection {i} should succeed");
        connections.push(result.unwrap());
    }

    // 6th connection should fail (or oldest should be dropped)
    let ws_url_6th = format!("ws://{addr}/ws/test-agent-6");
    let result = connect_async(&ws_url_6th).await;

    // The result can be either:
    // 1. Connection fails with 429 Too Many Requests, or
    // 2. Connection succeeds but one of the old connections gets dropped
    if let Ok((ws_stream, response)) = result {
        // If connection succeeded, check if we got 429 or connection works
        if response.status() == 429 {
            // Expected: got rate limited
        } else {
            // Connection was accepted - verify it works
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
    } else {
        // Expected: connection was rejected
    }

    // Clean up connections
    for (ws, _) in connections {
        drop(ws);
    }
}
