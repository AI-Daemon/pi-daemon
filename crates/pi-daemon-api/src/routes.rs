use crate::state::AppState;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use pi_daemon_types::agent::{AgentId, AgentKind};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

// --- Request/Response types ---

#[derive(Deserialize)]
pub struct RegisterAgentRequest {
    pub name: String,
    pub kind: AgentKind,
    pub model: Option<String>,
}

#[derive(Serialize)]
pub struct StatusResponse {
    pub version: String,
    pub uptime_secs: u64,
    pub agent_count: usize,
}

#[derive(Serialize)]
pub struct AgentResponse {
    pub agent_id: String,
    pub name: String,
}

// --- Handlers ---

/// GET /api/status — daemon status + uptime + agent count.
pub async fn get_status(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    Json(StatusResponse {
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime_secs: state.started_at.elapsed().as_secs(),
        agent_count: state.kernel.registry.count(),
    })
}

/// GET /api/agents — list all agents.
pub async fn list_agents(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    Json(state.kernel.registry.list())
}

/// POST /api/agents — register a new agent.
pub async fn register_agent(
    State(state): State<Arc<AppState>>,
    Json(req): Json<RegisterAgentRequest>,
) -> impl IntoResponse {
    let id = state
        .kernel
        .register_agent(req.name.clone(), req.kind, req.model)
        .await;
    (
        StatusCode::CREATED,
        Json(AgentResponse {
            agent_id: id.to_string(),
            name: req.name,
        }),
    )
}

/// GET /api/agents/:agent_id — get a specific agent.
pub async fn get_agent(
    State(state): State<Arc<AppState>>,
    Path(agent_id): Path<String>,
) -> impl IntoResponse {
    let uuid = match Uuid::parse_str(&agent_id) {
        Ok(u) => u,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "Invalid agent ID"})),
            )
                .into_response()
        }
    };
    let id = AgentId(uuid);
    match state.kernel.registry.get(&id) {
        Some(entry) => Json(entry).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Agent not found"})),
        )
            .into_response(),
    }
}

/// DELETE /api/agents/:agent_id — unregister an agent.
pub async fn unregister_agent(
    State(state): State<Arc<AppState>>,
    Path(agent_id): Path<String>,
) -> impl IntoResponse {
    let uuid = match Uuid::parse_str(&agent_id) {
        Ok(u) => u,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };
    let id = AgentId(uuid);
    state
        .kernel
        .unregister_agent(&id, "API request".to_string())
        .await;
    StatusCode::NO_CONTENT.into_response()
}

/// POST /api/agents/:agent_id/heartbeat — record heartbeat.
pub async fn agent_heartbeat(
    State(state): State<Arc<AppState>>,
    Path(agent_id): Path<String>,
) -> impl IntoResponse {
    let uuid = match Uuid::parse_str(&agent_id) {
        Ok(u) => u,
        Err(_) => return StatusCode::BAD_REQUEST,
    };
    let id = AgentId(uuid);
    match state.kernel.registry.heartbeat(&id) {
        Ok(_) => StatusCode::OK,
        Err(_) => StatusCode::NOT_FOUND,
    }
}

/// GET /api/events — get recent event history.
pub async fn get_events(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let events = state.kernel.event_bus.history(100).await;
    Json(events)
}

/// GET /api/health — simple health check.
pub async fn health_check() -> impl IntoResponse {
    Json(serde_json::json!({"status": "ok"}))
}

/// POST /api/shutdown — graceful shutdown.
pub async fn shutdown(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    tracing::info!("Shutdown requested via API");

    // Trigger graceful shutdown
    state.shutdown_notify.notify_one();

    Json(serde_json::json!({
        "message": "Shutdown initiated"
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;
    use pi_daemon_kernel::PiDaemonKernel;
    use pi_daemon_types::config::DaemonConfig;
    use std::sync::Arc;

    fn test_state() -> Arc<AppState> {
        let kernel = Arc::new(PiDaemonKernel::new());
        let config = DaemonConfig::default();
        Arc::new(AppState::new(kernel, config))
    }

    #[tokio::test]
    async fn test_get_status() {
        let state = test_state();
        let response = get_status(State(state.clone())).await.into_response();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_health_check() {
        let response = health_check().await.into_response();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_register_agent() {
        let state = test_state();
        let req = RegisterAgentRequest {
            name: "test-agent".to_string(),
            kind: AgentKind::WebChat,
            model: Some("test-model".to_string()),
        };

        let response = register_agent(State(state.clone()), Json(req))
            .await
            .into_response();
        assert_eq!(response.status(), StatusCode::CREATED);
    }

    #[tokio::test]
    async fn test_get_agent_invalid_id() {
        let state = test_state();
        let response = get_agent(State(state), Path("invalid-uuid".to_string()))
            .await
            .into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_get_agent_not_found() {
        let state = test_state();
        let fake_id = AgentId::new().to_string();
        let response = get_agent(State(state), Path(fake_id)).await.into_response();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
