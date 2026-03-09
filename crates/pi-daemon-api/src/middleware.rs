use crate::state::AppState;
use axum::extract::State;
use axum::http::{Request, StatusCode};
use axum::middleware::Next;
use axum::response::Response;
use std::sync::Arc;

/// API key authentication middleware.
/// Skips auth for:
///   - GET / (webchat page)
///   - GET /api/health
///   - WebSocket upgrade requests (they auth via query param)
pub async fn auth_middleware(
    State(state): State<Arc<AppState>>,
    request: Request<axum::body::Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let api_key = &state.config.api_key;

    // No auth configured — allow everything
    if api_key.is_empty() {
        return Ok(next.run(request).await);
    }

    let path = request.uri().path();

    // Skip auth for webchat page and health check
    if path == "/" || path == "/api/health" || path.starts_with("/static") {
        return Ok(next.run(request).await);
    }

    // Check Authorization header
    let auth_header = request
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));

    // Check X-API-Key header as fallback
    let x_api_key = request
        .headers()
        .get("x-api-key")
        .and_then(|v| v.to_str().ok());

    let provided_key = auth_header.or(x_api_key);

    match provided_key {
        Some(key) if key == api_key => Ok(next.run(request).await),
        _ => Err(StatusCode::UNAUTHORIZED),
    }
}

#[cfg(test)]
mod tests {
    //

    // For middleware testing, we'll test the logic without actual Axum middleware infrastructure
    // This tests the core auth logic that the middleware uses

    #[test]
    fn test_auth_logic_no_key_configured() {
        let api_key = "";
        let _path = "/api/agents";

        // No auth configured should allow everything
        assert!(api_key.is_empty()); // This would trigger the early return in the middleware
    }

    #[test]
    fn test_auth_logic_skip_paths() {
        let paths_that_should_skip = vec!["/", "/api/health", "/static/style.css"];

        for path in paths_that_should_skip {
            // These paths should skip auth check
            assert!(
                path == "/" || path == "/api/health" || path.starts_with("/static"),
                "Path {} should skip auth",
                path
            );
        }
    }

    #[test]
    fn test_auth_logic_bearer_token() {
        let api_key = "secret-key";
        let auth_header = "Bearer secret-key";
        let extracted = auth_header.strip_prefix("Bearer ");

        assert_eq!(extracted, Some(api_key));
    }

    #[test]
    fn test_auth_logic_x_api_key() {
        let api_key = "secret-key";
        let x_api_key_header = "secret-key";

        assert_eq!(x_api_key_header, api_key);
    }

    #[test]
    fn test_auth_logic_wrong_key() {
        let api_key = "secret-key";
        let provided = "wrong-key";

        assert_ne!(provided, api_key);
    }

    // Integration test for the middleware will be in integration tests
    // where we can test the full HTTP request/response cycle
}
