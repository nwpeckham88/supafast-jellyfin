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
