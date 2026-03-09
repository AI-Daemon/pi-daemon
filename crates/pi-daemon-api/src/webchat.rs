use axum::http::StatusCode;
use axum::response::{Html, IntoResponse};

/// GET / — serve the webchat page.
pub async fn webchat_page() -> impl IntoResponse {
    // For now, return a simple placeholder
    // Issue #9 will implement the full embedded SPA
    let html = r#"
<!DOCTYPE html>
<html>
<head>
    <title>pi-daemon webchat</title>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
</head>
<body>
    <h1>pi-daemon webchat</h1>
    <p>Webchat UI placeholder. Full implementation in issue #9.</p>
    <p>Status: <span id="status">Checking...</span></p>
    <script>
        fetch('/api/health')
            .then(r => r.json())
            .then(data => {
                document.getElementById('status').textContent = data.status === 'ok' ? 'Connected' : 'Error';
            })
            .catch(() => {
                document.getElementById('status').textContent = 'Connection failed';
            });
    </script>
</body>
</html>
"#.trim();

    (StatusCode::OK, Html(html))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_webchat_page_returns_html() {
        let response = webchat_page().await.into_response();
        assert_eq!(response.status(), StatusCode::OK);

        // Should have HTML content type (axum sets this automatically for Html response)
        let content_type = response.headers().get("content-type");
        assert!(content_type.is_some());
    }
}
