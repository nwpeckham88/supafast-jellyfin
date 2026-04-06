use std::{
    env,
    time::{Duration, SystemTime},
};

use supafast_jellyfin::{
    db::connect_pg_pool,
    models::scan::{ScanResult, ScanSummary},
    repositories::scan_repo::{PgScanRepository, ScanRepository, ScanSessionRecord},
};
use uuid::Uuid;

fn sample_result(scan_id: Uuid) -> ScanResult {
    ScanResult {
        scan_id,
        media_type: "tv".to_string(),
        base_path: "/tmp".to_string(),
        groups: Vec::new(),
        summary: ScanSummary {
            total_shows: 0,
            total_episodes: 0,
            merges: 0,
            conflicts: 0,
        },
    }
}

#[tokio::test]
#[ignore = "requires DATABASE_URL and applied migrations"]
async fn postgres_repo_round_trip_when_database_url_is_set() {
    let database_url = env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set when running postgres repository tests");

    let pool = connect_pg_pool(&database_url)
        .await
        .expect("connect postgres pool");
    let repo = PgScanRepository::new(pool);

    let scan_id = Uuid::new_v4();
    let record = ScanSessionRecord {
        result: sample_result(scan_id),
        base_path: "/tmp".to_string(),
        expires_at: SystemTime::now() + Duration::from_secs(120),
    };

    repo.save_scan(record.clone()).await.expect("save scan");

    let loaded = repo
        .get_scan(scan_id)
        .await
        .expect("get scan")
        .expect("scan should exist");
    assert_eq!(loaded.result.scan_id, scan_id);

    let deleted = repo.delete_scan(scan_id).await.expect("delete scan");
    assert!(deleted);
}

#[tokio::test]
#[ignore = "requires DATABASE_URL and applied migrations"]
async fn postgres_repo_keeps_referenced_expired_scans() {
    let database_url = env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set when running postgres repository tests");

    let pool = connect_pg_pool(&database_url)
        .await
        .expect("connect postgres pool");
    let repo = PgScanRepository::new(pool.clone());

    let scan_id = Uuid::new_v4();
    let session_id = Uuid::new_v4();
    let record = ScanSessionRecord {
        result: sample_result(scan_id),
        base_path: "/tmp".to_string(),
        expires_at: SystemTime::now() - Duration::from_secs(60),
    };

    repo.save_scan(record).await.expect("save expired scan");

    sqlx::query(
        "INSERT INTO audit_sessions \
         (id, media_type, base_path, scan_id, idempotency_key, status, dir_count, file_count, merge_count) \
         VALUES ($1, 'movie', '/tmp', $2, $3, 'completed', 0, 0, 0)",
    )
    .bind(session_id)
    .bind(scan_id)
    .bind(format!("scan-repo-postgres-{session_id}"))
    .execute(&pool)
    .await
    .expect("insert audit session");

    repo.prune_expired(SystemTime::now())
        .await
        .expect("prune referenced expired scans");

    let loaded = repo
        .get_scan(scan_id)
        .await
        .expect("get referenced scan after prune");
    assert!(loaded.is_some(), "referenced expired scan should not be deleted");

    let deleted = repo
        .delete_scan(scan_id)
        .await
        .expect("delete referenced scan should not error");
    assert!(
        !deleted,
        "referenced scan should not be deleted while audit_sessions still point at it"
    );

    sqlx::query("DELETE FROM audit_sessions WHERE id = $1")
        .bind(session_id)
        .execute(&pool)
        .await
        .expect("cleanup audit session");
    sqlx::query("DELETE FROM scan_sessions WHERE id = $1")
        .bind(scan_id)
        .execute(&pool)
        .await
        .expect("cleanup scan session");
}
