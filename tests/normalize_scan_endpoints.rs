use std::{path::PathBuf, sync::Arc};

use async_trait::async_trait;
use axum::{
    body::{to_bytes, Body},
    http::{Request, StatusCode},
};
use serde_json::Value;
use supafast_jellyfin::{
    config::AppConfig,
    http::build_app,
    repositories::scan_repo::{ScanRepository, ScanRepositoryError, ScanSessionRecord},
    services::scan::{InMemoryScanService, ScanService},
    state::AppState,
};
use tempfile::tempdir;
use tower::ServiceExt;

fn test_app() -> axum::Router {
    test_app_with_ttl(60)
}

fn test_app_with_ttl(scan_ttl_minutes: u64) -> axum::Router {
    test_app_with_service(
        scan_ttl_minutes,
        Arc::new(InMemoryScanService::new()) as Arc<dyn ScanService>,
    )
}

fn test_app_with_service(
    scan_ttl_minutes: u64,
    scan_service: Arc<dyn ScanService>,
) -> axum::Router {
    let config = AppConfig {
        tv_base_path: PathBuf::from("/tmp"),
        movies_base_path: PathBuf::from("/tmp"),
        database_url: None,
        audit_log_dir: PathBuf::from("/tmp/supafast-jellyfin-tests"),
        port: 3001,
        scan_ttl_minutes,
        follow_symlinks: false,
    };
    let state = AppState::new(config, scan_service);
    build_app(state)
}

#[derive(Clone, Copy)]
enum FailingMethod {
    Save,
    Get,
    Delete,
    Prune,
}

struct FailingRepo {
    method: FailingMethod,
}

#[async_trait]
impl ScanRepository for FailingRepo {
    async fn save_scan(&self, _record: ScanSessionRecord) -> Result<(), ScanRepositoryError> {
        if matches!(self.method, FailingMethod::Save) {
            return Err(ScanRepositoryError::LockPoisoned);
        }
        Ok(())
    }

    async fn get_scan(
        &self,
        _scan_id: uuid::Uuid,
    ) -> Result<Option<ScanSessionRecord>, ScanRepositoryError> {
        if matches!(self.method, FailingMethod::Get) {
            return Err(ScanRepositoryError::LockPoisoned);
        }
        Ok(None)
    }

    async fn delete_scan(&self, _scan_id: uuid::Uuid) -> Result<bool, ScanRepositoryError> {
        if matches!(self.method, FailingMethod::Delete) {
            return Err(ScanRepositoryError::LockPoisoned);
        }
        Ok(false)
    }

    async fn prune_expired(&self, _now: std::time::SystemTime) -> Result<(), ScanRepositoryError> {
        if matches!(self.method, FailingMethod::Prune) {
            return Err(ScanRepositoryError::LockPoisoned);
        }
        Ok(())
    }
}

#[tokio::test]
async fn scan_expires_immediately_when_ttl_is_zero() {
    let app = test_app_with_ttl(0);

    let create_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/normalize/scan")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"media_type":"tv","base_path":"/tmp"}"#))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(create_response.status(), StatusCode::CREATED);
    let create_body = parse_json(create_response).await;
    let scan_id = create_body["data"]["scan_id"]
        .as_str()
        .expect("scan_id")
        .to_string();

    let get_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/normalize/scan/{scan_id}"))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(get_response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn post_scan_returns_500_when_ttl_overflows() {
    let app = test_app_with_ttl(u64::MAX);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/normalize/scan")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"media_type":"tv","base_path":"/tmp"}"#))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let body = parse_json(response).await;
    assert_eq!(body["success"], Value::Bool(false));
    assert_eq!(
        body["error"]["code"],
        Value::String("internal_error".to_string())
    );
}

async fn parse_json(response: axum::response::Response) -> Value {
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body bytes");
    serde_json::from_slice(&bytes).expect("json body")
}

#[tokio::test]
async fn post_scan_returns_201_and_envelope() {
    let app = test_app();

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/normalize/scan")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"media_type":"tv","base_path":"/tmp"}"#))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::CREATED);
    let body = parse_json(response).await;

    assert_eq!(body["success"], Value::Bool(true));
    assert!(body["error"].is_null());
    assert!(body["data"].is_object());
    assert!(body["data"]["scan_id"].as_str().is_some());
    assert_eq!(body["data"]["media_type"], Value::String("tv".to_string()));
}

#[tokio::test]
async fn get_scan_returns_404_for_missing_scan() {
    let app = test_app();

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/normalize/scan/5d7707e5-b9a8-4b70-a2de-e2060d6dc011")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body = parse_json(response).await;

    assert_eq!(body["success"], Value::Bool(false));
    assert!(body["data"].is_null());
    assert_eq!(
        body["error"]["code"],
        Value::String("scan_not_found".to_string())
    );
}

