use axum::{
    extract::{rejection::JsonRejection, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};

use crate::{
    http::response::ApiResponse,
    models::rollback::{RollbackExecuteRequest, RollbackPreviewRequest, RollbackSessionsResponse},
    services::rollback::RollbackServiceError,
    state::AppState,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/rollback/sessions", get(list_rollback_sessions))
        .route("/api/rollback/preview", post(rollback_preview))
        .route("/api/rollback/execute", post(rollback_execute))
}

async fn list_rollback_sessions(State(state): State<AppState>) -> Response {
    match state.rollback_service.list_sessions().await {
        Ok(sessions) => (
            StatusCode::OK,
            Json(ApiResponse::success(RollbackSessionsResponse { sessions })),
        )
            .into_response(),
        Err(error) => map_rollback_error(error),
    }
}

async fn rollback_preview(
    State(state): State<AppState>,
    payload: Result<Json<RollbackPreviewRequest>, JsonRejection>,
) -> Response {
    let payload = match payload {
        Ok(Json(payload)) => payload,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::<serde_json::Value>::failure(
                    "invalid_request",
                    "request body is invalid",
                )),
            )
                .into_response();
        }
    };

    match state.rollback_service.preview(payload).await {
        Ok(preview) => (StatusCode::OK, Json(ApiResponse::success(preview))).into_response(),
        Err(error) => map_rollback_error(error),
    }
}

async fn rollback_execute(
    State(state): State<AppState>,
    payload: Result<Json<RollbackExecuteRequest>, JsonRejection>,
) -> Response {
    let mut payload = match payload {
        Ok(Json(payload)) => payload,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::<serde_json::Value>::failure(
                    "invalid_request",
                    "request body is invalid",
                )),
            )
                .into_response();
        }
    };

    let normalized_key = payload.idempotency_key.trim();
    if normalized_key.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::<serde_json::Value>::failure(
                "invalid_request",
                "idempotency_key must not be empty",
            )),
        )
            .into_response();
    }

    if normalized_key.len() > 128 {
        return (
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::<serde_json::Value>::failure(
                "invalid_request",
                "idempotency_key must be at most 128 characters",
            )),
        )
            .into_response();
    }

    payload.idempotency_key = normalized_key.to_string();

    match state.rollback_service.execute(payload).await {
        Ok(response) => (StatusCode::ACCEPTED, Json(ApiResponse::success(response))).into_response(),
        Err(error) => map_rollback_error(error),
    }
}

fn map_rollback_error(error: RollbackServiceError) -> Response {
    match error {
        RollbackServiceError::SessionNotFound => (
            StatusCode::NOT_FOUND,
            Json(ApiResponse::<serde_json::Value>::failure(
                "session_not_found",
                "rollback session not found",
            )),
        )
            .into_response(),
        RollbackServiceError::SessionNotRollbackable => (
            StatusCode::CONFLICT,
            Json(ApiResponse::<serde_json::Value>::failure(
                "session_not_rollbackable",
                "session has no successful operations to rollback",
            )),
        )
            .into_response(),
        RollbackServiceError::IdempotencyConflict => (
            StatusCode::CONFLICT,
            Json(ApiResponse::<serde_json::Value>::failure(
                "idempotency_conflict",
                "idempotency key has already been used with a different request",
            )),
        )
            .into_response(),
        RollbackServiceError::Preflight => (
            StatusCode::CONFLICT,
            Json(ApiResponse::<serde_json::Value>::failure(
                "preflight_failed",
                "rollback preflight validation failed",
            )),
        )
            .into_response(),
        RollbackServiceError::Internal(message) => {
            tracing::error!(error = %message, "rollback service internal error");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ApiResponse::<serde_json::Value>::failure(
                    "internal_error",
                    "internal server error",
                )),
            )
                .into_response()
        }
    }
}
