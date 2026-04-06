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
    models::execute::MovieRenameMode,
    services::scan::{InMemoryScanService, ScanService},
    state::AppState,
};
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
        movie_rename_mode: MovieRenameMode::Clean,
    };
    let scan_service: Arc<dyn ScanService> = Arc::new(InMemoryScanService::new());
    let state = AppState::new(config, scan_service, None);
    build_app(state)
}

async fn parse_json(response: axum::response::Response) -> Value {
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("response body bytes");
    serde_json::from_slice(&bytes).expect("json body")
}

async fn create_scan(app: &axum::Router, base_path: &str) -> String {
    create_scan_with_media_type(app, base_path, "tv").await
}

async fn create_scan_with_media_type(
    app: &axum::Router,
    base_path: &str,
    media_type: &str,
) -> String {
    let request_body = format!(
        r#"{{"media_type":"{}","base_path":"{}"}}"#,
        media_type, base_path
    );
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
async fn post_execute_movie_mode_renames_directory_only() {
    let app = test_app();
    let temp = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir(temp.path().join("Spider-Man: Into the Spider-Verse (2018)"))
        .expect("create source dir");

    let scan_id =
        create_scan_with_media_type(&app, &temp.path().display().to_string(), "movie").await;
    let payload = format!(
        r#"{{"scan_id":"{}","idempotency_key":"movie-key","media_type":"movie","base_path":"{}","name_overrides":[],"conflict_resolutions":[],"dry_run":true}}"#,
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

    let status_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/normalize/jobs/{job_id}"))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(status_response.status(), StatusCode::OK);
    let status_body = parse_json(status_response).await;
    let entries = status_body["data"]["entries"]
        .as_array()
        .expect("entries array");
    assert!(!entries.is_empty());
    assert!(entries
        .iter()
        .all(|entry| entry["op_type"] == Value::String("rename_dir".to_string())));
}

#[tokio::test]
async fn post_execute_tv_one_pass_normalizes_show_season_and_episode_names() {
    let app = test_app();
    let temp = tempfile::tempdir().expect("tempdir");
    let show_dir = temp.path().join("My Show 2020 - [tmdbid-9999]");
    let season_dir = show_dir.join("Season 01");
    std::fs::create_dir_all(&season_dir).expect("create season dir");
    std::fs::write(
        season_dir.join("My Show - S01E01 - Pilot [WEBDL-1080p][h264]-GROUP.mkv"),
        "video",
    )
    .expect("create episode file");
    std::fs::write(
        season_dir.join("My Show S01E02 1080p WEB h264-GROUP.mkv"),
        "video",
    )
    .expect("create noisy episode file");
    std::fs::write(
        season_dir.join("My Show S01E02 1080p WEB h264-GROUP.nfo"),
        r#"<episodedetails><season>1</season><episode>2</episode><title>Second Episode</title></episodedetails>"#,
    )
    .expect("create sidecar nfo");

    let scan_id = create_scan(&app, &temp.path().display().to_string()).await;
    let payload = format!(
        r#"{{"scan_id":"{}","idempotency_key":"tv-one-pass","media_type":"tv","base_path":"{}","name_overrides":[],"conflict_resolutions":[],"dry_run":false}}"#,
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

    for _ in 0..100 {
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
        let status = status_body["data"]["status"].as_str().unwrap_or_default();
        if status == "completed" {
            break;
        }
        if status == "failed" {
            panic!("normalize job failed");
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    let expected_show = temp.path().join("My Show (2020)");
    let expected_season = expected_show.join("Season 1");
    let expected_episode = expected_season.join("01x01 - Pilot.mkv");
    let expected_episode_two = expected_season.join("01x02 - Second Episode.mkv");
    let expected_nfo_two = expected_season.join("01x02 - Second Episode.nfo");

    assert!(expected_show.is_dir(), "expected show directory to be normalized");
    assert!(expected_season.is_dir(), "expected season directory to be normalized");
    assert!(expected_episode.is_file(), "expected episode filename to be normalized");
    assert!(
        expected_episode_two.is_file(),
        "expected noisy episode filename to use sidecar metadata"
    );
    assert!(
        expected_nfo_two.is_file(),
        "expected sidecar nfo filename to use sidecar metadata"
    );
}

#[tokio::test]
async fn post_execute_tv_one_pass_renames_season_trickplay_with_show_rename() {
    let app = test_app();
    let temp = tempfile::tempdir().expect("tempdir");
    let show_dir = temp.path().join("My Show 2020 - [tmdbid-9999]");
    let season_dir = show_dir.join("Season 01");
    std::fs::create_dir_all(&season_dir).expect("create season dir");
    std::fs::create_dir(season_dir.with_extension("trickplay"))
        .expect("create season trickplay dir");

    let scan_id = create_scan(&app, &temp.path().display().to_string()).await;
    let payload = format!(
        r#"{{"scan_id":"{}","idempotency_key":"tv-trickplay-order","media_type":"tv","base_path":"{}","name_overrides":[],"conflict_resolutions":[],"dry_run":false}}"#,
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

    for _ in 0..200 {
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
        let status = status_body["data"]["status"].as_str().unwrap_or_default();
        if status == "completed" {
            break;
        }
        if status == "failed" {
            panic!("normalize job failed");
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }

    let expected_show = temp.path().join("My Show (2020)");
    let expected_trickplay = expected_show.join("Season 1.trickplay");
    assert!(expected_trickplay.is_dir(), "expected season trickplay to be renamed");
}

#[tokio::test]
async fn list_jobs_returns_paginated_results_with_meta() {
    let app = test_app();
    let mut temp_dirs = Vec::new();

    for index in 0..3 {
        let temp = tempfile::tempdir().expect("tempdir");
        let dir_name = format!("Show {index} 2020 - [tmdbid-{}]", 9000 + index);
        std::fs::create_dir(temp.path().join(dir_name)).expect("create source dir");

        let scan_id = create_scan(&app, &temp.path().display().to_string()).await;
        let payload = format!(
            r#"{{"scan_id":"{}","idempotency_key":"list-job-{}","media_type":"tv","base_path":"{}","name_overrides":[],"conflict_resolutions":[],"dry_run":true}}"#,
            scan_id,
            index,
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
        temp_dirs.push(temp);
    }

    let list_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/normalize/jobs?page=1&page_size=2")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(list_response.status(), StatusCode::OK);
    let list_body = parse_json(list_response).await;
    let jobs = list_body["data"]["jobs"].as_array().expect("jobs array");
    assert_eq!(jobs.len(), 2);
    assert_eq!(list_body["data"]["pagination"]["page"], Value::from(1));
    assert_eq!(list_body["data"]["pagination"]["page_size"], Value::from(2));
    assert!(
        list_body["data"]["pagination"]["total"]
            .as_u64()
            .unwrap_or(0)
            >= 3
    );
}

#[tokio::test]
async fn get_job_entries_returns_paginated_entries() {
    let app = test_app();
    let temp = tempfile::tempdir().expect("tempdir");
    for index in 0..3 {
        let dir_name = format!("Series {index} 2022 - [tmdbid-{}]", 5000 + index);
        std::fs::create_dir(temp.path().join(dir_name)).expect("create source dir");
    }

    let scan_id = create_scan(&app, &temp.path().display().to_string()).await;
    let payload = format!(
        r#"{{"scan_id":"{}","idempotency_key":"entries-key","media_type":"tv","base_path":"{}","name_overrides":[],"conflict_resolutions":[],"dry_run":true}}"#,
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
    let job_id = execute_body["data"]["job_id"].as_str().expect("job_id");

    let entries_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!(
                    "/api/normalize/jobs/{job_id}/entries?page=1&page_size=2"
                ))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");

    assert_eq!(entries_response.status(), StatusCode::OK);
    let entries_body = parse_json(entries_response).await;
    let entries = entries_body["data"]["entries"]
        .as_array()
        .expect("entries array");
    assert_eq!(entries.len(), 2);
    assert_eq!(entries_body["data"]["pagination"]["page"], Value::from(1));
    assert_eq!(
        entries_body["data"]["pagination"]["page_size"],
        Value::from(2)
    );
}

#[tokio::test]
async fn list_jobs_rejects_invalid_pagination_params() {
    let app = test_app();

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/normalize/jobs?page=0&page_size=10")
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
async fn post_execute_renames_trickplay_companion_directory_when_present() {
    let app = test_app();
    let temp = tempfile::tempdir().expect("tempdir");
    let source_name = "Animal Control - 2023 [tvdbid-423022]";
    let normalized_name = "Animal Control (2023)";

    std::fs::create_dir(temp.path().join(source_name)).expect("create source dir");
    std::fs::create_dir(temp.path().join(format!("{source_name}.trickplay")))
        .expect("create trickplay companion dir");

    let scan_id = create_scan(&app, &temp.path().display().to_string()).await;
    let payload = format!(
        r#"{{"scan_id":"{}","idempotency_key":"trickplay-key","media_type":"tv","base_path":"{}","name_overrides":[],"conflict_resolutions":[],"dry_run":false}}"#,
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

    for _ in 0..200 {
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
        let status = status_body["data"]["status"].as_str().unwrap_or_default();
        if status == "completed" {
            break;
        }

        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }

    assert!(temp.path().join(normalized_name).exists());

    let final_status_response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/normalize/jobs/{job_id}"))
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(final_status_response.status(), StatusCode::OK);
    let final_status_body = parse_json(final_status_response).await;
    let entries = final_status_body["data"]["entries"]
        .as_array()
        .expect("entries array");
    assert!(entries.iter().any(|entry| {
        entry["source_path"]
            .as_str()
            .unwrap_or_default()
            .ends_with(".trickplay")
            && entry["status"] == Value::String("succeeded".to_string())
    }));
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
    let body = parse_json(response).await;
    assert_eq!(body["error"]["code"], Value::String("preflight_failed".to_string()));
    assert_eq!(
        body["error"]["message"],
        Value::String(
            "duplicate group 'Animal Control (2023)' requires conflict resolution".to_string()
        )
    );
}

#[tokio::test]
async fn post_execute_rejects_real_world_duplicate_movie_dirs_without_resolution() {
    let app = test_app();
    let temp = tempfile::tempdir().expect("tempdir");
    let source_a = "28 Years Later∶ The Bone Temple (2026) - [imdb-tt32141377]";
    let source_b = "28 Years Later - The Bone Temple (2026) - [imdb-tt32141377]";

    std::fs::create_dir_all(temp.path().join(source_a)).expect("create source a");
    std::fs::create_dir_all(temp.path().join(source_b)).expect("create source b");

    let scan_id =
        create_scan_with_media_type(&app, &temp.path().display().to_string(), "movie").await;
    let payload = format!(
        r#"{{"scan_id":"{}","idempotency_key":"real-world-dup-no-resolution","media_type":"movie","base_path":"{}","name_overrides":[],"conflict_resolutions":[],"dry_run":false}}"#,
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
    let body = parse_json(response).await;
    assert_eq!(body["error"]["code"], Value::String("preflight_failed".to_string()));
    assert_eq!(
        body["error"]["message"],
        Value::String(
            "duplicate group '28 Years Later - The Bone Temple (2026)' requires conflict resolution"
                .to_string()
        )
    );
}

#[tokio::test]
async fn post_execute_reports_recommended_source_for_equivalent_tv_duplicate_group() {
    let app = test_app();
    let temp = tempfile::tempdir().expect("tempdir");
    let source_a = "Quality Show 2024 - [tmdbid-1]";
    let source_b = "Quality Show - 2024 [tvdbid-2]";

    std::fs::create_dir_all(temp.path().join(source_a).join("Season 01"))
        .expect("create source a season");
    std::fs::create_dir_all(temp.path().join(source_b).join("Season 1"))
        .expect("create source b season");
    std::fs::write(
        temp.path()
            .join(source_a)
            .join("Season 01")
            .join("Quality Show - S01E01 - Pilot [WEBDL-1080p][h264]-AAA.mkv"),
        vec![0_u8; 100],
    )
    .expect("write source a episode");
    std::fs::write(
        temp.path()
            .join(source_b)
            .join("Season 1")
            .join("Quality Show - S01E01 - Pilot [WEB-DL-1080p][h264]-BBB.mkv"),
        vec![0_u8; 103],
    )
    .expect("write source b equivalent episode");

    let scan_id = create_scan(&app, &temp.path().display().to_string()).await;
    let payload = format!(
        r#"{{"scan_id":"{}","idempotency_key":"tv-recommended-resolution","media_type":"tv","base_path":"{}","name_overrides":[],"conflict_resolutions":[],"dry_run":false}}"#,
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
    let body = parse_json(response).await;
    assert_eq!(body["error"]["code"], Value::String("preflight_failed".to_string()));
    assert_eq!(
        body["error"]["message"],
        Value::String(
            "duplicate group 'Quality Show (2024)' requires conflict resolution; recommended selected_source_directory is 'Quality Show - 2024 [tvdbid-2]'"
                .to_string()
        )
    );
}

#[tokio::test]
async fn post_execute_allows_manual_conflict_resolution_for_real_world_movie_merge() {
    let app = test_app();
    let temp = tempfile::tempdir().expect("tempdir");
    let source_a = "28 Years Later∶ The Bone Temple (2026) - [imdb-tt32141377]";
    let source_b = "28 Years Later - The Bone Temple (2026) - [imdb-tt32141377]";
    let trickplay_relative =
        "28 Years Later - The Bone Temple (2026) - [tmdbid-1272837].trickplay/320 - 10x10";

    std::fs::create_dir_all(temp.path().join(source_a).join(trickplay_relative))
        .expect("create source a trickplay tree");
    std::fs::create_dir_all(temp.path().join(source_b).join(trickplay_relative))
        .expect("create source b trickplay tree");
    std::fs::write(
        temp.path().join(source_a).join(trickplay_relative).join("3.jpg"),
        "a",
    )
    .expect("create source a trickplay file");
    std::fs::write(
        temp.path().join(source_b).join(trickplay_relative).join("4.jpg"),
        "b",
    )
    .expect("create source b trickplay file");

    let scan_id =
        create_scan_with_media_type(&app, &temp.path().display().to_string(), "movie").await;
    let payload = format!(
        r#"{{"scan_id":"{}","idempotency_key":"real-world-dup-manual-resolution","media_type":"movie","base_path":"{}","name_overrides":[{{"source_directory":"{}","normalized_name":"{}"}}],"conflict_resolutions":[{{"normalized_name":"28 Years Later - The Bone Temple (2026)","selected_source_directory":"{}"}}],"dry_run":false}}"#,
        scan_id,
        temp.path().display(),
        source_b,
        source_b,
        source_b
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

    let mut completed = false;
    for _ in 0..250 {
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
        let status = status_body["data"]["status"].as_str().unwrap_or_default();
        if status == "completed" {
            completed = true;
            break;
        }
        if status == "failed" {
            panic!("normalize job failed");
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    assert!(completed, "normalize job did not complete in expected time window");

    let resolved_root = temp.path().join("28 Years Later - The Bone Temple (2026)");
    assert!(
        resolved_root.join(trickplay_relative).join("3.jpg").is_file(),
        "expected merged trickplay file to be present under selected source"
    );
    assert!(
        resolved_root.join(trickplay_relative).join("4.jpg").is_file(),
        "expected pre-existing trickplay file in selected source to remain"
    );
    assert!(
        !temp.path().join(source_a).join(trickplay_relative).join("3.jpg").exists(),
        "expected source file to be moved out of non-selected folder"
    );
    assert!(
        !temp.path().join(source_b).exists(),
        "expected selected source directory to be renamed to canonical movie name"
    );
}

#[tokio::test]
async fn post_execute_rejects_empty_idempotency_key_with_preflight_reason() {
    let app = test_app();
    let temp = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir(temp.path().join("Animaniacs 1993 - [tmdbid-82]"))
        .expect("create source dir");
    let scan_id = create_scan(&app, &temp.path().display().to_string()).await;

    let payload = format!(
        r#"{{"scan_id":"{}","idempotency_key":"","media_type":"tv","base_path":"{}","name_overrides":[],"conflict_resolutions":[],"dry_run":true}}"#,
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
    let body = parse_json(response).await;
    assert_eq!(body["error"]["code"], Value::String("preflight_failed".to_string()));
    assert_eq!(
        body["error"]["message"],
        Value::String("idempotency_key must not be empty".to_string())
    );
}

#[tokio::test]
async fn post_execute_merges_duplicate_movie_folders_into_selected_source() {
    let app = test_app();
    let temp = tempfile::tempdir().expect("tempdir");
    let source_a = "TRON∶ Ares (2025) - [imdb-tt6604188]";
    let source_b = "TRON - Ares (2025) - [imdb-tt6604188]";
    let canonical = "TRON - Ares (2025)";

    std::fs::create_dir(temp.path().join(source_a)).expect("create source a");
    std::fs::create_dir(temp.path().join(source_b)).expect("create source b");
    std::fs::write(temp.path().join(source_a).join("a.mkv"), "a").expect("file a");
    std::fs::write(temp.path().join(source_b).join("b.mkv"), "b").expect("file b");

    let scan_id = create_scan_with_media_type(&app, &temp.path().display().to_string(), "movie").await;
    let payload = format!(
        r#"{{"scan_id":"{}","idempotency_key":"movie-merge","media_type":"movie","base_path":"{}","name_overrides":[],"conflict_resolutions":[{{"normalized_name":"TRON - Ares (2025)","selected_source_directory":"{}"}}],"dry_run":false}}"#,
        scan_id,
        temp.path().display(),
        source_b
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

    for _ in 0..200 {
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
        let status = status_body["data"]["status"].as_str().unwrap_or_default();
        if status == "completed" {
            break;
        }
        if status == "failed" {
            panic!("normalize job failed");
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }

    assert!(temp.path().join(canonical).join("a.mkv").is_file());
    assert!(temp.path().join(canonical).join("b.mkv").is_file());
    assert!(!temp.path().join(source_a).join("a.mkv").exists());
}

#[tokio::test]
async fn post_execute_movie_merge_renames_selected_source_to_canonical_name_even_with_tagged_override(
) {
    let app = test_app();
    let temp = tempfile::tempdir().expect("tempdir");
    let source_a = "Ghostbusters (1984) [tmdbid-620]";
    let source_b = "Ghostbusters (1984) - [imdb-tt0087332]";
    let canonical = "Ghostbusters (1984)";

    std::fs::create_dir(temp.path().join(source_a)).expect("create source a");
    std::fs::create_dir(temp.path().join(source_b)).expect("create source b");
    std::fs::write(temp.path().join(source_a).join("landscape.jpg"), "poster")
        .expect("write source artwork");

    let scan_id = create_scan_with_media_type(&app, &temp.path().display().to_string(), "movie").await;
    let payload = format!(
        r#"{{"scan_id":"{}","idempotency_key":"ghostbusters-movie-merge","media_type":"movie","base_path":"{}","name_overrides":[{{"source_directory":"{}","normalized_name":"{}"}}],"conflict_resolutions":[{{"normalized_name":"Ghostbusters (1984)","selected_source_directory":"{}"}}],"dry_run":false}}"#,
        scan_id,
        temp.path().display(),
        source_b,
        source_b,
        source_b
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

    for _ in 0..200 {
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
        let status = status_body["data"]["status"].as_str().unwrap_or_default();
        if status == "completed" {
            break;
        }
        if status == "failed" {
            panic!("normalize job failed");
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }

    assert!(
        temp.path().join(canonical).join("landscape.jpg").is_file(),
        "expected artwork to live under canonical movie directory"
    );
    assert!(
        !temp.path().join(source_a).exists(),
        "expected duplicate source folder to be removed after merge"
    );
    assert!(
        !temp.path().join(source_b).exists(),
        "expected tagged selected source folder to be renamed to canonical name"
    );
}

#[tokio::test]
async fn post_execute_movie_one_pass_normalizes_primary_movie_files_but_keeps_artwork_names() {
    let app = test_app();
    let temp = tempfile::tempdir().expect("tempdir");
    let source_dir = temp.path().join("Ghostbusters (1984) - [imdb-tt0087332]");
    std::fs::create_dir(&source_dir).expect("create source dir");
    std::fs::write(
        source_dir.join("Ghostbusters (1984) - [imdb-tt0087332].mkv"),
        "video",
    )
    .expect("write movie");
    std::fs::write(
        source_dir.join("Ghostbusters (1984) - [imdb-tt0087332].nfo"),
        "<movie />",
    )
    .expect("write nfo");
    std::fs::write(
        source_dir.join("Ghostbusters (1984) - [imdb-tt0087332].en.srt"),
        "subtitle",
    )
    .expect("write subtitle");
    std::fs::write(source_dir.join("landscape.jpg"), "art").expect("write artwork");

    let scan_id = create_scan_with_media_type(&app, &temp.path().display().to_string(), "movie").await;
    let payload = format!(
        r#"{{"scan_id":"{}","idempotency_key":"ghostbusters-one-pass","media_type":"movie","base_path":"{}","name_overrides":[],"conflict_resolutions":[],"dry_run":false}}"#,
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

    for _ in 0..200 {
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
        let status = status_body["data"]["status"].as_str().unwrap_or_default();
        if status == "completed" {
            break;
        }
        if status == "failed" {
            panic!("normalize job failed");
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }

    let canonical_dir = temp.path().join("Ghostbusters (1984)");
    assert!(canonical_dir.join("Ghostbusters (1984).mkv").is_file());
    assert!(canonical_dir.join("movie.nfo").is_file());
    assert!(canonical_dir.join("Ghostbusters (1984).en.srt").is_file());
    assert!(canonical_dir.join("landscape.jpg").is_file());
    assert!(
        !canonical_dir
            .join("Ghostbusters (1984) - [imdb-tt0087332].mkv")
            .exists()
    );
}

#[tokio::test]
async fn post_execute_movie_provider_ids_mode_keeps_provider_tags_in_folder_and_video_names() {
    let app = test_app();
    let temp = tempfile::tempdir().expect("tempdir");
    let source_a = temp.path().join("Ghostbusters (1984) [tmdbid-620]");
    let source_b = temp.path().join("Ghostbusters (1984) - [imdb-tt0087332]");
    std::fs::create_dir(&source_a).expect("create source a");
    std::fs::create_dir(&source_b).expect("create source b");
    std::fs::write(
        source_a.join("Ghostbusters (1984) [tmdbid-620].mkv"),
        "video-a",
    )
    .expect("write movie a");
    std::fs::write(
        source_b.join("Ghostbusters (1984) - [imdb-tt0087332].nfo"),
        "<movie />",
    )
    .expect("write nfo b");

    let scan_id = create_scan_with_media_type(&app, &temp.path().display().to_string(), "movie").await;
    let payload = format!(
        r#"{{"scan_id":"{}","idempotency_key":"ghostbusters-provider-ids","media_type":"movie","base_path":"{}","name_overrides":[],"conflict_resolutions":[{{"normalized_name":"Ghostbusters (1984)","selected_source_directory":"{}"}}],"movie_rename_mode":"provider_ids","dry_run":false}}"#,
        scan_id,
        temp.path().display(),
        source_b.file_name().and_then(|v| v.to_str()).expect("source b name")
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

    for _ in 0..200 {
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
        let status = status_body["data"]["status"].as_str().unwrap_or_default();
        if status == "completed" {
            break;
        }
        if status == "failed" {
            panic!("normalize job failed");
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }

    let canonical_dir =
        temp.path()
            .join("Ghostbusters (1984) [tmdbid-620] [imdbid-tt0087332]");
    assert!(
        canonical_dir
            .join("Ghostbusters (1984) [tmdbid-620] [imdbid-tt0087332].mkv")
            .is_file()
    );
    assert!(canonical_dir.join("movie.nfo").is_file());
}

#[tokio::test]
async fn post_execute_movie_merge_handles_duplicate_primary_files_without_target_collisions() {
    let app = test_app();
    let temp = tempfile::tempdir().expect("tempdir");
    let source_a = "Ghostbusters (1984) [tmdbid-620]";
    let source_b = "Ghostbusters (1984) - [imdb-tt0087332]";

    std::fs::create_dir(temp.path().join(source_a)).expect("create source a");
    std::fs::create_dir(temp.path().join(source_b)).expect("create source b");
    std::fs::write(
        temp.path()
            .join(source_a)
            .join("Ghostbusters (1984) [tmdbid-620].mkv"),
        "video-a",
    )
    .expect("write movie a");
    std::fs::write(
        temp.path()
            .join(source_b)
            .join("Ghostbusters (1984) - [imdb-tt0087332].mkv"),
        "video-b",
    )
    .expect("write movie b");
    std::fs::write(
        temp.path()
            .join(source_a)
            .join("Ghostbusters (1984) [tmdbid-620].nfo"),
        "<movie />",
    )
    .expect("write nfo a");
    std::fs::write(
        temp.path()
            .join(source_b)
            .join("Ghostbusters (1984) - [imdb-tt0087332].nfo"),
        "<movie />",
    )
    .expect("write nfo b");

    let scan_id =
        create_scan_with_media_type(&app, &temp.path().display().to_string(), "movie").await;
    let payload = format!(
        r#"{{"scan_id":"{}","idempotency_key":"ghostbusters-duplicate-primary-files","media_type":"movie","base_path":"{}","name_overrides":[],"conflict_resolutions":[{{"normalized_name":"Ghostbusters (1984)","selected_source_directory":"{}"}}],"dry_run":false}}"#,
        scan_id,
        temp.path().display(),
        source_b
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

    for _ in 0..200 {
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
        let status = status_body["data"]["status"].as_str().unwrap_or_default();
        if status == "completed" {
            break;
        }
        if status == "failed" {
            panic!("normalize job failed");
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }

    let canonical_dir = temp.path().join("Ghostbusters (1984)");
    assert!(canonical_dir.join("Ghostbusters (1984).mkv").is_file());
    assert!(canonical_dir.join("movie.nfo").is_file());
    assert!(
        canonical_dir
            .join("Ghostbusters (1984) [tmdbid-620].mkv")
            .is_file()
    );
    assert!(
        canonical_dir
            .join("Ghostbusters (1984) [tmdbid-620].nfo")
            .is_file()
    );
}

#[tokio::test]
async fn post_execute_merges_duplicate_tv_folders_into_selected_source() {
    let app = test_app();
    let temp = tempfile::tempdir().expect("tempdir");
    let source_a = "My Show∶ Home (2020) - [imdb-tt1234567]";
    let source_b = "My Show - Home (2020) - [imdb-tt1234567]";
    let canonical = "My Show - Home (2020)";

    std::fs::create_dir_all(temp.path().join(source_a).join("Season 01")).expect("create source a season");
    std::fs::create_dir_all(temp.path().join(source_b).join("Season 02")).expect("create source b season");
    std::fs::write(
        temp.path()
            .join(source_a)
            .join("Season 01")
            .join("My Show - S01E01 - Pilot [tmdbid-1].mkv"),
        "a",
    )
    .expect("file a");
    std::fs::write(
        temp.path()
            .join(source_b)
            .join("Season 02")
            .join("My Show - S02E01 - Return [tmdbid-1].mkv"),
        "b",
    )
    .expect("file b");

    let scan_id = create_scan(&app, &temp.path().display().to_string()).await;
    let payload = format!(
        r#"{{"scan_id":"{}","idempotency_key":"tv-merge","media_type":"tv","base_path":"{}","name_overrides":[],"conflict_resolutions":[{{"normalized_name":"My Show - Home (2020)","selected_source_directory":"{}"}}],"dry_run":false}}"#,
        scan_id,
        temp.path().display(),
        source_b
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

    for _ in 0..200 {
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
        let status = status_body["data"]["status"].as_str().unwrap_or_default();
        if status == "completed" {
            break;
        }
        if status == "failed" {
            panic!("normalize job failed");
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }

    let expected_show = temp.path().join(canonical);
    assert!(expected_show.join("Season 1").is_dir());
    assert!(expected_show.join("Season 2").is_dir());
    assert!(expected_show.join("Season 1").join("01x01 - Pilot.mkv").is_file());
    assert!(expected_show.join("Season 2").join("02x01 - Return.mkv").is_file());
}

#[tokio::test]
async fn post_execute_merges_duplicate_tv_folders_with_overlapping_seasons() {
    let app = test_app();
    let temp = tempfile::tempdir().expect("tempdir");
    let source_a = "Merge Show∶ Home (2021) - [imdb-tt2222222]";
    let source_b = "Merge Show - Home (2021) - [imdb-tt2222222]";
    let canonical = "Merge Show - Home (2021)";

    std::fs::create_dir_all(temp.path().join(source_a).join("Season 01")).expect("create source a season");
    std::fs::create_dir_all(temp.path().join(source_b).join("Season 1")).expect("create source b season");
    std::fs::write(
        temp.path()
            .join(source_a)
            .join("Season 01")
            .join("Merge Show - S01E01 - Pilot [tmdbid-1].mkv"),
        "a",
    )
    .expect("file a");
    std::fs::write(
        temp.path()
            .join(source_b)
            .join("Season 1")
            .join("Merge Show - S01E02 - Return [tmdbid-1].mkv"),
        "b",
    )
    .expect("file b");

    let scan_id = create_scan(&app, &temp.path().display().to_string()).await;
    let payload = format!(
        r#"{{"scan_id":"{}","idempotency_key":"tv-merge-overlap-season","media_type":"tv","base_path":"{}","name_overrides":[],"conflict_resolutions":[{{"normalized_name":"Merge Show - Home (2021)","selected_source_directory":"{}"}}],"dry_run":false}}"#,
        scan_id,
        temp.path().display(),
        source_b
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

    for _ in 0..250 {
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
        let status = status_body["data"]["status"].as_str().unwrap_or_default();
        if status == "completed" {
            break;
        }
        if status == "failed" {
            panic!("normalize job failed");
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }

    let expected_show = temp.path().join(canonical);
    assert!(expected_show.join("Season 1").is_dir());
    assert!(expected_show.join("Season 1").join("01x01 - Pilot.mkv").is_file());
    assert!(expected_show.join("Season 1").join("01x02 - Return.mkv").is_file());
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
