use std::path::PathBuf;

use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use serde_json::Value;
use supafast_jellyfin::{config::AppConfig, http::build_app, state::AppState};
use tower::ServiceExt;

fn test_app() -> axum::Router {
    let config = AppConfig {
        host: "127.0.0.1".to_string(),
        tv_base_path: PathBuf::from("/tmp"),
        movies_base_path: PathBuf::from("/tmp"),
        database_url: None,
        audit_log_dir: PathBuf::from("/tmp/supafast-jellyfin-tests"),
        port: 3001,
        scan_ttl_minutes: 60,
        follow_symlinks: false,
    };
    build_app(AppState::new_for_tests(config))
}

async fn parse_json(response: axum::response::Response) -> Value {
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body bytes");
    serde_json::from_slice(&bytes).expect("json body")
}

async fn create_scan(app: &axum::Router, base_path: &str) -> String {
    let payload = format!(r#"{{"media_type":"tv","base_path":"{}"}}"#, base_path);
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/normalize/scan")
                .header("content-type", "application/json")
                .body(Body::from(payload))
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

async fn execute_and_wait(app: &axum::Router, scan_id: &str, base_path: &str) -> String {
    let payload = format!(
        r#"{{"scan_id":"{}","idempotency_key":"exec-key","media_type":"tv","base_path":"{}","name_overrides":[],"conflict_resolutions":[],"dry_run":false}}"#,
        scan_id, base_path
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
    let job_id = execute_body["data"]["job_id"]
        .as_str()
        .expect("job_id")
        .to_string();

    for _ in 0..1000 {
        let status_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri(format!("/api/normalize/jobs/{job_id}"))
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");

        let status_body = parse_json(status_response).await;
        if status_body["success"] == Value::Bool(true) {
            let status = status_body["data"]["status"].as_str().unwrap_or_default();
            if status == "completed" {
                return job_id;
            }
            if status == "failed" {
                panic!("normalize execute failed");
            }
        }

        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }

    panic!("timed out waiting for normalize execute job to complete");
}

async fn setup_executed_session(app: &axum::Router) -> (tempfile::TempDir, String) {
    let temp = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir(temp.path().join("A Perfect Planet 2021 - [tmdbid-115194]"))
        .expect("create source dir");
    std::fs::create_dir(temp.path().join("Animaniacs 1993 - [tmdbid-82]"))
        .expect("create source dir");

    let base_path = temp.path().display().to_string();
    let scan_id = create_scan(app, &base_path).await;
    let execute_session_id = execute_and_wait(app, &scan_id, &base_path).await;

    (temp, execute_session_id)
}

#[tokio::test]
async fn list_rollback_sessions_returns_execute_session() {
    let app = test_app();
    let (_temp, session_id) = setup_executed_session(&app).await;

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/rollback/sessions?page=1&page_size=1")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = parse_json(response).await;
    assert_eq!(body["success"], Value::Bool(true));
    let sessions = body["data"]["sessions"].as_array().expect("sessions array");
    assert_eq!(body["data"]["pagination"]["page"], Value::from(1));
    assert_eq!(body["data"]["pagination"]["page_size"], Value::from(1));
    assert!(body["data"]["pagination"]["total"].as_u64().unwrap_or(0) >= 1);
    assert!(sessions
        .iter()
        .any(|session| session["session_id"] == Value::String(session_id.clone())));
}

#[tokio::test]
async fn list_rollback_sessions_rejects_invalid_pagination_params() {
    let app = test_app();

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/rollback/sessions?page=0&page_size=10")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = parse_json(response).await;
    assert_eq!(body["success"], Value::Bool(false));
    assert_eq!(
        body["error"]["code"],
        Value::String("invalid_request".to_string())
    );
}

#[tokio::test]
async fn rollback_preview_returns_reverse_order_operations() {
    let app = test_app();
    let (_temp, session_id) = setup_executed_session(&app).await;

    let payload = format!(r#"{{"session_id":"{}"}}"#, session_id);
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/rollback/preview")
                .header("content-type", "application/json")
                .body(Body::from(payload))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::OK);
    let body = parse_json(response).await;
    assert_eq!(body["success"], Value::Bool(true));
    assert_eq!(body["data"]["blocked_count"], Value::from(0));

    let operations = body["data"]["operations"].as_array().expect("operations");
    assert!(operations.len() >= 2);

    let first_index = operations[0]["op_index"].as_u64().expect("first index");
    let second_index = operations[1]["op_index"].as_u64().expect("second index");
    assert!(first_index > second_index);
}

#[tokio::test]
async fn rollback_execute_is_idempotent_for_same_key() {
    let app = test_app();
    let (_temp, session_id) = setup_executed_session(&app).await;

    let payload = format!(
        r#"{{"session_id":"{}","idempotency_key":"rollback-key","dry_run":false}}"#,
        session_id
    );

    let first_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/rollback/execute")
                .header("content-type", "application/json")
                .body(Body::from(payload.clone()))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(first_response.status(), StatusCode::ACCEPTED);
    let first_body = parse_json(first_response).await;
    assert_eq!(first_body["success"], Value::Bool(true));
    assert_eq!(first_body["data"]["replayed"], Value::Bool(false));

    let second_response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/rollback/execute")
                .header("content-type", "application/json")
                .body(Body::from(payload))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(second_response.status(), StatusCode::ACCEPTED);
    let second_body = parse_json(second_response).await;
    assert_eq!(second_body["success"], Value::Bool(true));
    assert_eq!(second_body["data"]["replayed"], Value::Bool(true));
    assert_eq!(
        second_body["data"]["rollback_job_id"],
        first_body["data"]["rollback_job_id"]
    );
}

#[tokio::test]
async fn rollback_execute_rejects_same_key_with_different_payload() {
    let app = test_app();
    let (_temp, session_id) = setup_executed_session(&app).await;

    let first_payload = format!(
        r#"{{"session_id":"{}","idempotency_key":"rollback-key-2","dry_run":true}}"#,
        session_id
    );
    let second_payload = format!(
        r#"{{"session_id":"{}","idempotency_key":"rollback-key-2","dry_run":false}}"#,
        session_id
    );

    let first_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/rollback/execute")
                .header("content-type", "application/json")
                .body(Body::from(first_payload))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(first_response.status(), StatusCode::ACCEPTED);

    let second_response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/rollback/execute")
                .header("content-type", "application/json")
                .body(Body::from(second_payload))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(second_response.status(), StatusCode::CONFLICT);
    let body = parse_json(second_response).await;
    assert_eq!(
        body["error"]["code"],
        Value::String("idempotency_conflict".to_string())
    );
}

#[tokio::test]
async fn rollback_preview_returns_404_for_unknown_session() {
    let app = test_app();

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/rollback/preview")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"session_id":"00000000-0000-0000-0000-000000000000"}"#,
                ))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn rollback_execute_restores_original_directory_names() {
    let app = test_app();
    let (temp, session_id) = setup_executed_session(&app).await;

    let normalized_a = temp.path().join("A Perfect Planet (2021)");
    let normalized_b = temp.path().join("Animaniacs (1993)");
    assert!(normalized_a.exists());
    assert!(normalized_b.exists());

    let payload = format!(
        r#"{{"session_id":"{}","idempotency_key":"restore-key","dry_run":false}}"#,
        session_id
    );
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/rollback/execute")
                .header("content-type", "application/json")
                .body(Body::from(payload))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::ACCEPTED);
    assert!(temp
        .path()
        .join("A Perfect Planet 2021 - [tmdbid-115194]")
        .exists());
    assert!(temp.path().join("Animaniacs 1993 - [tmdbid-82]").exists());
}

#[tokio::test]
async fn rollback_execute_rejects_oversized_idempotency_key() {
    let app = test_app();
    let (_temp, session_id) = setup_executed_session(&app).await;
    let oversized = "k".repeat(129);
    let payload = format!(
        r#"{{"session_id":"{}","idempotency_key":"{}","dry_run":true}}"#,
        session_id, oversized
    );

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/rollback/execute")
                .header("content-type", "application/json")
                .body(Body::from(payload))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn rollback_execute_normalizes_whitespace_in_idempotency_key() {
    let app = test_app();
    let (_temp, session_id) = setup_executed_session(&app).await;

    let first_payload = format!(
        r#"{{"session_id":"{}","idempotency_key":"normalized-key","dry_run":true}}"#,
        session_id
    );
    let second_payload = format!(
        r#"{{"session_id":"{}","idempotency_key":"  normalized-key  ","dry_run":true}}"#,
        session_id
    );

    let first_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/rollback/execute")
                .header("content-type", "application/json")
                .body(Body::from(first_payload))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(first_response.status(), StatusCode::ACCEPTED);
    let first_body = parse_json(first_response).await;

    let second_response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/rollback/execute")
                .header("content-type", "application/json")
                .body(Body::from(second_payload))
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(second_response.status(), StatusCode::ACCEPTED);
    let second_body = parse_json(second_response).await;

    assert_eq!(second_body["data"]["replayed"], Value::Bool(true));
    assert_eq!(
        second_body["data"]["rollback_job_id"],
        first_body["data"]["rollback_job_id"]
    );
}
