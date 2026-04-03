use axum::{
    extract::{rejection::JsonRejection, Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::path::{Component, Path as StdPath};
use uuid::Uuid;

use crate::{
    http::response::ApiResponse,
    services::scan::{NewScanRequest, ScanServiceError},
    state::AppState,
};

#[derive(Debug, Deserialize)]
pub struct CreateScanRequest {
    pub media_type: String,
    pub base_path: String,
}

#[derive(Debug, Serialize)]
pub struct ExpireScanResponse {
    pub scan_id: Uuid,
    pub expired: bool,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/normalize/scan", post(create_scan))
        .route(
            "/api/normalize/scan/:scan_id",
            get(get_scan).delete(expire_scan),
        )
}

async fn create_scan(
    State(state): State<AppState>,
    payload: Result<Json<CreateScanRequest>, JsonRejection>,
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

    if !is_valid_media_type(&payload.media_type) {
        return (
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::<serde_json::Value>::failure(
                "invalid_request",
                "media_type must be one of: tv, movie",
            )),
        )
            .into_response();
    }

    if payload.base_path.trim().is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::<serde_json::Value>::failure(
                "invalid_request",
                "base_path must not be empty",
            )),
        )
            .into_response();
    }

    let allowed_root = match payload.media_type.as_str() {
        "tv" => &state.config.tv_base_path,
        "movie" => &state.config.movies_base_path,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::<serde_json::Value>::failure(
                    "invalid_request",
                    "media_type must be one of: tv, movie",
                )),
            )
                .into_response();
        }
    };

    let requested_base = StdPath::new(&payload.base_path);
    if !is_allowed_base_path(requested_base, allowed_root) {
        return (
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::<serde_json::Value>::failure(
                "invalid_request",
                "base_path must be under configured media root",
            )),
        )
            .into_response();
    }

    match state
        .scan_service
        .create_scan(NewScanRequest {
            media_type: payload.media_type,
            base_path: payload.base_path,
            ttl_minutes: state.config.scan_ttl_minutes,
        })
        .await
    {
        Ok(scan) => (StatusCode::CREATED, Json(ApiResponse::success(scan))).into_response(),
        Err(error) => map_service_error(error),
    }
}

async fn get_scan(State(state): State<AppState>, Path(scan_id): Path<String>) -> Response {
    let scan_id = match parse_scan_id(&scan_id) {
        Ok(scan_id) => scan_id,
        Err(response) => return response,
    };

    match state.scan_service.get_scan(scan_id).await {
        Ok(Some(scan)) => (StatusCode::OK, Json(ApiResponse::success(scan))).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(ApiResponse::<serde_json::Value>::failure(
                "scan_not_found",
                format!("scan session {scan_id} not found"),
            )),
        )
            .into_response(),
        Err(error) => map_service_error(error),
    }
}

async fn expire_scan(State(state): State<AppState>, Path(scan_id): Path<String>) -> Response {
    let scan_id = match parse_scan_id(&scan_id) {
        Ok(scan_id) => scan_id,
        Err(response) => return response,
    };

    match state.scan_service.expire_scan(scan_id).await {
        Ok(expired) => (
            StatusCode::OK,
            Json(ApiResponse::success(ExpireScanResponse {
                scan_id,
                expired,
            })),
        )
            .into_response(),
        Err(error) => map_service_error(error),
    }
}

fn is_valid_media_type(value: &str) -> bool {
    matches!(value, "tv" | "movie")
}

fn map_service_error(error: ScanServiceError) -> Response {
    match error {
        ScanServiceError::NotFound => (
            StatusCode::NOT_FOUND,
            Json(ApiResponse::<serde_json::Value>::failure(
                "scan_not_found",
                "scan session not found",
            )),
        )
            .into_response(),
        ScanServiceError::Internal(message) => {
            tracing::error!(error = %message, "scan service internal error");
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

fn parse_scan_id(raw: &str) -> Result<Uuid, Response> {
    match Uuid::parse_str(raw) {
        Ok(scan_id) => Ok(scan_id),
        Err(_) => Err((
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::<serde_json::Value>::failure(
                "invalid_request",
                "scan_id must be a valid UUID",
            )),
        )
            .into_response()),
    }
}

fn is_allowed_base_path(candidate: &StdPath, allowed_root: &StdPath) -> bool {
    if !candidate.is_absolute() {
        return false;
    }

    if candidate
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return false;
    }

    let canonical_root = match allowed_root.canonicalize() {
        Ok(path) => path,
        Err(_) => return false,
    };

    let canonical_candidate = match candidate.canonicalize() {
        Ok(path) => path,
        Err(_) => return false,
    };

    canonical_candidate.starts_with(canonical_root)
}
