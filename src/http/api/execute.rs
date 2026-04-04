use axum::{
    extract::{rejection::JsonRejection, Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use uuid::Uuid;

use crate::{
    http::response::ApiResponse, models::execute::ExecuteNormalizeRequest,
    services::execute::ExecuteServiceError, state::AppState,
};

#[derive(Debug, serde::Deserialize)]
struct PaginationQuery {
    page: Option<usize>,
    page_size: Option<usize>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/normalize/execute", post(execute_normalize))
        .route("/api/normalize/jobs", get(list_jobs))
        .route("/api/normalize/jobs/:job_id", get(get_job_status))
        .route("/api/normalize/jobs/:job_id/entries", get(get_job_entries))
}

async fn execute_normalize(
    State(state): State<AppState>,
    payload: Result<Json<ExecuteNormalizeRequest>, JsonRejection>,
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

    if payload.media_type != "tv" && payload.media_type != "movie" {
        return (
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::<serde_json::Value>::failure(
                "invalid_request",
                "media_type must be one of: tv, movie",
            )),
        )
            .into_response();
    }

    match state.execute_service.execute(payload).await {
        Ok(job) => (StatusCode::ACCEPTED, Json(ApiResponse::success(job))).into_response(),
        Err(error) => map_execute_error(error),
    }
}

async fn get_job_status(State(state): State<AppState>, Path(job_id): Path<String>) -> Response {
    let job_id = match Uuid::parse_str(&job_id) {
        Ok(job_id) => job_id,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::<serde_json::Value>::failure(
                    "invalid_request",
                    "job_id must be a valid UUID",
                )),
            )
                .into_response();
        }
    };

    match state.execute_service.get_job_status(job_id).await {
        Ok(Some(job)) => (StatusCode::OK, Json(ApiResponse::success(job))).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(ApiResponse::<serde_json::Value>::failure(
                "job_not_found",
                "normalize job not found",
            )),
        )
            .into_response(),
        Err(error) => map_execute_error(error),
    }
}

async fn list_jobs(
    State(state): State<AppState>,
    Query(query): Query<PaginationQuery>,
) -> Response {
    let (page, page_size) = match parse_pagination(query) {
        Ok(value) => value,
        Err(response) => return response,
    };

    match state.execute_service.list_jobs(page, page_size).await {
        Ok(jobs) => (StatusCode::OK, Json(ApiResponse::success(jobs))).into_response(),
        Err(error) => map_execute_error(error),
    }
}

async fn get_job_entries(
    State(state): State<AppState>,
    Path(job_id): Path<String>,
    Query(query): Query<PaginationQuery>,
) -> Response {
    let job_id = match Uuid::parse_str(&job_id) {
        Ok(job_id) => job_id,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(ApiResponse::<serde_json::Value>::failure(
                    "invalid_request",
                    "job_id must be a valid UUID",
                )),
            )
                .into_response();
        }
    };

    let (page, page_size) = match parse_pagination(query) {
        Ok(value) => value,
        Err(response) => return response,
    };

    match state
        .execute_service
        .get_job_entries(job_id, page, page_size)
        .await
    {
        Ok(Some(entries)) => (StatusCode::OK, Json(ApiResponse::success(entries))).into_response(),
        Ok(None) => (
            StatusCode::NOT_FOUND,
            Json(ApiResponse::<serde_json::Value>::failure(
                "job_not_found",
                "normalize job not found",
            )),
        )
            .into_response(),
        Err(error) => map_execute_error(error),
    }
}

fn parse_pagination(query: PaginationQuery) -> Result<(usize, usize), Response> {
    let page = query.page.unwrap_or(1);
    let page_size = query.page_size.unwrap_or(50);

    if page == 0 || page_size == 0 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::<serde_json::Value>::failure(
                "invalid_request",
                "page and page_size must be greater than zero",
            )),
        )
            .into_response());
    }

    Ok((page, page_size.min(200)))
}

fn map_execute_error(error: ExecuteServiceError) -> Response {
    match error {
        ExecuteServiceError::ScanNotFound => (
            StatusCode::NOT_FOUND,
            Json(ApiResponse::<serde_json::Value>::failure(
                "scan_not_found",
                "scan session not found",
            )),
        )
            .into_response(),
        ExecuteServiceError::ScanMismatch => (
            StatusCode::BAD_REQUEST,
            Json(ApiResponse::<serde_json::Value>::failure(
                "scan_mismatch",
                "request does not match scan session",
            )),
        )
            .into_response(),
        ExecuteServiceError::IdempotencyConflict => (
            StatusCode::CONFLICT,
            Json(ApiResponse::<serde_json::Value>::failure(
                "idempotency_conflict",
                "idempotency key has already been used with a different request",
            )),
        )
            .into_response(),
        ExecuteServiceError::Preflight => (
            StatusCode::CONFLICT,
            Json(ApiResponse::<serde_json::Value>::failure(
                "preflight_failed",
                "preflight validation failed",
            )),
        )
            .into_response(),
        ExecuteServiceError::Internal(message) => {
            let _ = message;
            tracing::error!("execute service internal error");
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
