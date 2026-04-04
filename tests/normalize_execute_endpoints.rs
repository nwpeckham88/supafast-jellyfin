use std::path::PathBuf;

use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use serde_json::Value;
use std::sync::Arc;
use supafast_jellyfin::{
    config::AppConfig,
    http::build_app,
    services::scan::{InMemoryScanService, ScanService},
    state::AppState,
};
use tower::ServiceExt;

fn test_app() -> axum::Router {
    let config = AppConfig {
        tv_base_path: PathBuf::from("/tmp"),
        movies_base_path: PathBuf::from("/tmp"),
        database_url: None,
        audit_log_dir: PathBuf::from("/tmp/supafast-jellyfin-tests"),
        port: 3001,
        scan_ttl_minutes: 60,
        follow_symlinks: false,
    };
    let scan_service: Arc<dyn ScanService> = Arc::new(InMemoryScanService::new());
    let state = AppState::new(config, scan_service);
    build_app(state)
}

async fn parse_json(response: axum::response::Response) -> Value {
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body bytes");
    serde_json::from_slice(&bytes).expect("json body")
}

async fn create_scan(app: &axum::Router, base_path: &str) -> String {
    let request_body = format!(r#"{{"media_type":"tv","base_path":"{}"}}"#, base_path);
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/normalize/scan")
                .header("content-type", "application/json")
                .body(Body::from(request_body))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::CREATED);
    let body = parse_json(response).await;
    body["data"]["scan_id"]
        .as_str()
        .expect("scan_id")
        .to_string()
}

#[tokio::test]
async fn post_execute_returns_202_and_job_id_for_valid_request() {
    let app = test_app();
    let temp = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir(temp.path().join("A Perfect Planet 2021 - [tmdbid-115194]"))
        .expect("create source dir");

    let scan_id = create_scan(&app, &temp.path().display().to_string()).await;
    let payload = format!(
        r#"{{"scan_id":"{}","idempotency_key":"key-1","media_type":"tv","base_path":"{}","name_overrides":[],"conflict_resolutions":[],"dry_run":false}}"#,
        scan_id,
        temp.path().display()
    );

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/normalize/execute")
                .header("content-type", "application/json")
                .body(Body::from(payload))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let body = parse_json(response).await;
    assert_eq!(body["success"], Value::Bool(true));
    assert!(body["data"]["job_id"].as_str().is_some());
}

#[tokio::test]
async fn post_execute_returns_existing_job_for_same_idempotency_key() {
    let app = test_app();
    let temp = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir(temp.path().join("Animal Control - 2023 [tvdbid-423022]"))
        .expect("create source dir");

    let scan_id = create_scan(&app, &temp.path().display().to_string()).await;
    let payload = format!(
        r#"{{"scan_id":"{}","idempotency_key":"same-key","media_type":"tv","base_path":"{}","name_overrides":[],"conflict_resolutions":[],"dry_run":false}}"#,
        scan_id,
        temp.path().display()
    );

    let first = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/normalize/execute")
                .header("content-type", "application/json")
                .body(Body::from(payload.clone()))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(first.status(), StatusCode::ACCEPTED);
    let first_body = parse_json(first).await;
    let first_job_id = first_body["data"]["job_id"]
        .as_str()
        .expect("job id")
        .to_string();

    let second = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/normalize/execute")
                .header("content-type", "application/json")
                .body(Body::from(payload))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(second.status(), StatusCode::ACCEPTED);
    let second_body = parse_json(second).await;
    assert_eq!(
        second_body["data"]["job_id"],
        Value::String(first_job_id.clone())
    );
    assert_eq!(second_body["data"]["replayed"], Value::Bool(true));
}

