use sqlx::{
    migrate::{MigrateError, Migrator},
    postgres::PgPoolOptions,
    PgPool,
};

static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

pub async fn connect_pg_pool(database_url: &str) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new()
        .max_connections(5)
        .connect(database_url)
        .await
}

pub async fn run_migrations(pool: &PgPool) -> Result<(), MigrateError> {
    MIGRATOR.run(pool).await
}

pub async fn assert_scan_schema_ready(pool: &PgPool) -> Result<(), sqlx::Error> {
    sqlx::query(
        "SELECT id, media_type, base_path, expires_at, summary_json, result_json \
         FROM scan_sessions \
         LIMIT 1",
    )
    .execute(pool)
    .await
    .map(|_| ())?;

    sqlx::query(
        "SELECT id, media_type, base_path, scan_id, idempotency_key, \
                request_fingerprint, plan_hash, status, error \
         FROM audit_sessions \
         LIMIT 1",
    )
    .execute(pool)
    .await
    .map(|_| ())?;

    sqlx::query(
        "SELECT id, session_id, op_index, op_type, source_path, target_path, \
                file_size, status, error_message \
         FROM audit_entries \
         LIMIT 1",
    )
    .execute(pool)
    .await
    .map(|_| ())
}