#[tokio::test]
async fn create_then_get_then_delete_scan() {
    let app = test_app();

    let create_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/normalize/scan")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"media_type":"tv","base_path":"/tmp"}"#))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(create_response.status(), StatusCode::CREATED);
    let create_body = parse_json(create_response).await;
    let scan_id = create_body["data"]["scan_id"]
        .as_str()
        .expect("scan_id")
        .to_string();

    let get_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/normalize/scan/{scan_id}"))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(get_response.status(), StatusCode::OK);

    let delete_response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/normalize/scan/{scan_id}"))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(delete_response.status(), StatusCode::OK);
    let delete_body = parse_json(delete_response).await;
    assert_eq!(delete_body["success"], Value::Bool(true));
    assert_eq!(
        delete_body["data"]["scan_id"],
        Value::String(scan_id.clone())
    );
    assert_eq!(delete_body["data"]["expired"], Value::Bool(true));

    let final_get_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/normalize/scan/{scan_id}"))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(final_get_response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn preserves_correlation_id_header_on_scan_endpoint() {
    let app = test_app();
    let correlation_id = "85c6f0b1-4c5f-45b7-a74f-5198db2c4f3e";

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/normalize/scan")
                .header("x-correlation-id", correlation_id)
                .header("content-type", "application/json")
                .body(Body::from(r#"{"media_type":"tv","base_path":"/tmp"}"#))
                .expect("request"),
        )
        .await
        .expect("response");

    let header = response
        .headers()
        .get("x-correlation-id")
        .expect("x-correlation-id")
        .to_str()
        .expect("header value");

    assert_eq!(header, correlation_id);
}

#[tokio::test]
async fn rejects_invalid_media_type_with_envelope() {
    let app = test_app();

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/normalize/scan")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"media_type":"podcast","base_path":"/tmp"}"#))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = parse_json(response).await;
    assert_eq!(body["success"], Value::Bool(false));
    assert!(body["data"].is_null());
    assert_eq!(
        body["error"]["code"],
        Value::String("invalid_request".to_string())
    );
}

#[tokio::test]
async fn rejects_invalid_scan_id_with_envelope() {
    let app = test_app();

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/normalize/scan/not-a-uuid")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = parse_json(response).await;
    assert_eq!(body["success"], Value::Bool(false));
    assert!(body["data"].is_null());
    assert_eq!(
        body["error"]["code"],
        Value::String("invalid_request".to_string())
    );
}

#[tokio::test]
async fn rejects_base_path_outside_configured_media_root() {
    let app = test_app();

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/normalize/scan")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"media_type":"tv","base_path":"/etc"}"#))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = parse_json(response).await;
    assert_eq!(body["success"], Value::Bool(false));
    assert!(body["data"].is_null());
    assert_eq!(
        body["error"]["code"],
        Value::String("invalid_request".to_string())
    );
}

#[tokio::test]
async fn post_scan_groups_normalized_duplicates_and_reports_merges() {
    let app = test_app();
    let temp = tempdir().expect("temp dir");
    let base_path = temp.path();

    std::fs::create_dir(base_path.join("Animal Control - 2023 [tvdbid-423022]"))
        .expect("create first duplicate dir");
    std::fs::create_dir(base_path.join("Animal Control 2023 - [tmdbid-214162]"))
        .expect("create second duplicate dir");
    std::fs::create_dir(base_path.join("A Perfect Planet 2021 - [tmdbid-115194]"))
        .expect("create unique dir");

    let request_body = format!(
        r#"{{"media_type":"tv","base_path":"{}"}}"#,
        base_path.display()
    );

    let response = app
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

    assert_eq!(body["success"], Value::Bool(true));
    assert!(body["error"].is_null());
    assert_eq!(body["data"]["summary"]["total_shows"], Value::from(2));
    assert_eq!(body["data"]["summary"]["merges"], Value::from(1));
    assert_eq!(body["data"]["summary"]["conflicts"], Value::from(0));
    assert_eq!(body["data"]["summary"]["total_episodes"], Value::from(0));

    let groups = body["data"]["groups"].as_array().expect("groups array");
    assert_eq!(groups.len(), 2);

    let animal_group = groups
        .iter()
        .find(|group| group["normalized_name"] == Value::String("Animal Control (2023)".into()))
        .expect("animal control group");
    let animal_sources = animal_group["source_directories"]
        .as_array()
        .expect("animal source array");
    assert_eq!(animal_sources.len(), 2);
    assert!(animal_sources.contains(&Value::String(
        "Animal Control - 2023 [tvdbid-423022]".into()
    )));
    assert!(animal_sources.contains(&Value::String(
        "Animal Control 2023 - [tmdbid-214162]".into()
    )));

    let planet_group = groups
        .iter()
        .find(|group| group["normalized_name"] == Value::String("A Perfect Planet (2021)".into()))
        .expect("planet group");
    let planet_sources = planet_group["source_directories"]
        .as_array()
        .expect("planet source array");
    assert_eq!(
        planet_sources,
        &vec![Value::String(
            "A Perfect Planet 2021 - [tmdbid-115194]".into()
        )]
    );
}

#[tokio::test]
async fn post_scan_reports_zero_merges_when_no_duplicates_exist() {
    let app = test_app();
    let temp = tempdir().expect("temp dir");
    let base_path = temp.path();

    std::fs::create_dir(base_path.join("Animaniacs 1993 - [tmdbid-82]")).expect("create first dir");
    std::fs::create_dir(base_path.join("Alien - Earth 2025 - [tmdbid-157239]"))
        .expect("create second dir");

    let request_body = format!(
        r#"{{"media_type":"tv","base_path":"{}"}}"#,
        base_path.display()
    );

    let response = app
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

    assert_eq!(body["data"]["summary"]["total_shows"], Value::from(2));
    assert_eq!(body["data"]["summary"]["merges"], Value::from(0));
    assert_eq!(body["data"]["summary"]["conflicts"], Value::from(0));
    assert_eq!(
        body["data"]["groups"]
            .as_array()
            .expect("groups array")
            .len(),
        2
    );
}

#[tokio::test]
async fn post_scan_returns_500_when_repository_save_fails() {
    let failing_service = Arc::new(InMemoryScanService::with_repository(Arc::new(
        FailingRepo {
            method: FailingMethod::Save,
        },
    )));
    let app = test_app_with_service(60, failing_service);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/normalize/scan")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"media_type":"tv","base_path":"/tmp"}"#))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let body = parse_json(response).await;
    assert_eq!(body["success"], Value::Bool(false));
    assert_eq!(
        body["error"]["code"],
        Value::String("internal_error".to_string())
    );
}

#[tokio::test]
async fn get_scan_returns_500_when_repository_get_fails() {
    let failing_service = Arc::new(InMemoryScanService::with_repository(Arc::new(
        FailingRepo {
            method: FailingMethod::Get,
        },
    )));
    let app = test_app_with_service(60, failing_service);

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/normalize/scan/5d7707e5-b9a8-4b70-a2de-e2060d6dc011")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let body = parse_json(response).await;
    assert_eq!(body["success"], Value::Bool(false));
    assert_eq!(
        body["error"]["code"],
        Value::String("internal_error".to_string())
    );
}

#[tokio::test]
async fn delete_scan_returns_500_when_repository_delete_fails() {
    let failing_service = Arc::new(InMemoryScanService::with_repository(Arc::new(
        FailingRepo {
            method: FailingMethod::Delete,
        },
    )));
    let app = test_app_with_service(60, failing_service);

    let response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/normalize/scan/5d7707e5-b9a8-4b70-a2de-e2060d6dc011")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let body = parse_json(response).await;
    assert_eq!(body["success"], Value::Bool(false));
    assert_eq!(
        body["error"]["code"],
        Value::String("internal_error".to_string())
    );
}

#[tokio::test]
async fn get_scan_returns_500_when_repository_prune_fails() {
    let failing_service = Arc::new(InMemoryScanService::with_repository(Arc::new(
        FailingRepo {
            method: FailingMethod::Prune,
        },
    )));
    let app = test_app_with_service(60, failing_service);

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/normalize/scan/5d7707e5-b9a8-4b70-a2de-e2060d6dc011")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let body = parse_json(response).await;
    assert_eq!(body["success"], Value::Bool(false));
    assert_eq!(
        body["error"]["code"],
        Value::String("internal_error".to_string())
    );
}

#[tokio::test]
async fn post_scan_returns_500_when_repository_prune_fails() {
    let failing_service = Arc::new(InMemoryScanService::with_repository(Arc::new(
        FailingRepo {
            method: FailingMethod::Prune,
        },
    )));
    let app = test_app_with_service(60, failing_service);

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/normalize/scan")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"media_type":"tv","base_path":"/tmp"}"#))
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let body = parse_json(response).await;
    assert_eq!(body["success"], Value::Bool(false));
    assert_eq!(
        body["error"]["code"],
        Value::String("internal_error".to_string())
    );
}

#[tokio::test]
async fn delete_scan_returns_500_when_repository_prune_fails() {
    let failing_service = Arc::new(InMemoryScanService::with_repository(Arc::new(
        FailingRepo {
            method: FailingMethod::Prune,
        },
    )));
    let app = test_app_with_service(60, failing_service);

    let response = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/normalize/scan/5d7707e5-b9a8-4b70-a2de-e2060d6dc011")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let body = parse_json(response).await;
    assert_eq!(body["success"], Value::Bool(false));
    assert_eq!(
        body["error"]["code"],
        Value::String("internal_error".to_string())
    );
}
