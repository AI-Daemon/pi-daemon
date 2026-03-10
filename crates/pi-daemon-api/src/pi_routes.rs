//! API routes for managed Pi process control.

use crate::state::AppState;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use std::sync::Arc;

/// GET /api/pi/status — managed Pi process status.
pub async fn pi_status(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let manager = state.pi_manager.lock().await;
    match manager.as_ref() {
        Some(pm) => {
            let status = pm.status().await;
            Json(serde_json::json!(status)).into_response()
        }
        None => Json(serde_json::json!({
            "running": false,
            "message": "Pi manager not initialized"
        }))
        .into_response(),
    }
}

/// POST /api/pi/start — manually start managed Pi.
pub async fn pi_start(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let manager = state.pi_manager.lock().await;
    match manager.as_ref() {
        Some(pm) => match pm.start_pi().await {
            Ok(true) => (
                StatusCode::OK,
                Json(serde_json::json!({"message": "Managed Pi started"})),
            )
                .into_response(),
            Ok(false) => (
                StatusCode::OK,
                Json(serde_json::json!({"message": "Pi not available"})),
            )
                .into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": e})),
            )
                .into_response(),
        },
        None => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": "Pi manager not initialized"})),
        )
            .into_response(),
    }
}

/// POST /api/pi/stop — stop managed Pi.
pub async fn pi_stop(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let manager = state.pi_manager.lock().await;
    match manager.as_ref() {
        Some(pm) => match pm.stop().await {
            Ok(()) => Json(serde_json::json!({"message": "Managed Pi stopped"})).into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": e})),
            )
                .into_response(),
        },
        None => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": "Pi manager not initialized"})),
        )
            .into_response(),
    }
}

/// POST /api/pi/restart — restart managed Pi.
pub async fn pi_restart(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let manager = state.pi_manager.lock().await;
    match manager.as_ref() {
        Some(pm) => match pm.restart().await {
            Ok(true) => {
                Json(serde_json::json!({"message": "Managed Pi restarted"})).into_response()
            }
            Ok(false) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Failed to restart Pi"})),
            )
                .into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": e})),
            )
                .into_response(),
        },
        None => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"error": "Pi manager not initialized"})),
        )
            .into_response(),
    }
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
    async fn test_pi_status_no_manager() {
        let state = test_state();
        let response = pi_status(State(state)).await.into_response();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_pi_start_no_manager() {
        let state = test_state();
        let response = pi_start(State(state)).await.into_response();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn test_pi_stop_no_manager() {
        let state = test_state();
        let response = pi_stop(State(state)).await.into_response();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn test_pi_restart_no_manager() {
        let state = test_state();
        let response = pi_restart(State(state)).await.into_response();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }
}
