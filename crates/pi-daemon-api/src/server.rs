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
use tower_http::compression::CompressionLayer;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing::info;

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

    let router = Router::new()
        .merge(api_routes)
        .merge(webchat_routes)
        .layer(CompressionLayer::new())
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state.clone());

    (router, state)
}

/// Run the daemon server.
pub async fn run_daemon(kernel: Arc<PiDaemonKernel>, config: DaemonConfig) -> anyhow::Result<()> {
    let addr: SocketAddr = config.listen_addr.parse()?;
    let (router, state) = build_router(kernel, config);

    info!("pi-daemon listening on http://{addr}");

    // Create TCP listener with SO_REUSEADDR for better port reuse
    let listener = {
        let socket = socket2::Socket::new(
            socket2::Domain::for_address(addr),
            socket2::Type::STREAM,
            Some(socket2::Protocol::TCP),
        )?;
        socket.set_reuse_address(true)?;
        socket.bind(&addr.into())?;
        socket.listen(1024)?;
        socket.set_nonblocking(true)?;
        tokio::net::TcpListener::from_std(socket.into())?
    };

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

        // Router should be non-null (can't easily test much more without integration)
        // The actual routes are tested in integration tests
    }
}
