//! Webchat UI integration tests

use pi_daemon_test_utils::FullTestServer;

#[tokio::test]
async fn test_webchat_page_loads() {
    let server = FullTestServer::new().await;
    let client = server.client();

    let response = client.get("/").await;

    assert_eq!(response.status(), 200);
    assert_eq!(
        response.headers().get("content-type").unwrap(),
        "text/html; charset=utf-8"
    );

    // Should have ETag header for caching
    assert!(response.headers().get("etag").is_some());

    let body = response.text().await.unwrap();

    assert!(body.contains("<!DOCTYPE html>"));
    assert!(body.contains("<html"));
    assert!(body.contains("</html>"));
    assert!(body.contains("PI-DAEMON"));
    assert!(body.contains("x-data=\"app\""));
    assert!(body.contains("Alpine"));
    assert!(body.contains("marked"));
    assert!(body.contains("var(--bg-primary)"));
    assert!(body.len() > 50000);
}

#[tokio::test]
async fn test_webchat_etag_consistency() {
    let server = FullTestServer::new().await;
    let client = server.client();

    let response1 = client.get("/").await;
    let etag1 = response1
        .headers()
        .get("etag")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();

    let response2 = client.get("/").await;
    let etag2 = response2
        .headers()
        .get("etag")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();

    assert_eq!(etag1, etag2);
    assert!(etag1.contains(env!("CARGO_PKG_VERSION")));
}

#[tokio::test]
async fn test_webchat_performance() {
    let server = FullTestServer::new().await;
    let client = server.client();

    let start = std::time::Instant::now();
    let response = client.get("/").await;
    let duration = start.elapsed();

    assert!(
        duration.as_millis() < 500,
        "Webchat should load in <500ms, took {}ms",
        duration.as_millis()
    );
    assert_eq!(response.status(), 200);

    let body = response.text().await.unwrap();
    assert!(body.contains("PI-DAEMON"));
}

#[tokio::test]
async fn test_webchat_static_asset_embedding() {
    let server = FullTestServer::new().await;
    let client = server.client();

    let response = client.get("/").await;
    let body = response.text().await.unwrap();

    // Should contain embedded Alpine.js and marked.js
    assert!(body.contains("Alpine"));
    assert!(body.contains("marked"));

    // Should contain custom app JavaScript
    assert!(body.contains("Alpine.data('app'"));
    assert!(body.contains("Alpine.data('chatPage'"));
    assert!(body.contains("Alpine.data('agentsPage'"));
    assert!(body.contains("Alpine.data('overviewPage'"));
    assert!(body.contains("Alpine.data('settingsPage'"));

    // Should contain CSS variables for theming
    assert!(body.contains("--bg-primary"));
    assert!(body.contains("--text-primary"));

    // Should contain the complete HTML structure
    assert!(body.contains("sidebar"));
    assert!(body.contains("main-content"));
    assert!(body.contains("chat-page"));
}

#[tokio::test]
async fn test_webchat_no_external_requests() {
    let server = FullTestServer::new().await;
    let client = server.client();

    let response = client.get("/").await;
    let body = response.text().await.unwrap();

    // Should not contain any external CDN references
    assert!(!body.contains("unpkg.com"));
    assert!(!body.contains("cdnjs.cloudflare.com"));
    assert!(!body.contains("jsdelivr.net"));
    assert!(!body.contains("googleapis.com"));

    // Should not contain external script/link tags
    assert!(!body.contains("src=\"http"));
    assert!(!body.contains("href=\"http"));

    // All assets should be embedded inline
    assert!(body.contains("<style>"));
    assert!(body.contains("<script"));
    assert!(body.contains("</style>"));
    assert!(body.contains("</script>"));
}

// ─── New edge case tests ─────────────────────────────────

#[tokio::test]
async fn test_webchat_concurrent_loads() {
    let server = FullTestServer::new().await;
    let client = server.client();

    // 10 concurrent page loads should all succeed
    let responses = client.get_concurrent("/", 10).await;
    assert_eq!(responses.len(), 10);
    for resp in responses {
        assert_eq!(resp.status(), 200);
    }
}

#[tokio::test]
async fn test_webchat_content_length_is_substantial() {
    let server = FullTestServer::new().await;
    let client = server.client();

    let response = client.get("/").await;
    let body = response.text().await.unwrap();

    // Webchat with all embedded assets should be substantial
    let size = body.len();
    assert!(
        size > 50_000,
        "Webchat should be >50KB with all assets, got {} bytes",
        size
    );
    assert!(
        size < 5_000_000,
        "Webchat should be <5MB (not bloated), got {} bytes",
        size
    );
}

#[tokio::test]
async fn test_webchat_has_security_relevant_structure() {
    let server = FullTestServer::new().await;
    let client = server.client();

    let response = client.get("/").await;
    let body = response.text().await.unwrap();

    // Should not contain inline event handlers (XSS surface)
    assert!(
        !body.contains("onclick=\""),
        "Should not use inline onclick handlers"
    );
    assert!(
        !body.contains("onerror=\""),
        "Should not use inline onerror handlers"
    );

    // Should have proper charset declaration
    assert!(body.contains("charset"));
}
