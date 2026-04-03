use std::time::{Duration, SystemTime};

use supafast_jellyfin::{
    models::scan::{ScanResult, ScanSummary},
    repositories::scan_repo::{InMemoryScanRepository, ScanRepository, ScanSessionRecord},
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
async fn in_memory_repo_round_trip_save_get_delete() {
    let repo = InMemoryScanRepository::new();
    let scan_id = Uuid::new_v4();
    let record = ScanSessionRecord {
        result: sample_result(scan_id),
        base_path: "/tmp".to_string(),
        expires_at: SystemTime::now() + Duration::from_secs(60),
    };

    repo.save_scan(record.clone()).await.expect("save scan");
    let loaded = repo
        .get_scan(scan_id)
        .await
        .expect("get scan")
        .expect("scan exists");
    assert_eq!(loaded.result.scan_id, scan_id);

    let deleted = repo.delete_scan(scan_id).await.expect("delete scan");
    assert!(deleted);
    assert!(repo
        .get_scan(scan_id)
        .await
        .expect("get deleted scan")
        .is_none());
}

#[tokio::test]
async fn in_memory_repo_prunes_expired_records() {
    let repo = InMemoryScanRepository::new();
    let expired_id = Uuid::new_v4();
    let fresh_id = Uuid::new_v4();

    repo.save_scan(ScanSessionRecord {
        result: sample_result(expired_id),
        base_path: "/tmp".to_string(),
        expires_at: SystemTime::now() - Duration::from_secs(1),
    })
    .await
    .expect("save expired");

    repo.save_scan(ScanSessionRecord {
        result: sample_result(fresh_id),
        base_path: "/tmp".to_string(),
        expires_at: SystemTime::now() + Duration::from_secs(120),
    })
    .await
    .expect("save fresh");

    repo.prune_expired(SystemTime::now())
        .await
        .expect("prune scans");

    assert!(repo
        .get_scan(expired_id)
        .await
        .expect("expired query")
        .is_none());
    assert!(repo
        .get_scan(fresh_id)
        .await
        .expect("fresh query")
        .is_some());
}
