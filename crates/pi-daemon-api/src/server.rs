use crate::openai_compat;
use crate::routes;
use crate::state::AppState;
use crate::webchat;
use crate::ws;
use axum::Router;
use pi_daemon_kernel::PiDaemonKernel;
use pi_daemon_types::config::DaemonConfig;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tower::limit::ConcurrencyLimitLayer;
use tower_http::compression::CompressionLayer;
use tower_http::cors::CorsLayer;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;
use tracing::info;

/// Maximum number of in-flight requests the server will handle concurrently.
/// Requests beyond this limit are queued (backpressure). This prevents the
/// tokio runtime from being overwhelmed by unbounded concurrent handlers.
const MAX_CONCURRENT_REQUESTS: usize = 256;

/// HTTP request timeout. If a request (including response body) takes longer
/// than this, the connection is dropped with 408 Request Timeout. This prevents
/// stalled connections from accumulating under load.
const HTTP_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Build the full API router.
pub fn build_router(kernel: Arc<PiDaemonKernel>, config: DaemonConfig) -> (Router, Arc<AppState>) {
    let state = Arc::new(AppState::new(kernel, config));

    let api_routes = Router::new()
        .route("/api/status", axum::routing::get(routes::get_status))
        .route("/api/agents", axum::routing::get(routes::list_agents))
        .route("/api/agents", axum::routing::post(routes::register_agent))
        .route(
            "/api/agents/{agent_id}",
            axum::routing::get(routes::get_agent),
        )
        .route(
            "/api/agents/{agent_id}",
            axum::routing::delete(routes::unregister_agent),
        )
        .route(
            "/api/agents/{agent_id}/heartbeat",
            axum::routing::post(routes::agent_heartbeat),
        )
        .route("/api/events", axum::routing::get(routes::get_events))
        .route("/api/health", axum::routing::get(routes::health_check))
        .route("/api/shutdown", axum::routing::post(routes::shutdown))
        .route("/ws/{agent_id}", axum::routing::get(ws::ws_upgrade))
        .route("/v1/models", axum::routing::get(openai_compat::models))
        .route(
            "/v1/chat/completions",
            axum::routing::post(openai_compat::chat_completions),
        );

    // Webchat static files
    let webchat_routes = Router::new().route("/", axum::routing::get(webchat::webchat_page));

    let cors = CorsLayer::new()
        .allow_origin(tower_http::cors::Any)
        .allow_methods(tower_http::cors::Any)
        .allow_headers(tower_http::cors::Any);

    // Layer order matters (outermost first):
    // 1. ConcurrencyLimit — bounds in-flight requests to prevent runtime exhaustion
    // 2. Timeout — drops requests that take too long, freeing concurrency slots
    // 3. Compression — applied to response bodies
    // 4. CORS — adds headers
    // 5. Trace — logs request/response
    let router = Router::new()
        .merge(api_routes)
        .merge(webchat_routes)
        .layer(CompressionLayer::new())
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .layer(TimeoutLayer::with_status_code(
            axum::http::StatusCode::REQUEST_TIMEOUT,
            HTTP_REQUEST_TIMEOUT,
        ))
        .layer(ConcurrencyLimitLayer::new(MAX_CONCURRENT_REQUESTS))
        .with_state(state.clone());

    (router, state)
}

/// Run the daemon server.
pub async fn run_daemon(kernel: Arc<PiDaemonKernel>, config: DaemonConfig) -> anyhow::Result<()> {
    let addr: SocketAddr = config.listen_addr.parse()?;
    let (router, state) = build_router(kernel, config);

    info!("pi-daemon listening on http://{addr}");

    let socket = tokio::net::TcpSocket::new_v4()?;

    // Enable SO_REUSEADDR so the port can be re-bound quickly after a crash
    // or restart (avoids TIME_WAIT blocking).
    socket.set_reuseaddr(true)?;

    socket.bind(addr)?;
    let listener = socket.listen(1024)?;

    axum::serve(
        listener,
        router.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(async move {
        state.shutdown_notify.notified().await;
        info!("Graceful shutdown initiated");
    })
    .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use pi_daemon_types::config::DaemonConfig;

    #[test]
    fn test_build_router_creates_state_and_router() {
        let kernel = Arc::new(PiDaemonKernel::new());
        let config = DaemonConfig::default();

        let (_router, state) = build_router(kernel.clone(), config.clone());

        // Verify state is properly initialized
        assert_eq!(state.config.listen_addr, config.listen_addr);
        assert_eq!(state.config.default_model, config.default_model);
    }

    #[test]
    fn test_server_constants_are_reasonable() {
        // Use runtime values to avoid clippy::assertions_on_constants
        let max_concurrent: usize = MAX_CONCURRENT_REQUESTS;
        let timeout_secs: u64 = HTTP_REQUEST_TIMEOUT.as_secs();

        assert!(max_concurrent >= 64, "Concurrency limit too low");
        assert!(max_concurrent <= 4096, "Concurrency limit too high");
        assert!(timeout_secs >= 5, "Timeout too short");
        assert!(timeout_secs <= 120, "Timeout too long");
    }
}