#[tokio::test]
async fn post_execute_rejects_same_key_with_different_payload() {
    let app = test_app();
    let temp = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir(temp.path().join("Animal Control - 2023 [tvdbid-423022]"))
        .expect("create source dir");

    let scan_id = create_scan(&app, &temp.path().display().to_string()).await;
    let first_payload = format!(
        r#"{{"scan_id":"{}","idempotency_key":"same-key-different","media_type":"tv","base_path":"{}","name_overrides":[],"conflict_resolutions":[],"dry_run":true}}"#,
        scan_id,
        temp.path().display()
    );
    let second_payload = format!(
        r#"{{"scan_id":"{}","idempotency_key":"same-key-different","media_type":"tv","base_path":"{}","name_overrides":[],"conflict_resolutions":[],"dry_run":false}}"#,
        scan_id,
        temp.path().display()
    );

    let first = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/normalize/execute")
                .header("content-type", "application/json")
                .body(Body::from(first_payload))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(first.status(), StatusCode::ACCEPTED);

    let second = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/normalize/execute")
                .header("content-type", "application/json")
                .body(Body::from(second_payload))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(second.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn post_execute_rejects_unknown_scan_id_with_404() {
    let app = test_app();
    let payload = r#"{"scan_id":"5d7707e5-b9a8-4b70-a2de-e2060d6dc011","idempotency_key":"k","media_type":"tv","base_path":"/tmp","name_overrides":[],"conflict_resolutions":[],"dry_run":false}"#;

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/normalize/execute")
                .header("content-type", "application/json")
                .body(Body::from(payload))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn post_execute_rejects_base_path_mismatch_with_400() {
    let app = test_app();
    let temp = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir(temp.path().join("Animaniacs 1993 - [tmdbid-82]"))
        .expect("create source dir");
    let scan_id = create_scan(&app, &temp.path().display().to_string()).await;

    let payload = format!(
        r#"{{"scan_id":"{}","idempotency_key":"key-2","media_type":"tv","base_path":"/tmp","name_overrides":[],"conflict_resolutions":[],"dry_run":false}}"#,
        scan_id,
    );

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/normalize/execute")
                .header("content-type", "application/json")
                .body(Body::from(payload))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn post_execute_rejects_preflight_collision_with_409() {
    let app = test_app();
    let temp = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir(temp.path().join("Animal Control - 2023 [tvdbid-423022]"))
        .expect("create first source dir");
    std::fs::create_dir(temp.path().join("Animal Control 2023 - [tmdbid-214162]"))
        .expect("create second source dir");
    let scan_id = create_scan(&app, &temp.path().display().to_string()).await;

    let payload = format!(
        r#"{{"scan_id":"{}","idempotency_key":"key-3","media_type":"tv","base_path":"{}","name_overrides":[],"conflict_resolutions":[],"dry_run":false}}"#,
        scan_id,
        temp.path().display()
    );

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/normalize/execute")
                .header("content-type", "application/json")
                .body(Body::from(payload))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn get_job_status_returns_not_found_for_unknown_job() {
    let app = test_app();

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/normalize/jobs/f4203ef4-e145-4b8a-af4d-2cbcc3f97b04")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn get_job_status_returns_created_job() {
    let app = test_app();
    let temp = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir(temp.path().join("A Perfect Planet 2021 - [tmdbid-115194]"))
        .expect("create source dir");

    let scan_id = create_scan(&app, &temp.path().display().to_string()).await;
    let payload = format!(
        r#"{{"scan_id":"{}","idempotency_key":"status-key","media_type":"tv","base_path":"{}","name_overrides":[],"conflict_resolutions":[],"dry_run":true}}"#,
        scan_id,
        temp.path().display()
    );

    let execute_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/normalize/execute")
                .header("content-type", "application/json")
                .body(Body::from(payload))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(execute_response.status(), StatusCode::ACCEPTED);
    let execute_body = parse_json(execute_response).await;
    let job_id = execute_body["data"]["job_id"].as_str().expect("job id");

    let job_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/normalize/jobs/{job_id}"))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(job_response.status(), StatusCode::OK);
    let job_body = parse_json(job_response).await;
    assert_eq!(job_body["success"], Value::Bool(true));
    let status = job_body["data"]["status"].as_str().expect("status");
    assert!(matches!(status, "pending" | "running" | "completed"));
}

#[tokio::test]
async fn post_execute_replays_with_canonical_equivalent_base_path() {
    let app = test_app();
    let temp = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir(temp.path().join("Animaniacs 1993 - [tmdbid-82]"))
        .expect("create source dir");

    let scan_id = create_scan(&app, &temp.path().display().to_string()).await;
    let base_a = temp.path().display().to_string();
    let base_b = format!("{}/.", temp.path().display());

    let first_payload = format!(
        r#"{{"scan_id":"{}","idempotency_key":"canonical-replay","media_type":"tv","base_path":"{}","name_overrides":[],"conflict_resolutions":[],"dry_run":true}}"#,
        scan_id, base_a
    );
    let second_payload = format!(
        r#"{{"scan_id":"{}","idempotency_key":"canonical-replay","media_type":"tv","base_path":"{}","name_overrides":[],"conflict_resolutions":[],"dry_run":true}}"#,
        scan_id, base_b
    );

    let first = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/normalize/execute")
                .header("content-type", "application/json")
                .body(Body::from(first_payload))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(first.status(), StatusCode::ACCEPTED);

    let second = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/normalize/execute")
                .header("content-type", "application/json")
                .body(Body::from(second_payload))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(second.status(), StatusCode::ACCEPTED);
    let second_body = parse_json(second).await;
    assert_eq!(second_body["data"]["replayed"], Value::Bool(true));
}

#[tokio::test]
async fn post_execute_rejects_second_active_job_for_same_base_path() {
    let app = test_app();
    let temp = tempfile::tempdir().expect("tempdir");

    for index in 0..200 {
        let name = format!("Show {} 2020 - [tmdbid-{}]", index, 1000 + index);
        std::fs::create_dir(temp.path().join(name)).expect("create source dir");
    }

    let scan_id = create_scan(&app, &temp.path().display().to_string()).await;
    let first_payload = format!(
        r#"{{"scan_id":"{}","idempotency_key":"active-job-1","media_type":"tv","base_path":"{}","name_overrides":[],"conflict_resolutions":[],"dry_run":false}}"#,
        scan_id,
        temp.path().display()
    );
    let second_payload = format!(
        r#"{{"scan_id":"{}","idempotency_key":"active-job-2","media_type":"tv","base_path":"{}","name_overrides":[],"conflict_resolutions":[],"dry_run":false}}"#,
        scan_id,
        temp.path().display()
    );

    let first = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/normalize/execute")
                .header("content-type", "application/json")
                .body(Body::from(first_payload))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(first.status(), StatusCode::ACCEPTED);

    let second = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/normalize/execute")
                .header("content-type", "application/json")
                .body(Body::from(second_payload))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(second.status(), StatusCode::CONFLICT);
}
